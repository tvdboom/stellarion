use super::*;

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
fn jump_gate_wave_fronts_are_open_and_bow_toward_the_destination() {
    let center = egui::pos2(20.0, 30.0);
    let points = jump_gate_wave_front(center, 8.0, 4.0);
    let middle = points[points.len() / 2];

    assert!((points.first().unwrap().x - center.x).abs() <= f32::EPSILON);
    assert!((points.last().unwrap().x - center.x).abs() <= f32::EPSILON);
    assert_eq!(points.first().unwrap().y, center.y - 8.0);
    assert_eq!(points.last().unwrap().y, center.y + 8.0);
    assert!(middle.x > center.x);
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
