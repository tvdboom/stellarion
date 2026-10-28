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

    for (heading, option) in [("Priority", "Capital ships"), ("Protection", "Screen capitals")] {
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
    assert_eq!(plan.target_priority, TargetPriority::CapitalShips);
    assert_eq!(plan.protection, ProtectionPosture::ScreenCapitals);
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
                    resolution: (820, 880).into(),
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
                        "target defenses",
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
                        ui.set_width(800.0);
                        let army = Army::from([
                            (Unit::Ship(Ship::LightFighter), 10),
                            (Unit::Ship(Ship::Cruiser), 2),
                        ]);
                        ui.horizontal_top(|ui| {
                            ui.allocate_ui_with_layout(
                                egui::vec2(330.0, 420.0),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    ui.small("Planet fleet header");
                                    let (_, _, icon) = draw_world_overview_header(
                                        ui,
                                        "20",
                                        Some("18"),
                                        &images,
                                        true,
                                        true,
                                    );
                                    let rect = icon.unwrap();
                                    let plan = BattlePlan::default().resolved(&army);
                                    paint_option_icon(ui, &images, tactic_key(plan.tactic), rect);
                                    paint_tactic_affinity(ui, rect, plan, &army);
                                    draw_battle_plan_editor(
                                        ui,
                                        &images,
                                        &mut BattlePlan::default(),
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
                                egui::vec2(400.0, 800.0),
                                Layout::top_down(Align::Min),
                                |ui| {
                                    if *frame < 50 {
                                        ui.label("Tactics");
                                        for tactic in CombatTactic::ALL {
                                            tactic_choice(
                                                ui,
                                                &images,
                                                tactic,
                                                &BattlePlan::default().resolved(&army),
                                                &army,
                                            );
                                        }
                                    } else if *frame < 90 {
                                        ui.label("Target priority");
                                        for target in TargetPriority::ALL {
                                            described_option(
                                                ui,
                                                &images,
                                                target_key(target),
                                                target.name(),
                                                target.description(),
                                                target == TargetPriority::Standard,
                                            );
                                        }
                                    } else {
                                        ui.label("Protection");
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
