use super::*;

#[test]
fn enemy_mission_origin_shows_all_foreign_planet_objectives() {
    use crate::core::turns::filter_missions;

    let mut home = Planet::new(0, "Home".into(), Vec2::ZERO, false, 1.0);
    home.owned = Some(1);
    home.controlled = Some(1);
    home.army.extend(Unit::ships().into_iter().map(|unit| (unit, 10)));
    home.army.insert(Unit::interplanetary_missile(), 10);
    home.army.insert(Unit::Building(Building::SensorPhalanx), 2);
    let mut enemy = Planet::new(1, "Enemy origin".into(), Vec2::X * 1_000.0, false, 1.0);
    enemy.owned = Some(2);
    enemy.controlled = Some(2);
    let mut incoming = Mission::new_with_id(
        7,
        1,
        2,
        &enemy,
        &home,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Cruiser), 3)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    incoming.position = Vec2::X * Planet::SIZE;
    let map = Map {
        rect: Rect::from_corners(Vec2::splat(-2_000.0), Vec2::splat(2_000.0)),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![home.clone(), enemy.clone()],
    };
    let objectives =
        [Icon::Colonize, Icon::Attack, Icon::Raid, Icon::Spy, Icon::MissileStrike, Icon::Destroy];
    let images = ImageIds(
        objectives
            .iter()
            .enumerate()
            .map(|(index, icon)| (icon.asset_key(), egui::TextureId::User(index as u64 + 1)))
            .collect(),
    );

    // Check both a newly detected fleet and the report retained after it arrives.
    for resolved in [false, true] {
        for viewport in [egui::vec2(800.0, 600.0), egui::vec2(1280.0, 720.0)] {
            for selected in objectives {
                let mut player = Player::new(1, home.id);
                assert_eq!(player.known_controller(&enemy, &[]), None);
                let missions = if resolved {
                    player
                        .reports
                        .push(crate::test_support::empty_report(incoming.clone(), home.clone()));
                    vec![]
                } else {
                    filter_missions(std::slice::from_ref(&incoming), &map, &player)
                };
                assert_eq!(player.known_controller(&enemy, &missions), Some(2));
                assert_eq!(visible_planet_affiliation(&map, &player, &missions, &enemy), Some(2));
                let mut map = map.clone();
                let mut state = UiState {
                    mission: true,
                    mission_info: Mission::from_mission(
                        2,
                        player.id,
                        &home,
                        &enemy,
                        &Mission {
                            objective: selected,
                            ..default()
                        },
                    ),
                    ..default()
                };
                let context = egui::Context::default();
                context.set_global_style(NordDark.custom_style());
                let scale = mission_panel_scale(viewport);
                let size = mission_panel_size(viewport / scale);
                let position = (viewport - size * scale) * 0.5;
                let mut world = World::new();
                world.init_resource::<Messages<SendMissionMsg>>();
                world.init_resource::<Messages<MultiplayerRequest>>();
                let mut params = bevy::ecs::system::SystemState::<(
                    MessageWriter<SendMissionMsg>,
                    MessageWriter<MultiplayerRequest>,
                )>::new(&mut world);
                for frame in 0..3 {
                    let mut output = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                viewport,
                            )),
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
                                    let (mut sends, mut requests) =
                                        params.get_mut(&mut world).unwrap();
                                    draw_new_mission(
                                        ui,
                                        &mut sends,
                                        &missions,
                                        &Settings {
                                            turn: 2,
                                            p_colonizable: 100,
                                            ..default()
                                        },
                                        &mut state,
                                        &mut map,
                                        &mut player,
                                        &MultiplayerSession::default(),
                                        &mut requests,
                                        true,
                                        &ButtonInput::default(),
                                        &images,
                                    );
                                },
                            );
                        },
                    );
                    output.textures_delta.clear();
                    if frame < 2 {
                        continue;
                    }
                    for objective in objectives {
                        assert!(
                            output.shapes.iter().any(|shape| {
                                matches!(&shape.shape, egui::Shape::Mesh(mesh)
                                if mesh.texture_id == images.get(objective.asset_key())
                                    && (mesh.calc_bounds().width() - 40.0 * scale).abs() < 0.1
                                    && shape.clip_rect.contains_rect(mesh.calc_bounds()))
                            }),
                            "missing or clipped {objective:?}: resolved={resolved}, viewport={viewport:?}"
                        );
                    }
                    assert_eq!(state.mission_info.objective, selected);
                }
            }
        }
    }
}

#[test]
fn deploy_drafts_hide_intercept_and_keep_colonize_on_controlled_planets() {
    use crate::core::simulation::{GameModel, GameRules};

    for owned in [false, true] {
        for width in [650.0, 900.0] {
            let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
            model.start().unwrap();
            let mut player = model.players[0].clone();
            let origin = player.home_planet;
            let destination = model.players[1].home_planet;
            model.map.get_mut(origin).army.insert(Unit::Ship(Ship::Cruiser), 3);
            model.map.get_mut(origin).army.remove(&Unit::colony_ship());
            model.map.get_mut(destination).owned = owned.then_some(player.id);
            model.map.get_mut(destination).controlled = Some(player.id);
            let mut state = UiState {
                mission_info: Mission {
                    origin,
                    destination,
                    objective: Icon::Deploy,
                    ..default()
                },
                ..default()
            };
            let objective_icons = [Icon::Colonize, Icon::Deploy, Icon::Patrol, Icon::Intercept];
            let images = ImageIds(
                objective_icons
                    .iter()
                    .enumerate()
                    .map(|(index, icon)| {
                        (icon.asset_key(), egui::TextureId::User(index as u64 + 1))
                    })
                    .collect(),
            );
            let context = egui::Context::default();
            let mut world = World::new();
            world.init_resource::<Messages<SendMissionMsg>>();
            world.init_resource::<Messages<MultiplayerRequest>>();
            let mut params = bevy::ecs::system::SystemState::<(
                MessageWriter<SendMissionMsg>,
                MessageWriter<MultiplayerRequest>,
            )>::new(&mut world);
            for _ in 0..2 {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 650.0),
                        )),
                        ..default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            let (mut sends, mut requests) = params.get_mut(&mut world).unwrap();
                            draw_new_mission(
                                ui,
                                &mut sends,
                                &[],
                                &Settings::default(),
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
                let visible_icons = objective_icons
                    .into_iter()
                    .filter(|icon| {
                        output.shapes.iter().any(|shape| {
                            matches!(&shape.shape, egui::Shape::Mesh(mesh)
                                if mesh.texture_id == images.get(icon.asset_key())
                                    && mesh.calc_bounds().size() == egui::Vec2::splat(40.0))
                        })
                    })
                    .collect::<Vec<_>>();
                let expected = if owned {
                    vec![Icon::Deploy, Icon::Patrol]
                } else {
                    vec![Icon::Colonize, Icon::Deploy, Icon::Patrol]
                };
                assert_eq!(visible_icons, expected, "owned={owned}, width={width}");
                assert_eq!(state.mission_info.objective, Icon::Deploy);
            }
        }
    }
}

#[test]
fn intercept_drafts_remain_selected_with_a_friendly_previous_destination() {
    let player = Player::new(1, 1);
    let mission = Mission {
        objective: Icon::Intercept,
        ..default()
    };
    for owned in [false, true] {
        let mut destination = Planet::new(2, "Friendly".into(), Vec2::X, false, 1.0);
        destination.controlled = Some(player.id);
        destination.owned = owned.then_some(player.id);
        assert!(draft_objectives_for_mission(&player, &destination, &mission)
            .contains(&Icon::Intercept));
    }
}

#[test]
fn intercepted_fleet_reports_require_survivors_to_reveal_attackers_and_combat() {
    for objective in [Icon::Patrol, Icon::Intercept] {
        for survivors in [0, 1] {
            let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
            let mut encounter = Planet::new(1, "Deep space".into(), Vec2::X, false, 1.0);
            encounter.controlled = Some(2);
            encounter.army.insert(Unit::Ship(Ship::LightFighter), 6);
            let mut report = crate::test_support::empty_report(
                Mission {
                    owner: 1,
                    origin: origin.id,
                    destination: encounter.id,
                    objective,
                    army: Army::from([(Unit::Ship(Ship::Cruiser), 17)]),
                    ..default()
                },
                encounter.clone(),
            );
            report.id = 90;
            report.turn = 1;
            report.surviving_attacker = report.mission.army.clone();
            report.surviving_defender =
                Army::from([(Unit::Ship(Ship::LightFighter), survivors)]).into();
            report.combat_report = Some(crate::core::combat::report::CombatReport {
                rounds: vec![RoundReport::default()],
                ..default()
            });
            let mut player = Player::new(2, encounter.id);
            player.reports.push(report);
            assert_eq!(combat_selection_reports(&player, 1).len(), usize::from(survivors > 0));
            let map = Map {
                rect: Rect::default(),
                solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
                planets: vec![origin, encounter],
            };

            for viewport in [egui::vec2(800.0, 600.0), egui::vec2(1280.0, 720.0)] {
                let context = egui::Context::default();
                context.set_global_style(NordDark.custom_style());
                let mut state = UiState {
                    mission_report: Some(90),
                    ..default()
                };
                for frame in 0..3 {
                    let mut output = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                viewport,
                            )),
                            ..default()
                        },
                        |ui| {
                            draw_mission_reports(
                                ui,
                                &mut state,
                                &map,
                                &player,
                                &MultiplayerSession::default(),
                                false,
                                &ImageIds::default(),
                            );
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
                            egui::Shape::Text(text) => Some(text.galley.text()),
                            _ => None,
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        texts.iter().filter(|text| **text == "?").count(),
                        if survivors == 0 {
                            Unit::ships().len()
                        } else {
                            0
                        },
                        "{objective:?}, {survivors} survivors, {viewport:?}"
                    );
                    assert!(texts.contains(&format!("{}/6", 6 - survivors).as_str()));
                    assert_eq!(texts.contains(&"17"), survivors > 0);
                    assert_eq!(texts.contains(&"Combat details"), survivors > 0);
                }
            }
        }
    }
}

#[test]
fn friendly_patrol_reports_show_detection_below_ships_without_header_badges() {
    let model = crate::core::simulation::GameModel::new([73; 32], default()).unwrap();
    let map = &model.map;
    let mut player = model.players[0].clone();
    let origin = map.get(player.home_planet);
    let mut mission = Mission::new_patrol(
        700,
        1,
        player.id,
        origin,
        origin.position + Vec2::splat(Planet::SIZE),
        Army::from([(Unit::Ship(Ship::Cruiser), 3)]),
        None,
    );
    mission.patrol.as_mut().unwrap().friendly_players.insert(2);
    let mut contact = origin.clone();
    contact.name = "Friendly fleet contact".into();
    contact.controlled = Some(2);
    contact.owned = None;
    contact.army = Army::from([(Unit::Ship(Ship::Cruiser), 17)]).into();
    let mut report = crate::test_support::empty_report(mission, contact.clone());
    report.id = 90;
    report.turn = 1;
    report.destination_controlled = Some(2);
    report.surviving_attacker = report.mission.army.clone();
    report.surviving_defender = contact.army;
    player.reports.push(report);
    let protect_texture = egui::TextureId::User(901);
    let ship_texture = egui::TextureId::User(902);
    let mut images = ImageIds::default();
    images.0.insert("protect".into(), protect_texture);
    for ship in Unit::ships() {
        images.0.insert(ship.to_lowername(), ship_texture);
    }
    for viewport in [egui::vec2(1280.0, 720.0), egui::vec2(850.0, 640.0), egui::vec2(800.0, 600.0)]
    {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let scale = mission_panel_scale(viewport);
        let size = mission_panel_size(viewport / scale);
        let position = (viewport - size * scale) * 0.5;
        let mut state = UiState {
            mission_tab: MissionTab::MissionReports,
            mission_report: Some(90),
            ..default()
        };
        let mut panel = egui::Rect::NOTHING;
        for frame in 0..3 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                    ..default()
                },
                |ui| {
                    draw_panel_on_context(
                        ui,
                        "mission",
                        "panel",
                        (position.x, position.y),
                        (size.x, size.y),
                        0.0,
                        scale,
                        &images,
                        |ui| {
                            panel = ui.max_rect();
                            ui.add_space(17.0);
                            draw_mission_tabs(ui, &mut state.mission_tab);
                            draw_mission_reports(
                                ui,
                                &mut state,
                                map,
                                &player,
                                &MultiplayerSession::default(),
                                false,
                                &images,
                            );
                        },
                    );
                },
            );
            output.textures_delta.clear();
            if frame < 2 {
                continue;
            }
            let text_rects = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.text(),
                        egui::Rect::from_min_size(text.pos, text.galley.size()),
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let detection = text_rects
                .iter()
                .find(|(text, _)| *text == "Friendly Fleet Detected")
                .expect("friendly detection label")
                .1;
            assert!(!text_rects.iter().any(|(text, _)| text.contains("No combat")));
            assert!(text_rects.iter().any(|(text, _)| *text == "17"));
            assert!(!text_rects.iter().any(|(text, _)| *text == "Combat details"));
            let image_rects = |texture| {
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
            let shields = image_rects(protect_texture);
            assert_eq!(shields.len(), 1, "only the detection footer should show a shield");
            let shield = shields[0];
            let ships = image_rects(ship_texture);
            assert_eq!(ships.len(), Unit::ships().len() * 2);
            assert!(ships.iter().all(|ship| ship.bottom() < shield.top()));
            assert!(ships.iter().all(|ship| ship.bottom() < detection.top()));
            assert!(shield.right() < detection.left());
            assert!((shield.center().y - detection.center().y).abs() < scale);
            let panel = egui::emath::TSTransform::from_scaling(scale).mul_rect(panel);
            assert!(panel.contains_rect(shield), "shield overflowed panel at {viewport:?}");
            assert!(panel.contains_rect(detection), "label overflowed panel at {viewport:?}");
        }
    }
}

#[test]
fn objective_buttons_keep_their_size_and_selection_while_hovering_and_pressing() {
    let image = egui::TextureId::User(910);
    for enabled in [true, false] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut button = egui::Rect::NOTHING;
        let mut selected = false;
        let mut clicks = 0;
        for step in 0..7 {
            let mut events = Vec::new();
            if (1..=4).contains(&step) {
                events.push(egui::Event::PointerMoved(button.center()));
            } else {
                events.push(egui::Event::PointerGone);
            }
            if step == 3 || step == 4 {
                events.push(egui::Event::PointerButton {
                    pos: button.center(),
                    button: egui::PointerButton::Primary,
                    pressed: step == 3,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(330.0, 140.0),
                    )),
                    time: Some(step as f64 * 0.2),
                    events,
                    ..default()
                },
                |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        let response =
                            draw_mission_objective_button(ui, image, Icon::Raid, selected);
                        if step > 0 {
                            assert_eq!(response.rect, button);
                        }
                        button = response.rect;
                        if response.clicked() {
                            clicks += 1;
                            selected = true;
                        }
                    });
                },
            );
            output.textures_delta.clear();
            let artwork = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == image => Some(mesh.calc_bounds()),
                    _ => None,
                })
                .unwrap();
            assert_eq!(button.size(), MISSION_OBJECTIVE_BUTTON_SIZE);
            assert_eq!(artwork.size(), egui::vec2(40.0, 40.0));
            assert_eq!(
                artwork.min.y - button.min.y,
                if step == 3 && enabled {
                    3.0
                } else {
                    2.0
                }
            );
            assert!(button.contains_rect(artwork));
            let markers = output.shapes.iter().filter(|shape| matches!(&shape.shape,
                egui::Shape::Rect(rect) if rect.fill == MISSION_OBJECTIVE_ACCENT && rect.rect.size() == egui::vec2(22.0, 2.0)
            )).count();
            if enabled && step >= 5 {
                assert_eq!(markers, 1);
            } else {
                assert_eq!(markers, 0);
            }
        }
        assert_eq!(clicks, usize::from(enabled));
        assert_eq!(selected, enabled);
    }
}

#[test]
fn objective_buttons_support_keyboard_activation_and_fit_all_seven_choices() {
    let context = egui::Context::default();
    let mut activated = false;
    for step in 0..3 {
        let mut output = context.run_ui(
            egui::RawInput {
                events: if step == 0 {
                    vec![]
                } else {
                    vec![egui::Event::Key {
                        key: egui::Key::Enter,
                        physical_key: None,
                        pressed: step == 1,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]
                },
                ..default()
            },
            |ui| {
                let response =
                    draw_mission_objective_button(ui, egui::TextureId::User(1), Icon::Raid, true);
                if step == 0 {
                    response.request_focus();
                }
                activated |= response.clicked();
            },
        );
        output.textures_delta.clear();
    }
    assert!(activated);
    for width in [330.0, 480.0] {
        let context = egui::Context::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            ui.set_width(width);
            let left = ui.cursor().left();
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = MISSION_OBJECTIVE_BUTTON_GAP;
                for icon in Icon::objectives(false, false, true, false) {
                    let response = draw_mission_objective_button(
                        ui,
                        egui::TextureId::User(1),
                        icon,
                        icon == Icon::Attack,
                    );
                    assert!(response.rect.right() - left <= width);
                }
            });
        });
        output.textures_delta.clear();
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[path = "ui_objective_preview.rs"]
mod objective_preview;

#[test]
fn mission_report_combat_button_stays_inside_the_panel_and_opens_details() {
    let mut destination = Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
    destination.owned = Some(2);
    destination.controlled = Some(2);
    destination.army = Unit::ships()
        .into_iter()
        .chain(Unit::defenses())
        .chain(Unit::buildings())
        .map(|unit| (unit, 5))
        .collect::<Army>()
        .into();
    let mission = Mission {
        owner: 1,
        origin: 0,
        destination: 1,
        objective: Icon::Attack,
        army: Unit::ships().into_iter().map(|unit| (unit, 6)).collect(),
        ..default()
    };
    let mut report = crate::test_support::empty_report(mission, destination.clone());
    report.id = 42;
    report.scout_probes = 2;
    report.combat_report = Some(crate::core::combat::report::CombatReport {
        rounds: vec![RoundReport::default()],
        ..default()
    });
    let mut player = Player::new(2, 1);
    player.reports.push(report);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0), destination],
    };
    let button_texture = egui::TextureId::User(90);
    let fighter_texture = egui::TextureId::User(91);
    let origin_texture = egui::TextureId::User(92);
    let resource_textures = [93, 94, 95].map(egui::TextureId::User);
    let images = ImageIds(HashMap::from([
        ("button".to_owned(), button_texture),
        ("button hover".to_owned(), button_texture),
        (Unit::Ship(Ship::LightFighter).to_lowername().to_string(), fighter_texture),
        (map.get(0).image(), origin_texture),
        ("metal".to_owned(), resource_textures[0]),
        ("crystal".to_owned(), resource_textures[1]),
        ("deuterium".to_owned(), resource_textures[2]),
    ]));
    for (objective, loot, viewport) in [
        (Icon::Attack, Resources::new(21, 1, 0)),
        (Icon::Raid, Resources::new(21, 1, 0)),
        (Icon::Raid, Resources::default()),
        (Icon::Raid, Resources::new(1_234_567, 89, 0)),
        (Icon::Raid, Resources::new(1_234_567, 8_901_234, 5_678_901)),
    ]
    .into_iter()
    .flat_map(|(objective, loot)| {
        [
            egui::vec2(1920.0, 1080.0),
            egui::vec2(1280.0, 720.0),
            egui::vec2(850.0, 640.0),
            egui::vec2(800.0, 600.0),
        ]
        .map(|viewport| (objective, loot, viewport))
    }) {
        player.reports[0].mission.objective = objective;
        player.reports[0].raid_loot = loot;
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let scale = mission_panel_scale(viewport);
        let size = mission_panel_size(viewport / scale);
        let position = (viewport - size * scale) * 0.5;
        let mut state = UiState {
            mission_tab: MissionTab::MissionReports,
            ..default()
        };
        let mut panel = egui::Rect::NOTHING;
        let mut button = egui::Rect::NOTHING;
        for step in 0..4 {
            let mut events = Vec::new();
            if step >= 2 {
                events.push(egui::Event::PointerMoved(button.center()));
                events.push(egui::Event::PointerButton {
                    pos: button.center(),
                    button: egui::PointerButton::Primary,
                    pressed: step == 2,
                    modifiers: default(),
                });
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
                            panel = ui.max_rect();
                            ui.add_space(17.0);
                            draw_mission_tabs(ui, &mut state.mission_tab);
                            draw_mission_reports(
                                ui,
                                &mut state,
                                &map,
                                &player,
                                &MultiplayerSession::default(),
                                false,
                                &images,
                            );
                        },
                    );
                },
            );
            output.textures_delta.clear();
            let Some(painted_button) = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == button_texture => {
                    Some(mesh.calc_bounds())
                },
                _ => None,
            }) else {
                assert_eq!(step, 0, "combat details button should be painted after sizing");
                continue;
            };
            button = painted_button;
            let panel = egui::emath::TSTransform::from_scaling(scale).mul_rect(panel);
            assert!(
                panel.contains_rect(button),
                "button {button:?} overflowed panel {panel:?} at {viewport:?}"
            );
            assert!(
                (panel.right() - button.right() - MISSION_REPORT_FOOTER_RIGHT_INSET * scale).abs()
                    < 1.0,
                "details button must stay anchored to the panel's right edge at {viewport:?}"
            );
            assert!(
                (panel.bottom() - button.bottom() - MISSION_REPORT_FOOTER_BOTTOM_INSET * scale)
                    .abs()
                    < 1.0,
                "details button must stay above the panel's lower border at {viewport:?}"
            );
            let image_rects = |texture| {
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
            let fighters = image_rects(fighter_texture);
            assert_eq!(fighters.len(), 2);
            assert!(
                (fighters[0].top() - fighters[1].top()).abs() < 1.0,
                "attacker and defender grids must start at the same height: {fighters:?}"
            );
            let origin = image_rects(origin_texture)
                .into_iter()
                .find(|rect| (rect.width() - MISSION_PLANET_IMAGE_SIZE * scale).abs() < 1.0)
                .expect("report origin planet");
            let bar = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if (rect.rect.height() - 6.0 * scale).abs() < 0.1
                            && (rect.rect.width() - MISSION_REPORT_STRENGTH_BAR_WIDTH * scale)
                                .abs()
                                < 0.1 =>
                    {
                        Some(rect.rect)
                    },
                    _ => None,
                })
                .expect("attacker strength bar");
            let caption_bottom = origin.bottom() + 4.0 * scale;
            assert!(bar.top() >= caption_bottom);
            assert!(
                bar.top() - caption_bottom <= 20.0 * scale,
                "planet caption and strength bar are too far apart: {origin:?}, {bar:?}"
            );
            let divider = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::LineSegment {
                        points,
                        stroke,
                    } if (stroke.width - 0.75 * scale).abs() < 0.01
                        && points[0].x > fighters[0].right()
                        && points[0].x < fighters[1].left() =>
                    {
                        Some(egui::Rect::from_two_pos(points[0], points[1]))
                    },
                    _ => None,
                })
                .expect("thin attacker/defender divider");
            let mut resource_right = f32::NEG_INFINITY;
            let mut resource_top: Option<f32> = None;
            for texture in resource_textures {
                let resources = image_rects(texture);
                if objective == Icon::Raid {
                    assert_eq!(resources.len(), 1);
                    let icon = resources[0];
                    assert!(panel.contains_rect(icon), "raid resource overflowed panel: {icon:?}");
                    assert!(
                        !icon.intersects(button),
                        "raid resource {icon:?} overlaps combat button {button:?} at {viewport:?}"
                    );
                    assert!(
                        (icon.top()
                            - divider.bottom()
                            - (MISSION_REPORT_LOOT_TOP_GAP + 1.0) * scale)
                            .abs()
                            < 1.0,
                        "raid resources must sit just below the armies and divider at {viewport:?}: \
                         icon {icon:?}, divider {divider:?}, scale {scale}"
                    );
                    if let Some(top) = resource_top {
                        assert!((icon.top() - top).abs() < 0.1, "raid resources must align");
                    }
                    resource_top = Some(icon.top());
                    for shape in &output.shapes {
                        if let egui::Shape::Text(text) = &shape.shape {
                            if text.galley.text() == "Scouts: 2" {
                                assert!(
                                    !icon.intersects(egui::Rect::from_min_size(
                                        text.pos,
                                        text.galley.size(),
                                    )),
                                    "raid resource {icon:?} overlaps the scouts count"
                                );
                            }
                        }
                    }
                    assert!(
                        icon.left() > resource_right,
                        "raid resources must stay on one row in resource order"
                    );
                    resource_right = icon.right();
                } else {
                    assert!(resources.is_empty());
                }
            }
            let text = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(!text.iter().any(|text| text.contains("Raided cargo")
                || text.contains("Credited when")
                || text.contains("No resources seized")));
            if objective == Icon::Raid {
                for resource in CRAWLER_SALVAGE_RESOURCE_ORDER {
                    assert!(text.contains(&format_thousands(loot.get(&resource)).as_str()));
                }
            }
        }
        assert_eq!(state.combat_report, Some(42));
        assert_eq!(state.combat_report_round, 1);
    }
}

#[test]
fn raid_resource_icons_share_the_hud_border_and_show_their_names_on_hover() {
    let resources = [("metal", "Metal"), ("crystal", "Crystal"), ("deuterium", "Deuterium")];
    let images = ImageIds(
        resources
            .iter()
            .enumerate()
            .map(|(index, (key, _))| (key.to_string(), egui::TextureId::User(index as u64 + 1)))
            .collect(),
    );
    for (key, name) in resources {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        context.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let mut icon = egui::Rect::NOTHING;
        for step in 0..5 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 200.0),
                    )),
                    time: Some(step as f64),
                    events: if step >= 2 {
                        vec![egui::Event::PointerMoved(icon.center())]
                    } else {
                        vec![]
                    },
                    ..default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        draw_raid_loot(ui, Resources::new(61, 19, 4), &images);
                    });
                },
            );
            output.textures_delta.clear();
            icon = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == images.get(key) => {
                        Some(mesh.calc_bounds())
                    },
                    _ => None,
                })
                .expect("raid resource icon");
            assert!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Rect(rect)
                    if rect.stroke.color == RESOURCE_IMAGE_BORDER_COLOR
                        && rect.stroke.width == 2.0
                        && rect.rect == icon.expand(1.0))
            }));
            let tooltip_shown = output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == name)
            });
            if step < 2 {
                assert!(!tooltip_shown, "resource names must wait for hover");
            } else if step == 4 {
                assert!(tooltip_shown, "hover must identify {name}");
            }
        }
    }
}

#[test]
fn report_selection_distinguishes_encounter_from_later_arrival_for_one_mission() {
    let planet = Planet::new(2, "Destination".into(), Vec2::ZERO, false, 1.0);
    let mission = Mission {
        id: 42,
        destination: planet.id,
        objective: Icon::Protect,
        ..default()
    };
    let encounter = MissionReport {
        id: 7,
        turn: 4,
        ..crate::test_support::empty_report(mission.clone(), planet.clone())
    };
    let arrival = MissionReport {
        id: 8,
        turn: 7,
        ..crate::test_support::empty_report(mission, planet)
    };
    let reports = [&encounter, &arrival];

    assert_eq!(
        selected_mission_report(&reports, Some(arrival.id)).map(|report| report.id),
        Some(8)
    );
    assert_eq!(
        selected_mission_report(&reports, Some(encounter.id)).map(|report| report.id),
        Some(7)
    );
}

#[test]
fn mission_planet_selector_marks_only_visible_affiliations() {
    let mut home = Planet::new(0, "Home".into(), Vec2::ZERO, false, 1.0);
    home.owned = Some(1);
    home.controlled = Some(1);
    let mut hidden = Planet::new(1, "Hidden".into(), Vec2::X, false, 1.0);
    hidden.owned = Some(2);
    hidden.controlled = Some(2);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![home, hidden],
    };
    let player = Player::new(1, 0);
    let session = MultiplayerSession::default();
    let context = egui::Context::default();
    let mut labels = None;

    let mut output = context.run_ui(Default::default(), |ui| {
        labels = Some((
            mission_planet_selector_label(ui, &map, &player, &[], &session, map.get(0), true),
            mission_planet_selector_label(ui, &map, &player, &[], &session, map.get(1), true),
        ));
    });
    output.textures_delta.clear();
    let (home, hidden) = labels.unwrap();

    assert_eq!(home.text, "Home");
    assert_eq!(home.sections.len(), 1);
    assert_eq!(home.sections[0].format.color, session.player_color(1).color().to_color32());
    assert_eq!(hidden.text, "Hidden");
    assert_eq!(hidden.sections.len(), 1);
}

#[test]
fn intercept_route_wraps_plain_fleet_names_in_owner_colors_and_keeps_the_destination_column() {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::player::PlayerColor;
    use crate::core::simulation::{GameModel, GameRules, MatchStatus, PersistedGame};
    use crate::multiplayer::model::GameRecord;

    let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = model.players[0].clone();
    let home = player.home_planet;
    let owner = model.players[1].id;
    model.players[1].color = PlayerColor::new(4).unwrap();
    model.map.get_mut(home).name = "A world with a deliberately long name".into();
    model.map.get_mut(home).army.insert(Unit::Building(Building::SensorPhalanx), 30);
    let target = Mission {
        id: 900,
        owner,
        origin: model.players[1].home_planet,
        destination: home,
        position: model.map.get(home).position + Vec2::X * Planet::SIZE,
        objective: Icon::Attack,
        army: Army::from([(Unit::war_sun(), 1)]),
        ..default()
    };
    assert!(interception::visible_contact(&target, &model.map, &player));
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: GameId::new("intercept-route"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: 2,
        status: MatchStatus::Active,
        persisted: PersistedGame::new(model.clone()),
        submitted_players: Vec::new(),
        members: vec![GameMembership {
            game_id: GameId::new("intercept-route"),
            player_id: owner,
            user_id: UserId::new("enemy"),
            display_name: String::new(),
            is_creator: false,
            identity_version: 0,
            connected: true,
        }],
    });
    let deep_space = egui::TextureId::User(1);
    let enemy_fleet = egui::TextureId::User(2);
    let images = ImageIds(HashMap::from([
        ("deep space".to_owned(), deep_space),
        ("mission destroy".to_owned(), enemy_fleet),
    ]));
    for width in [820.0, 650.0, 520.0, 320.0] {
        let mut image_column = None;
        for name in ["Mara", "Practice P2", "Aether Ray Swarm of the Distant Nebula"] {
            session.active_game.as_mut().unwrap().members[0].display_name = name.to_owned();
            let expected = format!("{name}'s fleet");
            let color = session.player_color(owner).color().to_color32();
            let mut mission = Mission {
                origin: home,
                destination: home,
                owner: player.id,
                objective: Icon::Intercept,
                ..default()
            };
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let expected_font = TextStyle::Button.resolve(&context.style_of(context.theme()));
            context.style_mut_of(context.theme(), |style| {
                style.interaction.tooltip_delay = f32::MAX;
            });
            let mut label_rect = egui::Rect::NOTHING;
            for frame in 0..8 {
                let mut events = Vec::new();
                if width >= 650.0 && matches!(frame, 2 | 3) {
                    events.push(egui::Event::PointerMoved(label_rect.center()));
                    events.push(egui::Event::PointerButton {
                        pos: label_rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed: frame == 2,
                        modifiers: default(),
                    });
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 300.0),
                        )),
                        time: Some(frame as f64 * 0.25),
                        events,
                        ..default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            let mut scroll = ScrollArea::horizontal();
                            if width < 650.0 && frame == 0 {
                                scroll = scroll.horizontal_scroll_offset(1000.0);
                            }
                            scroll.show(ui, |ui| {
                                let route = draw_mission_route(
                                    ui,
                                    &mut mission,
                                    &model.map,
                                    &player,
                                    std::slice::from_ref(&target),
                                    &session,
                                    &Army::new(),
                                    &Unit::ships(),
                                    &images,
                                    false,
                                    Some(target.id),
                                );
                                let rect = route[1].as_ref().unwrap().rect;
                                if frame >= 2 {
                                    if let Some(previous) = image_column {
                                        assert_eq!(
                                            (rect.x_range(), rect.top()),
                                            previous,
                                            "destination column moved for {name} at {width}"
                                        );
                                    }
                                    image_column = Some((rect.x_range(), rect.top()));
                                    if width >= 650.0 {
                                        assert!(ui.ctx().content_rect().contains_rect(rect));
                                    }
                                }
                            });
                        });
                    },
                );
                output.textures_delta.clear();
                let mut labels = 0;
                let mut saw_deep_space = false;
                for shape in &output.shapes {
                    match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == expected => {
                            labels += 1;
                            assert!(text.galley.job.sections.iter().all(|section| {
                                section.format.color == color
                                    && section.format.font_id == expected_font
                            }));
                            assert!(text.galley.size().x <= interception::TARGET_LABEL_WIDTH);
                            assert!(!text.galley.elided);
                            if labels == 1 {
                                label_rect = text.galley.rect.translate(text.pos.to_vec2());
                                if width >= 650.0 {
                                    assert!(shape.clip_rect.contains_rect(label_rect));
                                }
                            }
                        },
                        egui::Shape::Mesh(mesh) => {
                            assert_ne!(mesh.texture_id, enemy_fleet);
                            if mesh.texture_id == deep_space {
                                saw_deep_space = true;
                                assert!(mesh
                                    .vertices
                                    .iter()
                                    .all(|vertex| vertex.color == Color32::WHITE));
                            }
                        },
                        egui::Shape::Rect(rect) if rect.brush.is_some() => {
                            assert_ne!(rect.fill_texture_id(), enemy_fleet);
                            if rect.fill_texture_id() == deep_space {
                                saw_deep_space = true;
                                assert_eq!(rect.fill, Color32::WHITE);
                            }
                        },
                        _ => {},
                    }
                }
                if frame >= 2 {
                    assert!(saw_deep_space, "missing Deep space image at {width} on frame {frame}");
                }
                assert!(labels <= 1, "clicking the fleet label must not open a dropdown");
                if frame >= 2 && width >= 650.0 {
                    assert_eq!(output.platform_output.cursor_icon, CursorIcon::Default);
                }
                if width >= 650.0 && frame >= 1 {
                    assert!(
                        labels >= 1,
                        "missing {expected} at {width} on frame {frame}: {:?}",
                        output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) => Some(text.galley.text()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

#[test]
fn mission_report_route_names_players_without_disclosing_a_hidden_sender() {
    let mut destination = Planet::new(2, "Target".into(), Vec2::ZERO, false, 1.0);
    destination.owned = Some(2);
    destination.controlled = Some(2);
    let mut report = crate::test_support::empty_report(
        Mission {
            owner: 3,
            objective: Icon::Spy,
            ..default()
        },
        destination,
    );
    let session = MultiplayerSession::default();

    assert_eq!(mission_report_attacker_name(&report, 1, &session), "Unknown");
    assert_eq!(mission_report_attacker_color(&report, 1, &session), NEUTRAL_COMBAT_COLOR);
    assert_eq!(mission_report_fleet_color(&report, 1, &session), NEUTRAL_COMBAT_COLOR);
    assert_eq!(mission_report_attacker_name(&report, 3, &session), "Player 3");
    assert_eq!(
        mission_report_attacker_color(&report, 3, &session),
        session.player_color(3).color().to_color32()
    );
    assert_eq!(
        mission_report_fleet_color(&report, 3, &session),
        session.player_color(3).color().to_color32()
    );
    assert_eq!(mission_report_defender_name(&report, &session), "Player 2");
    assert_eq!(
        mission_report_defender_color(&report, &session),
        session.player_color(2).color().to_color32()
    );

    report.mission.objective = Icon::Attack;
    assert_eq!(mission_report_attacker_name(&report, 1, &session), "Player 3");
    assert_eq!(
        mission_report_fleet_color(&report, 1, &session),
        session.player_color(3).color().to_color32()
    );

    report.planet.owned = None;
    report.planet.controlled = None;
    assert_eq!(mission_report_defender_name(&report, &session), "Unclaimed");
    assert_eq!(mission_report_defender_color(&report, &session), NEUTRAL_COMBAT_COLOR);
}

#[test]
fn mission_report_route_is_centered_without_a_fixed_horizontal_offset() {
    assert_eq!(MISSION_REPORT_ROUTE_WIDTH, 540.0);
    assert_eq!(mission_report_route_leading_space(640.0), 50.0);
    assert_eq!(mission_report_route_leading_space(540.0), 0.0);
    assert_eq!(mission_report_route_leading_space(480.0), 0.0);

    let row = egui::Rect::from_min_size(
        egui::pos2(20.0, 30.0),
        egui::vec2(640.0, MISSION_PLANET_CELL_HEIGHT),
    );
    let route = mission_report_route_rects(row);
    let (origin_planet, origin_name) = mission_planet_rects(route.origin);
    let (destination_planet, destination_name) = mission_planet_rects(route.destination);

    assert_eq!(origin_planet.top(), destination_planet.top());
    assert_eq!(origin_planet.size(), destination_planet.size());
    assert_eq!(origin_name.top(), destination_name.top());
    assert_eq!(origin_name.size(), destination_name.size());
    assert_eq!(route.attacker.size(), route.defender.size());
    assert_eq!(route.attacker.top(), route.defender.top());
    assert_eq!(route.attacker.width(), MISSION_REPORT_ROUTE_PLAYER_WIDTH);
    assert_eq!(route.origin.center().x + route.destination.center().x, row.center().x * 2.0);
    assert_eq!(route.attacker.center().x + route.defender.center().x, row.center().x * 2.0);
    assert_eq!(route.center.center().x, row.center().x);
}

#[test]
fn report_player_names_show_full_name_tooltips_only_when_truncated() {
    for name in ["Mara", "Aether Ray Swarm of the Distant Nebula"] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        context.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let mut label_rect = egui::Rect::NOTHING;
        let truncated = name != "Mara";
        for step in 0..5 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(600.0, 250.0),
                    )),
                    time: Some(step as f64),
                    events: if step >= 2 {
                        vec![egui::Event::PointerMoved(label_rect.center())]
                    } else {
                        vec![]
                    },
                    ..default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        draw_mission_report_player(ui, name.to_owned(), Color32::WHITE);
                    });
                },
            );
            output.textures_delta.clear();
            let names = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == name => Some(text),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let label = names.iter().find(|text| text.galley.elided == truncated).unwrap();
            if step < 2 {
                label_rect = egui::Rect::from_min_size(label.pos, label.galley.size());
                assert_eq!(names.len(), 1, "a full-name tooltip must wait for hover");
            }
            assert_eq!(label.galley.rows.len(), 1);
            assert!(label.galley.size().x <= MISSION_REPORT_ROUTE_PLAYER_WIDTH);
            if step == 4 {
                assert_eq!(
                    names.len(),
                    if truncated {
                        2
                    } else {
                        1
                    }
                );
                assert_eq!(names.iter().filter(|text| !text.galley.elided).count(), 1);
            }
        }
    }
}

#[test]
fn space_report_images_and_caption_focus_the_recorded_encounter_at_each_viewport_scale() {
    use crate::core::units::fauna::SpaceFauna;

    for fauna in [false, true] {
        let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
        let destination = Planet::new(1, "Destination".into(), Vec2::X, false, 1.0);
        let mut encounter = destination.clone();
        let encounter_position = Vec2::new(375.0, 240.0);
        encounter.position = encounter_position;
        encounter.name = "Aether Ray Swarm".to_owned();
        if fauna {
            encounter.army.insert(Unit::Fauna(SpaceFauna::AetherRay), 5);
        }
        let mut report = crate::test_support::empty_report(
            Mission {
                owner: 1,
                origin: 0,
                destination: 1,
                position: Vec2::new(-100.0, 50.0),
                objective: if fauna {
                    Icon::Raid
                } else {
                    Icon::Patrol
                },
                army: Army::from([(Unit::Ship(Ship::Cruiser), 3)]),
                ..default()
            },
            encounter,
        );
        report.id = 42;
        report.combat_report = Some(crate::core::combat::report::CombatReport {
            rounds: vec![RoundReport::default()],
            ..default()
        });
        let mut player = Player::new(1, 0);
        player.reports.push(report);
        let map = Map {
            rect: Rect::default(),
            solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
            planets: vec![origin, destination],
        };
        let deep_space = egui::TextureId::User(901);
        let images = ImageIds(HashMap::from([("deep space".to_owned(), deep_space)]));
        for (viewport, target) in [egui::vec2(1280.0, 720.0), egui::vec2(800.0, 600.0)]
            .into_iter()
            .flat_map(|viewport| (0..4).map(move |target| (viewport, target)))
        {
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let scale = mission_panel_scale(viewport);
            let size = mission_panel_size(viewport / scale);
            let position = (viewport - size * scale) * 0.5;
            let mut state = UiState {
                mission: true,
                mission_tab: MissionTab::MissionReports,
                planet_selected: Some(0),
                planet_hover: Some(0),
                mission_planet_hover: Some(0),
                focus_planet: Some(1),
                focus_position: Some(Vec2::ZERO),
                focus_zoom: Some(2.0),
                combat_report: (target == 3).then_some(42),
                ..default()
            };
            let mut target_rect = egui::Rect::NOTHING;
            for step in 0..6 {
                let point = target_rect.lerp_inside(egui::vec2(0.35, 0.65));
                let mut events = Vec::new();
                if step >= 3 {
                    events.push(egui::Event::PointerMoved(point));
                    if step >= 4 {
                        events.push(egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: step == 4,
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
                                if target == 3 {
                                    draw_combat_report(
                                        ui,
                                        &mut state,
                                        &map,
                                        &player,
                                        &MultiplayerSession::default(),
                                        &images,
                                    );
                                } else {
                                    draw_mission_reports(
                                        ui,
                                        &mut state,
                                        &map,
                                        &player,
                                        &MultiplayerSession::default(),
                                        false,
                                        &images,
                                    );
                                }
                            },
                        );
                    },
                );
                output.textures_delta.clear();
                if step < 2 {
                    continue;
                }
                let images = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) if mesh.texture_id == deep_space => {
                            Some(mesh.calc_bounds())
                        },
                        egui::Shape::Rect(rect)
                            if rect.brush.is_some() && rect.fill_texture_id() == deep_space =>
                        {
                            Some(rect.rect)
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                if target == 3 {
                    assert_eq!(images.len(), 1, "combat details destination");
                    assert!((images[0].width() - 35.0 * scale).abs() < 1.0);
                    if step == 2 {
                        target_rect = images[0];
                    }
                } else {
                    assert_eq!(
                        images.len(),
                        2,
                        "list and detail images, fauna {fauna}, viewport {viewport:?}"
                    );
                    assert!((images[0].width() - 40.0 * scale).abs() < 1.0);
                    assert!((images[1].width() - MISSION_PLANET_IMAGE_SIZE * scale).abs() < 1.0);
                    let caption = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == "Deep space" => {
                                Some(text)
                            },
                            _ => None,
                        })
                        .unwrap();
                    assert!(!caption.galley.elided);
                    assert_eq!(caption.galley.rows.len(), 1);
                    assert!(images[0].right() < images[1].left());
                    if step == 2 {
                        target_rect = match target {
                            0 | 1 => images[target],
                            _ => egui::Rect::from_min_size(caption.pos, caption.galley.size()),
                        };
                    }
                }
                if step == 3 {
                    assert_eq!(output.platform_output.cursor_icon, CursorIcon::PointingHand);
                }
                if step < 5 {
                    assert!(state.mission, "hovering or pressing must not navigate yet");
                    assert_eq!(state.focus_position, Some(Vec2::ZERO));
                    assert!(state.location_marker.is_none());
                }
            }
            assert_eq!(state.focus_position, Some(encounter_position));
            assert_eq!(state.location_marker.as_ref().unwrap().position, encounter_position);
            assert_eq!(state.combat_report, None);
            assert!(state.to_selected);
            assert!(!state.mission);
            assert_eq!(state.planet_selected, None);
            assert_eq!(state.focus_planet, None);
            assert_eq!(state.focus_zoom, None);
            assert_eq!(state.planet_hover, None);
            assert_eq!(state.mission_planet_hover, None);
        }
    }
}

#[test]
fn planet_links_cancel_an_earlier_coordinate_focus() {
    let context = egui::Context::default();
    let map = Map::new(1, 0);
    let player = Player::new(1, 0);
    let mut state = UiState {
        mission: true,
        focus_planet: Some(1),
        focus_position: Some(Vec2::new(300.0, 200.0)),
        focus_zoom: Some(2.0),
        ..default()
    };
    let mut image_rect = egui::Rect::NOTHING;
    for step in 0..4 {
        let events = if step >= 2 {
            vec![
                egui::Event::PointerMoved(image_rect.center()),
                egui::Event::PointerButton {
                    pos: image_rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed: step == 2,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        } else {
            vec![]
        };
        let mut output = context.run_ui(
            egui::RawInput {
                events,
                ..default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let (image, name) = draw_mission_planet_link(
                        ui,
                        egui::TextureId::User(1),
                        "Origin",
                        Sense::click(),
                    );
                    image_rect = image.rect;
                    handle_mission_planet_link(
                        &image,
                        &name,
                        map.get(0),
                        &mut false,
                        &mut state,
                        &map,
                        &player,
                    );
                });
            },
        );
        output.textures_delta.clear();
    }
    assert_eq!(state.planet_selected, Some(0));
    assert!(state.to_selected);
    assert!(!state.mission);
    assert_eq!(state.focus_position, None);
    assert_eq!(state.focus_planet, None);
    assert_eq!(state.focus_zoom, None);
}

#[test]
fn mission_objectives_require_their_origin_units_before_selection() {
    let destination = Planet::new(2, "Target".into(), Vec2::ZERO, false, 1.0);
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut army = Army::from([(fighter, 1)]);

    assert!(mission_objective_available(Icon::Attack, &army, 0, &destination, 1, 2));
    for objective in [Icon::Colonize, Icon::Spy, Icon::MissileStrike, Icon::Destroy] {
        assert!(
            !mission_objective_available(objective, &army, 0, &destination, 1, 2),
            "{objective:?} should be disabled without its required unit"
        );
    }

    army.insert(Unit::colony_ship(), 1);
    army.insert(Unit::probe(), crate::core::constants::MIN_SPY_PROBES);
    army.insert(Unit::interplanetary_missile(), 1);
    army.insert(Unit::war_sun(), 1);

    for objective in [Icon::Colonize, Icon::Spy, Icon::MissileStrike, Icon::Destroy] {
        assert!(
            mission_objective_available(objective, &army, 0, &destination, 1, 2),
            "{objective:?} should be enabled once its required unit is available"
        );
    }
}

#[test]
fn raid_moon_objective_is_disabled_while_normal_attacks_remain_available() {
    let moon = Planet::new(2, "Moon".into(), Vec2::ZERO, true, 1.0);
    let planet = Planet::new(3, "Planet".into(), Vec2::ZERO, false, 1.0);
    let army = Army::from([(Unit::Ship(Ship::LightFighter), 1)]);
    assert!(!mission_objective_available(Icon::Raid, &army, 0, &moon, 1, 2));
    assert!(mission_objective_available(Icon::Raid, &army, 0, &planet, 1, 2));
    assert!(mission_objective_available(Icon::Attack, &army, 0, &moon, 1, 2));
    assert_eq!(
        mission_objective_unavailable_reason(Icon::Raid, &army, 0, &moon, 1, 2),
        Some("Moons cannot be raided.")
    );
}

#[test]
fn objective_disabled_reasons_identify_the_actual_blocker() {
    let moon = Planet::new(2, "Moon".into(), Vec2::ZERO, true, 1.0);
    let planet = Planet::new(3, "Planet".into(), Vec2::ZERO, false, 1.0);
    let empty = Army::new();
    for (objective, required_unit, missing_units, moon_restriction) in [
        (
            Icon::Raid,
            Unit::Ship(Ship::LightFighter),
            "No combat ships on the origin planet.",
            "Moons cannot be raided.",
        ),
        (
            Icon::Colonize,
            Unit::colony_ship(),
            "No Colony Ship on the origin planet.",
            "Moons cannot be colonized.",
        ),
        (
            Icon::MissileStrike,
            Unit::interplanetary_missile(),
            "No Interplanetary Missiles on the origin planet.",
            "Interplanetary Missiles cannot target moons.",
        ),
    ] {
        let equipped = Army::from([(required_unit, 1)]);
        assert_eq!(
            mission_objective_unavailable_reason(objective, &empty, 0, &planet, 1, 2),
            Some(missing_units)
        );
        assert_eq!(
            mission_objective_unavailable_reason(objective, &equipped, 0, &planet, 1, 2),
            None
        );
        for army in [&empty, &equipped] {
            assert_eq!(
                mission_objective_unavailable_reason(objective, army, 0, &moon, 1, 2),
                Some(moon_restriction)
            );
        }
    }
    assert_eq!(
        mission_objective_unavailable_reason(Icon::Colonize, &empty, 0, &planet, 2, 2),
        Some("Maximum number of colonized planets reached.")
    );
    assert_eq!(
        mission_objective_unavailable_reason(Icon::Deploy, &empty, planet.id, &planet, 1, 2),
        Some("No planets to deploy to.")
    );
    assert_eq!(
        mission_objective_unavailable_reason(Icon::Deploy, &empty, 0, &planet, 1, 2),
        Some("No ships on the origin planet.")
    );
}

#[test]
fn allied_drafts_allow_only_shared_objectives() {
    for objective in [Icon::Colonize, Icon::Attack, Icon::Destroy] {
        assert!(mission_objective_selectable(objective, true, true));
    }
    for objective in [Icon::Protect, Icon::Deploy, Icon::Raid, Icon::Spy, Icon::MissileStrike] {
        assert!(!mission_objective_selectable(objective, true, true));
    }
    assert!(mission_objective_selectable(Icon::Protect, true, false));
    assert!(mission_objective_selectable(Icon::Raid, true, false));
    assert!(!mission_objective_selectable(Icon::Attack, false, true));
}

#[test]
fn destroy_draft_shows_first_volley_chance_for_selected_war_suns() {
    use crate::core::simulation::{GameModel, GameRules};

    let mut model = GameModel::new([91; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = model.players[0].clone();
    let origin = player.home_planet;
    let destination = model.players[1].home_planet;
    let context = egui::Context::default();

    for (diameter, suns, per_sun, combined) in [
        (1_500, 0, "12.0%", "0.0%"),
        (1_500, 1, "12.0%", "12.0%"),
        (1_500, 3, "12.0%", "31.9%"),
        (120_000, 3, "8.0%", "22.1%"),
    ] {
        model.map.get_mut(destination).diameter = diameter;
        let mut mission = Mission {
            origin,
            destination,
            objective: Icon::Destroy,
            army: Army::from([(Unit::war_sun(), suns)]),
            ..default()
        };
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(840.0, 800.0),
                )),
                ..default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    draw_mission_details(
                        ui,
                        &mut mission,
                        &model.map,
                        &player,
                        1,
                        &ImageIds::default(),
                        None,
                    );
                });
            },
        );
        output.textures_delta.clear();
        let text = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect::<Vec<_>>();
        assert!(text.windows(2).any(|rows| {
            rows[0].starts_with("⛽ Fuel consumption:")
                && rows[1] == format!("💥 Chance of destruction: {per_sun} / {combined}")
        }));
    }
}

#[test]
fn spy_missions_deep_cover_toggle_and_fuel_require_a_completed_origin_relay() {
    use crate::core::simulation::{GameModel, GameRules};

    for width in [840.0, 1_200.0] {
        for has_relay in [false, true] {
            let mut model = GameModel::new([75; 32], GameRules::default()).unwrap();
            model.start().unwrap();
            let mut player = model.players[0].clone();
            player.resources.deuterium = 100_000;
            let origin = player.home_planet;
            let destination = model.players[1].home_planet;
            model.map.get_mut(origin).army.insert(Unit::probe(), 5);
            let relay = Unit::Building(Building::CommandRelay);
            if has_relay {
                model.map.get_mut(origin).army.insert(relay, 1);
            } else {
                // A queued relay must not enable the option or preserve a stale selection.
                model.map.get_mut(origin).buy.push(relay);
            }
            // A stronger hidden target must not prevent selecting the attempt.
            model.map.get_mut(destination).army.insert(relay, 5);
            let mut state = UiState {
                mission_info: Mission {
                    origin,
                    destination,
                    objective: Icon::Spy,
                    army: Army::from([(Unit::probe(), 5)]),
                    deep_cover: !has_relay,
                    ..default()
                },
                ..default()
            };
            let context = egui::Context::default();
            let mut world = World::new();
            world.init_resource::<Messages<SendMissionMsg>>();
            world.init_resource::<Messages<MultiplayerRequest>>();
            let mut params = bevy::ecs::system::SystemState::<(
                MessageWriter<SendMissionMsg>,
                MessageWriter<MultiplayerRequest>,
            )>::new(&mut world);
            let mut option = egui::Rect::NOTHING;
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 800.0));
            let mut final_text = Vec::new();
            for step in 0..5 {
                let mut events = Vec::new();
                if step >= 2 {
                    assert!(option.is_positive());
                    let point = egui::pos2(option.right() + 20.0, option.center().y);
                    events.push(egui::Event::PointerMoved(point));
                    if step <= 3 {
                        events.push(egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: step == 2,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
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
                                &Settings::default(),
                                &mut state,
                                &mut model.map,
                                &mut player,
                                &MultiplayerSession::default(),
                                &mut requests,
                                true,
                                &ButtonInput::default(),
                                &ImageIds::default(),
                            );
                        });
                    },
                );
                output.textures_delta.clear();
                final_text.clear();
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        final_text.push(text.galley.text().to_owned());
                        if text.galley.text() == "Deep Cover:" {
                            option = egui::Rect::from_min_size(text.pos, text.galley.size());
                            assert!(
                                screen.contains_rect(option),
                                "cover option must fit at {width}"
                            );
                        }
                    }
                }
            }
            assert_eq!(state.mission_info.deep_cover, has_relay);
            let fuel = state.mission_info.fuel_consumption(&model.map);
            let fuel_row = final_text
                .iter()
                .position(|text| text == &format!("⛽ Fuel consumption: {fuel}"))
                .expect("spy mission should show its fuel consumption");
            let cover_row = final_text
                .iter()
                .position(|text| text == "Deep Cover:")
                .expect("spy mission should show the Deep Cover option");
            assert!(fuel_row < cover_row, "Deep Cover should follow fuel consumption");
            state.mission_info.objective = Icon::Attack;
            let mut output = context.run_ui(egui::RawInput::default(), |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    draw_deep_cover_option(ui, &mut state.mission_info, model.map.get(origin));
                });
            });
            output.textures_delta.clear();
            assert!(!state.mission_info.deep_cover);
        }
    }
}

#[test]
fn deep_cover_label_and_switch_share_click_and_hover_behavior() {
    for click_label in [true, false] {
        let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
        origin.army.insert(Unit::Building(Building::CommandRelay), 1);
        for check_hover in [false, true] {
            let context = egui::Context::default();
            if check_hover {
                context.all_styles_mut(|style| {
                    style.interaction.tooltip_delay = 0.0;
                    style.interaction.show_tooltips_only_when_still = false;
                });
            }
            let mut mission = Mission {
                objective: Icon::Spy,
                ..default()
            };
            let mut label_rect = egui::Rect::NOTHING;
            let mut hovered_tooltip = false;
            let steps = if check_hover {
                5
            } else {
                3
            };
            for step in 0..steps {
                let mut events = Vec::new();
                if step > 0 {
                    let point = if click_label {
                        label_rect.center()
                    } else {
                        egui::pos2(label_rect.right() + 20.0, label_rect.center().y)
                    };
                    events.push(egui::Event::PointerMoved(point));
                    if !check_hover {
                        events.push(egui::Event::PointerButton {
                            pos: point,
                            button: egui::PointerButton::Primary,
                            pressed: step == 1,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(840.0, 800.0),
                        )),
                        time: Some(if check_hover {
                            step as f64
                        } else {
                            step as f64 * 0.1
                        }),
                        events,
                        ..default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            draw_deep_cover_option(ui, &mut mission, &origin);
                        });
                    },
                );
                output.textures_delta.clear();
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        if text.galley.text() == "Deep Cover:" {
                            label_rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                        }
                        if text.galley.text().contains("extra deuterium per Probe") {
                            hovered_tooltip = true;
                        }
                    }
                }
                if step == 1 {
                    assert_eq!(output.platform_output.cursor_icon, egui::CursorIcon::PointingHand);
                }
            }
            assert!(label_rect.is_positive());
            if check_hover {
                assert!(
                    hovered_tooltip,
                    "the {} should show the Deep Cover tooltip",
                    if click_label {
                        "label"
                    } else {
                        "switch"
                    }
                );
            } else {
                assert!(
                    mission.deep_cover,
                    "clicking the {} should enable Deep Cover",
                    if click_label {
                        "label"
                    } else {
                        "switch"
                    }
                );
            }
        }
    }
}

#[test]
fn protect_jump_draft_stays_selected_when_the_origin_has_no_ships() {
    use crate::core::simulation::{GameModel, GameRules};

    let mut model = GameModel::new([76; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let mut player = model.players[0].clone();
    let origin = player.home_planet;
    let destination = model.players[1].home_planet;
    let gate = Unit::Building(Building::JumpGate);
    model.map.get_mut(origin).army.retain(|unit, _| !unit.is_ship());
    model.map.get_mut(origin).army.insert(Unit::interplanetary_missile(), 1);
    model.map.get_mut(origin).army.insert(gate, 1);
    model.map.get_mut(destination).army.insert(gate, 1);
    model.map.get_mut(destination).protection_permissions.insert(player.id);
    let mut world = World::new();
    world.init_resource::<Messages<SendMissionMsg>>();
    world.init_resource::<Messages<MultiplayerRequest>>();
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<SendMissionMsg>,
        MessageWriter<MultiplayerRequest>,
    )>::new(&mut world);

    for width in [840.0, 1_200.0] {
        let context = egui::Context::default();
        let mut state = UiState {
            mission_info: Mission {
                origin,
                destination,
                objective: Icon::Protect,
                protected_player: model.map.get(destination).controlled,
                jump_gate: true,
                ..default()
            },
            jump_gate_history: true,
            ..default()
        };
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 800.0),
                )),
                ..default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let (mut sends, mut requests) = params.get_mut(&mut world).unwrap();
                    draw_new_mission(
                        ui,
                        &mut sends,
                        &[],
                        &Settings::default(),
                        &mut state,
                        &mut model.map,
                        &mut player,
                        &MultiplayerSession::default(),
                        &mut requests,
                        true,
                        &ButtonInput::default(),
                        &ImageIds::default(),
                    );
                });
            },
        );
        output.textures_delta.clear();
        assert_eq!(state.mission_info.origin, origin);
        assert_eq!(state.mission_info.destination, destination);
        assert_eq!(state.mission_info.objective, Icon::Protect);
        assert!(state.mission_info.jump_gate);
        assert!(!state.mission_info.army.has_army());
    }
    assert!(world.resource::<Messages<SendMissionMsg>>().is_empty());
}

#[test]
fn new_mission_origin_picker_selects_and_keeps_stationed_protection() {
    use crate::core::simulation::{GameModel, GameRules};

    let mut model = GameModel::new(
        [75; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let mut player = model.players[0].clone();
    let home = player.home_planet;
    let protected = model.players[1].home_planet;
    let target = model.players[2].home_planet;
    let fighter = Unit::Ship(Ship::LightFighter);
    model.map.get_mut(home).name = "Home port".into();
    model.map.get_mut(home).army.insert(fighter, 20);
    model.map.get_mut(protected).name = "Protected port".into();
    model.map.get_mut(protected).army.insert(fighter, 17);
    model.map.get_mut(protected).protection_permissions.insert(player.id);
    model.map.get_mut(protected).dock_protecting_fleet(player.id, Army::from([(fighter, 3)]));

    for width in [840.0, 1_200.0] {
        let context = egui::Context::default();
        let mut state = UiState::default();
        state.mission_info.origin = home;
        state.mission_info.destination = target;
        state.mission_info.objective = Icon::Attack;
        state.mission_info.army = Army::from([(fighter, 20)]);
        let mut world = World::new();
        world.init_resource::<Messages<SendMissionMsg>>();
        world.init_resource::<Messages<MultiplayerRequest>>();
        let mut params = bevy::ecs::system::SystemState::<(
            MessageWriter<SendMissionMsg>,
            MessageWriter<MultiplayerRequest>,
        )>::new(&mut world);
        let mut home_rect = egui::Rect::NOTHING;
        let mut protected_rect = egui::Rect::NOTHING;

        for step in 0..10 {
            let mut events = Vec::new();
            if step >= 3 {
                let rect = if step <= 5 {
                    home_rect
                } else {
                    protected_rect
                };
                assert!(rect.is_positive(), "the origin option must be visible at step {step}");
                events.push(egui::Event::PointerMoved(rect.center()));
                if matches!(step, 3 | 4 | 6 | 7) {
                    events.push(egui::Event::PointerButton {
                        pos: rect.center(),
                        button: egui::PointerButton::Primary,
                        pressed: matches!(step, 3 | 6),
                        modifiers: egui::Modifiers::NONE,
                    });
                }
            }
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 800.0),
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
                            &Settings::default(),
                            &mut state,
                            &mut model.map,
                            &mut player,
                            &MultiplayerSession::default(),
                            &mut requests,
                            true,
                            &ButtonInput::default(),
                            &ImageIds::default(),
                        );
                    });
                },
            );
            output.textures_delta.clear();
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    if text.galley.text().ends_with("Home port") {
                        home_rect = rect;
                    } else if text.galley.text().ends_with("Protected port") {
                        protected_rect = rect;
                    }
                }
            }
        }
        assert_eq!(state.mission_info.origin, protected);
        assert_eq!(state.mission_info.army.amount(&fighter), 3);
        assert!(validate_mission(
            &player,
            &model.map,
            model.map.get(protected),
            model.map.get(target),
            &state.mission_info
        )
        .is_ok());
    }
}

#[test]
fn incoming_protection_reveals_its_objective_only_to_the_protected_player() {
    let mission = Mission {
        owner: 2,
        objective: Icon::Protect,
        protected_player: Some(1),
        ..default()
    };

    assert_eq!(mission.displayed_objective(1), Icon::Protect);
    assert_eq!(mission.displayed_objective(2), Icon::Protect);
    assert_eq!(mission.displayed_objective(3), Icon::EnemyFleet);
    assert!(mission.is_incoming_protection_for(1));
    assert!(!mission.is_incoming_protection_for(2));
    assert!(!mission.is_incoming_protection_for(3));
}

#[test]
fn joint_attack_participants_see_the_actual_mission_objective() {
    let mut mission = Mission {
        owner: 1,
        objective: Icon::Destroy,
        joint_attack: Some(JointAttackMission {
            leader: 1,
            attackers: std::collections::BTreeMap::from([
                (1, Army::from([(Unit::war_sun(), 1)])),
                (2, Army::from([(Unit::Ship(Ship::LightFighter), 2)])),
            ]),
            ..default()
        }),
        ..default()
    };

    for objective in [Icon::Colonize, Icon::Destroy] {
        mission.objective = objective;
        assert_eq!(mission.displayed_objective(1), objective);
        assert_eq!(mission.displayed_objective(2), objective);
        assert_eq!(mission.displayed_objective(3), Icon::EnemyFleet);
    }
}

#[test]
fn joint_attack_report_icon_uses_the_viewers_fleet_and_color() {
    let war_sun = Unit::war_sun();
    let fighter = Unit::Ship(crate::core::units::ships::Ship::LightFighter);
    let report = crate::test_support::empty_report(
        Mission {
            owner: 1,
            army: Army::from([(war_sun, 1), (fighter, 1)]),
            joint_attack: Some(JointAttackMission {
                attackers: std::collections::BTreeMap::from([
                    (1, Army::from([(war_sun, 1)])),
                    (2, Army::from([(fighter, 1)])),
                ]),
                ..default()
            }),
            ..default()
        },
        Planet::new(0, "Target".into(), Vec2::ZERO, false, 1.0),
    );
    let leader = Player::new(1, 0);
    let participant = Player::new(2, 0);
    let observer = Player::new(3, 0);

    assert_eq!(report.mission.image(&leader), "mission destroy");
    assert_eq!(report.mission.image(&participant), "mission");
    assert_eq!(report.mission.image(&observer), "mission destroy");
    assert_eq!(mission_report_fleet_owner(&report, participant.id), participant.id);
    assert_eq!(mission_report_fleet_owner(&report, observer.id), leader.id);
}

#[test]
fn joint_attack_fleet_strength_uses_each_ships_production_level() {
    let army = Army::from([
        (Unit::probe(), 2),
        (Unit::colony_ship(), 3),
        (Unit::war_sun(), 4),
        (Unit::space_dock(), 9),
    ]);

    assert_eq!(fleet_strength(&army), 28);
}

#[test]
fn mission_strength_badge_shows_only_icon_and_selected_total() {
    let selected = Army::from([(Unit::probe(), 2), (Unit::colony_ship(), 3)]);
    let images = ImageIds([("fleet".to_string(), egui::TextureId::User(92))].into());
    let context = egui::Context::default();
    let mut output = context.run_ui(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            let rect = ui.available_rect_before_wrap();
            draw_mission_strength_badge(ui, &selected, &images, rect.center().y, rect.right());
        });
    });
    output.textures_delta.clear();
    let text = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(!text.iter().any(|text| text.contains("Fleet strength")));
    assert!(text.iter().any(|text| text == &selected.total_production().to_string()));
    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Mesh(mesh) if mesh.texture_id == egui::TextureId::User(92))));
}

#[test]
fn mission_plan_badge_orders_all_icons_and_reserves_its_percentage_before_strength() {
    use crate::core::combat::plan::CombatTactic;

    let fleet_texture = egui::TextureId::User(92);
    let tactic_texture = egui::TextureId::User(93);
    let priority_texture = egui::TextureId::User(94);
    let protection_texture = egui::TextureId::User(95);
    let images = ImageIds(
        [
            ("fleet".to_owned(), fleet_texture),
            ("target standard".to_owned(), priority_texture),
            ("protection standard".to_owned(), protection_texture),
        ]
        .into_iter()
        .chain(
            CombatTactic::ALL
                .into_iter()
                .map(|tactic| (tactic.image_key().to_owned(), tactic_texture)),
        )
        .collect(),
    );
    // These are logical footer widths; the mission panel scales down for smaller viewports.
    for width in [360.0, 560.0] {
        for amount in [0, 1, 1_000_000] {
            for tactic in [CombatTactic::Swarm, CombatTactic::Siege] {
                let context = egui::Context::default();
                context.set_global_style(NordDark.custom_style());
                let army = Army::from([(Unit::Ship(Ship::LightFighter), amount)]);
                let mut plan = BattlePlan {
                    automatic_tactic: false,
                    tactic,
                    ..default()
                };
                let label = format!("{}%", plan.suitability(&army));
                let mut measured_width = 0.0;
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 120.0),
                        )),
                        ..default()
                    },
                    |ui| {
                        measured_width = mission_badges_width(ui, &army, Some(plan));
                        let fleet_icon =
                            draw_mission_strength_badge(ui, &army, &images, 50.0, width - 12.0);
                        draw_mission_plan_badge(ui, &images, fleet_icon, &mut plan, &army);
                    },
                );
                output.textures_delta.clear();
                let image_bounds = |texture| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                                Some(mesh.calc_bounds())
                            },
                            _ => None,
                        })
                        .unwrap()
                };
                let text_shape = |label: &str| {
                    output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                            _ => None,
                        })
                        .unwrap()
                };
                let icon = image_bounds(tactic_texture);
                let priority = image_bounds(priority_texture);
                let protection = image_bounds(protection_texture);
                let fleet = image_bounds(fleet_texture);
                let percentage = text_shape(&label);
                let percentage_rect =
                    egui::Rect::from_min_size(percentage.pos, percentage.galley.size());
                let strength = text_shape(&army.total_production().to_string());
                assert_eq!(icon.size(), fleet.size());
                assert_eq!(priority.size(), icon.size());
                assert_eq!(protection.size(), icon.size());
                assert!(priority.right() < protection.left());
                assert!(protection.right() < icon.left());
                assert!(icon.width() > 22.0, "both icons should be larger than the old badge");
                assert!(priority.left() >= 0.0, "{amount} ships at width {width}");
                assert!(percentage_rect.left() >= icon.right());
                assert!(percentage_rect.right() + 24.0 <= fleet.left() + 0.5);
                assert!((measured_width - (width - 12.0 - priority.left())).abs() < 0.5);
                assert!((percentage_rect.center().y - fleet.center().y).abs() < 0.5);
                assert_eq!(
                    percentage.galley.job.sections[0].format.font_id.size,
                    strength.galley.job.sections[0].format.font_id.size,
                );
            }
        }
    }
}

#[test]
fn mission_timing_copy_explains_acceleration_and_the_arrival_turn() {
    let movement_tooltip = mission_movement_tooltip(false);
    assert!(movement_tooltip.contains("not a fixed per-turn speed"));
    assert!(movement_tooltip.contains("accelerate each travel turn"));
    assert!(mission_movement_tooltip(true).contains("Jump Gate"));
    assert_eq!(mission_arrival_turn(1, 6), 7);
    assert_eq!(mission_arrival_tooltip(1, 6), "The fleet will arrive at turn 7.");
}

#[test]
fn planet_report_layout_contains_every_unit_exactly_once() {
    for is_home_planet in [false, true] {
        let (critical, orbitals, buildings) = mission_report_planet_intel(is_home_planet);

        assert_eq!(critical, [Unit::planetary_shield(), Unit::space_dock()]);
        assert_eq!(orbitals.len(), 7);
        assert!(!orbitals.contains(&Unit::space_dock()));
        assert_eq!(buildings.len(), 9);
        assert!(!buildings.contains(&Unit::planetary_shield()));

        let presented = Unit::ships()
            .into_iter()
            .chain(Unit::defenses())
            .chain(critical)
            .chain(orbitals)
            .chain(buildings)
            .collect::<Vec<_>>();
        let expected =
            Unit::all_for_world(false, is_home_planet).into_iter().flatten().collect::<Vec<_>>();
        let unique = presented.iter().copied().collect::<std::collections::HashSet<_>>();

        assert_eq!(presented.len(), expected.len());
        assert_eq!(unique.len(), presented.len());
        assert_eq!(unique, expected.into_iter().collect());
    }
}

#[test]
fn planet_report_uses_the_recorded_administration_when_it_is_visible() {
    let mut home = Planet::new(9, "Home".into(), Vec2::ZERO, false, 1.0);
    home.controlled = Some(1);
    home.army.insert(Unit::Building(Building::Senate), 3);
    let mut report = MissionReport {
        id: 1,
        turn: 1,
        surviving_attacker: Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
        surviving_defender: home.army.clone(),
        destination_controlled: home.controlled,
        ..crate::test_support::empty_report(
            Mission {
                owner: 2,
                objective: Icon::Protect,
                destination: home.id,
                protected_player: Some(1),
                army: Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
                ..default()
            },
            home.clone(),
        )
    };

    assert_eq!(mission_report_administration(&report, 2), Unit::Building(Building::Senate));

    report.planet.army.remove(&Unit::Building(Building::Senate));
    report.planet.army.insert(Unit::Building(Building::ColonialAdministration), 2);
    assert_eq!(
        mission_report_administration(&report, 2),
        Unit::Building(Building::ColonialAdministration)
    );
}

#[test]
fn planet_report_reveals_a_senate_only_with_sufficient_intelligence() {
    let mut home = Planet::new(9, "Unknown home".into(), Vec2::ZERO, false, 1.0);
    home.controlled = Some(1);
    home.army.insert(Unit::Building(Building::Senate), 3);
    let report = MissionReport {
        id: 1,
        turn: 1,
        surviving_defender: home.army.clone(),
        destination_controlled: home.controlled,
        ..crate::test_support::empty_report(
            Mission {
                owner: 2,
                objective: Icon::Attack,
                destination: home.id,
                ..default()
            },
            home.clone(),
        )
    };

    assert!(!mission_report_unit_is_visible(
        &report,
        2,
        &Side::Defender,
        &Unit::Building(Building::Senate)
    ));
    assert_eq!(
        mission_report_administration(&report, 2),
        Unit::Building(Building::ColonialAdministration)
    );

    let mut informed_report = report;
    informed_report.scout_probes = usize::MAX;
    assert!(mission_report_unit_is_visible(
        &informed_report,
        2,
        &Side::Defender,
        &Unit::Building(Building::Senate)
    ));
    assert_eq!(
        mission_report_administration(&informed_report, 2),
        Unit::Building(Building::Senate)
    );
}

#[test]
fn planet_report_compact_intel_fits_the_existing_column() {
    let context = egui::Context::default();
    let (critical, orbitals, buildings) = mission_report_planet_intel(false);
    let units = critical.into_iter().chain(orbitals).chain(buildings).collect::<Vec<_>>();
    let images = ImageIds(
        units
            .iter()
            .map(|unit| (unit.to_lowername().to_string(), egui::TextureId::User(1)))
            .collect(),
    );
    let mut planet = Planet::new(1, "Intel".into(), Vec2::ZERO, false, 1.0);
    planet.controlled = Some(7);
    planet.army = units.iter().copied().map(|unit| (unit, 1)).collect();
    let report = MissionReport {
        id: 1,
        turn: 1,
        surviving_defender: planet.army.clone(),
        destination_controlled: planet.controlled,
        ..crate::test_support::empty_report(
            Mission {
                owner: 8,
                destination: planet.id,
                ..default()
            },
            planet.clone(),
        )
    };
    let mut rect = egui::Rect::NOTHING;

    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(200.0, 500.0),
            )),
            ..default()
        },
        |ui| {
            rect = ui
                .scope(|ui| {
                    draw_mission_report_planet_intel(ui, &report, &Player::new(7, 0), &images);
                })
                .response
                .rect;
        },
    );
    output.textures_delta.clear();

    assert!(
        (rect.width() - MISSION_REPORT_PLANET_INTEL_WIDTH).abs() <= 0.05,
        "compact intel was {} px wide",
        rect.width()
    );
    assert!(rect.height() <= 357.0, "compact intel was {} px tall", rect.height());
}

#[test]
fn planet_report_compact_columns_align_with_space_dock() {
    let compact_width = MISSION_REPORT_INTEL_IMAGE_SIZE * MISSION_REPORT_INTEL_COLUMNS as f32
        + MISSION_REPORT_INTEL_COLUMN_GAP * (MISSION_REPORT_INTEL_COLUMNS - 1) as f32;

    assert_eq!(compact_width, MISSION_REPORT_PLANET_INTEL_WIDTH);
}

#[test]
fn active_mission_destination_stays_in_the_third_grid_column() {
    for width in [700.0, 850.0, 1_200.0] {
        let context = egui::Context::default();
        for _ in 0..3 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 400.0),
                    )),
                    ..default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        let panel = ui.available_rect_before_wrap();
                        let (route_width, leading) = mission_row_layout(panel.width());
                        ui.horizontal(|ui| {
                            ui.add_space(leading);
                            egui::Grid::new("mission row regression")
                                .spacing([MISSION_COLUMN_GAP, 0.0])
                                .show(ui, |ui| {
                                    for _ in 0..2 {
                                        let (origin, _) = draw_mission_planet_link(
                                            ui,
                                            egui::TextureId::User(1),
                                            "Origin",
                                            Sense::click(),
                                        );
                                        draw_mission_log_badge(
                                            ui,
                                            egui::TextureId::User(2),
                                            origin.rect,
                                        );
                                        let (mut route, _) = mission_route_cell(ui, route_width);
                                        route.horizontal_centered(|ui| {
                                            ui.allocate_exact_size(
                                                egui::vec2(
                                                    route_width,
                                                    MISSION_ROUTE_PREVIEW_HEIGHT,
                                                ),
                                                Sense::hover(),
                                            );
                                        });
                                        let (destination, name) = draw_mission_planet_link(
                                            ui,
                                            egui::TextureId::User(1),
                                            "Destination",
                                            Sense::click(),
                                        );
                                        let expected_distance = MISSION_PLANET_COLUMN_WIDTH
                                            + 2.0 * MISSION_COLUMN_GAP
                                            + route_width;
                                        // The invisible first pass measures columns using placeholder widths.
                                        if ui.is_sizing_pass() {
                                            ui.end_row();
                                            continue;
                                        }
                                        assert!(
                                            (destination.rect.center().x
                                                - origin.rect.center().x
                                                - expected_distance)
                                                .abs()
                                                < 0.5,
                                            "width {width}: origin {:?}, destination {:?}, expected distance {expected_distance}",
                                            origin.rect, destination.rect
                                        );
                                        assert!(
                                            (destination.rect.top() - origin.rect.top()).abs()
                                                < 0.5
                                        );
                                        assert!(panel.contains_rect(destination.rect));
                                        assert!(panel.contains_rect(name.rect));
                                        ui.end_row();
                                    }
                                });
                        });
                    });
                },
            );
            output.textures_delta.clear();
        }
    }
}

#[test]
fn active_mission_arrow_lanes_align_across_affinities_and_expand_for_hidden_plans() {
    use crate::core::combat::plan::CombatTactic;

    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.owned = Some(1);
    origin.controlled = Some(1);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, Planet::new(1, "Destination".into(), Vec2::X * 500.0, false, 1.0)],
    };
    let player = Player::new(1, 0);
    let missions =
        [Some(CombatTactic::Siege), Some(CombatTactic::Skirmish), Some(CombatTactic::Swarm), None]
            .into_iter()
            .enumerate()
            .map(|(index, tactic)| Mission {
                id: index as u64,
                owner: if tactic.is_some() {
                    1
                } else {
                    2
                },
                origin: 0,
                destination: 1,
                objective: Icon::Attack,
                army: Army::from([(Unit::Ship(Ship::LightFighter), 10)]),
                battle_plan: BattlePlan {
                    automatic_tactic: false,
                    tactic: tactic.unwrap_or_default(),
                    ..default()
                },
                ..default()
            })
            .collect::<Vec<_>>();
    let objective_texture = egui::TextureId::User(904);
    let images = ImageIds(
        missions
            .iter()
            .map(|mission| (mission.displayed_objective(player.id).asset_key(), objective_texture))
            .collect(),
    );

    for width in [700.0, 850.0, 1_200.0] {
        for editable in [false, true] {
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let mut state = UiState::default();
            let mut world = World::new();
            world.init_resource::<Messages<RecallMissionMsg>>();
            let mut params =
                bevy::ecs::system::SystemState::<MessageWriter<RecallMissionMsg>>::new(&mut world);

            // Settle the grid before comparing the painted lanes.
            for step in 0..3 {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 600.0),
                        )),
                        time: Some(0.0),
                        ..default()
                    },
                    |ui| {
                        egui::CentralPanel::default().show(ui, |ui| {
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
                if step < 2 {
                    continue;
                }
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
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(lanes.len(), missions.len());
                for (index, lane) in lanes.iter().enumerate() {
                    if index < 3 {
                        assert!(
                            (lane.left() - lanes[0].left()).abs() < 0.5,
                            "width {width}: {lanes:?}"
                        );
                        assert!(
                            (lane.right() - lanes[0].right()).abs() < 0.5,
                            "width {width}: {lanes:?}"
                        );
                    }
                    assert!(lane.width() > 0.0);
                }
                let objectives = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Mesh(mesh) if mesh.texture_id == objective_texture => {
                            Some(mesh.calc_bounds())
                        },
                        egui::Shape::Rect(rect)
                            if rect.brush.is_some()
                                && rect.fill_texture_id() == objective_texture =>
                        {
                            Some(rect.rect)
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert_eq!(objectives.len(), missions.len());
                let enemy_lane = lanes[3];
                assert!((enemy_lane.left() - objectives[3].right() - 8.0).abs() < 0.5,
                    "the enemy animation must start immediately after its objective: {enemy_lane:?}, {:?}", objectives[3]);
                assert!(enemy_lane.width() > lanes[0].width() + 100.0);
                let row_text = |name: &str| {
                    output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text)
                            if text.galley.text() == name
                                && (text.pos.y - enemy_lane.center().y).abs() < 40.0 =>
                        {
                            Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                        },
                        _ => None,
                    })
                };
                let enemy_eta =
                    row_text(&format!("+{}", mission_display_turns(&missions[3], &map))).unwrap();
                let origin = row_text("Origin").unwrap();
                let destination = row_text("Destination").unwrap();
                let contents = objectives[3].union(enemy_lane).union(enemy_eta);
                assert!(
                    (contents.center().x - (origin.center().x + destination.center().x) * 0.5)
                        .abs()
                        < 0.5,
                    "width {width}: enemy route contents must be centered without a recall slot"
                );
                for label in ["0%", "50%", "100%"] {
                    let percentage = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                            _ => None,
                        })
                        .unwrap();
                    assert!(percentage.pos.x + percentage.galley.size().x < lanes[0].left());
                }
            }
        }
    }
}

#[test]
fn compact_recall_action_sits_between_eta_and_destination() {
    let context = egui::Context::default();
    let images = ImageIds::default();
    let mut positions = None;

    for _ in 0..3 {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 300.0),
                )),
                ..default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let (route_width, leading) = mission_row_layout(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.add_space(leading);
                        egui::Grid::new("inline recall regression")
                            .spacing([MISSION_COLUMN_GAP, 0.0])
                            .show(ui, |ui| {
                                draw_mission_planet_link(
                                    ui,
                                    egui::TextureId::User(1),
                                    "Origin",
                                    Sense::click(),
                                );
                                let (mut route, _) = mission_route_cell(ui, route_width);
                                let mut eta_and_recall = None;
                                route.horizontal_centered(|ui| {
                                    ui.spacing_mut().item_spacing.x = 8.0;
                                    ui.allocate_exact_size([25.0, 25.0].into(), Sense::hover());
                                    ui.allocate_exact_size(
                                        [
                                            (route_width - MISSION_ROUTE_FIXED_CONTENT_WIDTH)
                                                .max(180.0),
                                            MISSION_ROUTE_PREVIEW_HEIGHT,
                                        ]
                                        .into(),
                                        Sense::hover(),
                                    );
                                    let eta = ui.label(RichText::new("+3").strong());
                                    ui.add_space(10.0);
                                    let recall = draw_recall_button(ui, &images, true);
                                    eta_and_recall = Some((eta.rect, recall.rect));
                                });
                                let (destination, _) = draw_mission_planet_link(
                                    ui,
                                    egui::TextureId::User(2),
                                    "Destination",
                                    Sense::click(),
                                );
                                if !ui.is_sizing_pass() {
                                    let (eta, recall) = eta_and_recall.unwrap();
                                    positions = Some((eta, recall, destination.rect));
                                }
                                ui.end_row();
                            });
                    });
                });
            },
        );
        output.textures_delta.clear();
    }

    let (eta, recall, destination) = positions.unwrap();
    assert!(eta.right() < recall.left());
    assert!(recall.right() < destination.left());
    assert!((recall.center().y - destination.center().y).abs() < 0.5);
    assert_eq!(recall.size(), egui::Vec2::splat(MISSION_RECALL_BUTTON_SIZE));
}

#[test]
fn active_mission_route_and_objective_hovers_keep_only_the_fleet_preview() {
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.owned = Some(1);
    origin.controlled = Some(1);
    let destination = Planet::new(1, "Destination".into(), Vec2::X * 500.0, false, 1.0);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, destination],
    };
    let player = Player::new(1, 0);
    let objective_texture = egui::TextureId::User(91);

    for width in [700.0, 1_200.0] {
        for editable in [true, false] {
            for (owner, objective, expected) in [
                (1, Icon::Attack, "Attack"),
                (1, Icon::Spy, "Spy"),
                (1, Icon::MissileStrike, "Missile Strike"),
                (1, Icon::Protect, "Protect"),
                (1, Icon::Destroy, "Destroy"),
                (1, Icon::Deploy, "Deploy"),
                (1, Icon::Colonize, "Colonize"),
                (2, Icon::Attack, "Unknown"),
            ] {
                let mission = Mission::new_with_id(
                    7,
                    1,
                    owner,
                    map.get(0),
                    map.get(1),
                    objective,
                    Army::from([(Unit::probe(), 1)]),
                    BombingRaid::None,
                    false,
                    false,
                    None,
                );
                let images = ImageIds(
                    [(mission.displayed_objective(player.id).asset_key(), objective_texture)]
                        .into_iter()
                        .collect(),
                );
                let context = egui::Context::default();
                context.all_styles_mut(|style| {
                    style.interaction.tooltip_delay = 0.0;
                    style.interaction.show_tooltips_only_when_still = false;
                });
                let mut state = UiState::default();
                let mut world = World::new();
                world.init_resource::<Messages<RecallMissionMsg>>();
                let mut params =
                    bevy::ecs::system::SystemState::<MessageWriter<RecallMissionMsg>>::new(
                        &mut world,
                    );
                let mut objective_rect = egui::Rect::NOTHING;
                let mut route_position = egui::Pos2::ZERO;
                let eta = format!("+{}", mission_display_turns(&mission, &map));

                // Settle layout, hover the arrows, move onto the objective, then back.
                for step in 0..15 {
                    let events = if step < 3 {
                        Vec::new()
                    } else {
                        let position = if (7..11).contains(&step) {
                            objective_rect.center()
                        } else {
                            route_position
                        };
                        vec![egui::Event::PointerMoved(position)]
                    };
                    state.mission_hover_from_ui = false;
                    let mut output = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(width, 600.0),
                            )),
                            time: Some(step as f64),
                            events,
                            ..default()
                        },
                        |context| {
                            egui::CentralPanel::default().show(context, |ui| {
                                draw_active_missions(
                                    ui,
                                    vec![&mission],
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
                    let objective_bounds =
                        output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Mesh(mesh) if mesh.texture_id == objective_texture => {
                                Some(mesh.calc_bounds())
                            },
                            egui::Shape::Rect(rect)
                                if rect.brush.is_some()
                                    && rect.fill_texture_id() == objective_texture =>
                            {
                                Some(rect.rect)
                            },
                            _ => None,
                        });
                    let Some(bounds) = objective_bounds else {
                        assert!(step < 3, "objective must be painted after layout settles");
                        continue;
                    };
                    objective_rect = bounds;
                    route_position = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == eta => {
                                Some(text.pos + egui::vec2(-16.0, text.galley.size().y * 0.5))
                            },
                            _ => None,
                        })
                        .unwrap();
                    if matches!(step, 6 | 10 | 14) {
                        let texts = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) => Some(text.galley.text()),
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(
                            texts.contains(&expected),
                            step == 10,
                            "width {width}, editable {editable}, owner {owner}, step {step}: {texts:?}"
                        );
                        assert!(!texts.contains(&"Origin:"), "{texts:?}");
                        assert!(!texts.contains(&"Destination:"), "{texts:?}");
                        assert_eq!(state.mission_hover, Some(mission.id));
                        assert!(state.mission_hover_from_ui);
                    }
                }
            }
        }
    }
}

#[test]
fn mission_animations_focus_the_current_location_with_a_marker_at_each_panel_scale() {
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.owned = Some(1);
    origin.controlled = Some(1);
    let destination = Planet::new(1, "Destination".into(), Vec2::X * 500.0, false, 1.0);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, destination],
    };
    let player = Player::new(1, 0);
    let images = ImageIds::default();
    for owner in [1, 2] {
        for kind in ["outbound", "returning", "jump gate", "patrol outbound", "patrolling"] {
            let mut mission = if kind.starts_with("patrol") {
                Mission::new_patrol(
                    7,
                    1,
                    owner,
                    map.get(0),
                    Vec2::new(200.0, 250.0),
                    Army::from([(Unit::probe(), 1)]),
                    None,
                )
            } else {
                Mission::new_with_id(
                    7,
                    1,
                    owner,
                    map.get(0),
                    map.get(1),
                    Icon::Attack,
                    Army::from([(Unit::probe(), 1)]),
                    BombingRaid::None,
                    false,
                    false,
                    None,
                )
            };
            mission.position = Vec2::new(180.0, 80.0);
            match kind {
                "returning" => mission.recall(&map, 2),
                "jump gate" => mission.jump_gate = true,
                "patrolling" => {
                    let patrol = mission.patrol.as_mut().unwrap();
                    patrol.phase = crate::core::patrols::PatrolPhase::OnStation;
                    mission.position = patrol.center;
                },
                _ => {},
            }
            for viewport in [egui::vec2(1280.0, 720.0), egui::vec2(800.0, 600.0)] {
                for editable in [true, false] {
                    let context = egui::Context::default();
                    context.set_global_style(NordDark.custom_style());
                    let scale = mission_panel_scale(viewport);
                    let size = mission_panel_size(viewport / scale);
                    let position = (viewport - size * scale) * 0.5;
                    let mut state = UiState {
                        mission: true,
                        mission_tab: if owner == player.id {
                            MissionTab::ActiveMissions
                        } else {
                            MissionTab::EnemyMissions
                        },
                        planet_selected: Some(0),
                        focus_planet: Some(0),
                        focus_position: Some(Vec2::ZERO),
                        focus_zoom: Some(2.0),
                        ..default()
                    };
                    let mut world = World::new();
                    world.init_resource::<Messages<RecallMissionMsg>>();
                    let mut params = bevy::ecs::system::SystemState::<
                        MessageWriter<RecallMissionMsg>,
                    >::new(&mut world);
                    let mut target_rect = egui::Rect::NOTHING;
                    // Settle layout, hover, secondary-click, then primary-click the animation.
                    for frame in 0..8 {
                        let mut events = Vec::new();
                        if frame >= 3 {
                            events.push(egui::Event::PointerMoved(target_rect.center()));
                            if frame >= 4 {
                                events.push(egui::Event::PointerButton {
                                    pos: target_rect.center(),
                                    button: if frame < 6 {
                                        egui::PointerButton::Secondary
                                    } else {
                                        egui::PointerButton::Primary
                                    },
                                    pressed: frame % 2 == 0,
                                    modifiers: egui::Modifiers::NONE,
                                });
                            }
                        }
                        let mut output = context.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    viewport,
                                )),
                                time: Some(frame as f64 * 0.1),
                                events,
                                ..default()
                            },
                            |ui| {
                                draw_panel_on_context(
                                    ui.ctx(),
                                    "mission navigation",
                                    "panel",
                                    (position.x, position.y),
                                    (size.x, size.y),
                                    0.0,
                                    scale,
                                    &images,
                                    |ui| {
                                        draw_active_missions(
                                            ui,
                                            vec![&mission],
                                            &mut params.get_mut(&mut world).unwrap(),
                                            &mut state,
                                            &map,
                                            &player,
                                            &MultiplayerSession::default(),
                                            true,
                                            &images,
                                            editable,
                                        )
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
                                    egui::Shape::LineSegment {
                                        stroke,
                                        ..
                                    } if stroke.color
                                        == Color32::from_rgba_unmultiplied(143, 158, 174, 52) =>
                                    {
                                        Some(shape.clip_rect)
                                    },
                                    egui::Shape::Path(path) if path.points.len() > 2 => {
                                        Some(shape.clip_rect)
                                    },
                                    _ => None,
                                })
                                .unwrap();
                            assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)
                                .contains_rect(target_rect));
                        }
                        if frame == 3 {
                            assert_eq!(
                                output.platform_output.cursor_icon,
                                CursorIcon::PointingHand,
                                "owner {owner}, {kind}, viewport {viewport:?}, editable {editable}"
                            );
                        }
                        if frame < 7 {
                            assert!(state.mission, "only a primary click release should navigate");
                            assert_eq!(state.focus_position, Some(Vec2::ZERO));
                            assert!(state.location_marker.is_none());
                        }
                    }
                    assert!(
                        !state.mission,
                        "owner {owner}, {kind}, viewport {viewport:?}, editable {editable}"
                    );
                    assert!(state.to_selected);
                    assert_eq!(state.focus_position, Some(mission.position));
                    assert_eq!(state.location_marker.as_ref().unwrap().position, mission.position);
                    assert_eq!(
                        state.location_marker.as_ref().unwrap().mission_id,
                        Some(mission.id)
                    );
                    assert_eq!(state.focus_planet, None);
                    assert_eq!(state.focus_zoom, None);
                    assert_eq!(state.planet_selected, None);
                    assert_eq!(state.planet_hover, None);
                    assert_eq!(state.mission_planet_hover, None);
                    assert_eq!(state.mission_hover, None);
                    assert!(!state.mission_hover_from_ui);
                    assert!(state.mission_battle_plan_changes.is_empty());
                    assert!(world.resource::<Messages<RecallMissionMsg>>().is_empty());
                }
            }
        }
    }
}

#[test]
fn recall_hover_keeps_the_fleet_preview_and_preserves_click_behavior() {
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.owned = Some(1);
    origin.controlled = Some(1);
    let destination = Planet::new(1, "Destination".into(), Vec2::X * 500.0, false, 1.0);
    let mission = Mission::new_with_id(
        7,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::probe(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, destination],
    };
    let player = Player::new(1, 0);
    let images = ImageIds(
        [
            ("recall".to_owned(), egui::TextureId::User(76)),
            ("recall hover".to_owned(), egui::TextureId::User(77)),
        ]
        .into_iter()
        .collect(),
    );

    for width in [700.0, 1_200.0] {
        for editable in [true, false] {
            let context = egui::Context::default();
            context.all_styles_mut(|style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let mut state = UiState {
                mission: true,
                ..default()
            };
            let mut world = World::new();
            world.init_resource::<Messages<RecallMissionMsg>>();
            let mut params =
                bevy::ecs::system::SystemState::<MessageWriter<RecallMissionMsg>>::new(&mut world);
            let mut recall_rect = egui::Rect::NOTHING;

            // Settle the grid, then move from the arrows onto recall, click, and leave the row.
            for step in 0..9 {
                let mut events = Vec::new();
                if step >= 3 {
                    let position = match step {
                        3 => recall_rect.center() - egui::vec2(80.0, 0.0),
                        8 => egui::pos2(10.0, 390.0),
                        _ => recall_rect.center(),
                    };
                    events.push(egui::Event::PointerMoved(position));
                    if matches!(step, 6 | 7) {
                        events.push(egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed: step == 6,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                state.mission_hover_from_ui = false;
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 400.0),
                        )),
                        time: Some(step as f64 * 0.1),
                        events,
                        ..default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            draw_active_missions(
                                ui,
                                vec![&mission],
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
                let texts = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some(text.galley.text()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert!(!texts.contains(&"Origin:"), "{texts:?}");
                assert!(!texts.contains(&"Destination:"), "{texts:?}");
                if step == 5 {
                    assert!(
                        texts.iter().any(|text| text.starts_with(if editable {
                            "Recall this mission."
                        } else {
                            "Continue your turn"
                        })),
                        "width {width}, editable {editable}: {texts:?}"
                    );
                }
                for shape in &output.shapes {
                    if let egui::Shape::Mesh(mesh) = &shape.shape {
                        if matches!(mesh.texture_id, egui::TextureId::User(76 | 77)) {
                            recall_rect = mesh.calc_bounds();
                            if matches!(step, 4 | 5) {
                                assert_eq!(
                                    mesh.texture_id,
                                    egui::TextureId::User(if editable {
                                        77
                                    } else {
                                        76
                                    }),
                                );
                            }
                        }
                    }
                }
                assert!(recall_rect.is_positive(), "the recall button must be visible");
                assert!(
                    !output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::Shape::Rect(rect)
                        if rect.rect == recall_rect && rect.fill != Color32::TRANSPARENT)
                    }),
                    "the recall icon must preserve its transparent exterior"
                );
                // Pressing a non-interactive icon can start dragging the surrounding scroll area.
                if (3..=5).contains(&step) || (editable && matches!(step, 6 | 7)) {
                    assert_eq!(
                        state.mission_hover,
                        Some(mission.id),
                        "width {width}, editable {editable}, step {step}, recall {recall_rect:?}",
                    );
                    assert!(
                        state.mission_hover_from_ui,
                        "width {width}, editable {editable}, step {step}"
                    );
                } else if step == 8 {
                    assert_eq!(state.mission_hover, None);
                    assert!(!state.mission_hover_from_ui);
                }
            }
            let recalls =
                world.resource_mut::<Messages<RecallMissionMsg>>().drain().collect::<Vec<_>>();
            assert_eq!(recalls.len(), usize::from(editable));
            assert!(state.mission, "recalling a fleet must keep the mission panel open");
            assert!(state.location_marker.is_none());
            if editable {
                assert_eq!(recalls[0].mission_id, mission.id);
            }
        }
    }
}

#[test]
fn mission_planet_names_share_the_same_centered_planet_relative_position() {
    for left in [0.0, 640.0] {
        let cell = egui::Rect::from_min_size(
            egui::pos2(left, 20.0),
            egui::vec2(MISSION_PLANET_COLUMN_WIDTH, MISSION_PLANET_CELL_HEIGHT),
        );
        let (planet, name) = mission_planet_rects(cell);

        assert!((name.center().x - planet.center().x).abs() <= f32::EPSILON);
        assert_eq!(name.top(), planet.bottom() - MISSION_PLANET_NAME_OVERLAP);
    }
}

#[test]
fn mission_route_is_centered_on_the_planet_artwork() {
    let cell = egui::Rect::from_min_size(
        egui::pos2(120.0, 20.0),
        egui::vec2(480.0, MISSION_PLANET_CELL_HEIGHT),
    );
    let (planet, _) = mission_planet_rects(cell);
    let route = mission_route_rect(cell);

    assert!((route.center().y - planet.center().y).abs() <= f32::EPSILON);
    assert_eq!(route.width(), cell.width());
}

#[test]
fn enemy_mission_route_absorbs_clicks_with_the_default_cursor() {
    let context = egui::Context::default();
    let mut route_rect = egui::Rect::NOTHING;
    let mut route_clicked = false;

    for pressed in [None, Some(true), Some(false)] {
        let events = pressed.map_or_else(Vec::new, |pressed| {
            vec![
                egui::Event::PointerMoved(route_rect.center()),
                egui::Event::PointerButton {
                    pos: route_rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        });
        let mut output = context.run_ui(
            egui::RawInput {
                events,
                ..default()
            },
            |ui| {
                let (_, response) = mission_route_cell(ui, 400.0);
                route_rect = response.rect;
                let response = block_enemy_route_clicks(ui, response, 17);
                route_clicked |= response.clicked();
            },
        );
        output.textures_delta.clear();

        if pressed.is_some() {
            assert_eq!(output.platform_output.cursor_icon, CursorIcon::Default);
        }
    }

    assert!(route_clicked, "the route must consume the click instead of passing it to the map");
}

#[test]
fn active_mission_rows_are_centered_with_equal_outer_space() {
    for available_width in [700.0, 850.0, 1_200.0] {
        let (route_width, leading_space) = mission_row_layout(available_width);
        let row_width = 2.0 * MISSION_PLANET_COLUMN_WIDTH + 2.0 * MISSION_COLUMN_GAP + route_width;
        let trailing_space = available_width - leading_space - row_width;

        assert!((leading_space - trailing_space).abs() <= 0.0001);
        assert!((MISSION_ROUTE_COLUMN_MIN_WIDTH..=MISSION_ROUTE_COLUMN_MAX_WIDTH)
            .contains(&route_width));
    }
}

#[test]
fn active_mission_eta_never_displays_plus_zero() {
    let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    let destination = Planet::new(1, "Destination".into(), Vec2::X * 500.0, false, 1.0);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin.clone(), destination.clone()],
    };
    let mut mission = Mission::new_with_id(
        7,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::probe(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.recall(&map, 1);

    assert_eq!(mission.turns_to_destination(&map), 0);
    assert_eq!(mission_display_turns(&mission, &map), 1);
}

#[test]
fn missile_strikes_and_allied_attacks_do_not_offer_the_recall_action() {
    let map = crate::core::simulation::GameModel::new(
        [7; 32],
        crate::core::simulation::GameRules::default(),
    )
    .unwrap()
    .map;
    let owned_attack = Mission {
        owner: 7,
        origin_owned: Some(7),
        objective: Icon::Attack,
        ..default()
    };
    let missile_strike = Mission {
        objective: Icon::MissileStrike,
        ..owned_attack.clone()
    };
    let allied_attack = Mission {
        joint_attack: Some(JointAttackMission::default()),
        ..owned_attack.clone()
    };

    assert!(mission_recall_available(&owned_attack, &map, 7));
    assert!(!mission_recall_available(&missile_strike, &map, 7));
    assert!(!mission_recall_available(&allied_attack, &map, 7));
}

#[test]
fn revoked_protection_origin_does_not_offer_mission_recall() {
    let mut model = crate::core::simulation::GameModel::new(
        [81; 32],
        crate::core::simulation::GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let protected = model.players[2].home_planet;
    let destination = model.players[1].home_planet;
    model.map.get_mut(protected).protection_permissions.insert(1);
    let mission = Mission::new_with_id(
        88,
        model.turn as usize,
        1,
        model.map.get(protected),
        model.map.get(destination),
        Icon::Attack,
        Army::from([(Unit::Ship(crate::core::units::ships::Ship::LightFighter), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    assert!(mission_recall_available(&mission, &model.map, 1));
    model.map.get_mut(protected).protection_permissions.remove(&1);
    assert!(!mission_recall_available(&mission, &model.map, 1));
}

#[test]
fn route_preview_markers_remain_evenly_spaced_throughout_animation() {
    const SPACING: f32 = 27.0;

    for phase in [0.0, 6.5, 26.9, 27.0, 92.25] {
        let positions = route_marker_positions(12.0, 492.0, SPACING, phase).collect::<Vec<_>>();
        assert!(positions.len() > 10);
        assert!(positions.iter().all(|position| (12.0..=492.0).contains(position)));
        assert!(positions
            .windows(2)
            .all(|pair| { ((pair[1] - pair[0]) - SPACING).abs() <= 0.0001 }));
    }
}

#[test]
fn route_preview_marker_grid_handles_empty_or_invalid_lanes() {
    assert!(route_marker_positions(20.0, 10.0, 27.0, 0.0).next().is_none());
    assert!(route_marker_positions(10.0, 20.0, 0.0, 0.0).next().is_none());
}

#[test]
fn returning_routes_keep_the_outbound_planet_layout() {
    let outbound = Mission {
        owner: 7,
        origin: 3,
        origin_controlled: Some(7),
        destination: 9,
        objective: Icon::Attack,
        ..default()
    };
    let returning = Mission {
        owner: 7,
        origin: 9,
        origin_controlled: Some(2),
        destination: 3,
        objective: Icon::Deploy,
        return_objective: Some(Icon::Attack),
        ..default()
    };
    let marked_returning = Mission {
        origin_controlled: Some(7),
        return_objective: Some(Icon::Spy),
        ..returning.clone()
    };
    let recalled_deploy = Mission {
        return_objective: Some(Icon::Deploy),
        ..marked_returning.clone()
    };

    assert_eq!(mission_route_presentation(&outbound), (3, 9, false));
    assert_eq!(mission_route_presentation(&returning), (3, 9, true));
    assert_eq!(mission_route_presentation(&marked_returning), (3, 9, true));
    assert_eq!(mission_route_presentation(&recalled_deploy), (3, 9, true));
}

#[test]
fn route_chevrons_face_the_displayed_travel_direction() {
    let center = egui::pos2(20.0, 30.0);
    let outgoing = route_chevron(center, false);
    let returning = route_chevron(center, true);

    assert!(outgoing[0][1].x > outgoing[0][0].x);
    assert!(returning[0][1].x < returning[0][0].x);
    assert_eq!(outgoing[0][1], center + egui::vec2(2.0, 0.0));
    assert_eq!(returning[0][1], center + egui::vec2(-2.0, 0.0));
}

#[test]
fn jump_gate_preview_has_opposite_strands_with_constant_turn_spacing() {
    for width in [0.0, 8.0, 180.0, 480.0] {
        for returning in [false, true] {
            let [first, second] =
                jump_gate_preview_strands(12.0, 12.0 + width, 30.0, 0.7, returning);
            assert_eq!(first.len(), second.len());
            for (a, b) in first.iter().zip(&second) {
                assert_eq!(a.x, b.x);
                assert!((a.y + b.y - 60.0).abs() < 0.0001);
                assert!((12.0..=12.0 + width).contains(&a.x));
                assert!((22.5..=37.5).contains(&a.y));
            }
            for (a, b) in first.iter().zip(first.iter().skip(30)) {
                if a.x >= 22.0 && b.x <= 2.0 + width {
                    assert!((b.x - a.x - 60.0).abs() < 0.0001);
                    assert!((a.y - b.y).abs() < 0.0001);
                }
            }
        }
    }
    let short = jump_gate_preview_strands(12.0, 192.0, 30.0, 0.7, false);
    let long = jump_gate_preview_strands(12.0, 492.0, 30.0, 0.7, false);
    assert!(long[0].len() > short[0].len());
    for (a, b) in short[0].iter().zip(&long[0]).filter(|(a, _)| a.x < 182.0) {
        assert!(a.distance(*b) < 0.0001, "extending the route must not stretch its turns");
    }
}

#[test]
fn jump_gate_preview_reverses_motion_on_the_return_leg() {
    let outbound = jump_gate_preview_strands(0.0, 180.0, 30.0, 0.4, false);
    let returning = jump_gate_preview_strands(0.0, 180.0, 30.0, 0.4, true);
    let previous = jump_gate_preview_strands(0.0, 180.0, 30.0, -0.4, false);
    assert_eq!(returning, previous);
    assert_ne!(outbound, returning);
}

#[test]
fn route_preview_speed_tracks_next_turn_movement_and_acceleration() {
    let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    let destination =
        Planet::new(1, "Destination".into(), Vec2::X * Planet::SIZE * 20.0, false, 1.0);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin.clone(), destination.clone()],
    };
    let mut mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::ColonyShip), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );

    let launch_speed = mission.route_animation_speed(&map);
    mission.advance(&map);
    let accelerated_speed = mission.route_animation_speed(&map);

    assert!(accelerated_speed > launch_speed);
    assert_eq!(accelerated_speed, 16.0 * f64::from(mission.next_turn_movement(&map)));
}

#[test]
fn report_thumbnail_scales_balance_broad_mission_artwork() {
    let base_size = 50.0;

    assert_eq!(
        mission_report_image_size("mission", base_size),
        base_size * MISSION_FLEET_IMAGE_SCALE
    );
    assert_eq!(
        mission_report_image_size("mission spy", base_size),
        base_size * MISSION_SPY_IMAGE_SCALE
    );
    assert_eq!(
        mission_report_image_size("mission colonize", base_size),
        base_size * MISSION_COLONY_IMAGE_SCALE
    );
    assert_eq!(mission_report_image_size("mission destroy", base_size), base_size);
}

#[test]
fn mission_report_thumbnail_size_does_not_shift_the_following_column() {
    let context = egui::Context::default();
    let mut following_column_lefts = Vec::new();

    let mut output = context.run_ui(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            for size in [48.0, 48.0 * MISSION_SPY_IMAGE_SCALE] {
                ui.horizontal(|ui| {
                    draw_mission_image(
                        ui,
                        egui::TextureId::User(1),
                        size,
                        MISSION_REPORT_IMAGE_SLOT_SIZE,
                        egui::Vec2::ZERO,
                        Color32::WHITE,
                    );
                    following_column_lefts.push(ui.label("4").rect.left());
                });
            }
        });
    });
    output.textures_delta.clear();

    assert_eq!(following_column_lefts.len(), 2);
    assert!((following_column_lefts[0] - following_column_lefts[1]).abs() <= f32::EPSILON);
}

#[test]
fn missile_report_thumbnail_is_optically_shifted_left() {
    assert_eq!(
        mission_report_image_offset("mission missile"),
        egui::vec2(MISSION_MISSILE_IMAGE_OFFSET_X, 0.0)
    );
    assert_eq!(mission_report_image_offset("mission"), egui::Vec2::ZERO);
}

#[test]
fn report_thumbnail_size_changes_only_for_selection() {
    assert_eq!(mission_report_image_base_size(false), MISSION_REPORT_IMAGE_SIZE);
    assert_eq!(mission_report_image_base_size(true), MISSION_REPORT_SELECTED_IMAGE_SIZE);
}

#[test]
fn report_list_top_padding_keeps_the_outside_hover_stroke_visible() {
    const { assert!(MISSION_REPORT_LIST_TOP_PADDING >= MISSION_REPORT_HOVER_STROKE_WIDTH * 0.5) };
}

#[test]
fn mission_tabs_are_centered_within_the_panel() {
    let context = egui::Context::default();
    context.global_style_mut(|style| {
        style
            .text_styles
            .insert(TextStyle::Body, egui::FontId::new(23.0, egui::FontFamily::Proportional));
        style.spacing.item_spacing.x = 18.0;
    });
    let mut selected = MissionTab::NewMission;
    for width in [840.0, 1_200.0] {
        let mut centers = None;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 400.0),
                )),
                ..default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    let panel_center = ui.available_rect_before_wrap().center().x;
                    let tab_row = draw_mission_tabs(ui, &mut selected);
                    centers = Some((panel_center, tab_row.center().x));
                });
            },
        );
        output.textures_delta.clear();

        let (panel_center, tab_center) = centers.expect("the tab row should be drawn");
        assert!(
            (panel_center - tab_center).abs() <= 0.5,
            "panel width {width}: panel center {panel_center}, tab center {tab_center}"
        );
    }
}

#[test]
fn unavailable_jump_gate_route_cannot_keep_the_previous_missions_icon() {
    let player = Player::new(1, 1);
    let mut origin = Planet::new(1, "Origin".to_string(), Vec2::ZERO, false, 1.0);
    let mut destination = Planet::new(2, "Destination".to_string(), Vec2::X, false, 1.0);
    origin.owned = Some(player.id);
    destination.owned = Some(player.id);
    origin.army.insert(Unit::Building(Building::JumpGate), 1);

    let mut draft = Mission {
        owner: player.id,
        objective: Icon::Deploy,
        jump_gate: true,
        ..default()
    };

    sync_jump_gate_selection(&mut draft, &origin, &destination, &player, true);

    assert!(!draft.jump_gate);
    assert_eq!(draft.image(&player), "mission");
}

#[test]
fn remembered_jump_gate_selection_only_applies_to_an_available_route() {
    let player = Player::new(1, 1);
    let mut origin = Planet::new(1, "Origin".to_string(), Vec2::ZERO, false, 1.0);
    let mut destination = Planet::new(2, "Destination".to_string(), Vec2::X, false, 1.0);
    origin.owned = Some(player.id);
    destination.owned = Some(player.id);
    origin.army.insert(Unit::Building(Building::JumpGate), 1);
    destination.army.insert(Unit::Building(Building::JumpGate), 1);

    let mut draft = Mission {
        owner: player.id,
        objective: Icon::Deploy,
        ..default()
    };

    sync_jump_gate_selection(&mut draft, &origin, &destination, &player, true);

    assert!(draft.jump_gate);
    assert_eq!(draft.image(&player), "mission");

    draft.objective = Icon::Attack;
    sync_jump_gate_selection(&mut draft, &origin, &destination, &player, true);

    assert!(!draft.jump_gate);
    assert_eq!(draft.image(&player), "mission");
}
