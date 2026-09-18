//! Short map encounters: deformable creature art, continuous flight and shared combat weapons.
//! Everything here is presentation; the report already determines who survives.

use std::f32::consts::{PI, TAU};

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology, VertexAttributeValues};
use bevy::prelude::*;

use super::battle::FaunaPresentation;
use super::model::MapCmp;
use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::combat::effects::{fauna_weapon_trail, EffectTextures, Weapon, WeaponTrail};
use crate::core::combat::report::ReportId;
use crate::core::constants::EXPLOSION_Z;
use crate::core::missions::{MissionId, SuppressedMapMissions};
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::units::fauna::SpaceFauna;
use crate::core::units::Unit;
use crate::utils::NameFromEnum;

pub(super) const FAUNA_ACTION_SECONDS: f32 = 7.5;
pub(super) const FAUNA_AFTERMATH_SECONDS: f32 =
    FAUNA_ACTION_SECONDS + super::AFTERMATH_LABEL_EXTENSION_SECONDS;
pub(super) const FAUNA_RESULT_LABEL_Y: f32 = 110.0;
// Flight, weapons, body motion, audio and fades all use this authored timeline.
const CHOREOGRAPHY_SECONDS: f32 = 2.8;
const FAUNA_LABEL_APPEAR_SECONDS: f32 = FAUNA_ACTION_SECONDS * 1.95 / CHOREOGRAPHY_SECONDS;
const FAUNA_LABEL_FADE_IN_SECONDS: f32 = FAUNA_ACTION_SECONDS * 0.15 / CHOREOGRAPHY_SECONDS;
const FAUNA_LABEL_FADE_OUT_SECONDS: f32 = FAUNA_ACTION_SECONDS * 0.25 / CHOREOGRAPHY_SECONDS;
const ENTRY_END: f32 = 0.28;
const ORBIT_END: f32 = 1.08;
const STRIKE_END: f32 = 1.66;
const EXIT_END: f32 = 2.55;
const OPENING_RIPPLE_COUNT: usize = 4;
const OPENING_RIPPLE_INTERVAL: f32 = 0.10;
const OPENING_RIPPLE_SECONDS: f32 = 0.45;
const MESH_COLUMNS: usize = 20;
const MESH_ROWS: usize = 16;
const SHOT_PARTS: usize = 48;

#[derive(Component)]
pub(crate) struct FaunaEffect {
    pub(super) report: ReportId,
    pub(super) mission: MissionId,
    turn: usize,
    pub(super) timer: Timer,
    destroyed: bool,
    victory: bool,
    fleet_weapon: Option<Weapon>,
    mission_rotation: f32,
    mission_size: f32,
    creatures: Vec<Creature>,
    sounds: u8,
}

#[derive(Clone, Copy)]
struct Creature {
    fauna: SpaceFauna,
    slot: usize,
    count: usize,
    survives: bool,
}

impl Creature {
    fn size(self) -> f32 {
        // Keep small shoaling creatures compact and make the stronger tiers visibly larger.
        44.0 + self.fauna.production() as f32 * 10.0
    }

    fn delay(self) -> f32 {
        self.slot as f32 * 0.085
    }

    fn radius(self) -> f32 {
        88.0 + self.fauna.production() as f32 * 2.0 + self.slot as f32 * 5.0
    }

    fn orbit(self, t: f32) -> (Vec2, Vec2) {
        let angle =
            2.55 + TAU * self.slot as f32 / self.count.max(1) as f32 + (t - ENTRY_END) * 5.1;
        let radial = Vec2::from_angle(angle);
        let position = radial * self.radius();
        (position, Vec2::new(-radial.y, radial.x) * self.radius() * 5.1)
    }

    fn position(self, elapsed: f32) -> Vec2 {
        let t = elapsed - self.delay();
        if t < ENTRY_END {
            let (end, tangent) = self.orbit(ENTRY_END);
            let start = Vec2::from_angle(end.to_angle() - 0.8) * (self.radius() + 67.0);
            return hermite(
                start,
                end,
                (end - start) * 0.7,
                tangent * ENTRY_END,
                (t / ENTRY_END).clamp(0.0, 1.0),
            );
        }
        if t < ORBIT_END {
            return self.orbit(t).0;
        }
        let (launch, tangent) = self.orbit(ORBIT_END);
        let direction = -launch.normalize();
        let impact_velocity = direction * 320.0;
        if t < STRIKE_END {
            let duration = STRIKE_END - ORBIT_END;
            return hermite(
                launch,
                Vec2::ZERO,
                tangent * duration,
                impact_velocity * duration,
                (t - ORBIT_END) / duration,
            );
        }
        let duration = EXIT_END - STRIKE_END;
        let exit = direction * 165.0 + Vec2::new(-direction.y, direction.x) * 38.0;
        hermite(
            Vec2::ZERO,
            exit,
            impact_velocity * duration,
            direction * 80.0,
            ((t - STRIKE_END) / duration).clamp(0.0, 1.0),
        )
    }

    fn pose(self, elapsed: f32) -> Transform {
        let position = self.position(elapsed);
        let velocity = self.position(elapsed + 0.004) - self.position(elapsed - 0.004);
        let mut heading = velocity.to_angle();
        let t = elapsed - self.delay();
        // Banking into the strike aligns the mouth before discharge; the body then follows the
        // fly-through tangent. Sampled from time, so a slow frame cannot change the trajectory.
        let aim = smooth((t - 0.90) / 0.26) * (1.0 - smooth((t - 1.40) / 0.24));
        heading = angle_lerp(heading, (-position).to_angle(), aim);
        Transform::from_translation(position.extend(0.18 + self.slot as f32 * 0.01))
            .with_rotation(Quat::from_rotation_z(heading - anatomy(self.fauna).1.to_angle()))
    }

    fn mouth(self, elapsed: f32) -> Vec2 {
        self.pose(elapsed)
            .transform_point((anatomy(self.fauna).0 * self.size()).extend(0.0))
            .truncate()
    }

    fn alpha(self, elapsed: f32) -> f32 {
        let t = elapsed - self.delay();
        let arrival = smooth(t / 0.13);
        if self.survives {
            arrival * (1.0 - smooth((elapsed - 2.35) / 0.34))
        } else {
            arrival * (1.0 - smooth((t - STRIKE_END - 0.03) / 0.30))
        }
    }

    /// Shrinks a defeated creature into the fleet during its final inward strike.
    fn dive_envelope(self, elapsed: f32) -> f32 {
        let t = elapsed - self.delay();
        1.0 - smooth((t - ORBIT_END) / (STRIKE_END - ORBIT_END))
    }
}

/// Mouth anchors are measured on each actual map sprite in normalized, Y-up coordinates.
/// Keeping the jaw fixed while deforming the rest of the mesh also pins its weapon muzzle.
fn anatomy(fauna: SpaceFauna) -> (Vec2, Vec2) {
    use SpaceFauna::*;
    let (mouth, forward) = match fauna {
        AetherRay => (Vec2::new(0.29, -0.10), Vec2::new(1.0, -0.32)),
        IonWisp => (Vec2::new(0.33, 0.08), Vec2::new(1.0, 0.32)),
        VoidManta => (Vec2::new(-0.33, -0.07), Vec2::new(-1.0, -0.28)),
        VoidMantaCalf => (Vec2::new(-0.34, -0.05), Vec2::new(-1.0, -0.2)),
        CrystalLeviathan => (Vec2::new(-0.39, -0.23), Vec2::new(-1.0, -0.58)),
        CrystalShardling => (Vec2::new(-0.40, -0.26), Vec2::new(-1.0, -0.67)),
        Gravemaw => (Vec2::new(0.16, -0.03), Vec2::new(1.0, -0.36)),
        StarKraken => (Vec2::new(0.015, 0.025), Vec2::new(0.6, -1.0)),
        StarKrakenSpawn => (Vec2::new(0.02, 0.0), Vec2::new(0.58, -1.0)),
        NebulaGrazer => (Vec2::new(-0.35, -0.015), Vec2::NEG_X),
        NebulaGrazerCalf => (Vec2::new(-0.38, 0.025), Vec2::NEG_X),
        RiftSerpent => (Vec2::new(-0.39, 0.075), Vec2::new(-1.0, 0.10)),
        SolarRoc => (Vec2::new(0.13, -0.015), Vec2::new(1.0, -0.3)),
        ElderStarDragon => (Vec2::new(-0.38, 0.035), Vec2::new(-1.0, 0.1)),
        StarDragonWyrmling => (Vec2::new(-0.38, 0.10), Vec2::new(-1.0, 0.28)),
        NullstarBehemoth => (Vec2::new(0.37, -0.005), Vec2::X),
    };
    (mouth, forward.normalize())
}

fn deform(fauna: SpaceFauna, point: Vec2, elapsed: f32) -> Vec2 {
    use SpaceFauna::*;
    let (mouth, forward) = anatomy(fauna);
    let normal = Vec2::new(-forward.y, forward.x);
    let relative = point - mouth;
    let along = relative.dot(forward);
    let across = relative.dot(normal);
    let tail = (-along).max(0.0);
    let phase = elapsed * 12.0;
    let (bend, stretch) = match fauna {
        RiftSerpent => {
            (0.10 * (phase - tail * 11.0).sin() * tail, 0.015 * (phase - tail * 5.0).sin() * tail)
        },
        IonWisp => (
            0.07 * (phase * 0.65 - tail * 13.0 + across * 6.0).sin() * tail,
            0.06 * (phase * 0.7).sin() * tail,
        ),
        StarKraken | StarKrakenSpawn => {
            let limb = relative.length().min(0.7);
            (
                0.12 * (phase * 0.85 - across * 9.0 - along * 8.0).sin() * limb,
                0.08 * (phase * 0.85 + across * 8.0).cos() * limb,
            )
        },
        Gravemaw | CrystalLeviathan | CrystalShardling => {
            (0.012 * (phase * 0.4 - tail * 4.0).sin() * tail, 0.015 * (phase * 0.5).sin() * tail)
        },
        NullstarBehemoth => {
            (0.045 * (phase * 0.55 - tail * 7.0).sin() * tail, 0.025 * (phase * 0.4).sin() * tail)
        },
        AetherRay | VoidManta | VoidMantaCalf | NebulaGrazer | NebulaGrazerCalf | SolarRoc
        | ElderStarDragon | StarDragonWyrmling => {
            // The torso stays stable; the outer membranes sweep and fold with delayed tips.
            let wing = across.abs().powf(1.4);
            (
                across * 0.26 * (phase - wing * 4.5).sin()
                    + tail * 0.035 * (phase - tail * 8.0).sin(),
                wing * 0.16 * (phase - wing * 5.0).cos(),
            )
        },
    };
    point + normal * bend + forward * stretch
}

fn creature_mesh(creature: Creature) -> Mesh {
    let mut positions = Vec::with_capacity((MESH_COLUMNS + 1) * (MESH_ROWS + 1));
    let mut uvs = Vec::with_capacity(positions.capacity());
    let mut indices = Vec::with_capacity(MESH_COLUMNS * MESH_ROWS * 6);
    for y in 0..=MESH_ROWS {
        for x in 0..=MESH_COLUMNS {
            let uv = Vec2::new(x as f32 / MESH_COLUMNS as f32, y as f32 / MESH_ROWS as f32);
            positions.push([(uv.x - 0.5) * creature.size(), (0.5 - uv.y) * creature.size(), 0.0]);
            uvs.push(uv.to_array());
            if x < MESH_COLUMNS && y < MESH_ROWS {
                let i = (y * (MESH_COLUMNS + 1) + x) as u32;
                let row = (MESH_COLUMNS + 1) as u32;
                indices.extend([i, i + row, i + 1, i + 1, i + row, i + row + 1]);
            }
        }
    }
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
        .with_inserted_indices(Indices::U32(indices))
}

#[derive(Component)]
pub(super) enum FaunaPart {
    Mission,
    Creature(usize),
    Ripple(usize),
    Shot {
        creature: usize,
        returning: bool,
        sample: usize,
    },
    Blast {
        creature: Option<usize>,
        index: usize,
        last_frame: usize,
    },
    Label(f32),
}

#[derive(Clone, Copy, Default)]
enum Mask {
    #[default]
    Glow,
    Ring,
    Beam,
    Shard,
    Missile,
}

impl Mask {
    fn image(self, textures: &EffectTextures) -> Handle<Image> {
        match self {
            Self::Glow => textures.glow.clone(),
            Self::Ring => textures.ring.clone(),
            Self::Beam => textures.beam.clone(),
            Self::Shard => textures.shard.clone(),
            Self::Missile => textures.missile.clone(),
        }
    }
}

#[derive(Clone, Copy)]
struct FxSample {
    mask: Mask,
    position: Vec2,
    size: Vec2,
    angle: f32,
    color: Color,
}

impl Default for FxSample {
    fn default() -> Self {
        Self {
            mask: Mask::Glow,
            position: Vec2::ZERO,
            size: Vec2::ZERO,
            angle: 0.0,
            color: Color::NONE,
        }
    }
}

struct ShotSamples {
    parts: [FxSample; SHOT_PARTS],
    len: usize,
}

impl Default for ShotSamples {
    fn default() -> Self {
        Self {
            parts: [FxSample::default(); SHOT_PARTS],
            len: 0,
        }
    }
}

impl ShotSamples {
    fn add(&mut self, mask: Mask, position: Vec2, size: Vec2, angle: f32, color: Color) {
        if self.len < SHOT_PARTS {
            self.parts[self.len] = FxSample {
                mask,
                position,
                size,
                angle,
                color,
            };
            self.len += 1;
        }
    }

    fn beam(&mut self, from: Vec2, to: Vec2, width: f32, color: Color) {
        let delta = to - from;
        self.add(
            Mask::Beam,
            (from + to) * 0.5,
            Vec2::new(delta.length(), width),
            delta.to_angle(),
            color,
        );
    }
}

fn mission_pose(effect: &FaunaEffect, elapsed: f32) -> (Vec2, f32) {
    let bank = smooth(elapsed / 0.25) * (1.0 - smooth((elapsed - 1.95) / 0.55));
    (
        Vec2::new((elapsed * 5.0).sin() * 5.0, (elapsed * 6.0).sin() * 7.0) * bank,
        effect.mission_rotation + (elapsed * 6.0).sin() * 0.13 * bank,
    )
}

fn shot_sample(
    effect: &FaunaEffect,
    creature: Creature,
    returning: bool,
    elapsed: f32,
) -> ShotSamples {
    let mut samples = ShotSamples::default();
    let weapon = if returning {
        let Some(weapon) = effect.fleet_weapon else {
            return samples;
        };
        weapon
    } else {
        Weapon::for_unit(Unit::Fauna(creature.fauna))
    };
    let delay = creature.delay();
    // Two defensive bursts during the orbit; one sustained creature strike during the dive.
    let (start, duration) = if returning {
        (
            if elapsed < 1.02 + delay {
                0.48 + delay
            } else {
                1.04 + delay
            },
            0.32,
        )
    } else {
        (1.11 + delay, 0.43)
    };
    let age = elapsed - start;
    let charge = 0.16;
    if age < -charge || age > duration + 0.20 {
        return samples;
    }
    let (mission, rotation) = mission_pose(effect, elapsed);
    let (from, to) = if returning {
        (
            mission + Vec2::from_angle(rotation) * effect.mission_size * 0.34,
            creature.position(elapsed),
        )
    } else {
        (creature.mouth(elapsed), mission)
    };
    let size = if returning {
        24.0
    } else {
        32.0 + creature.fauna.production() as f32 * 2.0
    };
    let color = weapon.color();
    if age < 0.0 {
        let p = ((age + charge) / charge).clamp(0.0, 1.0);
        samples.add(
            Mask::Glow,
            from,
            Vec2::splat(size * (0.15 + 0.65 * p)),
            0.0,
            color.with_alpha(p * 0.85),
        );
        if !returning {
            samples.add(
                Mask::Ring,
                from,
                Vec2::splat(size * (0.9 - p * 0.55)),
                0.0,
                color.with_alpha(p * 0.65),
            );
        }
        return samples;
    }
    let p = (age / duration).clamp(0.0, 1.0);
    let direction = (to - from).normalize_or_zero();
    let tip = from.lerp(to, (p * 2.0).min(1.0));
    let fade = (1.0 - smooth((p - 0.82) / 0.18)) * smooth(p / 0.07);
    if p < 1.0 {
        samples.add(Mask::Glow, from, Vec2::splat(size * 0.42), 0.0, color.with_alpha(fade * 0.8));
        if let Some(width) = weapon.beam_width() {
            samples.beam(from, tip, size * width, color.with_alpha(fade * 0.65));
            samples.beam(from, tip, size * width * 0.18, Color::WHITE.with_alpha(fade));
        } else {
            // Pulses and acid are travelling projectiles, just as in full combat.
            let tip = from.lerp(to, p);
            let dimensions = weapon.projectile_size(size);
            let center = tip - direction * dimensions.x.min(from.distance(tip)) * 0.5;
            let mask = if matches!(weapon, Weapon::Missile | Weapon::Bomb) {
                Mask::Missile
            } else {
                Mask::Beam
            };
            samples.add(mask, center, dimensions, direction.to_angle(), color.with_alpha(fade));
            samples.add(
                mask,
                center,
                dimensions * Vec2::new(0.95, 0.18),
                direction.to_angle(),
                Color::WHITE.with_alpha(fade),
            );
        }
        for slot in 0..4 {
            let trail_age = slot as f32 * 0.045;
            let trail_p = ((age - trail_age) / duration).clamp(0.0, 1.0);
            if age < trail_age {
                continue;
            }
            let position = from.lerp(
                to,
                if weapon.beam_width().is_some() {
                    (trail_p * 2.0).min(1.0)
                } else {
                    trail_p
                },
            );
            fauna_weapon_trail(
                weapon,
                from.extend(0.0),
                position.extend(0.0),
                size,
                elapsed - trail_age,
                trail_p,
                |trail| match trail {
                    WeaponTrail::Glow(at, radius, tint, life)
                    | WeaponTrail::Ring(at, radius, tint, life) => {
                        let ring = matches!(trail, WeaponTrail::Ring(..));
                        let f = (trail_age / life).clamp(0.0, 1.0);
                        let radius = radius
                            * if ring {
                                0.2 + f * 0.8
                            } else {
                                1.0 + f * 0.8
                            };
                        samples.add(
                            if ring {
                                Mask::Ring
                            } else {
                                Mask::Glow
                            },
                            at.truncate(),
                            Vec2::splat(radius),
                            0.0,
                            tint.with_alpha(tint.alpha() * (1.0 - f) * fade),
                        );
                    },
                    WeaponTrail::Beam(a, b, width, tint, life) => {
                        if trail_age < life {
                            samples.beam(
                                a.truncate(),
                                b.truncate(),
                                width.max(0.6),
                                tint.with_alpha((1.0 - trail_age / life) * fade),
                            );
                        }
                    },
                    WeaponTrail::Sparks(at, radius, tint, count) => {
                        for index in 0..count {
                            let angle = index as f32 * 2.399_963;
                            let direction = Vec2::from_angle(angle);
                            let position = at.truncate() + direction * radius * trail_age * 3.0;
                            samples.add(
                                Mask::Shard,
                                position,
                                Vec2::new(size * 0.07, size * 0.03),
                                angle,
                                tint.with_alpha((1.0 - trail_age / 0.25).max(0.0) * fade),
                            );
                        }
                    },
                },
            );
        }
    }
    if p > 0.55 {
        let flash = (1.0 - ((age - duration) / 0.20).abs()).clamp(0.0, 1.0);
        samples.add(Mask::Glow, to, Vec2::splat(size * 0.85), 0.0, color.with_alpha(flash * 0.85));
        samples.add(Mask::Glow, to, Vec2::splat(size * 0.24), 0.0, Color::WHITE.with_alpha(flash));
    }
    samples
}

pub(super) fn spawn_fauna_aftermath(
    commands: &mut Commands,
    report: ReportId,
    turn: usize,
    position: Vec2,
    outcome: &FaunaPresentation,
    player_color: Color,
    assets: &WorldAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
    textures: &EffectTextures,
) {
    let creatures = outcome
        .creatures
        .iter()
        .copied()
        .enumerate()
        .map(|(slot, fauna)| Creature {
            fauna,
            slot,
            count: outcome.creatures.len(),
            survives: outcome.creature_survivors[slot],
        })
        .collect::<Vec<_>>();
    let effect = FaunaEffect {
        report,
        mission: outcome.mission,
        turn,
        timer: Timer::from_seconds(FAUNA_AFTERMATH_SECONDS, TimerMode::Once),
        destroyed: outcome.destroyed,
        victory: outcome.victory,
        fleet_weapon: outcome.return_fire.map(Weapon::for_unit),
        mission_rotation: outcome.mission_rotation,
        mission_size: outcome.mission_size,
        creatures: creatures.clone(),
        sounds: 0,
    };
    commands
        .spawn((
            Transform::from_translation(position.extend(EXPLOSION_Z)),
            Visibility::Inherited,
            Pickable::IGNORE,
            MapCmp,
            effect,
        ))
        .with_children(|parent| {
            parent.spawn((
                Sprite {
                    image: assets.image(&outcome.mission_image),
                    color: player_color,
                    custom_size: Some(Vec2::splat(outcome.mission_size)),
                    flip_x: outcome.mission_flip_x,
                    flip_y: outcome.mission_flip_y,
                    ..default()
                },
                Transform::from_rotation(Quat::from_rotation_z(outcome.mission_rotation)),
                Pickable::IGNORE,
                FaunaPart::Mission,
            ));
            for (slot, creature) in creatures.iter().copied().enumerate() {
                parent.spawn((
                    Mesh2d(meshes.add(creature_mesh(creature))),
                    MeshMaterial2d(materials.add(ColorMaterial {
                        color: Color::WHITE.with_alpha(0.0),
                        texture: Some(
                            assets.image(format!("map {}", creature.fauna.to_lowername())),
                        ),
                        ..default()
                    })),
                    creature.pose(0.0),
                    Pickable::IGNORE,
                    FaunaPart::Creature(slot),
                ));
                for returning in [false, true] {
                    if returning && outcome.return_fire.is_none() {
                        continue;
                    }
                    for sample in 0..SHOT_PARTS {
                        parent.spawn((
                            Sprite {
                                image: textures.glow.clone(),
                                color: Color::NONE,
                                ..default()
                            },
                            Transform::from_xyz(0.0, 0.0, 0.4),
                            Pickable::IGNORE,
                            FaunaPart::Shot {
                                creature: slot,
                                returning,
                                sample,
                            },
                        ));
                    }
                }
            }
            let blast = assets.texture("explosion");
            let mut spawn_blast = |creature, index| {
                parent.spawn((
                    Sprite {
                        image: blast.image.clone(),
                        texture_atlas: Some(blast.atlas.clone()),
                        color: Color::NONE,
                        custom_size: Some(Vec2::splat(64.0)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 0.3),
                    Pickable::IGNORE,
                    FaunaPart::Blast {
                        creature,
                        index,
                        last_frame: blast.last_index,
                    },
                ));
            };
            if outcome.destroyed {
                for index in 0..3 {
                    spawn_blast(None, index);
                }
            }
            for (slot, creature) in creatures.iter().enumerate() {
                if !outcome.victory && !creature.survives {
                    spawn_blast(Some(slot), 0);
                }
            }
            // Opening pulses announce every encounter, independently of its eventual outcome.
            for index in 0..OPENING_RIPPLE_COUNT {
                parent.spawn((
                    // The shared combat mask keeps the expanding rings smooth at close zoom.
                    Sprite {
                        image: textures.ring.clone(),
                        color: player_color.with_alpha(0.0),
                        custom_size: Some(Vec2::splat(outcome.mission_size * 2.25)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, 0.02),
                    Pickable::IGNORE,
                    FaunaPart::Ripple(index),
                ));
            }
            let y = FAUNA_RESULT_LABEL_Y;
            parent.spawn((
                Text2d::new(outcome.label),
                TextFont {
                    font: assets.font("bold").into(),
                    font_size: 17.0.into(),
                    ..default()
                },
                TextColor(player_color.with_alpha(0.0)),
                Transform::from_xyz(0.0, y, 0.5),
                Pickable::IGNORE,
                FaunaPart::Label(y),
            ));
        });
}

pub(super) fn animate_fauna_aftermath(
    mut commands: Commands,
    time: Res<Time>,
    game_state: Res<State<GameState>>,
    settings: Res<Settings>,
    mut suppressed: ResMut<SuppressedMapMissions>,
    mut effects: Query<(Entity, &mut FaunaEffect, &Children, &mut Visibility)>,
    mut parts: Query<(
        &FaunaPart,
        &mut Transform,
        Option<&mut Sprite>,
        Option<&Mesh2d>,
        Option<&MeshMaterial2d<ColorMaterial>>,
        Option<&mut TextColor>,
    )>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut textures: Local<EffectTextures>,
    mut images: ResMut<Assets<Image>>,
    mut audio: MessageWriter<PlayAudioMsg>,
) {
    if effects.is_empty() {
        return;
    }
    textures.initialize(&mut images);
    for (entity, mut effect, children, mut visibility) in &mut effects {
        if effect.turn != settings.turn {
            suppressed.release(effect.mission);
            commands.entity(entity).despawn();
            continue;
        }
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
            suppressed.release(effect.mission);
            commands.entity(entity).despawn();
            continue;
        }
        let effect_elapsed = effect.timer.elapsed_secs();
        let elapsed =
            (effect_elapsed / FAUNA_ACTION_SECONDS).clamp(0.0, 1.0) * CHOREOGRAPHY_SECONDS;
        for (threshold, flag) in [(0.48, 1), (1.11, 2), (STRIKE_END, 4)] {
            if elapsed < threshold || effect.sounds & flag != 0 {
                continue;
            }
            effect.sounds |= flag;
            match flag {
                1 => {
                    if let Some(cue) = effect.fleet_weapon.and_then(Weapon::launch_cue) {
                        audio.write(cue);
                    }
                },
                2 => {
                    if let Some(creature) = effect.creatures.first() {
                        if let Some(cue) =
                            Weapon::for_unit(Unit::Fauna(creature.fauna)).launch_cue()
                        {
                            audio.write(cue);
                        }
                    }
                },
                _ => {
                    if effect.destroyed
                        || (!effect.victory
                            && effect.creatures.iter().any(|creature| !creature.survives))
                    {
                        audio.write(PlayAudioMsg::new("short explosion"));
                    }
                },
            }
        }
        let volleys: [(ShotSamples, ShotSamples); 3] = std::array::from_fn(|index| {
            effect.creatures.get(index).map_or_else(
                || (ShotSamples::default(), ShotSamples::default()),
                |creature| {
                    (
                        shot_sample(&effect, *creature, false, elapsed),
                        shot_sample(&effect, *creature, true, elapsed),
                    )
                },
            )
        });
        for child in children.iter() {
            let Ok((part, mut transform, sprite, mesh, material, text)) = parts.get_mut(child)
            else {
                continue;
            };
            match *part {
                FaunaPart::Mission => {
                    let (position, rotation) = mission_pose(&effect, elapsed);
                    transform.translation = position.extend(0.1);
                    transform.rotation = Quat::from_rotation_z(rotation);
                    if let Some(mut sprite) = sprite {
                        sprite.color.set_alpha(if effect.destroyed {
                            1.0 - smooth((elapsed - STRIKE_END) / 0.24)
                        } else {
                            1.0
                        });
                    }
                },
                FaunaPart::Creature(slot) => {
                    let creature = effect.creatures[slot];
                    // A defeated pack recedes into the mission throughout its final dive;
                    // victorious fauna keep flying past the destroyed mission as before.
                    let flight_time = if effect.victory {
                        elapsed.min(STRIKE_END + creature.delay())
                    } else {
                        elapsed
                    };
                    *transform = creature.pose(flight_time);
                    let dive_envelope = if effect.victory {
                        creature.dive_envelope(elapsed)
                    } else {
                        1.0
                    };
                    transform.scale = Vec3::splat(dive_envelope);
                    let alpha = creature.alpha(flight_time) * dive_envelope;
                    if let Some(mut material) =
                        material.and_then(|handle| materials.get_mut(&handle.0))
                    {
                        material.color = Color::srgb(0.86, 0.93, 1.0).with_alpha(alpha);
                    }
                    if let Some(mut mesh) = mesh.and_then(|handle| meshes.get_mut(&handle.0)) {
                        if let Some(VertexAttributeValues::Float32x3(positions)) =
                            mesh.attribute_mut(Mesh::ATTRIBUTE_POSITION)
                        {
                            for (index, vertex) in positions.iter_mut().enumerate() {
                                let point = Vec2::new(
                                    (index % (MESH_COLUMNS + 1)) as f32 / MESH_COLUMNS as f32 - 0.5,
                                    0.5 - (index / (MESH_COLUMNS + 1)) as f32 / MESH_ROWS as f32,
                                );
                                *vertex =
                                    (deform(creature.fauna, point, elapsed + slot as f32 * 0.4)
                                        * creature.size())
                                    .extend(0.0)
                                    .to_array();
                            }
                        }
                    }
                },
                FaunaPart::Ripple(index) => {
                    let progress = ((elapsed - index as f32 * OPENING_RIPPLE_INTERVAL)
                        / OPENING_RIPPLE_SECONDS)
                        .clamp(0.0, 1.0);
                    let outward = 1.0 - (1.0 - progress).powi(2);
                    transform.translation = mission_pose(&effect, elapsed).0.extend(0.02);
                    transform.scale = Vec3::splat(1.0 + 2.3 * outward);
                    if let Some(mut sprite) = sprite {
                        let fade_in = (progress / 0.08).min(1.0);
                        sprite.color.set_alpha(0.8 * fade_in * (1.0 - progress).powf(1.2));
                    }
                },
                FaunaPart::Shot {
                    creature,
                    returning,
                    sample,
                } => {
                    let volley = if returning {
                        &volleys[creature].1
                    } else {
                        &volleys[creature].0
                    };
                    let part = volley.parts[sample];
                    if let Some(mut sprite) = sprite {
                        sprite.image = part.mask.image(&textures);
                        sprite.color = part.color;
                        sprite.custom_size = Some(part.size);
                    }
                    transform.translation = part.position.extend(0.4 + sample as f32 * 0.0001);
                    transform.rotation = Quat::from_rotation_z(part.angle);
                },
                FaunaPart::Blast {
                    creature,
                    index,
                    last_frame,
                } => {
                    let delay = creature
                        .map_or(index as f32 * 0.075, |slot| effect.creatures[slot].delay());
                    let p = (elapsed - STRIKE_END - delay) / 0.62;
                    if let Some(mut sprite) = sprite {
                        sprite.color = Color::WHITE.with_alpha(if (0.0..1.0).contains(&p) {
                            1.0
                        } else {
                            0.0
                        });
                        if let Some(atlas) = &mut sprite.texture_atlas {
                            atlas.index =
                                ((p.clamp(0.0, 1.0) * last_frame as f32) as usize).min(last_frame);
                        }
                    }
                    let position = creature.map_or_else(
                        || Vec2::from_angle(index as f32 * 2.4) * index as f32 * 9.0,
                        |slot| {
                            effect.creatures[slot]
                                .position((elapsed).min(STRIKE_END + delay + 0.12))
                        },
                    );
                    transform.translation = position.extend(0.35);
                },
                FaunaPart::Label(y) => {
                    let (label_y, alpha) = super::aftermath_label_motion(
                        y,
                        effect_elapsed,
                        FAUNA_LABEL_APPEAR_SECONDS,
                        FAUNA_AFTERMATH_SECONDS,
                        FAUNA_LABEL_FADE_IN_SECONDS,
                        FAUNA_LABEL_FADE_OUT_SECONDS,
                    );
                    transform.translation.y = label_y;
                    if let Some(mut text) = text {
                        text.0.set_alpha(alpha);
                    }
                },
            }
        }
    }
}

fn smooth(value: f32) -> f32 {
    let p = value.clamp(0.0, 1.0);
    p * p * (3.0 - 2.0 * p)
}

fn angle_lerp(from: f32, to: f32, weight: f32) -> f32 {
    from + ((to - from + PI).rem_euclid(TAU) - PI) * weight
}

fn hermite(start: Vec2, end: Vec2, start_tangent: Vec2, end_tangent: Vec2, p: f32) -> Vec2 {
    let p2 = p * p;
    let p3 = p2 * p;
    start * (2.0 * p3 - 3.0 * p2 + 1.0)
        + start_tangent * (p3 - 2.0 * p2 + p)
        + end * (-2.0 * p3 + 3.0 * p2)
        + end_tangent * (p3 - p2)
}

#[cfg(test)]
#[path = "../../../tests/core/map_fauna.rs"]
mod tests;
