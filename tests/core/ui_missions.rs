use super::*;

#[test]
fn planet_report_layout_contains_every_unit_exactly_once() {
    for is_home_planet in [false, true] {
        let (critical, orbitals, buildings) = mission_report_planet_intel(is_home_planet);

        assert_eq!(critical, [Unit::planetary_shield(), Unit::space_dock()]);
        assert_eq!(orbitals.len(), 6);
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
        mission: Mission {
            owner: 8,
            destination: planet.id,
            ..default()
        },
        planet: planet.clone(),
        scout_probes: 0,
        surviving_attacker: Army::new(),
        surviving_defender: planet.army.clone(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: None,
        destination_controlled: planet.controlled,
        combat_report: None,
        hidden: false,
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
                    draw_mission_report_planet_intel(
                        ui,
                        &report,
                        &Player::new(7, 0),
                        false,
                        &images,
                    );
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
fn missile_strikes_do_not_offer_the_recall_action() {
    let owned_attack = Mission {
        owner: 7,
        objective: Icon::Attack,
        ..default()
    };
    let missile_strike = Mission {
        objective: Icon::MissileStrike,
        ..owned_attack.clone()
    };

    assert!(mission_recall_available(&owned_attack, 7));
    assert!(!mission_recall_available(&missile_strike, 7));
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
fn jump_gate_rings_span_both_sides_of_the_route_with_foreshortened_depth() {
    let center = egui::pos2(20.0, 30.0);
    let points = jump_gate_wave_front(center, 8.0, 4.0);
    for (index, offset) in [(0, (4.0, 0.0)), (8, (0.0, 8.0)), (16, (-4.0, 0.0)), (24, (0.0, -8.0))]
    {
        assert!(points[index].distance(center + egui::vec2(offset.0, offset.1)) < 0.0001);
    }
    assert!(points.iter().all(|point| {
        let relative = *point - center;
        ((relative.x / 4.0).powi(2) + (relative.y / 8.0).powi(2) - 1.0).abs() < 0.0001
    }));
}

#[test]
fn route_preview_speed_tracks_the_missions_slowest_ship() {
    let slow = Mission {
        army: Army::from([(Unit::Ship(Ship::ColonyShip), 1)]),
        ..default()
    };
    let fast = Mission {
        army: Army::from([(Unit::Ship(Ship::Probe), 1)]),
        ..default()
    };

    assert!(fast.route_animation_speed() > slow.route_animation_speed());
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
    assert_eq!(mission_report_image_size("mission jump", base_size), base_size);
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
    assert_eq!(draft.image(&player), "mission jump");

    draft.objective = Icon::Attack;
    sync_jump_gate_selection(&mut draft, &origin, &destination, &player, true);

    assert!(!draft.jump_gate);
    assert_eq!(draft.image(&player), "mission");
}
