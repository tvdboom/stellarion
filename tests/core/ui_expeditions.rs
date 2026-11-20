use super::*;

use crate::core::combat::report::CombatReport;
use crate::core::constants::SOLAR_STAR_SIZE;
use crate::core::patrols::valid_patrol_target;
use crate::core::simulation::{GameModel, GameRules};
use crate::test_support::empty_report;

fn setup() -> (GameModel, Player, UiState, Vec2) {
    let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let mut player = model.players[0].clone();
    player.resources.deuterium = 100_000;
    let army = Army::from([(Unit::colony_ship(), 1), (Unit::Ship(Ship::Cruiser), 3)]);
    model.map.get_mut(player.home_planet).army.extend(army.clone());
    let center = model.map.rect.center();
    let state = UiState {
        mission: true,
        mission_info: Mission {
            origin: player.home_planet,
            destination: model.players[1].home_planet,
            army,
            ..default()
        },
        ..default()
    };
    (model, player, state, center)
}

#[test]
fn expedition_report_uses_deployment_grids_and_shows_only_delivered_cargo() {
    let (mut model, mut player, _, center) = setup();
    let home = model.map.get_mut(player.home_planet);
    let cruiser = Unit::Ship(Ship::Cruiser);
    let destroyer = Unit::Ship(Ship::Destroyer);
    home.army = Army::from([(destroyer, 13)]).into();
    let port = home.clone();
    let mut mission = Mission::new_expedition(
        42,
        1,
        player.id,
        &port,
        center,
        Army::from([(Unit::colony_ship(), 2), (cruiser, 4)]),
    );
    mission.return_expedition(&model.map, 2);
    let resources = ["metal", "crystal", "deuterium"].map(str::to_owned);
    let textures = ["expedition", "deploy", "recall", "deep space"]
        .map(str::to_owned)
        .into_iter()
        .chain([cruiser.to_lowername(), destroyer.to_lowername()])
        .chain(resources.clone())
        .zip((1..).map(egui::TextureId::User))
        .collect::<HashMap<_, _>>();
    let images = ImageIds(textures);

    for (docked, cargo) in [
        (true, Resources::new(23, 35, 57)),
        (true, Resources::default()),
        (false, Resources::new(23, 35, 57)),
    ] {
        let mut encounter = port.clone();
        if !docked {
            encounter.name = "Aether Ray Swarm".to_owned();
            encounter.army = Army::from([(Unit::Fauna(SpaceFauna::AetherRay), 5)]).into();
        }
        let mut report = empty_report(mission.clone(), encounter);
        report.id = 90;
        report.turn = 9;
        report.mission.expedition.as_mut().unwrap().resources = cargo;
        report.surviving_defender = report.planet.army.clone();
        if !cargo.is_empty() {
            report.mission.expedition.as_mut().unwrap().ships.insert(cruiser, 3);
        }
        if docked {
            report.surviving_attacker = mission.army.clone();
            if !cargo.is_empty() {
                report.surviving_attacker.insert(cruiser, 7);
            }
        } else {
            report.combat_report = Some(CombatReport {
                rounds: vec![RoundReport::default()],
                ..default()
            });
        }
        player.reports = vec![report];

        for viewport in [egui::vec2(1280.0, 720.0), egui::vec2(800.0, 600.0)] {
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let scale = mission_panel_scale(viewport);
            let size = mission_panel_size(viewport / scale);
            let position = (viewport - size * scale) * 0.5;
            let mut state = UiState {
                mission_tab: MissionTab::MissionReports,
                ..default()
            };
            let mut output = egui::FullOutput::default();
            for _ in 0..2 {
                output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                        ..default()
                    },
                    |context| {
                        draw_panel_on_context(
                            context,
                            "mission",
                            "panel",
                            (position.x, position.y),
                            (size.x, size.y),
                            0.0,
                            scale,
                            &images,
                            |ui| {
                                draw_mission_reports(
                                    ui,
                                    &mut state,
                                    &model.map,
                                    &player,
                                    &MultiplayerSession::default(),
                                    true,
                                    &images,
                                );
                            },
                        );
                    },
                );
                output.textures_delta.clear();
            }
            let rects = |key: &str| {
                let texture = images.get(key);
                output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                            Some(mesh.calc_bounds())
                        },
                        egui::Shape::Rect(rect)
                            if rect.brush.is_some() && rect.fill_texture_id() == texture =>
                        {
                            Some(rect.rect)
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            };
            let labels = output.shapes.iter().filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((text.galley.text(), text.pos)),
                _ => None,
            });
            let objectives = rects("expedition");
            let badges = rects("recall");
            assert_eq!(objectives.len(), 2, "list and selected report icons");
            assert_eq!(badges.len(), 2);
            for (icon, badge) in objectives.iter().zip(badges) {
                assert!(badge.width() > icon.width() * 0.6);
                assert!(badge.center().x > icon.center().x && badge.center().y > icon.center().y);
            }
            assert!(rects("deploy").is_empty());
            assert!(!labels.clone().any(|(text, _)| {
                ["Returned", "Return leg", "Recovered ships", "No ships recovered."].contains(&text)
                    || text.contains("Recovered cargo")
            }));

            let cruisers = rects(&cruiser.to_lowername());
            let count = if !docked {
                "0"
            } else if cargo.is_empty() {
                "4"
            } else {
                "7"
            };
            assert!(
                labels
                    .clone()
                    .any(|(text, location)| text == count && cruisers[0].contains(location)),
                "docked={docked}, expected={count}, cruiser labels={:?}",
                labels
                    .clone()
                    .filter(|(_, location)| cruisers[0].contains(*location))
                    .collect::<Vec<_>>(),
            );
            assert_eq!(
                labels.clone().any(|(text, location)| text == "-4"
                    && cruisers[0].expand(2.0 * scale).contains(location)),
                !docked,
            );
            let gains = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "+3" => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>();
            if docked && !cargo.is_empty() {
                assert_eq!(gains.len(), 1);
                assert!(cruisers[0].contains(gains[0].pos));
                assert_eq!(
                    gains[0].galley.job.sections[0].format.color,
                    Color32::from_rgb(163, 190, 140)
                );
            } else {
                assert!(gains.is_empty());
            }
            if docked {
                let defending_ships = rects(&destroyer.to_lowername());
                let defender = defending_ships.last().unwrap();
                assert!(defender.left() > cruisers[0].right());
                assert!(labels
                    .clone()
                    .any(|(text, location)| text == "13" && defender.contains(location)));
                for (name, resource) in resources.iter().zip(CRAWLER_SALVAGE_RESOURCE_ORDER) {
                    let icons = rects(name);
                    assert_eq!(icons.len(), 1);
                    let icon = icons[0];
                    assert!(icon.top() > defender.bottom().max(cruisers[0].bottom()));
                    assert!(labels.clone().any(|(text, location)| {
                        text == format_thousands(cargo.get(&resource))
                            && icon.y_range().contains(location.y)
                            && (icon.right()..icon.right() + 10.0).contains(&location.x)
                    }));
                }
            } else {
                assert!(!rects("deep space").is_empty());
                assert!(resources.iter().all(|key| rects(key).is_empty()));
            }
        }
    }
}

#[test]
fn selecting_expedition_retains_fleet_and_accepts_sites_throughout_the_map() {
    let (model, player, mut state, center) = setup();
    let army = state.mission_info.army.clone();
    state.begin_space_selection(Icon::Expedition);
    assert!(!state.mission && state.selecting_patrol_zone);
    assert!(!state.choose_patrol_zone(&model.map, &player, 1, model.map.rect.max + Vec2::ONE));
    assert!(!state.choose_patrol_zone(
        &model.map,
        &player,
        1,
        model.map.get(player.home_planet).position
    ));
    let sun = model.map.solar_star_position();
    let inside_sun = sun - model.map.solar_corner().x * Vec2::X * (SOLAR_STAR_SIZE * 0.5 - 20.0);
    assert!(!state.mission && state.selecting_patrol_zone);
    for site in [inside_sun, center] {
        assert!(state.choose_patrol_zone(&model.map, &player, 1, site));
        assert!(state.mission && !state.selecting_patrol_zone);
        assert_eq!(state.mission_info.expedition.as_ref().unwrap().center, site);
        state.begin_space_selection(Icon::Expedition);
    }
    state.cancel_patrol_selection();
    assert!(state.mission && !state.selecting_patrol_zone);
    assert_eq!(state.mission_info.objective, Icon::Expedition);
    assert_eq!(state.mission_info.army, army);
    assert!(state.mission_info.patrol.is_none());
    assert_eq!(state.mission_info.expedition.as_ref().unwrap().center, center);
    assert_eq!(state.mission_info.image(&player), "mission colonize");
    assert_eq!(
        draft_objectives(&player, model.map.get(player.home_planet), Icon::Expedition),
        vec![Icon::Deploy, Icon::Patrol, Icon::Expedition]
    );
    state.begin_space_selection(Icon::Patrol);
    state.cancel_patrol_selection();
    assert_eq!(state.mission_info.objective, Icon::Expedition);
    state.set_planet_mission_destination(model.players[1].home_planet);
    assert!(state.mission_info.expedition.is_none());
    assert_eq!(state.mission_info.objective, Icon::Attack);
}

#[test]
fn switching_patrol_to_expedition_keeps_the_point_until_expedition_is_clicked_again() {
    for viewport in [egui::vec2(1280.0, 720.0), egui::vec2(800.0, 600.0)] {
        let (mut model, mut player, mut state, _) = setup();
        let center = (1..20)
            .flat_map(|x| (1..20).map(move |y| (x, y)))
            .map(|(x, y)| {
                model.map.rect.min
                    + model.map.rect.size() * Vec2::new(x as f32 / 20.0, y as f32 / 20.0)
            })
            .find(|point| valid_patrol_target(&model.map, player.home_planet, *point, None))
            .unwrap();
        assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
        let army = state.mission_info.army.clone();
        let mission_id = state.mission_info.id;
        let previous_destination = state.patrol_previous_destination;
        state.mission_info.battle_plan.automatic_tactic = false;
        state.mission_battle_plan_origin = Some(player.home_planet);
        let battle_plan = state.mission_info.battle_plan;
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let expedition_image = egui::TextureId::User(991);
        let images = ImageIds(HashMap::from([("expedition".to_owned(), expedition_image)]));
        let mut world = World::new();
        world.init_resource::<Messages<SendMissionMsg>>();
        world.init_resource::<Messages<MultiplayerRequest>>();
        let mut params = bevy::ecs::system::SystemState::<(
            MessageWriter<SendMissionMsg>,
            MessageWriter<MultiplayerRequest>,
        )>::new(&mut world);
        let mut icon_rect = egui::Rect::NOTHING;
        for frame in 0..8 {
            let mut events = vec![];
            if frame % 4 >= 2 {
                events.push(egui::Event::PointerMoved(icon_rect.center()));
                events.push(egui::Event::PointerButton {
                    pos: icon_rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed: frame % 4 == 2,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                    events,
                    ..default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        let (mut sends, mut requests) = params.get_mut(&mut world).unwrap();
                        draw_new_mission(
                            ui,
                            &mut sends,
                            &[],
                            &Settings {
                                turn: 1,
                                ..default()
                            },
                            &mut state,
                            &mut model.map,
                            &mut player,
                            &MultiplayerSession::default(),
                            &mut requests,
                            true,
                            &ButtonInput::default(),
                            &images,
                        );
                    });
                },
            );
            if frame % 4 < 2 {
                icon_rect = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh)
                            if mesh.texture_id == expedition_image
                                && mesh.calc_bounds().size() == egui::Vec2::splat(40.0) =>
                        {
                            Some(mesh.calc_bounds())
                        },
                        _ => None,
                    })
                    .unwrap();
            }
            output.textures_delta.clear();
            assert_eq!(state.mission, frame != 7, "frame {frame} at {viewport:?}");
            assert_eq!(state.selecting_patrol_zone, frame == 7);
            assert_eq!(state.mission_info.army, army);
            assert_eq!(state.mission_info.id, mission_id);
            assert_eq!(state.mission_info.battle_plan, battle_plan);
            assert_eq!(state.patrol_previous_destination, previous_destination);
            assert_eq!(state.patrol_center, Some(center));
            if frame >= 3 {
                assert_eq!(state.mission_info.objective, Icon::Expedition);
                assert_eq!(state.mission_info.expedition.as_ref().unwrap().center, center);
                assert!(state.mission_info.patrol.is_none());
            }
        }
        state.cancel_patrol_selection();
        assert!(state.mission && !state.selecting_patrol_zone);
        assert_eq!(state.mission_info.expedition.as_ref().unwrap().center, center);
        assert!(world.resource::<Messages<SendMissionMsg>>().is_empty());
    }
}

#[test]
fn switching_an_expedition_on_a_planet_to_patrol_requires_a_new_space_destination() {
    let (model, player, mut state, _) = setup();
    let center = model.map.get(model.players[1].home_planet).position;
    state.begin_space_selection(Icon::Expedition);
    assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
    state.select_space_objective(&model.map, &player, 1, Icon::Patrol);
    assert!(!state.mission && state.selecting_patrol_zone);
    state.cancel_patrol_selection();
    assert!(state.mission && !state.selecting_patrol_zone);
    assert_eq!(state.mission_info.objective, Icon::Expedition);
    assert_eq!(state.mission_info.expedition.as_ref().unwrap().center, center);
}

#[test]
fn objective_is_disabled_without_a_colony_ship_and_does_not_use_the_colony_limit() {
    let (model, player, _, _) = setup();
    let origin = model.map.get(player.home_planet);
    let fleet = Army::from([(Unit::Ship(Ship::Cruiser), 3)]);
    assert_eq!(
        mission_objective_unavailable_reason(Icon::Expedition, &fleet, origin.id, origin, 100, 100),
        Some(Icon::Expedition.requirement())
    );
    assert!(mission_objective_available(
        Icon::Expedition,
        &origin.army,
        origin.id,
        origin,
        100,
        100
    ));
}

#[test]
fn preview_shows_outlook_cargo_and_both_distances_at_small_and_large_widths() {
    let (mut model, player, mut state, center) = setup();
    let moon = model.map.planets.iter_mut().find(|world| world.is_moon()).unwrap();
    moon.position = center + Vec2::X * Planet::SIZE;
    moon.controlled = Some(player.id);
    state.begin_space_selection(Icon::Expedition);
    assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
    // A large level-one escort burns fuel without adding cargo capacity.
    state.mission_info.army.insert(Unit::Ship(Ship::HeavyFighter), 100_000);
    let fuel = state.mission_info.dispatch_fuel_consumption(&model.map, &player);
    let target_distance =
        model.map.get(player.home_planet).position.distance(center) / Planet::SIZE;
    for width in [240.0, 330.0, 650.0] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 650.0),
                )),
                ..default()
            },
            |ui| {
                draw_mission_details(
                    ui,
                    &mut state.mission_info,
                    &model.map,
                    &player,
                    1,
                    &ImageIds::default(),
                    None,
                );
            },
        );
        output.textures_delta.clear();
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text),
                _ => None,
            })
            .collect::<Vec<_>>();
        let outlook = text
            .iter()
            .find(|row| row.galley.text().starts_with("⚖ Expedition outlook: "))
            .unwrap();
        let score: u8 = outlook
            .galley
            .text()
            .strip_prefix("⚖ Expedition outlook: ")
            .unwrap()
            .strip_suffix("/100")
            .unwrap()
            .parse()
            .unwrap();
        assert!(score < 50);
        assert_eq!(
            outlook.galley.job.sections[0].format.color,
            context.global_style().visuals.error_fg_color
        );
        let rows = text.iter().map(|row| row.galley.text()).collect::<Vec<_>>();
        assert!(rows.contains(&format!("📏 Target distance: {target_distance:.1} AU").as_str()));
        assert!(rows.contains(&"📦 Cargo capacity: 290"));
        assert!(rows.contains(&format!("⛽ Fuel consumption: {fuel}").as_str()));
        assert!(!rows.iter().any(|row| row.contains("Home distance")));
        assert!(!rows.iter().any(|row| row.contains("Landmark distance")));
        assert!(!rows.iter().any(|row| row.contains("Discovery success") || row.contains("%")));
        assert!(!rows.iter().any(|row| row.contains("Requires at least")));
    }
}

#[test]
fn send_expedition_tooltip_names_the_missing_colony_ship() {
    for (width, cruisers) in [(360.0, 0), (900.0, 3)] {
        let (mut model, mut player, mut state, center) = setup();
        state.begin_space_selection(Icon::Expedition);
        assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
        state.mission_info.army = Army::from([(Unit::Ship(Ship::Cruiser), cruisers)]);

        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        context.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let mut world = World::new();
        world.init_resource::<Messages<SendMissionMsg>>();
        world.init_resource::<Messages<MultiplayerRequest>>();
        let mut params = bevy::ecs::system::SystemState::<(
            MessageWriter<SendMissionMsg>,
            MessageWriter<MultiplayerRequest>,
        )>::new(&mut world);
        let mut send_position = egui::Pos2::ZERO;
        let mut saw_requirement = false;
        let requirement = "An Expedition requires at least one Colony Ship.";

        for frame in 0..6 {
            let mut keyboard = ButtonInput::default();
            if frame >= 4 {
                keyboard.press(KeyCode::Enter);
            }
            if frame == 5 {
                state.mission_info.army.insert(Unit::colony_ship(), 1);
            }
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 650.0),
                    )),
                    time: Some(f64::from(frame) * 0.25),
                    events: if (2..5).contains(&frame) {
                        vec![egui::Event::PointerMoved(send_position)]
                    } else {
                        vec![]
                    },
                    ..default()
                },
                |ui| {
                    let (mut sends, mut requests) = params.get_mut(&mut world).unwrap();
                    draw_new_mission(
                        ui,
                        &mut sends,
                        &[],
                        &Settings {
                            turn: 1,
                            ..default()
                        },
                        &mut state,
                        &mut model.map,
                        &mut player,
                        &MultiplayerSession::default(),
                        &mut requests,
                        true,
                        &keyboard,
                        &ImageIds::default(),
                    );
                },
            );
            output.textures_delta.clear();
            for shape in &output.shapes {
                let egui::Shape::Text(text) = &shape.shape else {
                    continue;
                };
                if text.galley.text() == "Send mission" {
                    send_position = text.pos + text.galley.size() * 0.5;
                }
                if text.galley.text() == requirement {
                    assert!(frame >= 2, "the requirement should wait for hover");
                    saw_requirement = true;
                }
            }
            assert_eq!(world.resource::<Messages<SendMissionMsg>>().len(), usize::from(frame == 5));
        }
        assert!(saw_requirement, "missing tooltip at width {width} with {cruisers} cruisers");
    }
}
