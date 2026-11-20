use super::*;
use crate::core::sabotage::SabotageTarget;
use crate::core::simulation::{GameModel, GameRules};
use crate::test_support::empty_report;

#[test]
fn target_picker_uses_only_known_structures_and_keeps_probe_fleet_in_small_windows() {
    for width in [650.0, 900.0] {
        for public_railgun in [false, true] {
            let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
            model.start().unwrap();
            let mut player = model.players[0].clone();
            player.resources.deuterium = 100_000;
            let origin = player.home_planet;
            let destination = model.players[1].home_planet;
            model.map.get_mut(origin).army.insert(Unit::probe(), 5);
            model.map.get_mut(destination).army.insert(Unit::Building(Building::SensorPhalanx), 1);
            model.map.get_mut(destination).army.insert(Unit::Building(Building::JumpGate), 1);
            if public_railgun {
                model
                    .map
                    .get_mut(destination)
                    .army
                    .insert(Unit::Building(Building::OrbitalRailgun), 1);
            }
            let mission = Mission::new_with_id(
                800,
                1,
                player.id,
                model.map.get(origin),
                model.map.get(destination),
                Icon::Sabotage,
                Army::from([(Unit::probe(), 5)]),
                BombingRaid::None,
                false,
                false,
                None,
            );
            let mut state = UiState {
                mission: true,
                mission_info: mission,
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
            for pass in 0..2 {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 650.0),
                        )),
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
                            &ButtonInput::default(),
                            &ImageIds::default(),
                        );
                    },
                );
                output.textures_delta.clear();
                if pass == 1 {
                    let text = |label| {
                        output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => Some(text),
                            _ => None,
                        })
                    };
                    assert!(text("Send mission").is_some());
                    assert!(text("0%").is_none());
                    let deep_cover = text("🕶 Deep Cover:").expect("Deep Cover row");
                    let target = text("⌖ Sabotage target:").expect("sabotage target row");
                    assert!(deep_cover.pos.y < target.pos.y);

                    let selection = text(if public_railgun {
                        "Orbital Railgun"
                    } else {
                        "No known target"
                    })
                    .expect("sabotage dropdown selection");
                    assert_eq!(selection.galley.job.sections[0].format.font_id.size, 18.0);
                    assert!(!selection.galley.elided);
                    let button = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.fill == ABANDON_CONFIRMATION_BUTTON_FILL
                                && rect.rect.contains(selection.pos) =>
                        {
                            Some(rect.rect)
                        },
                        _ => None,
                    });
                    let button = button.expect("sabotage dropdown button");
                    assert!(button.width() >= MISSION_OPTION_WIDTH);
                    assert!(button.height() <= selection.galley.size().y + 3.0);
                }
            }
            assert_eq!(
                state.mission_info.sabotage.map(|sabotage| sabotage.target),
                public_railgun.then_some(SabotageTarget::OrbitalRailgun)
            );
            assert_eq!(state.mission_info.army, Army::from([(Unit::probe(), 5)]));
            assert_eq!(state.mission_info.objective, Icon::Sabotage);
            assert!(world.resource::<Messages<SendMissionMsg>>().is_empty());
        }
    }
}

#[test]
fn target_picker_keeps_full_names_and_compact_popup_rows() {
    for width in [375.0, 650.0] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        context.add_font(FontInsert::new(
            "firasans",
            FontData::from_static(include_bytes!("../../assets/fonts/FiraSans-Bold.ttf")),
            vec![InsertFontFamily {
                family: FontFamily::Proportional,
                priority: FontPriority::Highest,
            }],
        ));
        let mut mission =
            Mission::default().with_sabotage_target(Some(SabotageTarget::OrbitalRailgun));
        let mut frame = |events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 320.0),
                    )),
                    events,
                    ..default()
                },
                |ui| draw_sabotage_target(ui, &mut mission, SabotageTarget::ALL.to_vec()),
            );
            output.textures_delta.clear();
            output.shapes
        };

        let closed = frame(vec![]);
        let selection = closed
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Orbital Railgun" => Some(text),
                _ => None,
            })
            .expect("selected Orbital Railgun");
        assert!(!selection.galley.elided);
        assert_eq!(selection.galley.rows.len(), 1);

        let pos = selection.pos + selection.galley.size() * 0.5;
        let button_event = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(vec![egui::Event::PointerMoved(pos), button_event(true)]);
        frame(vec![button_event(false)]);
        let open = frame(vec![]);
        let mut previous_row_bottom = None;
        for target in SabotageTarget::ALL {
            let name = target.name();
            let option = open
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == name && text.pos.y > selection.pos.y =>
                    {
                        Some(text)
                    },
                    _ => None,
                })
                .expect(name);
            assert!(!option.galley.elided);
            assert_eq!(option.galley.rows.len(), 1);
            if let Some(bottom) = previous_row_bottom {
                // The 5-point row gap and rounded button padding leave about 9 points.
                let gap = option.pos.y - bottom;
                assert!((0.0..=10.0).contains(&gap), "popup row gap was {gap}");
            }
            previous_row_bottom = Some(option.pos.y + option.galley.size().y);
        }
    }
}

#[test]
fn mission_report_shows_the_sabotaged_structure_below_scouts() {
    let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let mut player = model.players[0].clone();
    let destination = model.map.get(model.players[1].home_planet);
    let scout_texture = egui::TextureId::User(91);
    let sabotage_texture = egui::TextureId::User(92);
    let images = ImageIds(HashMap::from([
        (Icon::Spy.asset_key(), scout_texture),
        (Icon::Sabotage.asset_key(), sabotage_texture),
    ]));

    for (target, succeeded, width) in [
        (SabotageTarget::OrbitalRailgun, true, 800.0),
        (SabotageTarget::JumpGate, true, 1280.0),
        (SabotageTarget::JumpGate, false, 800.0),
    ] {
        let mission = Mission {
            owner: player.id,
            origin: player.home_planet,
            destination: destination.id,
            objective: Icon::Sabotage,
            army: Army::from([(Unit::probe(), 13)]),
            sabotage: Some(Sabotage {
                target,
                succeeded,
            }),
            ..default()
        };
        let mut report = empty_report(mission, destination.clone());
        report.id = 42;
        report.scout_probes = 13;
        player.reports = vec![report];
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut state = UiState {
            mission_report: Some(42),
            ..default()
        };
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 650.0),
                )),
                ..default()
            },
            |ui| {
                draw_mission_reports(
                    ui,
                    &mut state,
                    &model.map,
                    &player,
                    &MultiplayerSession::default(),
                    false,
                    &images,
                );
            },
        );
        output.textures_delta.clear();
        let text_rect = |label| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == label => {
                    Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
                },
                _ => None,
            })
        };
        let small_icon = |texture| {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                        Some(mesh.calc_bounds())
                    },
                    egui::Shape::Rect(rect) if rect.fill_texture_id() == texture => Some(rect.rect),
                    _ => None,
                })
                .find(|rect| rect.width() == 15.0)
        };
        let scouts = text_rect("Scouts: 13").expect("scout count");
        match (text_rect(target.name()), small_icon(sabotage_texture), succeeded) {
            (Some(name), Some(icon), true) => {
                let scout_icon = small_icon(scout_texture).expect("scout icon");
                assert!(name.top() >= scouts.bottom());
                assert_eq!(icon.left(), scout_icon.left());
                assert!(icon.right() < name.left());
                assert!(name.y_range().contains(icon.center().y));
            },
            (None, None, false) => {},
            _ => panic!("only successful sabotage should identify a disabled structure"),
        }
    }
}
