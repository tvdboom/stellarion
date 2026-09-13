use super::*;
use crate::core::units::operations::{MineMode, SenatePolicy, SenateSupport, SpaceDockMode};

fn has_text_fragment(shapes: &[egui::epaint::ClippedShape], text: &str) -> bool {
    shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(label) if label.galley.job.text.contains(text))
    })
}

fn mode_images() -> ImageIds {
    ImageIds(HashMap::from([
        ("mine normal".into(), egui::TextureId::User(1)),
        ("mine intensive".into(), egui::TextureId::User(2)),
        ("mine suspended".into(), egui::TextureId::User(3)),
        ("no focus".into(), egui::TextureId::User(4)),
        ("metal".into(), egui::TextureId::User(5)),
        ("crystal".into(), egui::TextureId::User(6)),
        ("deuterium".into(), egui::TextureId::User(7)),
        ("dock industrial".into(), egui::TextureId::User(8)),
        ("dock bastion".into(), egui::TextureId::User(9)),
        ("senate expansion".into(), egui::TextureId::User(10)),
        ("senate consolidation".into(), egui::TextureId::User(11)),
    ]))
}

fn click_tile(texture: u64, mut draw: impl FnMut(&mut Ui)) {
    let context = egui::Context::default();
    let mut frame = |events| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500., 300.),
                )),
                events,
                ..default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| draw(ui));
            },
        );
        output.textures_delta.clear();
        output
    };
    frame(vec![]);
    let output = frame(vec![]);
    let position = image_rect(&output.shapes, egui::TextureId::User(texture)).unwrap().center();
    frame(vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: default(),
        },
    ]);
    frame(vec![egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: default(),
    }]);
}

#[test]
fn extraction_tiles_emit_orders_and_cannot_override_recovery() {
    let mut planet = Planet::new(1, "Mine".into(), Vec2::ZERO, false, 1.);
    planet.army.insert(Unit::Building(Building::MetalMine), 2);
    let mut pending = PendingTurnCommands::default();
    let images = mode_images();
    click_tile(2, |ui| {
        shop::draw_mine_mode(ui, &mut planet, ResourceName::Metal, &mut pending, &images)
    });
    assert_eq!(planet.operations.mine(ResourceName::Metal).mode, MineMode::Intensive);
    assert!(matches!(
        pending.commands.as_slice(),
        [TurnCommand::SetMineMode {
            mode: MineMode::Intensive,
            resource: ResourceName::Metal,
            ..
        }]
    ));
    planet.operations.mine_mut(ResourceName::Metal).finish_turn();
    pending.commands.clear();
    click_tile(1, |ui| {
        shop::draw_mine_mode(ui, &mut planet, ResourceName::Metal, &mut pending, &images)
    });
    assert!(pending.commands.is_empty());
    assert_eq!(planet.operations.mine(ResourceName::Metal).mode, MineMode::Suspended);
}

#[test]
fn recycler_tiles_select_and_return_to_bulk() {
    let mut planet = Planet::new(1, "Recycler".into(), Vec2::ZERO, false, 1.);
    planet.army.insert(Unit::Building(Building::Recycler), 1);
    let mut pending = PendingTurnCommands::default();
    let images = mode_images();
    click_tile(6, |ui| shop::draw_recycler_focus(ui, &mut planet, &mut pending, &images));
    assert_eq!(planet.operations.recycler_focus, Some(ResourceName::Crystal));
    click_tile(4, |ui| shop::draw_recycler_focus(ui, &mut planet, &mut pending, &images));
    assert_eq!(planet.operations.recycler_focus, None);
    assert_eq!(pending.commands.len(), 2);
}

#[test]
fn jump_energy_display_tracks_the_toggle_and_selected_fleet() {
    let context = egui::Context::default();
    let mut mission = Mission::default();
    for (enabled, count, cost) in [(false, 1, 0), (true, 1, 1), (true, 6, 2), (false, 6, 0)] {
        mission.jump_gate = enabled;
        mission.army = Army::from([(Unit::Ship(Ship::LightFighter), count)]);
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            missions::draw_jump_energy_cost(ui, &mission, &ImageIds::default())
        });
        output.textures_delta.clear();
        assert!(!has_text_fragment(&output.shapes, "Jump Energy:"));
        if enabled {
            assert!(has_text(&output.shapes, &cost.to_string()));
        } else {
            assert!(!has_text(&output.shapes, &cost.to_string()));
        }
    }
}

#[test]
fn dock_tiles_enforce_commitment_and_queued_production() {
    let mut planet = Planet::new(1, "Dock".into(), Vec2::ZERO, false, 1.);
    planet.army.insert(Unit::space_dock(), 1);
    let mut pending = PendingTurnCommands::default();
    let images = mode_images();
    planet.buy.push(Unit::Ship(Ship::LightFighter));
    click_tile(9, |ui| {
        shop::draw_space_dock_mode(ui, &mut planet, &mut pending, &images, Default::default())
    });
    assert!(pending.commands.is_empty());
    planet.buy.clear();
    click_tile(9, |ui| {
        shop::draw_space_dock_mode(ui, &mut planet, &mut pending, &images, Default::default())
    });
    assert_eq!(planet.operations.space_dock, SpaceDockMode::Bastion);
    assert!(planet.operations.space_dock_selection_pending);
    assert_eq!(planet.operations.space_dock_locked_until, 0);
    click_tile(8, |ui| {
        shop::draw_space_dock_mode(ui, &mut planet, &mut pending, &images, Default::default())
    });
    assert_eq!(planet.operations.space_dock, SpaceDockMode::Industrial);
    click_tile(9, |ui| {
        shop::draw_space_dock_mode(ui, &mut planet, &mut pending, &images, Default::default())
    });
    assert_eq!(planet.operations.space_dock, SpaceDockMode::Bastion);
    assert_eq!(pending.commands.len(), 3);
    // The simulation commits the final choice when it advances to the next planning turn.
    pending.turn += 1;
    planet.operations.space_dock_locked_until = pending.turn + 3;
    planet.operations.space_dock_selection_pending = false;
    pending.commands.clear();
    for _ in 0..3 {
        click_tile(8, |ui| {
            shop::draw_space_dock_mode(ui, &mut planet, &mut pending, &images, Default::default())
        });
        assert!(pending.commands.is_empty());
        pending.turn += 1;
    }
    click_tile(8, |ui| {
        shop::draw_space_dock_mode(ui, &mut planet, &mut pending, &images, Default::default())
    });
    assert_eq!(planet.operations.space_dock, SpaceDockMode::Industrial);
}

#[test]
fn senate_tiles_enforce_empire_queues_and_commitment_without_locking_a_draft() {
    let mut planet = Planet::new(1, "Senate".into(), Vec2::ZERO, false, 1.);
    planet.owned = Some(1);
    planet.army.insert(Unit::Building(Building::Senate), 3);
    let mut senate = SenateSupport {
        owner: 1,
        level: 3,
        policy: SenatePolicy::Expansion,
    };
    let mut pending = PendingTurnCommands::default();
    let images = mode_images();
    // A remote colony can prevent switching even though this planet's queue is empty.
    click_tile(11, |ui| {
        shop::draw_senate_policy(ui, &mut planet, &mut pending, &images, &mut senate, [true, false])
    });
    assert!(pending.commands.is_empty());
    for texture in [11, 10, 11] {
        click_tile(texture, |ui| {
            shop::draw_senate_policy(ui, &mut planet, &mut pending, &images, &mut senate, [true; 2])
        });
    }
    assert_eq!(pending.commands.len(), 3);
    assert_eq!(planet.operations.senate, SenatePolicy::Consolidation);
    assert_eq!(senate.policy, SenatePolicy::Consolidation);
    assert!(planet.operations.senate_selection_pending);
    assert_eq!(planet.operations.senate_locked_until, 0);
    assert_eq!(
        shop::shop_capacity_summary(Shop::Defenses, &planet, senate),
        Some(("Production", 0, 6))
    );
    assert_eq!(
        shop::shop_capacity_summary(Shop::Fleet, &planet, senate),
        Some(("Production", 0, 0))
    );
    planet.operations.senate_locked_until = pending.turn + 3;
    planet.operations.senate_selection_pending = false;
    pending.commands.clear();
    click_tile(10, |ui| {
        shop::draw_senate_policy(ui, &mut planet, &mut pending, &images, &mut senate, [true; 2])
    });
    assert!(pending.commands.is_empty());
    pending.turn += 3;
    click_tile(10, |ui| {
        shop::draw_senate_policy(ui, &mut planet, &mut pending, &images, &mut senate, [true; 2])
    });
    assert_eq!(senate.policy, SenatePolicy::Expansion);
}

#[test]
fn operating_details_appear_only_on_hover_including_disabled_choices() {
    for (texture, recovering, locked, queued_ship, detail, state) in [
        (1, false, false, false, "Normal: 100% output", "1 energy per level."),
        (
            2,
            false,
            false,
            false,
            "Intensive: 150% output at 3 energy per level.",
            "Automatically suspended next turn.",
        ),
        (
            2,
            true,
            false,
            false,
            "Intensive: 150% output",
            "Recovery: suspended for this entire turn.",
        ),
        (
            3,
            true,
            false,
            false,
            "Suspended: 0% output at no energy cost.",
            "Recovery: suspended for this entire turn.",
        ),
        (4, false, false, false, "Bulk: recover the normal mixture", "all three resources"),
        (6, false, false, false, "Selective Crystal: recover 150%", "no other resources"),
        (9, false, false, false, "Bastion: +50% combat strength.", "fixed for the next 3 turns"),
        (
            10,
            false,
            false,
            false,
            "Expansion: +2 fleet production",
            "per completed Senate level on every planet.",
        ),
        (
            11,
            false,
            false,
            false,
            "Consolidation: +2 defense production",
            "The final selection becomes fixed for the next 3 turns.",
        ),
        (10, false, true, false, "Expansion: +2 fleet production", "Committed for 3 more turns."),
        (
            11,
            false,
            false,
            true,
            "Consolidation: +2 defense production",
            "Queued units on a planet require the current Senate production bonus.",
        ),
        (9, false, true, false, "Bastion: +50% combat strength.", "Committed for 3 more turn(s)."),
        (8, false, true, false, "Industrial: +5 fleet production", "Committed for 3 more turn(s)."),
        (
            9,
            false,
            false,
            true,
            "Bastion: +50% combat strength.",
            "Queued ships require Industrial production.",
        ),
    ] {
        let context = egui::Context::default();
        let mut style = NordDark.custom_style();
        style.interaction.tooltip_delay = 0.;
        style.interaction.show_tooltips_only_when_still = false;
        context.set_global_style(style);
        let mut planet = Planet::new(1, "Industry".into(), Vec2::ZERO, false, 1.);
        planet.army.insert(Unit::Building(Building::MetalMine), 2);
        planet.army.insert(Unit::Building(Building::Recycler), 1);
        planet.army.insert(Unit::Building(Building::Senate), 3);
        planet.army.insert(Unit::space_dock(), 1);
        planet.buy.push(Unit::Building(Building::MetalMine));
        if recovering {
            let mine = planet.operations.mine_mut(ResourceName::Metal);
            mine.mode = MineMode::Suspended;
            mine.recovering = true;
        }
        let mut pending = PendingTurnCommands::default();
        let mut senate = SenateSupport {
            owner: 1,
            level: 3,
            policy: SenatePolicy::Expansion,
        };
        if locked {
            planet.operations.space_dock = SpaceDockMode::Bastion;
            planet.operations.space_dock_locked_until = pending.turn + 3;
            planet.operations.senate = SenatePolicy::Consolidation;
            planet.operations.senate_locked_until = pending.turn + 3;
            senate.policy = SenatePolicy::Consolidation;
        }
        if queued_ship {
            planet.buy.push(Unit::Ship(Ship::LightFighter));
        }
        let images = mode_images();
        let mut frame = |events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(500., 400.),
                    )),
                    events,
                    ..default()
                },
                |ui| match texture {
                    1..=3 => shop::draw_mine_mode(
                        ui,
                        &mut planet,
                        ResourceName::Metal,
                        &mut pending,
                        &images,
                    ),
                    4..=7 => shop::draw_recycler_focus(ui, &mut planet, &mut pending, &images),
                    8..=9 => shop::draw_space_dock_mode(
                        ui,
                        &mut planet,
                        &mut pending,
                        &images,
                        Default::default(),
                    ),
                    _ => shop::draw_senate_policy(
                        ui,
                        &mut planet,
                        &mut pending,
                        &images,
                        &mut senate,
                        [true, !queued_ship],
                    ),
                },
            );
            output.textures_delta.clear();
            output
        };
        let resting = frame(vec![]);
        for text in [
            detail,
            state,
            "energy",
            "combat strength",
            "Selective",
            "Committed",
            "Recovery:",
            "production",
        ] {
            assert!(
                !has_text_fragment(&resting.shapes, text),
                "details displayed without hover: {text}"
            );
        }
        let target = image_rect(&resting.shapes, egui::TextureId::User(texture)).unwrap();
        frame(vec![egui::Event::PointerMoved(target.center())]);
        let hovered = frame(vec![]);
        assert!(
            has_text_fragment(&hovered.shapes, detail),
            "missing mode detail for texture {texture}: {detail}"
        );
        assert!(
            has_text_fragment(&hovered.shapes, state),
            "missing mode state for texture {texture}: {state}"
        );
    }
}

#[test]
fn operating_choices_wrap_inside_small_panels() {
    for width in [220., 280., 480.] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut planet = Planet::new(1, "Industry".into(), Vec2::ZERO, false, 1.);
        for unit in [
            Unit::Building(Building::MetalMine),
            Unit::Building(Building::Recycler),
            Unit::Building(Building::Senate),
            Unit::space_dock(),
        ] {
            planet.army.insert(unit, 1);
        }
        let mut pending = PendingTurnCommands::default();
        let images = mode_images();
        let mut bounds = egui::Rect::NOTHING;
        let mut senate = SenateSupport {
            owner: 1,
            level: 1,
            policy: SenatePolicy::Expansion,
        };
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(width, 850.),
                )),
                ..default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    bounds = ui
                        .scope(|ui| {
                            shop::draw_mine_mode(
                                ui,
                                &mut planet,
                                ResourceName::Metal,
                                &mut pending,
                                &images,
                            );
                            shop::draw_recycler_focus(ui, &mut planet, &mut pending, &images);
                            shop::draw_space_dock_mode(
                                ui,
                                &mut planet,
                                &mut pending,
                                &images,
                                Default::default(),
                            );
                            shop::draw_senate_policy(
                                ui,
                                &mut planet,
                                &mut pending,
                                &images,
                                &mut senate,
                                [true; 2],
                            );
                        })
                        .response
                        .rect;
                });
            },
        );
        output.textures_delta.clear();
        assert!(bounds.right() <= width, "operating choices exceed {width}: {bounds:?}");
        for texture in 1..=11 {
            assert!(image_rect(&output.shapes, egui::TextureId::User(texture)).is_some());
        }
    }
}

/// Renders the real egui controls with their source artwork for visual inspection.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires a GPU; writes target/ui-operations/choices.png"]
fn render_operating_choices() {
    use bevy::app::AppExit;
    use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
    use bevy::winit::{WinitPlugin, WinitSettings};
    use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
    let mut app = App::new();
    std::fs::create_dir_all("target/ui-operations").unwrap();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Operating choices preview".into(),
                    resolution: (900, 900).into(),
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
             mut commands: Commands,
             mut frame: Local<usize>,
             mut handles: Local<Vec<egui::TextureHandle>>,
             mut ids: Local<ImageIds>| {
                let Ok(ctx) = contexts.ctx_mut() else {
                    return;
                };
                if handles.is_empty() {
                    for name in [
                        "mine normal",
                        "mine intensive",
                        "mine suspended",
                        "dock industrial",
                        "dock bastion",
                        "senate expansion",
                        "senate consolidation",
                        "no focus",
                        "metal",
                        "crystal",
                        "deuterium",
                        "energy",
                    ] {
                        let pixels = image::open(format!("assets/images/resources/{name}.png"))
                            .unwrap()
                            .into_rgba8();
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [pixels.width() as usize, pixels.height() as usize],
                            pixels.as_raw(),
                        );
                        let handle = ctx.load_texture(name, image, egui::TextureOptions::LINEAR);
                        ids.0.insert(name.into(), handle.id());
                        handles.push(handle);
                    }
                }
                let mut planet = Planet::new(1, "Industry".into(), Vec2::ZERO, false, 1.);
                planet.owned = Some(1);
                for unit in [
                    Unit::Building(Building::MetalMine),
                    Unit::Building(Building::Recycler),
                    Unit::Building(Building::Senate),
                    Unit::space_dock(),
                ] {
                    planet.army.insert(unit, 3);
                }
                let mut pending = PendingTurnCommands::default();
                let mut senate = SenateSupport {
                    owner: 1,
                    level: 3,
                    policy: SenatePolicy::Expansion,
                };
                egui::Area::new("operating choices".into()).fixed_pos(egui::pos2(16., 16.)).show(
                    ctx,
                    |ui| {
                        ui.set_width(860.);
                        ui.add_space(14.);
                        ui.heading("Building operations");
                        ui.add_space(18.);
                        ui.columns(2, |columns| {
                            let ui = &mut columns[0];
                            shop::draw_mine_mode(
                                ui,
                                &mut planet,
                                ResourceName::Metal,
                                &mut pending,
                                &ids,
                            );
                            ui.add_space(18.);
                            planet.operations.mine_mut(ResourceName::Metal).mode =
                                MineMode::Intensive;
                            shop::draw_mine_mode(
                                ui,
                                &mut planet,
                                ResourceName::Metal,
                                &mut pending,
                                &ids,
                            );
                            ui.add_space(18.);
                            planet.operations.mine_mut(ResourceName::Metal).finish_turn();
                            shop::draw_mine_mode(
                                ui,
                                &mut planet,
                                ResourceName::Metal,
                                &mut pending,
                                &ids,
                            );
                            ui.add_space(18.);
                            ui.small("Jump Gate mission · 6 production");
                            missions::draw_jump_energy_cost(
                                ui,
                                &Mission {
                                    jump_gate: true,
                                    army: Army::from([(Unit::Ship(Ship::LightFighter), 6)]),
                                    ..default()
                                },
                                &ids,
                            );
                            ui.add_space(18.);
                            shop::draw_senate_policy(
                                ui,
                                &mut planet,
                                &mut pending,
                                &ids,
                                &mut senate,
                                [true; 2],
                            );
                            ui.add_space(18.);
                            planet.operations.senate = SenatePolicy::Consolidation;
                            planet.operations.senate_locked_until = pending.turn + 3;
                            senate.policy = SenatePolicy::Consolidation;
                            shop::draw_senate_policy(
                                ui,
                                &mut planet,
                                &mut pending,
                                &ids,
                                &mut senate,
                                [true; 2],
                            );
                            let ui = &mut columns[1];
                            shop::draw_recycler_focus(ui, &mut planet, &mut pending, &ids);
                            ui.add_space(18.);
                            planet.operations.recycler_focus = Some(ResourceName::Crystal);
                            shop::draw_recycler_focus(ui, &mut planet, &mut pending, &ids);
                            ui.add_space(18.);
                            shop::draw_space_dock_mode(
                                ui,
                                &mut planet,
                                &mut pending,
                                &ids,
                                Default::default(),
                            );
                            ui.add_space(18.);
                            planet.operations.space_dock = SpaceDockMode::Bastion;
                            planet.operations.space_dock_locked_until = pending.turn + 3;
                            shop::draw_space_dock_mode(
                                ui,
                                &mut planet,
                                &mut pending,
                                &ids,
                                Default::default(),
                            );
                        });
                    },
                );
                *frame += 1;
                if *frame == 60 {
                    commands
                        .spawn(Screenshot::primary_window())
                        .observe(save_to_disk("target/ui-operations/choices.png"))
                        .observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                            exit.write(AppExit::Success);
                        });
                }
            },
        )
            .chain(),
    );
    app.run();
}
