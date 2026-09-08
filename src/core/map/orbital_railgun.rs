//! Public strategic-map playback for synchronized Orbital Railgun strikes.

use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::camera::MainCamera;
use crate::core::constants::EXPLOSION_Z;
use crate::core::map::model::{Map, MapCmp};
use crate::core::map::planet::PlanetId;
use crate::core::map::systems::OrbitalRailgunCmp;
use crate::core::settings::Settings;
use crate::core::simulation::OrbitalStrike;
use crate::core::states::GameState;
use crate::core::turns::{start_turn, StartTurnMsg};
use crate::multiplayer::client::MultiplayerSession;

const CHARGE_SECONDS: f32 = 1.75;
const CONVERGE_SECONDS: f32 = 0.72;
const FOCUS_SECONDS: f32 = 0.32;
const BEAM_SECONDS: f32 = 1.45;
const EXPLOSION_FRAME_SECONDS: f32 = 0.09;
const AFTERGLOW_SECONDS: f32 = 2.2;
const EFFECT_SECONDS: f32 = 5.65;
const GOLD: Color = Color::srgb(1.0, 0.57, 0.16);
const HOT_GOLD: Color = Color::srgb(1.0, 0.84, 0.34);
const WHITE_HOT: Color = Color::srgb(1.0, 0.97, 0.78);

/// Canonical strike outcomes copied from the persisted model into the live projection.
#[derive(Resource, Clone, Default)]
pub struct OrbitalStrikes(pub Vec<OrbitalStrike>);

#[derive(Component)]
/// Active map effect that delays a destroyed world's sprite swap until beam impact.
pub struct OrbitalStrikeEffect {
    timer: Timer,
    pub(crate) target: PlanetId,
    pub(crate) destroyed: bool,
    beam_sound: bool,
    impact_sound: bool,
}

#[derive(Clone, Copy)]
enum BeamLayer {
    Halo,
    Body,
    Core,
}

impl BeamLayer {
    fn width(self) -> f32 {
        match self {
            Self::Halo => 2.35,
            Self::Body => 0.95,
            Self::Core => 0.26,
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Halo => GOLD,
            Self::Body => HOT_GOLD,
            Self::Core => WHITE_HOT,
        }
    }

    fn alpha(self) -> f32 {
        match self {
            Self::Halo => 0.34,
            Self::Body => 0.9,
            Self::Core => 1.0,
        }
    }

    fn depth(self) -> f32 {
        match self {
            Self::Halo => -0.12,
            Self::Body => -0.08,
            Self::Core => -0.04,
        }
    }
}

#[derive(Component)]
enum StrikePart {
    ChargeRing {
        center: Vec2,
        radius: f32,
        phase: f32,
    },
    ChargeCore {
        phase: f32,
    },
    ChargeMote {
        center: Vec2,
        offset: Vec2,
        phase: f32,
    },
    FeederBeam {
        start: Vec2,
        end: Vec2,
        thickness: f32,
        layer: BeamLayer,
    },
    ConvergenceRing {
        radius: f32,
        phase: f32,
    },
    ConvergenceCore,
    MainBeam {
        start: Vec2,
        end: Vec2,
        thickness: f32,
        layer: BeamLayer,
    },
    BeamMote {
        start: Vec2,
        end: Vec2,
        phase: f32,
    },
    ImpactRing {
        radius: f32,
        phase: f32,
    },
    ExplosionAfterglow {
        phase: f32,
    },
    Explosion {
        last_index: usize,
    },
}

#[derive(Default)]
struct StrikeTextures {
    glow: Handle<Image>,
    ring: Handle<Image>,
    beam: Handle<Image>,
    ready: bool,
}

impl StrikeTextures {
    fn initialize(&mut self, images: &mut Assets<Image>) {
        if self.ready {
            return;
        }
        for kind in 0..3 {
            let resolution = 128;
            let center = (resolution as f32 - 1.0) * 0.5;
            let mut pixels = Vec::with_capacity(resolution * resolution * 4);
            for y in 0..resolution {
                for x in 0..resolution {
                    let uv = Vec2::new((x as f32 - center) / center, (y as f32 - center) / center);
                    let radius = uv.length();
                    let alpha = match kind {
                        0 => (1.0 - radius).clamp(0.0, 1.0).powf(2.5),
                        1 => (1.0 - ((radius - 0.78) / 0.045).abs()).clamp(0.0, 1.0),
                        _ => {
                            let tapered_ends = ((1.0 - uv.x.abs()) * 10.0).clamp(0.0, 1.0);
                            (1.0 - uv.y.abs()).clamp(0.0, 1.0).powf(3.0) * tapered_ends
                        },
                    };
                    pixels.extend_from_slice(&[255, 255, 255, (alpha * 255.0) as u8]);
                }
            }
            let mut image = Image::new(
                Extent3d {
                    width: resolution as u32,
                    height: resolution as u32,
                    depth_or_array_layers: 1,
                },
                TextureDimension::D2,
                pixels,
                TextureFormat::Rgba8UnormSrgb,
                RenderAssetUsages::default(),
            );
            image.sampler = bevy::image::ImageSampler::linear();
            match kind {
                0 => self.glow = images.add(image),
                1 => self.ring = images.add(image),
                _ => self.beam = images.add(image),
            }
        }
        self.ready = true;
    }
}

fn smooth(progress: f32) -> f32 {
    progress * progress * (3.0 - 2.0 * progress)
}

fn afterglow_alpha(age: f32, phase: f32) -> f32 {
    let local_age = age - phase;
    let progress = (local_age / AFTERGLOW_SECONDS).clamp(0.0, 1.0);
    if local_age < 0.0 || progress >= 1.0 {
        return 0.0;
    }
    let attack = (local_age / 0.12).clamp(0.0, 1.0);
    attack * (1.0 - smooth(progress)) * (0.72 - phase * 0.55)
}

fn explosion_alpha(age: f32, duration: f32) -> f32 {
    let progress = (age / duration).clamp(0.0, 1.0);
    if age < 0.0 || progress >= 1.0 {
        return 0.0;
    }
    let fade = (1.0 - ((progress - 0.62) / 0.38).clamp(0.0, 1.0)).powi(2);
    (age / 0.08).clamp(0.0, 1.0) * fade
}

fn beam_transform(start: Vec2, end: Vec2, fraction: f32, z: f32) -> Transform {
    let route = end - start;
    let length = route.length() * fraction;
    let direction = route.normalize_or_zero();
    Transform {
        translation: (start + direction * length * 0.5).extend(z),
        rotation: Quat::from_rotation_z(route.y.atan2(route.x)),
        ..default()
    }
}

fn spawn_beam_layer(
    parent: &mut ChildSpawnerCommands,
    image: Handle<Image>,
    start: Vec2,
    end: Vec2,
    thickness: f32,
    layer: BeamLayer,
    main: bool,
) {
    let z = EXPLOSION_Z
        + layer.depth()
        + if main {
            0.08
        } else {
            0.0
        };
    parent.spawn((
        Sprite {
            image,
            color: layer.color().with_alpha(0.0),
            custom_size: Some(Vec2::new(1.0, thickness * layer.width())),
            ..default()
        },
        beam_transform(start, end, 0.0, z),
        if main {
            StrikePart::MainBeam {
                start,
                end,
                thickness,
                layer,
            }
        } else {
            StrikePart::FeederBeam {
                start,
                end,
                thickness,
                layer,
            }
        },
        Pickable::IGNORE,
    ));
}

fn convergence_point(origins: &[Vec2], target: Vec2) -> Vec2 {
    let centroid = origins.iter().copied().sum::<Vec2>() / origins.len().max(1) as f32;
    centroid.lerp(target, 0.58)
}

/// Creates the warm War-Sun-like charge, convergence, discharge, and impact choreography.
fn start_orbital_strikes(
    mut commands: Commands,
    mut turns: MessageReader<StartTurnMsg>,
    strikes: Res<OrbitalStrikes>,
    settings: Res<Settings>,
    map: Res<Map>,
    session: Res<MultiplayerSession>,
    assets: Res<WorldAssets>,
    mut images: ResMut<Assets<Image>>,
    mut textures: Local<StrikeTextures>,
    railguns: Query<(&OrbitalRailgunCmp, &GlobalTransform)>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
) {
    if !turns.read().any(|request| !request.skip_battle) {
        return;
    }
    textures.initialize(&mut images);
    let active_strikes =
        strikes.0.iter().filter(|strike| strike.turn == settings.turn as u64).collect::<Vec<_>>();
    let local_player = session.membership.as_ref().map(|membership| membership.player_id);
    if let Some(target) = active_strikes
        .iter()
        .copied()
        .find(|strike| {
            local_player.is_some_and(|player_id| {
                strike.origins.iter().any(|origin| {
                    map.try_get(*origin).and_then(|planet| planet.owned) == Some(player_id)
                })
            })
        })
        .and_then(|strike| map.try_get(strike.target))
    {
        if let Ok(mut transform) = camera.single_mut() {
            transform.translation.x = target.position.x;
            transform.translation.y = target.position.y;
        }
    }

    for strike in active_strikes {
        let origins = strike
            .origins
            .iter()
            .filter_map(|origin| {
                railguns
                    .iter()
                    .find(|(railgun, _)| railgun.planet == *origin)
                    .map(|(_, transform)| transform.translation().truncate())
                    .or_else(|| map.try_get(*origin).map(|planet| planet.position))
            })
            .collect::<Vec<_>>();
        let Some(target_planet) = map.try_get(strike.target) else {
            continue;
        };
        if origins.is_empty() {
            continue;
        }
        let target = target_planet.position;
        let target_size = target_planet.size();
        let convergence = convergence_point(&origins, target);
        let feeder_thickness = (target_size * 0.065).clamp(5.0, 9.0);
        let main_thickness = (target_size * 0.24).clamp(18.0, 32.0);

        commands
            .spawn((
                Transform::default(),
                Visibility::Inherited,
                MapCmp,
                OrbitalStrikeEffect {
                    timer: Timer::from_seconds(EFFECT_SECONDS, TimerMode::Once),
                    target: strike.target,
                    destroyed: strike.destroyed,
                    beam_sound: false,
                    impact_sound: false,
                },
            ))
            .with_children(|parent| {
                for (origin_index, origin) in origins.iter().copied().enumerate() {
                    for ring in 0..3 {
                        parent.spawn((
                            Sprite {
                                image: textures.ring.clone(),
                                color: GOLD.with_alpha(0.0),
                                custom_size: Some(Vec2::splat(78.0 + ring as f32 * 22.0)),
                                ..default()
                            },
                            Transform::from_translation(origin.extend(EXPLOSION_Z - 0.1)),
                            StrikePart::ChargeRing {
                                center: origin,
                                radius: 78.0 + ring as f32 * 22.0,
                                phase: ring as f32 * 0.13,
                            },
                            Pickable::IGNORE,
                        ));
                    }
                    parent.spawn((
                        Sprite {
                            image: textures.glow.clone(),
                            color: WHITE_HOT.with_alpha(0.0),
                            custom_size: Some(Vec2::splat(44.0)),
                            ..default()
                        },
                        Transform::from_translation(origin.extend(EXPLOSION_Z - 0.02)),
                        StrikePart::ChargeCore {
                            phase: origin_index as f32 / origins.len() as f32,
                        },
                        Pickable::IGNORE,
                    ));
                    for mote in 0..12 {
                        let angle = mote as f32 * TAU / 12.0
                            + origin_index as f32 * 0.71
                            + (mote % 3) as f32 * 0.11;
                        let radius = 52.0 + (mote % 4) as f32 * 14.0;
                        parent.spawn((
                            Sprite {
                                image: textures.glow.clone(),
                                color: HOT_GOLD.with_alpha(0.0),
                                custom_size: Some(Vec2::splat(8.0 + (mote % 3) as f32 * 2.0)),
                                ..default()
                            },
                            Transform::from_translation(
                                (origin + Vec2::X * radius).extend(EXPLOSION_Z - 0.04),
                            ),
                            StrikePart::ChargeMote {
                                center: origin,
                                offset: Vec2::new(angle.cos(), angle.sin()) * radius,
                                phase: mote as f32 / 12.0,
                            },
                            Pickable::IGNORE,
                        ));
                    }
                    for layer in [BeamLayer::Halo, BeamLayer::Body, BeamLayer::Core] {
                        spawn_beam_layer(
                            parent,
                            textures.beam.clone(),
                            origin,
                            convergence,
                            feeder_thickness,
                            layer,
                            false,
                        );
                    }
                }

                for ring in 0..3 {
                    parent.spawn((
                        Sprite {
                            image: textures.ring.clone(),
                            color: HOT_GOLD.with_alpha(0.0),
                            custom_size: Some(Vec2::splat(64.0 + ring as f32 * 28.0)),
                            ..default()
                        },
                        Transform::from_translation(convergence.extend(EXPLOSION_Z - 0.02)),
                        StrikePart::ConvergenceRing {
                            radius: 64.0 + ring as f32 * 28.0,
                            phase: ring as f32 * 0.12,
                        },
                        Pickable::IGNORE,
                    ));
                }
                parent.spawn((
                    Sprite {
                        image: textures.glow.clone(),
                        color: WHITE_HOT.with_alpha(0.0),
                        custom_size: Some(Vec2::splat(72.0)),
                        ..default()
                    },
                    Transform::from_translation(convergence.extend(EXPLOSION_Z + 0.02)),
                    StrikePart::ConvergenceCore,
                    Pickable::IGNORE,
                ));

                for layer in [BeamLayer::Halo, BeamLayer::Body, BeamLayer::Core] {
                    spawn_beam_layer(
                        parent,
                        textures.beam.clone(),
                        convergence,
                        target,
                        main_thickness,
                        layer,
                        true,
                    );
                }
                for mote in 0..20 {
                    parent.spawn((
                        Sprite {
                            image: textures.glow.clone(),
                            color: WHITE_HOT.with_alpha(0.0),
                            custom_size: Some(Vec2::splat(5.0 + (mote % 4) as f32 * 1.5)),
                            ..default()
                        },
                        Transform::from_translation(convergence.extend(EXPLOSION_Z + 0.06)),
                        StrikePart::BeamMote {
                            start: convergence,
                            end: target,
                            phase: mote as f32 / 20.0,
                        },
                        Pickable::IGNORE,
                    ));
                }
                for ring in 0..4 {
                    parent.spawn((
                        Sprite {
                            image: textures.ring.clone(),
                            color: if ring == 0 {
                                WHITE_HOT
                            } else {
                                GOLD
                            }
                            .with_alpha(0.0),
                            custom_size: Some(Vec2::splat(target_size * (1.2 + ring as f32 * 0.3))),
                            ..default()
                        },
                        Transform::from_translation(target.extend(EXPLOSION_Z + 0.04)),
                        StrikePart::ImpactRing {
                            radius: target_size * (1.2 + ring as f32 * 0.3),
                            phase: ring as f32 * 0.11,
                        },
                        Pickable::IGNORE,
                    ));
                }
                if strike.destroyed {
                    for afterglow in 0..3 {
                        parent.spawn((
                            Sprite {
                                image: textures.glow.clone(),
                                color: match afterglow {
                                    0 => WHITE_HOT,
                                    1 => HOT_GOLD,
                                    _ => GOLD,
                                }
                                .with_alpha(0.0),
                                custom_size: Some(Vec2::splat(
                                    target_size * (1.8 + afterglow as f32 * 0.55),
                                )),
                                ..default()
                            },
                            Transform::from_translation(
                                target.extend(EXPLOSION_Z + 0.06 - afterglow as f32 * 0.015),
                            ),
                            StrikePart::ExplosionAfterglow {
                                phase: afterglow as f32 * 0.16,
                            },
                            Pickable::IGNORE,
                        ));
                    }
                    let explosion = assets.texture("explosion");
                    parent.spawn((
                        Sprite {
                            image: explosion.image,
                            texture_atlas: Some(explosion.atlas),
                            custom_size: Some(Vec2::splat(target_size * 2.15)),
                            color: Color::WHITE.with_alpha(0.0),
                            ..default()
                        },
                        Transform::from_translation(target.extend(EXPLOSION_Z + 0.1)),
                        StrikePart::Explosion {
                            last_index: explosion.last_index,
                        },
                        Pickable::IGNORE,
                    ));
                }
            });
    }
}

fn animate_orbital_strikes(
    mut commands: Commands,
    time: Res<Time>,
    game_state: Res<State<GameState>>,
    mut map: ResMut<Map>,
    mut effects: Query<(Entity, &mut OrbitalStrikeEffect, &Children, &mut Visibility)>,
    mut parts: Query<(&StrikePart, &mut Transform, &mut Sprite)>,
    mut audio: MessageWriter<PlayAudioMsg>,
) {
    let converge_start = CHARGE_SECONDS;
    let focus_start = converge_start + CONVERGE_SECONDS;
    let beam_start = focus_start + FOCUS_SECONDS;
    let impact_start = beam_start + 0.12;
    let explosion_start = impact_start + 0.18;

    for (entity, mut effect, children, mut visibility) in &mut effects {
        let playing = *game_state.get() == GameState::Playing;
        *visibility = if playing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if !playing {
            continue;
        }
        effect.timer.tick(time.delta());
        if effect.timer.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }
        let elapsed = effect.timer.elapsed_secs();
        if elapsed >= beam_start && !effect.beam_sound {
            effect.beam_sound = true;
            audio.write(PlayAudioMsg::new("beam fire").rate(0.76).gain(-5.0));
        }
        if elapsed >= explosion_start && effect.destroyed && !effect.impact_sound {
            effect.impact_sound = true;
            audio.write(PlayAudioMsg::new("large explosion"));
        }

        for child in children.iter() {
            let Ok((part, mut transform, mut sprite)) = parts.get_mut(child) else {
                continue;
            };
            match part {
                StrikePart::ChargeRing {
                    center,
                    radius,
                    phase,
                } => {
                    let local =
                        ((elapsed - phase * 0.42) / (CHARGE_SECONDS - 0.25)).clamp(0.0, 1.0);
                    let collapse = smooth(local);
                    let size = radius * (1.0 - collapse * 0.82);
                    transform.translation = center.extend(EXPLOSION_Z - 0.1);
                    transform.scale = Vec3::splat(size / radius);
                    transform.rotate_z(time.delta_secs() * (0.8 + phase));
                    sprite.color.set_alpha(if elapsed < beam_start {
                        (PI * local).sin().max(0.0) * 0.72
                    } else {
                        0.0
                    });
                },
                StrikePart::ChargeCore {
                    phase,
                } => {
                    let progress = (elapsed / CHARGE_SECONDS).clamp(0.0, 1.0);
                    let fade = (1.0 - ((elapsed - beam_start) / 0.18).clamp(0.0, 1.0)).max(0.0);
                    let pulse = 0.92 + 0.08 * (elapsed * 17.0 + phase * TAU).sin();
                    transform.scale = Vec3::splat((0.18 + smooth(progress) * 1.18) * pulse);
                    sprite.color.set_alpha(smooth(progress) * fade);
                },
                StrikePart::ChargeMote {
                    center,
                    offset,
                    phase,
                } => {
                    let cycle = (elapsed / CHARGE_SECONDS + phase).fract();
                    let inward = smooth(cycle);
                    transform.translation =
                        (*center + *offset * (1.0 - inward)).extend(EXPLOSION_Z - 0.04);
                    transform.scale = Vec3::splat(0.55 + inward * 0.65);
                    sprite.color.set_alpha(if elapsed < converge_start {
                        (PI * cycle).sin().max(0.0) * (0.35 + 0.65 * inward)
                    } else {
                        0.0
                    });
                },
                StrikePart::FeederBeam {
                    start,
                    end,
                    thickness,
                    layer,
                } => {
                    let progress =
                        smooth(((elapsed - converge_start) / CONVERGE_SECONDS).clamp(0.0, 1.0));
                    let fade = 1.0 - ((elapsed - beam_start) / 0.2).clamp(0.0, 1.0);
                    *transform =
                        beam_transform(*start, *end, progress, EXPLOSION_Z + layer.depth());
                    sprite.custom_size =
                        Some(Vec2::new(start.distance(*end) * progress, thickness * layer.width()));
                    sprite.color.set_alpha(progress * fade * layer.alpha());
                },
                StrikePart::ConvergenceRing {
                    radius,
                    phase,
                } => {
                    let gather = ((elapsed - converge_start - phase * 0.2) / CONVERGE_SECONDS)
                        .clamp(0.0, 1.0);
                    let fade = 1.0 - ((elapsed - (beam_start + 0.22)) / 0.3).clamp(0.0, 1.0);
                    transform.scale = Vec3::splat(1.0 - smooth(gather) * 0.76);
                    transform.rotate_z(time.delta_secs() * (1.3 + phase));
                    sprite.color.set_alpha((PI * gather).sin().max(0.0) * fade * 0.8);
                    sprite.custom_size = Some(Vec2::splat(*radius));
                },
                StrikePart::ConvergenceCore => {
                    let gather = ((elapsed - converge_start) / (CONVERGE_SECONDS + FOCUS_SECONDS))
                        .clamp(0.0, 1.0);
                    let fade =
                        1.0 - ((elapsed - (beam_start + BEAM_SECONDS)) / 0.22).clamp(0.0, 1.0);
                    let pulse = 0.9 + 0.1 * (elapsed * 21.0).sin();
                    transform.scale = Vec3::splat((0.12 + smooth(gather) * 1.35) * pulse);
                    sprite.color.set_alpha(smooth(gather) * fade);
                },
                StrikePart::MainBeam {
                    start,
                    end,
                    thickness,
                    layer,
                } => {
                    let age = elapsed - beam_start;
                    let attack = (age / 0.055).clamp(0.0, 1.0);
                    let fade = 1.0 - ((age - (BEAM_SECONDS - 0.22)) / 0.22).clamp(0.0, 1.0);
                    let active = attack * fade;
                    let throb = 1.0 + 0.06 * (elapsed * 25.0 + layer.width()).sin();
                    *transform = beam_transform(
                        *start,
                        *end,
                        if age >= 0.0 {
                            1.0
                        } else {
                            0.0
                        },
                        EXPLOSION_Z + 0.08 + layer.depth(),
                    );
                    sprite.custom_size =
                        Some(Vec2::new(start.distance(*end), thickness * layer.width() * throb));
                    sprite.color.set_alpha(if (0.0..=BEAM_SECONDS).contains(&age) {
                        active * layer.alpha()
                    } else {
                        0.0
                    });
                },
                StrikePart::BeamMote {
                    start,
                    end,
                    phase,
                } => {
                    let age = elapsed - beam_start;
                    let progress = (age * 1.85 / BEAM_SECONDS + phase).fract();
                    transform.translation = start.lerp(*end, progress).extend(EXPLOSION_Z + 0.1);
                    transform.scale = Vec3::splat(0.7 + (PI * progress).sin() * 0.7);
                    sprite.color.set_alpha(if (0.0..=BEAM_SECONDS).contains(&age) {
                        (PI * progress).sin().max(0.0) * 0.9
                    } else {
                        0.0
                    });
                },
                StrikePart::ImpactRing {
                    radius,
                    phase,
                } => {
                    let progress = ((elapsed - impact_start - phase) / 0.85).clamp(0.0, 1.0);
                    transform.scale = Vec3::splat(0.18 + smooth(progress) * 1.55);
                    transform.rotate_z(time.delta_secs() * (0.7 + phase));
                    sprite.custom_size = Some(Vec2::splat(*radius));
                    sprite.color.set_alpha(if elapsed >= impact_start + phase {
                        (1.0 - progress).powi(2) * 0.92
                    } else {
                        0.0
                    });
                },
                StrikePart::ExplosionAfterglow {
                    phase,
                } => {
                    let age = elapsed - explosion_start - phase;
                    let progress = (age / AFTERGLOW_SECONDS).clamp(0.0, 1.0);
                    transform.scale = Vec3::splat(0.62 + smooth(progress) * 0.86);
                    sprite.color.set_alpha(afterglow_alpha(elapsed - explosion_start, *phase));
                },
                StrikePart::Explosion {
                    last_index,
                } => {
                    let age = elapsed - explosion_start;
                    let duration = EXPLOSION_FRAME_SECONDS * (*last_index + 1) as f32;
                    let progress = (age / duration).clamp(0.0, 1.0);
                    sprite.color.set_alpha(explosion_alpha(age, duration));
                    if let Some(atlas) = &mut sprite.texture_atlas {
                        atlas.index =
                            ((progress * (*last_index + 1) as f32) as usize).min(*last_index);
                        if effect.destroyed && atlas.index >= *last_index / 3 {
                            if let Some(planet) = map.try_get_mut(effect.target) {
                                planet.image = 0;
                            }
                        }
                    }
                },
            }
        }
    }
}

pub(crate) struct OrbitalRailgunAnimationPlugin;

impl Plugin for OrbitalRailgunAnimationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<OrbitalStrikes>()
            .add_systems(
                First,
                start_orbital_strikes.after(start_turn).run_if(resource_exists::<Map>),
            )
            .add_systems(Update, animate_orbital_strikes.run_if(resource_exists::<Map>));
    }
}

#[cfg(test)]
#[path = "../../../tests/core/orbital_railgun.rs"]
mod tests;
