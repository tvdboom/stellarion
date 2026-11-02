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
    assert_eq!(draft_planet_destination(&state, &model.map), planet_destination);
}

#[test]
fn clicking_the_patrol_objective_icon_starts_map_selection() {
    let (mut model, mut player, mut state, _) = patrol_setup();
    let context = egui::Context::default();
    let patrol_image = egui::TextureId::User(991);
    let images = ImageIds(std::collections::HashMap::from([("patrol".to_owned(), patrol_image)]));
    let mut world = World::new();
    world.init_resource::<Messages<SendMissionMsg>>();
    world.init_resource::<Messages<MultiplayerRequest>>();
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<SendMissionMsg>,
        MessageWriter<MultiplayerRequest>,
    )>::new(&mut world);
    let mut icon_rect = egui::Rect::NOTHING;
    for frame in 0..4 {
        let mut events = vec![];
        if frame >= 2 {
            events.push(egui::Event::PointerMoved(icon_rect.center()));
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
        if frame < 2 {
            icon_rect = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == patrol_image => {
                        Some(mesh.calc_bounds())
                    },
                    _ => None,
                })
                .unwrap();
        }
        output.textures_delta.clear();
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
fn patrol_uses_the_normal_panel_and_send_action_in_small_windows() {
    for size in [egui::vec2(820.0, 560.0), egui::vec2(520.0, 380.0), egui::vec2(320.0, 280.0)] {
        for enabled in [true, false] {
            let (mut model, mut player, mut state, center) = patrol_setup();
            assert!(state.choose_patrol_zone(&model.map, &player, 1, center));
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
                    assert!(text.iter().any(|text| text.galley.text() == "Deep space"));
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
            assert_eq!(sent.len(), usize::from(enabled));
            if enabled {
                assert_eq!(sent[0].mission.objective, Icon::Patrol);
                assert_eq!(sent[0].mission.patrol.as_ref().unwrap().center, center);
                assert!(sent[0].mission.valid_patrol(&model.map));
            }
        }
    }
}

#[test]
fn selection_instruction_uses_the_right_notification_stack_and_escape_restores_the_draft() {
    let (_, _, mut state, _) = patrol_setup();
    state.begin_patrol_selection();
    let context = egui::Context::default();
    for frame in 0..3 {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 700.0),
                )),
                ..default()
            },
            |context| {
                super::super::patrols::draw_selection_toast(context, &mut state, frame == 2);
            },
        );
        if frame == 1 {
            let text = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == "Click empty space to choose a patrol zone." =>
                    {
                        Some(text)
                    },
                    _ => None,
                })
                .unwrap();
            assert!(text.pos.x > 500.0, "instruction is not right aligned: {:?}", text.pos);
        }
        output.textures_delta.clear();
    }
    assert!(state.mission);
    assert!(!state.selecting_patrol_zone);
    assert_ne!(state.mission_info.objective, Icon::Patrol);
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
                            &ImageIds::default(),
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
            assert_eq!(texts.iter().filter(|text| text.galley.text() == "—").count(), 1);
            assert!(!texts.iter().any(|text| text.galley.text().contains("Secret launch")));
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
