use super::*;

fn context() -> egui::Context {
    let context = egui::Context::default();
    let mut style = NordDark.custom_style();
    style.interaction.tooltip_delay = 0.0;
    context.set_global_style(style);
    context.add_font(FontInsert::new(
        "firasans",
        FontData::from_static(include_bytes!("../../assets/fonts/FiraSans-Bold.ttf")),
        vec![InsertFontFamily {
            family: FontFamily::Proportional,
            priority: FontPriority::Highest,
        }],
    ));
    context
}

fn text_shapes(output: &egui::FullOutput) -> impl Iterator<Item = &egui::epaint::TextShape> {
    output.shapes.iter().filter_map(|shape| match &shape.shape {
        egui::Shape::Text(text) => Some(text),
        _ => None,
    })
}

fn text_rect(output: &egui::FullOutput, label: &str) -> egui::Rect {
    let text = text_shapes(output).find(|text| text.galley.text() == label).expect(label);
    text.galley.rect.translate(text.pos.to_vec2())
}

fn image_rect(output: &egui::FullOutput, texture: egui::TextureId) -> egui::Rect {
    fn find(shape: &egui::Shape, texture: egui::TextureId) -> Option<egui::Rect> {
        match shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == texture => Some(mesh.calc_bounds()),
            egui::Shape::Rect(rect)
                if rect.brush.is_some() && rect.fill_texture_id() == texture =>
            {
                Some(rect.rect)
            },
            egui::Shape::Vec(shapes) => shapes.iter().find_map(|shape| find(shape, texture)),
            _ => None,
        }
    }
    output.shapes.iter().find_map(|shape| find(&shape.shape, texture)).expect("affinity image")
}

#[test]
fn affinity_section_excludes_noncombat_units_but_includes_armed_defenses_and_docks() {
    for (unit, visible) in [
        (Unit::Ship(Ship::LightFighter), true),
        (Unit::Ship(Ship::WarSun), true),
        (Unit::Defense(Defense::RocketLauncher), true),
        (Unit::Defense(Defense::PlasmaTurret), true),
        (Unit::space_dock(), true),
        (Unit::Ship(Ship::Probe), false),
        (Unit::Ship(Ship::ColonyShip), false),
        (Unit::Defense(Defense::Crawler), false),
        (Unit::Defense(Defense::RepairTruck), false),
        (Unit::Defense(Defense::AntiballisticMissile), false),
        (Unit::Defense(Defense::InterplanetaryMissile), false),
        (Unit::Building(Building::MetalMine), false),
    ] {
        let mut output = context().run_ui(Default::default(), |ui| {
            ui.set_width(482.0);
            draw_unit_tactic_affinities(ui, unit, &ImageIds::default());
        });
        output.textures_delta.clear();
        assert_eq!(
            text_shapes(&output).any(|text| text.galley.text() == TACTIC_AFFINITY_TITLE),
            visible,
            "{unit:?}"
        );
        if !visible {
            assert!(output.shapes.is_empty(), "excluded units must not leave a separator");
        }
    }
}

#[test]
fn six_affinities_match_the_model_and_stay_in_one_row_at_compact_widths() {
    for unit in Unit::iter()
        .filter(|unit| CombatTactic::ALL.into_iter().any(|tactic| tactic.affinity(*unit) > 0))
    {
        for width in [240.0, 300.0, 482.0] {
            let context = context();
            let images = ImageIds(
                CombatTactic::ALL
                    .into_iter()
                    .enumerate()
                    .map(|(index, tactic)| {
                        (tactic.image_key().to_owned(), egui::TextureId::User(index as u64 + 100))
                    })
                    .collect(),
            );
            let mut bounds = egui::Rect::NOTHING;
            let mut output = context.run_ui(Default::default(), |ui| {
                ui.set_width(width);
                draw_unit_tactic_affinities(ui, unit, &images);
                bounds = ui.min_rect();
            });
            output.textures_delta.clear();
            let percentages = text_shapes(&output)
                .filter(|text| text.galley.text().ends_with('%'))
                .collect::<Vec<_>>();
            assert_eq!(percentages.len(), 6, "{unit:?}");
            let heading = text_rect(&output, TACTIC_AFFINITY_TITLE);
            let mut previous = egui::Rect::NOTHING;
            for (tactic, percentage) in CombatTactic::ALL.into_iter().zip(percentages) {
                let fit = tactic.affinity(unit);
                let icon = image_rect(&output, images.get(tactic.image_key()));
                let label = percentage.galley.rect.translate(percentage.pos.to_vec2());
                assert_eq!(percentage.galley.text(), format!("{fit}%"));
                assert_eq!(
                    percentage.galley.job.sections[0].format.color,
                    battle_plan_controls::affinity_color(fit)
                );
                assert!(bounds.contains_rect(icon));
                assert!(
                    bounds.expand(1.0).contains_rect(label),
                    "{unit:?}, {tactic:?} at width {width}: {label:?} outside {bounds:?}"
                );
                assert!(label.top() >= icon.bottom());
                assert!(
                    (label.center().x - icon.center().x).abs() < 1.0,
                    "{unit:?}, {tactic:?} at width {width}: {label:?} vs {icon:?}"
                );
                if previous.is_positive() {
                    assert!((icon.top() - previous.top()).abs() < 1.0);
                    assert!(icon.left() > previous.right());
                } else {
                    assert!((icon.left() - heading.left()).abs() < 1.0);
                }
                previous = icon;
            }
            assert!(bounds.width() <= width + 1.0, "{unit:?} at {width}: {bounds:?}");
        }
    }
}

#[test]
fn affinity_heading_opens_an_illustrated_tooltip_inside_the_unit_tooltip() {
    use bevy::ecs::system::SystemState;

    let context = context();
    let mut app = App::new();
    app.add_message::<MessageMsg>();
    let mut writer_state = SystemState::<MessageWriter<MessageMsg>>::new(app.world_mut());
    let artwork = egui::TextureId::User(201);
    let images = ImageIds(HashMap::from([("tactic affinity".to_owned(), artwork)]));
    let mut time = 1.0;
    let mut frame = |pointer: Option<egui::Pos2>| {
        time += 0.1;
        let mut writer = writer_state.get_mut(app.world_mut()).unwrap();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                time: Some(time),
                events: pointer.into_iter().map(egui::Event::PointerMoved).collect(),
                ..default()
            },
            |ui| {
                ui.label("Unit details").on_hover_ui(|ui| {
                    draw_unit_hover(
                        ui,
                        &Unit::Ship(Ship::LightFighter),
                        0,
                        false,
                        &mut UiState::default(),
                        &mut Player::new(1, 0),
                        &mut Planet::new(0, "Shipyard".into(), Vec2::ZERO, false, 1.0),
                        None,
                        &mut SenateSupport::default(),
                        [true; 2],
                        &mut PendingTurnCommands::default(),
                        &mut writer,
                        None,
                        &images,
                    );
                });
            },
        );
        output.textures_delta.clear();
        output
    };
    let output = frame(None::<egui::Pos2>);
    let origin = text_rect(&output, "Unit details").center();
    frame(Some(origin));
    frame(Some(origin));
    let output = frame(Some(origin));
    let heading = text_rect(&output, TACTIC_AFFINITY_TITLE).center();
    frame(Some(heading));
    frame(Some(heading));
    let output = frame(Some(heading));
    assert!(
        text_shapes(&output).any(|text| text.galley.text() == TACTIC_AFFINITY_DESCRIPTION),
        "visible labels: {:?}",
        text_shapes(&output).map(|text| text.galley.text()).collect::<Vec<_>>()
    );
    assert_eq!(
        text_shapes(&output).filter(|text| text.galley.text() == TACTIC_AFFINITY_TITLE).count(),
        2
    );
    assert_eq!(image_rect(&output, artwork).size(), egui::vec2(130.0, 90.0));
    assert_eq!(output.platform_output.cursor_icon, CursorIcon::Default);
}

/// Renders the actual ship/defense panels beside the shared attribute explanation.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires a GPU; writes unit-affinity previews under target"]
fn render_unit_affinity_panels() {
    use bevy::app::AppExit;
    use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
    use bevy::winit::{WinitPlugin, WinitSettings};
    use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};

    std::fs::create_dir_all("target/ui-unit-affinity").unwrap();
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Unit tactic affinity preview".into(),
                    resolution: (1300, 900).into(),
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
    .add_message::<MessageMsg>()
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
             mut writer: MessageWriter<MessageMsg>,
             mut handles: Local<Vec<egui::TextureHandle>>| {
                let Ok(context) = contexts.ctx_mut() else {
                    return;
                };
                if handles.is_empty() {
                    for folder in ["combat", "ships", "defense", "orbitals", "resources", "mission"]
                    {
                        let directory =
                            format!("{}/assets/images/{folder}", env!("CARGO_MANIFEST_DIR"));
                        for entry in std::fs::read_dir(directory).unwrap() {
                            let path = entry.unwrap().path();
                            if path.extension().is_none_or(|extension| extension != "png") {
                                continue;
                            }
                            let name = path.file_stem().unwrap().to_str().unwrap();
                            let pixels = image::open(&path).unwrap().into_rgba8();
                            handles.push(context.load_texture(
                                name,
                                egui::ColorImage::from_rgba_unmultiplied(
                                    [pixels.width() as usize, pixels.height() as usize],
                                    pixels.as_raw(),
                                ),
                                egui::TextureOptions::LINEAR,
                            ));
                        }
                    }
                }
                let images = ImageIds(
                    handles.iter().map(|handle| (handle.name().to_owned(), handle.id())).collect(),
                );
                let unit = if *frame < 60 {
                    Unit::Ship(Ship::LightFighter)
                } else {
                    Unit::Defense(Defense::PlasmaTurret)
                };
                egui::Window::new("unit panel")
                    .fixed_pos(egui::pos2(8.0, 8.0))
                    .resizable(false)
                    .title_bar(false)
                    .show(context, |ui| {
                        draw_unit_hover(
                            ui,
                            &unit,
                            0,
                            false,
                            &mut UiState::default(),
                            &mut Player::new(1, 0),
                            &mut Planet::new(0, "Shipyard".into(), Vec2::ZERO, false, 1.0),
                            None,
                            &mut SenateSupport::default(),
                            [true; 2],
                            &mut PendingTurnCommands::default(),
                            &mut writer,
                            None,
                            &images,
                        );
                    });
                egui::Window::new("attribute tooltip")
                    .fixed_pos(egui::pos2(760.0, 8.0))
                    .resizable(false)
                    .title_bar(false)
                    .show(context, |ui| {
                        draw_attribute_hover(
                            ui,
                            "tactic affinity",
                            TACTIC_AFFINITY_TITLE,
                            TACTIC_AFFINITY_DESCRIPTION,
                            &images,
                        );
                    });
                *frame += 1;
                if [40, 100].contains(&*frame) {
                    let path = if *frame == 40 {
                        "target/ui-unit-affinity/light-fighter.png"
                    } else {
                        "target/ui-unit-affinity/plasma-turret.png"
                    };
                    let mut capture = commands.spawn(Screenshot::primary_window());
                    capture.observe(save_to_disk(path));
                    if *frame == 100 {
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
