use super::*;
use crate::core::simulation::{GameModel, GameRules};

#[test]
fn right_clicking_a_visible_contact_opens_a_draft_and_preserves_selected_origin() {
    let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = &model.players[0];
    let home = player.home_planet;
    model.map.get_mut(home).army.insert(Unit::Building(Building::SensorPhalanx), 20);
    let mut target = Mission::new_with_id(
        900,
        1,
        model.players[1].id,
        model.map.get(model.players[1].home_planet),
        model.map.get(home),
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    target.position = model.map.get(home).position + Vec2::X * Planet::SIZE;
    let mut state = UiState {
        planet_selected: Some(home),
        selecting_patrol_zone: true,
        ..Default::default()
    };
    assert!(state.choose_intercept_target(&model.map, player, 1, &target));
    assert!(state.mission);
    assert_eq!(state.mission_info.origin, home);
    assert_eq!(state.intercept_target, Some(900));
    assert_eq!(state.mission_tab, MissionTab::NewMission);
    assert!(!state.selecting_patrol_zone);
    assert_eq!(state.mission_info.objective, Icon::Intercept);
    target.owner = player.id;
    assert!(!state.choose_intercept_target(&model.map, player, 1, &target));

    let enemy_home = model.players[1].home_planet;
    state.set_planet_mission_destination(enemy_home);
    assert_eq!(state.mission_info.destination, enemy_home);
    assert_eq!(state.mission_info.objective, Icon::Attack);
    assert!(state.intercept_target.is_none());
}

#[test]
fn intercept_uses_the_normal_panel_send_button_and_disabled_hover_reasons() {
    let objective_icons = Icon::iter().filter(Icon::is_mission).collect::<Vec<_>>();
    let images = ImageIds(
        objective_icons
            .iter()
            .enumerate()
            .map(|(index, icon)| (icon.asset_key(), egui::TextureId::User(index as u64 + 1)))
            .collect(),
    );
    for size in [egui::vec2(820.0, 560.0), egui::vec2(520.0, 380.0), egui::vec2(320.0, 280.0)] {
        for case in [
            "reachable",
            "final",
            "empty",
            "final_empty",
            "arrived",
            "unreachable",
            "lost",
            "hidden",
            "fuel",
        ] {
            let sendable = matches!(case, "reachable" | "final");
            let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
            model.start().unwrap();
            model.map.rect =
                bevy::prelude::Rect::from_corners(Vec2::splat(-10_000.0), Vec2::splat(10_000.0));
            let mut player = model.players[0].clone();
            player.resources.deuterium = if case == "fuel" {
                0
            } else {
                100_000
            };
            let home = player.home_planet;
            let enemy_home = model.players[1].home_planet;
            model.map.get_mut(home).position = Vec2::ZERO;
            model.map.get_mut(home).army.insert(Unit::Building(Building::SensorPhalanx), 30);
            let army = Army::from([(Unit::Ship(Ship::LightFighter), 8)]);
            model.map.get_mut(home).army.extend(army.clone());
            let mut target = Mission::new_with_id(
                900,
                1,
                model.players[1].id,
                model.map.get(enemy_home),
                model.map.get(home),
                Icon::Attack,
                Army::from([(Unit::Ship(Ship::Battleship), 4)]),
                BombingRaid::None,
                true,
                false,
                None,
            );
            target.position = Vec2::X * Planet::SIZE * 12.0;
            if case.starts_with("final") {
                target.position = Vec2::X * Planet::SIZE;
            }
            if case == "arrived" {
                target.position = Vec2::X * Planet::SIZE * 0.5;
            }
            if case == "unreachable" {
                model.map.get_mut(enemy_home).position = Vec2::X * Planet::SIZE * 30.0;
                target.origin = home;
                target.destination = enemy_home;
                target.position = Vec2::X * Planet::SIZE * 6.0;
                target.travel_turns = 4;
                target.army = Army::from([(Unit::probe(), 5)]);
            }
            let mut state = UiState {
                planet_selected: Some(home),
                mission_info: Mission {
                    origin: home,
                    destination: enemy_home,
                    ..Default::default()
                },
                ..Default::default()
            };
            assert!(state.choose_intercept_target(&model.map, &player, 1, &target));
            if case == "hidden" {
                for planet in &mut model.map.planets {
                    planet.army.remove(&Unit::Building(Building::SensorPhalanx));
                    planet.army.remove(&Unit::Building(Building::OrbitalRadar));
                }
                assert!(!visible_contact(&target, &model.map, &player));
            }
            if !matches!(case, "empty" | "final_empty") {
                state.mission_info.army = army;
            }
            let contacts = if case == "lost" {
                vec![]
            } else {
                vec![target]
            };
            let context = egui::Context::default();
            context.style_mut_of(context.theme(), |style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.tooltip_grace_time = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let mut world = World::new();
            world.init_resource::<Messages<SendMissionMsg>>();
            world.init_resource::<Messages<MultiplayerRequest>>();
            let mut params = bevy::ecs::system::SystemState::<(
                MessageWriter<SendMissionMsg>,
                MessageWriter<MultiplayerRequest>,
            )>::new(&mut world);
            let mut button = egui::Rect::NOTHING;
            let mut saw_reason = false;
            let expected = match case {
                "empty" | "final_empty" => "at least one combat ship",
                "arrived" => "already reached its destination",
                "unreachable" => "cannot reach a meeting point",
                "lost" => "no longer be visible",
                "hidden" => "no longer visible",
                "fuel" => "Not enough fuel",
                _ => "",
            };
            for frame in 0..8 {
                let mut events = Vec::new();
                if frame >= 2 {
                    events.push(egui::Event::PointerMoved(button.center()));
                    if frame <= 3 {
                        events.push(egui::Event::PointerButton {
                            pos: button.center(),
                            button: egui::PointerButton::Primary,
                            pressed: frame == 2,
                            modifiers: egui::Modifiers::default(),
                        });
                    }
                }
                let mut keyboard = ButtonInput::default();
                if frame == 4 {
                    keyboard.press(KeyCode::Enter);
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        time: Some(frame as f64 * 0.5),
                        events,
                        ..Default::default()
                    },
                    |context| {
                        egui::CentralPanel::default().show(context, |ui| {
                            let (mut sends, mut requests) = params.get_mut(&mut world).unwrap();
                            draw_new_mission(
                                ui,
                                &mut sends,
                                &contacts,
                                &Settings {
                                    turn: 1,
                                    ..Default::default()
                                },
                                &mut state,
                                &mut model.map,
                                &mut player,
                                &MultiplayerSession::default(),
                                &mut requests,
                                true,
                                &keyboard,
                                &images,
                            );
                        });
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
                let send = text.iter().find(|text| text.galley.text() == "Send mission").unwrap();
                if case == "hidden" {
                    assert!(!text.iter().any(|text| text.galley.text().ends_with("'s fleet")));
                }
                if frame == 0 && size.x >= 520.0 {
                    let visible_objectives = objective_icons
                        .iter()
                        .copied()
                        .filter(|icon| {
                            output.shapes.iter().any(|shape| {
                                matches!(&shape.shape, egui::Shape::Mesh(mesh)
                                    if mesh.texture_id == images.get(icon.asset_key())
                                        && mesh.calc_bounds().size() == egui::Vec2::splat(40.0))
                            })
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(visible_objectives, [Icon::Intercept], "{case} at {size:?}");
                    let mission = &state.mission_info;
                    let expected_details = if let Some(intercept) = &mission.intercept {
                        let distance =
                            model.map.get(home).position.distance(intercept.point) / Planet::SIZE;
                        let duration = mission.duration(&model.map);
                        let fuel = mission.dispatch_fuel_consumption(&model.map, &player);
                        assert!(distance > 0.0 && duration > 0 && fuel > 0);
                        vec![
                            format!("📏 Target distance: {distance:.1} AU"),
                            format!(
                                "⏱ Duration: +{duration} turn{} ({})",
                                if duration == 1 {
                                    ""
                                } else {
                                    "s"
                                },
                                1 + duration
                            ),
                            format!("⛽ Fuel consumption: {fuel}"),
                        ]
                    } else {
                        vec![
                            "📏 Target distance: ---".to_owned(),
                            "⏱ Duration: ---".to_owned(),
                            "⛽ Fuel consumption: ---".to_owned(),
                        ]
                    };
                    for expected in expected_details {
                        assert!(
                            text.iter().any(|text| text.galley.text() == expected),
                            "missing {expected} for {case} at {size:?}"
                        );
                    }
                }
                button = egui::Rect::from_min_size(send.pos, send.galley.size());
                assert!(
                    button.bottom() <= size.y && button.right() <= size.x,
                    "send escaped {size:?}: {button:?}"
                );
                assert!(!text.iter().any(|text| matches!(
                    text.galley.text(),
                    "Other objectives"
                        | "Enemy contact:"
                        | "Launch from:"
                        | "Rendezvous"
                        | "Send intercept"
                )));
                if frame >= 3 && !sendable {
                    saw_reason |= text.iter().any(|text| text.galley.text().contains(expected));
                }
                if sendable && frame == 3 {
                    break;
                }
                assert_eq!(
                    state.mission_info.objective,
                    Icon::Intercept,
                    "{case} must retain its objective"
                );
            }
            let sent = world.resource_mut::<Messages<SendMissionMsg>>().drain().collect::<Vec<_>>();
            assert_eq!(sent.len(), usize::from(sendable), "{case} at {size:?}");
            if !sendable {
                assert!(saw_reason, "missing {case} tooltip at {size:?}");
            }
            if let Some(sent) = sent.first() {
                assert_eq!(sent.mission.objective, Icon::Intercept);
                assert!(sent.mission.valid_intercept(&model.map));
                assert_eq!(sent.mission.intercept.as_ref().unwrap().target, 900);
            }
        }
    }
}
