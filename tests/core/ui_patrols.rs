use super::*;
use crate::core::simulation::{GameModel, GameRules};

fn patrol_setup() -> (GameModel, Player, UiState, Vec2) {
    let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let mut player = model.players[0].clone();
    player.resources.deuterium = 100_000;
    let home = player.home_planet;
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 3)]);
    model.map.get_mut(home).army.extend(army.clone());
    let center = (1..20)
        .flat_map(|x| (1..20).map(move |y| (x, y)))
        .map(|(x, y)| {
            model.map.rect.min + model.map.rect.size() * Vec2::new(x as f32 / 20.0, y as f32 / 20.0)
        })
        .find(|point| crate::core::patrols::valid_patrol_target(&model.map, home, *point, None))
        .unwrap();
    let state = UiState {
        mission: true,
        mission_info: Mission {
            origin: home,
            destination: model.players[1].home_planet,
            army,
            ..default()
        },
        ..default()
    };
    (model, player, state, center)
}

#[test]
fn selecting_space_hides_the_panel_then_restores_a_patrol_with_the_same_fleet() {
    let (model, player, mut state, center) = patrol_setup();
    let army = state.mission_info.army.clone();
    let planet_destination = state.mission_info.destination;
    state.begin_patrol_selection();
    assert!(!state.mission);
    assert!(state.selecting_patrol_zone);
    assert!(!state.choose_patrol_zone(
        &model.map,
        &player,
        1,
        model.map.get(player.home_planet).position
    ));
    assert!(state.selecting_patrol_zone);
    assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
    assert!(state.mission);
    assert!(!state.selecting_patrol_zone);
    assert_eq!(state.mission_tab, MissionTab::NewMission);
    assert_eq!(state.mission_info.army, army);
    assert_eq!(state.mission_info.destination, player.home_planet);
    assert_eq!(state.patrol_previous_destination, Some(planet_destination));
    assert_eq!(state.mission_info.patrol.as_ref().unwrap().center, center);
    assert_eq!(state.mission_info.patrol.as_ref().unwrap().remaining_turns, None);
    assert!(state.mission_info.valid_patrol(&model.map));
    state.begin_patrol_selection();
    state.cancel_patrol_selection();
    assert_eq!(state.mission_info.patrol.as_ref().unwrap().center, center);
    assert_eq!(draft_planet_destination(&state, &model.map, &player), player.home_planet);
}

#[test]
fn space_drafts_restore_a_friendly_deploy_destination_instead_of_a_previous_enemy() {
    let (mut model, player, mut state, center) = patrol_setup();
    let enemy = state.mission_info.destination;
    assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
    assert_eq!(draft_planet_destination(&state, &model.map, &player), player.home_planet);
    let colony = model
        .map
        .planets
        .iter_mut()
        .find(|planet| planet.id != player.home_planet && planet.id != enemy && !planet.is_moon())
        .unwrap();
    colony.owned = Some(player.id);
    colony.controlled = Some(player.id);
    let colony_id = colony.id;
    assert_eq!(draft_planet_destination(&state, &model.map, &player), colony_id);
    state.patrol_previous_destination = Some(colony_id);
    assert_eq!(draft_planet_destination(&state, &model.map, &player), colony_id);
}

#[test]
fn patrol_objective_is_next_to_deploy_for_friendly_worlds_and_space_drafts_only() {
    let (mut model, player, state, _) = patrol_setup();
    let enemy = model.map.get(state.mission_info.destination);
    assert_eq!(
        draft_objectives(&player, enemy, false),
        vec![
            Icon::Colonize,
            Icon::Attack,
            Icon::Raid,
            Icon::Spy,
            Icon::MissileStrike,
            Icon::Intercept,
            Icon::Destroy,
        ]
    );
    assert_eq!(draft_objectives(&player, enemy, true), vec![Icon::Deploy, Icon::Patrol]);
    assert_eq!(
        draft_objectives(&player, model.map.get(player.home_planet), false),
        vec![Icon::Deploy, Icon::Patrol]
    );
    let controlled = model.map.get_mut(state.mission_info.destination);
    controlled.controlled = Some(player.id);
    assert_eq!(
        draft_objectives(&player, controlled, false),
        vec![Icon::Colonize, Icon::Deploy, Icon::Patrol]
    );
}

#[test]
fn deploy_requires_another_usable_friendly_world_and_explains_disabled_clicks() {
    for (owned, controlled, destroyed) in
        [(false, false, false), (true, true, true), (false, true, false), (true, true, false)]
    {
        for width in [650.0, 900.0] {
            let (mut model, mut player, mut state, center) = patrol_setup();
            let destination_id = state.mission_info.destination;
            let destination = model.map.get_mut(destination_id);
            destination.owned = owned.then_some(player.id);
            destination.controlled = controlled.then_some(player.id);
            destination.is_destroyed = destroyed;
            let available = (owned || controlled) && !destroyed;
            assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
            let patrol = state.mission_info.clone();
            let context = egui::Context::default();
            context.all_styles_mut(|style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let deploy_image = egui::TextureId::User(992);
            let images =
                ImageIds(std::collections::HashMap::from([("deploy".to_owned(), deploy_image)]));
            let mut world = World::new();
            world.init_resource::<Messages<SendMissionMsg>>();
            world.init_resource::<Messages<MultiplayerRequest>>();
            let mut params = bevy::ecs::system::SystemState::<(
                MessageWriter<SendMissionMsg>,
                MessageWriter<MultiplayerRequest>,
            )>::new(&mut world);
            let mut icon_rect = egui::Rect::NOTHING;
            let mut saw_reason = false;
            for frame in 0..7 {
                let mut events = Vec::new();
                if frame >= 2 {
                    events.push(egui::Event::PointerMoved(icon_rect.center()));
                }
                if frame == 2 || frame == 3 {
                    events.push(egui::Event::PointerButton {
                        pos: icon_rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: egui::Modifiers::NONE,
                    });
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 650.0),
                        )),
                        time: Some(frame as f64 * 0.1),
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
                output.textures_delta.clear();
                for shape in &output.shapes {
                    match &shape.shape {
                        egui::Shape::Mesh(mesh)
                            if mesh.texture_id == deploy_image
                                && mesh.calc_bounds().size() == egui::Vec2::splat(40.0) =>
                        {
                            icon_rect = mesh.calc_bounds();
                            for vertex in &mesh.vertices {
                                assert_eq!(vertex.color == Color32::WHITE, available);
                            }
                        },
                        egui::Shape::Text(text)
                            if text.galley.text() == "No planets to deploy to." =>
                        {
                            assert!(!available);
                            assert!(text
                                .galley
                                .job
                                .sections
                                .iter()
                                .all(|section| section.format.color == Color32::RED));
                            saw_reason = true;
                        },
                        egui::Shape::Text(text) if !available => {
                            assert_ne!(text.galley.text(), Icon::Deploy.description());
                        },
                        _ => {},
                    }
                }
                assert!(icon_rect.is_finite());
            }
            assert_eq!(saw_reason, !available);
            if available {
                assert_eq!(
                    state.mission_info.objective,
                    Icon::Deploy,
                    "owned={owned}, controlled={controlled}, destroyed={destroyed}, width={width}"
                );
                assert_eq!(state.mission_info.destination, destination_id);
                assert!(state.mission_info.patrol.is_none());
            } else {
                assert_eq!(state.mission_info.objective, Icon::Patrol);
                assert_eq!(state.mission_info.patrol.as_ref().unwrap().center, center);
                assert_eq!(state.mission_info.army, patrol.army);
            }
            assert!(world.resource::<Messages<SendMissionMsg>>().is_empty());
        }
    }
}

#[test]
fn space_patrol_can_switch_to_deploy_then_restart_location_selection() {
    let (mut model, mut player, mut state, center) = patrol_setup();
    let destination = model.map.get_mut(state.mission_info.destination);
    destination.owned = Some(player.id);
    destination.controlled = Some(player.id);
    let friendly_destination = destination.id;
    assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
    let context = egui::Context::default();
    let patrol_image = egui::TextureId::User(991);
    let deploy_image = egui::TextureId::User(992);
    let images = ImageIds(std::collections::HashMap::from([
        ("patrol".to_owned(), patrol_image),
        ("deploy".to_owned(), deploy_image),
    ]));
    let mut world = World::new();
    world.init_resource::<Messages<SendMissionMsg>>();
    world.init_resource::<Messages<MultiplayerRequest>>();
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<SendMissionMsg>,
        MessageWriter<MultiplayerRequest>,
    )>::new(&mut world);
    let mut icon_rect = egui::Rect::NOTHING;
    for frame in 0..8 {
        let image = if frame < 4 {
            deploy_image
        } else {
            patrol_image
        };
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
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 650.0),
                )),
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
                        if mesh.texture_id == image
                            && mesh.calc_bounds().size() == egui::Vec2::splat(40.0) =>
                    {
                        Some(mesh.calc_bounds())
                    },
                    _ => None,
                })
                .unwrap();
        }
        output.textures_delta.clear();
        if frame == 3 || frame == 4 {
            assert_eq!(state.mission_info.objective, Icon::Deploy);
            assert_eq!(state.mission_info.destination, friendly_destination);
            assert!(state.mission);
            assert!(!state.selecting_patrol_zone);
        }
    }
    assert_eq!(state.mission_info.objective, Icon::Patrol);
    assert!(state.selecting_patrol_zone);
    assert!(!state.mission);
    assert!(world.resource::<Messages<SendMissionMsg>>().is_empty());
}

#[test]
fn right_click_destination_opens_a_patrol_without_an_existing_draft() {
    let (model, player, _, center) = patrol_setup();
    let mut state = UiState::default();
    state.mission_info.origin = PlanetId::MAX;
    assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
    assert_eq!(state.mission_info.origin, player.home_planet);
    assert_eq!(state.mission_info.objective, Icon::Patrol);
    assert!(state.mission);
    assert_eq!(state.patrol_center, Some(center));
}

#[test]
fn reinforcement_selection_uses_the_owned_operating_patrol_center_and_keeps_the_draft_fleet() {
    let (model, player, mut state, center) = patrol_setup();
    let mut target = Mission::new_patrol(
        900,
        1,
        player.id,
        model.map.get(player.home_planet),
        center,
        Army::new(),
        None,
    );
    let army = state.mission_info.army.clone();
    state.begin_patrol_selection();
    assert!(!state.choose_patrol_reinforcement(&model.map, &player, 1, &target));
    assert!(state.selecting_patrol_zone);
    target.position = center;
    target.patrol.as_mut().unwrap().phase = crate::core::patrols::PatrolPhase::OnStation;
    target.owner = model.players[1].id;
    assert!(!state.choose_patrol_reinforcement(&model.map, &player, 1, &target));
    target.owner = player.id;
    assert!(state.choose_patrol_reinforcement(&model.map, &player, 1, &target));
    assert_eq!(state.mission_info.army, army);
    assert_ne!(state.mission_info.id, target.id);
    assert_eq!(state.mission_info.patrol.as_ref().unwrap().center, center);
    assert_eq!(state.mission_tab, MissionTab::NewMission);
    assert!(state.mission);
    assert!(!state.selecting_patrol_zone);
    target.return_patrol(&model.map, 1);
    assert!(!state.choose_patrol_reinforcement(&model.map, &player, 1, &target));
}

#[test]
fn patrol_draft_combines_fuel_and_previews_full_exposure_for_the_selected_ships() {
    let (model, player, mut state, center) = patrol_setup();
    assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
    let context = egui::Context::default();
    for (army, expected) in [
        (Army::new(), "0.0%"),
        (Army::from([(Unit::Ship(Ship::LightFighter), 20)]), "50.0%"),
        (Army::from([(Unit::Ship(Ship::Cruiser), 20)]), "75.0%"),
        (
            Army::from([(Unit::Ship(Ship::LightFighter), 10), (Unit::Ship(Ship::Cruiser), 10)]),
            "66.7%",
        ),
    ] {
        state.mission_info.army = army;
        let fuel = state.mission_info.dispatch_fuel_consumption(&model.map, &player);
        let upkeep = state.mission_info.patrol_upkeep();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            draw_mission_details(
                ui,
                &mut state.mission_info,
                &model.map,
                &player,
                1,
                &ImageIds::default(),
                None,
            );
        });
        output.textures_delta.clear();
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text()),
                _ => None,
            })
            .collect::<Vec<_>>();
        let fuel_row = format!("⛽ Fuel consumption: {fuel} + {upkeep} / turn");
        let detection_row = format!("Detection probability: {expected}");
        assert!(text.windows(2).any(|rows| rows == [fuel_row.as_str(), detection_row.as_str()]));
        assert!(!text.iter().any(|text| text.contains("Patrol fuel:")));
        if state.mission_info.army.is_empty() {
            assert!(text.contains(&"⛽ Fuel consumption: 0 + 0 / turn"));
        }
    }
}

#[test]
fn patrol_draft_tooltips_explain_both_fuel_costs_and_the_detection_upper_bound() {
    let (model, player, state, center) = patrol_setup();
    for (objective, row, explanation) in [
        (Icon::Patrol, "⛽ Fuel consumption:", "The first amount is the one-time deuterium cost"),
        (Icon::Patrol, "Detection probability:", "Maximum detection probability per turn"),
        (
            Icon::Attack,
            "⛽ Fuel consumption:",
            "Amount of deuterium it costs to send this mission.",
        ),
    ] {
        let mut state = UiState {
            mission_info: state.mission_info.clone(),
            ..default()
        };
        assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
        state.mission_info.objective = objective;
        let context = egui::Context::default();
        context.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let mut target = egui::Rect::NOTHING;
        let mut found_explanation = false;
        for frame in 0..6 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(900.0, 700.0),
                    )),
                    time: Some(frame as f64),
                    events: if frame >= 3 {
                        vec![egui::Event::PointerMoved(target.center())]
                    } else {
                        Vec::new()
                    },
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
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    if text.galley.text().starts_with(row) {
                        target = egui::Rect::from_min_size(text.pos, text.galley.size());
                    }
                    if frame >= 3 && text.galley.text().contains(explanation) {
                        found_explanation = true;
                        if objective == Icon::Patrol {
                            assert!(text.galley.text().contains("after arrival"));
                            if row == "Detection probability:" {
                                assert!(text
                                    .galley
                                    .text()
                                    .contains("fraction of the enemy's route"));
                            } else {
                                assert!(text.galley.text().contains("per turn"));
                            }
                        }
                    }
                }
            }
        }
        assert!(found_explanation, "missing tooltip for {objective:?} {row}");
    }
}

#[test]
fn patrol_uses_the_normal_panel_and_send_action_in_small_windows() {
    for (size, reinforcing, probes_only) in [
        (egui::vec2(820.0, 560.0), false, false),
        (egui::vec2(520.0, 380.0), false, false),
        (egui::vec2(320.0, 280.0), false, false),
        (egui::vec2(820.0, 560.0), true, false),
        (egui::vec2(520.0, 380.0), true, false),
        (egui::vec2(320.0, 280.0), true, false),
        (egui::vec2(820.0, 560.0), false, true),
        (egui::vec2(520.0, 380.0), false, true),
        (egui::vec2(320.0, 280.0), false, true),
        (egui::vec2(820.0, 560.0), true, true),
        (egui::vec2(520.0, 380.0), true, true),
        (egui::vec2(320.0, 280.0), true, true),
    ] {
        for enabled in [true, false] {
            let (mut model, mut player, mut state, center) = patrol_setup();
            assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
            let missions = if reinforcing {
                let mut target = state.mission_info.clone();
                target.id = state.mission_info.id.wrapping_add(1).max(1);
                target.position = center;
                target.patrol.as_mut().unwrap().phase =
                    crate::core::patrols::PatrolPhase::OnStation;
                vec![target]
            } else {
                vec![]
            };
            if probes_only {
                let origin = model.map.get_mut(player.home_planet);
                for unit in Unit::ships() {
                    origin.army.remove(&unit);
                }
                origin.army.insert(Unit::probe(), 3);
                state.mission_info.army = Army::from([(Unit::probe(), 3)]);
            }
            if !enabled {
                player.resources.deuterium = 0;
            }
            let context = egui::Context::default();
            let mut world = World::new();
            world.init_resource::<Messages<SendMissionMsg>>();
            world.init_resource::<Messages<MultiplayerRequest>>();
            let mut params = bevy::ecs::system::SystemState::<(
                MessageWriter<SendMissionMsg>,
                MessageWriter<MultiplayerRequest>,
            )>::new(&mut world);
            for frame in 0..3 {
                let mut keyboard = ButtonInput::default();
                if frame == 2 {
                    keyboard.press(KeyCode::Enter);
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        ..default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            let (mut sends, mut requests) = params.get_mut(&mut world).unwrap();
                            draw_new_mission(
                                ui,
                                &mut sends,
                                &missions,
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
                        });
                    },
                );
                let text = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some(text),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if size.x >= 820.0 {
                    let destination = if reinforcing {
                        "Reinforce"
                    } else {
                        "Deep space"
                    };
                    assert!(text.iter().any(|text| text.galley.text() == destination));
                    for prefix in ["⛽ Fuel consumption:", "Detection probability:"] {
                        let row = text
                            .iter()
                            .find(|text| text.galley.text().starts_with(prefix))
                            .unwrap();
                        assert_eq!(row.galley.rows.len(), 1, "{prefix} should fit in one row");
                        assert!(row.pos.x + row.galley.size().x <= size.x);
                    }
                }
                assert!(!text.iter().any(|text| matches!(
                    text.galley.text(),
                    "Planet mission" | "Until recalled" | "Send Patrol"
                )));
                let send = text.iter().find(|text| text.galley.text() == "Send mission").unwrap();
                let rect = egui::Rect::from_min_size(send.pos, send.galley.size());
                assert!(
                    rect.bottom() <= size.y && rect.right() <= size.x,
                    "send escaped {size:?}: {rect:?}"
                );
                output.textures_delta.clear();
            }
            let sent = world.resource_mut::<Messages<SendMissionMsg>>().drain().collect::<Vec<_>>();
            let can_send = enabled && (!probes_only || reinforcing);
            assert_eq!(sent.len(), usize::from(can_send));
            if can_send {
                assert_eq!(sent[0].mission.objective, Icon::Patrol);
                assert_eq!(sent[0].mission.patrol.as_ref().unwrap().center, center);
                assert!(sent[0].mission.valid_patrol(&model.map));
                let mut app = App::new();
                app.insert_resource(model.map)
                    .insert_resource(player)
                    .insert_resource(Missions(missions))
                    .insert_resource(crate::multiplayer::client::PendingTurnCommands {
                        turn: 1,
                        ..default()
                    })
                    .add_message::<SendMissionMsg>()
                    .add_message::<crate::core::messages::MessageMsg>()
                    .add_message::<crate::core::audio::PlayAudioMsg>()
                    .add_message::<MultiplayerRequest>()
                    .add_systems(Update, crate::core::mission_systems::send_mission);
                app.world_mut().write_message(SendMissionMsg::new(sent[0].mission.clone()));
                app.update();
                assert!(app.world().resource::<Missions>().iter().any(|mission| {
                    mission.id == sent[0].mission.id && mission.army == sent[0].mission.army
                }));
                assert!(matches!(
                    app.world()
                        .resource::<crate::multiplayer::client::PendingTurnCommands>()
                        .commands
                        .as_slice(),
                    [crate::core::simulation::TurnCommand::SendPatrol { .. }]
                ));
                assert_eq!(
                    sent[0].mission.army.amount(&Unit::probe()),
                    if probes_only {
                        3
                    } else {
                        0
                    }
                );
            }
        }
    }
}

#[test]
fn selection_instruction_uses_the_right_notification_stack_and_escape_restores_the_draft() {
    for width in [1000.0, 640.0, 340.0] {
        let (_, _, mut state, _) = patrol_setup();
        state.begin_patrol_selection();
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut toast = egui::Rect::NOTHING;
        let mut cancel_button = egui::Rect::NOTHING;
        let mut text_bounds = egui::Rect::NOTHING;
        for frame in 0..7 {
            let pointer = match frame {
                2 => Some(toast.left_top() + egui::vec2(3.0, 3.0)),
                3 => Some(text_bounds.center()),
                4 => Some(cancel_button.center()),
                5 => Some(egui::pos2(10.0, 650.0)),
                _ => None,
            };
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 700.0),
                    )),
                    events: pointer.into_iter().map(egui::Event::PointerMoved).collect(),
                    ..default()
                },
                |context| {
                    super::super::patrols::draw_selection_toast(context, &mut state, frame == 6);
                },
            );
            if (1..6).contains(&frame) {
                let id = egui::Id::new("patrol zone selection");
                let bounds = context.memory(|memory| memory.area_rect(id)).unwrap();
                toast = context
                    .layer_transform_to_global(egui::LayerId::new(egui::Order::Tooltip, id))
                    .unwrap_or(egui::emath::TSTransform::IDENTITY)
                    .mul_rect(bounds);
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        let bounds = egui::Rect::from_min_size(text.pos, text.galley.size());
                        match text.galley.text() {
                            "Cancel" => cancel_button = bounds,
                            "Click empty space for a patrol zone, or your active patrol to reinforce it." => {
                                text_bounds = bounds;
                            },
                            _ => {},
                        }
                    }
                }
                assert!(toast.right() > width - 30.0 && toast.right() <= width);
                let padding = text_bounds.left() - toast.left();
                assert!((8.0..20.0).contains(&padding), "extra left gap at {width}: {padding}");
                assert!(toast.contains_rect(text_bounds));
                assert!(toast.contains_rect(cancel_button));
                if (2..=4).contains(&frame) {
                    assert!(
                        context.is_pointer_over_egui(),
                        "toast must capture hover at {width}, frame {frame}"
                    );
                } else if frame == 5 {
                    assert!(!context.is_pointer_over_egui());
                }
            }
            output.textures_delta.clear();
        }
        assert!(state.mission);
        assert!(!state.selecting_patrol_zone);
        assert_ne!(state.mission_info.objective, Icon::Patrol);
    }
}

fn active_patrol_fixture() -> (Map, Player, Vec<Mission>) {
    let (mut model, player, state, center) = patrol_setup();
    let home = player.home_planet;
    let enemy_home = model.players[1].home_planet;
    let destination =
        model.map.planets.iter().find(|p| p.id != home && p.id != enemy_home).unwrap().id;
    model.map.get_mut(home).name = "Origin".to_owned();
    model.map.get_mut(enemy_home).name = "Secret launch".to_owned();
    model.map.get_mut(destination).name = "Destination".to_owned();
    let outbound = Mission::new_patrol(
        901,
        1,
        player.id,
        model.map.get(home),
        center,
        state.mission_info.army,
        None,
    );
    let mut stationed = outbound.clone();
    stationed.id = 902;
    stationed.position = center;
    stationed.patrol.as_mut().unwrap().phase = crate::core::patrols::PatrolPhase::OnStation;
    let mut returning = stationed.clone();
    returning.id = 903;
    returning.return_patrol(&model.map, 1);
    let mut attack = outbound.clone();
    attack.id = 904;
    attack.patrol = None;
    attack.objective = Icon::Attack;
    attack.destination = destination;
    let mut foreign = stationed.clone();
    foreign.id = 905;
    foreign.owner = model.players[1].id;
    foreign.origin = enemy_home;
    foreign.destination = enemy_home;
    (model.map, player, vec![outbound, stationed, returning, attack, foreign])
}

#[test]
fn active_patrols_share_compact_rows_with_planet_missions_and_keep_foreign_origins_private() {
    let (map, player, missions) = active_patrol_fixture();
    let objective_texture = egui::TextureId::User(904);
    let protection_texture = egui::TextureId::User(906);
    let images = ImageIds(
        [(Icon::Patrol.asset_key(), objective_texture), ("protect".to_owned(), protection_texture)]
            .into_iter()
            .collect(),
    );
    for width in [700.0, 1_200.0] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut state = UiState::default();
        let mut world = World::new();
        world.init_resource::<Messages<RecallMissionMsg>>();
        let mut params =
            bevy::ecs::system::SystemState::<MessageWriter<RecallMissionMsg>>::new(&mut world);
        for frame in 0..3 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 700.0),
                    )),
                    ..default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        draw_active_missions(
                            ui,
                            missions.iter().collect(),
                            &mut params.get_mut(&mut world).unwrap(),
                            &mut state,
                            &map,
                            &player,
                            &MultiplayerSession::default(),
                            true,
                            &images,
                            true,
                        );
                    });
                },
            );
            output.textures_delta.clear();
            if frame < 2 {
                continue;
            }
            let texts = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let left = texts
                .iter()
                .filter(|text| matches!(text.galley.text(), "Origin" | "Unknown"))
                .collect::<Vec<_>>();
            let right = texts
                .iter()
                .filter(|text| matches!(text.galley.text(), "Destination" | "Deep space"))
                .collect::<Vec<_>>();
            assert_eq!(left.len(), missions.len());
            assert_eq!(right.len(), missions.len());
            assert_eq!(texts.iter().filter(|text| text.galley.text() == "Deep space").count(), 4);
            assert_eq!(texts.iter().filter(|text| text.galley.text() == "∞").count(), 1);
            assert!(!texts.iter().any(|text| text.galley.text() == "—"));
            assert!(!texts.iter().any(|text| text.galley.text().contains("Secret launch")));
            let enemy_name = left.iter().find(|text| text.galley.text() == "Unknown").unwrap();
            let enemy_lane = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Path(path)
                        if path.points.len() > 2
                            && (enemy_name.pos.y - shape.clip_rect.center().y).abs() < 30.0 =>
                    {
                        Some(shape.clip_rect)
                    },
                    _ => None,
                })
                .unwrap();
            let enemy_objective = output
                .shapes
                .iter()
                .find_map(|shape| {
                    let rect = match &shape.shape {
                        egui::Shape::Mesh(mesh) if mesh.texture_id == objective_texture => {
                            mesh.calc_bounds()
                        },
                        egui::Shape::Rect(rect)
                            if rect.brush.is_some()
                                && rect.fill_texture_id() == objective_texture =>
                        {
                            rect.rect
                        },
                        _ => return None,
                    };
                    ((rect.center().y - enemy_lane.center().y).abs() < 0.5).then_some(rect)
                })
                .unwrap();
            assert!((enemy_lane.left() - enemy_objective.right() - 8.0).abs() < 0.5,
                "the enemy patrol animation must use the hidden plan's space: {enemy_lane:?}, {enemy_objective:?}");
            let enemy_destination =
                right.iter().find(|text| (text.pos.y - enemy_name.pos.y).abs() < 0.5).unwrap();
            let endpoint_center = (enemy_name.pos.x
                + enemy_name.galley.size().x * 0.5
                + enemy_destination.pos.x
                + enemy_destination.galley.size().x * 0.5)
                * 0.5;
            assert!(
                (enemy_objective.union(enemy_lane).center().x - endpoint_center).abs() < 0.5,
                "width {width}: the enemy objective and animation must be centered between endpoints"
            );
            let protection_icons = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == protection_texture => {
                        Some(mesh.calc_bounds())
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(protection_icons.is_empty(), "friendship requires at least three players");
            for (left, right) in left.iter().zip(right.iter()) {
                assert_eq!(left.galley.rows.len(), 1);
                assert_eq!(right.galley.rows.len(), 1);
                assert!((left.pos.y - right.pos.y).abs() < 0.5);
                let bounds = egui::Rect::from_min_size(right.pos, right.galley.size());
                assert!(bounds.right() <= width && bounds.bottom() <= 700.0, "{width}: {bounds:?}");
            }
            for rows in [&left, &right] {
                for pair in rows.windows(2) {
                    assert!(
                        (pair[1].pos.y - pair[0].pos.y - MISSION_PLANET_CELL_HEIGHT).abs() < 0.5
                    );
                    let center =
                        |text: &&egui::epaint::TextShape| text.pos.x + text.galley.size().x * 0.5;
                    assert!((center(pair[0]) - center(pair[1])).abs() < 0.5);
                }
            }
        }
    }
}

#[test]
fn two_player_patrols_replace_friendship_with_white_infinity_without_moving_the_row() {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::simulation::PersistedGame;
    use crate::multiplayer::model::{GameMembership, GameRecord};

    let (map, player, missions) = active_patrol_fixture();
    let protection_texture = egui::TextureId::User(920);
    let recall_texture = egui::TextureId::User(921);
    let images = ImageIds(
        [("protect".to_owned(), protection_texture), ("recall".to_owned(), recall_texture)]
            .into_iter()
            .collect(),
    );
    for width in [700.0, 1_200.0] {
        for editable in [false, true] {
            let mut two_player_layout = None;
            for player_count in [2, 3] {
                let model = GameModel::new(
                    [73; 32],
                    GameRules {
                        player_count,
                        ..default()
                    },
                )
                .unwrap();
                let members = model
                    .players
                    .iter()
                    .map(|player| GameMembership {
                        game_id: GameId::new("patrol-access"),
                        player_id: player.id,
                        user_id: UserId::new(format!("player-{}", player.id)),
                        display_name: format!("Player {}", player.id),
                        is_creator: player.id == 1,
                        identity_version: 1,
                        connected: true,
                    })
                    .collect();
                let mut session = MultiplayerSession::default();
                session.active_game = Some(GameRecord {
                    id: GameId::new("patrol-access"),
                    code: GameCode::new("ABCDEF"),
                    revision: 0,
                    saved_at: 0,
                    max_players: player_count,
                    status: model.status,
                    persisted: PersistedGame::new(model),
                    members,
                    submitted_players: vec![],
                });
                let context = egui::Context::default();
                context.set_global_style(NordDark.custom_style());
                let mut state = UiState::default();
                let mut world = World::new();
                world.init_resource::<Messages<RecallMissionMsg>>();
                let mut params =
                    bevy::ecs::system::SystemState::<MessageWriter<RecallMissionMsg>>::new(
                        &mut world,
                    );
                let mut slot = egui::Rect::NOTHING;
                for frame in 0..5 {
                    let mut output = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 700.0),
                            )),
                            time: Some(frame as f64 * 0.1),
                            events: if frame >= 3 {
                                vec![egui::Event::PointerMoved(slot.center())]
                            } else {
                                vec![]
                            },
                            ..default()
                        },
                        |context| {
                            egui::CentralPanel::default().show(context, |ui| {
                                draw_active_missions(
                                    ui,
                                    vec![&missions[1], &missions[3]],
                                    &mut params.get_mut(&mut world).unwrap(),
                                    &mut state,
                                    &map,
                                    &player,
                                    &session,
                                    true,
                                    &images,
                                    editable,
                                );
                            });
                        },
                    );
                    output.textures_delta.clear();
                    if frame < 2 {
                        continue;
                    }
                    let text = |label: &str| {
                        output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                            _ => None,
                        })
                    };
                    let image = |texture| {
                        output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                                Some(mesh.calc_bounds())
                            },
                            _ => None,
                        })
                    };
                    assert_eq!(image(protection_texture).is_some(), player_count >= 3);
                    assert_eq!(text("∞").is_some(), player_count == 2);
                    if frame == 2 {
                        if let Some(infinity) = text("∞") {
                            let eta =
                                text(&format!("+{}", mission_display_turns(&missions[3], &map)))
                                    .unwrap();
                            let format = &infinity.galley.job.sections[0].format;
                            assert_eq!(format.color, Color32::WHITE);
                            assert_eq!(format.font_id, eta.galley.job.sections[0].format.font_id);
                            let center = infinity.pos + infinity.galley.size() * 0.5;
                            slot = egui::Rect::from_center_size(
                                center,
                                egui::Vec2::splat(MISSION_ROUTE_ICON_SIZE),
                            );
                        } else {
                            slot = image(protection_texture).unwrap();
                        }
                        let lane = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::Shape::Path(path) if path.points.len() > 2 => {
                                    Some(shape.clip_rect)
                                },
                                _ => None,
                            })
                            .unwrap();
                        let destination = text("Deep space").unwrap();
                        let layout = [
                            lane,
                            slot,
                            image(recall_texture).unwrap(),
                            egui::Rect::from_min_size(destination.pos, destination.galley.size()),
                        ];
                        assert!((slot.center().y - lane.center().y).abs() < 0.5);
                        if let Some(previous) = two_player_layout {
                            assert_eq!(layout, previous, "width {width}, editable {editable}");
                        } else {
                            two_player_layout = Some(layout);
                        }
                    }
                    if frame == 4 {
                        assert_eq!(text("Manage friendly players").is_some(), player_count >= 3);
                    }
                    assert!(state.patrol_friendly_changes.is_empty());
                }
            }
        }
    }
}

#[test]
fn active_mission_icons_share_the_animation_center_on_both_sides() {
    let (map, player, missions) = active_patrol_fixture();
    let icon_texture = egui::TextureId::User(910);
    let mut images = ImageIds::default();
    for mission in &missions {
        images.0.insert(mission.displayed_objective(player.id).asset_key(), icon_texture);
        let plan = mission.battle_plan_for(Some(player.id)).resolved(&mission.army);
        for key in
            [plan.target_priority.image_key(), plan.protection.image_key(), plan.tactic.image_key()]
        {
            images.0.insert(key.to_owned(), icon_texture);
        }
    }
    for key in ["patrol", "protect", "recall", "recall hover"] {
        images.0.insert(key.to_owned(), icon_texture);
    }

    for (width, pixels_per_point) in [(700.0, 1.0), (1_200.0, 1.0), (700.0, 1.25), (1_200.0, 2.0)] {
        for editable in [false, true] {
            let context = egui::Context::default();
            context.set_pixels_per_point(pixels_per_point);
            context.set_global_style(NordDark.custom_style());
            let mut state = UiState::default();
            let mut world = World::new();
            world.init_resource::<Messages<RecallMissionMsg>>();
            let mut params =
                bevy::ecs::system::SystemState::<MessageWriter<RecallMissionMsg>>::new(&mut world);
            for frame in 0..3 {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 700.0),
                        )),
                        ..default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            draw_active_missions(
                                ui,
                                missions.iter().collect(),
                                &mut params.get_mut(&mut world).unwrap(),
                                &mut state,
                                &map,
                                &player,
                                &MultiplayerSession::default(),
                                true,
                                &images,
                                editable,
                            );
                        });
                    },
                );
                output.textures_delta.clear();
                if frame < 2 {
                    continue;
                }
                // Tessellation can snap image widgets even when their layout boxes are centered.
                let icons = output
                    .shapes
                    .iter()
                    .filter(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) => mesh.texture_id == icon_texture,
                        egui::Shape::Rect(rect) => {
                            rect.brush.is_some() && rect.fill_texture_id() == icon_texture
                        },
                        _ => false,
                    })
                    .flat_map(|shape| {
                        context.tessellate(vec![shape.clone()], output.pixels_per_point)
                    })
                    .filter_map(|primitive| match primitive.primitive {
                        egui::epaint::Primitive::Mesh(mesh) => Some(mesh.calc_bounds()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let lanes = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::LineSegment {
                            stroke,
                            ..
                        } if stroke.color == Color32::from_rgba_unmultiplied(143, 158, 174, 52) => {
                            Some(shape.clip_rect)
                        },
                        egui::Shape::Path(path) if path.points.len() > 2 => Some(shape.clip_rect),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(lanes.len(), missions.len());
                for lane in lanes {
                    let row_icons = icons
                        .iter()
                        .filter(|icon| (icon.center().y - lane.center().y).abs() < 35.0)
                        .collect::<Vec<_>>();
                    assert!(!row_icons.is_empty());
                    for icon in &row_icons {
                        assert!(
                            (icon.center().y - lane.center().y).abs() < 0.01,
                            "width {width}, editable {editable}: lane {lane:?}, icons {row_icons:?}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn patrol_hover_shows_placeholders_on_station_and_travel_details_in_flight() {
    let (map, player, mut missions) = active_patrol_fixture();
    let foreign_owner = missions.last().unwrap().owner;
    let foreign_travel = missions
        .iter()
        .filter(|mission| {
            mission.owner == player.id && mission.patrol.is_some() && !mission.is_patrolling()
        })
        .cloned()
        .map(|mut mission| {
            mission.owner = foreign_owner;
            mission
        })
        .collect::<Vec<_>>();
    missions.extend(foreign_travel);
    for mission in
        missions.iter_mut().filter(|mission| mission.owner != player.id && mission.is_patrolling())
    {
        mission.patrol.as_mut().unwrap().remaining_turns = Some(7);
    }
    for mission in &missions {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let height = mission_info_hover_height(mission, player.id);
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(MISSION_HOVER_INFO_WIDTH, height),
                )),
                ..default()
            },
            |ui| {
                draw_mission_info_hover(
                    ui,
                    mission,
                    &Settings::default(),
                    &map,
                    &player,
                    &MultiplayerSession::default(),
                    &ImageIds::default(),
                );
            },
        );
        output.textures_delta.clear();
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => {
                    assert!(text.pos.y + text.galley.size().y <= height);
                    Some(text.galley.text())
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        for label in ["Distance:", "Movement:", "Duration:"] {
            assert!(text.iter().any(|text| text.contains(label)));
        }
        assert_eq!(text.contains(&"⏱ Duration: ∞"), mission.is_patrolling());
        assert_eq!(text.contains(&"📏 Distance: ---"), mission.is_patrolling());
        assert_eq!(text.contains(&"🚀 Movement: ---"), mission.is_patrolling());
        if mission.is_patrolling() {
            assert!(!text.contains(&"📏 Distance: 0.0 AU"));
            assert!(!text.contains(&"🚀 Movement: 0.00 AU"));
            assert!(!text.iter().any(|text| text.contains("Duration: +")));
        }
        assert!(text.iter().any(|text| text.contains("Objective:")));
        assert_eq!(text.contains(&"Balanced fire"), mission.owner == player.id);
        let objective = if mission.owner == player.id {
            mission.objective.to_name()
        } else if mission.is_returning() {
            "Unknown".to_owned()
        } else {
            "Patrol".to_owned()
        };
        assert!(text.contains(&objective.as_str()));
        let separators = output
            .shapes
            .iter()
            .filter(|shape| matches!(shape.shape, egui::Shape::LineSegment { .. }))
            .count();
        assert_eq!(separators, 2 + usize::from(mission.owner == player.id));
    }
}

#[test]
fn detected_patrol_route_previews_show_maneuvers_only_on_station() {
    let (map, player, missions) = active_patrol_fixture();
    for mission in
        missions.iter().filter(|mission| mission.owner == player.id && mission.patrol.is_some())
    {
        let mut contact = mission.clone();
        contact.owner = player.id + 1;
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            draw_route_preview(
                ui,
                180.0,
                &contact,
                &map,
                &player,
                Color32::RED,
                contact.is_returning(),
            );
        });
        output.textures_delta.clear();
        let has_patrol_path = output
            .shapes
            .iter()
            .any(|shape| matches!(&shape.shape, egui::Shape::Path(path) if path.points.len() > 2));
        let has_maneuver_head =
            output.shapes.iter().any(|shape| matches!(shape.shape, egui::Shape::Circle(_)));
        assert_eq!(has_patrol_path, contact.is_patrolling());
        assert_eq!(has_maneuver_head, contact.is_patrolling());
        assert_eq!(
            contact.displayed_objective(player.id),
            if contact.is_returning() {
                Icon::EnemyFleet
            } else {
                Icon::Patrol
            }
        );
    }
}

#[test]
fn active_patrol_space_links_focus_the_zone_at_each_viewport_scale() {
    let (map, player, mut missions) = active_patrol_fixture();
    let foreign_owner = missions.last().unwrap().owner;
    let foreign_travel = missions
        .iter()
        .filter(|mission| {
            mission.owner == player.id && mission.patrol.is_some() && !mission.is_patrolling()
        })
        .cloned()
        .map(|mut mission| {
            mission.owner = foreign_owner;
            mission
        })
        .collect::<Vec<_>>();
    missions.extend(foreign_travel);
    let deep_space = egui::TextureId::User(903);
    let images = ImageIds([("deep space".to_owned(), deep_space)].into_iter().collect());
    for mission in missions.iter().filter(|mission| mission.patrol.is_some()) {
        let center = mission.patrol.as_ref().unwrap().center;
        let can_focus_zone = mission.owner == player.id || mission.is_patrolling();
        for (viewport, click_image, editable) in
            [egui::vec2(1280.0, 720.0), egui::vec2(800.0, 600.0)]
                .into_iter()
                .flat_map(|viewport| [false, true].map(move |click_image| (viewport, click_image)))
                .flat_map(|(viewport, click_image)| {
                    [false, true].map(move |editable| (viewport, click_image, editable))
                })
        {
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let scale = mission_panel_scale(viewport);
            let size = mission_panel_size(viewport / scale);
            let position = (viewport - size * scale) * 0.5;
            let previous_focus = center + Vec2::splat(100.0);
            let mut state = UiState {
                mission: true,
                mission_tab: if mission.owner == player.id {
                    MissionTab::ActiveMissions
                } else {
                    MissionTab::EnemyMissions
                },
                planet_selected: Some(player.home_planet),
                planet_hover: Some(player.home_planet),
                focus_planet: Some(player.home_planet),
                focus_position: Some(previous_focus),
                focus_zoom: Some(2.0),
                ..default()
            };
            let mut world = World::new();
            world.init_resource::<Messages<RecallMissionMsg>>();
            let mut params =
                bevy::ecs::system::SystemState::<MessageWriter<RecallMissionMsg>>::new(&mut world);
            let mut target_rect = egui::Rect::NOTHING;
            for frame in 0..6 {
                let point = target_rect.lerp_inside(egui::vec2(0.35, 0.65));
                let mut events = Vec::new();
                if frame >= 3 {
                    events.push(egui::Event::PointerMoved(point));
                    if frame >= 4 {
                        events.push(egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: frame == 4,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                        events,
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
                                ui.add_space(17.0);
                                draw_mission_tabs(ui, &mut state.mission_tab);
                                draw_active_missions(
                                    ui,
                                    vec![mission],
                                    &mut params.get_mut(&mut world).unwrap(),
                                    &mut state,
                                    &map,
                                    &player,
                                    &MultiplayerSession::default(),
                                    true,
                                    &images,
                                    editable,
                                );
                            },
                        );
                    },
                );
                output.textures_delta.clear();
                if frame == 2 {
                    target_rect = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Mesh(mesh)
                                if click_image && mesh.texture_id == deep_space =>
                            {
                                Some(mesh.calc_bounds())
                            },
                            egui::Shape::Rect(rect)
                                if click_image
                                    && rect.brush.is_some()
                                    && rect.fill_texture_id() == deep_space =>
                            {
                                Some(rect.rect)
                            },
                            egui::Shape::Text(text)
                                if !click_image && text.galley.text() == "Deep space" =>
                            {
                                Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                            },
                            _ => None,
                        })
                        .unwrap_or_else(|| {
                            panic!(
                                "missing patrol link: mission {}, viewport {viewport:?}, image {click_image}",
                                mission.id
                            )
                        });
                    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)
                        .contains_rect(target_rect));
                }
                if frame == 3 {
                    assert_eq!(
                        output.platform_output.cursor_icon,
                        if can_focus_zone {
                            CursorIcon::PointingHand
                        } else {
                            CursorIcon::Default
                        }
                    );
                    assert_eq!(state.mission_hover, Some(mission.id));
                    assert!(state.mission_hover_from_ui);
                }
                if frame < 5 || !can_focus_zone {
                    assert!(state.mission, "hovering and pressing must not navigate yet");
                    assert_eq!(state.focus_position, Some(previous_focus));
                    assert!(state.location_marker.is_none());
                }
            }
            assert!(world.resource_mut::<Messages<RecallMissionMsg>>().drain().next().is_none());
            if !can_focus_zone {
                assert!(!state.to_selected);
                assert_eq!(state.planet_selected, Some(player.home_planet));
                continue;
            }
            assert_eq!(state.focus_position, Some(center));
            assert_eq!(state.location_marker.as_ref().unwrap().position, center);
            assert!(state.to_selected);
            assert!(!state.mission);
            assert_eq!(state.planet_selected, None);
            assert_eq!(state.planet_hover, None);
            assert_eq!(state.focus_planet, None);
            assert_eq!(state.focus_zoom, None);
            assert_eq!(state.mission_planet_hover, None);
            assert_eq!(state.mission_hover, None);
            assert!(!state.mission_hover_from_ui);
        }
    }
}

#[test]
fn active_patrol_recall_uses_the_standard_control_and_deep_space_hovers_the_fleet() {
    let (map, player, missions) = active_patrol_fixture();
    let images = ImageIds(
        [
            ("recall".to_owned(), egui::TextureId::User(901)),
            ("recall hover".to_owned(), egui::TextureId::User(902)),
        ]
        .into_iter()
        .collect(),
    );
    for mission in missions.iter().filter(|mission| mission.patrol.is_some()) {
        for editable in [false, true] {
            let context = egui::Context::default();
            let mut state = UiState::default();
            let mut world = World::new();
            world.init_resource::<Messages<RecallMissionMsg>>();
            let mut params =
                bevy::ecs::system::SystemState::<MessageWriter<RecallMissionMsg>>::new(&mut world);
            let can_recall = mission.owner == player.id && !mission.is_returning();
            let mut recall_rect = egui::Rect::NOTHING;
            let mut space_rect = egui::Rect::NOTHING;
            for frame in 0..7 {
                let mut events = Vec::new();
                if frame >= 3 {
                    let position = if frame == 3 || !can_recall {
                        space_rect.center()
                    } else {
                        recall_rect.center()
                    };
                    events.push(egui::Event::PointerMoved(position));
                    if frame >= 5 {
                        events.push(egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: frame == 5,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(700.0, 350.0),
                        )),
                        events,
                        ..default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            draw_active_missions(
                                ui,
                                vec![mission],
                                &mut params.get_mut(&mut world).unwrap(),
                                &mut state,
                                &map,
                                &player,
                                &MultiplayerSession::default(),
                                true,
                                &images,
                                editable,
                            );
                        });
                    },
                );
                output.textures_delta.clear();
                let recall = output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh)
                        if matches!(mesh.texture_id, egui::TextureId::User(901 | 902)) =>
                    {
                        Some(mesh.calc_bounds())
                    },
                    _ => None,
                });
                if frame >= 2 {
                    assert_eq!(recall.is_some(), can_recall);
                }
                if let Some(rect) = recall {
                    recall_rect = rect;
                    assert_eq!(rect.size(), egui::Vec2::splat(MISSION_RECALL_BUTTON_SIZE));
                }
                space_rect = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Deep space" => {
                            Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                        },
                        _ => None,
                    })
                    .unwrap_or(space_rect);
                if frame == 3 {
                    assert_eq!(state.mission_hover, Some(mission.id));
                    assert!(state.mission_hover_from_ui);
                    assert!(state.planet_selected.is_none());
                    assert!(state.mission_planet_hover.is_none());
                }
            }
            let recalls =
                world.resource_mut::<Messages<RecallMissionMsg>>().drain().collect::<Vec<_>>();
            assert_eq!(recalls.len(), usize::from(editable && can_recall));
            if let Some(recall) = recalls.first() {
                assert_eq!(recall.mission_id, mission.id);
            }
        }
    }
}
