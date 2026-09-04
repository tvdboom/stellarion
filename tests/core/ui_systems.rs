use super::*;
use crate::core::units::ships::Ship;

fn text_rect(shapes: &[egui::epaint::ClippedShape], text: &str) -> egui::Rect {
    shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(label) if label.galley.job.text == text => {
                Some(label.galley.rect.translate(label.pos.to_vec2()))
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing `{text}` shortcut label"))
}

fn has_text(shapes: &[egui::epaint::ClippedShape], text: &str) -> bool {
    shapes.iter().any(|shape| match &shape.shape {
        egui::Shape::Text(label) => label.galley.job.text == text,
        _ => false,
    })
}

fn text_color(shapes: &[egui::epaint::ClippedShape], text: &str) -> Color32 {
    shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(label) if label.galley.job.text == text => {
                label.galley.job.sections.first().map(|section| section.format.color)
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing `{text}` text color"))
}

fn text_font_size(shapes: &[egui::epaint::ClippedShape], text: &str) -> f32 {
    shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(label) if label.galley.job.text == text => {
                label.galley.job.sections.first().map(|section| section.format.font_id.size)
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing `{text}` font size"))
}

fn collect_rects(shape: &egui::Shape, rects: &mut Vec<egui::Rect>) {
    match shape {
        egui::Shape::Rect(candidate) => rects.push(candidate.rect),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_rects(shape, rects);
            }
        },
        _ => {},
    }
}

fn shape_has_fill(shape: &egui::Shape, fill: Color32) -> bool {
    match shape {
        egui::Shape::Rect(candidate) => candidate.fill == fill,
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| shape_has_fill(shape, fill)),
        _ => false,
    }
}

fn image_rect(
    shapes: &[egui::epaint::ClippedShape],
    texture_id: egui::TextureId,
) -> Option<egui::Rect> {
    image_rects(shapes, texture_id).into_iter().next()
}

fn image_rects(
    shapes: &[egui::epaint::ClippedShape],
    texture_id: egui::TextureId,
) -> Vec<egui::Rect> {
    fn collect(shape: &egui::Shape, texture_id: egui::TextureId, rects: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == texture_id => {
                let min = mesh
                    .vertices
                    .iter()
                    .fold(egui::pos2(f32::INFINITY, f32::INFINITY), |min, v| min.min(v.pos));
                let max = mesh
                    .vertices
                    .iter()
                    .fold(egui::pos2(f32::NEG_INFINITY, f32::NEG_INFINITY), |max, v| {
                        max.max(v.pos)
                    });
                rects.push(egui::Rect::from_min_max(min, max));
            },
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, texture_id, rects);
                }
            },
            _ => {},
        }
    }

    let mut rects = Vec::new();
    for shape in shapes {
        collect(&shape.shape, texture_id, &mut rects);
    }
    rects
}

fn image_tint(
    shapes: &[egui::epaint::ClippedShape],
    texture_id: egui::TextureId,
) -> Option<Color32> {
    shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Mesh(mesh) if mesh.texture_id == texture_id => {
            mesh.vertices.first().map(|vertex| vertex.color)
        },
        _ => None,
    })
}

#[test]
fn world_shortcut_selects_fleet_silhouettes_from_stationed_ships() {
    let mut planet = Planet::new(1, "Masduk".to_string(), Vec2::ZERO, false, 1.0);
    planet.army.insert(Unit::Building(Building::Shipyard), 1);
    planet.army.insert(Unit::Defense(Defense::RocketLauncher), 2);
    assert_eq!(world_shortcut_fleet_image(&planet), None);

    planet.army.insert(Unit::probe(), 3);
    assert_eq!(world_shortcut_fleet_image(&planet), Some("mission spy"));

    planet.army.insert(Unit::Ship(Ship::LightFighter), 1);
    assert_eq!(world_shortcut_fleet_image(&planet), Some("mission"));

    planet.army.insert(Unit::war_sun(), 1);
    assert_eq!(world_shortcut_fleet_image(&planet), Some("mission destroy"));
}

#[test]
fn world_shortcut_centers_the_name_and_only_shows_a_fleet_icon_for_a_fleet() {
    let context = egui::Context::default();
    let mut style = NordDark.custom_style();
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    context.set_global_style(style);
    let mut planet = Planet::new(1, "Masduk".to_string(), Vec2::ZERO, false, 1.0);
    planet.army.insert(Unit::Ship(Ship::LightFighter), 1);
    let planet_texture = egui::TextureId::User(1);
    let mission_texture = egui::TextureId::User(2);
    let fleet_color = Color32::from_rgb(244, 197, 66);
    let images = ImageIds(HashMap::from([
        (planet.image(), planet_texture),
        ("mission".to_string(), mission_texture),
    ]));

    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(260.0, 100.0),
            )),
            events: vec![egui::Event::PointerMoved(egui::pos2(50.0, 25.0))],
            ..default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                draw_world_shortcut(ui, &planet, fleet_color, &images, 1.0);
            });
        },
    );
    output.textures_delta.clear();

    let name = text_rect(&output.shapes, "Masduk");
    let planet_icon = image_rect(&output.shapes, planet_texture).expect("missing planet icon");
    let fleet_icon = image_rect(&output.shapes, mission_texture).expect("missing fleet icon");
    assert!((name.center().y - planet_icon.center().y).abs() < 1.0);
    assert!((fleet_icon.center().y - planet_icon.center().y).abs() < 1.0);
    assert!(name.right() < fleet_icon.left());
    assert_eq!(fleet_icon.size(), egui::Vec2::splat(20.0));
    assert_eq!(image_tint(&output.shapes, mission_texture), Some(fleet_color));
    assert!(
        !has_text(&output.shapes, "FLEET") && !has_text(&output.shapes, "NO FLEET"),
        "world shortcut still painted fleet status text"
    );
    for tooltip in
        ["Center the map and open this planet", "Center the map on this controlled world"]
    {
        assert!(!has_text(&output.shapes, tooltip), "world shortcut still painted `{tooltip}`");
    }

    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let planet = Planet::new(2, "Galix".to_string(), Vec2::ZERO, false, 1.0);
    let mut output = context.run_ui(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            draw_world_shortcut(ui, &planet, fleet_color, &images, 1.0);
        });
    });
    output.textures_delta.clear();

    assert!(image_rect(&output.shapes, mission_texture).is_none());
    assert!(!has_text(&output.shapes, "NO FLEET"));
}

#[test]
fn world_groups_use_separate_headings_with_prominent_counts() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(300.0, 160.0),
            )),
            ..default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                ui.set_width(OWNED_WORLDS_WIDTH);
                draw_world_group_header(ui, "OWNED PLANETS", 7, 1.0);
                draw_world_group_header(ui, "CONTROLLED PLANETS AND MOONS", 3, 1.0);
            });
        },
    );
    output.textures_delta.clear();

    for label in ["OWNED PLANETS", "7", "CONTROLLED PLANETS AND MOONS", "3"] {
        text_rect(&output.shapes, label);
    }
    let own_heading = text_rect(&output.shapes, "OWNED PLANETS");
    let own_count = text_rect(&output.shapes, "7");
    let controlled_heading = text_rect(&output.shapes, "CONTROLLED PLANETS AND MOONS");
    let controlled_count = text_rect(&output.shapes, "3");
    assert!(own_count.height() > own_heading.height());
    assert!(controlled_count.height() > controlled_heading.height());
    assert!(controlled_count.left() - controlled_heading.right() >= 8.0);
    assert!(
        output.shapes.iter().all(|shape| match &shape.shape {
            egui::Shape::Text(label) => label.galley.job.text != "YOUR WORLDS",
            _ => true,
        }),
        "the removed aggregate heading was still painted"
    );
}

#[test]
fn owned_worlds_panel_grows_with_the_number_of_planets() {
    fn panel_height(planet_count: usize) -> f32 {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let planets = (0..planet_count)
            .map(|id| {
                let mut planet = Planet::new(id, format!("Planet {id}"), Vec2::ZERO, false, 1.0);
                planet.owned = Some(1);
                planet
            })
            .collect::<Vec<_>>();
        let images = ImageIds(
            planets.iter().map(|planet| (planet.image(), egui::TextureId::User(1))).collect(),
        );
        let map = Map {
            rect: Rect::default(),
            planets,
        };
        let player = Player::new(1, 0);
        let mut state = UiState::default();
        let mut settings = Settings::default();
        let mut height = 0.0;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1_280.0, 2_000.0),
                )),
                ..default()
            },
            |context| {
                height = draw_owned_worlds_widget(
                    context,
                    &map,
                    &player,
                    &mut state,
                    &mut settings,
                    &images,
                )
                .height();
            },
        );
        output.textures_delta.clear();
        height
    }

    let one_planet = panel_height(1);
    let four_planets = panel_height(4);

    assert!(
        four_planets >= one_planet + 3.0 * (WORLD_SHORTCUT_HEIGHT + WORLD_LIST_ITEM_SPACING),
        "one planet: {one_planet}, four planets: {four_planets}"
    );
}

#[test]
fn owned_worlds_panel_uses_the_compact_screen_edge_inset_below_the_resource_panel() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let map = Map {
        rect: Rect::default(),
        planets: Vec::new(),
    };
    let player = Player::new(1, 0);
    let mut state = UiState::default();
    let mut settings = Settings::default();
    let images = ImageIds::default();
    let mut panel = egui::Rect::NOTHING;
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1_280.0, 720.0),
            )),
            ..default()
        },
        |context| {
            panel = draw_owned_worlds_widget(
                context,
                &map,
                &player,
                &mut state,
                &mut settings,
                &images,
            );
        },
    );
    output.textures_delta.clear();

    assert_eq!(panel.min, egui::pos2(OWNED_WORLDS_LEFT, OWNED_WORLDS_TOP));
    assert_eq!(panel.width(), OWNED_WORLDS_WIDTH + 20.0);
}

#[test]
fn strategic_hud_panels_scale_with_viewports() {
    fn panel_metrics(viewport: egui::Vec2) -> (egui::Rect, egui::Rect, egui::Vec2, egui::Vec2) {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut planet = Planet::new(0, "Masduk".to_string(), Vec2::ZERO, false, 1.0);
        planet.owned = Some(1);
        let planet_texture = egui::TextureId::User(6);
        let turn_texture = egui::TextureId::User(1);
        let images = ImageIds(HashMap::from([
            ("turn".to_string(), turn_texture),
            ("owned".to_string(), egui::TextureId::User(2)),
            ("metal".to_string(), egui::TextureId::User(3)),
            ("crystal".to_string(), egui::TextureId::User(4)),
            ("deuterium".to_string(), egui::TextureId::User(5)),
            (planet.image(), planet_texture),
        ]));
        let map = Map {
            rect: Rect::default(),
            planets: vec![planet],
        };
        let player = Player::new(1, 0);
        let mut state = UiState::default();
        let mut settings = Settings::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
            ..default()
        };

        let mut warmup = context.run_ui(input(), |context| {
            draw_owned_worlds_widget(context, &map, &player, &mut state, &mut settings, &images);
            draw_resources_widget(context, &settings, &map, &player, &images);
        });
        warmup.textures_delta.clear();

        let mut worlds = egui::Rect::NOTHING;
        let mut resources = egui::Rect::NOTHING;
        let mut output = context.run_ui(input(), |context| {
            worlds = draw_owned_worlds_widget(
                context,
                &map,
                &player,
                &mut state,
                &mut settings,
                &images,
            );
            resources = draw_resources_widget(context, &settings, &map, &player, &images);
        });
        output.textures_delta.clear();

        let turn_icon = image_rect(&output.shapes, turn_texture).expect("missing turn icon").size();
        let planet_icon =
            image_rect(&output.shapes, planet_texture).expect("missing planet icon").size();
        (worlds, resources, turn_icon, planet_icon)
    }

    let baseline = panel_metrics(egui::vec2(HUD_REFERENCE_WIDTH, HUD_REFERENCE_HEIGHT));
    let small =
        panel_metrics(egui::vec2(HUD_REFERENCE_WIDTH * 0.9, HUD_REFERENCE_HEIGHT * HUD_MIN_SCALE));
    let large = panel_metrics(egui::vec2(HUD_REFERENCE_WIDTH * 2.0, HUD_REFERENCE_HEIGHT * 2.0));

    assert_eq!(strategic_hud_scale(egui::vec2(800.0, 600.0)), HUD_MIN_SCALE);
    assert_eq!(
        strategic_hud_scale(egui::vec2(HUD_REFERENCE_WIDTH * 2.0, HUD_REFERENCE_HEIGHT * 2.0,)),
        HUD_MAX_SCALE
    );
    assert!(
        (large.0.width() / baseline.0.width() - HUD_MAX_SCALE).abs() < 0.02,
        "world panel did not scale proportionally: baseline={:?}, large={:?}",
        baseline.0,
        large.0
    );
    assert!(
        large.0.height() / baseline.0.height() > 1.5,
        "world panel did not scale proportionally: baseline={:?}, large={:?}",
        baseline.0,
        large.0
    );
    assert!(
        (large.1.width() / baseline.1.width() - HUD_MAX_SCALE).abs() < 0.02,
        "resource panel did not scale proportionally: baseline={:?}, large={:?}",
        baseline.1,
        large.1
    );
    assert_eq!(baseline.2, egui::vec2(70.0, 44.0));
    assert!((small.2 - baseline.2 * HUD_MIN_SCALE).length() < 0.01);
    assert!((large.2 - baseline.2 * HUD_MAX_SCALE).length() < 0.01);
    assert_eq!(baseline.3, egui::Vec2::splat(30.0));
    assert_eq!(small.3, baseline.3);
    assert_eq!(large.3, baseline.3 * HUD_MAX_SCALE);
    assert_eq!(small.0, baseline.0);
    assert!(small.1.height() < baseline.1.height());
    let expected_world_position = baseline.0.min * HUD_MAX_SCALE;
    assert!((large.0.min.x - expected_world_position.x).abs() < 1.0);
    assert!((large.0.min.y - expected_world_position.y).abs() < 1.0);
    assert!((large.1.top() - RESOURCE_BAR_TOP * HUD_MAX_SCALE).abs() < 1.0);
}

#[test]
fn resource_bar_uses_the_shared_hud_frame_without_the_legacy_texture() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let legacy_texture = egui::TextureId::User(99);
    let images = ImageIds(HashMap::from([
        ("turn".to_string(), egui::TextureId::User(1)),
        ("owned".to_string(), egui::TextureId::User(2)),
        ("metal".to_string(), egui::TextureId::User(3)),
        ("crystal".to_string(), egui::TextureId::User(4)),
        ("deuterium".to_string(), egui::TextureId::User(5)),
        ("thin panel".to_string(), legacy_texture),
    ]));
    let map = Map {
        rect: Rect::default(),
        planets: Vec::new(),
    };
    let player = Player::default();
    let settings = Settings::default();
    let viewport = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(HUD_REFERENCE_WIDTH, HUD_REFERENCE_HEIGHT),
    );
    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };

    let mut warmup = context.run_ui(input(), |context| {
        draw_resources_widget(context, &settings, &map, &player, &images);
    });
    warmup.textures_delta.clear();

    let mut panel = egui::Rect::NOTHING;
    let mut output = context.run_ui(input(), |context| {
        panel = draw_resources_widget(context, &settings, &map, &player, &images);
    });
    output.textures_delta.clear();

    assert_eq!(panel.top(), RESOURCE_BAR_TOP);
    assert!(panel.left() >= RESOURCE_BAR_SIDE_INSET);
    assert!(panel.right() <= viewport.right() - RESOURCE_BAR_SIDE_INSET);
    assert!(
        (panel.center().x - viewport.center().x).abs() < 1.0,
        "resource bar was not centered: {panel:?} in {viewport:?}"
    );
    assert!(
        (780.0..1_000.0).contains(&panel.width()),
        "resource bar did not use the intended larger content width: {panel:?}"
    );
    assert_eq!(hud_panel_frame().fill, HUD_PANEL_FILL);
    assert_eq!(hud_panel_frame().stroke.color, HUD_PANEL_STROKE);
    for label in ["TURN", "PLANETS", "METAL", "CRYSTAL", "DEUTERIUM"] {
        text_rect(&output.shapes, label);
    }
    let turn_image = image_rect(&output.shapes, images.get("turn")).expect("missing turn image");
    assert_eq!(turn_image.size(), egui::vec2(70.0, 44.0));
    assert!((turn_image.center().y - panel.center().y).abs() < 1.0);
    let turn_label = text_rect(&output.shapes, "TURN");
    let turn_value = text_rect(&output.shapes, "1");
    let turn_text_center = (turn_label.top() + turn_value.bottom()) * 0.5;
    assert!(
        (turn_text_center - panel.center().y - RESOURCE_SUMMARY_TEXT_VERTICAL_OFFSET).abs() < 1.0,
        "turn text was not optically centered: label={turn_label:?}, value={turn_value:?}, panel={panel:?}"
    );
    let value_bottom_padding = panel.bottom() - text_rect(&output.shapes, "1500").bottom();
    assert!(
        value_bottom_padding < 10.0,
        "resource value retained too much bottom padding: {value_bottom_padding}"
    );
    let planets_to_metal =
        image_rect(&output.shapes, images.get("metal")).expect("missing metal image").left()
            - text_rect(&output.shapes, "PLANETS").right();
    let metal_to_crystal =
        image_rect(&output.shapes, images.get("crystal")).expect("missing crystal image").left()
            - text_rect(&output.shapes, "1500").right();
    assert!(planets_to_metal > metal_to_crystal + 15.0);
    assert_eq!(text_font_size(&output.shapes, "1500"), 30.0);
    assert!(text_rect(&output.shapes, "1500").height() > 30.0);
    assert!(
        image_rect(&output.shapes, legacy_texture).is_none(),
        "the removed thin-panel texture was still painted"
    );
}

#[test]
fn resource_summaries_have_one_disjoint_hover_target_without_highlight_chrome() {
    let context = egui::Context::default();
    let mut style = NordDark.custom_style();
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    context.set_global_style(style);
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 120.0));
    let draw = |ui: &mut Ui, targets: &mut Vec<egui::Rect>| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let first =
                draw_resource_summary(ui, egui::TextureId::User(1), "METAL", "1500", false, 1.0);
            targets.push(first.rect);
            first.on_hover_ui(|ui| {
                ui.label("FIRST RESOURCE TOOLTIP");
            });
            draw_resource_gap(ui, resource_bar_gap(false, 1.0), false, 1.0);
            let second =
                draw_resource_summary(ui, egui::TextureId::User(2), "CRYSTAL", "1200", false, 1.0);
            targets.push(second.rect);
            second.on_hover_ui(|ui| {
                ui.label("SECOND RESOURCE TOOLTIP");
            });
        });
    };
    let input = |events| egui::RawInput {
        screen_rect: Some(viewport),
        events,
        ..default()
    };

    let mut targets = Vec::new();
    let mut warmup = context.run_ui(input(Vec::new()), |ui| draw(ui, &mut targets));
    warmup.textures_delta.clear();
    let pointer = targets[0].center();
    targets.clear();

    let mut hover_start = context
        .run_ui(input(vec![egui::Event::PointerMoved(pointer)]), |ui| draw(ui, &mut targets));
    hover_start.textures_delta.clear();
    targets.clear();
    let mut output = context.run_ui(input(Vec::new()), |ui| draw(ui, &mut targets));
    output.textures_delta.clear();

    assert!(targets[0].right() < targets[1].left());
    assert!(has_text(&output.shapes, "FIRST RESOURCE TOOLTIP"));
    assert!(!has_text(&output.shapes, "SECOND RESOURCE TOOLTIP"));
    let removed_hover_fill = Color32::from_rgba_unmultiplied(130, 170, 215, 18);
    assert!(
        output.shapes.iter().all(|shape| !shape_has_fill(&shape.shape, removed_hover_fill)),
        "resource hover still painted a visible container"
    );
}

#[test]
fn resource_tooltip_restores_the_large_image_and_production_details() {
    let context = egui::Context::default();
    let mut style = NordDark.custom_style();
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    context.set_global_style(style);
    let textures = ["turn", "owned", "metal", "crystal", "deuterium"].map(|name| {
        context.load_texture(
            format!("resource tooltip test {name}"),
            egui::ColorImage::filled([1, 1], Color32::WHITE),
            default(),
        )
    });
    let metal_texture = textures[2].id();
    let images = ImageIds(HashMap::from([
        ("turn".to_string(), textures[0].id()),
        ("owned".to_string(), textures[1].id()),
        ("metal".to_string(), metal_texture),
        ("crystal".to_string(), textures[3].id()),
        ("deuterium".to_string(), textures[4].id()),
    ]));
    let map = Map {
        rect: Rect::default(),
        planets: Vec::new(),
    };
    let player = Player::default();
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_000.0, 300.0));
    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };
    let mut tooltip_image = egui::Rect::NOTHING;
    let mut warmup = context.run_ui(input(), |ui| {
        tooltip_image = draw_resource_tooltip(ui, ResourceName::Metal, &map, &player, &images);
    });
    warmup.textures_delta.clear();
    let mut output = context.run_ui(input(), |ui| {
        tooltip_image = draw_resource_tooltip(ui, ResourceName::Metal, &map, &player, &images);
    });
    output.textures_delta.clear();

    assert!(has_text(&output.shapes, "Metal"));
    assert!(has_text(&output.shapes, "Production: +0"));
    assert_eq!(tooltip_image.size(), egui::vec2(130.0, 90.0));
}

#[test]
fn controlled_world_shortcut_keeps_the_world_selected_as_a_mission_origin() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut planet = Planet::new(7, "Forward Base".to_string(), Vec2::ZERO, false, 1.0);
    planet.controlled = Some(1);
    let images = ImageIds(HashMap::from([(planet.image(), egui::TextureId::User(1))]));
    let map = Map {
        rect: Rect::default(),
        planets: vec![planet],
    };
    let player = Player::new(1, 0);
    let mut state = UiState {
        mission: true,
        combat_report: Some(3),
        focus_planet: Some(2),
        ..default()
    };
    let mut settings = Settings {
        show_menu: false,
        ..default()
    };
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_280.0, 720.0));
    {
        let mut frame = |events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..default()
                },
                |context| {
                    draw_owned_worlds_widget(
                        context,
                        &map,
                        &player,
                        &mut state,
                        &mut settings,
                        &images,
                    );
                },
            );
            output.textures_delta.clear();
            output
        };

        frame(Vec::new());
        let output = frame(Vec::new());
        let position = text_rect(&output.shapes, "Forward Base").center();
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

    assert_eq!(state.planet_selected, Some(7));
    assert_eq!(state.mission_info.origin, 7);
    assert_eq!(state.focus_planet, None);
    assert!(state.to_selected);
    assert!(!state.mission);
    assert_eq!(state.combat_report, None);
    assert!(settings.show_menu);
}

#[test]
fn mission_planet_hover_uses_a_units_only_panel_without_replacing_click_selection() {
    let state = UiState {
        mission_planet_hover: Some(3),
        planet_hover: Some(2),
        planet_selected: Some(1),
        ..default()
    };

    assert_eq!(visible_planet_panel(&state), Some((3, PlanetPanelMode::UnitsOnly)));
    assert_eq!(state.planet_selected, Some(1));
}

#[test]
fn map_hover_and_click_selection_keep_the_full_planet_panel() {
    for state in [
        UiState {
            planet_hover: Some(2),
            ..default()
        },
        UiState {
            planet_selected: Some(1),
            ..default()
        },
    ] {
        assert_eq!(visible_planet_panel(&state).map(|(_, mode)| mode), Some(PlanetPanelMode::Full));
    }
}

#[test]
fn planet_panel_slide_restarts_for_each_world_and_eases_from_its_map_edge() {
    let left_target = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: false,
    };
    let right_target = PlanetPanelSlideTarget {
        id: 2,
        mode: PlanetPanelMode::Full,
        right_side: true,
    };
    let mut slide = PlanetPanelSlide::default();

    assert_eq!(slide.progress(left_target, 0.0), 0.0);
    let halfway = slide.progress(left_target, PLANET_PANEL_SLIDE_DURATION * 0.5);
    assert_eq!(halfway, 0.5);
    assert_eq!(planet_panel_slide_offset(halfway, false, 600.0), -75.0);
    assert_eq!(planet_panel_slide_offset(halfway, true, 600.0), 75.0);
    assert_eq!(slide.progress(left_target, PLANET_PANEL_SLIDE_DURATION), 1.0);

    assert_eq!(slide.progress(right_target, PLANET_PANEL_SLIDE_DURATION), 0.0);
    slide.hide();
    assert_eq!(slide.progress(right_target, PLANET_PANEL_SLIDE_DURATION), 0.0);
}

#[test]
fn planet_detail_lines_start_after_the_panel_and_follow_from_top_to_bottom() {
    let target = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: true,
    };
    let mut slide = PlanetPanelSlide::default();

    slide.progress(target, 0.0);
    slide.progress(target, PLANET_PANEL_SLIDE_DURATION);
    assert_eq!(slide.detail_progress(0), 0.0);
    assert_eq!(slide.detail_progress(1), 0.0);

    slide.progress(target, PLANET_DETAIL_LINE_STAGGER * 0.5);
    assert!(slide.detail_progress(0) > 0.0);
    assert_eq!(slide.detail_progress(1), 0.0);

    slide.progress(target, PLANET_DETAIL_LINE_STAGGER);
    assert!(slide.detail_progress(0) > slide.detail_progress(1));
    assert!(slide.detail_progress(1) > 0.0);
    assert_eq!(slide.detail_progress(2), 0.0);

    slide.progress(target, PLANET_PANEL_TOTAL_DURATION);
    for line in 0..PLANET_DETAIL_LINE_COUNT {
        assert_eq!(slide.detail_progress(line), 1.0);
    }
    assert!(!slide.is_animating());
}

#[test]
fn mission_hover_panels_stay_opposite_the_pointer() {
    const VIEWPORT_WIDTH: f32 = 1_200.0;

    for pointer_x in [100.0, 1_100.0] {
        let (fleet_x, info_x) = mission_hover_panel_x_positions(Some(pointer_x), VIEWPORT_WIDTH);
        let fleet = egui::Rect::from_min_size(
            egui::pos2(fleet_x, 0.0),
            egui::vec2(MISSION_HOVER_FLEET_WIDTH, 630.0),
        );
        let info = egui::Rect::from_min_size(
            egui::pos2(info_x, 0.0),
            egui::vec2(MISSION_HOVER_INFO_WIDTH, 280.0),
        );

        assert!(!fleet.x_range().contains(pointer_x));
        assert!(!info.x_range().contains(pointer_x));
        assert_eq!(fleet_x > VIEWPORT_WIDTH * 0.5, pointer_x < VIEWPORT_WIDTH * 0.5);
    }
}

#[test]
fn abandon_confirmation_is_centered_and_reuses_the_planet_panel_texture() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let panel_texture = egui::TextureId::User(9);
    let images = ImageIds(HashMap::from([("panel".to_string(), panel_texture)]));
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_000.0, 700.0));
    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };
    context.begin_pass(input());
    assert_eq!(draw_abandon_confirmation(&context, &images), None);
    let mut warmup = context.end_pass();
    warmup.textures_delta.clear();

    context.begin_pass(input());
    assert_eq!(draw_abandon_confirmation(&context, &images), None);
    let content_center = context.content_rect().center();
    let mut output = context.end_pass();
    output.textures_delta.clear();

    let panel = image_rect(&output.shapes, panel_texture).expect("missing modal panel image");
    assert!(
        (panel.center().x - content_center.x).abs() < 1.0,
        "panel {panel:?}, content center {content_center:?}"
    );
    assert!(
        (panel.center().y - content_center.y).abs() < 1.0,
        "panel {panel:?}, content center {content_center:?}"
    );
    assert_eq!(panel.size(), egui::vec2(520.0, 230.0));
    for text in ["Are you sure you want to abandon this planet?", "Yes", "No"] {
        let label = text_rect(&output.shapes, text);
        assert!(panel.contains_rect(label), "modal text `{text}` was outside {panel:?}: {label:?}");
        assert_eq!(text_color(&output.shapes, text), ABANDON_CONFIRMATION_TEXT_COLOR);
    }

    let mut painted_rects = Vec::new();
    for shape in &output.shapes {
        collect_rects(&shape.shape, &mut painted_rects);
    }
    let buttons = painted_rects
        .iter()
        .filter(|button| {
            (button.width() - 96.0).abs() < 1.0 && (button.height() - 40.0).abs() < 1.0
        })
        .collect::<Vec<_>>();
    assert_eq!(
        buttons.len(),
        2,
        "expected two styled confirmation buttons; painted rectangles: {painted_rects:?}"
    );
    assert!(buttons.iter().all(|button| panel.contains_rect(**button)));
    let button_row = buttons[0].union(*buttons[1]);
    assert!((button_row.center().x - panel.center().x).abs() < 1.0);

    for removed in [
        "ABANDON PLANET",
        "The buildings on Masduk will remain, but its defenses will be destroyed.",
    ] {
        assert!(
            !has_text(&output.shapes, removed),
            "removed modal text `{removed}` was still shown"
        );
    }
}
