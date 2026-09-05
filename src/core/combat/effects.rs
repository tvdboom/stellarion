//! Bounded, presentation-only combat choreography. Reports supply all damage; visual
//! sampling, particle timing and curved trajectories never feed back into simulation.

use std::collections::BTreeMap;
use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

use super::report::Side;
use super::systems::{
    BackgroundImageCmp, CombatCmp, CombatUnitCmp, PSCombatImageCmp, SpawnShotMsg,
};
use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::camera::MainCamera;
use crate::core::constants::{COMBAT_BACKGROUND_Z, COMBAT_EXPLOSION_Z};
use crate::core::settings::Settings;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::Unit;

const ICE: Color = Color::srgb(0.32, 0.85, 1.0);
const GOLD: Color = Color::srgb(1.0, 0.57, 0.16);
const MINT: Color = Color::srgb(0.35, 1.0, 0.68);
const VIOLET: Color = Color::srgb(0.75, 0.38, 1.0);
const MAX_PARTICLES: usize = 1800;
pub(crate) const DEATH_RAY_DURATION: f32 = 6.0;

#[derive(Clone, Copy, Debug, PartialEq)]
enum Weapon {
    Laser,
    HeavyLaser,
    TwinLaser,
    Repeater,
    Railgun,
    Missile,
    Bomb,
    Broadside,
    Plasma,
    Ion,
    Lance,
    Solar,
    Siege,
    Repair,
}

impl Weapon {
    fn for_unit(unit: Unit) -> Self {
        match unit {
            Unit::Ship(ship) => match ship {
                Ship::Probe | Ship::ColonyShip | Ship::LightFighter => Self::Laser,
                Ship::HeavyFighter => Self::TwinLaser,
                Ship::Destroyer => Self::Repeater,
                Ship::Cruiser => Self::Railgun,
                Ship::Bomber => Self::Missile,
                Ship::Battleship => Self::Broadside,
                Ship::Dreadnought => Self::Lance,
                Ship::WarSun => Self::Solar,
            },
            Unit::Defense(defense) => match defense {
                Defense::Crawler => Self::Repair,
                Defense::LightLaser => Self::Laser,
                Defense::HeavyLaser => Self::HeavyLaser,
                Defense::GaussCannon => Self::Railgun,
                Defense::IonCannon => Self::Ion,
                Defense::PlasmaTurret => Self::Plasma,
                Defense::SpaceDock => Self::Siege,
                Defense::RocketLauncher
                | Defense::AntiballisticMissile
                | Defense::InterplanetaryMissile => Self::Missile,
            },
            Unit::Building(_) => Self::Laser,
        }
    }

    fn color(self) -> Color {
        match self {
            Self::Laser | Self::HeavyLaser | Self::TwinLaser => Color::srgb(1.0, 0.22, 0.3),
            Self::Repeater => ICE,
            Self::Railgun => Color::srgb(0.7, 0.83, 1.0),
            Self::Missile | Self::Bomb | Self::Broadside => GOLD,
            Self::Plasma => Color::srgb(0.18, 1.0, 0.28),
            Self::Ion => Color::srgb(0.14, 0.45, 1.0),
            Self::Lance => VIOLET,
            Self::Solar => Color::srgb(1.0, 0.82, 0.3),
            Self::Siege => Color::srgb(1.0, 0.2, 0.65),
            Self::Repair => MINT,
        }
    }

    fn flight(self) -> f32 {
        match self {
            Self::Laser | Self::HeavyLaser | Self::Repeater => 0.28,
            Self::TwinLaser | Self::Railgun => 0.32,
            Self::Missile => 0.64,
            Self::Bomb => 0.9,
            Self::Broadside => 0.46,
            Self::Plasma | Self::Ion => 0.42,
            Self::Lance => 0.5,
            Self::Solar | Self::Siege => 0.62,
            Self::Repair => 1.3,
        }
    }

    fn charge(self) -> f32 {
        match self {
            Self::Plasma | Self::Ion => 0.16,
            Self::Lance => 0.3,
            Self::Solar => 0.58,
            Self::Siege => 0.45,
            _ => 0.,
        }
    }

    fn beam_width(self) -> Option<f32> {
        match self {
            Self::Plasma => Some(0.11),
            Self::Ion => Some(0.085),
            Self::Lance => Some(0.17),
            Self::Solar => Some(0.42),
            Self::Siege => Some(0.14),
            _ => None,
        }
    }

    fn barrels(self) -> usize {
        match self {
            Self::TwinLaser | Self::Broadside | Self::Siege => 2,
            Self::Repeater => 3,
            _ => 1,
        }
    }

    fn salvo_limit(self) -> usize {
        match self {
            Self::Solar | Self::Siege | Self::Bomb | Self::Repair => 1,
            Self::Plasma | Self::Ion | Self::Lance => 2,
            _ => 3,
        }
    }

    fn projectile_size(self, size: f32) -> Vec2 {
        size * match self {
            Self::Laser => Vec2::new(0.55, 0.06),
            Self::HeavyLaser => Vec2::new(0.55, 0.09),
            Self::TwinLaser => Vec2::new(0.5, 0.045),
            Self::Repeater => Vec2::new(0.4, 0.17),
            Self::Railgun => Vec2::new(1.25, 0.055),
            Self::Missile => Vec2::new(0.23, 0.09),
            Self::Bomb => Vec2::new(0.3, 0.16),
            Self::Broadside => Vec2::new(0.65, 0.10),
            Self::Repair => Vec2::splat(0.15),
            Self::Plasma | Self::Ion | Self::Lance | Self::Solar | Self::Siege => Vec2::ONE,
        }
    }
}

/// One of at most three representative salvos per source/target/outcome. Aggregated
/// counters retain *every* recorded hit, including building levels and repairs.
#[derive(Component)]
pub struct PendingImpact {
    target: Entity,
    source: Option<Entity>,
    origin: Vec3,
    destination: Vec3,
    size: f32,
    weapon: Weapon,
    missed: bool,
    hull: usize,
    shield: usize,
    planetary: usize,
    levels: usize,
    elapsed: f32,
    delay: f32,
    lane: f32,
    launched: bool,
    trail_clock: f32,
}

impl PendingImpact {
    fn position(&self, progress: f32) -> Vec3 {
        let mut p = progress.clamp(0.0, 1.0);
        if self.weapon == Weapon::Bomb {
            p *= p;
        }
        let delta = self.destination - self.origin;
        let normal = Vec3::new(-delta.y, delta.x, 0.).normalize_or_zero();
        if self.weapon == Weapon::Repair {
            // Fly out, orbit the repair site, then return to the crawler.
            if p < 0.25 {
                return self.origin.lerp(self.destination, smooth(p * 4.));
            }
            if p > 0.8 {
                return self.destination.lerp(self.origin, smooth((p - 0.8) * 5.));
            }
            let orbit = (p - 0.25) / 0.55;
            let radius = (orbit * std::f32::consts::PI).sin() * self.size * 0.38;
            return self.destination
                + Vec3::new((orbit * TAU * 2.).cos(), (orbit * TAU * 2.).sin(), 0.) * radius;
        }
        let curve = if self.weapon == Weapon::Bomb {
            self.size * (self.lane - 0.5) * 1.4
        } else if self.weapon == Weapon::Missile {
            self.size * self.lane * 0.9
        } else {
            0.
        };
        self.origin.lerp(self.destination, p) + normal * (std::f32::consts::PI * p).sin() * curve
    }
}

/// A defeated card remains until its secondary blasts, flash and debris launch finish.
#[derive(Component)]
pub struct Wreck {
    origin: Vec3,
    size: f32,
    elapsed: f32,
    stage: usize,
    heavy: bool,
}

impl Wreck {
    pub(crate) fn new(origin: Vec3, size: f32, unit: Unit) -> Self {
        Self {
            origin,
            size,
            elapsed: 0.,
            stage: 0,
            heavy: matches!(unit, Unit::Ship(Ship::Battleship | Ship::Dreadnought | Ship::WarSun)),
        }
    }
}

/// Death-ray charge, sustained beam and planetary shockwave, in playback seconds.
#[derive(Component)]
pub struct Cinematic {
    origin: Vec3,
    target: Vec3,
    viewport: Vec2,
    size: f32,
    elapsed: f32,
    stage: usize,
    destroys_planet: bool,
    boom_stage: usize,
}

impl Cinematic {
    pub(crate) fn new(
        origin: Vec3,
        target: Vec3,
        viewport: Vec2,
        size: f32,
        destroys_planet: bool,
    ) -> Self {
        Self {
            origin: origin.truncate().extend(COMBAT_EXPLOSION_Z),
            target: target.truncate().extend(COMBAT_EXPLOSION_Z),
            viewport,
            size,
            elapsed: 0.,
            stage: 0,
            destroys_planet,
            boom_stage: 0,
        }
    }
}

#[derive(Component, Default)]
/// Additive card motion, preserving its original color and tween-controlled scale.
pub struct UnitMotion {
    offset: Vec3,
    impulse: Vec3,
    flash: f32,
    miss_flash: f32,
    miss_cooldown: f32,
    sparks: f32,
    base_color: Color,
}

#[derive(Component)]
/// A transient sprite with analytical motion and a bounded lifetime.
pub struct Particle {
    origin: Vec3,
    velocity: Vec3,
    start_size: Vec2,
    end_size: Vec2,
    color: Color,
    elapsed: f32,
    delay: f32,
    lifetime: f32,
    spin: f32,
}

#[derive(Component)]
/// Atlas frames sampled from effect age, including frames crossed by fast-forward.
pub struct BlastFrames(usize);

#[derive(Component)]
/// Opaque cover while the planetary backdrop switches under its explosion cloud.
pub struct PlanetFlash;

#[derive(Component)]
/// A brief miss or repair label, timed with the same playback clock.
pub struct CombatReadout {
    age: f32,
    origin: Vec3,
    size: f32,
    color: Color,
}

/// Shared procedural masks; no downloads or per-frame images.
#[derive(Default)]
pub struct EffectTextures {
    glow: Handle<Image>,
    ring: Handle<Image>,
    shard: Handle<Image>,
    beam: Handle<Image>,
    ready: bool,
}

impl EffectTextures {
    fn initialize(&mut self, images: &mut Assets<Image>) {
        if self.ready {
            return;
        }
        for kind in 0..4 {
            let resolution = if kind == 2 {
                32
            } else {
                256
            };
            let center = (resolution as f32 - 1.) * 0.5;
            let mut pixels = Vec::with_capacity(resolution * resolution * 4);
            for y in 0..resolution {
                for x in 0..resolution {
                    let uv = Vec2::new((x as f32 - center) / center, (y as f32 - center) / center);
                    let r = uv.length();
                    let alpha = match kind {
                        1 => (1. - ((r - 0.79) / 0.03).abs()).clamp(0., 1.),
                        2 => ((0.8 - uv.x.abs()).min(uv.y + 0.4).min(0.35 - uv.x * 0.3 - uv.y)
                            * 20.)
                            .clamp(0., 1.),
                        3 => {
                            (1. - uv.y.abs()).clamp(0., 1.).powf(2.)
                                * ((1. - uv.x.abs()) * 14.).clamp(0., 1.)
                        },
                        _ => (1. - r).clamp(0., 1.).powf(2.5),
                    };
                    // Sub-byte dithering avoids visible alpha bands in screen-sized glows.
                    let dither = ((x * 73 + y * 151 + x * y * 17) % 101) as f32 / 101.;
                    pixels.extend_from_slice(&[255, 255, 255, (255. * alpha + dither) as u8]);
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
                1 => self.ring = images.add(image),
                2 => self.shard = images.add(image),
                3 => self.beam = images.add(image),
                _ => self.glow = images.add(image),
            }
        }
        self.ready = true;
    }
}

struct Painter<'a, 'w, 's> {
    commands: &'a mut Commands<'w, 's>,
    textures: &'a EffectTextures,
    budget: usize,
    art: Option<&'a WorldAssets>,
}

impl Painter<'_, '_, '_> {
    fn particle(&mut self, ring: bool, mut p: Particle) {
        if self.budget == 0 {
            return;
        }
        self.budget -= 1;
        p.origin.z = COMBAT_EXPLOSION_Z + 0.2;
        self.commands.spawn((
            Sprite {
                image: if ring {
                    self.textures.ring.clone()
                } else if p.spin != 0. {
                    self.textures.shard.clone()
                } else {
                    self.textures.glow.clone()
                },
                color: p.color.with_alpha(0.),
                custom_size: Some(p.start_size),
                ..default()
            },
            Transform::from_translation(p.origin),
            p,
            CombatCmp,
            Pickable::IGNORE,
        ));
    }

    fn glow(&mut self, origin: Vec3, size: f32, color: Color, lifetime: f32) {
        self.particle(
            false,
            Particle {
                origin,
                velocity: Vec3::ZERO,
                start_size: Vec2::splat(size),
                end_size: Vec2::splat(size * 1.8),
                color,
                elapsed: 0.,
                delay: 0.,
                lifetime,
                spin: 0.,
            },
        );
    }

    fn ring(&mut self, origin: Vec3, size: f32, color: Color, lifetime: f32) {
        self.particle(
            true,
            Particle {
                origin,
                velocity: Vec3::ZERO,
                start_size: Vec2::splat(size * 0.2),
                end_size: Vec2::splat(size),
                color,
                elapsed: 0.,
                delay: 0.,
                lifetime,
                spin: 0.,
            },
        );
    }

    fn sparks(&mut self, origin: Vec3, size: f32, color: Color, count: usize, debris: bool) {
        for i in 0..count {
            let angle = i as f32 * 2.399_963;
            let direction = Vec3::new(angle.cos(), angle.sin(), 0.);
            let distance = size * (0.5 + (i % 5) as f32 * 0.17);
            let fragment = (size * 0.045).clamp(2.5, 11.) * (0.6 + (i % 4) as f32 * 0.15);
            self.particle(
                false,
                Particle {
                    origin: origin
                        + if debris {
                            direction * size * 0.08
                        } else {
                            Vec3::ZERO
                        },
                    velocity: direction * distance,
                    start_size: if debris {
                        Vec2::new(fragment, fragment * 0.6)
                    } else {
                        Vec2::new(size * 0.035, size * 0.018)
                    },
                    end_size: Vec2::splat(if debris {
                        fragment * 0.35
                    } else {
                        size * 0.008
                    }),
                    color: if debris && color == GOLD {
                        Color::srgb(0.28, 0.32, 0.38)
                    } else {
                        color
                    },
                    elapsed: 0.,
                    delay: if debris {
                        0.22
                    } else {
                        0.
                    },
                    lifetime: if debris {
                        1.7
                    } else {
                        0.42
                    },
                    spin: if debris {
                        angle - 3.
                    } else {
                        0.
                    },
                },
            );
        }
    }

    fn blast(&mut self, origin: Vec3, size: f32, lifetime: f32) {
        self.blast_after(origin, size, lifetime, 0.);
    }

    fn blast_after(&mut self, origin: Vec3, size: f32, lifetime: f32, delay: f32) {
        if self.budget == 0 {
            return;
        }
        let Some(art) = self.art else {
            return;
        };
        self.budget -= 1;
        let texture = art.texture("explosion");
        let origin = Vec3::new(origin.x, origin.y, COMBAT_EXPLOSION_Z + 0.15);
        self.commands.spawn((
            Sprite {
                image: texture.image,
                texture_atlas: Some(texture.atlas),
                color: Color::WHITE.with_alpha(0.),
                custom_size: Some(Vec2::splat(size)),
                ..default()
            },
            Transform::from_translation(origin),
            Particle {
                origin,
                velocity: Vec3::ZERO,
                start_size: Vec2::splat(size),
                end_size: Vec2::splat(size * 1.1),
                color: Color::WHITE,
                elapsed: 0.,
                delay,
                lifetime,
                spin: 0.,
            },
            BlastFrames(texture.last_index),
            CombatCmp,
            Pickable::IGNORE,
        ));
    }

    fn beam(&mut self, from: Vec3, to: Vec3, width: f32, color: Color, lifetime: f32) {
        if self.budget == 0 {
            return;
        }
        let d = to - from;
        if d.length_squared() < 0.01 {
            return;
        }
        self.commands.spawn((
            Sprite {
                image: self.textures.beam.clone(),
                color,
                custom_size: Some(Vec2::new(d.length(), width)),
                ..default()
            },
            Transform::from_translation(Vec3::new(
                (from.x + to.x) * 0.5,
                (from.y + to.y) * 0.5,
                COMBAT_EXPLOSION_Z + 0.1,
            ))
            .with_rotation(Quat::from_rotation_z(d.y.atan2(d.x))),
            Particle {
                origin: Vec3::new(
                    (from.x + to.x) * 0.5,
                    (from.y + to.y) * 0.5,
                    COMBAT_EXPLOSION_Z + 0.1,
                ),
                velocity: Vec3::ZERO,
                start_size: Vec2::new(d.length(), width),
                end_size: Vec2::new(d.length(), width * 0.4),
                color,
                elapsed: 0.,
                delay: 0.,
                lifetime,
                spin: 0.,
            },
            CombatCmp,
            Pickable::IGNORE,
        ));
        self.budget -= 1;
    }
}

fn smooth(p: f32) -> f32 {
    p * p * (3. - 2. * p)
}

fn queue_combat_sound(
    audio: &mut MessageWriter<PlayAudioMsg>,
    cooldowns: &mut BTreeMap<&'static str, f32>,
    name: &'static str,
) {
    if let std::collections::btree_map::Entry::Vacant(entry) = cooldowns.entry(name) {
        audio.write(PlayAudioMsg::new(name));
        entry.insert(0.12);
    }
}

#[derive(Component, Default)]
/// Removable camera offset; never persisted or applied to the simulation camera position.
pub struct CombatCameraMotion(Vec3);

/// Brief, small camera jolts only for capital-ship destruction and planetary shockwaves.
pub fn shake_combat_camera(
    mut commands: Commands,
    mut cameras: Query<
        (Entity, &mut Transform, &Projection, Option<&mut CombatCameraMotion>),
        With<MainCamera>,
    >,
    wrecks: Query<&Wreck>,
    rays: Query<&Cinematic>,
) {
    let mut shake = Vec2::ZERO;
    for wreck in &wrecks {
        let age = wreck.elapsed - 0.43 * 1.5;
        if wreck.heavy && (0.0..0.35).contains(&age) {
            shake += Vec2::new((age * 75.).sin(), (age * 59.).cos()) * 1.7 * (1. - age / 0.35);
        }
    }
    for ray in &rays {
        let age = ray.elapsed - 3.7;
        if ray.destroys_planet && (0.0..0.65).contains(&age) {
            shake += Vec2::new((age * 65.).sin(), (age * 81.).sin()) * 5. * (1. - age / 0.65);
        }
    }
    for (entity, mut transform, projection, motion) in &mut cameras {
        let Projection::Orthographic(projection) = projection else {
            continue;
        };
        let offset = (shake.clamp_length_max(5.) * projection.scale).extend(0.);
        if let Some(mut motion) = motion {
            transform.translation += offset - motion.0;
            motion.0 = offset;
        } else {
            transform.translation += offset;
            commands.entity(entity).insert(CombatCameraMotion(offset));
        }
    }
}

/// Restores the exact map camera position even when playback is exited mid-shockwave.
pub fn restore_combat_camera(
    mut commands: Commands,
    mut cameras: Query<(Entity, &mut Transform, &CombatCameraMotion), With<MainCamera>>,
) {
    for (entity, mut transform, motion) in &mut cameras {
        transform.translation -= motion.0;
        commands.entity(entity).remove::<CombatCameraMotion>();
    }
}

/// Advances projectile arrivals, bounded particles, ship reactions, destruction and
/// the death ray using one pause/speed-aware clock. Arrival applies recorded totals once.
pub fn run_combat_animations(
    mut commands: Commands,
    mut shots: MessageReader<SpawnShotMsg>,
    mut pending: Query<
        (Entity, &mut PendingImpact, &mut Sprite, &mut Transform, &mut Visibility),
        Without<CombatUnitCmp>,
    >,
    mut units: Query<
        (Entity, &mut Sprite, &mut Transform, &mut CombatUnitCmp, Option<&mut UnitMotion>),
        (Without<PendingImpact>, Without<Particle>),
    >,
    shields: Query<
        (&Sprite, &GlobalTransform),
        (With<PSCombatImageCmp>, Without<CombatUnitCmp>, Without<PendingImpact>, Without<Particle>),
    >,
    mut particles: Query<
        (
            Entity,
            &mut Particle,
            &mut Sprite,
            &mut Transform,
            Option<&BlastFrames>,
            Option<&PlanetFlash>,
        ),
        (Without<CombatUnitCmp>, Without<PendingImpact>),
    >,
    mut wrecks: Query<(Entity, &mut Wreck)>,
    mut cinematics: Query<&mut Cinematic>,
    mut textures: Local<EffectTextures>,
    mut images: ResMut<Assets<Image>>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut audio: MessageWriter<PlayAudioMsg>,
    presentation: (
        Option<Res<WorldAssets>>,
        Query<
            &mut Sprite,
            (
                With<BackgroundImageCmp>,
                Without<PSCombatImageCmp>,
                Without<CombatUnitCmp>,
                Without<PendingImpact>,
                Without<Particle>,
            ),
        >,
    ),
    mut readouts: Query<
        (Entity, &mut CombatReadout, &mut TextColor, &mut Transform),
        (Without<CombatUnitCmp>, Without<PendingImpact>, Without<Particle>),
    >,
    mut sound_cooldowns: Local<BTreeMap<&'static str, f32>>,
) {
    let (art, mut backdrops) = presentation;
    textures.initialize(&mut images);
    // Audio remains at normal pitch/speed. Limit cues in real seconds so fast-forward
    // cannot stack a whole fleet's sound onto one audible instant.
    sound_cooldowns.retain(|_, remaining| {
        *remaining -= time.delta_secs();
        *remaining > 0.
    });
    let dt = time.delta_secs() * settings.speed();
    let mut painter = Painter {
        commands: &mut commands,
        textures: &textures,
        budget: MAX_PARTICLES.saturating_sub(particles.iter().len()),
        art: art.as_deref(),
    };

    // A fleet of ten thousand ships costs the same number of visible salvos as a
    // small fleet with the same unit types. BTreeMap gives stable launch ordering.
    let mut grouped = BTreeMap::<(Entity, Option<Entity>, bool, bool, usize), PendingImpact>::new();
    let mut counts = BTreeMap::new();
    for message in shots.read() {
        let Some((target, sprite, transform, cu, _)) = units
            .iter()
            .find(|(_, _, _, cu, _)| Some(cu.unit) == message.shot.unit && cu.side == message.side)
        else {
            continue;
        };
        let (mut destination, size) = if cu.unit == Unit::planetary_shield() {
            shields
                .iter()
                .next()
                .map(|(s, t)| (t.translation(), s.custom_size.unwrap_or(Vec2::splat(120.)).x))
                .unwrap_or((transform.translation, 120.))
        } else {
            (transform.translation, sprite.custom_size.unwrap_or(Vec2::splat(120.)).x)
        };
        let source = message.source.map(|s| s.0);
        let key = (target, source, message.repair, message.shot.missed);
        let count = counts.entry(key).or_insert(0usize);

        destination.z = COMBAT_EXPLOSION_Z;
        let mut origin = message.source.map_or(destination + Vec3::Y * size * 2., |s| s.2);
        origin.z = COMBAT_EXPLOSION_Z;
        origin += (destination - origin).normalize_or_zero() * size * 0.28;
        let weapon = if message.repair {
            Weapon::Repair
        } else if message.shot.is_bombing() {
            Weapon::Bomb
        } else {
            message.source.map_or(Weapon::Laser, |s| Weapon::for_unit(s.1))
        };
        let lane = *count % weapon.salvo_limit();
        *count += 1;
        if message.shot.missed {
            destination.x += size * (0.8 + lane as f32 * 0.12);
        } else {
            destination.x += (lane as f32 - 1.) * size * 0.17;
        }
        let impact = grouped
            .entry((target, source, message.repair, message.shot.missed, lane))
            .or_insert(PendingImpact {
                target,
                source,
                origin,
                destination,
                size,
                weapon,
                missed: message.shot.missed,
                hull: 0,
                shield: 0,
                planetary: 0,
                levels: 0,
                elapsed: 0.,
                delay: 0.08 + lane as f32 * 0.12 + weapon.charge(),
                lane: if weapon.salvo_limit() == 1 {
                    0.
                } else {
                    lane as f32 - 1.
                },
                launched: false,
                trail_clock: 0.,
            });
        impact.hull = impact.hull.saturating_add(message.shot.hull_damage);
        impact.shield = impact.shield.saturating_add(message.shot.shield_damage);
        impact.planetary = impact.planetary.saturating_add(message.shot.planetary_shield_damage);
        impact.levels = impact.levels.saturating_add(usize::from(message.shot.killed));
    }
    for (index, (_, mut impact)) in grouped.into_iter().enumerate() {
        impact.delay += (index % 5) as f32 * 0.025;
        let color = impact.weapon.color();
        let massive = matches!(impact.weapon, Weapon::Solar | Weapon::Siege);
        if impact.weapon.charge() > 0. {
            let radius = impact.size
                * if massive {
                    1.35
                } else {
                    0.65
                };
            painter.ring(impact.origin, radius, color.with_alpha(0.6), impact.delay);
            painter.glow(impact.origin, radius * 0.6, color, impact.delay);
            if massive {
                for i in 0..8 {
                    let angle = i as f32 * TAU / 8.;
                    let offset = Vec3::new(angle.cos(), angle.sin(), 0.) * radius * 0.5;
                    painter.particle(
                        false,
                        Particle {
                            origin: impact.origin + offset,
                            velocity: -offset / impact.delay,
                            start_size: Vec2::splat(impact.size * 0.07),
                            end_size: Vec2::splat(impact.size * 0.12),
                            color,
                            elapsed: 0.,
                            delay: 0.,
                            lifetime: impact.delay,
                            spin: 0.,
                        },
                    );
                }
            }
        }
        let weapon = impact.weapon;
        let soft = matches!(weapon, Weapon::Missile | Weapon::Repair);
        painter
            .commands
            .spawn((
                Sprite {
                    image: if soft {
                        textures.glow.clone()
                    } else {
                        textures.beam.clone()
                    },
                    color: color.with_alpha(0.55),
                    custom_size: Some(Vec2::ONE),
                    ..default()
                },
                Transform::from_translation(impact.origin),
                Visibility::Hidden,
                impact,
                CombatCmp,
                Pickable::IGNORE,
            ))
            .with_children(|parent| {
                // A crisp inner core remains readable over the artwork; the parent's
                // local scale stretches every layer together without more per-frame entities.
                for barrel in 0..weapon.barrels() {
                    let offset = (barrel as f32 - (weapon.barrels() - 1) as f32 * 0.5) * 0.55;
                    parent.spawn((
                        Sprite {
                            image: if soft {
                                textures.glow.clone()
                            } else {
                                textures.beam.clone()
                            },
                            color: if weapon == Weapon::Repair {
                                MINT
                            } else {
                                Color::WHITE
                            },
                            custom_size: Some(Vec2::new(
                                0.96,
                                if soft {
                                    0.5
                                } else {
                                    0.18
                                },
                            )),
                            ..default()
                        },
                        Transform::from_xyz(0., offset, 0.01),
                        Pickable::IGNORE,
                    ));
                    if weapon.barrels() > 1 {
                        parent.spawn((
                            Sprite {
                                image: textures.beam.clone(),
                                color,
                                custom_size: Some(Vec2::new(1., 0.43)),
                                ..default()
                            },
                            Transform::from_xyz(0., offset, 0.005),
                            Pickable::IGNORE,
                        ));
                    }
                }
            });
    }

    let mut hit_sound = false;
    let mut repair_sound = false;
    for (entity, mut impact, mut sprite, mut transform, mut visibility) in &mut pending {
        impact.elapsed += dt;
        if impact.elapsed < impact.delay || dt == 0. {
            continue;
        }
        if !impact.launched {
            impact.launched = true;
            *visibility = Visibility::Inherited;
            if let Some(source) = impact.source {
                if let Ok((_, _, _, _, Some(mut motion))) = units.get_mut(source) {
                    if impact.weapon != Weapon::Repair {
                        motion.impulse += (impact.origin - impact.destination).normalize_or_zero()
                            * impact.size
                            * 0.06;
                    }
                }
            }
            if impact.weapon != Weapon::Repair {
                painter.glow(impact.origin, impact.size * 0.35, impact.weapon.color(), 0.18);
                if impact.weapon == Weapon::Bomb {
                    painter.ring(impact.destination, impact.size * 0.9, GOLD.with_alpha(0.5), 0.9);
                }
            } else {
                repair_sound = true;
                painter.beam(
                    impact.origin,
                    impact.destination,
                    impact.size * 0.035,
                    MINT.with_alpha(0.7),
                    0.7,
                );
                painter.ring(impact.destination, impact.size * 1.2, MINT.with_alpha(0.7), 1.1);
            }
        }
        let p = ((impact.elapsed - impact.delay) / impact.weapon.flight()).clamp(0., 1.);
        let position = impact.position(p);
        let direction = position - impact.position((p - 0.02).max(0.));
        let (center, dimensions) = if let Some(width) = impact.weapon.beam_width() {
            (
                (impact.origin + position) * 0.5,
                Vec2::new(impact.origin.distance(position).max(0.01), impact.size * width),
            )
        } else if impact.weapon != Weapon::Repair {
            // The travelling point is the leading tip. Centering a projectile on
            // it would draw its front half through and beyond the target.
            let mut dimensions = impact.weapon.projectile_size(impact.size);
            dimensions.x = dimensions.x.min(impact.origin.distance(position)).max(0.01);
            (position - direction.normalize_or_zero() * dimensions.x * 0.5, dimensions)
        } else {
            (position, impact.weapon.projectile_size(impact.size))
        };
        transform.translation = Vec3::new(center.x, center.y, COMBAT_EXPLOSION_Z + 0.3);
        transform.rotation = Quat::from_rotation_z(direction.y.atan2(direction.x));
        transform.scale = dimensions.extend(1.);
        sprite.color = impact.weapon.color().with_alpha(0.6);
        impact.trail_clock += dt;
        if impact.trail_clock >= 0.035 && p < 1. {
            impact.trail_clock %= 0.035;
            match impact.weapon {
                Weapon::Missile | Weapon::Bomb => {
                    painter.glow(position, impact.size * 0.18, GOLD.with_alpha(0.65), 0.22);
                    painter.glow(
                        position,
                        impact.size * 0.14,
                        Color::srgb(0.42, 0.47, 0.56).with_alpha(0.45),
                        0.48,
                    );
                },
                Weapon::Repair if (0.25..0.8).contains(&p) => {
                    painter.beam(
                        position,
                        impact.destination,
                        impact.size * 0.025,
                        MINT.with_alpha(0.7),
                        0.085,
                    );
                    painter.glow(
                        impact.destination,
                        impact.size * 0.2,
                        MINT.with_alpha(0.25),
                        0.12,
                    );
                },
                Weapon::Railgun | Weapon::Broadside => {
                    painter.beam(
                        impact.position((p - 0.2).max(0.)),
                        position,
                        impact.size * 0.025,
                        impact.weapon.color().with_alpha(0.45),
                        0.12,
                    );
                },
                Weapon::Ion => {
                    // A segmented electrical filament distinguishes ion fire from plasma.
                    let normal = Vec3::new(-direction.y, direction.x, 0.).normalize_or_zero();
                    let mut previous = impact.origin;
                    for i in 1..=7 {
                        let t = i as f32 / 7.;
                        let offset = if i == 7 {
                            0.
                        } else {
                            (impact.elapsed * 45. + i as f32 * 2.3).sin() * impact.size * 0.035
                        };
                        let point = impact.origin.lerp(position, t) + normal * offset;
                        painter.beam(previous, point, impact.size * 0.012, ICE, 0.06);
                        previous = point;
                    }
                },
                Weapon::Solar | Weapon::Siege => {
                    painter.glow(
                        position,
                        impact.size
                            * if impact.weapon == Weapon::Siege {
                                0.22
                            } else {
                                0.45
                            },
                        impact.weapon.color().with_alpha(0.6),
                        0.13,
                    );
                },
                Weapon::Laser
                | Weapon::HeavyLaser
                | Weapon::TwinLaser
                | Weapon::Repeater
                | Weapon::Plasma
                | Weapon::Lance
                | Weapon::Repair => {},
            }
        }
        if p < 1. {
            continue;
        }
        if let Some(width) = impact.weapon.beam_width() {
            painter.beam(
                impact.origin,
                impact.destination,
                impact.size * width,
                impact.weapon.color().with_alpha(0.7),
                0.22,
            );
            for barrel in 0..impact.weapon.barrels() {
                let d = (impact.destination - impact.origin).normalize_or_zero();
                let normal = Vec3::new(-d.y, d.x, 0.);
                let offset = normal
                    * (barrel as f32 - (impact.weapon.barrels() - 1) as f32 * 0.5)
                    * impact.size
                    * width
                    * 0.55;
                painter.beam(
                    impact.origin + offset,
                    impact.destination + offset,
                    impact.size * width * 0.18,
                    Color::WHITE.with_alpha(0.85),
                    0.18,
                );
            }
        }
        painter.commands.entity(entity).despawn();
        let Ok((_, _, target_t, mut cu, motion)) = units.get_mut(impact.target) else {
            continue;
        };
        if impact.weapon == Weapon::Repair {
            cu.hull = cu.hull.saturating_add(impact.hull).min(cu.max_hull);
            painter.ring(target_t.translation, impact.size * 0.85, MINT.with_alpha(0.6), 0.5);
            let origin = Vec3::new(
                target_t.translation.x,
                target_t.translation.y + impact.size * 0.7,
                COMBAT_EXPLOSION_Z + 0.5,
            );
            painter.commands.spawn((
                Text2d::new(format!("+{} HULL", impact.hull)),
                TextFont {
                    font_size: (impact.size * 0.15).into(),
                    ..default()
                },
                TextColor(MINT),
                Transform::from_translation(origin),
                CombatReadout {
                    age: 0.,
                    origin,
                    size: impact.size,
                    color: MINT,
                },
                CombatCmp,
                Pickable::IGNORE,
            ));
            continue;
        }
        if impact.missed {
            if let Some(mut motion) = motion {
                if motion.miss_cooldown > 0. {
                    continue;
                }
                motion.miss_flash = 0.4;
                motion.miss_cooldown = 1.15;
            }
            let center = target_t.translation.truncate().extend(COMBAT_EXPLOSION_Z + 0.5);
            let color = Color::srgb(0.9, 0.95, 1.0);
            for offset in [-0.12, 0.12] {
                painter.beam(
                    center + Vec3::new(-0.3, offset - 0.08, 0.) * impact.size,
                    center + Vec3::new(0.3, offset + 0.08, 0.) * impact.size,
                    impact.size * 0.018,
                    color.with_alpha(0.7),
                    0.35,
                );
            }
            let origin = center - Vec3::Y * impact.size * 0.32;
            painter.commands.spawn((
                Text2d::new("MISS"),
                TextFont {
                    font_size: (impact.size * 0.15).into(),
                    ..default()
                },
                TextColor(color),
                Transform::from_translation(origin),
                CombatReadout {
                    age: 0.,
                    origin,
                    size: impact.size,
                    color,
                },
                CombatCmp,
                Pickable::IGNORE,
            ));
            continue;
        }
        // All weapon hits use the original shared combat cue, including shield hits.
        hit_sound = true;
        if matches!(impact.weapon, Weapon::Solar | Weapon::Siege) {
            painter.ring(
                impact.destination,
                impact.size * 2.0,
                impact.weapon.color().with_alpha(0.75),
                0.55,
            );
            painter.glow(impact.destination, impact.size * 1.25, impact.weapon.color(), 0.3);
        }
        let old_shield = cu.shield;
        if cu.unit == Unit::planetary_shield() {
            cu.shield = cu.shield.saturating_sub(impact.planetary);
        } else if cu.unit.is_building() {
            cu.hull = cu.hull.saturating_sub(impact.levels);
        } else {
            cu.shield = cu.shield.saturating_sub(impact.shield);
            cu.hull = cu.hull.saturating_sub(impact.hull);
        }
        if old_shield > cu.shield {
            painter.ring(impact.destination, impact.size * 1.15, ICE.with_alpha(0.8), 0.4);
            if cu.shield == 0 {
                painter.ring(impact.destination, impact.size * 1.8, ICE, 0.65);
                painter.sparks(impact.destination, impact.size, ICE, 16, true);
                if cu.unit == Unit::planetary_shield() {
                    painter.commands.entity(impact.target).insert(Wreck::new(
                        target_t.translation,
                        impact.size,
                        cu.unit,
                    ));
                }
            }
        }
        if impact.hull > 0 || impact.levels > 0 {
            painter.blast(impact.destination, impact.size * 0.65, 0.42);
            painter.glow(impact.destination, impact.size * 0.62, GOLD, 0.24);
            painter.glow(impact.destination, impact.size * 0.28, Color::WHITE, 0.1);
            painter.sparks(impact.destination, impact.size * 0.65, GOLD, 7, false);
            if let Some(mut motion) = motion {
                motion.flash = 0.18;
            }
            if impact.weapon == Weapon::Bomb && impact.levels > 0 {
                painter.blast(impact.destination, impact.size * 1.4, 0.7);
                painter.ring(impact.destination, impact.size * 1.7, GOLD.with_alpha(0.6), 0.7);
                painter.sparks(impact.destination, impact.size * 1.4, GOLD, 18, true);
                let origin = target_t.translation.truncate().extend(COMBAT_EXPLOSION_Z + 0.5)
                    + Vec3::Y * impact.size * 0.65;
                painter.commands.spawn((
                    Text2d::new(format!(
                        "-{} {}",
                        impact.levels,
                        if impact.levels == 1 {
                            "LEVEL"
                        } else {
                            "LEVELS"
                        }
                    )),
                    TextFont {
                        font_size: (impact.size * 0.17).into(),
                        ..default()
                    },
                    TextColor(GOLD),
                    Transform::from_translation(origin),
                    CombatReadout {
                        age: 0.,
                        origin,
                        size: impact.size,
                        color: GOLD,
                    },
                    CombatCmp,
                    Pickable::IGNORE,
                ));
            }
        }
    }
    if hit_sound {
        queue_combat_sound(&mut audio, &mut sound_cooldowns, "short explosion");
    }
    if repair_sound {
        queue_combat_sound(&mut audio, &mut sound_cooldowns, "repair");
    }

    // Existing card tweens control scale. Add/remove only our translation offset,
    // so firing recoil cannot accumulate drift or overwrite their animation.
    for (entity, mut sprite, mut transform, cu, motion) in &mut units {
        let Some(mut motion) = motion else {
            painter.commands.entity(entity).insert(UnitMotion {
                base_color: sprite.color,
                ..default()
            });
            continue;
        };
        transform.translation -= motion.offset;
        motion.impulse = motion
            .impulse
            .clamp_length_max(sprite.custom_size.unwrap_or(Vec2::splat(120.)).x * 0.18);
        motion.impulse *= (-dt * 9.).exp();
        motion.offset = motion.impulse;
        transform.translation += motion.offset;
        motion.flash = (motion.flash - dt).max(0.);
        motion.miss_flash = (motion.miss_flash - dt).max(0.);
        motion.miss_cooldown = (motion.miss_cooldown - dt).max(0.);
        let damage = 1. - cu.hull as f32 / cu.max_hull.max(1) as f32;
        let tint = 1. - damage * 0.25;
        let base = motion.base_color.to_srgba();
        sprite.color = if motion.flash > 0. {
            Color::srgb(1.5, 1.25, 1.1)
        } else {
            let shimmer = (motion.miss_flash / 0.4 * std::f32::consts::PI).sin() * 0.4;
            Color::srgba(
                base.red * tint + shimmer,
                base.green * tint + shimmer,
                base.blue * tint + shimmer,
                base.alpha,
            )
        };
        if dt == 0. || cu.hull == 0 || damage < 0.2 {
            continue;
        }
        motion.sparks += dt;
        if motion.sparks > 0.65 + (1. - damage) * 1.2 {
            motion.sparks = 0.;
            let size = sprite.custom_size.unwrap_or(Vec2::splat(120.)).x;
            let origin = transform.translation + Vec3::new(size * 0.22, -size * 0.12, 0.);
            painter.sparks(origin, size * 0.3, GOLD.with_alpha(0.65), 3, false);
            painter.glow(origin - Vec3::Y * size * 0.12, size * 0.24, ICE.with_alpha(0.4), 0.13);
        }
    }

    for (entity, mut wreck) in &mut wrecks {
        wreck.elapsed += dt;
        if dt == 0. {
            continue;
        }
        let heavy = if wreck.heavy {
            1.5
        } else {
            1.
        };
        if let Ok((_, _, _, _, Some(mut motion))) = units.get_mut(entity) {
            if wreck.stage == 0 {
                motion.flash = 0.2;
            }
        }
        let stages = [0., 0.13, 0.27, 0.43];
        while wreck.stage < stages.len() && wreck.elapsed >= stages[wreck.stage] * heavy {
            let i = wreck.stage;
            let origin = wreck.origin
                + Vec3::new((i as f32 * 2.4).cos(), (i as f32 * 2.4).sin(), 0.) * wreck.size * 0.2;
            if i < 3 {
                painter.blast(origin, wreck.size * 0.65, 0.42);
                painter.glow(origin, wreck.size * 0.75, GOLD, 0.24);
                painter.sparks(origin, wreck.size * 0.7, GOLD, 6, false);
            } else {
                painter.blast(wreck.origin, wreck.size * 1.6 * heavy, 0.95);
                painter.glow(wreck.origin, wreck.size * 1.8 * heavy, GOLD, 0.55);
                painter.glow(wreck.origin, wreck.size * 0.95 * heavy, Color::WHITE, 0.16);
                painter.ring(wreck.origin, wreck.size * 2.4 * heavy, GOLD.with_alpha(0.65), 0.85);
                painter.sparks(wreck.origin, wreck.size * heavy, GOLD, 18, true);
                audio.write(PlayAudioMsg::new(if wreck.heavy {
                    "large explosion"
                } else {
                    "explosion"
                }));
            }
            wreck.stage += 1;
        }
        if wreck.elapsed > 0.62 * heavy {
            painter.commands.entity(entity).despawn();
        }
    }

    for mut ray in &mut cinematics {
        ray.elapsed += dt;
        if dt == 0. {
            continue;
        }
        let focus = ray.origin.lerp(ray.target, 0.22);
        if ray.stage == 0 {
            ray.stage = 1;
            painter.commands.spawn((
                Sprite {
                    color: Color::BLACK.with_alpha(0.),
                    custom_size: Some(ray.viewport),
                    ..default()
                },
                Transform::from_xyz(ray.target.x, ray.target.y, COMBAT_BACKGROUND_Z + 0.5),
                Particle {
                    origin: Vec3::new(ray.target.x, ray.target.y, COMBAT_BACKGROUND_Z + 0.5),
                    velocity: Vec3::ZERO,
                    start_size: ray.viewport,
                    end_size: ray.viewport,
                    color: Color::BLACK.with_alpha(0.8),
                    elapsed: 0.,
                    delay: 0.,
                    lifetime: DEATH_RAY_DURATION,
                    spin: 0.,
                },
                CombatCmp,
                Pickable::IGNORE,
            ));
            for i in 0..28 {
                let angle = i as f32 * TAU / 28.;
                let offset = Vec3::new(angle.cos(), angle.sin(), 0.) * ray.size * 2.2;
                painter.particle(
                    false,
                    Particle {
                        origin: ray.origin + offset,
                        velocity: -offset / 1.65,
                        start_size: Vec2::splat(ray.size * 0.045),
                        end_size: Vec2::splat(ray.size * 0.18),
                        color: GOLD,
                        elapsed: 0.,
                        delay: 0.,
                        lifetime: 1.65,
                        spin: 0.,
                    },
                );
            }
            for radius in [1.2, 1.8, 2.4] {
                painter.particle(
                    true,
                    Particle {
                        origin: ray.origin,
                        velocity: Vec3::ZERO,
                        start_size: Vec2::splat(ray.size * radius),
                        end_size: Vec2::splat(ray.size * 0.15),
                        color: GOLD.with_alpha(0.6),
                        elapsed: 0.,
                        delay: 0.,
                        lifetime: 1.9,
                        spin: radius,
                    },
                );
            }
            painter.glow(ray.origin, ray.size * 1.7, GOLD, 2.1);
        }
        if ray.stage == 1 && ray.elapsed >= 1.15 {
            ray.stage = 2;
            for i in 0..8 {
                let angle = i as f32 * TAU / 8.;
                let emitter =
                    ray.origin + Vec3::new(angle.cos(), angle.sin(), 0.) * ray.size * 0.48;
                painter.beam(emitter, focus, ray.size * 0.07, GOLD, 1.6);
                painter.beam(emitter, focus, ray.size * 0.018, Color::WHITE, 1.6);
            }
            painter.glow(focus, ray.size * 0.9, Color::WHITE, 1.4);
            painter.ring(focus, ray.size * 1.8, GOLD, 1.2);
        }
        if ray.stage == 2 && ray.elapsed >= 2.0 {
            ray.stage = 3;
            painter.beam(focus, ray.target, ray.size * 1.5, GOLD.with_alpha(0.6), 1.9);
            painter.beam(focus, ray.target, ray.size * 0.6, GOLD, 1.9);
            painter.beam(focus, ray.target, ray.size * 0.17, Color::WHITE, 1.9);
            painter.glow(ray.target, ray.size * 4.5, GOLD, 1.9);
            painter.ring(ray.target, ray.size * 5., GOLD, 1.8);
            painter.sparks(ray.target, ray.size * 2.5, Color::WHITE, 32, false);
        }
        if ray.stage == 3 && ray.elapsed >= 2.9 {
            ray.stage = 4;
            if ray.destroys_planet {
                // Branching fissures spread outward before the planet breaks apart.
                for i in 0..12 {
                    let angle = i as f32 * TAU / 12.;
                    let direction = Vec3::new(angle.cos(), angle.sin(), 0.);
                    let elbow = ray.target + direction * ray.size * (0.8 + (i % 3) as f32 * 0.2);
                    let end = elbow
                        + Vec3::new((angle + 0.3).cos(), (angle + 0.3).sin(), 0.) * ray.size * 1.4;
                    painter.beam(ray.target, elbow, ray.size * 0.06, GOLD, 1.0);
                    painter.beam(elbow, end, ray.size * 0.035, GOLD, 1.0);
                }
            }
        }
        if ray.stage == 4 && ray.elapsed >= 3.7 {
            ray.stage = 5;
            if !ray.destroys_planet {
                painter.ring(ray.target, ray.size * 4., GOLD, 1.3);
                painter.glow(ray.target, ray.size * 3., GOLD.with_alpha(0.6), 1.0);
                continue;
            }
            // The backdrop changes in the same frame as an opaque flash. It stays
            // covered while the overlapping atlas explosions spread across the screen.
            let flash_origin = Vec3::new(ray.target.x, ray.target.y, COMBAT_EXPLOSION_Z + 0.12);
            let flash_size = ray.viewport * 1.08;
            painter.commands.spawn((
                Sprite {
                    color: Color::WHITE,
                    custom_size: Some(flash_size),
                    ..default()
                },
                Transform::from_translation(flash_origin),
                Particle {
                    origin: flash_origin,
                    velocity: Vec3::ZERO,
                    start_size: flash_size,
                    end_size: flash_size,
                    color: Color::WHITE,
                    elapsed: 0.,
                    delay: 0.,
                    lifetime: 1.0,
                    spin: 0.,
                },
                PlanetFlash,
                CombatCmp,
                Pickable::IGNORE,
            ));
            if let Some(art) = painter.art {
                for mut backdrop in &mut backdrops {
                    backdrop.image = art.image("destroyed bg");
                }
            }
            for row in 0..4 {
                for column in 0..6 {
                    let x = column as f32 - 2.5;
                    let y = row as f32 - 1.5;
                    let center = ray.target
                        + Vec3::new(x * ray.viewport.x * 0.18, y * ray.viewport.y * 0.25, 0.);
                    painter.blast_after(
                        center,
                        ray.viewport.max_element() * 0.53,
                        1.75,
                        (x.abs() + y.abs()) * 0.07,
                    );
                }
            }
            painter.glow(ray.target, ray.size * 8., Color::WHITE, 0.35);
            painter.blast(ray.target, ray.size * 6., 1.8);
            painter.glow(ray.target, ray.size * 11., GOLD, 1.7);
            painter.beam(
                ray.target - Vec3::X * ray.viewport.x * 0.65,
                ray.target + Vec3::X * ray.viewport.x * 0.65,
                ray.size * 0.13,
                Color::WHITE,
                0.7,
            );
            for (factor, color) in [(1.2, Color::WHITE), (1.8, GOLD), (2.2, VIOLET)] {
                painter.ring(
                    ray.target,
                    ray.viewport.max_element() * factor,
                    color.with_alpha(0.65),
                    2.1,
                );
            }
            painter.sparks(ray.target, ray.size * 6., GOLD, 80, true);
            for (entity, sprite, transform, cu, _) in &units {
                if cu.side == Side::Defender {
                    painter.commands.entity(entity).insert(Wreck::new(
                        transform.translation,
                        sprite.custom_size.unwrap_or(Vec2::splat(ray.size)).x,
                        cu.unit,
                    ));
                }
            }
        }
        let boom_times = [3.7, 3.92, 4.15, 4.4, 4.65];
        while ray.destroys_planet
            && ray.boom_stage < boom_times.len()
            && ray.elapsed >= boom_times[ray.boom_stage]
        {
            let cue = if ray.boom_stage % 2 == 0 {
                "large explosion"
            } else {
                "explosion"
            };
            queue_combat_sound(&mut audio, &mut sound_cooldowns, cue);
            ray.boom_stage += 1;
        }
    }

    for (entity, mut label, mut color, mut transform) in &mut readouts {
        label.age += dt;
        if label.age >= 1.15 {
            painter.commands.entity(entity).despawn();
            continue;
        }
        transform.translation = label.origin + Vec3::Y * label.size * 0.14 * label.age;
        color.0 = label.color.with_alpha((1.15 - label.age).min(0.4) / 0.4);
    }

    for (entity, mut particle, mut sprite, mut transform, frames, planet_flash) in &mut particles {
        particle.elapsed += dt;
        if particle.elapsed < particle.delay {
            continue;
        }
        let age = particle.elapsed - particle.delay;
        let p = (age / particle.lifetime).clamp(0., 1.);
        if p >= 1. {
            painter.commands.entity(entity).despawn();
            continue;
        }
        transform.translation = particle.origin + particle.velocity * age;
        transform.rotate_z(particle.spin * dt);
        sprite.custom_size = Some(particle.start_size.lerp(particle.end_size, smooth(p)));
        let envelope = if planet_flash.is_some() {
            ((1. - p) / 0.6).min(1.)
        } else {
            (p * 18.).min(1.) * (1. - p).powf(1.3)
        };
        sprite.color = particle.color.with_alpha(particle.color.alpha() * envelope);
        if let (Some(frames), Some(atlas)) = (frames, sprite.texture_atlas.as_mut()) {
            atlas.index = (p * frames.0 as f32) as usize;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/core/combat_effects.rs"]
mod tests;
