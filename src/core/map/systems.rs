//! Bevy systems that render and animate the strategic map projection.

pub use super::scanner::ScannerCmp;

use std::collections::{BTreeMap, HashMap};
use std::f32::consts::{PI, TAU};
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::color::{palettes::css::WHITE, Mix};
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_tweening::lens::SpriteColorLens;
use bevy_tweening::{EaseMethod, RepeatCount, Tween, TweenAnim, Tweenable};
use itertools::Itertools;
use rand::{rng, RngExt};
use strum::IntoEnumIterator;
use voronator::delaunator::Point;
use voronator::VoronoiDiagram;

use crate::core::assets::{WorldAssets, ASTEROID_IMAGE_NAMES};
use crate::core::camera::{drag_camera_position, MainCamera, ParallaxCmp};
use crate::core::constants::{
    BACKGROUND_Z, BUTTON_TEXT_SIZE, HOME_CROWN_INDICES, HOME_CROWN_VERTICES, HOME_PLANET_COLOR,
    MISSION_Z, ORBITAL_RAILGUN_RANGE_PER_LEVEL, OWN_COLOR, PHALANX_DISTANCE, PLANET_Z,
    RADAR_DISTANCE, SOLAR_STAR_SIZE, TITLE_TEXT_SIZE, VORONOI_Z,
};
use crate::core::identity::PlayerId;
use crate::core::loading::{PublicStructure, PublicStructureChange};
use crate::core::map::details::{DevelopmentVisibility, DEBRIS_DEPTH, DEVELOPMENT_MAX_SCALE};
use crate::core::map::detection::PublicStructureEffect;
use crate::core::map::icon::Icon;
use crate::core::map::model::{Map, MapCmp};
use crate::core::map::orbital_railgun::{OrbitalRailgunFiring, OrbitalStrikeEffect};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::scenery::CelestialKind;
use crate::core::map::utils::{
    cursor, spawn_main_button, MainButtonLabelCmp, TransformOrbitLens, TransformSpinLens,
};
use crate::core::missions::{Mission, MissionId, Missions};
use crate::core::player::Player;
use crate::core::recycling::{debris_sites, recycler_sources, DebrisSite, RecyclerSource};
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::simulation::{orbital_railgun_origins, TurnCommand};
use crate::core::states::GameState;
use crate::core::ui::systems::{MapRangePreview, MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Unit};
use crate::multiplayer::client::{MultiplayerSession, PendingTurnCommands};
use crate::utils::NameFromEnum;

const MAP_ICON_IDLE_TINT: Color = Color::srgb(0.82, 0.82, 0.82);
const PHALANX_DRONE_COUNT: usize = 3;
const PHALANX_DRONE_CYCLE_SECONDS: f32 = 11.0;
const RANGE_MARKER_CYCLE_SECONDS: f32 = 7.0;
const SOLAR_SATELLITE_PHASE_STEPS: [usize; Building::MAX_LEVEL] = [0, 2, 4, 1, 3];
const JUMP_GATE_LINK_SPACING: f32 = 26.0;
const JUMP_GATE_LINK_SPEED: f32 = 38.0;
const JUMP_GATE_LINK_STRANDS: usize = 2;
// These are local to the planet entity, whose own transform contributes PLANET_Z. Keep the whole
// stack below MISSION_Z, including the mission exhaust at MISSION_Z - 0.1.
const PLANETARY_SHIELD_DEPTH: f32 = 0.05;
const SOLAR_SATELLITE_DEPTH: f32 = 0.10;
const SOLAR_SATELLITE_DEPTH_STEP: f32 = 0.001;
// Recyclers pass over their salvage target, so render them above debris while leaving them
// non-pickable. Pointer hits then continue to the debris interaction target underneath.
const RECYCLER_DEPTH: f32 = DEBRIS_DEPTH + 0.01;
const RECYCLER_DEPTH_STEP: f32 = 0.001;
const RECYCLER_SCAN_DEPTH: f32 = DEBRIS_DEPTH + 0.009;
const RECYCLER_CYCLE_SECONDS: f32 = 6.0;
const RECYCLER_PHASE_OFFSETS: [f32; Building::MAX_LEVEL] = [0.0, 0.43, 0.78, 0.21, 0.62];
const RECYCLER_HOME_RADIUS: f32 = 0.70;
const RECYCLER_HOME_ANGLE_OFFSETS: [f32; Building::MAX_LEVEL] = [0.0, -0.22, 0.22, -0.44, 0.44];
const RECYCLER_TARGET_OFFSETS: [usize; Building::MAX_LEVEL] = [0, 2, 1, 3, 0];
const JUMP_GATE_DEPTH: f32 = 0.15;
const COMMAND_RELAY_DEPTH: f32 = 0.16;
const SENSOR_PHALANX_DEPTH: f32 = 0.17;
const SENSOR_PHALANX_DEPTH_RANGE: f32 = 0.035;
const ORBITAL_RAILGUN_DEPTH: f32 = 0.22;
const ORBITAL_RAILGUN_ANGLE: f32 = PI * 0.5;
const ORBITAL_RAILGUN_RADIUS: f32 = 1.2;
const SPACE_DOCK_DEPTH: f32 = 0.25;
const PLANETARY_SHIELD_MAX_ALPHA: f32 = 0.85;
const PLANETARY_SHIELD_PULSE_PEAK: Duration = Duration::from_millis(1_500);
const PLANETARY_SHIELD_OVERLOAD_ROTATION_SECONDS: f32 = 8.0;
const PLANETARY_SHIELD_OVERLOAD_SPIN_RAMP_SECONDS: f32 = 0.65;

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

#[derive(SystemParam)]
/// Shared canonical resources used while projecting planet information onto the map.
pub struct PlanetInfoResources<'w, 's> {
    map: Res<'w, Map>,
    player: Res<'w, Player>,
    session: Res<'w, MultiplayerSession>,
    pending: Option<Res<'w, PendingTurnCommands>>,
    missions: Res<'w, Missions>,
    structure_effects: Query<'w, 's, &'static PublicStructureEffect>,
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
/// Sourced animated landmark; only stellar kinds use proximity ambience.
pub(crate) struct CelestialCmp {
    pub(crate) kind: CelestialKind,
    frames: Vec<Handle<Image>>,
}

#[derive(Component)]
/// One of the two sprites used to crossfade between sourced NASA animation frames.
pub(crate) struct CelestialFrameCmp {
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

#[derive(Component)]
/// Marker for the map's single decorative asteroid belt.
pub(crate) struct AsteroidBeltCmp;

#[derive(Component)]
/// One decorative, non-authoritative rock tumbling along a solar-band boundary.
pub(crate) struct AsteroidCmp {
    center: Vec2,
    radius: f32,
    phase: f32,
    angular_speed: f32,
    wobble_phase: f32,
    wobble_speed: f32,
    wobble_amplitude: f32,
    spin: f32,
    tumble_phase: f32,
    tumble_speed: f32,
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
const SOLAR_STAR_DEPTH: f32 = BACKGROUND_Z + 0.78;
const NEBULA_SIZE: Vec2 = Vec2::new(1_900.0, 1_566.0);
const NEBULA_DEPTH: f32 = BACKGROUND_Z + 0.1;
const NEBULA_PARALLAX_FOLLOW: f32 = 0.9;
const CELESTIAL_SIZE: Vec2 = Vec2::new(480.0, 270.0);
const CELESTIAL_DEPTH: f32 = VORONOI_Z + 0.15;
const CELESTIAL_MAP_MARGIN: f32 = 32.0;
const CELESTIAL_TINT: f32 = 0.92;

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
/// Crown attached to the local home planet's name, inheriting its visibility.
pub(crate) struct HomeCrownCmp;

/// Places the crown before the measured name after font loading or text changes.
pub(crate) fn position_home_crown(
    names: Query<&bevy::text::TextLayoutInfo, With<PlanetNameCmp>>,
    mut crowns: Query<(&ChildOf, &mut Transform), With<HomeCrownCmp>>,
) {
    for (parent, mut transform) in &mut crowns {
        if let Ok(layout) = names.get(parent.parent()) {
            transform.translation.x = -layout.size.x * 0.5 - TITLE_TEXT_SIZE * 0.7;
        }
    }
}

#[derive(Component)]
/// Bevy component marking planet resources presentation entities.
pub struct PlanetResourcesCmp;

/// Tracks the displayed owner's color and smooth overload rotation state.
#[derive(Component, Default)]
pub struct PlanetaryShieldCmp {
    color: Option<Color>,
    overloaded: bool,
    spin_factor: f32,
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
            SpriteColorLens {
                start: color.with_alpha(0.0),
                end: color.with_alpha(PLANETARY_SHIELD_MAX_ALPHA),
            },
        )
        .with_repeat_count(RepeatCount::Infinite)
    }
}

fn approach(current: f32, target: f32, max_delta: f32) -> f32 {
    if current < target {
        (current + max_delta).min(target)
    } else {
        (current - max_delta).max(target)
    }
}

#[derive(Component)]
/// Bevy component marking space dock presentation entities.
pub struct SpaceDockCmp;

#[derive(Component)]
/// Public, faction-tinted Orbital Railgun presentation entity.
pub struct OrbitalRailgunCmp {
    pub(crate) planet: PlanetId,
    pub(crate) anchor: Vec2,
    pub(crate) base_rotation: f32,
    pub(crate) phase: f32,
}

#[derive(Component)]
/// Animated, faction-tinted jump-gate marker orbiting a world.
pub struct JumpGateCmp {
    planet: PlanetId,
}

/// Returns whether this world contributes one usable endpoint to the player's gate network.
fn is_usable_owned_jump_gate(planet: &Planet, player: &Player) -> bool {
    player.owns(planet)
        && !planet.is_destroyed
        && !planet.is_moon()
        && planet.has(&Unit::Building(Building::JumpGate))
}

/// A higher gate level adds capacity, but only a second distinct gate activates the shortcut.
fn jump_gate_network_available(map: &Map, player: &Player) -> bool {
    map.planets.iter().filter(|planet| is_usable_owned_jump_gate(planet, player)).take(2).count()
        >= 2
}

fn jump_gate_shortcut_destination(
    map: &Map,
    player: &Player,
    origin: PlanetId,
    preferred: PlanetId,
) -> Option<PlanetId> {
    let is_destination =
        |planet: &&Planet| planet.id != origin && is_usable_owned_jump_gate(planet, player);
    map.planets
        .iter()
        .find(|planet| planet.id == preferred && is_destination(planet))
        .or_else(|| map.planets.iter().find(is_destination))
        .map(|planet| planet.id)
}

/// Opens a safe, unsent Deploy draft between two owned gates with jump travel selected.
fn open_jump_gate_mission(
    state: &mut UiState,
    settings: &Settings,
    map: &Map,
    player: &Player,
    origin: PlanetId,
) -> bool {
    let Some(origin_planet) = map.planets.iter().find(|planet| planet.id == origin) else {
        return false;
    };
    if !is_usable_owned_jump_gate(origin_planet, player) {
        return false;
    }
    let Some(destination) =
        jump_gate_shortcut_destination(map, player, origin, state.mission_info.destination)
    else {
        return false;
    };

    state.mission_info = Mission::new(
        settings.turn,
        player.id,
        origin_planet,
        map.get(destination),
        Icon::Deploy,
        Army::new(),
        default(),
        false,
        true,
        None,
    );
    state.mission = true;
    state.mission_tab = MissionTab::NewMission;
    state.planet_selected = None;
    state.combat_report = None;
    state.mission_report = None;
    state.mission_hover = None;
    state.mission_hover_from_ui = false;
    state.jump_gate_hover = None;
    state.jump_gate_history = true;
    true
}

#[derive(Component)]
/// One particle in the hover-only animated network between owned Jump Gates.
pub(crate) struct JumpGateLinkCmp {
    index: usize,
}

#[derive(Component, Debug)]
/// One animated solar-satellite marker in a world's level-based orbital network.
pub struct SolarSatelliteCmp {
    level: usize,
}

#[derive(Component, Debug)]
/// One small Recycler craft shuttling between its planet and the best available salvage source.
pub struct RecyclerCmp {
    planet: PlanetId,
    level: usize,
    phase: f32,
    gate_angle: f32,
}

#[derive(Component, Debug)]
/// Expanding scan ring shown while a Recycler inspects its current target.
pub struct RecyclerScanCmp {
    planet: PlanetId,
    recycler_level: usize,
    pulse_offset: f32,
}

#[derive(Default)]
pub(crate) struct RecyclerAnimationCache {
    scenery_seed: Option<u32>,
    turn: usize,
    debris: BTreeMap<PlanetId, DebrisSite>,
    sources: BTreeMap<PlanetId, RecyclerSource>,
    asteroid_targets: BTreeMap<PlanetId, Vec<Vec2>>,
}

fn recycler_smoothstep(amount: f32) -> f32 {
    let amount = amount.clamp(0.0, 1.0);
    amount * amount * (3.0 - 2.0 * amount)
}

fn recycler_angular_delta(from: f32, to: f32) -> f32 {
    (to - from + PI).rem_euclid(TAU) - PI
}

fn recycler_avoid_angle(angle: f32, reserved: f32, clearance: f32, level: usize) -> f32 {
    let delta = recycler_angular_delta(reserved, angle);
    if delta.abs() >= clearance {
        angle
    } else {
        let side = if delta.abs() > 0.001 {
            delta.signum()
        } else if level.is_multiple_of(2) {
            -1.0
        } else {
            1.0
        };
        reserved + side * clearance
    }
}

fn recycler_home(source_anchor: Vec2, recycler: &RecyclerCmp, planet: &Planet) -> Vec2 {
    let destination_angle = (source_anchor - planet.position).to_angle();
    let mut angle = destination_angle + RECYCLER_HOME_ANGLE_OFFSETS[recycler.level - 1];
    // Recheck the short list because moving away from one station can approach its neighbor.
    for _ in 0..2 {
        for (reserved, clearance) in [
            (ORBITAL_RAILGUN_ANGLE, 0.20),
            (PI * 0.75, 0.38),
            (PI * 1.25, 0.45),
            (recycler.gate_angle, 0.40),
        ] {
            angle = recycler_avoid_angle(angle, reserved, clearance, recycler.level);
        }
    }
    Vec2::from_angle(angle) * planet.size() * RECYCLER_HOME_RADIUS
}

fn recycler_progress(cycle: f32) -> f32 {
    if cycle < 0.30 {
        recycler_smoothstep(cycle / 0.30)
    } else if cycle < 0.56 {
        1.0
    } else if cycle < 0.86 {
        1.0 - recycler_smoothstep((cycle - 0.56) / 0.30)
    } else {
        0.0
    }
}

fn recycler_route_position(home: Vec2, target: Vec2, progress: f32) -> Vec2 {
    let angle =
        home.to_angle() + recycler_angular_delta(home.to_angle(), target.to_angle()) * progress;
    let radius = home.length() + (target.length() - home.length()) * progress;
    Vec2::from_angle(angle) * radius
}

fn recycler_heading(outbound: Vec2, next_outbound: Vec2, cycle: f32) -> f32 {
    let outbound_angle = outbound.to_angle();
    if cycle < 0.43 {
        outbound_angle
    } else if cycle < 0.56 {
        outbound_angle + recycler_smoothstep((cycle - 0.43) / 0.13) * PI
    } else if cycle < 0.86 {
        outbound_angle + PI
    } else {
        let homeward_angle = outbound_angle + PI;
        homeward_angle
            + recycler_angular_delta(homeward_angle, next_outbound.to_angle())
                * recycler_smoothstep((cycle - 0.86) / 0.14)
    }
}

fn recycler_animation_target(
    source: RecyclerSource,
    asteroid_targets: Option<&[Vec2]>,
    recycler: &RecyclerCmp,
    planet: &Planet,
    trip: u64,
) -> Vec2 {
    match source {
        RecyclerSource::AsteroidField {
            target,
        } => asteroid_targets
            .filter(|targets| !targets.is_empty())
            .map(|targets| {
                let trip = (trip % targets.len() as u64) as usize;
                targets[(trip + RECYCLER_TARGET_OFFSETS[recycler.level - 1]) % targets.len()]
            })
            .unwrap_or(target),
        RecyclerSource::Debris {
            target,
            ..
        } => {
            let index = (recycler.level - 1 + (trip % Building::MAX_LEVEL as u64) as usize)
                % Building::MAX_LEVEL;
            let side = if index.is_multiple_of(2) {
                -1.0
            } else {
                1.0
            };
            let lane = (index / 2 + 1) as f32;
            target
                + Vec2::new(
                    (lane - 1.0) * planet.size() * -0.035,
                    side * lane * planet.size() * 0.055,
                )
        },
    }
}

/// Animates known Recycler craft, preferring fresh local debris over a reachable asteroid field.
#[allow(clippy::too_many_arguments)]
pub(crate) fn animate_recyclers(
    time: Res<Time>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    settings: Res<Settings>,
    camera: Single<&Projection, With<MainCamera>>,
    mut cache: Local<RecyclerAnimationCache>,
    mut recyclers: Query<
        (&RecyclerCmp, &mut Transform, &mut Visibility, &mut Sprite),
        Without<RecyclerScanCmp>,
    >,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut scans: Query<
        (&RecyclerScanCmp, &MeshMaterial2d<ColorMaterial>, &mut Transform, &mut Visibility),
        Without<RecyclerCmp>,
    >,
) {
    let sites = if let Some(record) = &session.active_game {
        debris_sites(
            record.persisted.state.players.iter().flat_map(|player| player.reports.iter()),
            settings.turn,
        )
    } else {
        debris_sites(player.reports.iter(), settings.turn)
    };
    let scenery_seed = map.scenery_seed();
    if cache.scenery_seed != Some(scenery_seed)
        || cache.turn != settings.turn
        || cache.debris != sites
    {
        cache.scenery_seed = Some(scenery_seed);
        cache.turn = settings.turn;
        cache.sources = recycler_sources(&map, &sites);
        cache.asteroid_targets = crate::core::map::asteroids::recycler_asteroid_target_groups(&map);
        cache.debris = sites;
    }
    let elapsed = time.elapsed_secs();
    let close_zoom = matches!(
        *camera,
        Projection::Orthographic(ref projection) if projection.scale <= DEVELOPMENT_MAX_SCALE
    );

    for (recycler, mut transform, mut visibility, mut sprite) in &mut recyclers {
        let planet = map.get(recycler.planet);
        let info =
            (!player.controls(planet)).then(|| player.last_info(planet, &missions.0)).flatten();
        let (army, controller) = if player.controls(planet) {
            (Some(&planet.army), planet.controlled.or(planet.owned))
        } else {
            (info.as_ref().map(|info| &info.army), info.as_ref().and_then(|info| info.controlled))
        };
        let level = army.map_or(0, |army| {
            army.amount(&Unit::Building(Building::Recycler)).min(Building::MAX_LEVEL)
        });
        let source = cache.sources.get(&planet.id).copied();
        let active = close_zoom && recycler.level <= level && source.is_some();
        *visibility = if active {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        let player_color = controller
            .map(|id| session.player_color(id).color())
            .unwrap_or(Color::srgb_u8(190, 198, 210));
        let cycle_time = elapsed / RECYCLER_CYCLE_SECONDS + recycler.phase;
        let trip = cycle_time.floor() as u64;
        let cycle = cycle_time.fract();
        let scanning = active && (0.30..0.56).contains(&cycle);
        for (scan, material, mut scan_transform, mut scan_visibility) in &mut scans {
            if scan.planet != recycler.planet || scan.recycler_level != recycler.level {
                continue;
            }
            *scan_visibility = if scanning {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            };
            if let (true, Some(source)) = (scanning, source) {
                let target = recycler_animation_target(
                    source,
                    cache.asteroid_targets.get(&planet.id).map(Vec::as_slice),
                    recycler,
                    planet,
                    trip,
                ) - planet.position;
                let pulse = (((cycle - 0.30) / 0.26) + scan.pulse_offset).fract();
                scan_transform.translation = target.extend(
                    RECYCLER_SCAN_DEPTH + (recycler.level - 1) as f32 * RECYCLER_DEPTH_STEP,
                );
                scan_transform.scale = Vec3::splat(5.0 + pulse * (14.0 + level as f32));
                scan_transform.rotation = Quat::from_rotation_z(elapsed * 0.8);
                if let Some(mut material) = materials.get_mut(&material.0) {
                    material.color = player_color.with_alpha((1.0 - pulse) * 0.58);
                }
            }
        }
        let Some(source) = source.filter(|_| recycler.level <= level) else {
            continue;
        };

        sprite.color = player_color;
        let target_world = recycler_animation_target(
            source,
            cache.asteroid_targets.get(&planet.id).map(Vec::as_slice),
            recycler,
            planet,
            trip,
        );
        let next_target_world = recycler_animation_target(
            source,
            cache.asteroid_targets.get(&planet.id).map(Vec::as_slice),
            recycler,
            planet,
            trip.saturating_add(1),
        );
        let source_anchor = match source {
            RecyclerSource::Debris {
                target,
                ..
            }
            | RecyclerSource::AsteroidField {
                target,
            } => target,
        };
        let target = target_world - planet.position;
        let next_target = next_target_world - planet.position;
        let home = recycler_home(source_anchor, recycler, planet);
        let position = recycler_route_position(home, target, recycler_progress(cycle));
        let tangent = Vec2::new(-position.y, position.x).normalize_or_zero();
        let position = position + tangent * (elapsed * 2.2 + recycler.phase * TAU).sin() * 1.4;
        transform.translation =
            position.extend(RECYCLER_DEPTH + (recycler.level - 1) as f32 * RECYCLER_DEPTH_STEP);
        let outbound = target - home;
        let scan_sway = if scanning {
            ((cycle - 0.30) / 0.26 * TAU * 2.0).sin() * 0.12
        } else {
            0.0
        };
        transform.rotation = Quat::from_rotation_z(
            recycler_heading(outbound, next_target - home, cycle) + scan_sway,
        );
        transform.scale = Vec3::ONE;
    }
}

#[derive(Component)]
/// Stationary, faction-tinted command-relay range marker beside a world.
pub struct CommandRelayCmp;

#[derive(Component, Debug)]
/// One of three faction-tinted Sensor Phalanx drones in a pseudo-orbital formation.
pub struct SensorPhalanxCmp {
    index: usize,
    anchor: Vec2,
    phase: f32,
}

#[derive(Component, Debug)]
/// Subtle local motion around a fixed range-marker anchor.
pub(crate) struct RangeMarkerMotionCmp {
    anchor: Vec2,
    elapsed: f32,
    phase: f32,
}

fn range_marker_pose(anchor: Vec2, elapsed: f32, phase: f32) -> (Vec2, f32) {
    let wave = elapsed / RANGE_MARKER_CYCLE_SECONDS * TAU + phase;
    // Integer harmonics make position, velocity, and tilt meet seamlessly when elapsed wraps.
    let offset = Vec2::new((wave * 2.0).sin() * 1.1, wave.sin() * 2.4);
    let tilt = (wave * 3.0).sin() * 0.05;
    (anchor + offset, tilt)
}

/// Gives fixed Relay markers a restrained idle drift.
pub(crate) fn animate_range_markers(
    time: Res<Time>,
    mut markers: Query<(&mut Transform, &mut RangeMarkerMotionCmp, &Visibility)>,
) {
    for (mut transform, mut motion, visibility) in &mut markers {
        if *visibility == Visibility::Hidden {
            continue;
        }
        motion.elapsed =
            (motion.elapsed + time.delta_secs()).rem_euclid(RANGE_MARKER_CYCLE_SECONDS);
        let (position, tilt) = range_marker_pose(motion.anchor, motion.elapsed, motion.phase);
        transform.translation.x = position.x;
        transform.translation.y = position.y;
        transform.rotation = Quat::from_rotation_z(tilt);
    }
}

/// Gives the public Railgun the same restrained free-floating drift as other fixed markers.
pub(crate) fn animate_orbital_railguns(
    mut commands: Commands,
    time: Res<Time>,
    mut railguns: Query<(
        Entity,
        &OrbitalRailgunCmp,
        &mut Transform,
        &Visibility,
        Option<&mut OrbitalRailgunFiring>,
    )>,
) {
    let elapsed = time.elapsed_secs();
    for (entity, railgun, mut transform, visibility, firing) in &mut railguns {
        if *visibility == Visibility::Hidden {
            continue;
        }
        let (position, tilt) = range_marker_pose(railgun.anchor, elapsed, railgun.phase);
        let idle_rotation = railgun.base_rotation + tilt;
        let (position, rotation, finished) = if let Some(mut firing) = firing {
            firing.elapsed += time.delta_secs();
            firing.pose(position, idle_rotation)
        } else {
            (position, idle_rotation, false)
        };
        transform.translation = position.extend(ORBITAL_RAILGUN_DEPTH);
        transform.rotation = Quat::from_rotation_z(rotation);
        transform.scale = Vec3::ONE;
        if finished {
            commands.entity(entity).remove::<OrbitalRailgunFiring>();
        }
    }
}

fn phalanx_drone_pose(drone: &SensorPhalanxCmp, elapsed: f32) -> Transform {
    let wave = elapsed / PHALANX_DRONE_CYCLE_SECONDS * TAU;
    let phase = drone.phase + drone.index as f32 * TAU / PHALANX_DRONE_COUNT as f32;
    // Different integer harmonics close into one seamless loop without ever reading as a flat
    // circular orbit. Apparent depth also changes scale and draw order as the drones cross.
    let x = (wave * 2.0 + phase).sin() * 8.5 + (wave * 5.0 - phase * 0.7).sin() * 2.2;
    let y = (wave * 3.0 + phase * 1.3).sin() * 5.8 + (wave * 4.0 - phase * 0.5).cos() * 1.8;
    let depth = 0.5 + 0.5 * (wave + phase).sin();
    let tilt = (wave * 3.0 + phase).sin() * 0.10;
    Transform {
        translation: (drone.anchor + Vec2::new(x, y))
            .extend(SENSOR_PHALANX_DEPTH + depth * SENSOR_PHALANX_DEPTH_RANGE),
        rotation: Quat::from_rotation_z(tilt),
        scale: Vec3::splat(0.76 + depth * 0.28),
    }
}

/// Moves each visible Phalanx drone on its own looping pseudo-orbit around the formation.
pub(crate) fn animate_phalanx_drones(
    time: Res<Time>,
    mut drones: Query<(&mut Transform, &SensorPhalanxCmp, &Visibility)>,
) {
    for (mut transform, drone, visibility) in &mut drones {
        if *visibility != Visibility::Hidden {
            *transform = phalanx_drone_pose(drone, time.elapsed_secs());
        }
    }
}

#[derive(Clone)]
struct JumpGateLinkParticle {
    transform: Transform,
    color: Color,
    size: Vec2,
}

fn jump_gate_link_particles(
    from: Vec2,
    to: Vec2,
    color: Color,
    elapsed: f32,
    route_phase: f32,
) -> Vec<JumpGateLinkParticle> {
    let route = to - from;
    let route_length = route.length();
    if route_length <= 32.0 {
        return Vec::new();
    }
    let direction = route / route_length;
    let normal = Vec2::new(-direction.y, direction.x);
    let clearance = 16.0;
    let length = route_length - clearance * 2.0;
    let start = from + direction * clearance;
    let count = (length / JUMP_GATE_LINK_SPACING).ceil() as usize;
    let travel = (elapsed * JUMP_GATE_LINK_SPEED).rem_euclid(JUMP_GATE_LINK_SPACING);
    let point = |t: f32, strand: usize| {
        let t = t.clamp(0.0, 1.0);
        let envelope = (PI * t).sin();
        let strand_phase = strand as f32 * PI;
        let wave = t * TAU * 2.25 + elapsed * 3.1 + route_phase + strand_phase;
        start
            + direction * (t * length)
            + normal * ((wave.sin() * 9.0 + (wave * 2.0 + route_phase).sin() * 1.8) * envelope)
    };

    (0..JUMP_GATE_LINK_STRANDS)
        .flat_map(|strand| {
            (0..count).filter_map(move |index| {
                let distance = index as f32 * JUMP_GATE_LINK_SPACING + travel;
                (distance < length).then(|| {
                    let t = distance / length;
                    let position = point(t, strand);
                    let ahead = point((t + 0.01).min(1.0), strand);
                    let tangent = (ahead - position).normalize_or(direction);
                    let edge_fade = (t.min(1.0 - t) / 0.08).clamp(0.0, 1.0);
                    let pulse = 0.62 + 0.38 * (elapsed * 5.0 + index as f32 * 0.8).sin().abs();
                    let strand_color = if strand == 0 {
                        color.mix(&Color::srgb(0.25, 0.92, 1.0), 0.55)
                    } else {
                        Color::WHITE.mix(&color, 0.72)
                    };
                    JumpGateLinkParticle {
                        transform: Transform {
                            translation: position.extend(MISSION_Z - 0.18),
                            rotation: Quat::from_rotation_z(tangent.y.atan2(tangent.x)),
                            ..default()
                        },
                        color: strand_color.with_alpha(edge_fade * pulse * 0.82),
                        size: Vec2::new(13.0 + pulse * 5.0, 2.0 + pulse * 1.4),
                    }
                })
            })
        })
        .collect()
}

/// Draws braided, flowing energy strands from a hovered owned gate to every other owned gate.
pub(crate) fn update_jump_gate_links(
    mut commands: Commands,
    mut link_q: Query<
        (Entity, &mut Transform, &mut Sprite, &JumpGateLinkCmp),
        Without<JumpGateCmp>,
    >,
    gate_q: Query<(&Transform, &JumpGateCmp), Without<JumpGateLinkCmp>>,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    session: Res<MultiplayerSession>,
    time: Res<Time>,
) {
    let mut gates = gate_q
        .iter()
        .filter_map(|(transform, gate)| {
            let planet = map.get(gate.planet);
            is_usable_owned_jump_gate(planet, &player)
                .then_some((gate.planet, planet.position + transform.translation.truncate()))
        })
        .collect::<Vec<_>>();
    gates.sort_by_key(|(planet, _)| *planet);

    let particles = state
        .jump_gate_hover
        .and_then(|hovered| {
            let from = gates.iter().find(|(planet, _)| *planet == hovered)?.1;
            Some(
                gates
                    .iter()
                    .filter(|(planet, _)| *planet != hovered)
                    .flat_map(|(planet, to)| {
                        jump_gate_link_particles(
                            from,
                            *to,
                            session.player_color(player.id).color(),
                            time.elapsed_secs(),
                            visual_noise(hovered as u32 ^ (*planet as u32).rotate_left(13)) * TAU,
                        )
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .unwrap_or_default();
    let mut present = vec![false; particles.len()];

    for (entity, mut transform, mut sprite, link) in &mut link_q {
        let Some(particle) = particles.get(link.index) else {
            commands.entity(entity).despawn();
            continue;
        };
        present[link.index] = true;
        *transform = particle.transform;
        sprite.color = particle.color;
        sprite.custom_size = Some(particle.size);
    }
    for (index, particle) in particles.into_iter().enumerate() {
        if present[index] {
            continue;
        }
        commands.spawn((
            Sprite {
                color: particle.color,
                custom_size: Some(particle.size),
                ..default()
            },
            particle.transform,
            Pickable::IGNORE,
            JumpGateLinkCmp {
                index,
            },
            MapCmp,
        ));
    }
}

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

#[derive(Clone, Copy, Debug)]
struct AsteroidBeltGap {
    between_bands: usize,
    inner_center: f32,
    outer_center: f32,
    inner_edge: f32,
    outer_edge: f32,
}

#[derive(Clone, Copy, Debug)]
struct AsteroidBeltLayout {
    between_bands: usize,
    radius: f32,
    radial_half_width: f32,
    maximum_asteroid_diameter: f32,
}

#[derive(Clone, Copy, Debug)]
struct AsteroidBeltPlacement {
    seed: u32,
    phase: f32,
    radius: f32,
    diameter: f32,
}

const ASTEROID_PLANET_CLEARANCE: f32 = 12.0;
const ASTEROID_BELT_RADIUS_SAMPLES: usize = 12;
const ASTEROID_BELT_TARGET_SPACING: f32 = 34.0;
const ASTEROID_BELT_MINIMUM_COUNT: usize = 96;
const ASTEROID_BELT_MAXIMUM_COUNT: usize = 420;
const ASTEROID_BELT_MINIMUM_VISIBLE_COUNT: usize = 32;
const ASTEROID_BELT_VISIBLE_SUBDIVISIONS: usize = 8;
const ASTEROID_BELT_DEPTH: f32 = VORONOI_Z + 0.2;
const ASTEROID_MINIMUM_WOBBLE: f32 = 4.0;
const ASTEROID_WOBBLE_RANGE: f32 = 6.0;
const ASTEROID_MAXIMUM_WOBBLE: f32 = ASTEROID_MINIMUM_WOBBLE + ASTEROID_WOBBLE_RANGE;

fn solar_band_gaps(map: &Map) -> Vec<AsteroidBeltGap> {
    let star = map.solar_star_position();
    let mut bands = [Vec::new(), Vec::new(), Vec::new()];
    for planet in map.planets() {
        let index = match map.solar_band(planet.id) {
            Some(crate::core::map::planet::SolarBand::Inner) => 0,
            Some(crate::core::map::planet::SolarBand::Temperate) => 1,
            Some(crate::core::map::planet::SolarBand::Outer) => 2,
            None => continue,
        };
        bands[index].push((planet.position.distance(star), planet.size() * 0.5));
    }
    [(0, 1), (1, 2)]
        .into_iter()
        .enumerate()
        .filter_map(|(between_bands, (near, far))| {
            let inner_center =
                bands[near].iter().map(|(distance, _)| *distance).reduce(f32::max)?;
            let outer_center = bands[far].iter().map(|(distance, _)| *distance).reduce(f32::min)?;
            let inner_edge =
                bands[near].iter().map(|(distance, radius)| distance + radius).reduce(f32::max)?;
            let outer_edge =
                bands[far].iter().map(|(distance, radius)| distance - radius).reduce(f32::min)?;
            (inner_center < outer_center).then_some(AsteroidBeltGap {
                between_bands,
                inner_center,
                outer_center,
                inner_edge,
                outer_edge,
            })
        })
        .collect()
}

fn asteroid_belt_layout_in_gap(gap: AsteroidBeltGap, radial_position: f32) -> AsteroidBeltLayout {
    let surface_width = gap.outer_edge - gap.inner_edge;
    let (safe_inner, safe_outer, maximum_asteroid_radius) = if surface_width > 0.0 {
        let maximum_asteroid_radius = 19.0_f32.min(surface_width * 0.2);
        let planet_padding = 8.0_f32.min(surface_width * 0.08);
        (
            gap.inner_edge + maximum_asteroid_radius + planet_padding,
            gap.outer_edge - maximum_asteroid_radius - planet_padding,
            maximum_asteroid_radius,
        )
    } else {
        let center_width = gap.outer_center - gap.inner_center;
        (gap.inner_center + center_width * 0.12, gap.outer_center - center_width * 0.12, 19.0)
    };
    let safe_width = safe_outer - safe_inner;
    let radial_half_width = (safe_width * 0.32).min(50.0);
    let minimum_radius = safe_inner + radial_half_width;
    let maximum_radius = safe_outer - radial_half_width;
    AsteroidBeltLayout {
        between_bands: gap.between_bands,
        radius: minimum_radius + (maximum_radius - minimum_radius) * radial_position,
        radial_half_width,
        maximum_asteroid_diameter: maximum_asteroid_radius * 2.0,
    }
}

fn asteroid_is_visible_on_map(map: &Map, placement: &AsteroidBeltPlacement) -> bool {
    let center = map.solar_star_position();
    let position = center + Vec2::from_angle(placement.phase) * placement.radius;
    let visibility_margin = placement.diameter * 0.5 + ASTEROID_MAXIMUM_WOBBLE;
    let clear_of_star = placement.radius - visibility_margin > SOLAR_STAR_SIZE * 0.5;
    let fully_on_map = position.x - visibility_margin >= map.rect.min.x
        && position.x + visibility_margin <= map.rect.max.x
        && position.y - visibility_margin >= map.rect.min.y
        && position.y + visibility_margin <= map.rect.max.y;
    clear_of_star && fully_on_map
}

fn visible_asteroid_count(map: &Map, placements: &[AsteroidBeltPlacement]) -> usize {
    placements.iter().filter(|placement| asteroid_is_visible_on_map(map, placement)).count()
}

/// Selects a complete belt in either the inner/temperate or temperate/outer gap. Sampling each
/// safe radial span keeps the layout seed-varied while preferring the arc with the most rocks
/// inside the playable map instead of choosing a ring that is mostly beyond its corner.
fn asteroid_belt_layout(map: &Map) -> Option<AsteroidBeltLayout> {
    let seed = map.scenery_seed().wrapping_add(0x7a21_6d4b);
    let gaps = solar_band_gaps(map);
    let clear_gaps =
        gaps.iter().copied().filter(|gap| gap.inner_edge < gap.outer_edge).collect::<Vec<_>>();
    let candidates = if clear_gaps.is_empty() {
        gaps
    } else {
        clear_gaps
    };
    let radial_offset = visual_noise(seed.wrapping_add(1));
    candidates
        .into_iter()
        .flat_map(|gap| {
            (0..ASTEROID_BELT_RADIUS_SAMPLES).map(move |sample| {
                let radial_position =
                    (radial_offset + sample as f32 / ASTEROID_BELT_RADIUS_SAMPLES as f32).fract();
                asteroid_belt_layout_in_gap(gap, radial_position)
            })
        })
        .enumerate()
        .max_by_key(|(candidate, layout)| {
            let placements = asteroid_belt_base_placements(map, *layout);
            let complete = placements.len() == asteroid_belt_asteroid_count(layout.radius);
            (
                visible_asteroid_count(map, &placements),
                complete,
                visual_noise(seed.wrapping_add(*candidate as u32 + 2)).to_bits(),
            )
        })
        .map(|(_, layout)| layout)
}

fn asteroid_belt_asteroid_count(radius: f32) -> usize {
    ((TAU * radius / ASTEROID_BELT_TARGET_SPACING).round() as usize)
        .clamp(ASTEROID_BELT_MINIMUM_COUNT, ASTEROID_BELT_MAXIMUM_COUNT)
}

struct AsteroidBeltPlacementGenerator {
    center: Vec2,
    planets: Vec<(Vec2, f32)>,
    layout: AsteroidBeltLayout,
    count: usize,
    belt_seed: u32,
    starting_angle: f32,
    search_increment: f32,
}

impl AsteroidBeltPlacementGenerator {
    fn new(map: &Map, layout: AsteroidBeltLayout) -> Self {
        let count = asteroid_belt_asteroid_count(layout.radius);
        let belt_seed = map
            .scenery_seed()
            .wrapping_add(0x2c91_7a4d)
            .wrapping_add((layout.between_bands as u32).wrapping_mul(0x51d7_34ab));
        Self {
            center: map.solar_star_position(),
            planets: map
                .planets()
                .into_iter()
                .map(|planet| (planet.position, planet.size() * 0.5))
                .collect(),
            layout,
            count,
            belt_seed,
            starting_angle: visual_noise(belt_seed) * TAU,
            search_increment: TAU / count as f32 * 0.25,
        }
    }

    fn placement(&self, sequence: usize, angular_index: f32) -> Option<AsteroidBeltPlacement> {
        let seed = self.belt_seed.wrapping_add((sequence as u32).wrapping_mul(31));
        let base_phase = self.starting_angle
            + angular_index / self.count as f32 * TAU
            + (visual_noise(seed) - 0.5) * TAU / self.count as f32 * 0.7;
        let radius = self.layout.radius
            + (visual_noise(seed.wrapping_add(1)) - 0.5) * self.layout.radial_half_width * 2.0;
        let minimum_diameter = 18.0_f32.min(self.layout.maximum_asteroid_diameter);
        let diameter = minimum_diameter
            + visual_noise(seed.wrapping_add(2))
                * (self.layout.maximum_asteroid_diameter - minimum_diameter);
        let phase = (0..=self.count * 4).find_map(|search| {
            let offset = match search {
                0 => 0.0,
                value if value % 2 == 1 => (value / 2 + 1) as f32,
                value => -((value / 2) as f32),
            };
            let phase = base_phase + offset * self.search_increment;
            let position = self.center + Vec2::from_angle(phase) * radius;
            self.planets
                .iter()
                .all(|(planet_position, planet_radius)| {
                    position.distance(*planet_position)
                        > planet_radius
                            + diameter * 0.5
                            + ASTEROID_PLANET_CLEARANCE
                            + ASTEROID_MAXIMUM_WOBBLE
                })
                .then_some(phase)
        })?;
        Some(AsteroidBeltPlacement {
            seed,
            phase,
            radius,
            diameter,
        })
    }

    fn base_placements(&self) -> Vec<AsteroidBeltPlacement> {
        (0..self.count).filter_map(|index| self.placement(index, index as f32)).collect()
    }
}

fn asteroid_belt_base_placements(
    map: &Map,
    layout: AsteroidBeltLayout,
) -> Vec<AsteroidBeltPlacement> {
    AsteroidBeltPlacementGenerator::new(map, layout).base_placements()
}

fn asteroid_belt_placements(map: &Map, layout: AsteroidBeltLayout) -> Vec<AsteroidBeltPlacement> {
    let generator = AsteroidBeltPlacementGenerator::new(map, layout);
    let mut placements = generator.base_placements();
    let mut visible = visible_asteroid_count(map, &placements);
    if visible >= ASTEROID_BELT_MINIMUM_VISIBLE_COUNT {
        return placements;
    }

    // Keep portions beyond the playable map sparse, and only subdivide its on-map arc. This
    // guarantees a readable belt without multiplying sprite cost across the whole orbit.
    'supplements: for subdivision in 1..ASTEROID_BELT_VISIBLE_SUBDIVISIONS {
        for index in 0..generator.count {
            let sequence = generator.count + (subdivision - 1) * generator.count + index;
            let angular_index =
                index as f32 + subdivision as f32 / ASTEROID_BELT_VISIBLE_SUBDIVISIONS as f32;
            let Some(placement) = generator.placement(sequence, angular_index) else {
                continue;
            };
            if asteroid_is_visible_on_map(map, &placement) {
                placements.push(placement);
                visible += 1;
                if visible >= ASTEROID_BELT_MINIMUM_VISIBLE_COUNT {
                    break 'supplements;
                }
            }
        }
    }
    placements
}

fn spawn_asteroid_belt(commands: &mut Commands, map: &Map, images: &[Handle<Image>]) {
    if images.is_empty() {
        return;
    }
    let Some(layout) = asteroid_belt_layout(map) else {
        return;
    };
    let center = map.solar_star_position();
    let placements = asteroid_belt_placements(map, layout);
    commands.spawn((Name::new("Asteroid belt"), AsteroidBeltCmp, MapCmp));
    // Keep renderable rocks as world roots so their visibility never depends on the non-rendering
    // lifecycle marker. This also lets projection repair replace either side independently.
    for (index, placement) in placements.into_iter().enumerate() {
        let position = center + Vec2::from_angle(placement.phase) * placement.radius;
        commands.spawn((
            Sprite {
                image: images[index % images.len()].clone(),
                custom_size: Some(Vec2::splat(placement.diameter)),
                // Keep the decorative belt visually behind interactive planets and overlays.
                color: Color::srgba(0.78, 0.76, 0.72, 0.82),
                ..default()
            },
            Transform {
                // Keep the belt legible over territory shading while planets and all
                // interactive overlays remain in front of it.
                translation: position.extend(ASTEROID_BELT_DEPTH),
                rotation: Quat::from_rotation_z(visual_noise(placement.seed.wrapping_add(3)) * TAU),
                ..default()
            },
            Pickable::IGNORE,
            AsteroidCmp {
                center,
                radius: placement.radius,
                phase: placement.phase,
                angular_speed: 0.0,
                wobble_phase: visual_noise(placement.seed.wrapping_add(6)) * TAU,
                wobble_speed: 0.35 + visual_noise(placement.seed.wrapping_add(7)) * 0.5,
                wobble_amplitude: ASTEROID_MINIMUM_WOBBLE
                    + visual_noise(placement.seed.wrapping_add(8)) * ASTEROID_WOBBLE_RANGE,
                spin: (0.1 + visual_noise(placement.seed.wrapping_add(9)) * 0.22)
                    * if visual_noise(placement.seed.wrapping_add(10)) < 0.5 {
                        -1.0
                    } else {
                        1.0
                    },
                tumble_phase: visual_noise(placement.seed.wrapping_add(11)) * TAU,
                tumble_speed: 0.55 + visual_noise(placement.seed.wrapping_add(12)) * 0.75,
            },
            MapCmp,
        ));
    }
}

/// Repairs a missing decorative belt after an in-place multiplayer projection refresh.
pub(crate) fn ensure_asteroid_belt(
    mut commands: Commands,
    map: Res<Map>,
    assets: Res<WorldAssets>,
    belts: Query<Entity, With<AsteroidBeltCmp>>,
    asteroids: Query<Entity, With<AsteroidCmp>>,
) {
    if belts.iter().count() == 1 && !asteroids.is_empty() {
        return;
    }
    for entity in belts.iter().chain(asteroids.iter()) {
        commands.entity(entity).try_despawn();
    }
    let images = ASTEROID_IMAGE_NAMES.iter().map(|name| assets.image(name)).collect::<Vec<_>>();
    spawn_asteroid_belt(&mut commands, &map, &images);
}

/// Only fixed coordinates contribute: conquest, destruction and economy cannot reroll scenery.
fn map_scenery_seed(map: &Map) -> u32 {
    map.scenery_seed()
}

fn map_scenery_corner(map: &Map) -> Vec2 {
    map.solar_corner()
}

/// One compact landmark accompanies the large solar arc in every game.
fn map_scenery_selection(map: &Map) -> CelestialKind {
    let seed = map_scenery_seed(map);
    match (seed >> 8) % 3 {
        0 => CelestialKind::NeutronStar,
        1 => CelestialKind::Magnetar,
        _ => CelestialKind::BlackHole,
    }
}

#[cfg(test)]
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
    map.solar_star_position()
}

fn celestial_position(map: &Map) -> Vec2 {
    let sun_corner = map_scenery_corner(map);
    let edge_direction = Vec2::new(-sun_corner.x, 0.0);
    let edge_x = if edge_direction.x < 0.0 {
        map.rect.min.x
    } else {
        map.rect.max.x
    };
    // Keep the complete landmark on the map opposite the large sun. Very small synthetic maps
    // cannot contain the full sprite, so clamp their center to the inner tenth of the bounds.
    let horizontal_inset =
        (CELESTIAL_SIZE.x * 0.5 + CELESTIAL_MAP_MARGIN).min(map.rect.half_size().x * 0.9);
    let x = edge_x - edge_direction.x * horizontal_inset;
    let usable_half_height =
        (map.rect.half_size().y - CELESTIAL_SIZE.y * 0.5 - CELESTIAL_MAP_MARGIN).max(0.0);
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
    let kind = map_scenery_selection(map);
    spawn_solar_star(commands, assets, map);
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

    let celestial_anchor = celestial_position(map);
    let celestial_frames = (1..=kind.frame_count())
        .map(|index| assets.image(format!("{} {index}", kind.name())))
        .collect::<Vec<_>>();
    commands
        .spawn((
            Name::new(format!("Decorative {} map edge", kind.name())),
            Transform::from_xyz(0.0, 0.0, CELESTIAL_DEPTH),
            Visibility::Inherited,
            Pickable::IGNORE,
            MapCmp,
        ))
        .with_children(|parent| {
            parent
                .spawn((
                    Name::new(format!("Animated {}", kind.name())),
                    Transform::from_translation(celestial_anchor.extend(0.0)),
                    Visibility::Inherited,
                    Pickable::IGNORE,
                    CelestialCmp {
                        kind,
                        frames: celestial_frames,
                    },
                ))
                .with_children(|celestial| {
                    for slot in 0..2 {
                        let (frame, alpha) = celestial_frame_state(kind, slot, 0.0);
                        celestial.spawn((
                            Sprite {
                                image: assets.image(format!("{} {}", kind.name(), frame + 1)),
                                color: Color::srgba(
                                    CELESTIAL_TINT,
                                    CELESTIAL_TINT,
                                    CELESTIAL_TINT,
                                    alpha,
                                ),
                                custom_size: Some(CELESTIAL_SIZE * kind.size_scale()),
                                ..default()
                            },
                            Transform::from_xyz(0.0, 0.0, slot as f32 * 0.001),
                            Pickable::IGNORE,
                            CelestialFrameCmp {
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

/// Selects a planet without moving the camera and updates controlled worlds' mission origin.
pub(crate) fn select_planet(planet: &Planet, state: &mut UiState, player: &Player) {
    state.planet_selected = Some(planet.id);
    state.focus_planet = None;
    state.focus_zoom = None;
    state.to_selected = false;
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
                    state.focus_zoom = None;
                    state.to_selected = false;
                    commands.entity(*window_e).insert(CursorIcon::from(SystemCursorIcon::Grabbing));
                }
            },
        )
        .observe(cursor::<Release>(SystemCursorIcon::Default))
        .observe(
            |event: On<Pointer<Move>>,
             camera_q: Single<(&mut Transform, &Projection), With<MainCamera>>,
             map: Res<Map>,
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
                        let movement = Vec2::new(-event.delta.x, event.delta.y) * projection.scale;
                        camera_t.translation = drag_camera_position(
                            camera_t.translation.truncate(),
                            movement,
                            projection.area.size(),
                            &map,
                        )
                        .extend(camera_t.translation.z);
                        state.to_selected = false;
                        state.focus_planet = None;
                        state.focus_zoom = None;
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
    let asteroid_images =
        ASTEROID_IMAGE_NAMES.iter().map(|name| assets.image(name)).collect::<Vec<_>>();
    spawn_asteroid_belt(&mut commands, &map, &asteroid_images);
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
                      mut settings: ResMut<Settings>,
                      map: Res<Map>,
                      player: Res<Player>| {
                    let planet = map.get(planet_id);
                    if event.button == PointerButton::Primary {
                        select_planet(planet, &mut state, &player);
                        if player.owns(planet) || (planet.is_moon() && player.controls(planet)) {
                            settings.show_menu = true;
                        }
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
                let mut name = parent.spawn((
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
                if planet.id == player.home_planet {
                    name.with_children(|label| {
                        label.spawn((
                            Mesh2d(
                                meshes.add(
                                    Mesh::new(
                                        PrimitiveTopology::TriangleList,
                                        RenderAssetUsages::default(),
                                    )
                                    .with_inserted_attribute(
                                        Mesh::ATTRIBUTE_POSITION,
                                        HOME_CROWN_VERTICES
                                            .map(|[x, y]| {
                                                [
                                                    (x - 0.5) * TITLE_TEXT_SIZE,
                                                    (y - 0.5) * TITLE_TEXT_SIZE * 0.8,
                                                    0.0,
                                                ]
                                            })
                                            .to_vec(),
                                    )
                                    .with_inserted_indices(
                                        Indices::U32(HOME_CROWN_INDICES.to_vec()),
                                    ),
                                ),
                            ),
                            MeshMaterial2d(materials.add(ColorMaterial::from(HOME_PLANET_COLOR))),
                            Transform::from_xyz(0., 0., 0.01),
                            HomeCrownCmp,
                            Pickable::IGNORE,
                        ));
                    });
                }

                // Destroyed planets have no resources nor icons
                if !planet.is_destroyed {
                    for (i, icon) in Icon::iter().enumerate() {
                        parent
                            .spawn((
                                Sprite {
                                    image: assets.image(icon.to_lowername().as_str()),
                                    custom_size: Some(Vec2::splat(Icon::SIZE)),
                                    color: MAP_ICON_IDLE_TINT,
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
                                move |event: On<Pointer<Over>>,
                                      mut sprites: Query<&mut Sprite, With<Icon>>,
                                      mut state: ResMut<UiState>,
                                      map: Res<Map>,
                                      missions: Res<Missions>| {
                                    if let Ok(mut sprite) = sprites.get_mut(event.entity) {
                                        sprite.color = Color::WHITE;
                                    }
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
                            .observe(
                                |event: On<Pointer<Out>>,
                                 mut sprites: Query<&mut Sprite, With<Icon>>,
                                 mut state: ResMut<UiState>| {
                                    if let Ok(mut sprite) = sprites.get_mut(event.entity) {
                                        sprite.color = MAP_ICON_IDLE_TINT;
                                    }
                                    state.planet_hover = None;
                                    state.mission_hover = None;
                                    state.mission_hover_from_ui = false;
                                },
                            )
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
                                            select_planet(planet, &mut state, &player);
                                            settings.show_menu = true;
                                            if let Some(shop) = icon.shop() {
                                                state.shop = shop;
                                            }
                                        } else if icon == Icon::Attacked {
                                            state.mission = true;
                                            state.planet_selected = None;
                                            state.mission_tab = MissionTab::EnemyMissions;
                                        } else if icon == Icon::RailgunStrike {
                                            state.mission = false;
                                            state.planet_selected = None;
                                            state.railgun_confirmation = Some(planet_id);
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

                    // Draw a detailed electromagnetic field around the planet. Neutral source
                    // art preserves its filaments while the sprite tint supplies faction hue.
                    parent.spawn((
                        Sprite {
                            image: assets.image("planetary shield marker"),
                            color: OWN_COLOR.with_alpha(0.0),
                            custom_size: Some(Vec2::splat(planet.size() * 1.3)),
                            ..default()
                        },
                        Transform::from_xyz(0., 0., PLANETARY_SHIELD_DEPTH),
                        TweenAnim::new(PlanetaryShieldCmp::tween(OWN_COLOR)),
                        Visibility::Hidden,
                        PlanetaryShieldCmp::new(),
                    ));

                    // Draw the space dock at a random point on its orbit.
                    let dock_radius = planet.size() * 0.75;
                    let angle = rng().random_range(0.0..TAU);

                    parent.spawn((
                        Sprite {
                            image: assets.image("dock"),
                            custom_size: Some(Vec2::splat(planet.size() * 0.4)),
                            ..default()
                        },
                        Transform::from_xyz(
                            angle.cos() * dock_radius,
                            angle.sin() * dock_radius,
                            SPACE_DOCK_DEPTH,
                        ),
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::Linear,
                                Duration::from_secs(12),
                                TransformOrbitLens {
                                    radius: dock_radius,
                                    offset: angle,
                                },
                            )
                            .with_repeat_count(RepeatCount::Infinite),
                        ),
                        Pickable::IGNORE,
                        Visibility::Hidden,
                        SpaceDockCmp,
                    ));

                    // The endgame Railgun holds a close fixed station above the planet, clear of
                    // the upper-left Phalanx formation. Its barrel points inward while idle;
                    // strike playback draws the synchronized beams.
                    let railgun_angle = ORBITAL_RAILGUN_ANGLE;
                    let railgun_radius = planet.size() * ORBITAL_RAILGUN_RADIUS;
                    let railgun_anchor =
                        Vec2::new(railgun_angle.cos(), railgun_angle.sin()) * railgun_radius;
                    let railgun_rotation = railgun_angle - PI * 0.5;
                    let railgun_phase = visual_noise(planet.id as u32 + 9_271) * TAU;
                    parent
                        .spawn((
                            Sprite {
                                image: assets.image("orbital railgun marker"),
                                custom_size: Some(Vec2::splat(planet.size() * 0.48)),
                                ..default()
                            },
                            Transform {
                                translation: railgun_anchor.extend(ORBITAL_RAILGUN_DEPTH),
                                rotation: Quat::from_rotation_z(railgun_rotation),
                                ..default()
                            },
                            Pickable::IGNORE,
                            Visibility::Hidden,
                            OrbitalRailgunCmp {
                                planet: planet.id,
                                anchor: railgun_anchor,
                                base_rotation: railgun_rotation,
                                phase: railgun_phase,
                            },
                        ))
                        .observe(cursor::<Over>(SystemCursorIcon::Default))
                        .observe(move |mut event: On<Pointer<Over>>, mut state: ResMut<UiState>| {
                            event.propagate(false);
                            state.range_preview = Some(MapRangePreview::OrbitalRailgun(planet_id));
                        })
                        .observe(move |mut event: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                            event.propagate(false);
                            if state.range_preview
                                == Some(MapRangePreview::OrbitalRailgun(planet_id))
                            {
                                state.range_preview = None;
                            }
                        })
                        .observe(|mut event: On<Pointer<Click>>| event.propagate(false));

                    // Keep the fixed gate in the right-hand half of the orbit, safely opposite the
                    // two range markers on the left. Only the gate itself spins, so it stays
                    // visually active without circling the planet.
                    let gate_candidate = (angle + PI).rem_euclid(TAU);
                    let gate_angle = if gate_candidate.cos() < 0.0 {
                        PI - gate_candidate
                    } else {
                        gate_candidate
                    };
                    let gate_radius = planet.size() * 0.9;
                    parent
                        .spawn((
                            Sprite {
                                image: assets.image("jump gate marker"),
                                custom_size: Some(Vec2::splat(planet.size() * 0.36)),
                                ..default()
                            },
                            Transform::from_xyz(
                                gate_angle.cos() * gate_radius,
                                gate_angle.sin() * gate_radius,
                                JUMP_GATE_DEPTH,
                            ),
                            TweenAnim::new(
                                Tween::new(
                                    EaseFunction::Linear,
                                    Duration::from_secs(17),
                                    TransformSpinLens {
                                        offset: gate_angle,
                                        rotations: -2.0,
                                    },
                                )
                                .with_repeat_count(RepeatCount::Infinite),
                            ),
                            Pickable::IGNORE,
                            Visibility::Hidden,
                            JumpGateCmp {
                                planet: planet_id,
                            },
                        ))
                        .observe(cursor::<Over>(SystemCursorIcon::Pointer))
                        .observe(cursor::<Out>(SystemCursorIcon::Default))
                        .observe(move |mut event: On<Pointer<Over>>, mut state: ResMut<UiState>| {
                            event.propagate(false);
                            state.jump_gate_hover = Some(planet_id);
                        })
                        .observe(move |mut event: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                            event.propagate(false);
                            if state.jump_gate_hover == Some(planet_id) {
                                state.jump_gate_hover = None;
                            }
                        })
                        .observe(
                            move |mut event: On<Pointer<Click>>,
                                  mut state: ResMut<UiState>,
                                  settings: Res<Settings>,
                                  map: Res<Map>,
                                  player: Res<Player>| {
                                event.propagate(false);
                                if event.button == PointerButton::Primary {
                                    open_jump_gate_mission(
                                        &mut state, &settings, &map, &player, planet_id,
                                    );
                                }
                            },
                        );

                    // Spawn one satellite for every possible building level. The visibility system
                    // reveals the prefix matching the known current level. The phase order spreads
                    // low-level prefixes out while forming an even pentagon at level five, and the
                    // shared period prevents collisions without rotating the individual sprites.
                    let satellite_radius = planet.size() * 0.68;
                    for (index, step) in SOLAR_SATELLITE_PHASE_STEPS.into_iter().enumerate() {
                        let satellite_angle =
                            angle + TAU / 10.0 + TAU * step as f32 / Building::MAX_LEVEL as f32;
                        parent.spawn((
                            Sprite {
                                image: assets.image("solar satellite marker"),
                                custom_size: Some(Vec2::splat(planet.size() * 0.2)),
                                ..default()
                            },
                            Transform::from_xyz(
                                satellite_angle.cos() * satellite_radius,
                                satellite_angle.sin() * satellite_radius,
                                SOLAR_SATELLITE_DEPTH + index as f32 * SOLAR_SATELLITE_DEPTH_STEP,
                            ),
                            TweenAnim::new(
                                Tween::new(
                                    EaseFunction::Linear,
                                    Duration::from_secs(14),
                                    TransformOrbitLens {
                                        radius: satellite_radius,
                                        offset: satellite_angle,
                                    },
                                )
                                .with_repeat_count(RepeatCount::Infinite),
                            ),
                            Pickable::IGNORE,
                            Visibility::Hidden,
                            SolarSatelliteCmp {
                                level: index + 1,
                            },
                        ));
                    }

                    // Spawn one worker for every possible Recycler level. Animation places each
                    // visible worker beyond the orbital stack on the side facing its destination,
                    // where staggered phases keep the fleet from moving as one stacked sprite.
                    let recycler_phase = visual_noise(planet.id as u32 + 4_337);
                    for (index, phase_offset) in RECYCLER_PHASE_OFFSETS.into_iter().enumerate() {
                        let level = index + 1;
                        parent.spawn((
                            Sprite {
                                image: assets.image("recycler marker"),
                                custom_size: Some(Vec2::new(
                                    planet.size() * 0.30,
                                    planet.size() * 0.214,
                                )),
                                ..default()
                            },
                            Transform::from_xyz(
                                0.0,
                                0.0,
                                RECYCLER_DEPTH + index as f32 * RECYCLER_DEPTH_STEP,
                            ),
                            Pickable::IGNORE,
                            Visibility::Hidden,
                            RecyclerCmp {
                                planet: planet.id,
                                level,
                                phase: (recycler_phase + phase_offset).fract(),
                                gate_angle,
                            },
                        ));
                        for pulse in 0..3 {
                            parent.spawn((
                                Mesh2d(meshes.add(Annulus::new(0.86, 1.0))),
                                MeshMaterial2d(
                                    materials
                                        .add(ColorMaterial::from(Color::WHITE.with_alpha(0.0))),
                                ),
                                Transform::from_xyz(
                                    0.0,
                                    0.0,
                                    RECYCLER_SCAN_DEPTH + index as f32 * RECYCLER_DEPTH_STEP,
                                ),
                                Pickable::IGNORE,
                                Visibility::Hidden,
                                RecyclerScanCmp {
                                    planet: planet.id,
                                    recycler_level: level,
                                    pulse_offset: pulse as f32 / 3.0,
                                },
                            ));
                        }
                    }

                    // Range infrastructure stays in fixed map positions so it remains easy to
                    // acquire at every zoom level. Phalanx hover owns its coverage preview; the
                    // Relay is decorative because Spy missions are galaxy-wide.
                    // Three small Phalanx drones share the left-side station but follow independent
                    // pseudo-orbits, keeping the gate clear and echoing the shop artwork.
                    let phalanx_angle = PI * 0.75;
                    let phalanx_radius = planet.size() * 0.83;
                    let phalanx_anchor =
                        Vec2::new(phalanx_angle.cos(), phalanx_angle.sin()) * phalanx_radius;
                    let phalanx_phase = visual_noise(planet.id as u32 + 6_701) * TAU;
                    for index in 0..PHALANX_DRONE_COUNT {
                        let drone = SensorPhalanxCmp {
                            index,
                            anchor: phalanx_anchor,
                            phase: phalanx_phase,
                        };
                        let transform = phalanx_drone_pose(&drone, 0.0);
                        parent
                            .spawn((
                                Sprite {
                                    image: assets.image("sensor phalanx marker"),
                                    custom_size: Some(Vec2::splat(planet.size() * 0.23)),
                                    ..default()
                                },
                                transform,
                                Pickable::IGNORE,
                                Visibility::Hidden,
                                drone,
                            ))
                            .observe(cursor::<Over>(SystemCursorIcon::Default))
                            .observe(
                                move |mut event: On<Pointer<Over>>, mut state: ResMut<UiState>| {
                                    event.propagate(false);
                                    state.range_preview =
                                        Some(MapRangePreview::SensorPhalanx(planet_id));
                                },
                            )
                            .observe(
                                move |mut event: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                                    event.propagate(false);
                                    if state.range_preview
                                        == Some(MapRangePreview::SensorPhalanx(planet_id))
                                    {
                                        state.range_preview = None;
                                    }
                                },
                            )
                            .observe(|mut event: On<Pointer<Click>>| event.propagate(false));
                    }

                    let relay_angle = PI * 1.25;
                    let relay_radius = planet.size() * 0.83;
                    let relay_anchor =
                        Vec2::new(relay_angle.cos(), relay_angle.sin()) * relay_radius;
                    let relay_phase = visual_noise(planet.id as u32 + 2_609) * TAU;
                    let (relay_position, relay_tilt) =
                        range_marker_pose(relay_anchor, 0.0, relay_phase);
                    parent.spawn((
                        Sprite {
                            image: assets.image("command relay marker"),
                            custom_size: Some(Vec2::splat(planet.size() * 0.34)),
                            ..default()
                        },
                        Transform {
                            translation: relay_position.extend(COMMAND_RELAY_DEPTH),
                            rotation: Quat::from_rotation_z(relay_tilt),
                            ..default()
                        },
                        Pickable::IGNORE,
                        Visibility::Hidden,
                        CommandRelayCmp,
                        RangeMarkerMotionCmp {
                            anchor: relay_anchor,
                            elapsed: 0.0,
                            phase: relay_phase,
                        },
                    ));

                    // Vertex alpha supplies the scanner's soft field, rim glow, and fading trails.
                    let scanner_material = materials.add(ColorMaterial {
                        color: Color::WHITE,
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

fn railgun_action_available(
    map: &Map,
    player: &Player,
    pending: Option<&PendingTurnCommands>,
    target: PlanetId,
    hovered: bool,
) -> bool {
    let (can_accept, already_committed) = pending.map_or((true, false), |pending| {
        (
            pending.can_accept_commands(),
            pending
                .commands
                .iter()
                .chain(&pending.queued_commands)
                .any(|command| matches!(command, TurnCommand::FireOrbitalRailguns { .. })),
        )
    });
    hovered
        && can_accept
        && !already_committed
        && !orbital_railgun_origins(map, player.id, target).is_empty()
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
        (
            &mut Visibility,
            &mut Mesh2d,
            &mut Transform,
            &mut ScannerCmp,
            &MeshMaterial2d<ColorMaterial>,
        ),
        (
            Without<Icon>,
            Without<PlanetNameCmp>,
            Without<PlanetResourcesCmp>,
            Without<SpaceDockCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    active_destructions: Query<
        (Option<&ExplosionCmp>, Option<&OrbitalStrikeEffect>),
        Or<(With<ExplosionCmp>, With<OrbitalStrikeEffect>)>,
    >,
    children_q: Query<&Children>,
    world: PlanetInfoResources,
    state: Res<UiState>,
    settings: Res<Settings>,
    assets: Res<WorldAssets>,
    time: Res<Time>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let map: &Map = &world.map;
    let player: &Player = &world.player;
    let (n_owned, n_max_owned) = player.planets_owned(map, &settings);

    for (planet_e, mut planet_s, planet_c) in &mut planet_q {
        let planet = map.get(planet_c.id);

        let destruction_active = active_destructions.iter().any(|(mission, railgun)| {
            mission.is_some_and(|effect| effect.planet == planet.id)
                || railgun.is_some_and(|effect| effect.destroyed && effect.target == planet.id)
        });
        planet_s.image = assets.image(map_planet_image(planet, destruction_active));

        let hovered = state.planet_hover == Some(planet.id);

        // Show/hide planet icons
        let mut count = 0;
        for child in children_q.iter_descendants(planet_e) {
            if let Ok((mut icon_v, mut icon_t, icon)) = icon_q.get_mut(child) {
                let visible = match icon {
                    Icon::Attacked => world.missions.iter().any(|m| {
                        player.owns(planet)
                            && m.objective != Icon::Deploy
                            && m.destination == planet.id
                    }),
                    Icon::RailgunStrike => railgun_action_available(
                        map,
                        player,
                        world.pending.as_deref(),
                        planet.id,
                        hovered,
                    ),
                    Icon::Buildings => {
                        (player.owns(planet) || (player.controls(planet) && planet.is_moon()))
                            && (hovered || icon.condition(planet) || settings.show_info)
                    },
                    Icon::Orbitals => {
                        // Match the other construction shortcuts: keep completed infrastructure
                        // visible, and expose the empty category while inspecting an owned world.
                        player.owns(planet)
                            && !planet.is_moon()
                            && (hovered || icon.condition(planet) || settings.show_info)
                    },
                    Icon::Fleet => {
                        // Shows when having an army on a not-owned planet, but hides when hovered
                        player.controls(planet)
                            && if player.owns(planet) || planet.is_moon() {
                                hovered || icon.condition(planet) || settings.show_info
                            } else {
                                icon.condition(planet) && !hovered && !settings.show_info
                            }
                    },
                    Icon::Defenses => {
                        player.owns(planet)
                            && !planet.is_moon()
                            && (hovered || icon.condition(planet) || settings.show_info)
                    },
                    _ => {
                        // Existing missions stay visible; hover or the info toggle also shows
                        // available objectives from worlds under the player's control.
                        let has_mission = world.missions.iter().any(|m| {
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

                        has_mission || ((hovered || settings.show_info) && has_condition)
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
                *visibility = if hovered || settings.show_info {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
            if let Ok(mut visibility) = resources_q.get_mut(child) {
                *visibility = if (hovered || settings.show_info) && !planet.is_destroyed {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }

            // Show/hide scanner indicator
            if let Ok((mut visibility, mut mesh, mut transform, mut scanner, material)) =
                scanner_q.get_mut(child)
            {
                // Planet hover still previews a moon's Orbital Radar because moons have no
                // dedicated marker. Planetary Phalanx coverage belongs exclusively to its
                // stationary marker hover.
                let radius = match state.range_preview {
                    Some(MapRangePreview::OrbitalRailgun(id))
                        if id == planet.id
                            && !planet.is_moon()
                            && planet.has(&Unit::Building(Building::OrbitalRailgun)) =>
                    {
                        ORBITAL_RAILGUN_RANGE_PER_LEVEL
                            * Planet::SIZE
                            * planet
                                .army
                                .amount(&Unit::Building(Building::OrbitalRailgun))
                                .min(Building::MAX_LEVEL) as f32
                    },
                    Some(MapRangePreview::SensorPhalanx(id))
                        if id == planet.id
                            && !planet.is_moon()
                            && player.owns(planet)
                            && planet.has(&Unit::Building(Building::SensorPhalanx)) =>
                    {
                        PHALANX_DISTANCE
                            * Planet::SIZE
                            * planet.army.amount(&Unit::Building(Building::SensorPhalanx)) as f32
                            + planet.size() * 0.5
                    },
                    _ if hovered
                        && planet.is_moon()
                        && player.controls(planet)
                        && planet.has(&Unit::Building(Building::OrbitalRadar)) =>
                    {
                        RADAR_DISTANCE
                            * Planet::SIZE
                            * planet.army.amount(&Unit::Building(Building::OrbitalRadar)) as f32
                            + planet.size() * 0.5
                    },
                    _ => 0.,
                };

                if radius > 0. && !planet.is_destroyed {
                    if let Some(mut material) = materials.get_mut(&material.0) {
                        material.color = match state.range_preview {
                            Some(MapRangePreview::OrbitalRailgun(id)) if id == planet.id => planet
                                .owned
                                .map(|owner| world.session.player_color(owner).color())
                                .unwrap_or(Color::srgb_u8(190, 198, 210)),
                            _ => player.color().color(),
                        };
                    }
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

/// Keeps the intact world visible until its destruction blast reaches the swap frame.
fn map_planet_image(planet: &Planet, destruction_active: bool) -> String {
    if destruction_active && planet.is_destroyed && planet.image != 0 {
        let prefix = if planet.is_moon() {
            "moon"
        } else {
            "planet"
        };
        format!("{prefix}{}", planet.image)
    } else {
        planet.image()
    }
}

/// Colors visible defenses from private control intelligence and public ownership signals.
pub fn update_planet_defenses(
    planet_q: Query<(Entity, &PlanetCmp)>,
    children_q: Query<&Children>,
    mut ps_q: Query<(
        &mut Visibility,
        &mut TweenAnim,
        &mut PlanetaryShieldCmp,
        &mut Sprite,
        &mut Transform,
    )>,
    mut dock_q: Query<
        (&mut Visibility, &mut Sprite),
        (With<SpaceDockCmp>, Without<JumpGateCmp>, Without<PlanetaryShieldCmp>),
    >,
    mut railgun_q: Query<
        (&mut Visibility, &mut Sprite, &mut Pickable),
        (
            With<OrbitalRailgunCmp>,
            Without<SpaceDockCmp>,
            Without<JumpGateCmp>,
            Without<SolarSatelliteCmp>,
            Without<CommandRelayCmp>,
            Without<SensorPhalanxCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    mut gate_q: Query<
        (&mut Visibility, &mut Sprite, &mut Pickable),
        (With<JumpGateCmp>, Without<SpaceDockCmp>, Without<PlanetaryShieldCmp>),
    >,
    mut satellite_q: Query<
        (&mut Visibility, &mut Sprite, &SolarSatelliteCmp),
        (
            With<SolarSatelliteCmp>,
            Without<SpaceDockCmp>,
            Without<JumpGateCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    mut relay_q: Query<
        (&mut Visibility, &mut Sprite, &mut Pickable),
        (
            With<CommandRelayCmp>,
            Without<SensorPhalanxCmp>,
            Without<SolarSatelliteCmp>,
            Without<SpaceDockCmp>,
            Without<JumpGateCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    mut phalanx_q: Query<
        (&mut Visibility, &mut Sprite, &mut Pickable),
        (
            With<SensorPhalanxCmp>,
            Without<CommandRelayCmp>,
            Without<SolarSatelliteCmp>,
            Without<SpaceDockCmp>,
            Without<JumpGateCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    camera: Single<&Projection, With<MainCamera>>,
    time: Res<Time>,
    mut development_visibility: Local<DevelopmentVisibility>,
    world: PlanetInfoResources,
) {
    let map = &world.map;
    let player = &world.player;
    let session = &world.session;
    let jump_gate_network_active = jump_gate_network_available(map, player);
    let scale = match *camera {
        Projection::Orthographic(ref projection) => projection.scale,
        _ => f32::INFINITY,
    };
    let detail_alpha =
        development_visibility.update(scale <= DEVELOPMENT_MAX_SCALE, time.delta_secs());

    for (entity, planet_c) in &planet_q {
        let planet = map.get(planet_c.id);
        let controls = player.controls(planet);
        // Overload intent is private to its controller. A steady, rotating field makes the
        // armed state conspicuous without leaking it through stale enemy intelligence.
        let overloaded = controls && planet.shield_overload.is_overloaded();
        // Read intelligence once per planet. A hidden capture must not change its displayed color.
        let info = (!controls).then(|| player.last_info(planet, &world.missions.0)).flatten();
        let (army, controller) = if controls {
            (Some(&planet.army), planet.controlled)
        } else {
            (info.as_ref().map(|info| &info.army), info.as_ref().and_then(|info| info.controlled))
        };
        let has_ps = !planet.is_destroyed
            && army.is_some_and(|army| army.amount(&Unit::planetary_shield()) > 0);
        // Strategic orbitals reveal their owner, never a possibly different controller. A
        // destruction effect temporarily preserves that public fact until its rings finish.
        let destroyed_owner = |structure| {
            world.structure_effects.iter().find_map(|effect| {
                (effect.planet == planet.id
                    && effect.structure == structure
                    && effect.change == PublicStructureChange::Destroyed)
                    .then_some(effect.owner)
            })
        };
        let dock_owner = (!planet.is_destroyed && planet.army.amount(&Unit::space_dock()) > 0)
            .then_some(planet.owned)
            .flatten()
            .or_else(|| destroyed_owner(PublicStructure::SpaceDock));
        let railgun_owner = (!planet.is_destroyed
            && planet.army.amount(&Unit::Building(Building::OrbitalRailgun)) > 0)
            .then_some(planet.owned)
            .flatten()
            .or_else(|| destroyed_owner(PublicStructure::OrbitalRailgun));
        let has_dock = dock_owner.is_some();
        let has_railgun = railgun_owner.is_some();
        let has_gate = !planet.is_destroyed
            && army.is_some_and(|army| army.amount(&Unit::Building(Building::JumpGate)) > 0);
        let satellite_level = if planet.is_destroyed {
            0
        } else {
            army.map_or(0, |army| {
                army.amount(&Unit::Building(Building::SolarSatellite)).min(Building::MAX_LEVEL)
            })
        };
        let has_relay = !planet.is_destroyed
            && army.is_some_and(|army| army.amount(&Unit::Building(Building::CommandRelay)) > 0);
        let has_phalanx = !planet.is_destroyed
            && army.is_some_and(|army| army.amount(&Unit::Building(Building::SensorPhalanx)) > 0);
        // Defenses left on an unclaimed world have no player color.
        let color = controller
            .map(|id| session.player_color(id).color())
            .unwrap_or(Color::srgb_u8(190, 198, 210));
        let dock_color = dock_owner
            .map(|id| session.player_color(id).color())
            .unwrap_or(Color::srgb_u8(190, 198, 210));
        let railgun_color = railgun_owner
            .map(|id| session.player_color(id).color())
            .unwrap_or(Color::srgb_u8(190, 198, 210));

        for child in children_q.iter_descendants(entity) {
            if let Ok((mut visibility, mut tween, mut ps, mut sprite, mut transform)) =
                ps_q.get_mut(child)
            {
                *visibility = if has_ps {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                let was_overloaded = ps.overloaded;
                ps.overloaded = has_ps && overloaded;
                ps.spin_factor = approach(
                    ps.spin_factor,
                    if ps.overloaded {
                        1.0
                    } else {
                        0.0
                    },
                    time.delta_secs() / PLANETARY_SHIELD_OVERLOAD_SPIN_RAMP_SECONDS,
                );
                transform.rotate_z(
                    TAU * ps.spin_factor * time.delta_secs()
                        / PLANETARY_SHIELD_OVERLOAD_ROTATION_SECONDS,
                );
                if has_ps && ps.color != Some(color) {
                    let mut pulse = PlanetaryShieldCmp::tween(color);
                    // Recolor the field without restarting its pulse midway through a fade.
                    pulse.set_elapsed(tween.tweenable().elapsed());
                    match tween.set_tweenable(pulse) {
                        Ok(_) => {
                            ps.color = Some(color);
                            sprite.color = color.with_alpha(sprite.color.alpha());
                        },
                        Err(error) => warn!("Failed to update planetary shield color: {error}"),
                    }
                }
                if ps.overloaded && !was_overloaded {
                    let mut steady_field = PlanetaryShieldCmp::tween(color);
                    steady_field.set_elapsed(PLANETARY_SHIELD_PULSE_PEAK);
                    match tween.set_tweenable(steady_field) {
                        Ok(_) => {
                            sprite.color = color.with_alpha(PLANETARY_SHIELD_MAX_ALPHA);
                        },
                        Err(error) => {
                            warn!("Failed to steady overloaded planetary shield: {error}")
                        },
                    }
                }
                tween.speed = if ps.overloaded {
                    0.0
                } else {
                    1.0
                };
            }
            if let Ok((mut visibility, mut sprite)) = dock_q.get_mut(child) {
                *visibility = if has_dock {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if has_dock {
                    sprite.color = dock_color;
                }
            }
            if let Ok((mut visibility, mut sprite, mut pickable)) = railgun_q.get_mut(child) {
                *visibility = if has_railgun {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                *pickable = if has_railgun {
                    Pickable::default()
                } else {
                    Pickable::IGNORE
                };
                if has_railgun {
                    sprite.color = railgun_color;
                }
            }
            if let Ok((mut visibility, mut sprite, mut pickable)) = gate_q.get_mut(child) {
                *visibility = if has_gate {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                *pickable = if has_gate
                    && jump_gate_network_active
                    && is_usable_owned_jump_gate(planet, player)
                {
                    Pickable::default()
                } else {
                    Pickable::IGNORE
                };
                if has_gate {
                    sprite.color = color;
                }
            }
            if let Ok((mut visibility, mut sprite, satellite)) = satellite_q.get_mut(child) {
                let constructed = satellite.level <= satellite_level;
                *visibility = if constructed && detail_alpha > 0.01 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if constructed {
                    sprite.color = color.with_alpha(detail_alpha);
                }
            }
            if let Ok((mut visibility, mut sprite, mut pickable)) = relay_q.get_mut(child) {
                *visibility = if has_relay && detail_alpha > 0.01 {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                *pickable = Pickable::IGNORE;
                if has_relay {
                    let active = !controls || planet.command_relay_active;
                    sprite.color = color.with_alpha(
                        detail_alpha
                            * if active {
                                1.0
                            } else {
                                0.32
                            },
                    );
                }
            }
            if let Ok((mut visibility, mut sprite, mut pickable)) = phalanx_q.get_mut(child) {
                *visibility = if has_phalanx {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                *pickable = if has_phalanx && player.owns(planet) {
                    Pickable::default()
                } else {
                    Pickable::IGNORE
                };
                if has_phalanx {
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
pub(crate) fn update_voronoi(
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
    structure_effects: Query<&PublicStructureEffect>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let fading_public_owners = structure_effects
        .iter()
        .filter(|effect| effect.change == PublicStructureChange::Destroyed)
        .map(|effect| (effect.planet, effect.owner))
        .collect::<HashMap<_, _>>();
    let known_controllers = map
        .planets
        .iter()
        .filter_map(|planet| {
            let public_owner = (!planet.is_destroyed
                && planet.owned.is_some()
                && (planet.army.amount(&Unit::space_dock()) > 0
                    || planet.army.amount(&Unit::Building(Building::OrbitalRailgun)) > 0))
                .then_some(planet.owned)
                .flatten()
                .or_else(|| fading_public_owners.get(&planet.id).copied());
            let controller = public_owner.or_else(|| {
                if player.controls(planet) {
                    planet.controlled
                } else {
                    player.last_info(planet, &missions.0).and_then(|info| info.controlled)
                }
            });
            controller.map(|controller| (planet.id, controller))
        })
        .collect::<HashMap<PlanetId, PlayerId>>();

    let animate = game_state.as_ref().is_none_or(|state| *state.get() == GameState::Playing);
    let delta_seconds =
        time.as_ref().map_or(TERRITORY_TRANSITION_SECONDS, |time| time.delta_secs());

    for (mut cell_v, cell_m, cell, mut transition) in &mut cell_q {
        let planet = map.get(cell.0);
        let controller = known_controllers.get(&planet.id).copied();
        let visible = controller.is_some()
            && (!planet.is_destroyed || fading_public_owners.contains_key(&planet.id));
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
            (!map.get(edge.planet).is_destroyed || fading_public_owners.contains_key(&edge.planet))
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

fn celestial_frame_state(kind: CelestialKind, slot: usize, elapsed: f32) -> (usize, f32) {
    let (current, next, blend) =
        looping_frame_sample(elapsed, kind.frame_count(), kind.frame_seconds());
    // Slow neutron-star motion must not ease to a stop at every sampled frame.
    let blend = if kind == CelestialKind::NeutronStar {
        (elapsed / kind.frame_seconds()).rem_euclid(kind.frame_count() as f32).fract()
    } else {
        blend
    };
    if slot == 0 {
        (current, (1.0 - blend) * kind.opacity())
    } else {
        (next, blend * kind.opacity())
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
        (With<SolarStarCmp>, Without<SolarStarFrameCmp>, Without<NebulaCmp>, Without<CelestialCmp>),
    >,
    mut solar_frame_q: Query<
        (&SolarStarFrameCmp, &mut Sprite),
        (Without<SolarStarCmp>, Without<CelestialFrameCmp>, Without<MainCamera>),
    >,
    mut nebula_q: Query<
        (&NebulaCmp, &mut Transform),
        (Without<SolarStarCmp>, Without<CelestialCmp>),
    >,
    celestial_q: Query<(&CelestialCmp, &Children)>,
    mut celestial_frame_q: Query<
        (&CelestialFrameCmp, &mut Sprite),
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
    for (celestial, children) in &celestial_q {
        for child in children.iter() {
            let Ok((frame, mut sprite)) = celestial_frame_q.get_mut(child) else {
                continue;
            };
            let (frame_index, alpha) = celestial_frame_state(celestial.kind, frame.slot, elapsed);
            sprite.image = celestial.frames[frame_index].clone();
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

/// Advances the visible, non-authoritative asteroid-band drift and tumbling.
pub(crate) fn animate_asteroid_belts(
    mut asteroids: Query<(&AsteroidCmp, &mut Transform)>,
    time: Res<Time>,
) {
    let elapsed = time.elapsed_secs_f64() as f32;
    for (asteroid, mut transform) in &mut asteroids {
        let angle = asteroid.phase + elapsed * asteroid.angular_speed;
        let wobble = (elapsed * asteroid.wobble_speed + asteroid.wobble_phase).sin();
        let radius = asteroid.radius + wobble * asteroid.wobble_amplitude;
        transform.translation =
            (asteroid.center + Vec2::from_angle(angle) * radius).extend(transform.translation.z);
        transform.rotation = Quat::from_rotation_z(elapsed * asteroid.spin + asteroid.phase);
        let tumble = (elapsed * asteroid.tumble_speed + asteroid.tumble_phase).sin();
        transform.scale = Vec3::new(1.0 + tumble * 0.04, 1.0 - tumble * 0.04, 1.0);
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
