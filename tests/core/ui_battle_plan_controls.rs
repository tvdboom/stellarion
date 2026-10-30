use super::*;

fn text_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == label => {
            Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
        },
        _ => None,
    })
}

fn pointer(position: egui::Pos2, pressed: Option<bool>) -> Vec<egui::Event> {
    let mut events = vec![egui::Event::PointerMoved(position)];
    if let Some(pressed) = pressed {
        events.push(egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
    }
    events
}

#[test]
fn hover_editor_stays_open_through_nested_tactic_target_and_protection_choices() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut plan = BattlePlan {
        automatic_tactic: false,
        ..Default::default()
    };
    let army = Army::from([(Unit::Ship(Ship::LightFighter), 10)]);
    let mut icon = egui::Pos2::ZERO;
    let mut time = 1.0;
    let mut frame = |plan: &mut BattlePlan, events| {
        time += 0.05;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1000.0, 900.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            },
            |ui| {
                ui.set_width(400.0);
                icon =
                    draw_mission_plan_button(ui, &ImageIds::default(), plan, &army).rect.center();
            },
        );
        output.textures_delta.clear();
        (output, icon)
    };
    let (_, icon) = frame(&mut plan, vec![]);
    frame(&mut plan, pointer(icon, None));
    let (output, _) = frame(&mut plan, pointer(icon, None));
    assert!(text_rect(&output, "Tactic").is_some(), "hover must open the editor");
    assert_eq!(output.platform_output.cursor_icon, CursorIcon::Default);
    assert!(text_rect(&output, "Battle plan: Breakthrough").is_none());
    let card = text_rect(&output, "Breakthrough").unwrap().center();
    frame(&mut plan, pointer(card, Some(true)));
    frame(&mut plan, pointer(card, Some(false)));
    let (output, _) = frame(&mut plan, pointer(card, None));
    let choice = text_rect(&output, "Swarm").expect("tactic rows must open on click").center();
    assert!(text_rect(&output, "100%").is_some());
    assert!(text_rect(&output, "+15.0%").is_some());
    assert!(text_rect(&output, "Good against: Breakthrough, Siege").is_none());
    assert!(text_rect(&output, "Enemy bonus").is_none());
    frame(&mut plan, pointer(choice, None));
    frame(&mut plan, pointer(choice, Some(true)));
    frame(&mut plan, pointer(choice, Some(false)));
    assert_eq!(plan.tactic, CombatTactic::Swarm);

    for (heading, option) in [("Priority", "Heavy targets"), ("Protection", "Light screen")] {
        let (output, _) = frame(&mut plan, pointer(icon, None));
        let card = text_rect(&output, heading).unwrap().center();
        frame(&mut plan, pointer(card, Some(true)));
        frame(&mut plan, pointer(card, Some(false)));
        let (output, _) = frame(&mut plan, pointer(card, None));
        let choice = text_rect(&output, option).unwrap().center();
        frame(&mut plan, pointer(choice, None));
        frame(&mut plan, pointer(choice, Some(true)));
        frame(&mut plan, pointer(choice, Some(false)));
    }
    assert_eq!(plan.target_priority, TargetPriority::HeavyTargets);
    assert_eq!(plan.protection, ProtectionPosture::Vanguard);
}

#[test]
fn battle_plan_tabs_highlight_hover_press_and_open_menus_without_blending_into_the_panel() {
    for label in ["Tactic", "Priority", "Protection"] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let background = context.global_style().visuals.window_fill();
        let mut plan = BattlePlan::default();
        let army = Army::from([(Unit::Ship(Ship::LightFighter), 10)]);
        let mut time = 1.0;
        let mut frame = |events| {
            time += 0.05;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 900.0),
                    )),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(640.0);
                    draw_battle_plan_editor(ui, &ImageIds::default(), &mut plan, &army);
                },
            );
            output.textures_delta.clear();
            let heading = text_rect(&output, label).expect("tab heading");
            let fill = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape) if shape.rect.contains(heading.center()) => {
                    Some(shape.fill)
                },
                _ => None,
            });
            (output, heading.center(), fill.expect("tab container"))
        };
        let (_, position, idle) = frame(vec![]);
        frame(pointer(position, None));
        let (output, _, hovered) = frame(pointer(position, None));
        assert_eq!(output.platform_output.cursor_icon, CursorIcon::PointingHand);
        let (output, _, pressed) = frame(pointer(position, Some(true)));
        assert_eq!(output.platform_output.cursor_icon, CursorIcon::PointingHand);
        assert_ne!(idle, hovered, "{label}");
        assert_ne!(hovered, pressed, "{label}");
        assert_ne!(pressed, idle, "{label}");
        assert_ne!(hovered, background, "{label}");
        assert_ne!(pressed, background, "{label}");
        frame(pointer(position, Some(false)));
        let (_, _, open) = frame(pointer(egui::pos2(990.0, 890.0), None));
        assert!(egui::Popup::is_any_open(&context), "{label}");
        assert_eq!(open, pressed, "{label} stays highlighted while its menu is open");
    }
}

#[test]
fn all_choice_lists_share_distinct_hover_and_pressed_feedback() {
    let mut colors = Vec::new();
    for choice in 0..3 {
        let context = egui::Context::default();
        let mut time = 1.0;
        let mut frame = |events| {
            time += 0.05;
            let mut rect = egui::Rect::NOTHING;
            let mut output = context.run_ui(
                egui::RawInput {
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    ui.set_width(300.0);
                    let response = match choice {
                        0 => tactic_choice(
                            ui,
                            &ImageIds::default(),
                            CombatTactic::Swarm,
                            &BattlePlan::default(),
                            &Army::new(),
                        ),
                        1 => described_option(
                            ui,
                            &ImageIds::default(),
                            target_key(TargetPriority::LightTargets),
                            TargetPriority::LightTargets.name(),
                            TargetPriority::LightTargets.description(),
                            false,
                        ),
                        _ => described_option(
                            ui,
                            &ImageIds::default(),
                            protection_key(ProtectionPosture::Bulwark),
                            ProtectionPosture::Bulwark.name(),
                            ProtectionPosture::Bulwark.description(),
                            false,
                        ),
                    };
                    rect = response.rect;
                },
            );
            output.textures_delta.clear();
            let fill = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape) if shape.rect == rect => Some(shape.fill),
                _ => None,
            });
            (rect.center(), fill.expect("choice container"))
        };
        let (position, idle) = frame(vec![]);
        frame(pointer(position, None));
        let (_, hovered) = frame(pointer(position, None));
        let (_, pressed) = frame(pointer(position, Some(true)));
        assert_ne!(idle, hovered);
        assert_ne!(hovered, pressed);
        colors.push((idle, hovered, pressed));
    }
    assert!(colors.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn priority_and_protection_descriptions_wrap_inside_their_cards() {
    for width in [240.0, 370.0] {
        for (icon, name, description) in
            TargetPriority::ALL
                .into_iter()
                .map(|priority| (target_key(priority), priority.name(), priority.description()))
                .chain(ProtectionPosture::ALL.into_iter().map(|posture| {
                    (protection_key(posture), posture.name(), posture.description())
                }))
        {
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let mut rect = egui::Rect::NOTHING;
            let mut output = context.run_ui(Default::default(), |ui| {
                ui.set_width(width);
                rect =
                    described_option(ui, &ImageIds::default(), icon, name, description, false).rect;
            });
            output.textures_delta.clear();
            let title = text_rect(&output, name).expect("option title");
            let body = text_rect(&output, description).expect("option description");
            assert!(rect.contains_rect(title), "{name} at width {width}");
            assert!(rect.contains_rect(body), "{name} at width {width}");
            assert!(body.top() >= title.bottom() + 5.0, "{name} at width {width}");
        }
    }
}

#[test]
fn four_priority_cards_are_centered_unclipped_and_clickable_without_scrollbar_space() {
    for viewport in [egui::vec2(1280.0, 720.0), egui::vec2(640.0, 480.0), egui::vec2(320.0, 568.0)]
    {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport);
        let mut plan = BattlePlan::default();
        let mut time = 1.0;
        let mut frame = |plan: &mut BattlePlan, events| {
            time += 0.05;
            let mut popup = egui::Rect::NOTHING;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    time: Some(time),
                    events,
                    ..Default::default()
                },
                |ui| {
                    let response = ui.button("Priority");
                    let popup_id = egui::Popup::default_response_id(&response);
                    egui::Popup::open_id(&context, popup_id);
                    priority_popup(
                        &response,
                        (viewport.x - 36.0).min(370.0),
                        &ImageIds::default(),
                        plan,
                    );
                    popup = context.memory(|memory| memory.area_rect(popup_id)).unwrap();
                },
            );
            output.textures_delta.clear();
            (output, popup)
        };
        frame(&mut plan, vec![]);
        frame(&mut plan, vec![]);
        let (output, popup) = frame(&mut plan, vec![]);
        let cards = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Rect(shape)
                    if [Color32::from_rgb(31, 64, 78), Color32::from_rgb(23, 32, 42)]
                        .contains(&shape.fill) =>
                {
                    Some(shape.rect)
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(cards.len(), 4, "{viewport:?}");
        assert!(screen.contains_rect(popup), "popup must fit {viewport:?}: {popup:?}");
        for (priority, card) in TargetPriority::ALL.into_iter().zip(cards) {
            assert!(
                (card.center().x - popup.center().x).abs() <= 1.0,
                "{priority:?} at {viewport:?}"
            );
            for label in [priority.name(), priority.description()] {
                let shape = output
                    .shapes
                    .iter()
                    .find(|shape| {
                        matches!(&shape.shape,
                    egui::Shape::Text(text) if text.galley.text() == label)
                    })
                    .expect(label);
                let text = text_rect(&output, label).unwrap();
                assert!(card.contains_rect(text), "{label} at {viewport:?}");
                assert!(shape.clip_rect.contains_rect(text), "clipped {label} at {viewport:?}");
            }
        }
        let heavy = text_rect(&output, TargetPriority::HeavyTargets.name()).unwrap().center();
        frame(&mut plan, pointer(heavy, None));
        frame(&mut plan, pointer(heavy, Some(true)));
        frame(&mut plan, pointer(heavy, Some(false)));
        assert_eq!(plan.target_priority, TargetPriority::HeavyTargets);
    }
}

#[test]
fn tactic_card_auto_toggle_resolves_immediately_without_opening_the_choice_list() {
    for width in [240.0, 400.0, 560.0] {
        for enabled in [false, true] {
            for click_switch in [false, true] {
                let context = egui::Context::default();
                context.set_global_style(NordDark.custom_style());
                let mut plan = BattlePlan {
                    automatic_tactic: false,
                    tactic: CombatTactic::Siege,
                    ..Default::default()
                };
                let army = Army::from([(Unit::Ship(Ship::LightFighter), 10)]);
                let mut time = 1.0;
                let mut frame = |plan: &mut BattlePlan, events| {
                    time += 0.05;
                    let mut output = context.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(1000.0, 900.0),
                            )),
                            time: Some(time),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            ui.set_width(width);
                            ui.add_enabled_ui(enabled, |ui| {
                                draw_battle_plan_editor(ui, &ImageIds::default(), plan, &army);
                            });
                        },
                    );
                    output.textures_delta.clear();
                    output
                };
                let output = frame(&mut plan, vec![]);
                let label = text_rect(&output, "Auto").expect("auto toggle on the tactic card");
                let name = text_rect(&output, "Siege").expect("selected tactic");
                assert!(label.top() > name.bottom(), "the toggle must clear the selected tactic");
                assert!(!output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().ends_with("% fit"))
                }));
                let position = if click_switch {
                    label.right_center() + egui::vec2(20.0, 0.0)
                } else {
                    label.center()
                };
                frame(&mut plan, pointer(position, None));
                frame(&mut plan, pointer(position, Some(true)));
                frame(&mut plan, pointer(position, Some(false)));
                let output = frame(&mut plan, pointer(position, None));
                assert_eq!(plan.automatic_tactic, enabled);
                assert_eq!(
                    plan.tactic,
                    if enabled {
                        CombatTactic::Swarm
                    } else {
                        CombatTactic::Siege
                    }
                );
                assert!(text_rect(&output, "Skirmish").is_none(), "toggle must not open the list");

                if enabled {
                    let card = text_rect(&output, "Swarm").unwrap().center();
                    frame(&mut plan, pointer(card, None));
                    frame(&mut plan, pointer(card, Some(true)));
                    frame(&mut plan, pointer(card, Some(false)));
                    let output = frame(&mut plan, pointer(card, None));
                    let choice = text_rect(&output, "Siege").expect("tactic choices").center();
                    frame(&mut plan, pointer(choice, None));
                    frame(&mut plan, pointer(choice, Some(true)));
                    frame(&mut plan, pointer(choice, Some(false)));
                    assert!(!plan.automatic_tactic);
                    assert_eq!(plan.tactic, CombatTactic::Siege);
                }
            }
        }
    }
}

#[test]
fn garrison_shows_only_tactic_and_opens_from_icon_number_and_percent_only_when_enabled() {
    for enabled in [false, true] {
        for target in 0..3 {
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let mut plan = BattlePlan::default();
            let army = Army::from([(Unit::Ship(Ship::LightFighter), 10)]);
            let tactic_texture = egui::TextureId::User(85);
            let priority_texture = egui::TextureId::User(86);
            let protection_texture = egui::TextureId::User(87);
            let images = ImageIds(HashMap::from([
                (tactic_key(plan.resolved(&army).tactic).to_owned(), tactic_texture),
                (target_key(plan.target_priority).to_owned(), priority_texture),
                (protection_key(plan.protection).to_owned(), protection_texture),
            ]));
            let label = format!("{}%", plan.resolved(&army).suitability(&army));
            let icon = egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(20.0, 20.0));
            let mut time = 1.0;
            let mut frame = |events| {
                time += 0.05;
                let mut open = false;
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 900.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        ui.add_enabled_ui(enabled, |ui| {
                            open = draw_tactic_button_at(
                                ui,
                                &images,
                                icon,
                                &mut plan,
                                &army,
                                WORLD_OVERVIEW_STRENGTH_FONT_SIZE,
                            );
                        });
                    },
                );
                output.textures_delta.clear();
                (output, open)
            };
            let (output, _) = frame(vec![]);
            let textures = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) => Some(mesh.texture_id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(textures.iter().filter(|texture| **texture == tactic_texture).count(), 1);
            assert!(!textures.contains(&priority_texture));
            assert!(!textures.contains(&protection_texture));
            let percentage = text_rect(&output, &label).expect("tactic affinity percentage");
            assert!((percentage.left() - icon.right() - TACTIC_AFFINITY_TEXT_GAP).abs() < 0.5);
            let position = match target {
                0 => icon.center(),
                1 => percentage.left_center() + egui::vec2(4.0, 0.0),
                _ => percentage.right_center() - egui::vec2(4.0, 0.0),
            };
            frame(pointer(position, None));
            let (output, open) = frame(pointer(position, None));
            assert_eq!(open, enabled, "hover target {target}");
            assert_eq!(output.platform_output.cursor_icon, CursorIcon::Default);
            assert_eq!(text_rect(&output, "Tactic").is_some(), enabled, "hover target {target}");
        }
    }
}

#[test]
fn active_mission_plan_follows_objective_and_opens_from_each_icon_and_percentage() {
    let objective_texture = egui::TextureId::User(81);
    let priority_texture = egui::TextureId::User(82);
    let protection_texture = egui::TextureId::User(83);
    let tactic_texture = egui::TextureId::User(84);
    let images = ImageIds(HashMap::from([
        ("attack".to_owned(), objective_texture),
        ("target standard".to_owned(), priority_texture),
        ("protection standard".to_owned(), protection_texture),
        ("tactic swarm".to_owned(), tactic_texture),
    ]));
    for enabled in [false, true] {
        for target in 0..4 {
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let army = Army::from([(Unit::Ship(Ship::LightFighter), 10)]);
            let mut plan = BattlePlan {
                automatic_tactic: false,
                tactic: CombatTactic::Swarm,
                ..Default::default()
            };
            let mut time = 1.0;
            let mut objective_rect = egui::Rect::NOTHING;
            let mut frame = |events| {
                time += 0.05;
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(1000.0, 900.0),
                        )),
                        time: Some(time),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        ui.horizontal(|ui| {
                            objective_rect = ui.add_image(objective_texture, [25.0; 2]).rect;
                            ui.add_enabled_ui(enabled, |ui| {
                                draw_mission_plan_button(ui, &images, &mut plan, &army);
                            });
                        });
                    },
                );
                output.textures_delta.clear();
                (output, objective_rect)
            };
            frame(vec![]);
            let (output, objective) = frame(vec![]);
            let icon_rect = |texture| {
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
            let icons = [priority_texture, protection_texture, tactic_texture].map(icon_rect);
            assert!(objective.right() < icons[0].left());
            assert!(icons.windows(2).all(|pair| pair[0].right() < pair[1].left()));
            let percentage = text_rect(&output, "100%").unwrap();
            assert!(icons[2].right() < percentage.left());
            let position = if target < 3 {
                icons[target].center()
            } else {
                percentage.center()
            };
            frame(pointer(position, None));
            let (output, _) = frame(pointer(position, None));
            assert_eq!(text_rect(&output, "Priority").is_some(), enabled, "target {target}");
            assert_eq!(text_rect(&output, "Protection").is_some(), enabled, "target {target}");
            assert_eq!(text_rect(&output, "Tactic").is_some(), enabled, "target {target}");
        }
    }
}

#[test]
fn disabled_battle_plan_icon_cannot_open_editing_controls() {
    let context = egui::Context::default();
    let mut plan = BattlePlan::default();
    let mut output = context.run_ui(
        egui::RawInput {
            events: pointer(egui::pos2(15.0, 15.0), None),
            ..Default::default()
        },
        |ui| {
            ui.add_enabled_ui(false, |ui| {
                draw_mission_plan_button(ui, &ImageIds::default(), &mut plan, &Army::new());
            });
        },
    );
    output.textures_delta.clear();
    assert!(text_rect(&output, "Tactic").is_none());
}

#[test]
fn battle_plan_cards_and_mission_summary_share_priority_protection_tactic_order() {
    for width in [240.0, 400.0, 560.0] {
        for enabled in [false, true] {
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let army = Army::from([(Unit::Ship(Ship::LightFighter), 10)]);
            let mut plan = BattlePlan::default();
            let mut output = context.run_ui(Default::default(), |ui| {
                ui.set_width(width);
                ui.add_enabled_ui(enabled, |ui| {
                    draw_battle_plan_editor(ui, &ImageIds::default(), &mut plan, &army);
                });
            });
            output.textures_delta.clear();
            let priority = text_rect(&output, "Priority").unwrap();
            let protection = text_rect(&output, "Protection").unwrap();
            let tactic = text_rect(&output, "Tactic").unwrap();
            assert!(priority.right() < protection.left(), "width {width}");
            assert!(protection.right() < tactic.left(), "width {width}");
            assert!((priority.center().y - tactic.center().y).abs() < 0.5);
        }
    }

    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let army = Army::from([(Unit::Ship(Ship::LightFighter), 10)]);
    let plan = BattlePlan::default().resolved(&army);
    let mut output = context.run_ui(Default::default(), |ui| {
        draw_mission_plan_summary(ui, &ImageIds::default(), plan, &army);
    });
    output.textures_delta.clear();
    let priority = text_rect(&output, plan.target_priority.name()).unwrap();
    let protection = text_rect(&output, plan.protection.name()).unwrap();
    let tactic = text_rect(&output, plan.tactic.name()).unwrap();
    assert!(priority.bottom() < protection.top());
    assert!(protection.bottom() < tactic.top());
    let fit = plan.suitability(&army);
    let percentage = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == format!("({fit}%)") => Some(text),
            _ => None,
        })
        .unwrap();
    let job = &percentage.galley.job;
    assert_eq!(job.text, format!("({fit}%)"));
    assert!(job.sections.iter().all(|section| section.format.color == affinity_color(fit)));
}

/// Uses the same cards and rows as the live popups to inspect spacing and icon legibility.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires a GPU; writes battle-plan UI previews under target"]
fn render_battle_plan_controls() {
    use bevy::app::AppExit;
    use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
    use bevy::winit::{WinitPlugin, WinitSettings};
    use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
    std::fs::create_dir_all("target/ui-battle-plan").unwrap();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Battle plan preview".into(),
                    resolution: (1060, 880).into(),
                    visible: false,
                    ..default()
                }),
                ..default()
            })
            .set(WinitPlugin {
                run_on_any_thread: true,
            }),
    )
    .add_plugins(EguiPlugin::default())
    .insert_resource(WinitSettings::continuous())
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn(Camera2d);
    })
    .add_systems(
        EguiPrimaryContextPass,
        (
            set_ui_style,
            |mut contexts: EguiContexts,
             mut frame: Local<usize>,
             mut commands: Commands,
             mut handles: Local<Vec<egui::TextureHandle>>| {
                let Ok(context) = contexts.ctx_mut() else {
                    return;
                };
                if handles.is_empty() {
                    for key in [
                        "battle plan",
                        "attack",
                        "fleet",
                        "defenses",
                        "protect",
                        "tactic swarm",
                        "tactic skirmish",
                        "tactic breakthrough",
                        "tactic siege",
                        "tactic interception",
                        "tactic bastion",
                        "target standard",
                        "target small ships",
                        "target medium ships",
                        "target capital ships",
                        "protection standard",
                        "protection screen capitals",
                        "protection heavy cover",
                        "protection protect bombers",
                    ] {
                        let path = format!(
                            "{}/assets/images/mission/{key}.png",
                            env!("CARGO_MANIFEST_DIR")
                        );
                        let pixels = image::open(path).unwrap().into_rgba8();
                        handles.push(context.load_texture(
                            key,
                            egui::ColorImage::from_rgba_unmultiplied(
                                [pixels.width() as usize, pixels.height() as usize],
                                pixels.as_raw(),
                            ),
                            egui::TextureOptions::LINEAR,
                        ));
                    }
                }
                let images = ImageIds(
                    handles.iter().map(|handle| (handle.name().to_owned(), handle.id())).collect(),
                );
                egui::Area::new(egui::Id::new("battle plan preview"))
                    .fixed_pos(egui::Pos2::ZERO)
                    .show(context, |ui| {
                        ui.set_width(1040.0);
                        let army = Army::from([
                            (Unit::Ship(Ship::LightFighter), 10),
                            (Unit::Ship(Ship::Cruiser), 2),
                        ]);
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(560.0, 420.0),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    ui.small("Planet fleet header");
                                    let mut plan = BattlePlan::default().resolved(&army);
                                    let plan_width = tactic_badge_width(
                                        ui,
                                        WORLD_OVERVIEW_BATTLE_PLAN_ICON_SIZE,
                                        plan,
                                        &army,
                                        WORLD_OVERVIEW_STRENGTH_FONT_SIZE,
                                    );
                                    for width in [270.0, 145.0] {
                                        ui.allocate_ui_with_layout(
                                            egui::vec2(width, 24.0),
                                            Layout::top_down(Align::Min),
                                            |ui| {
                                                let (_, _, icon) = draw_world_overview_header(
                                                    ui,
                                                    "20",
                                                    (width > 145.0).then_some("18"),
                                                    &images,
                                                    width > 145.0,
                                                    Some(plan_width),
                                                );
                                                draw_tactic_button_at(
                                                    ui,
                                                    &images,
                                                    icon.unwrap(),
                                                    &mut plan,
                                                    &army,
                                                    WORLD_OVERVIEW_STRENGTH_FONT_SIZE,
                                                );
                                            },
                                        );
                                    }
                                    ui.small("Active mission orders");
                                    ui.horizontal(|ui| {
                                        ui.add_image(images.get("attack"), [25.0; 2]);
                                        draw_mission_plan_button(ui, &images, &mut plan, &army);
                                    });
                                    draw_battle_plan_editor(
                                        ui,
                                        &images,
                                        &mut BattlePlan {
                                            automatic_tactic: *frame < 50,
                                            ..Default::default()
                                        },
                                        &army,
                                    );
                                    ui.separator();
                                    draw_mission_plan_summary(
                                        ui,
                                        &images,
                                        BattlePlan::default(),
                                        &army,
                                    );
                                },
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(300.0, 800.0),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    let heading = ui.label(if *frame < 50 {
                                        "Tactics"
                                    } else if *frame < 90 {
                                        "Target priority"
                                    } else {
                                        "Protection"
                                    });
                                    egui::Popup::open_id(
                                        context,
                                        egui::Popup::default_response_id(&heading),
                                    );
                                    let width = if *frame < 50 {
                                        300.0
                                    } else {
                                        370.0
                                    };
                                    if (50..90).contains(&*frame) {
                                        priority_popup(
                                            &heading,
                                            width,
                                            &images,
                                            &mut BattlePlan::default(),
                                        );
                                    } else {
                                        choice_popup(&heading, width, 280.0, *frame < 50, |ui| {
                                            if *frame < 50 {
                                                for tactic in CombatTactic::ALL {
                                                    tactic_choice(
                                                        ui,
                                                        &images,
                                                        tactic,
                                                        &BattlePlan::default().resolved(&army),
                                                        &army,
                                                    );
                                                }
                                            } else {
                                                for protection in ProtectionPosture::ALL {
                                                    described_option(
                                                        ui,
                                                        &images,
                                                        protection_key(protection),
                                                        protection.name(),
                                                        protection.description(),
                                                        protection == ProtectionPosture::Standard,
                                                    );
                                                }
                                            }
                                        });
                                    }
                                },
                            );
                        });
                    });
                *frame += 1;
                if [40, 80, 120].contains(&*frame) {
                    let path = match *frame {
                        40 => "target/ui-battle-plan/selector.png",
                        80 => "target/ui-battle-plan/target-priority.png",
                        _ => "target/ui-battle-plan/protection.png",
                    };
                    let mut capture = commands.spawn(Screenshot::primary_window());
                    capture.observe(save_to_disk(path));
                    if *frame == 120 {
                        capture.observe(
                            |_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                                exit.write(AppExit::Success);
                            },
                        );
                    }
                }
            },
        )
            .chain(),
    );
    app.run();
}
