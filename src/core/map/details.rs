//! Close-zoom development and public battle debris; neither changes simulation state.

use std::collections::BTreeMap;
use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy::window::SystemCursorIcon;

use super::model::{Map, MapCmp};
use super::planet::{Planet, PlanetId, PlanetKind};
use super::utils::cursor;
use crate::core::assets::WorldAssets;
use crate::core::camera::MainCamera;
#[cfg(test)]
use crate::core::combat::report::MissionReport;
use crate::core::combat::report::Side;
use crate::core::constants::PLANET_Z;
use crate::core::loading::{refresh_gameplay_projection, refresh_turn_draft};
use crate::core::missions::Missions;
use crate::core::player::Player;
#[cfg(test)]
use crate::core::recycling::destroyed_ships;
use crate::core::recycling::{debris_sites, DebrisSite, DebrisSize};
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Unit};
use crate::multiplayer::client::MultiplayerSession;

const DEBRIS_CENTER_ANGLE: f32 = TAU * 0.5;
const DEBRIS_ANGLE_JITTER: f32 = 0.02;
pub(crate) const DEBRIS_DEPTH: f32 = 0.26;
const PLANET_TERRAFORMER_ART_ASPECT: f32 = 1102.0 / 1427.0;
pub(crate) const DEVELOPMENT_MAX_SCALE: f32 = 0.9;

#[cfg(test)]
fn destroyed_units(report: &MissionReport) -> usize {
    destroyed_ships(report)
}

fn noise(mut seed: u32) -> f32 {
    seed = (seed ^ (seed >> 16)).wrapping_mul(0x7feb_352d);
    seed = (seed ^ (seed >> 15)).wrapping_mul(0x846c_a68b);
    (seed ^ (seed >> 16)) as f32 / u32::MAX as f32
}

fn detail_alpha(scale: f32) -> f32 {
    let amount = ((1.05 - scale) / 0.3).clamp(0.0, 1.0);
    amount * amount * (3.0 - 2.0 * amount)
}

#[derive(Default)]
/// Local fade progress shared by close-zoom development-style presentation.
pub struct DevelopmentVisibility(f32);

impl DevelopmentVisibility {
    pub(crate) fn update(&mut self, visible: bool, delta_secs: f32) -> f32 {
        // Zoom chooses an endpoint; elapsed time completes the fade even when zoom stops.
        let step = delta_secs / 0.22;
        self.0 = if visible {
            (self.0 + step).min(1.0)
        } else {
            (self.0 - step).max(0.0)
        };
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DebrisVisuals {
    pieces: usize,
    angle_step: f32,
    min_radius: f32,
    radius_jitter: f32,
    min_diameter: f32,
    diameter_jitter: f32,
}

fn debris_visuals(size: DebrisSize) -> DebrisVisuals {
    match size {
        DebrisSize::Small => DebrisVisuals {
            pieces: 2,
            angle_step: 0.028,
            min_radius: 1.02,
            radius_jitter: 0.06,
            min_diameter: 0.22,
            diameter_jitter: 0.06,
        },
        DebrisSize::Medium => DebrisVisuals {
            pieces: 5,
            angle_step: 0.045,
            min_radius: 1.04,
            radius_jitter: 0.10,
            min_diameter: 0.25,
            diameter_jitter: 0.09,
        },
        DebrisSize::Large => DebrisVisuals {
            pieces: 9,
            angle_step: 0.055,
            min_radius: 1.08,
            radius_jitter: 0.12,
            min_diameter: 0.29,
            diameter_jitter: 0.11,
        },
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Development {
    settlement: usize,
    mining: usize,
    refinery: usize,
    factory: usize,
    terraformer: usize,
    administration: usize,
    shipyard: usize,
    reactor: usize,
    laboratory: usize,
    silo: usize,
    lunar_base: usize,
    tidal_generator: usize,
    senate: bool,
    sensor: bool,
    surface_build_order: [Option<Building>; 4],
}

fn tier(level: usize) -> usize {
    match level {
        0 => 0,
        1..=2 => 1,
        3..=4 => 2,
        _ => 3,
    }
}

fn development(planet: &Planet, army: Option<&Army>) -> Development {
    let Some(army) = army.filter(|_| !planet.is_destroyed) else {
        return Development::default();
    };
    let level = |building| army.amount(&Unit::Building(building));
    Development {
        settlement: tier(
            [
                Building::MetalMine,
                Building::CrystalMine,
                Building::DeuteriumSynthesizer,
                Building::LunarBase,
            ]
            .into_iter()
            .map(level)
            .max()
            .unwrap_or(0),
        ),
        mining: tier(level(Building::MetalMine).max(level(Building::CrystalMine))),
        refinery: tier(level(Building::DeuteriumSynthesizer)),
        factory: tier(level(Building::Factory)),
        terraformer: tier(level(Building::Terraformer)),
        administration: tier(level(Building::ColonialAdministration)),
        shipyard: tier(level(Building::Shipyard)),
        reactor: tier(level(Building::Reactor)),
        laboratory: tier(level(Building::Laboratory)),
        silo: tier(level(Building::MissileSilo)),
        lunar_base: tier(level(Building::LunarBase)),
        tidal_generator: tier(level(Building::TidalGenerator)),
        senate: level(Building::Senate) > 0,
        sensor: level(Building::SensorPhalanx) > 0 || level(Building::OrbitalRadar) > 0,
        surface_build_order: planet.surface_build_order,
    }
}

#[derive(Resource, Default)]
struct DetailCache {
    turn: Option<usize>,
    assets_ready: bool,
    debris: BTreeMap<PlanetId, DebrisSite>,
    development: BTreeMap<PlanetId, Development>,
}

#[derive(Component)]
struct Detail {
    planet: PlanetId,
    opacity: f32,
    debris_turn: Option<(usize, usize)>,
}

#[derive(Component)]
struct Debris {
    origin: Vec2,
    rotation: f32,
    phase: f32,
    amplitude: f32,
    speed: f32,
}

#[derive(Component)]
struct DebrisHitTarget {
    planet: PlanetId,
    site: DebrisSite,
}

fn animate_debris(elapsed: Res<DetailAnimationTime>, mut debris: Query<(&Debris, &mut Transform)>) {
    for (debris, mut transform) in &mut debris {
        let phase = elapsed.0 * debris.speed + debris.phase;
        // Bounded local drift keeps wreckage clear of the planet and its status icons.
        let offset = Vec2::new(phase.sin(), (phase * 0.73 + 1.2).sin()) * debris.amplitude;
        transform.translation = (debris.origin + offset).extend(transform.translation.z);
        transform.rotation = Quat::from_rotation_z(debris.rotation + phase.sin() * 0.1);
    }
}

#[derive(Resource, Default)]
struct DetailAnimationTime(f32);

#[derive(Component)]
struct SurfaceLight {
    center: Vec2,
    diameter: f32,
    position_seed: u32,
    flicker_seed: u32,
    offset: Vec2,
    brightness: f32,
}

#[derive(Component)]
struct SurfaceLightBackdrop;

#[derive(Component)]
struct FloatingDetail {
    origin: Vec2,
    phase: f32,
    amplitude: f32,
}

#[derive(Component)]
struct PlatformLight {
    phase: f32,
    brightness: f32,
}

fn floating_detail(planet: &Planet, offset: Vec2, origin: Vec2) -> FloatingDetail {
    FloatingDetail {
        origin,
        phase: noise((planet.id as u32).wrapping_add(offset.x.to_bits())) * TAU,
        amplitude: planet.size() * 0.008,
    }
}

fn animate_floating_development(
    elapsed: Res<DetailAnimationTime>,
    mut details: Query<(&FloatingDetail, &mut Transform, Option<&mut PlatformLight>)>,
) {
    for (floating, mut transform, light) in &mut details {
        transform.translation.y =
            floating.origin.y + (elapsed.0 * 0.8 + floating.phase).sin() * floating.amplitude;
        if let Some(mut light) = light {
            light.brightness = 0.35 + 0.65 * (elapsed.0 * 2.0 + light.phase).sin().powi(8);
        }
    }
}

/// Shared soft footprint separates small structures from bright, detailed terrain.
#[derive(Resource)]
struct StructureShadow(Handle<Image>);

impl FromWorld for StructureShadow {
    fn from_world(world: &mut World) -> Self {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};
        let mut pixels = Vec::with_capacity(32 * 32 * 4);
        for y in 0..32 {
            for x in 0..32 {
                let radius = Vec2::new((x as f32 - 15.5) / 15.5, (y as f32 - 15.5) / 15.5).length();
                let alpha = ((1.0 - radius).clamp(0.0, 1.0) * 2.5).min(1.0);
                pixels.extend_from_slice(&[0, 0, 0, (alpha * 190.0) as u8]);
            }
        }
        let mut image = Image::new(
            Extent3d {
                width: 32,
                height: 32,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.sampler = bevy::image::ImageSampler::linear();
        Self(world.resource_mut::<Assets<Image>>().add(image))
    }
}

const SURFACE_LIGHT_CELL_SIZE: u32 = 16;
const SURFACE_LIGHT_VARIANTS: usize = 4;

/// Small procedural light silhouettes avoid repeating the same rectangular dot across a world.
#[derive(Resource)]
struct SurfaceLightAtlas(Handle<Image>);

impl FromWorld for SurfaceLightAtlas {
    fn from_world(world: &mut World) -> Self {
        use bevy::asset::RenderAssetUsages;
        use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat};

        let width = SURFACE_LIGHT_CELL_SIZE * SURFACE_LIGHT_VARIANTS as u32;
        let mut pixels = Vec::with_capacity((width * SURFACE_LIGHT_CELL_SIZE * 4) as usize);
        let spot = |point: Vec2, center: Vec2, scale: Vec2, intensity: f32| {
            let radius = ((point - center) / scale).length();
            let halo = (1.0 - radius).clamp(0.0, 1.0).powi(2) * 0.42;
            let core = (1.0 - radius * 3.2).clamp(0.0, 1.0);
            ((halo + core).min(1.0) * intensity).clamp(0.0, 1.0)
        };
        for y in 0..SURFACE_LIGHT_CELL_SIZE {
            for variant in 0..SURFACE_LIGHT_VARIANTS {
                for x in 0..SURFACE_LIGHT_CELL_SIZE {
                    let point = Vec2::new(
                        (x as f32 + 0.5) / SURFACE_LIGHT_CELL_SIZE as f32 * 2.0 - 1.0,
                        (y as f32 + 0.5) / SURFACE_LIGHT_CELL_SIZE as f32 * 2.0 - 1.0,
                    );
                    let shape = if point.abs().max_element() > 0.86 {
                        0.0
                    } else {
                        match variant {
                            0 => spot(point, Vec2::ZERO, Vec2::splat(0.82), 1.0),
                            1 => spot(point, Vec2::new(-0.18, 0.08), Vec2::new(0.72, 0.62), 1.0)
                                .max(spot(point, Vec2::new(0.38, -0.22), Vec2::splat(0.34), 0.78)),
                            2 => [
                                (Vec2::new(-0.38, 0.18), 0.48, 0.72),
                                (Vec2::new(0.02, -0.05), 0.56, 1.0),
                                (Vec2::new(0.40, 0.24), 0.31, 0.62),
                            ]
                            .into_iter()
                            .map(|(center, scale, intensity)| {
                                spot(point, center, Vec2::splat(scale), intensity)
                            })
                            .fold(0.0, f32::max),
                            _ => spot(point, Vec2::new(-0.06, 0.02), Vec2::new(0.82, 0.38), 1.0)
                                .max(spot(point, Vec2::new(0.36, 0.28), Vec2::splat(0.28), 0.66)),
                        }
                    };
                    // A dark outer fringe preserves contrast on illuminated terrain while the
                    // warm-white core still reads as emitted light on night-side surfaces.
                    let core = ((shape - 0.16) / 0.58).clamp(0.0, 1.0);
                    let core = core * core * (3.0 - 2.0 * core);
                    let channel = |edge: f32| (edge + (255.0 - edge) * core).round() as u8;
                    pixels.extend_from_slice(&[
                        channel(38.0),
                        channel(24.0),
                        channel(12.0),
                        (shape.sqrt() * 255.0).round() as u8,
                    ]);
                }
            }
        }
        let mut image = Image::new(
            Extent3d {
                width,
                height: SURFACE_LIGHT_CELL_SIZE,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            pixels,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.sampler = bevy::image::ImageSampler::linear();
        Self(world.resource_mut::<Assets<Image>>().add(image))
    }
}

struct DevelopmentArt<'a> {
    base: &'a Handle<Image>,
    base_size: Vec2,
    facilities: &'a Handle<Image>,
    facilities_size: Vec2,
    gas: &'a Handle<Image>,
    gas_size: Vec2,
    moon_shipyard: &'a Handle<Image>,
    moon_tidal_generator: &'a Handle<Image>,
    moon_orbital_radar: &'a Handle<Image>,
    planet_terraformer: &'a Handle<Image>,
    gas_planet_terraformer: &'a Handle<Image>,
    planet_administration: &'a Handle<Image>,
    gas_planet_administration: &'a Handle<Image>,
    surface_lights: &'a Handle<Image>,
    shadow: &'a Handle<Image>,
}

/// Selects a new surface position only while the cluster is fully dark.
fn light_sample(position_seed: u32, flicker_seed: u32, seconds: f32) -> (Vec2, f32) {
    let cycle_seconds = 11.0 + noise(position_seed.wrapping_add(3)) * 8.0;
    let phase = seconds / cycle_seconds + noise(position_seed);
    let cycle_seed = position_seed.wrapping_add((phase.floor() as u32).wrapping_mul(0x9e37_79b9));
    let position = Vec2::from_angle(noise(cycle_seed) * TAU)
        * (0.08 + noise(cycle_seed.wrapping_add(1)) * 0.3);
    let progress = phase.fract();
    let smooth = |value: f32| {
        let x = value.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    let fade_fraction = 0.06 + noise(position_seed.wrapping_add(4)) * 0.05;
    let envelope = smooth(progress / fade_fraction) * smooth((1.0 - progress) / fade_fraction);
    let speed = 0.8 + noise(flicker_seed.wrapping_add(1)) * 3.8;
    let depth = 0.04 + noise(flicker_seed.wrapping_add(2)) * 0.21;
    let flicker_phase = noise(flicker_seed.wrapping_add(3)) * TAU;
    let slow = (seconds * speed + flicker_phase).sin().powi(2);
    let sparkle = (seconds * speed * 2.7 + flicker_phase * 1.7).sin().powi(8);
    let flicker = 1.0 - depth + depth * (slow * 0.72 + sparkle * 0.28);
    (position, envelope * flicker)
}

fn animate_surface_lights(
    time: Res<Time>,
    game: Res<State<GameState>>,
    mut elapsed: ResMut<DetailAnimationTime>,
    mut lights: Query<(&mut SurfaceLight, &mut Transform)>,
) {
    if *game.get() == GameState::Playing {
        elapsed.0 += time.delta_secs();
    }
    for (mut light, mut transform) in &mut lights {
        let (position, brightness) =
            light_sample(light.position_seed, light.flicker_seed, elapsed.0);
        let position = light.center + position * light.diameter + light.offset;
        transform.translation.x = position.x;
        transform.translation.y = position.y;
        light.brightness = brightness;
    }
}

fn open_latest_battle(player: &Player, planet: PlanetId, state: &mut UiState) {
    let Some(report) = player.reports.iter().rev().find(|report| {
        report.mission.destination == planet && !report.hidden && report.combat_report.is_some()
    }) else {
        return;
    };
    state.planet_hover = None;
    state.mission_hover = None;
    state.planet_selected = None;
    state.mission = true;
    state.mission_tab = MissionTab::MissionReports;
    state.mission_report = Some(report.mission.id);
    state.combat_report = None;
    if report.can_see(&Side::Attacker, player.id) && report.can_see(&Side::Defender, player.id) {
        state.combat_report = Some(report.id);
        state.combat_report_round = 1;
        state.combat_report_total = true;
        state.combat_report_hover = None;
    }
}

fn spawn_debris(
    commands: &mut Commands,
    planet: &Planet,
    site: &DebrisSite,
    image: Handle<Image>,
    image_size: Vec2,
) {
    let planet_id = planet.id;
    let Some(size_class) = site.size() else {
        return;
    };
    let visuals = debris_visuals(size_class);
    let mut field_min = Vec2::splat(f32::INFINITY);
    let mut field_max = Vec2::splat(f32::NEG_INFINITY);
    for index in 0..visuals.pieces {
        let seed = site.seed.wrapping_add(index as u32 * 19);
        // Keep wreckage in the open corridor directly left of the planet, beyond the ordinary
        // orbital ring and between the fixed upper- and lower-left range infrastructure. Its
        // depth also keeps a passing orbital from briefly drawing over the wreckage.
        let centered_index = index as f32 - visuals.pieces.saturating_sub(1) as f32 * 0.5;
        let angle = DEBRIS_CENTER_ANGLE
            + centered_index * visuals.angle_step
            + (noise(seed) - 0.5) * DEBRIS_ANGLE_JITTER;
        let radius = planet.size()
            * (visuals.min_radius + noise(seed.wrapping_add(1)) * visuals.radius_jitter);
        let variant = (noise(seed.wrapping_add(2)) * 3.99) as usize;
        let cell = image_size * 0.5;
        let min = Vec2::new((variant % 2) as f32, (variant / 2) as f32) * cell;
        let size = planet.size()
            * (visuals.min_diameter + noise(seed.wrapping_add(3)) * visuals.diameter_jitter);
        let origin = planet.position + Vec2::from_angle(angle) * radius;
        let rotation = noise(seed.wrapping_add(4)) * TAU;
        let amplitude = planet.size() * 0.018;
        // A rotated square is widest at 45 degrees. Include its full possible idle drift so the
        // stable interaction target covers the art without making the art itself hover-sensitive.
        let half_extent = Vec2::splat(size * std::f32::consts::FRAC_1_SQRT_2 + amplitude);
        field_min = field_min.min(origin - half_extent);
        field_max = field_max.max(origin + half_extent);
        commands.spawn((
            Sprite {
                image: image.clone(),
                rect: Some(Rect::from_corners(min, min + cell)),
                custom_size: Some(Vec2::splat(size)),
                ..default()
            },
            Transform {
                translation: origin.extend(PLANET_Z + DEBRIS_DEPTH),
                rotation: Quat::from_rotation_z(rotation),
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
            Detail {
                planet: planet.id,
                opacity: 0.9,
                debris_turn: Some((site.latest_turn, size_class.lifetime_turns())),
            },
            Debris {
                origin,
                rotation,
                phase: noise(seed.wrapping_add(5)) * TAU,
                amplitude,
                speed: 0.22 + noise(seed.wrapping_add(6)) * 0.12,
            },
            MapCmp,
        ));
    }
    // Runtime art is compressed, so it cannot provide alpha-aware sprite picking. A separate,
    // permanently transparent rectangle keeps hover/click behavior stable and prevents pointer
    // state from ever changing the visible debris sprites.
    commands
        .spawn((
            Sprite::from_color(Color::NONE, field_max - field_min),
            Transform::from_translation(
                ((field_min + field_max) * 0.5).extend(PLANET_Z + DEBRIS_DEPTH + 0.001),
            ),
            Visibility::Hidden,
            Pickable::IGNORE,
            DebrisHitTarget {
                planet: planet.id,
                site: site.clone(),
            },
            MapCmp,
        ))
        .observe(cursor::<Over>(SystemCursorIcon::Pointer))
        .observe(cursor::<Out>(SystemCursorIcon::Default))
        .observe(
            move |event: On<Pointer<Click>>,
                  player: Res<Player>,
                  mut state: ResMut<UiState>,
                  game: Res<State<GameState>>| {
                if event.button == PointerButton::Primary && *game.get() == GameState::Playing {
                    open_latest_battle(&player, planet_id, &mut state);
                }
            },
        );
}

fn spawn_light(
    commands: &mut Commands,
    planet: &Planet,
    position_seed: u32,
    appearance_seed: u32,
    offset: Vec2,
    image: Handle<Image>,
) {
    let variant = ((noise(appearance_seed) * SURFACE_LIGHT_VARIANTS as f32) as usize)
        .min(SURFACE_LIGHT_VARIANTS - 1);
    let diameter = planet.size() * (0.042 + noise(appearance_seed.wrapping_add(1)) * 0.025);
    let aspect = 0.68 + noise(appearance_seed.wrapping_add(2)) * 0.72;
    let size = Vec2::new(diameter * aspect.sqrt(), diameter / aspect.sqrt());
    let opacity = 0.96 + noise(appearance_seed.wrapping_add(3)) * 0.04;
    let color = Color::srgba(
        1.0,
        0.88 + noise(appearance_seed.wrapping_add(4)) * 0.10,
        0.62 + noise(appearance_seed.wrapping_add(5)) * 0.28,
        opacity,
    );
    let cell = Vec2::splat(SURFACE_LIGHT_CELL_SIZE as f32);
    let min = Vec2::new(variant as f32 * cell.x, 0.0);
    let rotation = Quat::from_rotation_z(noise(appearance_seed.wrapping_add(6)) * TAU);
    for (scale, layer_color, layer_opacity, depth, backdrop) in [
        (1.28, Color::srgba(0.06, 0.018, 0.002, 0.76), 0.76, 0.115, true),
        (1.0, color, opacity, 0.12, false),
    ] {
        let mut entity = commands.spawn((
            Sprite {
                image: image.clone(),
                rect: Some(Rect::from_corners(min, min + cell)),
                custom_size: Some(size * scale),
                color: layer_color,
                ..default()
            },
            Transform {
                translation: planet.position.extend(PLANET_Z + depth),
                rotation,
                ..default()
            },
            Visibility::Hidden,
            Pickable::IGNORE,
            Detail {
                planet: planet.id,
                opacity: layer_opacity,
                debris_turn: None,
            },
            SurfaceLight {
                center: planet.position,
                diameter: planet.size(),
                position_seed,
                flicker_seed: appearance_seed,
                offset,
                brightness: 0.0,
            },
            MapCmp,
        ));
        if backdrop {
            entity.insert(SurfaceLightBackdrop);
        }
    }
}

fn structure_sprite(
    commands: &mut Commands,
    planet: &Planet,
    art: &DevelopmentArt,
    facilities: bool,
    variant: usize,
    offset: Vec2,
    diameter: f32,
) {
    let (image, cell, columns) = if planet.kind == PlanetKind::Gas {
        (art.gas, art.gas_size * 0.5, 2)
    } else if facilities {
        (art.facilities, art.facilities_size / Vec2::new(3.0, 2.0), 3)
    } else {
        (art.base, art.base_size * 0.5, 2)
    };
    let min = Vec2::new((variant % columns) as f32, (variant / columns) as f32) * cell;
    for (sprite, depth) in [
        (
            Sprite {
                image: art.shadow.clone(),
                custom_size: Some(Vec2::splat(diameter * 1.18)),
                ..default()
            },
            0.13,
        ),
        (
            Sprite {
                image: image.clone(),
                rect: Some(Rect::from_corners(min, min + cell)),
                custom_size: Some(Vec2::splat(diameter)),
                ..default()
            },
            0.14,
        ),
    ] {
        // Suspended gas infrastructure has no footprint on the clouds.
        if planet.kind == PlanetKind::Gas && depth < 0.14 {
            continue;
        }
        let mut entity = commands.spawn((
            sprite,
            Transform::from_translation((planet.position + offset).extend(PLANET_Z + depth)),
            Visibility::Hidden,
            Pickable::IGNORE,
            Detail {
                planet: planet.id,
                opacity: 1.0,
                debris_turn: None,
            },
            MapCmp,
        ));
        if planet.kind == PlanetKind::Gas {
            entity.insert(floating_detail(planet, offset, planet.position + offset));
        }
    }
}

fn moon_structure_sprite(
    commands: &mut Commands,
    planet: &Planet,
    art: &DevelopmentArt,
    building: Building,
    facilities: bool,
    variant: usize,
    offset: Vec2,
    diameter: f32,
) {
    let dedicated = match building {
        Building::Shipyard => Some(art.moon_shipyard),
        Building::TidalGenerator => Some(art.moon_tidal_generator),
        Building::OrbitalRadar => Some(art.moon_orbital_radar),
        _ => None,
    };
    let Some(image) = dedicated else {
        structure_sprite(commands, planet, art, facilities, variant, offset, diameter);
        return;
    };
    for (sprite, depth) in [
        (
            Sprite {
                image: art.shadow.clone(),
                custom_size: Some(Vec2::splat(diameter * 1.18)),
                ..default()
            },
            0.13,
        ),
        (
            Sprite {
                image: image.clone(),
                custom_size: Some(Vec2::splat(diameter)),
                ..default()
            },
            0.14,
        ),
    ] {
        commands.spawn((
            sprite,
            Transform::from_translation((planet.position + offset).extend(PLANET_Z + depth)),
            Visibility::Hidden,
            Pickable::IGNORE,
            Detail {
                planet: planet.id,
                opacity: 1.0,
                debris_turn: None,
            },
            MapCmp,
        ));
    }
}

fn planet_structure_sprite(
    commands: &mut Commands,
    planet: &Planet,
    art: &DevelopmentArt,
    building: Option<Building>,
    facilities: bool,
    variant: usize,
    offset: Vec2,
    diameter: f32,
) {
    if !matches!(building, Some(Building::Terraformer | Building::ColonialAdministration)) {
        structure_sprite(commands, planet, art, facilities, variant, offset, diameter);
        return;
    }
    let gas = planet.kind == PlanetKind::Gas;
    let terraformer_image = match (building, gas) {
        (Some(Building::ColonialAdministration), true) => art.gas_planet_administration,
        (Some(Building::ColonialAdministration), false) => art.planet_administration,
        (_, true) => art.gas_planet_terraformer,
        (_, false) => art.planet_terraformer,
    };
    let terraformer_size = Vec2::new(
        diameter,
        diameter
            * if gas {
                1.0
            } else if building == Some(Building::ColonialAdministration) {
                1137.0 / 1383.0
            } else {
                PLANET_TERRAFORMER_ART_ASPECT
            },
    );
    for (sprite, depth) in [
        (
            Sprite {
                image: art.shadow.clone(),
                custom_size: Some(Vec2::splat(diameter * 1.18)),
                ..default()
            },
            0.13,
        ),
        (
            Sprite {
                image: terraformer_image.clone(),
                custom_size: Some(terraformer_size),
                ..default()
            },
            0.14,
        ),
    ] {
        // Gas giants suspend the facility in their atmosphere and therefore have no footprint.
        if gas && depth < 0.14 {
            continue;
        }
        let mut entity = commands.spawn((
            sprite,
            Transform::from_translation((planet.position + offset).extend(PLANET_Z + depth)),
            Visibility::Hidden,
            Pickable::IGNORE,
            Detail {
                planet: planet.id,
                opacity: 1.0,
                debris_turn: None,
            },
            MapCmp,
        ));
        if gas {
            entity.insert(floating_detail(planet, offset, planet.position + offset));
        }
    }
}

fn planet_structure_spec(
    development: Development,
    building: Building,
    gas: bool,
) -> Option<(usize, Option<Building>, bool, usize)> {
    let spec = match building {
        Building::MetalMine => {
            (development.mining.max(development.refinery).max(development.reactor), None, false, 0)
        },
        Building::Shipyard => {
            (development.shipyard.max(development.factory), None, true, usize::from(gas))
        },
        Building::MissileSilo => (
            development.silo,
            None,
            true,
            if gas {
                2
            } else {
                4
            },
        ),
        Building::Senate => (
            usize::from(development.senate),
            None,
            false,
            if gas {
                3
            } else {
                1
            },
        ),
        Building::Terraformer => (development.terraformer, Some(Building::Terraformer), false, 0),
        Building::ColonialAdministration => {
            (development.administration, Some(Building::ColonialAdministration), false, 0)
        },
        _ => return None,
    };
    (spec.0 > 0).then_some(spec)
}

fn moon_structure_spec(
    development: Development,
    building: Building,
) -> Option<(usize, bool, usize)> {
    let spec = match building {
        Building::LunarBase => (development.lunar_base, false, 2),
        Building::TidalGenerator => (development.tidal_generator, false, 0),
        Building::OrbitalRadar => (usize::from(development.sensor), false, 1),
        Building::Laboratory => (development.laboratory, true, 2),
        Building::Shipyard => (development.shipyard, true, 0),
        _ => return None,
    };
    (spec.0 > 0).then_some(spec)
}

fn spawn_gas_development(
    commands: &mut Commands,
    planet: &Planet,
    development: Development,
    art: &DevelopmentArt,
) {
    // Each completed category has its own slot, entirely inside the planet and clear of icons.
    let slots = [
        Vec2::new(-0.06, 0.27),
        Vec2::new(-0.26, -0.01),
        Vec2::new(-0.05, -0.27),
        Vec2::new(0.13, 0.0),
    ];
    for (building, slot) in development.surface_build_order.into_iter().zip(slots) {
        let Some((_, dedicated, _, variant)) =
            building.and_then(|building| planet_structure_spec(development, building, true))
        else {
            continue;
        };
        let offset = slot * planet.size();
        let diameter = 0.29 * planet.size();
        planet_structure_sprite(commands, planet, art, dedicated, false, variant, offset, diameter);
        // Beacons stay attached to the platform instead of migrating across the gas surface.
        for (index, lamp) in [Vec2::new(-0.31, -0.13), Vec2::new(0.32, -0.19), Vec2::new(0.0, 0.42)]
            .into_iter()
            .enumerate()
        {
            let origin = planet.position + offset + lamp * diameter;
            for (size, opacity) in [(0.06, 0.2), (0.018, 1.0)] {
                commands.spawn((
                    Sprite::from_color(
                        Color::srgba(0.45, 0.9, 1.0, opacity),
                        Vec2::splat(diameter * size),
                    ),
                    Transform::from_translation(origin.extend(PLANET_Z + 0.16)),
                    Visibility::Hidden,
                    Pickable::IGNORE,
                    Detail {
                        planet: planet.id,
                        opacity,
                        debris_turn: None,
                    },
                    floating_detail(planet, offset, origin),
                    PlatformLight {
                        phase: index as f32 * 2.1,
                        brightness: 1.0,
                    },
                    MapCmp,
                ));
            }
        }
    }
}

fn spawn_development(
    commands: &mut Commands,
    planet: &Planet,
    development: Development,
    art: &DevelopmentArt,
) {
    let size = planet.size();
    if planet.kind == PlanetKind::Gas {
        spawn_gas_development(commands, planet, development, art);
        return;
    }
    let seed = (planet.id as u32).wrapping_mul(43);
    {
        // A few uneven clusters read as settlements without carpeting the whole surface.
        for index in 0..development.settlement * 3 {
            let position_seed = seed.wrapping_add(index as u32 * 11);
            let lamp_count = 2 + usize::from(noise(position_seed.wrapping_add(7)) > 0.58);
            for lamp in 0..lamp_count {
                let appearance_seed =
                    position_seed.wrapping_add((lamp as u32 + 1).wrapping_mul(0x85eb_ca6b));
                let angle = noise(appearance_seed.wrapping_add(8)) * TAU;
                let spacing = size
                    * (0.006
                        + lamp as f32 * (0.009 + noise(appearance_seed.wrapping_add(9)) * 0.008));
                let offset = Vec2::from_angle(angle) * spacing;
                spawn_light(
                    commands,
                    planet,
                    position_seed,
                    appearance_seed,
                    offset,
                    art.surface_lights.clone(),
                );
            }
        }
    }
    // Leave the right-hand status icon column clear, including on smaller moons. Stored slots are
    // never compacted, so destroyed structures leave holes instead of moving their neighbors.
    let slots = [
        Vec2::new(-0.08, 0.30),
        Vec2::new(-0.30, 0.0),
        Vec2::new(-0.08, -0.30),
        Vec2::new(0.12, 0.0),
    ];
    if planet.is_moon() {
        for (building, slot) in
            development.surface_build_order.into_iter().zip(slots.into_iter().take(3))
        {
            let Some(building) = building else {
                continue;
            };
            let Some((level, facilities, variant)) = moon_structure_spec(development, building)
            else {
                continue;
            };
            let offset = slot * size;
            let diameter = size * (0.285 + level as f32 * 0.018);
            moon_structure_sprite(
                commands, planet, art, building, facilities, variant, offset, diameter,
            );
        }
        return;
    }
    for (building, slot) in development.surface_build_order.into_iter().zip(slots) {
        let Some((level, dedicated, facilities, variant)) =
            building.and_then(|building| planet_structure_spec(development, building, false))
        else {
            continue;
        };
        let offset = slot * size;
        let diameter = size * (0.285 + level as f32 * 0.018);
        planet_structure_sprite(
            commands, planet, art, dedicated, facilities, variant, offset, diameter,
        );
    }
}

fn refresh_details(
    mut commands: Commands,
    mut cache: ResMut<DetailCache>,
    details: Query<(Entity, &Detail)>,
    debris_hit_targets: Query<(Entity, &DebrisHitTarget)>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    settings: Res<Settings>,
    assets: Res<WorldAssets>,
    images: Res<Assets<Image>>,
    shadow: Res<StructureShadow>,
    surface_lights: Res<SurfaceLightAtlas>,
) {
    if !map.is_changed()
        && !player.is_changed()
        && !missions.is_changed()
        && !session.is_changed()
        && cache.turn == Some(settings.turn)
        && cache.assets_ready
    {
        return;
    }
    let development_image = assets.image("development");
    let Some(development_size) = images.get(&development_image).map(|image| image.size().as_vec2())
    else {
        return;
    };
    let facilities_image = assets.image("facilities");
    let Some(facilities_size) = images.get(&facilities_image).map(|image| image.size().as_vec2())
    else {
        return;
    };
    let gas_image = assets.image("gas-development");
    let Some(gas_size) = images.get(&gas_image).map(|image| image.size().as_vec2()) else {
        return;
    };
    let moon_shipyard = assets.image("moon shipyard");
    let moon_tidal_generator = assets.image("moon tidal generator");
    let moon_orbital_radar = assets.image("moon orbital radar");
    let planet_terraformer = assets.image("planet terraformer");
    let gas_planet_terraformer = assets.image("gas planet terraformer");
    let planet_administration = assets.image("planet colonial administration");
    let gas_planet_administration = assets.image("gas planet colonial administration");
    let art = DevelopmentArt {
        base: &development_image,
        base_size: development_size,
        facilities: &facilities_image,
        facilities_size,
        gas: &gas_image,
        gas_size,
        moon_shipyard: &moon_shipyard,
        moon_tidal_generator: &moon_tidal_generator,
        moon_orbital_radar: &moon_orbital_radar,
        planet_terraformer: &planet_terraformer,
        gas_planet_terraformer: &gas_planet_terraformer,
        planet_administration: &planet_administration,
        gas_planet_administration: &gas_planet_administration,
        surface_lights: &surface_lights.0,
        shadow: &shadow.0,
    };
    // Canonical reports already persist on participants. Deduplicate their copies, exposing only
    // coarse debris to other players without copying reports into their intelligence history.
    let sites = if let Some(record) = &session.active_game {
        debris_sites(
            record.persisted.state.players.iter().flat_map(|player| player.reports.iter()),
            settings.turn,
        )
    } else {
        debris_sites(player.reports.iter(), settings.turn)
    };
    let development = map
        .planets
        .iter()
        .map(|planet| {
            let info =
                (!player.controls(planet)).then(|| player.last_info(planet, &missions.0)).flatten();
            let army = if player.controls(planet) {
                Some(&planet.army)
            } else {
                info.as_ref().map(|info| &info.army)
            };
            (planet.id, development(planet, army))
        })
        .collect::<BTreeMap<_, _>>();
    for (entity, detail) in &details {
        let changed = if detail.debris_turn.is_some() {
            cache.debris.get(&detail.planet) != sites.get(&detail.planet)
        } else {
            cache.development.get(&detail.planet) != development.get(&detail.planet)
        };
        if changed {
            commands.entity(entity).despawn();
        }
    }
    for (entity, target) in &debris_hit_targets {
        if cache.debris.get(&target.planet) != sites.get(&target.planet) {
            commands.entity(entity).despawn();
        }
    }
    let image = assets.image("wreckage");
    let image_size = images.get(&image).map(|image| image.size().as_vec2());
    if let Some(image_size) = image_size {
        cache.assets_ready = true;
        for (&id, site) in &sites {
            if cache.debris.get(&id) != Some(site) {
                if let Some(planet) = map.try_get(id) {
                    spawn_debris(&mut commands, planet, site, image.clone(), image_size);
                }
            }
        }
        if cache.debris != sites {
            cache.debris = sites;
        }
    }
    for (&id, &development) in &development {
        if cache.development.get(&id) != Some(&development) {
            if let Some(planet) = map.try_get(id) {
                spawn_development(&mut commands, planet, development, &art);
            }
        }
    }
    if cache.development != development {
        cache.development = development;
    }
    cache.turn = Some(settings.turn);
}

fn fade_details(
    camera: Single<&Projection, With<MainCamera>>,
    time: Res<Time<Real>>,
    mut development_visibility: Local<DevelopmentVisibility>,
    settings: Res<Settings>,
    game: Res<State<GameState>>,
    mut details: Query<(
        &Detail,
        &mut Sprite,
        &mut Visibility,
        Option<&SurfaceLight>,
        Option<&PlatformLight>,
    )>,
    mut debris_hit_targets: Query<
        (&DebrisHitTarget, &mut Visibility, &mut Pickable),
        Without<Detail>,
    >,
) {
    let scale = match *camera {
        Projection::Orthographic(ref projection) => projection.scale,
        _ => f32::INFINITY,
    };
    let development_alpha =
        development_visibility.update(scale <= DEVELOPMENT_MAX_SCALE, time.delta_secs());
    for (detail, mut sprite, mut visibility, light, platform_light) in &mut details {
        let alpha = if detail.debris_turn.is_some() {
            detail_alpha(scale)
        } else {
            development_alpha
        };
        let age_alpha = detail.debris_turn.map_or(1.0, |(turn, lifetime)| {
            settings
                .turn
                .checked_sub(turn)
                .filter(|age| *age < lifetime)
                .map_or(0.0, |age| 1.0 - age as f32 / lifetime.max(1) as f32 * 0.72)
        });
        sprite.color.set_alpha(
            detail.opacity
                * alpha
                * age_alpha
                * light.map_or(1.0, |light| light.brightness)
                * platform_light.map_or(1.0, |light| light.brightness),
        );
        *visibility = if alpha * age_alpha > 0.01 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    for (target, mut visibility, mut pickable) in &mut debris_hit_targets {
        let alpha = detail_alpha(scale);
        let lifetime = target.site.size().map_or(0, DebrisSize::lifetime_turns);
        let age_alpha = settings
            .turn
            .checked_sub(target.site.latest_turn)
            .filter(|age| *age < lifetime)
            .map_or(0.0, |age| 1.0 - age as f32 / lifetime.max(1) as f32 * 0.72);
        let interactive = alpha > 0.15 && age_alpha > 0.0 && *game.get() == GameState::Playing;
        *visibility = if interactive {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        *pickable = if interactive {
            Pickable::default()
        } else {
            Pickable::IGNORE
        };
    }
}

pub(crate) struct MapDetailsPlugin;

impl Plugin for MapDetailsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DetailCache>()
            .init_resource::<StructureShadow>()
            .init_resource::<SurfaceLightAtlas>()
            .init_resource::<DetailAnimationTime>()
            .add_systems(OnEnter(AppState::Game), |mut cache: ResMut<DetailCache>| {
                *cache = DetailCache::default()
            })
            .add_systems(
                Update,
                (
                    refresh_details,
                    animate_surface_lights,
                    animate_floating_development,
                    animate_debris,
                    fade_details,
                )
                    .chain()
                    .after(refresh_gameplay_projection)
                    .after(refresh_turn_draft)
                    .run_if(in_state(AppState::Game)),
            );
    }
}

#[cfg(test)]
#[path = "../../../tests/core/map_details.rs"]
mod tests;
