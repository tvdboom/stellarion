//! Close-zoom development and public battle debris; neither changes simulation state.

use std::collections::{BTreeMap, BTreeSet};
use std::f32::consts::TAU;

use bevy::prelude::*;
use bevy::window::SystemCursorIcon;

use super::model::{Map, MapCmp};
use super::planet::{Planet, PlanetId, PlanetKind};
use super::utils::cursor;
use crate::core::assets::WorldAssets;
use crate::core::camera::MainCamera;
use crate::core::combat::report::{MissionReport, Side};
use crate::core::constants::PLANET_Z;
use crate::core::loading::{refresh_gameplay_projection, refresh_turn_draft};
use crate::core::missions::Missions;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Unit};
use crate::multiplayer::client::MultiplayerSession;

const DEBRIS_TURNS: usize = 3;

/// A coarse public trace, deliberately containing no army composition or intelligence.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct DebrisSite {
    losses: usize,
    latest_turn: usize,
    seed: u32,
}

fn destroyed_units(report: &MissionReport) -> usize {
    if report.combat_report.is_none() {
        return 0;
    }
    [
        (&report.mission.army, &report.surviving_attacker),
        (&report.planet.army, &report.surviving_defender),
    ]
    .into_iter()
    .flat_map(|(before, after)| {
        // Consumed missiles, colony ships and demolished buildings aren't orbital wrecks.
        before
            .iter()
            .filter(|(unit, _)| unit.is_ship() && **unit != Unit::colony_ship())
            .map(move |(unit, count)| count.saturating_sub(after.amount(unit)))
    })
    .fold(0usize, usize::saturating_add)
}

fn debris_sites<'a>(
    reports: impl Iterator<Item = &'a MissionReport>,
    turn: usize,
) -> BTreeMap<PlanetId, DebrisSite> {
    let mut seen = BTreeSet::new();
    let mut sites = BTreeMap::<PlanetId, DebrisSite>::new();
    for report in reports {
        if !seen.insert(report.id)
            || turn.checked_sub(report.turn).is_none_or(|age| age >= DEBRIS_TURNS)
        {
            continue;
        }
        let losses = destroyed_units(report);
        if losses == 0 {
            continue;
        }
        let site = sites.entry(report.mission.destination).or_default();
        site.losses = site.losses.saturating_add(losses);
        site.latest_turn = site.latest_turn.max(report.turn);
        site.seed ^= report.id as u32;
    }
    sites
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
struct DevelopmentVisibility(f32);

impl DevelopmentVisibility {
    fn update(&mut self, visible: bool, delta_secs: f32) -> f32 {
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

fn debris_count(losses: usize) -> usize {
    if losses == 0 {
        0
    } else {
        (1 + losses.ilog2() as usize / 2).min(5)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Development {
    settlement: usize,
    mining: usize,
    refinery: usize,
    factory: usize,
    shipyard: usize,
    reactor: usize,
    laboratory: usize,
    silo: usize,
    lunar_base: usize,
    sensor: bool,
    lunar_build_order: [Option<Building>; 4],
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
        shipyard: tier(level(Building::Shipyard)),
        reactor: tier(level(Building::Reactor)),
        laboratory: tier(level(Building::Laboratory)),
        silo: tier(level(Building::MissileSilo)),
        lunar_base: tier(level(Building::LunarBase)),
        sensor: level(Building::SensorPhalanx) > 0 || level(Building::OrbitalRadar) > 0,
        lunar_build_order: planet
            .lunar_build_order
            .map(|building| building.filter(|b| level(*b) > 0)),
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
    debris_turn: Option<usize>,
}

#[derive(Component)]
struct Debris {
    origin: Vec2,
    rotation: f32,
    phase: f32,
    amplitude: f32,
    speed: f32,
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
    seed: u32,
    offset: Vec2,
    brightness: f32,
}

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

struct DevelopmentArt<'a> {
    base: &'a Handle<Image>,
    base_size: Vec2,
    facilities: &'a Handle<Image>,
    facilities_size: Vec2,
    gas: &'a Handle<Image>,
    gas_size: Vec2,
    shadow: &'a Handle<Image>,
}

/// Selects a new surface position only while the cluster is fully dark.
fn light_sample(seed: u32, seconds: f32) -> (Vec2, f32) {
    let phase = seconds / 14.0 + noise(seed);
    let cycle_seed = seed.wrapping_add((phase.floor() as u32).wrapping_mul(0x9e37_79b9));
    let position = Vec2::from_angle(noise(cycle_seed) * TAU)
        * (0.08 + noise(cycle_seed.wrapping_add(1)) * 0.3);
    let progress = phase.fract();
    let smooth = |value: f32| {
        let x = value.clamp(0.0, 1.0);
        x * x * (3.0 - 2.0 * x)
    };
    let envelope = smooth(progress / 0.12) * smooth((1.0 - progress) / 0.12);
    let flicker = 0.78 + 0.22 * (seconds * 1.8 + noise(seed.wrapping_add(2)) * TAU).sin().powi(2);
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
        let (position, brightness) = light_sample(light.seed, elapsed.0);
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
    if report.can_see(&Side::Attacker, player.id) && report.can_see(&Side::Defender, player.id) {
        state.mission = false;
        state.combat_report = Some(report.id);
        state.combat_report_round = 1;
        state.combat_report_total = true;
        state.combat_report_hover = None;
    } else {
        // A lost fleet can leave public wreckage without granting enemy-unit intelligence.
        state.combat_report = None;
        state.mission = true;
        state.mission_tab = MissionTab::MissionReports;
        state.mission_report = Some(report.mission.id);
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
    for index in 0..debris_count(site.losses) {
        let seed = site.seed.wrapping_add(index as u32 * 19);
        // The lower-left arc avoids the name above and the existing right-side status icons.
        let angle = 3.45 + index as f32 * 0.29 + noise(seed) * 0.15;
        let radius = planet.size() * (0.78 + noise(seed.wrapping_add(1)) * 0.15);
        let variant = (noise(seed.wrapping_add(2)) * 3.99) as usize;
        let cell = image_size * 0.5;
        let min = Vec2::new((variant % 2) as f32, (variant / 2) as f32) * cell;
        let size = planet.size() * (0.26 + noise(seed.wrapping_add(3)) * 0.09);
        let origin = planet.position + Vec2::from_angle(angle) * radius;
        let rotation = noise(seed.wrapping_add(4)) * TAU;
        commands
            .spawn((
                Sprite {
                    image: image.clone(),
                    rect: Some(Rect::from_corners(min, min + cell)),
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform {
                    translation: origin.extend(PLANET_Z + 0.15),
                    rotation: Quat::from_rotation_z(rotation),
                    ..default()
                },
                Visibility::Hidden,
                Pickable::IGNORE,
                Detail {
                    planet: planet.id,
                    opacity: 0.9,
                    debris_turn: Some(site.latest_turn),
                },
                Debris {
                    origin,
                    rotation,
                    phase: noise(seed.wrapping_add(5)) * TAU,
                    amplitude: planet.size() * 0.018,
                    speed: 0.22 + noise(seed.wrapping_add(6)) * 0.12,
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
}

fn spawn_light(
    commands: &mut Commands,
    planet: &Planet,
    seed: u32,
    offset: Vec2,
    size: Vec2,
    color: Color,
) {
    commands.spawn((
        Sprite::from_color(color, size),
        Transform::from_translation(planet.position.extend(PLANET_Z + 0.12)),
        Visibility::Hidden,
        Pickable::IGNORE,
        Detail {
            planet: planet.id,
            opacity: color.alpha(),
            debris_turn: None,
        },
        SurfaceLight {
            center: planet.position,
            diameter: planet.size(),
            seed,
            offset,
            brightness: 0.0,
        },
        MapCmp,
    ));
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
    for ((_, variant), slot) in [
        (development.mining.max(development.refinery).max(development.reactor), 0),
        (development.shipyard.max(development.factory), 1),
        (development.silo, 2),
        (usize::from(development.sensor), 3),
    ]
    .into_iter()
    .zip(slots)
    .filter(|((level, _), _)| *level > 0)
    {
        let offset = slot * planet.size();
        let diameter = 0.29 * planet.size();
        structure_sprite(commands, planet, art, false, variant, offset, diameter);
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
        for index in 0..development.settlement * 4 {
            let seed = seed.wrapping_add(index as u32 * 11);
            for lamp in 0..3 {
                let offset =
                    Vec2::new(lamp as f32 * size * 0.017, (lamp % 2) as f32 * size * 0.012);
                spawn_light(
                    commands,
                    planet,
                    seed,
                    offset,
                    Vec2::splat(size * 0.038),
                    Color::srgba(1.0, 0.66, 0.22, 0.22),
                );
                spawn_light(
                    commands,
                    planet,
                    seed,
                    offset,
                    Vec2::new(size * 0.016, size * 0.012),
                    Color::srgba(1.0, 0.93, 0.7, 1.0),
                );
            }
        }
    }
    // Leave the right-hand status icon column clear, including on smaller moons.
    // The fixed priority shows a stable sample of completed buildings within the image budget.
    let slots = [
        Vec2::new(-0.08, 0.30),
        Vec2::new(-0.30, 0.0),
        Vec2::new(-0.08, -0.30),
        Vec2::new(0.12, 0.0),
    ];
    let limit = if planet.is_moon() {
        3
    } else {
        4
    };
    let structures = if planet.is_moon() {
        let mut lunar = [
            (Building::LunarBase, development.lunar_base, false, 2),
            (Building::OrbitalRadar, usize::from(development.sensor), false, 1),
            (Building::Laboratory, development.laboratory, true, 2),
            (Building::Shipyard, development.shipyard, true, 0),
        ];
        lunar.sort_by_key(|(building, _, _, _)| {
            development
                .lunar_build_order
                .iter()
                .position(|entry| *entry == Some(*building))
                .unwrap_or(4)
        });
        lunar.map(|(_, level, facilities, variant)| (level, facilities, variant))
    } else {
        [
            (development.mining.max(development.refinery).max(development.reactor), false, 0),
            (development.shipyard.max(development.factory), true, 0),
            (development.silo, true, 4),
            (usize::from(development.sensor), false, 1),
        ]
    };
    for ((level, facilities, variant), slot) in
        structures.into_iter().filter(|(level, _, _)| *level > 0).zip(slots.into_iter().take(limit))
    {
        let offset = slot * size;
        let diameter = size * (0.285 + level as f32 * 0.018);
        structure_sprite(commands, planet, art, facilities, variant, offset, diameter);
    }
}

fn refresh_details(
    mut commands: Commands,
    mut cache: ResMut<DetailCache>,
    details: Query<(Entity, &Detail)>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    settings: Res<Settings>,
    assets: Res<WorldAssets>,
    images: Res<Assets<Image>>,
    shadow: Res<StructureShadow>,
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
    let art = DevelopmentArt {
        base: &development_image,
        base_size: development_size,
        facilities: &facilities_image,
        facilities_size,
        gas: &gas_image,
        gas_size,
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
        &mut Pickable,
        Option<&SurfaceLight>,
        Option<&PlatformLight>,
    )>,
) {
    let scale = match *camera {
        Projection::Orthographic(ref projection) => projection.scale,
        _ => f32::INFINITY,
    };
    let development_alpha = development_visibility.update(scale <= 0.9, time.delta_secs());
    for (detail, mut sprite, mut visibility, mut pickable, light, platform_light) in &mut details {
        let alpha = if detail.debris_turn.is_some() {
            detail_alpha(scale)
        } else {
            development_alpha
        };
        let age_alpha = detail.debris_turn.map_or(1.0, |turn| {
            settings
                .turn
                .checked_sub(turn)
                .filter(|age| *age < DEBRIS_TURNS)
                .map_or(0.0, |age| 1.0 - age as f32 * 0.24)
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
        // The rendered sprite rectangle is the hit area. No extra invisible hit target or tooltip.
        *pickable = if detail.debris_turn.is_some()
            && alpha > 0.15
            && age_alpha > 0.0
            && *game.get() == GameState::Playing
        {
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
