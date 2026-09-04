//! Bevy systems that render and animate the strategic map projection.

pub use super::scanner::ScannerCmp;

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::color::{palettes::css::WHITE, Mix};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_tweening::lens::ColorMaterialColorLens;
use bevy_tweening::{AnimTarget, EaseMethod, RepeatCount, Tween, TweenAnim, Tweenable};
use itertools::Itertools;
use rand::{rng, RngExt};
use strum::IntoEnumIterator;
use voronator::delaunator::Point;
use voronator::VoronoiDiagram;

use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{
    BACKGROUND_Z, BUTTON_TEXT_SIZE, OWN_COLOR, PHALANX_DISTANCE, PLANET_Z, RADAR_DISTANCE,
    TITLE_TEXT_SIZE, VORONOI_Z,
};
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::{Map, MapCmp};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::utils::{
    cursor, spawn_main_button, MainButtonLabelCmp, TransformOrbitLens, TransformOrbitSpinLens,
};
use crate::core::missions::{Mission, MissionId, Missions};
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Unit};
use crate::multiplayer::client::MultiplayerSession;
use crate::utils::NameFromEnum;

#[derive(Component)]
/// Bevy component mapping a rendered planet entity to a stable planet ID.
pub struct PlanetCmp {
    /// Stable identifier used to cross-reference this value.
    pub id: PlanetId,
}

#[derive(Component)]
/// Slow, presentation-only light variation applied to a world sprite.
pub(crate) struct PlanetAmbienceCmp {
    phase: f32,
    minimum_brightness: f32,
}

#[derive(Component)]
/// Root of the decorative, non-authoritative solar landmark.
pub(crate) struct SolarStarCmp;

#[derive(Component)]
/// One sourced image participating in the solar surface crossfade.
pub(crate) struct SolarStarFrameCmp {
    index: usize,
}

#[derive(Component)]
/// Slowly drifting transparent gas cloud behind the strategic worlds.
pub(crate) struct NebulaCmp {
    phase: f32,
}

#[derive(Component)]
/// Decorative black-hole landmark sharing the stellar proximity ambience.
pub(crate) struct BlackHoleCmp {
    frames: Vec<Handle<Image>>,
}

#[derive(Component)]
/// One of the two sprites used to crossfade between sourced NASA animation frames.
pub(crate) struct BlackHoleFrameCmp {
    slot: usize,
}

#[derive(Component)]
/// One point in the wrapping foreground star layer.
pub(crate) struct AmbientStarCmp {
    anchor: Vec2,
    phase: f32,
    speed: f32,
    base_alpha: f32,
    minimum_alpha: f32,
    pulse_power: f32,
}

#[derive(Component)]
/// A sparse beacon that briefly flares before reappearing at a new position.
pub(crate) struct AmbientPulsarCmp {
    seed: u32,
    phase: f32,
    cycle_duration: f32,
    peak_alpha: f32,
}

#[derive(Component)]
/// One light ray belonging to a briefly flaring ambient pulsar.
pub(crate) struct AmbientPulsarRayCmp {
    alpha_factor: f32,
}

#[derive(Clone, Copy)]
struct AmbientStarLayer {
    count: u32,
    seed: u32,
    depth: f32,
    camera_follow: f32,
    zoom_power: f32,
    drift: Vec2,
    minimum_size: f32,
    size_range: f32,
    minimum_base_alpha: f32,
    base_alpha_range: f32,
    minimum_alpha: f32,
    minimum_alpha_range: f32,
    pulse_power: f32,
    minimum_speed: f32,
    speed_range: f32,
}

#[derive(Component)]
/// A short-lived streak crossing behind the strategic map.
pub(crate) struct AmbientCometCmp {
    age: f32,
    lifetime: f32,
    velocity: Vec2,
    peak_alpha: f32,
}

#[derive(Component)]
pub(crate) struct AmbientCometPartCmp {
    alpha_factor: f32,
}

#[derive(Resource, Debug)]
/// Local scheduling state for occasional presentation-only comet streaks.
pub(crate) struct AmbientCometSpawner {
    remaining: f32,
    sequence: u32,
}

impl Default for AmbientCometSpawner {
    fn default() -> Self {
        Self {
            // Show the first streak soon enough to establish the effect, then make it occasional.
            remaining: 4.5,
            sequence: 0,
        }
    }
}

const AMBIENT_STAR_FIELD_SIZE: Vec2 = Vec2::new(5_200.0, 3_200.0);
const AMBIENT_PULSAR_FIELD_SIZE: Vec2 = Vec2::new(2_200.0, 1_300.0);
const SOLAR_STAR_FRAME_COUNT: usize = 4;
const SOLAR_STAR_FRAME_SECONDS: f32 = 1.8;
const SOLAR_STAR_SIZE: f32 = 1_440.0;
const SOLAR_STAR_DEPTH: f32 = BACKGROUND_Z + 0.78;
const NEBULA_SIZE: Vec2 = Vec2::new(1_900.0, 1_566.0);
const NEBULA_DEPTH: f32 = BACKGROUND_Z + 0.1;
const NEBULA_PARALLAX_FOLLOW: f32 = 0.9;
const BLACK_HOLE_SIZE: Vec2 = Vec2::new(480.0, 270.0);
const BLACK_HOLE_DEPTH: f32 = BACKGROUND_Z + 0.7;
const BLACK_HOLE_PARALLAX_FOLLOW: f32 = 0.82;
const BLACK_HOLE_FRAME_COUNT: usize = 66;
const BLACK_HOLE_FRAME_SECONDS: f32 = 0.08;
const BLACK_HOLE_OPACITY: f32 = 0.4;

impl PlanetCmp {
    /// Creates a new value from the supplied state.
    pub fn new(id: PlanetId) -> Self {
        Self {
            id,
        }
    }
}

#[derive(Component)]
/// Bevy component mapping a rendered mission entity to a stable mission ID.
pub struct MissionCmp {
    /// Stable identifier used to cross-reference this value.
    pub id: MissionId,
}

impl MissionCmp {
    /// Creates a new value from the supplied state.
    pub fn new(id: MissionId) -> Self {
        Self {
            id,
        }
    }
}

#[derive(Component)]
/// Timed map explosion associated with a destroyed planet.
pub struct ExplosionCmp {
    /// Timer controlling the current effect frame.
    pub timer: Timer,
    /// Highest valid texture-atlas frame index.
    pub last_index: usize,
    /// Stable planet associated with this component.
    pub planet: PlanetId,
}

#[derive(Component)]
/// Bevy component marking planet name presentation entities.
pub struct PlanetNameCmp;

#[derive(Component)]
/// Bevy component marking planet resources presentation entities.
pub struct PlanetResourcesCmp;

/// Tracks the displayed owner's color so a shield's pulse updates after control changes.
#[derive(Component, Default)]
pub struct PlanetaryShieldCmp {
    color: Option<Color>,
}

impl PlanetaryShieldCmp {
    /// Creates a new value from the supplied state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Breathes in and out with a smooth reversal, preserving the owner's hue.
    pub fn tween(color: Color) -> Tween {
        Tween::new(
            // One complete wave has the same opacity and zero slope at both ends, so the
            // loop never jumps or abruptly reverses when it wraps back to its start.
            EaseMethod::CustomFunction(|phase| 0.5 - 0.5 * (TAU * phase).cos()),
            Duration::from_secs(3),
            ColorMaterialColorLens {
                start: color.with_alpha(0.0),
                end: color.with_alpha(0.95),
            },
        )
        .with_repeat_count(RepeatCount::Infinite)
    }
}

#[derive(Component)]
/// Bevy component marking space dock presentation entities.
pub struct SpaceDockCmp;

#[derive(Component)]
/// Animated, faction-tinted jump-gate marker orbiting a world.
pub struct JumpGateCmp;

const TERRITORY_TRANSITION_SECONDS: f32 = 1.35;

#[derive(Component, Debug)]
/// Local presentation state for smooth ownership-color and visibility changes.
pub struct TerritoryTransitionCmp {
    target: Color,
    target_visible: bool,
    start: Color,
    elapsed: f32,
    initialized: bool,
}

impl Default for TerritoryTransitionCmp {
    fn default() -> Self {
        Self {
            target: OWN_COLOR.with_alpha(0.0),
            target_visible: false,
            start: OWN_COLOR.with_alpha(0.0),
            elapsed: 0.0,
            initialized: false,
        }
    }
}

#[derive(Component)]
/// Bevy component marking voronoi presentation entities.
pub struct VoronoiCmp(pub PlanetId);

#[derive(Component)]
/// Rendered ownership-border edge with a canonical deduplication key.
pub struct VoronoiEdgeCmp {
    /// Stable planet associated with this component.
    pub planet: PlanetId,
    /// Canonical quantized endpoints used to deduplicate this border edge.
    pub key: (i32, i32, i32, i32),
}

#[derive(Component)]
/// Bevy component marking end turn label presentation entities.
pub struct EndTurnLabelCmp;

#[derive(Component)]
/// Bevy component marking end turn button presentation entities.
pub struct EndTurnButtonCmp;

#[derive(Component)]
/// Bevy component marking spectator label presentation entities.
pub struct SpectatorLabelCmp;

/// Canonicalizes an undirected Voronoi edge for deduplication.
fn edge_key(v1: Vec2, v2: Vec2) -> (i32, i32, i32, i32) {
    let precision = 5.0;
    let mut a = ((v1.x / precision).round() as i32, (v1.y / precision).round() as i32);
    let mut b = ((v2.x / precision).round() as i32, (v2.y / precision).round() as i32);
    if a > b {
        std::mem::swap(&mut a, &mut b);
    } // Make direction irrelevant
    (a.0, a.1, b.0, b.1)
}

/// Spawns ownership geometry with transform depths used by Bevy's transparent sort.
fn spawn_voronoi_cells(
    commands: &mut Commands,
    map: &Map,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let Some(voronoi) = VoronoiDiagram::<Point>::from_tuple(
        &(-10000., -10000.),
        &(10000., 10000.),
        &map.planets.iter().map(|p| (p.position.x as f64, p.position.y as f64)).collect::<Vec<_>>(),
    ) else {
        warn!("Could not generate ownership cells for the strategic map.");
        return;
    };

    for (planet, cell) in map.planets.iter().zip(voronoi.cells()) {
        let points = cell.points();
        let n = points.len();
        if n < 3 {
            continue;
        }
        // Keep mesh vertices on their local plane. Baking depth into vertices alone leaves
        // the entity at z=0, so transparent cells can sort behind the map background.
        let positions =
            points.iter().map(|p| Vec3::new(p.x as f32, p.y as f32, 0.0)).collect::<Vec<_>>();
        let indices = (1..n - 1).flat_map(|i| [0, i as u32, (i + 1) as u32]).collect();
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_indices(Indices::U32(indices));
        commands.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(OWN_COLOR.with_alpha(0.01))),
            Transform::from_xyz(0.0, 0.0, VORONOI_Z),
            Visibility::Hidden,
            Pickable::IGNORE,
            VoronoiCmp(planet.id),
            TerritoryTransitionCmp::default(),
            MapCmp,
        ));

        for j in 0..n {
            let a = points[j];
            let b = points[(j + 1) % n];
            let v1 = Vec2::new(a.x as f32, a.y as f32);
            let v2 = Vec2::new(b.x as f32, b.y as f32);
            let mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    vec![v1.extend(0.0), v2.extend(0.0)],
                )
                .with_inserted_indices(Indices::U32(vec![0, 1]));
            commands.spawn((
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(materials.add(OWN_COLOR.with_alpha(0.58))),
                Transform::from_xyz(0.0, 0.0, VORONOI_Z + 0.1),
                Visibility::Hidden,
                Pickable::IGNORE,
                VoronoiEdgeCmp {
                    planet: planet.id,
                    key: edge_key(v1, v2),
                },
                TerritoryTransitionCmp::default(),
                MapCmp,
            ));
        }
    }
}

/// Returns a stable pseudo-random value in `[0, 1]` for presentation placement.
fn visual_noise(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    value as f32 / u32::MAX as f32
}

fn spawn_ambient_star_layer(commands: &mut Commands, layer: AmbientStarLayer) {
    commands
        .spawn((
            Transform::from_xyz(0.0, 0.0, layer.depth),
            Visibility::Inherited,
            ParallaxCmp::new(layer.camera_follow, 1.0, layer.zoom_power, layer.drift),
            Pickable::IGNORE,
            MapCmp,
        ))
        .with_children(|parent| {
            for index in 0..layer.count {
                let seed = layer.seed.wrapping_add(index.wrapping_mul(7));
                let x = (visual_noise(seed.wrapping_add(1)) - 0.5) * AMBIENT_STAR_FIELD_SIZE.x;
                let y = (visual_noise(seed.wrapping_add(2)) - 0.5) * AMBIENT_STAR_FIELD_SIZE.y;
                let size =
                    layer.minimum_size + visual_noise(seed.wrapping_add(3)) * layer.size_range;
                let base_alpha = layer.minimum_base_alpha
                    + visual_noise(seed.wrapping_add(4)) * layer.base_alpha_range;
                let temperature = visual_noise(seed.wrapping_add(5));
                let color = if temperature < 0.24 {
                    Color::srgba(0.62, 0.76, 1.0, base_alpha)
                } else if temperature > 0.88 {
                    Color::srgba(1.0, 0.82, 0.58, base_alpha)
                } else {
                    Color::srgba(0.9, 0.95, 1.0, base_alpha)
                };

                parent.spawn((
                    Sprite::from_color(color, Vec2::splat(size)),
                    Transform {
                        translation: Vec3::new(x, y, 0.0),
                        rotation: Quat::from_rotation_z(visual_noise(seed.wrapping_add(6)) * TAU),
                        ..default()
                    },
                    Pickable::IGNORE,
                    AmbientStarCmp {
                        anchor: Vec2::new(x, y),
                        phase: visual_noise(seed.wrapping_add(7)) * TAU,
                        speed: layer.minimum_speed
                            + visual_noise(seed.wrapping_add(8)) * layer.speed_range,
                        base_alpha,
                        minimum_alpha: layer.minimum_alpha
                            + visual_noise(seed.wrapping_add(9)) * layer.minimum_alpha_range,
                        pulse_power: layer.pulse_power,
                    },
                ));
            }
        });
}

fn spawn_ambient_pulsars(commands: &mut Commands) {
    commands
        .spawn((
            Transform::from_xyz(0.0, 0.0, BACKGROUND_Z + 0.64),
            Visibility::Inherited,
            ParallaxCmp::new(0.38, 1.0, 0.05, Vec2::new(0.18, -0.08)),
            Pickable::IGNORE,
            MapCmp,
        ))
        .with_children(|parent| {
            for index in 0..18_u32 {
                let seed = 0x6c91_3ea7_u32.wrapping_add(index.wrapping_mul(13));
                let size = 1.6 + visual_noise(seed.wrapping_add(1)) * 1.2;
                let tint = if visual_noise(seed.wrapping_add(2)) > 0.82 {
                    Color::srgb(1.0, 0.88, 0.68)
                } else {
                    Color::srgb(0.78, 0.9, 1.0)
                };

                parent
                    .spawn((
                        Sprite::from_color(tint.with_alpha(0.0), Vec2::splat(size)),
                        Transform::default(),
                        Pickable::IGNORE,
                        AmbientPulsarCmp {
                            seed,
                            phase: visual_noise(seed.wrapping_add(3)),
                            cycle_duration: 5.0 + visual_noise(seed.wrapping_add(4)) * 6.0,
                            peak_alpha: 0.68 + visual_noise(seed.wrapping_add(5)) * 0.24,
                        },
                    ))
                    .with_children(|pulsar| {
                        for (ray_size, rotation, alpha_factor) in [
                            (Vec2::new(size * 7.0, size * 0.24), 0.0, 0.4),
                            (Vec2::new(size * 5.2, size * 0.18), PI * 0.5, 0.26),
                        ] {
                            pulsar.spawn((
                                Sprite::from_color(tint.with_alpha(0.0), ray_size),
                                Transform::from_rotation(Quat::from_rotation_z(rotation)),
                                Pickable::IGNORE,
                                AmbientPulsarRayCmp {
                                    alpha_factor,
                                },
                            ));
                        }
                    });
            }
        });
}

/// Adds sparse stars and intermittent beacons between the backdrop and ownership projection.
fn spawn_ambient_stars(commands: &mut Commands) {
    spawn_ambient_star_layer(
        commands,
        AmbientStarLayer {
            count: 525,
            seed: 0x14d2_8a31,
            depth: BACKGROUND_Z + 0.22,
            camera_follow: 0.66,
            zoom_power: 0.12,
            drift: Vec2::new(0.28, -0.12),
            minimum_size: 0.8,
            size_range: 2.0,
            minimum_base_alpha: 0.16,
            base_alpha_range: 0.4,
            minimum_alpha: 0.48,
            minimum_alpha_range: 0.2,
            pulse_power: 1.25,
            minimum_speed: 0.22,
            speed_range: 0.68,
        },
    );
    spawn_ambient_star_layer(
        commands,
        AmbientStarLayer {
            count: 350,
            seed: 0xf274_9b13,
            depth: BACKGROUND_Z + 0.4,
            camera_follow: 0.43,
            zoom_power: 0.07,
            drift: Vec2::new(-0.34, 0.24),
            minimum_size: 0.9,
            size_range: 2.5,
            minimum_base_alpha: 0.2,
            base_alpha_range: 0.46,
            minimum_alpha: 0.22,
            minimum_alpha_range: 0.24,
            pulse_power: 2.1,
            minimum_speed: 0.4,
            speed_range: 0.92,
        },
    );
    spawn_ambient_star_layer(
        commands,
        AmbientStarLayer {
            count: 275,
            seed: 0xa8e5_3c79,
            depth: BACKGROUND_Z + 0.56,
            camera_follow: 0.2,
            zoom_power: 0.03,
            drift: Vec2::new(0.9, -0.42),
            minimum_size: 1.2,
            size_range: 3.4,
            minimum_base_alpha: 0.28,
            base_alpha_range: 0.62,
            minimum_alpha: 0.0,
            minimum_alpha_range: 0.16,
            pulse_power: 3.4,
            minimum_speed: 0.65,
            speed_range: 1.25,
        },
    );
    spawn_ambient_pulsars(commands);
}

fn scenery_corner_from_seed(seed: u32) -> Vec2 {
    match seed & 3 {
        0 => Vec2::new(-1.0, -1.0),
        1 => Vec2::new(1.0, -1.0),
        2 => Vec2::new(-1.0, 1.0),
        _ => Vec2::ONE,
    }
}

fn map_scenery_corner(map: &Map) -> Vec2 {
    let seed = map.planets.iter().fold(0x915f_43b7_u32, |seed, planet| {
        seed.rotate_left(7)
            ^ planet.position.x.to_bits().wrapping_mul(0x9e37_79b9)
            ^ planet.position.y.to_bits().rotate_left(13)
    });
    scenery_corner_from_seed(seed)
}

fn map_corner(map: &Map, direction: Vec2) -> Vec2 {
    Vec2::new(
        if direction.x < 0.0 {
            map.rect.min.x
        } else {
            map.rect.max.x
        },
        if direction.y < 0.0 {
            map.rect.min.y
        } else {
            map.rect.max.y
        },
    )
}

/// Hangs a large arc over a map-dependent corner while keeping most of the star off-map.
fn solar_star_position(map: &Map) -> Vec2 {
    let corner = map_scenery_corner(map);
    map_corner(map, corner) + corner * SOLAR_STAR_SIZE * 0.3
}

fn black_hole_position(map: &Map) -> Vec2 {
    let sun_corner = map_scenery_corner(map);
    let edge_direction = Vec2::new(-sun_corner.x, 0.0);
    let edge_x = if edge_direction.x < 0.0 {
        map.rect.min.x
    } else {
        map.rect.max.x
    };
    let x = edge_x - edge_direction.x * BLACK_HOLE_SIZE.x * 0.42;
    let usable_half_height = (map.rect.half_size().y - BLACK_HOLE_SIZE.y * 0.55).max(0.0);
    let preferred_y = -sun_corner.y * usable_half_height * 0.48;
    if map.planets.is_empty() {
        return Vec2::new(x, map.rect.center().y + preferred_y);
    }

    [-0.72, -0.48, -0.24, 0.0, 0.24, 0.48, 0.72]
        .into_iter()
        .map(|slot| Vec2::new(x, map.rect.center().y + usable_half_height * slot))
        .max_by(|left, right| {
            let nearest = |candidate: Vec2| {
                map.planets
                    .iter()
                    .map(|planet| candidate.distance_squared(planet.position))
                    .fold(f32::INFINITY, f32::min)
            };
            nearest(*left).total_cmp(&nearest(*right))
        })
        .unwrap_or(Vec2::new(x, map.rect.center().y + preferred_y))
}

fn nebula_position(map: &Map) -> Vec2 {
    let sun_corner = map_scenery_corner(map);
    let direction = Vec2::new(-sun_corner.x, sun_corner.y);
    map.rect.center() + direction * map.rect.half_size() * Vec2::new(0.28, 0.18)
}

fn spawn_background_landmarks(commands: &mut Commands, assets: &WorldAssets, map: &Map) {
    let nebula_anchor = nebula_position(map);
    commands
        .spawn((
            Name::new("Decorative nebula parallax"),
            Transform::from_xyz(0.0, 0.0, NEBULA_DEPTH),
            Visibility::Inherited,
            ParallaxCmp::new(NEBULA_PARALLAX_FOLLOW, 1.0, 0.025, Vec2::new(0.06, -0.03)),
            Pickable::IGNORE,
            MapCmp,
        ))
        .with_children(|parent| {
            parent.spawn((
                Sprite {
                    image: assets.image("nebula"),
                    color: Color::srgba(0.86, 0.88, 1.0, 0.58),
                    custom_size: Some(NEBULA_SIZE),
                    ..default()
                },
                Transform::from_translation(nebula_anchor.extend(0.0)),
                Pickable::IGNORE,
                NebulaCmp {
                    phase: visual_noise(0x24b7_96d1) * TAU,
                },
            ));
        });

    let black_hole_anchor = black_hole_position(map);
    let black_hole_frames = (1..=BLACK_HOLE_FRAME_COUNT)
        .map(|index| assets.image(format!("black hole {index}")))
        .collect::<Vec<_>>();
    commands
        .spawn((
            Name::new("Decorative black hole parallax"),
            Transform::from_xyz(0.0, 0.0, BLACK_HOLE_DEPTH),
            Visibility::Inherited,
            ParallaxCmp::new(BLACK_HOLE_PARALLAX_FOLLOW, 1.0, 0.04, Vec2::new(-0.08, 0.04)),
            Pickable::IGNORE,
            MapCmp,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Name::new("Animated NASA black hole"),
                    Transform::from_translation(black_hole_anchor.extend(0.0)),
                    Visibility::Inherited,
                    Pickable::IGNORE,
                    BlackHoleCmp {
                        frames: black_hole_frames,
                    },
                ))
                .with_children(|black_hole| {
                    for slot in 0..2 {
                        black_hole.spawn((
                            Sprite {
                                image: assets.image(format!("black hole {}", slot + 1)),
                                color: Color::srgba(
                                    0.72,
                                    0.72,
                                    0.72,
                                    if slot == 0 {
                                        BLACK_HOLE_OPACITY
                                    } else {
                                        0.0
                                    },
                                ),
                                custom_size: Some(BLACK_HOLE_SIZE),
                                ..default()
                            },
                            Transform::from_xyz(0.0, 0.0, slot as f32 * 0.001),
                            Pickable::IGNORE,
                            BlackHoleFrameCmp {
                                slot,
                            },
                        ));
                    }
                });
        });
}

fn spawn_solar_star(commands: &mut Commands, assets: &WorldAssets, map: &Map) {
    commands
        .spawn((
            Name::new("Decorative solar star"),
            Transform::from_translation(solar_star_position(map).extend(SOLAR_STAR_DEPTH)),
            Visibility::Inherited,
            Pickable::IGNORE,
            SolarStarCmp,
            MapCmp,
        ))
        .with_children(|parent| {
            for index in 0..SOLAR_STAR_FRAME_COUNT {
                parent.spawn((
                    Sprite {
                        image: assets.image(format!("solar star {}", index + 1)),
                        color: Color::srgba(
                            1.0,
                            1.0,
                            1.0,
                            if index == 0 {
                                1.0
                            } else {
                                0.0
                            },
                        ),
                        custom_size: Some(Vec2::splat(SOLAR_STAR_SIZE)),
                        ..default()
                    },
                    Transform::from_xyz(0.0, 0.0, index as f32 * 0.01),
                    Pickable::IGNORE,
                    SolarStarFrameCmp {
                        index,
                    },
                ));
            }
        });
}

/// Selects a planet and updates the mission origin for worlds under the player's control.
pub(crate) fn select_planet(planet: &Planet, state: &mut UiState, player: &Player) {
    state.planet_selected = Some(planet.id);
    state.focus_planet = None;
    state.to_selected = true;
    state.mission = false;
    state.combat_report = None;
    if player.owns(planet) || player.controls(planet) {
        state.mission_info.origin = planet.id;
    }
}

/// Draws the map interface and emits any resulting local actions.
pub fn draw_map(
    mut commands: Commands,
    camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
    map: Res<Map>,
    player: Res<Player>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    assets: Res<WorldAssets>,
) {
    let (mut camera_t, mut projection) = camera.into_inner();
    let Projection::Orthographic(projection) = &mut *projection else {
        return;
    };

    commands
        .spawn((
            Sprite::from_image(assets.image("bg")),
            Transform::from_xyz(0., 0., BACKGROUND_Z),
            Pickable::default(),
            ParallaxCmp::new(0.84, 0.6, 0.8, Vec2::ZERO),
            MapCmp,
        ))
        .observe(cursor::<Over>(SystemCursorIcon::Default))
        .observe(
            |event: On<Pointer<Press>>,
             mut commands: Commands,
             mut state: ResMut<UiState>,
             window_e: Single<Entity, With<Window>>| {
                if event.button == PointerButton::Primary {
                    state.planet_selected = None;
                    state.focus_planet = None;
                    state.to_selected = false;
                    commands.entity(*window_e).insert(CursorIcon::from(SystemCursorIcon::Grabbing));
                }
            },
        )
        .observe(cursor::<Release>(SystemCursorIcon::Default))
        .observe(
            |event: On<Pointer<Move>>,
             camera_q: Single<(&mut Transform, &Projection), With<MainCamera>>,
             mut state: ResMut<UiState>,
             mouse: Res<ButtonInput<MouseButton>>,
             window: Single<&CursorIcon, With<Window>>| {
                if mouse.pressed(MouseButton::Left)
                    && matches!(*window, CursorIcon::System(SystemCursorIcon::Grabbing))
                {
                    let (mut camera_t, projection) = camera_q.into_inner();

                    let Projection::Orthographic(projection) = projection else {
                        return;
                    };

                    if !event.delta.x.is_nan() && !event.delta.y.is_nan() {
                        camera_t.translation.x -= event.delta.x * projection.scale;
                        camera_t.translation.y += event.delta.y * projection.scale;
                        state.to_selected = false;
                        state.focus_planet = None;
                    }
                }
            },
        )
        .observe(|_: On<Pointer<Click>>, mut state: ResMut<UiState>| {
            state.mission = false;
            state.combat_report = None;
        });

    spawn_ambient_stars(&mut commands);
    spawn_background_landmarks(&mut commands, &assets, &map);
    spawn_solar_star(&mut commands, &assets, &map);

    for planet in &map.planets {
        let planet_id = planet.id;

        commands
            .spawn((
                Sprite {
                    image: assets.image(planet.image()),
                    custom_size: Some(Vec2::splat(planet.size())),
                    ..default()
                },
                Transform {
                    translation: planet.position.extend(PLANET_Z),
                    ..default()
                },
                Pickable::default(),
                PlanetCmp::new(planet.id),
                PlanetAmbienceCmp {
                    phase: visual_noise(planet.id as u32 + 701) * TAU,
                    minimum_brightness: if planet.is_moon() {
                        0.9
                    } else {
                        0.94
                    },
                },
                MapCmp,
            ))
            .observe(cursor::<Over>(SystemCursorIcon::Pointer))
            .observe(cursor::<Out>(SystemCursorIcon::Default))
            .observe(move |_: On<Pointer<Over>>, mut state: ResMut<UiState>| {
                state.planet_hover = Some(planet_id);
            })
            .observe(|_: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                state.planet_hover = None;
            })
            .observe(
                move |event: On<Pointer<Click>>,
                      mut state: ResMut<UiState>,
                      settings: Res<Settings>,
                      map: Res<Map>,
                      player: Res<Player>| {
                    let planet = map.get(planet_id);
                    if event.button == PointerButton::Primary {
                        select_planet(planet, &mut state, &player);
                    } else if event.button == PointerButton::Secondary && !planet.is_destroyed {
                        state.mission = true;
                        state.combat_report = None;
                        state.mission_tab = MissionTab::NewMission;
                        state.mission_info = Mission::from_mission(
                            settings.turn,
                            player.id,
                            map.get(
                                state
                                    .planet_selected
                                    .filter(|&p| player.controls(map.get(p)))
                                    .unwrap_or(player.home_planet),
                            ),
                            map.get(planet_id),
                            &state.mission_info,
                        );
                        state.planet_selected = None;
                    }
                },
            )
            .with_children(|parent| {
                parent.spawn((
                    Text2d::new(&planet.name),
                    TextFont {
                        font: assets.font("bold").into(),
                        font_size: TITLE_TEXT_SIZE.into(),
                        ..default()
                    },
                    TextColor(WHITE.into()),
                    Transform::from_xyz(0., planet.size() * 0.7, 0.9),
                    Pickable::IGNORE,
                    PlanetNameCmp,
                ));

                // Destroyed planets have no resources nor icons
                if !planet.is_destroyed {
                    for (i, icon) in Icon::iter().enumerate() {
                        parent
                            .spawn((
                                Sprite {
                                    image: assets.image(icon.to_lowername().as_str()),
                                    custom_size: Some(Vec2::splat(Icon::SIZE)),
                                    ..default()
                                },
                                Transform::from_translation(Vec3::new(
                                    planet.size() * 0.45,
                                    planet.size() * 0.4 - i as f32 * Icon::SIZE,
                                    0.8,
                                )),
                                Pickable::default(),
                                icon,
                            ))
                            .observe(cursor::<Over>(SystemCursorIcon::Pointer))
                            .observe(cursor::<Out>(SystemCursorIcon::Default))
                            .observe(
                                move |_: On<Pointer<Over>>,
                                      mut state: ResMut<UiState>,
                                      map: Res<Map>,
                                      missions: Res<Missions>| {
                                    state.planet_hover = Some(planet_id);
                                    state.mission_hover_from_ui = false;
                                    state.mission_hover = None;
                                    if let Some(mission) = missions
                                        .iter()
                                        .sorted_by(|a, b| {
                                            a.turns_to_destination(&map)
                                                .cmp(&b.turns_to_destination(&map))
                                        })
                                        .find(|m| {
                                            m.destination == planet_id
                                                && (m.objective == icon || icon == Icon::Attacked)
                                        })
                                    {
                                        state.mission_hover = Some(mission.id);
                                    }
                                },
                            )
                            .observe(|_: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                                state.planet_hover = None;
                                state.mission_hover = None;
                                state.mission_hover_from_ui = false;
                            })
                            .observe(
                                move |mut event: On<Pointer<Click>>,
                                      mut state: ResMut<UiState>,
                                      mut settings: ResMut<Settings>,
                                      map: Res<Map>,
                                      player: Res<Player>| {
                                    // Prevent the event from bubbling up to the planet
                                    event.propagate(false);

                                    if event.button == PointerButton::Primary {
                                        let planet = map.get(planet_id);
                                        if icon.on_units()
                                            && (player.owns(planet)
                                                || (player.controls(planet) && planet.is_moon()))
                                        {
                                            state.planet_selected = Some(planet_id);
                                            state.mission = false;
                                            settings.show_menu = true;
                                            if let Some(shop) = icon.shop() {
                                                state.shop = shop;
                                            }
                                        } else if icon == Icon::Attacked {
                                            state.mission = true;
                                            state.planet_selected = None;
                                            state.mission_tab = MissionTab::EnemyMissions;
                                        } else if icon.is_mission() {
                                            state.mission = true;
                                            state.planet_selected = None;
                                            state.mission_tab = MissionTab::NewMission;

                                            // The origin is determined as follows: the selected
                                            // planet if owned and fulfills condition, else the
                                            // first planet of the player that fulfills condition
                                            let origin_id = state
                                                .planet_selected
                                                .filter(|&id| {
                                                    id != planet_id && icon.condition(map.get(id))
                                                })
                                                .unwrap_or(
                                                    map.planets
                                                        .iter()
                                                        .find_map(|p| {
                                                            (p.id != planet_id
                                                                && player.controls(p)
                                                                && icon.condition(p))
                                                            .then_some(p.id)
                                                        })
                                                        .unwrap_or(player.home_planet),
                                                );

                                            let origin = map.get(origin_id);
                                            state.mission_info =
                                                Mission::new(
                                                    settings.turn,
                                                    player.id,
                                                    map.get(origin_id),
                                                    map.get(planet_id),
                                                    icon,
                                                    match icon {
                                                        Icon::Colonize => Army::from([(
                                                            Unit::Ship(Ship::ColonyShip),
                                                            1,
                                                        )]),
                                                        Icon::Spy => Army::from([(
                                                            Unit::probe(),
                                                            origin.army.amount(&Unit::probe()),
                                                        )]),
                                                        Icon::Attack | Icon::Destroy => origin
                                                            .army
                                                            .iter()
                                                            .filter_map(|(u, c)| {
                                                                (*c > 0 && u.is_combat_ship())
                                                                    .then_some((*u, *c))
                                                            })
                                                            .collect(),
                                                        Icon::MissileStrike => Army::from([(
                                                            Unit::interplanetary_missile(),
                                                            origin.army.amount(
                                                                &Unit::interplanetary_missile(),
                                                            ),
                                                        )]),
                                                        Icon::Deploy => origin
                                                            .army
                                                            .iter()
                                                            .filter_map(|(u, c)| {
                                                                (*c > 0 && u.is_ship())
                                                                    .then_some((*u, *c))
                                                            })
                                                            .collect(),
                                                        _ => Army::new(),
                                                    },
                                                    state.mission_info.bombing.clone(),
                                                    state.mission_info.combat_probes,
                                                    state.mission_info.jump_gate,
                                                    None,
                                                );
                                        }
                                    }
                                },
                            );
                    }

                    if !planet.is_moon() {
                        for (i, resource) in ResourceName::iter().enumerate() {
                            parent
                                .spawn((
                                    Sprite {
                                        image: assets.image(resource.to_lowername()),
                                        custom_size: Some(Vec2::new(
                                            planet.size() * 0.45,
                                            planet.size() * 0.3,
                                        )),
                                        ..default()
                                    },
                                    Transform {
                                        translation: Vec3::new(
                                            -planet.size() * 1.1,
                                            planet.size() * (0.27 - i as f32 * 0.25),
                                            0.7,
                                        ),
                                        scale: Vec3::splat(0.6),
                                        ..default()
                                    },
                                    Pickable::IGNORE,
                                    PlanetResourcesCmp,
                                ))
                                .with_children(|parent| {
                                    parent.spawn((
                                        Text2d::new(planet.resources.get(&resource).to_string()),
                                        TextFont {
                                            font: assets.font("bold").into(),
                                            font_size: 25.0.into(),
                                            ..default()
                                        },
                                        TextColor(WHITE.into()),
                                        Transform::from_xyz(55., 0., 0.8),
                                    ));
                                });
                        }
                    }

                    // Draw planetary shield
                    let material = materials.add(ColorMaterial {
                        color: OWN_COLOR.with_alpha(0.0),
                        // ColorMaterial::from an opaque color ignores the tween's alpha changes.
                        alpha_mode: bevy::sprite_render::AlphaMode2d::Blend,
                        ..default()
                    });
                    parent.spawn((
                        Mesh2d(
                            meshes.add(Annulus::new(planet.size() * 0.55, planet.size() * 0.57)),
                        ),
                        MeshMaterial2d(material.clone()),
                        Transform::from_xyz(0., 0., 0.6),
                        TweenAnim::new(PlanetaryShieldCmp::tween(OWN_COLOR)),
                        AnimTarget::asset(&material),
                        Visibility::Hidden,
                        PlanetaryShieldCmp::new(),
                    ));

                    // Draw space dock on random position around planet
                    let r = planet.size() * 0.5;
                    let angle = rng().random_range(0.0..TAU);

                    parent.spawn((
                        Sprite {
                            image: assets.image("dock"),
                            custom_size: Some(Vec2::splat(planet.size() * 0.4)),
                            ..default()
                        },
                        Transform::from_xyz(angle.cos() * r, angle.sin() * r, 0.7),
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::Linear,
                                Duration::from_secs(12),
                                TransformOrbitLens {
                                    radius: planet.size() * 0.75,
                                    offset: angle,
                                },
                            )
                            .with_repeat_count(RepeatCount::Infinite),
                        ),
                        Pickable::IGNORE,
                        Visibility::Hidden,
                        SpaceDockCmp,
                    ));

                    // Keep the gate opposite the dock on a wider, slower orbit. Its own spin
                    // makes it read as an active portal while avoiding the planet and UI icons.
                    let gate_angle = angle + PI;
                    let gate_radius = planet.size() * 0.9;
                    parent.spawn((
                        Sprite {
                            image: assets.image("jump gate marker"),
                            custom_size: Some(Vec2::splat(planet.size() * 0.36)),
                            ..default()
                        },
                        Transform::from_xyz(
                            gate_angle.cos() * gate_radius,
                            gate_angle.sin() * gate_radius,
                            0.72,
                        ),
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::Linear,
                                Duration::from_secs(17),
                                TransformOrbitSpinLens {
                                    radius: gate_radius,
                                    offset: gate_angle,
                                    rotations: -2.0,
                                },
                            )
                            .with_repeat_count(RepeatCount::Infinite),
                        ),
                        Pickable::IGNORE,
                        Visibility::Hidden,
                        JumpGateCmp,
                    ));

                    // Vertex alpha supplies the scanner's soft field, rim glow, and fading trails.
                    let scanner_material = materials.add(ColorMaterial {
                        color: Color::srgb(0.25, 0.95, 0.62),
                        ..default()
                    });
                    for (index, scanner) in ScannerCmp::layers().into_iter().enumerate() {
                        parent.spawn((
                            Mesh2d::default(),
                            MeshMaterial2d(scanner_material.clone()),
                            Transform::from_xyz(0., 0., -0.12 + index as f32 * 0.01),
                            Pickable::IGNORE,
                            Visibility::Hidden,
                            scanner,
                        ));
                    }
                }
            });

        if player.owns(planet) {
            // Place the camera on top of the player's home planet
            projection.scale = 0.8; // Increase zoom
            camera_t.translation = planet.position.extend(camera_t.translation.z);
        }
    }

    spawn_voronoi_cells(&mut commands, &map, &mut meshes, &mut materials);

    // Spawn end turn button
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(42.),
            right: Val::Px(270.),
            ..default()
        },
        Text::new("Waiting for other players to finish their turn..."),
        TextFont {
            font: assets.font("bold").into(),
            font_size: BUTTON_TEXT_SIZE.into(),
            ..default()
        },
        Visibility::Hidden,
        EndTurnLabelCmp,
        MapCmp,
    ));

    spawn_main_button(&mut commands, "End turn", &assets)
        .insert((EndTurnButtonCmp, MapCmp))
        .observe(|_: On<Pointer<Click>>, mut state: ResMut<UiState>| {
            state.planet_selected = None;
            state.mission = false;
            state.combat_report = None;
            state.end_turn = true;
        });

    // Spawn spectator mode label
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(30.),
            right: Val::Px(60.),
            ..default()
        },
        Text::new("Spectator Mode"),
        TextFont {
            font: assets.font("bold").into(),
            font_size: 30.0.into(),
            ..default()
        },
        TextColor(Color::WHITE),
        Visibility::Hidden,
        SpectatorLabelCmp,
        MapCmp,
    ));
}

/// Hides map details on menu entry, independently of selection and the show-info preference.
pub fn hide_planet_details(
    mut details: Query<
        &mut Visibility,
        Or<(With<PlanetNameCmp>, With<PlanetResourcesCmp>, With<Icon>, With<ScannerCmp>)>,
    >,
) {
    // Map presentation updates stop while a menu is open, so explicitly clear the last frame's
    // visible details instead of relying on a pointer-out event or normal display preferences.
    for mut visibility in &mut details {
        *visibility = Visibility::Hidden;
    }
}

/// Updates planet info from the current canonical ECS projection.
pub fn update_planet_info(
    mut planet_q: Query<(Entity, &mut Sprite, &PlanetCmp)>,
    mut icon_q: Query<(&mut Visibility, &mut Transform, &Icon)>,
    mut name_q: Query<
        &mut Visibility,
        (
            With<PlanetNameCmp>,
            Without<Icon>,
            Without<PlanetResourcesCmp>,
            Without<ScannerCmp>,
            Without<SpaceDockCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    mut resources_q: Query<
        &mut Visibility,
        (
            With<PlanetResourcesCmp>,
            Without<Icon>,
            Without<PlanetNameCmp>,
            Without<ScannerCmp>,
            Without<SpaceDockCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    mut scanner_q: Query<
        (&mut Visibility, &mut Mesh2d, &mut Transform, &mut ScannerCmp),
        (
            Without<Icon>,
            Without<PlanetNameCmp>,
            Without<PlanetResourcesCmp>,
            Without<SpaceDockCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    children_q: Query<&Children>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    state: Res<UiState>,
    settings: Res<Settings>,
    assets: Res<WorldAssets>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (n_owned, n_max_owned) = player.planets_owned(&map, &settings);

    for (planet_e, mut planet_s, planet_c) in &mut planet_q {
        let planet = map.get(planet_c.id);

        // Update destroyed planet image
        planet_s.image = assets.image(planet.image());

        let selected =
            state.planet_hover.or(state.planet_selected).map(|id| id == planet.id).unwrap_or(false);

        // Show/hide planet icons
        let mut count = 0;
        for child in children_q.iter_descendants(planet_e) {
            if let Ok((mut icon_v, mut icon_t, icon)) = icon_q.get_mut(child) {
                let visible = match icon {
                    Icon::Attacked => missions.iter().any(|m| {
                        player.owns(planet)
                            && m.objective != Icon::Deploy
                            && m.destination == planet.id
                    }),
                    Icon::Buildings => {
                        (player.owns(planet) || (player.controls(planet) && planet.is_moon()))
                            && (selected || icon.condition(planet) || settings.show_info)
                    },
                    Icon::Fleet => {
                        // Shows when having an army on a not-owned planet, but hides when hovered
                        player.controls(planet)
                            && if player.owns(planet) || planet.is_moon() {
                                selected || icon.condition(planet) || settings.show_info
                            } else {
                                icon.condition(planet) && !selected && !settings.show_info
                            }
                    },
                    Icon::Defenses => {
                        player.owns(planet)
                            && !planet.is_moon()
                            && (selected || icon.condition(planet) || settings.show_info)
                    },
                    _ => {
                        // Show icon if there is a mission with this objective towards this
                        // planet or, if there's selected planet, it fulfills the condition,
                        // else if any of the player's planets fulfills the condition
                        let has_mission = missions.iter().any(|m| {
                            m.owner == player.id
                                && m.objective == *icon
                                && m.destination == planet.id
                        });

                        let has_condition = {
                            map.planets.iter().any(|p| {
                                p.id != planet.id
                                    && icon.condition(p)
                                    && match icon {
                                        Icon::Deploy => {
                                            player.controls(p) && player.controls(planet)
                                        },
                                        Icon::Colonize => {
                                            player.controls(p)
                                                && !player.owns(planet)
                                                && !planet.is_moon()
                                                && n_owned < n_max_owned
                                        },
                                        Icon::MissileStrike => {
                                            player.controls(p)
                                                && !player.controls(planet)
                                                && !planet.is_moon()
                                        },
                                        _ => player.controls(p) && !player.controls(planet),
                                    }
                            })
                        };

                        has_mission || ((selected || settings.show_info) && has_condition)
                    },
                };

                *icon_v = if visible && !planet.is_destroyed {
                    icon_t.translation.y = planet.size() * 0.4 - count as f32 * Icon::SIZE;
                    count += 1;
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }

            // Show/hide planet resources and name
            if let Ok(mut visibility) = name_q.get_mut(child) {
                *visibility = if selected || settings.show_info {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
            if let Ok(mut visibility) = resources_q.get_mut(child) {
                *visibility = if (selected || settings.show_info) && !planet.is_destroyed {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }

            // Show/hide scanner indicator
            if let Ok((mut visibility, mut mesh, mut transform, mut scanner)) =
                scanner_q.get_mut(child)
            {
                // Range previews belong to map hover, using only the local player's scanners.
                let hovered = state.planet_hover == Some(planet.id);
                let mut radius = if hovered && !planet.is_moon() && player.owns(planet) {
                    PHALANX_DISTANCE
                        * Planet::SIZE
                        * planet.army.amount(&Unit::Building(Building::SensorPhalanx)) as f32
                } else if hovered && planet.is_moon() && player.controls(planet) {
                    RADAR_DISTANCE
                        * Planet::SIZE
                        * planet.army.amount(&Unit::Building(Building::OrbitalRadar)) as f32
                } else {
                    0.
                };

                if radius > 0. && !planet.is_destroyed {
                    radius += planet.size() * 0.5; // Start at the edge of the planet

                    *visibility = Visibility::Inherited;
                    scanner.update(
                        radius,
                        time.elapsed_secs_f64(),
                        &mut transform,
                        &mut mesh,
                        &mut meshes,
                    );
                } else {
                    *visibility = Visibility::Hidden;
                }
            }
        }
    }
}

/// Colors visible defenses from the same controller knowledge as territorial borders.
pub fn update_planet_defenses(
    planet_q: Query<(Entity, &PlanetCmp)>,
    children_q: Query<&Children>,
    mut ps_q: Query<(
        &mut Visibility,
        &mut TweenAnim,
        &mut PlanetaryShieldCmp,
        &MeshMaterial2d<ColorMaterial>,
    )>,
    mut dock_q: Query<
        (&mut Visibility, &mut Sprite),
        (With<SpaceDockCmp>, Without<JumpGateCmp>, Without<PlanetaryShieldCmp>),
    >,
    mut gate_q: Query<
        (&mut Visibility, &mut Sprite),
        (With<JumpGateCmp>, Without<SpaceDockCmp>, Without<PlanetaryShieldCmp>),
    >,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, planet_c) in &planet_q {
        let planet = map.get(planet_c.id);
        let controls = player.controls(planet);
        // Read intelligence once per planet. A hidden capture must not change its displayed color.
        let info = (!controls).then(|| player.last_info(planet, &missions.0)).flatten();
        let (army, controller) = if controls {
            (Some(&planet.army), planet.controlled)
        } else {
            (info.as_ref().map(|info| &info.army), info.as_ref().and_then(|info| info.controlled))
        };
        let has_ps = !planet.is_destroyed
            && army.is_some_and(|army| army.amount(&Unit::planetary_shield()) > 0);
        let has_dock =
            !planet.is_destroyed && army.is_some_and(|army| army.amount(&Unit::space_dock()) > 0);
        let has_gate = !planet.is_destroyed
            && army.is_some_and(|army| army.amount(&Unit::Building(Building::JumpGate)) > 0);
        // Defenses left on an unclaimed world have no player color.
        let color = controller
            .map(|id| session.player_color(id).color())
            .unwrap_or(Color::srgb_u8(190, 198, 210));

        for child in children_q.iter_descendants(entity) {
            if let Ok((mut visibility, mut tween, mut ps, material)) = ps_q.get_mut(child) {
                *visibility = if has_ps {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if has_ps && ps.color != Some(color) {
                    let mut pulse = PlanetaryShieldCmp::tween(color);
                    // Recolor the field without restarting its pulse midway through a fade.
                    pulse.set_elapsed(tween.tweenable().elapsed());
                    match tween.set_tweenable(pulse) {
                        Ok(_) => {
                            ps.color = Some(color);
                            if let Some(mut material) = materials.get_mut(&material.0) {
                                material.color = color.with_alpha(material.color.alpha());
                            }
                        },
                        Err(error) => warn!("Failed to update planetary shield color: {error}"),
                    }
                }
            }
            if let Ok((mut visibility, mut sprite)) = dock_q.get_mut(child) {
                *visibility = if has_dock {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if has_dock {
                    sprite.color = color;
                }
            }
            if let Ok((mut visibility, mut sprite)) = gate_q.get_mut(child) {
                *visibility = if has_gate {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if has_gate {
                    sprite.color = color;
                }
            }
        }
    }
}

/// Smoothly applies one territory cell or border's target color and visibility.
#[allow(clippy::too_many_arguments)]
fn update_territory_visual(
    visibility: &mut Visibility,
    material: &mut ColorMaterial,
    transition: &mut TerritoryTransitionCmp,
    base_color: Option<Color>,
    target_visible: bool,
    opacity: f32,
    show_cells: bool,
    animate: bool,
    delta_seconds: f32,
) {
    if !show_cells {
        *visibility = Visibility::Hidden;
        // Re-enabling the user's display preference should be immediate, not mistaken for a
        // gameplay ownership change that happened while borders were deliberately hidden.
        transition.initialized = false;
        return;
    }

    let target = base_color.map_or_else(
        || transition.target.with_alpha(0.0),
        |color| {
            color.with_alpha(if target_visible {
                opacity
            } else {
                0.0
            })
        },
    );

    if !transition.initialized {
        material.color = target;
        transition.target = target;
        transition.target_visible = target_visible;
        transition.start = target;
        transition.elapsed = TERRITORY_TRANSITION_SECONDS;
        transition.initialized = true;
        *visibility = if target_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        return;
    }

    if transition.target != target || transition.target_visible != target_visible {
        transition.start = material.color;
        transition.target = target;
        transition.target_visible = target_visible;
        transition.elapsed = 0.0;
    }

    if transition.elapsed < TERRITORY_TRANSITION_SECONDS {
        if animate {
            transition.elapsed =
                (transition.elapsed + delta_seconds).min(TERRITORY_TRANSITION_SECONDS);
        }
        let linear = (transition.elapsed / TERRITORY_TRANSITION_SECONDS).clamp(0.0, 1.0);
        let eased = linear * linear * (3.0 - 2.0 * linear);
        material.color = transition.start.mix(&transition.target, eased);
        *visibility = if transition.elapsed >= TERRITORY_TRANSITION_SECONDS && !target_visible {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    } else {
        material.color = transition.target;
        *visibility = if target_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Updates Voronoi ownership cells with smooth capture, loss, and recolor transitions.
pub fn update_voronoi(
    mut cell_q: Query<(
        &mut Visibility,
        &MeshMaterial2d<ColorMaterial>,
        &VoronoiCmp,
        &mut TerritoryTransitionCmp,
    )>,
    mut edge_q: Query<
        (
            &mut Visibility,
            &MeshMaterial2d<ColorMaterial>,
            &VoronoiEdgeCmp,
            &mut TerritoryTransitionCmp,
        ),
        Without<VoronoiCmp>,
    >,
    settings: Res<Settings>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    time: Option<Res<Time>>,
    game_state: Option<Res<State<GameState>>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let known_controllers = map
        .planets
        .iter()
        .filter_map(|planet| {
            let controller = if player.controls(planet) {
                planet.controlled
            } else {
                player.last_info(planet, &missions.0).and_then(|info| info.controlled)
            };
            controller.map(|controller| (planet.id, controller))
        })
        .collect::<HashMap<PlanetId, PlayerId>>();

    let animate = game_state.as_ref().is_none_or(|state| *state.get() == GameState::Playing);
    let delta_seconds =
        time.as_ref().map_or(TERRITORY_TRANSITION_SECONDS, |time| time.delta_secs());

    for (mut cell_v, cell_m, cell, mut transition) in &mut cell_q {
        let planet = map.get(cell.0);
        let controller = known_controllers.get(&planet.id).copied();
        let visible = !planet.is_destroyed && controller.is_some();
        let base_color = controller.map(|id| session.player_color(id).color());
        if let Some(mut material) = materials.get_mut(&cell_m.0) {
            update_territory_visual(
                &mut cell_v,
                &mut material,
                &mut transition,
                base_color,
                visible,
                0.01,
                settings.show_cells,
                animate,
                delta_seconds,
            );
        }
    }

    let mut counts_by_owner = HashMap::new();

    for (_, _, edge, _) in &edge_q {
        if let Some(&controller) = known_controllers.get(&edge.planet) {
            *counts_by_owner.entry((edge.key, controller)).or_default() += 1;
        }
    }

    for (mut edge_v, edge_m, edge, mut transition) in &mut edge_q {
        let controller = known_controllers.get(&edge.planet).copied();
        let visible = controller.is_some_and(|controller| {
            !map.get(edge.planet).is_destroyed
                && *counts_by_owner.get(&(edge.key, controller)).unwrap_or(&2) <= 1
        });
        let base_color = controller.map(|id| session.player_color(id).color());
        if let Some(mut material) = materials.get_mut(&edge_m.0) {
            update_territory_visual(
                &mut edge_v,
                &mut material,
                &mut transition,
                base_color,
                visible,
                0.58,
                settings.show_cells,
                animate,
                delta_seconds,
            );
        }
    }
}

/// Updates end turn from the current canonical ECS projection.
pub fn update_end_turn(
    mut button_c: Query<&mut Visibility, With<EndTurnButtonCmp>>,
    mut spectator_q: Query<&mut Visibility, (With<SpectatorLabelCmp>, Without<EndTurnButtonCmp>)>,
    mut button_q: Query<&mut Text, With<MainButtonLabelCmp>>,
    mut label_q: Query<
        &mut Visibility,
        (With<EndTurnLabelCmp>, Without<SpectatorLabelCmp>, Without<EndTurnButtonCmp>),
    >,
    game_state: Res<State<GameState>>,
    pending: Res<crate::multiplayer::client::PendingTurnCommands>,
    player: Res<Player>,
) {
    let playing = *game_state.get() == GameState::Playing;
    for mut button_v in &mut button_c {
        *button_v = if playing && !player.spectator {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for mut label_v in &mut spectator_q {
        *label_v = if player.spectator && playing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if playing {
        for mut button_t in &mut button_q {
            button_t.0 = pending.button_label().to_string();
        }
    }

    for mut label_v in &mut label_q {
        *label_v = if playing
            && !pending.resume_requested
            && matches!(
                pending.submission,
                crate::multiplayer::client::SubmissionState::Sending
                    | crate::multiplayer::client::SubmissionState::Accepted
                    | crate::multiplayer::client::SubmissionState::Retry
            )
            && !player.spectator
        {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

fn wrap_around(value: f32, center: f32, span: f32) -> f32 {
    center + (value - center + span * 0.5).rem_euclid(span) - span * 0.5
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn looping_frame_alpha(frame: usize, elapsed: f32, frame_count: usize, frame_seconds: f32) -> f32 {
    let (current, next, blend) = looping_frame_sample(elapsed, frame_count, frame_seconds);
    if frame == current {
        1.0 - blend
    } else if frame == next {
        blend
    } else {
        0.0
    }
}

fn looping_frame_sample(
    elapsed: f32,
    frame_count: usize,
    frame_seconds: f32,
) -> (usize, usize, f32) {
    let phase = (elapsed / frame_seconds).rem_euclid(frame_count as f32);
    let current = phase.floor() as usize;
    (current, (current + 1) % frame_count, smoothstep(phase.fract()))
}

fn solar_star_frame_alpha(frame: usize, elapsed: f32) -> f32 {
    looping_frame_alpha(frame, elapsed, SOLAR_STAR_FRAME_COUNT, SOLAR_STAR_FRAME_SECONDS)
}

fn black_hole_frame_state(slot: usize, elapsed: f32) -> (usize, f32) {
    let (current, next, blend) =
        looping_frame_sample(elapsed, BLACK_HOLE_FRAME_COUNT, BLACK_HOLE_FRAME_SECONDS);
    if slot == 0 {
        (current, (1.0 - blend) * BLACK_HOLE_OPACITY)
    } else {
        (next, blend * BLACK_HOLE_OPACITY)
    }
}

fn comet_visibility(progress: f32) -> f32 {
    let fade_in = smoothstep(progress / 0.12);
    let fade_out = smoothstep((1.0 - progress) / 0.34);
    fade_in * fade_out
}

fn pulsar_visibility(progress: f32) -> f32 {
    let fade_in = smoothstep(progress / 0.08);
    let fade_out = smoothstep((0.34 - progress) / 0.12);
    fade_in * fade_out
}

fn pulsar_anchor(seed: u32, cycle: u32) -> Vec2 {
    let cycle_seed = seed.wrapping_add(cycle.wrapping_mul(0x9e37_79b9));
    Vec2::new(
        (visual_noise(cycle_seed.wrapping_add(1)) - 0.5) * AMBIENT_PULSAR_FIELD_SIZE.x,
        (visual_noise(cycle_seed.wrapping_add(2)) - 0.5) * AMBIENT_PULSAR_FIELD_SIZE.y,
    )
}

/// Gives each sourced landmark a restrained presentation-only drift, pulse, or rotation.
pub(crate) fn animate_space_scenery(
    mut star_q: Query<
        &mut Transform,
        (With<SolarStarCmp>, Without<SolarStarFrameCmp>, Without<NebulaCmp>, Without<BlackHoleCmp>),
    >,
    mut solar_frame_q: Query<
        (&SolarStarFrameCmp, &mut Sprite),
        (Without<SolarStarCmp>, Without<BlackHoleFrameCmp>, Without<MainCamera>),
    >,
    mut nebula_q: Query<
        (&NebulaCmp, &mut Transform),
        (Without<SolarStarCmp>, Without<BlackHoleCmp>),
    >,
    black_hole_q: Query<(&BlackHoleCmp, &Children)>,
    mut black_hole_frame_q: Query<
        (&BlackHoleFrameCmp, &mut Sprite),
        (Without<SolarStarFrameCmp>, Without<MainCamera>),
    >,
    time: Res<Time>,
) {
    let elapsed = time.elapsed_secs_f64() as f32;
    for mut transform in &mut star_q {
        transform.rotation = Quat::from_rotation_z(elapsed * 0.018);
        transform.scale = Vec3::splat(1.0 + (elapsed * 0.72).sin() * 0.012);
    }
    for (frame, mut sprite) in &mut solar_frame_q {
        sprite.color.set_alpha(solar_star_frame_alpha(frame.index, elapsed));
    }
    for (nebula, mut transform) in &mut nebula_q {
        let phase = elapsed * 0.025 + nebula.phase;
        transform.rotation = Quat::from_rotation_z((phase * 0.41).sin() * 0.018);
        transform.scale = Vec3::splat(1.0 + (phase * 0.62).sin() * 0.018);
    }
    for (black_hole, children) in &black_hole_q {
        for child in children.iter() {
            let Ok((frame, mut sprite)) = black_hole_frame_q.get_mut(child) else {
                continue;
            };
            let (frame_index, alpha) = black_hole_frame_state(frame.slot, elapsed);
            sprite.image = black_hole.frames[frame_index].clone();
            sprite.color.set_alpha(alpha);
        }
    }
}

fn next_comet_delay(sequence: u32) -> f32 {
    if visual_noise(sequence.wrapping_add(0x713)) > 0.9 {
        1.4 + visual_noise(sequence.wrapping_add(0x919)) * 1.8
    } else {
        10.0 + visual_noise(sequence.wrapping_add(0xb53)) * 17.0
    }
}

fn spawn_ambient_comet(
    commands: &mut Commands,
    camera: &Transform,
    projection: &OrthographicProjection,
    sequence: u32,
) {
    let view_size = (projection.area.max - projection.area.min).max(Vec2::new(800.0, 450.0));
    let horizontal = if visual_noise(sequence.wrapping_add(1)) < 0.5 {
        1.0
    } else {
        -1.0
    };
    let direction =
        Vec2::new(horizontal, (visual_noise(sequence.wrapping_add(2)) - 0.5) * 0.65).normalize();
    let length = 90.0 + visual_noise(sequence.wrapping_add(3)) * 110.0;
    let thickness = 0.42 + visual_noise(sequence.wrapping_add(4)) * 0.38;
    let lifetime = 1.35 + visual_noise(sequence.wrapping_add(5)) * 0.65;
    let travel_distance = view_size.x * (0.44 + visual_noise(sequence.wrapping_add(9)) * 0.12);
    let speed = travel_distance / lifetime;
    let camera_position = camera.translation.truncate();
    let start = camera_position - direction * travel_distance * 0.5
        + Vec2::Y * (visual_noise(sequence.wrapping_add(6)) - 0.5) * view_size.y * 0.62;
    let tint = if visual_noise(sequence.wrapping_add(7)) > 0.84 {
        Color::srgb(1.0, 0.78, 0.54)
    } else {
        Color::srgb(0.66, 0.84, 1.0)
    };

    commands
        .spawn((
            Transform {
                translation: start.extend(BACKGROUND_Z + 0.72),
                rotation: Quat::from_rotation_z(direction.y.atan2(direction.x)),
                ..default()
            },
            Visibility::Inherited,
            Pickable::IGNORE,
            AmbientCometCmp {
                age: 0.0,
                lifetime,
                velocity: direction * speed,
                peak_alpha: 0.38 + visual_noise(sequence.wrapping_add(8)) * 0.16,
            },
            MapCmp,
        ))
        .with_children(|parent| {
            for (part_length, part_thickness, offset, depth, alpha_factor) in [
                (length, thickness * 1.6, -length * 0.5, 0.0, 0.18),
                (length * 0.72, thickness * 0.58, -length * 0.36, 0.01, 1.0),
            ] {
                parent.spawn((
                    Sprite::from_color(
                        tint.with_alpha(0.0),
                        Vec2::new(part_length, part_thickness),
                    ),
                    Transform::from_xyz(offset, 0.0, depth),
                    Pickable::IGNORE,
                    AmbientCometPartCmp {
                        alpha_factor,
                    },
                ));
            }
        });
}

/// Spawns and advances occasional comet streaks behind the strategic projection.
pub(crate) fn update_ambient_comets(
    mut commands: Commands,
    camera_q: Single<(&Transform, &Projection), (With<MainCamera>, Without<AmbientCometCmp>)>,
    mut spawner: ResMut<AmbientCometSpawner>,
    mut comet_q: Query<
        (Entity, &mut AmbientCometCmp, &mut Transform, &Children),
        Without<MainCamera>,
    >,
    mut part_q: Query<(&AmbientCometPartCmp, &mut Sprite)>,
    time: Res<Time>,
) {
    let (camera, projection) = camera_q.into_inner();
    let delta = time.delta_secs();
    for (entity, mut comet, mut transform, children) in &mut comet_q {
        comet.age += delta;
        let progress = comet.age / comet.lifetime;
        if progress >= 1.0 {
            commands.entity(entity).despawn();
            continue;
        }

        transform.translation += (comet.velocity * delta).extend(0.0);
        let visibility = comet_visibility(progress);
        for child in children.iter() {
            if let Ok((part, mut sprite)) = part_q.get_mut(child) {
                sprite.color.set_alpha(comet.peak_alpha * part.alpha_factor * visibility);
            }
        }
    }

    spawner.remaining -= delta;
    if spawner.remaining > 0.0 {
        return;
    }
    let Projection::Orthographic(projection) = projection else {
        return;
    };
    spawn_ambient_comet(&mut commands, camera, projection, spawner.sequence);
    spawner.sequence = spawner.sequence.wrapping_add(1);
    spawner.remaining = next_comet_delay(spawner.sequence);
}

/// Animates presentation-only depth cues without changing canonical world positions.
pub(crate) fn animate_map_ambience(
    camera_q: Single<
        &Transform,
        (
            With<MainCamera>,
            Without<ParallaxCmp>,
            Without<AmbientStarCmp>,
            Without<AmbientPulsarCmp>,
            Without<PlanetCmp>,
        ),
    >,
    parallax_q: Query<
        &Transform,
        (
            With<ParallaxCmp>,
            Without<MainCamera>,
            Without<AmbientStarCmp>,
            Without<AmbientPulsarCmp>,
            Without<PlanetCmp>,
        ),
    >,
    mut star_q: Query<
        (&AmbientStarCmp, &ChildOf, &mut Sprite, &mut Transform),
        (Without<MainCamera>, Without<ParallaxCmp>, Without<AmbientPulsarCmp>, Without<PlanetCmp>),
    >,
    mut pulsar_q: Query<
        (&AmbientPulsarCmp, &ChildOf, &mut Sprite, &mut Transform, &Children),
        (Without<MainCamera>, Without<ParallaxCmp>, Without<AmbientStarCmp>, Without<PlanetCmp>),
    >,
    mut pulsar_ray_q: Query<
        (&AmbientPulsarRayCmp, &mut Sprite),
        (Without<AmbientPulsarCmp>, Without<AmbientStarCmp>, Without<PlanetCmp>),
    >,
    mut planet_q: Query<
        (&PlanetAmbienceCmp, &mut Sprite),
        (
            With<PlanetCmp>,
            Without<AmbientStarCmp>,
            Without<AmbientPulsarCmp>,
            Without<MainCamera>,
            Without<ParallaxCmp>,
        ),
    >,
    time: Res<Time>,
) {
    let elapsed = time.elapsed_secs_f64() as f32;
    let camera_position = camera_q.translation.truncate();

    for (star, parent, mut sprite, mut transform) in &mut star_q {
        let Ok(layer_transform) = parallax_q.get(parent.parent()) else {
            continue;
        };
        let layer_scale = layer_transform.scale.x.max(f32::EPSILON);
        let local_center = (camera_position - layer_transform.translation.truncate()) / layer_scale;
        transform.translation.x =
            wrap_around(star.anchor.x, local_center.x, AMBIENT_STAR_FIELD_SIZE.x);
        transform.translation.y =
            wrap_around(star.anchor.y, local_center.y, AMBIENT_STAR_FIELD_SIZE.y);

        let pulse = (0.5 + 0.5 * (elapsed * star.speed + star.phase).sin()).powf(star.pulse_power);
        sprite
            .color
            .set_alpha(star.base_alpha * (star.minimum_alpha + (1.0 - star.minimum_alpha) * pulse));
        transform.scale = Vec3::splat(0.82 + 0.3 * pulse);
    }

    for (pulsar, parent, mut sprite, mut transform, children) in &mut pulsar_q {
        let Ok(layer_transform) = parallax_q.get(parent.parent()) else {
            continue;
        };
        let layer_scale = layer_transform.scale.x.max(f32::EPSILON);
        let local_center = (camera_position - layer_transform.translation.truncate()) / layer_scale;
        let cycle_position = elapsed / pulsar.cycle_duration + pulsar.phase;
        let cycle = cycle_position.floor() as u32;
        let progress = cycle_position.fract();
        let anchor = pulsar_anchor(pulsar.seed, cycle);
        transform.translation.x =
            wrap_around(anchor.x, local_center.x, AMBIENT_PULSAR_FIELD_SIZE.x);
        transform.translation.y =
            wrap_around(anchor.y, local_center.y, AMBIENT_PULSAR_FIELD_SIZE.y);

        let visibility = pulsar_visibility(progress);
        sprite.color.set_alpha(pulsar.peak_alpha * visibility);
        transform.scale = Vec3::splat(0.72 + visibility * 0.58);
        transform.rotation = Quat::from_rotation_z(elapsed * 0.035 + pulsar.phase * TAU);
        for child in children.iter() {
            if let Ok((ray, mut ray_sprite)) = pulsar_ray_q.get_mut(child) {
                ray_sprite.color.set_alpha(pulsar.peak_alpha * ray.alpha_factor * visibility);
            }
        }
    }

    for (ambience, mut sprite) in &mut planet_q {
        let pulse = 0.5 + 0.5 * (elapsed * 0.45 + ambience.phase).sin();
        let brightness = ambience.minimum_brightness + (1.0 - ambience.minimum_brightness) * pulse;
        sprite.color = Color::srgba(brightness, brightness, brightness, 1.0);
    }
}

/// Advances map animations effects for the current frame.
pub fn run_map_animations(
    mut commands: Commands,
    mut animation_q: Query<(Entity, &mut Sprite, &mut ExplosionCmp)>,
    mut map: ResMut<Map>,
    time: Res<Time>,
) {
    for (animation_e, mut sprite, mut animation) in &mut animation_q {
        animation.timer.tick(time.delta());

        let planet = map.get_mut(animation.planet);

        if animation.timer.just_finished() {
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index += 1;

                // Change planet's image at a third of the animation
                if atlas.index == animation.last_index / 3 {
                    planet.image = 0;
                } else if atlas.index == animation.last_index {
                    commands.entity(animation_e).despawn();
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/core/map_systems.rs"]
mod tests;
