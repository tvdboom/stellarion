//! Opt-in README captures using the complete application and a seeded, offline match.
//!
//! Run with STELLARION_BACKEND=mock:
//! cargo test --lib render_readme_screenshots -- --ignored --nocapture
//! Review target/readme-screenshots before copying the six PNGs into scenery/.

use crate::core::camera::MainCamera;
use crate::core::combat::plan::{BattlePlan, CombatTactic, TargetPriority};
use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::identity::{GameCode, GameId, UserId};
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::missions::Mission;
use crate::core::player::Player;
use crate::core::resources::Resources;
use crate::core::settings::Settings;
use crate::core::simulation::{GameModel, GameRules, MatchStatus, PersistedGame};
use crate::core::states::{AppState, AudioState, GameState};
use crate::core::ui::systems::{MissionTab, Shop, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::fauna::SpaceFauna;
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Unit};
use crate::multiplayer::client::{ConnectionStatus, MultiplayerSession};
use crate::multiplayer::model::{GameMembership, GameRecord};
use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;
use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
use bevy::window::WindowResolution;
use bevy::winit::{WinitPlugin, WinitSettings};
use bevy_egui::{egui, EguiInput, EguiPlugin, EguiPreUpdateSet};
use bevy_kira_audio::AudioPlugin;
use bevy_tweening::TweeningPlugin;
use std::time::Duration;

#[derive(Resource)]
struct CaptureScene {
    model: GameModel,
    origin: usize,
    destination: usize,
    camera: Vec2,
    frame: usize,
}

fn fleet(counts: &[(Ship, usize)]) -> Army {
    counts.iter().map(|(ship, count)| (Unit::Ship(*ship), *count)).collect()
}

fn name_world(map: &mut Map, id: usize, name: &str) {
    let previous = map.get(id).name.clone();
    if let Some(existing) = map.planets.iter_mut().find(|planet| planet.name == name) {
        existing.name = previous;
    }
    map.get_mut(id).name = name.into();
}

fn scene() -> CaptureScene {
    let mut model = GameModel::new(
        [73; 32],
        GameRules {
            player_count: 3,
            planets_per_player: 12,
            colonizable_percent: 35,
            moons_percent: 20,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    model.turn = 24;
    for planet in &mut model.map.planets {
        planet.owned = None;
        planet.controlled = None;
        planet.army.clear();
    }
    let center = model.map.rect.center();
    let mut occupied = std::collections::BTreeSet::new();
    let mut colonies = Vec::new();
    for (slot, (offset, count)) in
        [(Vec2::new(-550., 200.), 6), (Vec2::new(650., 550.), 5), (Vec2::new(100., -950.), 4)]
            .into_iter()
            .enumerate()
    {
        let owner = slot as u64 + 1;
        let mut worlds = model
            .map
            .planets
            .iter()
            .filter(|planet| !planet.is_moon() && !occupied.contains(&planet.id))
            .map(|planet| (planet.id, planet.position.distance_squared(center + offset)))
            .collect::<Vec<_>>();
        worlds.sort_by(|a, b| a.1.total_cmp(&b.1));
        let ids = worlds.iter().take(count).map(|(id, _)| *id).collect::<Vec<_>>();
        let player = &mut model.players[slot];
        player.home_planet = ids[0];
        player.world_acquisition_order = ids.clone();
        player.resources = [
            Resources {
                metal: 6840,
                crystal: 4275,
                deuterium: 2910,
            },
            Resources {
                metal: 3925,
                crystal: 6150,
                deuterium: 1840,
            },
            Resources {
                metal: 8140,
                crystal: 2865,
                deuterium: 3560,
            },
        ][slot];
        for (index, id) in ids.iter().copied().enumerate() {
            occupied.insert(id);
            let planet = model.map.get_mut(id);
            planet.owned = Some(owner);
            planet.controlled = Some(owner);
            let level = if index == 0 {
                4
            } else {
                2 + index % 2
            };
            for (building, amount) in [
                (Building::MetalMine, level),
                (Building::CrystalMine, level.saturating_sub(index % 2)),
                (Building::DeuteriumSynthesizer, 2 + index % 2),
                (Building::Factory, 3),
                (
                    Building::Shipyard,
                    if index == 0 {
                        5
                    } else {
                        3
                    },
                ),
                (Building::SolarSatellite, 4),
                (Building::Reactor, 2),
                (Building::ColonialAdministration, usize::from(index > 0)),
            ] {
                if amount > 0 {
                    planet.army.insert(Unit::Building(building), amount);
                }
            }
            if index == 0 {
                planet.army.insert(Unit::Building(Building::Senate), 2);
                planet.army.insert(Unit::Building(Building::SensorPhalanx), 2);
                planet.army.insert(Unit::Building(Building::CommandRelay), 1);
                planet.army.insert(Unit::Building(Building::PlanetaryShield), 2);
            }
            if index < 2 {
                planet.army.insert(Unit::Building(Building::TradingPost), 1);
                planet.army.insert(Unit::space_dock(), 1);
                planet.army.insert(Unit::Building(Building::Recycler), 2);
            }
            for (unit, amount) in fleet(&[
                (Ship::Probe, 7 + index * 2 + slot),
                (Ship::ColonyShip, usize::from(index == 0)),
                (Ship::LightFighter, 18 - index * 2 + slot * 3),
                (Ship::HeavyFighter, 9 - index + slot),
                (Ship::Destroyer, 6 - index + slot),
                (Ship::Cruiser, 4 - index.min(3)),
                (
                    Ship::Battleship,
                    if index < 2 {
                        3 - index
                    } else {
                        0
                    },
                ),
                (Ship::Bomber, usize::from(index == 0) * 2),
                (Ship::Dreadnought, usize::from(index == 0)),
            ]) {
                if amount > 0 {
                    planet.army.insert(unit, amount);
                }
            }
            for (defense, amount) in [
                (Defense::RocketLauncher, 8 + index * 2),
                (Defense::LightLaser, 5 + slot),
                (Defense::HeavyLaser, 3),
                (Defense::GaussCannon, usize::from(index < 2) * 2),
                (Defense::IonCannon, usize::from(index < 2)),
                (Defense::RepairTruck, usize::from(index < 2) * 2),
            ] {
                if amount > 0 {
                    planet.army.insert(Unit::Defense(defense), amount);
                }
            }
        }
        colonies.push(ids);
    }
    let origin = colonies[0][0];
    let origin_position = model.map.get(origin).position;
    let destination = colonies[1][1];
    model.map.get_mut(destination).army.insert(Unit::Building(Building::PlanetaryShield), 1);
    name_world(&mut model.map, origin, "Aegis");
    name_world(&mut model.map, colonies[0][1], "Orion");
    name_world(&mut model.map, colonies[0][2], "Syron");
    name_world(&mut model.map, destination, "Ghorin");
    // The report is resolved by the same combat rules as a normal submitted turn.
    let attack = Mission {
        id: 71,
        owner: 1,
        origin,
        origin_owned: Some(1),
        origin_controlled: Some(1),
        origin_army: model.map.get(origin).army.controller().clone(),
        destination,
        position: model.map.get(destination).position,
        send: 20,
        travel_turns: 3,
        objective: Icon::Attack,
        army: fleet(&[
            (Ship::LightFighter, 34),
            (Ship::HeavyFighter, 16),
            (Ship::Destroyer, 10),
            (Ship::Cruiser, 7),
            (Ship::Battleship, 5),
            (Ship::Bomber, 3),
            (Ship::Dreadnought, 1),
        ]),
        battle_plan: BattlePlan {
            automatic_tactic: false,
            tactic: CombatTactic::Breakthrough,
            target_priority: TargetPriority::HeavyTargets,
            ..default()
        },
        ..default()
    };
    let report =
        resolve_combat_with_rng(23, &attack, model.map.get(destination), &mut model.rng.next_rng());
    assert!(report.combat_report.as_ref().unwrap().rounds.len() > 2);
    // Keep this a past engagement; its surviving units and ownership become the live map.
    let fought = model.map.get_mut(destination);
    fought.owned = report.destination_owned;
    fought.controlled = report.destination_controlled;
    fought.army = report.surviving_defender.clone();
    if report.destination_controlled == Some(1) {
        *fought.army.controller_mut() = report.surviving_attacker.clone();
        model.players[0].record_world_acquisition(destination);
    }
    model.players[0].push_report(report.clone());
    model.players[1].push_report(report);
    let next_target =
        *colonies[1].iter().skip(1).find(|id| model.map.get(**id).controlled == Some(2)).unwrap();
    for (id, from, to, objective, army, elapsed) in [
        (
            81,
            origin,
            colonies[0][2],
            Icon::Deploy,
            fleet(&[(Ship::LightFighter, 8), (Ship::HeavyFighter, 3)]),
            1,
        ),
        (82, origin, colonies[2][1], Icon::Spy, fleet(&[(Ship::Probe, 7)]), 1),
        (
            83,
            colonies[0][1],
            next_target,
            Icon::Attack,
            fleet(&[(Ship::Destroyer, 4), (Ship::Cruiser, 2)]),
            2,
        ),
        (
            84,
            colonies[0][3],
            *model
                .map
                .planets
                .iter()
                .find(|p| !p.is_moon() && p.owned.is_none() && p.controlled.is_none())
                .map(|p| &p.id)
                .unwrap(),
            Icon::Colonize,
            fleet(&[(Ship::ColonyShip, 1), (Ship::LightFighter, 4)]),
            1,
        ),
    ] {
        let start = model.map.get(from);
        let mut mission = Mission {
            id,
            owner: 1,
            origin: from,
            origin_owned: Some(1),
            origin_controlled: Some(1),
            origin_army: start.army.controller().clone(),
            destination: to,
            position: start.position,
            send: 24 - elapsed,
            objective,
            army,
            ..default()
        };
        // Do not include a fleet that has already arrived in a list of active missions.
        for _ in 0..elapsed.min(mission.duration(&model.map).saturating_sub(1)) {
            mission.advance(&model.map);
        }
        mission.send = 24 - mission.travel_turns;
        // Preserve only fleets still travelling, as a real turn projection would.
        if mission.distance(&model.map) > 0.0 {
            model.missions.push(mission);
        }
    }
    model.validate().unwrap();
    println!(
        "Turn {}: {} active missions, battle outcome {}, colony at {:?}",
        model.turn,
        model.missions.len(),
        model.players[0].reports[0].status(&model.players[0]),
        origin_position,
    );
    CaptureScene {
        camera: origin_position + Vec2::new(260., 50.),
        model,
        origin,
        destination: next_target,
        frame: 0,
    }
}

#[test]
#[ignore = "requires generated assets and a GPU; captures all six README views offline"]
fn render_readme_screenshots() {
    assert_eq!(
        std::env::var("STELLARION_BACKEND").as_deref(),
        Ok("mock"),
        "Set STELLARION_BACKEND=mock for this offline fixture"
    );
    std::fs::create_dir_all("target/readme-screenshots").unwrap();
    let mut app = capture_app();
    app.add_systems(Last, capture_views);
    app.run();
}

#[test]
#[ignore = "requires generated assets and a GPU; captures raid report layout offline"]
fn render_raid_report_screenshots() {
    assert_eq!(std::env::var("STELLARION_BACKEND").as_deref(), Ok("mock"));
    std::fs::create_dir_all("target/raid-report-preview").unwrap();
    let mut app = capture_app();
    app.add_systems(Last, |world: &mut World, mut frame: Local<usize>| {
        if *world.resource::<State<AppState>>().get() != AppState::Game {
            return;
        }
        *frame += 1;
        if *frame == 80 {
            let mut player = world.resource_mut::<Player>();
            let report = &mut player.reports[0];
            report.mission.objective = Icon::Raid;
            report.raid_loot = Resources::new(61, 19, 4);
            let report_id = report.id;
            let mut ui = world.resource_mut::<UiState>();
            ui.planet_selected = None;
            ui.mission = true;
            ui.mission_tab = MissionTab::MissionReports;
            ui.mission_report = Some(report_id);
        } else if *frame == 160 {
            world.query::<&mut Window>().single_mut(world).unwrap().resolution.set(800.0, 600.0);
        } else if *frame == 240 {
            let mut player = world.resource_mut::<Player>();
            player.reports[0].raid_loot = Resources::new(1_234_567, 8_901_234, 5_678_901);
            player.reports[0].scout_probes = 2;
        }
        let name = match *frame {
            120 => "report",
            200 => "report-small",
            280 => "large-totals",
            _ => return,
        };
        let mut capture = world.spawn(Screenshot::primary_window());
        capture.observe(save_to_disk(format!("target/raid-report-preview/{name}.png")));
        if *frame == 280 {
            capture.observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            });
        }
    });
    app.run();
}

#[test]
#[ignore = "requires generated assets and a GPU; captures raid loot layering offline"]
fn render_raid_loot_screenshots() {
    use crate::core::combat::systems::SalvagePickupCmp;
    use crate::core::random::DeterministicRngState;

    assert_eq!(std::env::var("STELLARION_BACKEND").as_deref(), Ok("mock"));
    std::fs::create_dir_all("target/raid-loot-preview").unwrap();
    let mut app = capture_app();
    app.add_systems(Last, |world: &mut World, mut loot_frame: Local<Option<usize>>| {
        if *world.resource::<State<AppState>>().get() != AppState::Game {
            return;
        }
        let frame = {
            let mut scene = world.resource_mut::<CaptureScene>();
            scene.frame += 1;
            scene.frame
        };
        assert!(
            frame < 1_200,
            "raid capture timed out: game {:?}, combat {:?}, selected {:?}",
            world.resource::<State<GameState>>().get(),
            world.resource::<State<crate::core::states::CombatState>>().get(),
            world.resource::<UiState>().in_combat,
        );
        if frame == 80 {
            let mut target = world.resource::<Player>().reports[0].planet.clone();
            target.colonize(2);
            target.army = fleet(&[(Ship::LightFighter, 2)]).into();
            let mission = Mission {
                objective: Icon::Raid,
                destination: target.id,
                army: fleet(&[
                    (Ship::LightFighter, 3),
                    (Ship::HeavyFighter, 3),
                    (Ship::Destroyer, 3),
                    (Ship::Battleship, 3),
                ]),
                ..world.resource::<Player>().reports[0].mission.clone()
            };
            let mut rng = DeterministicRngState::from_u64(19).next_rng();
            let mut report = resolve_combat_with_rng(24, &mission, &target, &mut rng);
            report.raid_loot = Resources::new(24, 16, 9);
            world.resource_mut::<UiState>().in_combat = Some(report.id);
            world.resource_mut::<Player>().reports = vec![report];
            let mut settings = world.resource_mut::<Settings>();
            settings.combat_view = crate::core::settings::CombatView::Schematic;
            settings.combat_paused = false;
            settings.combat_speed = 4.0;
            settings.combat_volley_fire = true;
            world.resource_mut::<NextState<GameState>>().set(GameState::Combat);
        }
        if loot_frame.is_none()
            && world.query_filtered::<Entity, With<SalvagePickupCmp>>().iter(world).next().is_some()
        {
            *loot_frame = Some(frame);
            world.resource_mut::<Settings>().combat_speed = 1.0;
        }
        let Some(start) = *loot_frame else {
            return;
        };
        let name = match frame - start {
            100 => {
                world.resource_mut::<Settings>().combat_paused = true;
                "victory"
            },
            110 => {
                world.resource_mut::<Settings>().combat_paused = false;
                "paused"
            },
            120 => {
                world
                    .query::<&mut Window>()
                    .single_mut(world)
                    .unwrap()
                    .resolution
                    .set(640.0, 360.0);
                return;
            },
            140 => "compact",
            280 => {
                assert_eq!(
                    world.query_filtered::<Entity, With<SalvagePickupCmp>>().iter(world).count(),
                    0
                );
                "finished"
            },
            _ => return,
        };
        let mut capture = world.spawn(Screenshot::primary_window());
        capture.observe(save_to_disk(format!("target/raid-loot-preview/{name}.png")));
        if name == "finished" {
            capture.observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<bevy::app::AppExit>| {
                    exit.write(bevy::app::AppExit::Success);
                },
            );
        }
    });
    app.run();
}

#[test]
#[ignore = "requires generated assets and a GPU; captures space-location echoes offline"]
fn render_location_marker_screenshots() {
    assert_eq!(std::env::var("STELLARION_BACKEND").as_deref(), Ok("mock"));
    std::fs::create_dir_all("target/location-marker-preview").unwrap();
    let mut app = capture_app();
    app.add_systems(Last, |world: &mut World, mut arrived: Local<Option<usize>>| {
        if *world.resource::<State<AppState>>().get() != AppState::Game {
            return;
        }
        let frame = {
            let mut scene = world.resource_mut::<CaptureScene>();
            scene.frame += 1;
            scene.frame
        };
        assert!(frame < 1_000, "location marker capture timed out");
        if frame == 80 {
            world.insert_resource(crate::core::messages::Messages::default());
            let position = world.resource::<CaptureScene>().camera;
            let mut state = world.resource_mut::<UiState>();
            state.planet_selected = None;
            state.focus_planet = None;
            state.focus_position = Some(position);
            state.focus_zoom = None;
            state.to_selected = true;
            state.mission = false;
            state.combat_report = None;
            state.location_marker =
                Some(crate::core::ui::location_marker::LocationMarker::new(position));
        }
        if frame <= 80 || world.resource::<UiState>().to_selected {
            return;
        }
        let age = frame - *arrived.get_or_insert(frame);
        let name = match age {
            20 => "early",
            65 => "fading",
            150 => {
                assert!(world.resource::<UiState>().location_marker.is_none());
                "finished"
            },
            _ => return,
        };
        let mut capture = world.spawn(Screenshot::primary_window());
        capture.observe(save_to_disk(format!("target/location-marker-preview/{name}.png")));
        if name == "finished" {
            capture.observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<bevy::app::AppExit>| {
                    exit.write(bevy::app::AppExit::Success);
                },
            );
        }
    });
    app.run();
}

#[test]
#[ignore = "requires generated assets and a GPU; captures fleet-following location echoes offline"]
fn render_fleet_location_marker_screenshots() {
    use crate::core::ui::location_marker::LocationMarker;

    assert_eq!(std::env::var("STELLARION_BACKEND").as_deref(), Ok("mock"));
    std::fs::create_dir_all("target/fleet-marker-preview").unwrap();
    let mut app = capture_app();
    app.add_systems(Last, |world: &mut World| {
        if *world.resource::<State<AppState>>().get() != AppState::Game {
            return;
        }
        let frame = {
            let mut scene = world.resource_mut::<CaptureScene>();
            scene.frame += 1;
            scene.frame
        };
        assert!(frame < 1_000, "fleet marker capture timed out");
        if frame == 60 {
            world.insert_resource(crate::core::messages::Messages::default());
            let scene = world.resource::<CaptureScene>();
            let center = scene.camera + Vec2::Y * 350.0;
            let mut mission = Mission::new_patrol(
                9909,
                scene.model.turn as usize,
                1,
                world.resource::<Map>().get(scene.origin),
                center,
                fleet(&[(Ship::LightFighter, 6), (Ship::Cruiser, 2)]),
                None,
            );
            mission.position = center;
            mission.patrol.as_mut().unwrap().phase = crate::core::patrols::PatrolPhase::OnStation;
            world.resource_mut::<crate::core::missions::Missions>().0 = vec![mission];
            let mut state = world.resource_mut::<UiState>();
            state.planet_selected = None;
            state.focus_planet = None;
            state.focus_position = Some(center);
            state.focus_zoom = None;
            state.to_selected = true;
            state.mission = false;
            state.combat_report = None;
        }
        if frame == 300 {
            let position = world.resource::<UiState>().focus_position.unwrap();
            let mut fleets =
                world.query::<(&crate::core::map::systems::MissionCmp, &GlobalTransform)>();
            let (_, transform) = fleets.iter(world).find(|(fleet, _)| fleet.id == 9909).unwrap();
            assert!(transform.translation().truncate().distance(position) > 25.0);
            world.resource_mut::<UiState>().location_marker =
                Some(LocationMarker::for_mission(9909, position));
        }
        let name = match frame {
            320 => "early",
            365 => "moving",
            450 => {
                assert!(world.resource::<UiState>().location_marker.is_none());
                "finished"
            },
            _ => return,
        };
        let mut capture = world.spawn(Screenshot::primary_window());
        capture.observe(save_to_disk(format!("target/fleet-marker-preview/{name}.png")));
        if name == "finished" {
            capture.observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                exit.write(AppExit::Success);
            });
        }
    });
    app.run();
}

fn capture_app() -> App {
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: format!("{}/assets-runtime", env!("CARGO_MANIFEST_DIR")),
                meta_check: AssetMetaCheck::Never,
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Stellarion README captures".into(),
                    resolution: WindowResolution::new(1600, 900).with_scale_factor_override(1.0),
                    visible: false,
                    ..default()
                }),
                ..default()
            })
            .set(WinitPlugin {
                run_on_any_thread: true,
            }),
    )
    .add_plugins((
        EguiPlugin::default(),
        crate::core::messages::MessagesPlugin,
        AudioPlugin,
        TweeningPlugin,
    ))
    .add_plugins(crate::core::GamePlugin)
    .insert_resource(WinitSettings::continuous())
    .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
        1.0 / 60.0,
    )))
    .insert_resource(scene())
    .add_systems(PostStartup, install_scene)
    .add_systems(
        PreUpdate,
        (|mut input: Query<&mut EguiInput>| {
            for mut input in &mut input {
                input.events.push(egui::Event::PointerGone);
            }
        })
        .after(EguiPreUpdateSet::ProcessInput)
        .before(EguiPreUpdateSet::BeginPass),
    );
    app
}

#[test]
#[ignore = "requires generated assets and a GPU; captures patrol controls and map offline"]
fn render_patrol_screenshots() {
    assert_eq!(std::env::var("STELLARION_BACKEND").as_deref(), Ok("mock"));
    std::fs::create_dir_all("target/patrol-preview").unwrap();
    if std::env::var_os("STELLARION_PATROL_ANIMATION_PREVIEW").is_some() {
        std::fs::create_dir_all("target/patrol-preview/animation").unwrap();
    }
    let mut app = capture_app();
    app.add_systems(Last, capture_patrol_views);
    app.run();
}

#[test]
#[ignore = "requires generated assets and a GPU; captures interception controls and dogfight offline"]
fn render_intercept_screenshots() {
    assert_eq!(std::env::var("STELLARION_BACKEND").as_deref(), Ok("mock"));
    std::fs::create_dir_all("target/intercept-preview").unwrap();
    let mut app = capture_app();
    app.add_systems(Last, capture_intercept_views);
    app.run();
}

fn capture_intercept_views(world: &mut World) {
    use crate::core::map::planet::Planet;
    use crate::core::missions::BombingRaid;
    use crate::core::units::Amount;
    use rand::SeedableRng;
    if *world.resource::<State<AppState>>().get() != AppState::Game {
        return;
    }
    let frame = {
        let mut scene = world.resource_mut::<CaptureScene>();
        scene.frame += 1;
        scene.frame
    };
    if frame == 60 {
        world.insert_resource(crate::core::messages::Messages::default());
        let origin = world.resource::<CaptureScene>().origin;
        world
            .resource_mut::<Map>()
            .get_mut(origin)
            .army
            .insert(Unit::Building(Building::SensorPhalanx), 30);
        let map = world.resource::<Map>().clone();
        let player = world.resource::<Player>().clone();
        let enemy = world.resource::<CaptureScene>().model.players[1].home_planet;
        let turn = world.resource::<Settings>().turn;
        let mut target = Mission::new_with_id(
            9901,
            turn,
            2,
            map.get(enemy),
            map.get(origin),
            Icon::Attack,
            fleet(&[(Ship::Battleship, 4)]),
            BombingRaid::None,
            true,
            false,
            None,
        );
        target.position = map.get(origin).position + Vec2::X * Planet::SIZE * 12.0;
        world.resource_mut::<crate::core::missions::Missions>().0 = vec![target.clone()];
        let mut ui = world.resource_mut::<UiState>();
        ui.planet_selected = Some(origin);
        ui.choose_intercept_target(&map, &player, turn, &target);
        ui.mission_info.army = fleet(&[(
            Ship::LightFighter,
            8.min(map.get(origin).army.amount(&Unit::Ship(Ship::LightFighter))),
        )]);
    }
    if frame == 160 || frame == 260 {
        for mut window in world.query::<&mut Window>().iter_mut(world) {
            window.resolution.set(
                if frame == 160 {
                    800.0
                } else {
                    1600.0
                },
                if frame == 160 {
                    600.0
                } else {
                    900.0
                },
            );
        }
    }
    if frame == 330 {
        let mission = world.resource::<UiState>().mission_info.clone();
        let intercept = mission.intercept.as_ref().expect("preview must plot a contact");
        let point = intercept.point;
        let mut site = world.resource::<Map>().get(mission.origin).clone();
        site.position = point;
        site.owned = None;
        site.controlled = Some(2);
        site.army = fleet(&[(Ship::LightFighter, 3), (Ship::Cruiser, 1)]).into();
        let mut report = crate::core::combat::resolution::resolve_combat_with_rng(
            world.resource::<Settings>().turn,
            &mission,
            &site,
            &mut rand_chacha::ChaCha8Rng::from_seed([17; 32]),
        );
        report.mission.position = point;
        world.resource_mut::<Player>().reports.push(report);
        world.resource_mut::<UiState>().mission = false;
        world.resource_mut::<UiState>().planet_selected = None;
        for mut transform in
            world.query_filtered::<&mut Transform, With<MainCamera>>().iter_mut(world)
        {
            transform.translation.x = point.x;
            transform.translation.y = point.y;
        }
        for mut projection in
            world.query_filtered::<&mut Projection, With<MainCamera>>().iter_mut(world)
        {
            if let Projection::Orthographic(projection) = &mut *projection {
                projection.scale = 0.8;
            }
        }
    }
    if frame == 780 {
        let origin = world.resource::<CaptureScene>().origin;
        let map = world.resource::<Map>().clone();
        let player = world.resource::<Player>().clone();
        let enemy = world.resource::<CaptureScene>().model.players[1].home_planet;
        let turn = world.resource::<Settings>().turn;
        let mut target = Mission::new_with_id(
            9902,
            turn,
            2,
            map.get(enemy),
            map.get(origin),
            Icon::Attack,
            fleet(&[(Ship::Battleship, 4)]),
            BombingRaid::None,
            true,
            false,
            None,
        );
        target.position = map.get(origin).position + Vec2::X * Planet::SIZE;
        world.resource_mut::<crate::core::missions::Missions>().0 = vec![target.clone()];
        let mut ui = world.resource_mut::<UiState>();
        ui.planet_selected = Some(origin);
        ui.choose_intercept_target(&map, &player, turn, &target);
    }
    let name = match frame {
        130 => Some("draft"),
        230 => Some("draft-small"),
        440 => Some("dogfight"),
        545 => Some("shockwave"),
        660 => Some("aftermath"),
        850 => Some("late-draft"),
        _ => None,
    };
    if let Some(name) = name {
        let path = format!("{}/target/intercept-preview/{name}.png", env!("CARGO_MANIFEST_DIR"));
        world.spawn(Screenshot::primary_window()).observe(save_to_disk(path));
    }
    if frame == 900 {
        world.write_message(AppExit::Success);
    }
}

fn capture_patrol_views(world: &mut World) {
    if *world.resource::<State<AppState>>().get() != AppState::Game {
        return;
    }
    let frame = {
        let mut scene = world.resource_mut::<CaptureScene>();
        scene.frame += 1;
        scene.frame
    };
    let (origin, turn) = {
        let scene = world.resource::<CaptureScene>();
        (scene.origin, scene.model.turn as usize)
    };
    if frame == 60 {
        world.insert_resource(crate::core::messages::Messages::default());
        let map = world.resource::<Map>();
        let point = (1..20)
            .flat_map(|x| (1..20).map(move |y| (x, y)))
            .map(|(x, y)| {
                map.rect.min + map.rect.size() * Vec2::new(x as f32 / 20.0, y as f32 / 20.0)
            })
            .filter(|point| crate::core::patrols::valid_patrol_target(map, origin, *point, Some(5)))
            .min_by(|a, b| {
                a.distance_squared(map.get(origin).position)
                    .total_cmp(&b.distance_squared(map.get(origin).position))
            })
            .unwrap();
        let mission = Mission::new_patrol(
            9909,
            turn,
            1,
            map.get(origin),
            point,
            fleet(&[(Ship::LightFighter, 6), (Ship::Cruiser, 2)]),
            Some(5),
        );
        let mut ui = world.resource_mut::<UiState>();
        ui.planet_selected = None;
        ui.mission = true;
        ui.mission_tab = MissionTab::NewMission;
        ui.mission_info = mission;
        ui.patrol_center = Some(point);
    }
    if frame == 150 {
        let mut query = world.query::<&mut Window>();
        for mut window in query.iter_mut(world) {
            window.resolution.set(800.0, 600.0);
        }
    }
    if frame == 230 {
        world.resource_mut::<UiState>().begin_patrol_selection();
    }
    if frame == 280 {
        let mut query = world.query::<&mut Window>();
        for mut window in query.iter_mut(world) {
            window.resolution.set(1600.0, 900.0);
        }
        let mut mission = world.resource::<UiState>().mission_info.clone();
        let mut outbound = mission.clone();
        outbound.id = 9910;
        let point = mission.patrol.as_ref().unwrap().center;
        mission.position = point;
        mission.travel_turns = 1;
        mission.patrol.as_mut().unwrap().phase = crate::core::patrols::PatrolPhase::OnStation;
        let mut returning = mission.clone();
        returning.id = 9911;
        returning.return_patrol(world.resource::<Map>(), turn);
        world
            .resource_mut::<crate::core::missions::Missions>()
            .0
            .extend([mission, outbound, returning]);
        let mut ui = world.resource_mut::<UiState>();
        ui.patrol_center = Some(point);
        ui.selecting_patrol_zone = false;
        ui.mission = true;
        ui.mission_tab = MissionTab::ActiveMissions;
        let mut query = world.query_filtered::<&mut Transform, With<MainCamera>>();
        for mut transform in query.iter_mut(world) {
            transform.translation.x = point.x;
            transform.translation.y = point.y;
        }
    }
    if frame == 350 {
        let mut query = world.query::<&mut Window>();
        for mut window in query.iter_mut(world) {
            window.resolution.set(800.0, 600.0);
        }
    }
    if frame == 380 {
        let mut query = world.query::<&mut Window>();
        for mut window in query.iter_mut(world) {
            window.resolution.set(1600.0, 900.0);
        }
        world.resource_mut::<UiState>().mission = false;
        if std::env::var_os("STELLARION_PATROL_ANIMATION_PREVIEW").is_some() {
            let mut query = world.query_filtered::<&mut Projection, With<MainCamera>>();
            for mut projection in query.iter_mut(world) {
                if let Projection::Orthographic(projection) = &mut *projection {
                    projection.scale = 1.15;
                }
            }
        }
    }
    let animation = std::env::var_os("STELLARION_PATROL_ANIMATION_PREVIEW").is_some();
    if frame == 430 && !animation {
        let launch = world.resource::<Map>().get(origin).clone();
        let point = world.resource::<UiState>().patrol_center.unwrap();
        let mission = Mission::new_patrol(
            9912,
            turn,
            1,
            &launch,
            point,
            fleet(&[(Ship::LightFighter, 8), (Ship::Cruiser, 3)]),
            None,
        );
        let mut encounter = crate::core::map::planet::Planet::new(
            origin,
            "Aether Ray Swarm".to_owned(),
            point,
            false,
            1.0,
        );
        encounter.army.insert(Unit::Fauna(SpaceFauna::AetherRay), 5);
        let mut rng = world.resource::<CaptureScene>().model.rng.clone().next_rng();
        let mut report = resolve_combat_with_rng(turn, &mission, &encounter, &mut rng);
        report.id = 9912;
        world.resource_mut::<crate::core::player::Player>().reports.push(report);
        let mut ui = world.resource_mut::<UiState>();
        ui.mission = true;
        ui.mission_tab = MissionTab::MissionReports;
        ui.mission_report = Some(9912);
    }
    if frame == 500 && !animation {
        let mut query = world.query::<&mut Window>();
        for mut window in query.iter_mut(world) {
            window.resolution.set(800.0, 600.0);
        }
    }
    if frame == 550 && !animation {
        let mut query = world.query::<&mut Window>();
        for mut window in query.iter_mut(world) {
            window.resolution.set(1600.0, 900.0);
        }
        let mut ui = world.resource_mut::<UiState>();
        ui.combat_report = Some(9912);
        ui.combat_report_round = 1;
    }
    if frame == 620 && !animation {
        let mut query = world.query::<&mut Window>();
        for mut window in query.iter_mut(world) {
            window.resolution.set(800.0, 600.0);
        }
    }
    if animation && (420..=1620).contains(&frame) && (frame - 420).is_multiple_of(6) {
        let index = (frame - 420) / 6;
        let path = format!(
            "{}/target/patrol-preview/animation/{index:03}.png",
            env!("CARGO_MANIFEST_DIR")
        );
        let mut capture = world.spawn(Screenshot::primary_window());
        capture.observe(save_to_disk(path));
        if frame == 1620 {
            capture.observe(
                |_: On<ScreenshotCaptured>, mut exit: MessageWriter<bevy::app::AppExit>| {
                    exit.write(bevy::app::AppExit::Success);
                },
            );
        }
    }
    let name = match frame {
        120 => "launch",
        210 => "launch-small",
        260 => "zone-selection-small",
        340 => "active",
        370 => "active-small",
        420 if animation => return,
        420 => "map",
        490 if !animation => "report",
        540 if !animation => "report-small",
        610 if !animation => "combat-details",
        660 if !animation => "combat-details-small",
        _ => return,
    };
    let path = format!("{}/target/patrol-preview/{name}.png", env!("CARGO_MANIFEST_DIR"));
    let mut capture = world.spawn(Screenshot::primary_window());
    capture.observe(save_to_disk(path));
    if frame == 660 && !animation {
        capture.observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<bevy::app::AppExit>| {
                exit.write(bevy::app::AppExit::Success);
            },
        );
    }
}

fn install_scene(world: &mut World) {
    let model = world.resource::<CaptureScene>().model.clone();
    let members = ["Mara", "Jonas", "Priya"]
        .into_iter()
        .enumerate()
        .map(|(slot, name)| GameMembership {
            game_id: GameId("readme-offline".into()),
            player_id: slot as u64 + 1,
            user_id: UserId(format!("readme-{slot}")),
            display_name: name.into(),
            is_creator: slot == 0,
            identity_version: 1,
            connected: true,
        })
        .collect::<Vec<_>>();
    // Discard boot authentication and keep all profile writes in memory.
    crate::multiplayer::client::tests::isolate_screenshot_client(world);
    let mut session = MultiplayerSession::default();
    session.connection = ConnectionStatus::Connected;
    session.membership = Some(members[0].clone());
    session.active_game = Some(GameRecord {
        id: GameId("readme-offline".into()),
        code: GameCode("NEBULA".into()),
        revision: 23,
        saved_at: 0,
        max_players: 3,
        status: MatchStatus::Active,
        persisted: PersistedGame::new(model),
        members,
        submitted_players: vec![],
    });
    world.insert_resource(session);
    world.resource_mut::<Settings>().set_audio_mode(AudioState::Mute);
    world.resource_mut::<NextState<AppState>>().set(AppState::LoadingGame);
}

fn capture_views(world: &mut World) {
    if *world.resource::<State<AppState>>().get() != AppState::Game {
        return;
    }
    let frame = {
        let mut scene = world.resource_mut::<CaptureScene>();
        scene.frame += 1;
        scene.frame
    };
    let (origin, destination, camera) = {
        let scene = world.resource::<CaptureScene>();
        (scene.origin, scene.destination, scene.camera)
    };
    if frame < 440 {
        let mut query =
            world.query_filtered::<(&mut Transform, &mut Projection), With<MainCamera>>();
        for (mut transform, mut projection) in query.iter_mut(world) {
            transform.translation.x = camera.x;
            transform.translation.y = camera.y;
            if let Projection::Orthographic(projection) = &mut *projection {
                projection.scale = std::env::var("STELLARION_README_ZOOM")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(1.25);
            }
        }
    }
    let report_id = world.resource::<Player>().reports[0].id;
    match frame {
        80 => world.insert_resource(crate::core::messages::Messages::default()),
        140 => {
            let mut ui = world.resource_mut::<UiState>();
            ui.planet_selected = Some(origin);
            ui.shop = Shop::Buildings;
        },
        240 => {
            let map = world.resource::<Map>();
            let mission = Mission {
                owner: 1,
                origin,
                destination,
                position: map.get(origin).position,
                objective: Icon::Attack,
                army: fleet(&[
                    (Ship::LightFighter, 12),
                    (Ship::HeavyFighter, 6),
                    (Ship::Destroyer, 4),
                    (Ship::Cruiser, 2),
                ]),
                ..default()
            };
            let mut ui = world.resource_mut::<UiState>();
            ui.planet_selected = None;
            ui.mission = true;
            ui.mission_info = mission;
            ui.mission_tab = MissionTab::NewMission;
        },
        340 => world.resource_mut::<UiState>().mission_tab = MissionTab::ActiveMissions,
        440 => {
            let mut ui = world.resource_mut::<UiState>();
            ui.mission = false;
            ui.in_combat = Some(report_id);
            world.resource_mut::<Settings>().combat_speed = 0.5;
            world.resource_mut::<Settings>().combat_volley_fire = true;
            world.resource_mut::<NextState<GameState>>().set(GameState::Combat);
        },
        2200 => {
            world.resource_mut::<NextState<GameState>>().set(GameState::Playing);
        },
        2240 => {
            let mut ui = world.resource_mut::<UiState>();
            ui.combat_report = Some(report_id);
            ui.combat_report_total = true;
        },
        _ => {},
    }
    let name = match frame {
        120 => "map".to_owned(),
        220 => "shop".to_owned(),
        320 => "mission-planning".to_owned(),
        420 => "active-missions".to_owned(),
        2040 => "combat".to_owned(),
        2340 => "battle-report".to_owned(),
        frame if (900..=2100).contains(&frame) && frame.is_multiple_of(60) => {
            format!("combat-{frame}")
        },
        _ => return,
    };
    let path = format!("{}/target/readme-screenshots/{name}.png", env!("CARGO_MANIFEST_DIR"));
    let mut capture = world.spawn(Screenshot::primary_window());
    capture.observe(save_to_disk(path));
    if frame == 2340 {
        capture.observe(
            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<bevy::app::AppExit>| {
                exit.write(bevy::app::AppExit::Success);
            },
        );
    }
}
