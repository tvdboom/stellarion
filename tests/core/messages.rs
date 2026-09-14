use super::*;
use crate::core::simulation::{GameModel, GameRules};
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Unit};

#[test]
fn error_notifications_begin_with_a_capital_letter() {
    assert_eq!(MessageMsg::error("invalid command").message, "Invalid command");
    assert_eq!(MessageMsg::error("Already capitalized").message, "Already capitalized");
}

fn notification_frame(
    context: &egui::Context,
    messages: &Messages,
    screen: egui::Rect,
    events: Vec<egui::Event>,
) -> Vec<egui::Rect> {
    let scale = notification_scale(screen.size());
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..default()
        },
        |ui| {
            draw_notifications(ui.ctx(), messages, true, false);
        },
    );
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.corner_radius == egui::CornerRadius::same(5) * scale =>
            {
                Some(rect.rect)
            },
            _ => None,
        })
        .collect()
}

fn notification_area_rect(context: &egui::Context) -> Option<egui::Rect> {
    let id = egui::Id::new("stellarion_notifications");
    let rect = context.memory(|memory| memory.area_rect(id))?;
    let transform = context
        .layer_transform_to_global(egui::LayerId::new(egui::Order::Tooltip, id))
        .unwrap_or(egui::emath::TSTransform::IDENTITY);
    Some(transform.mul_rect(rect))
}

#[test]
fn notifications_stack_separately_and_shrink_when_long_messages_expire() {
    let context = egui::Context::default();
    let long = "Battle at planet Ganymede ended in a draw; the attacking fleet is returning to its planet of origin.";
    for size in [egui::vec2(1600.0, 900.0), egui::vec2(320.0, 320.0), egui::vec2(360.0, 640.0)] {
        let scale = notification_scale(size);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let mut messages = Messages::default();
        messages.push(&MessageMsg::warning(long));
        messages.push(&MessageMsg::info("Turn 2 started."));
        for _ in 0..3 {
            notification_frame(&context, &messages, screen, vec![]);
        }
        let rects = notification_frame(&context, &messages, screen, vec![]);
        assert_eq!(rects.len(), 2);
        assert!(rects.iter().all(|rect| screen.contains_rect(*rect)));
        assert!(
            rects[1].top() >= rects[0].bottom() + NOTIFICATION_SPACING * scale - 0.2,
            "boxes must not overlap: {rects:?}"
        );
        assert!((rects[0].right() - rects[1].right()).abs() < 1.0);
        assert!(
            rects[1].width() < rects[0].width() * 0.6,
            "short toast inherited long toast width: {rects:?}"
        );
        if size.x <= MAX_NOTIFICATION_WIDTH + 50.0 {
            assert!(rects[1].height() < rects[0].height());
        } else {
            assert!(rects[1].height() <= rects[0].height());
        }

        messages.0.pop_front();
        for _ in 0..3 {
            notification_frame(&context, &messages, screen, vec![]);
        }
        let compact = notification_frame(&context, &messages, screen, vec![]);
        assert_eq!(compact.len(), 1);
        let expected_top = (DEFAULT_NOTIFICATION_TOP * scale)
            .max(resource_bar_bottom(size) + RESOURCE_BAR_NOTIFICATION_GAP * scale);
        assert!((compact[0].top() - expected_top).abs() < 1.0);
        assert!(
            compact[0].top()
                >= resource_bar_bottom(size) + RESOURCE_BAR_NOTIFICATION_GAP * scale - 1.0,
            "toast overlaps the resource panel: {compact:?}"
        );
        assert!((compact[0].width() - rects[1].width()).abs() < 1.0);
        let area = notification_area_rect(&context).unwrap();
        assert!(
            (area.width() - compact[0].width()).abs() < 1.0,
            "expired toast left a fixed-width area"
        );
        assert!((area.height() - compact[0].height()).abs() < 1.0);
    }
}

#[test]
fn notification_can_override_the_default_display_lifetime() {
    let mut messages = Messages::default();
    messages.push(
        &MessageMsg::info("The host closed the lobby.")
            .with_duration(std::time::Duration::from_secs(2)),
    );

    assert_eq!(messages.0.front().unwrap().remaining_seconds, 2.0);
}

#[test]
fn notification_stack_does_not_scroll_with_the_mouse_wheel() {
    let context = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 320.0));
    let mut messages = Messages::default();
    for _ in 0..6 {
        messages.push(&MessageMsg::info("A long battle report from Ganymede. The attacking fleet is returning to its planet of origin."));
    }
    for _ in 0..3 {
        notification_frame(&context, &messages, screen, vec![]);
    }
    let before = notification_frame(&context, &messages, screen, vec![]);
    let pos = before[0].center();
    notification_frame(
        &context,
        &messages,
        screen,
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -100.0),
                phase: egui::TouchPhase::Move,
                modifiers: default(),
            },
        ],
    );
    let after = notification_frame(&context, &messages, screen, vec![]);
    assert_eq!(before, after, "toast positions must not be controlled by a scroll container");
}

#[test]
fn colony_toast_is_clickable_and_fits_small_viewports() {
    for size in [egui::vec2(1600.0, 900.0), egui::vec2(360.0, 640.0), egui::vec2(320.0, 320.0)] {
        let context = egui::Context::default();
        let mut messages = Messages::default();
        messages.push(
            &MessageMsg::info("Colony established in Ganymede.")
                .with_action(MessageAction::FocusColony(7)),
        );
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let frame = |events| {
            let mut clicked = None;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..default()
                },
                |ui| clicked = draw_notifications(ui.ctx(), &messages, true, false),
            );
            output.textures_delta.clear();
            clicked
        };
        frame(vec![]);
        frame(vec![]);
        let rect = notification_area_rect(&context).unwrap();
        assert!(screen.contains_rect(rect), "notification is clipped at {size:?}: {rect:?}");
        let pos = rect.center();
        frame(vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: default(),
            },
        ]);
        let clicked = frame(vec![egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: default(),
        }]);
        assert_eq!(clicked, Some((0, MessageAction::FocusColony(7))), "{size:?}");
    }
}

#[test]
fn toasts_are_hidden_and_paused_during_combat_selection_and_animation() {
    for game_state in [GameState::CombatMenu, GameState::Combat] {
        let hidden_for_combat = notifications_hidden_during_combat(true, Some(game_state));
        assert!(hidden_for_combat);
        let context = egui::Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let mut messages = Messages::default();
        messages.push(
            &MessageMsg::info("Battle won at planet Ganymede.")
                .with_action(MessageAction::OpenMissionReport(42)),
        );
        let mut clicked = None;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..default()
            },
            |ui| clicked = draw_notifications(ui.ctx(), &messages, false, hidden_for_combat),
        );
        output.textures_delta.clear();

        assert_eq!(clicked, None, "toast opened during {game_state:?}");
        assert!(
            context
                .memory(|memory| memory.area_rect(egui::Id::new("stellarion_notifications")))
                .is_none(),
            "toast was drawn during {game_state:?}"
        );

        let remaining = messages.0[0].remaining_seconds;
        assert!(advance_message_lifetime(&mut messages.0[0], 1.0, true));
        assert_eq!(
            messages.0[0].remaining_seconds, remaining,
            "toast expired during {game_state:?}"
        );
        assert!(advance_message_lifetime(&mut messages.0[0], 1.0, false));
        assert_eq!(messages.0[0].remaining_seconds, remaining - 1.0);
    }
}

#[test]
fn spy_toast_click_opens_the_requested_mission_report() {
    let context = egui::Context::default();
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 640.0));
    let mut messages = Messages::default();
    messages.push(
        &MessageMsg::info("Spy mission successful at planet Ganymede.")
            .with_action(MessageAction::OpenMissionReport(42)),
    );
    let frame = |events| {
        let mut clicked = None;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                events,
                ..default()
            },
            |ui| clicked = draw_notifications(ui.ctx(), &messages, true, false),
        );
        output.textures_delta.clear();
        clicked
    };
    frame(vec![]);
    frame(vec![]);
    let rect = notification_area_rect(&context).unwrap();
    let pos = rect.center();
    frame(vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: default(),
        },
    ]);
    let clicked = frame(vec![egui::Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: default(),
    }]);
    assert_eq!(clicked, Some((0, MessageAction::OpenMissionReport(42))));

    let mut state = UiState {
        planet_selected: Some(7),
        combat_report: Some(3),
        ..default()
    };
    open_mission_reports(&mut state, Some(42));
    assert_eq!(state.planet_selected, None);
    assert!(state.mission);
    assert_eq!(state.mission_tab, MissionTab::MissionReports);
    assert_eq!(state.mission_report, Some(42));
    assert_eq!(state.combat_report, None);
}

#[test]
fn colony_toast_selects_and_centers_using_the_planet_click_path() {
    let mut model = GameModel::new([8; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = &model.players[0];
    let planet = player.home_planet;
    let mut state = UiState {
        mission: true,
        combat_report: Some(3),
        ..default()
    };
    assert!(focus_colony(planet, &model.map, player, &mut state));
    assert_eq!(state.planet_selected, Some(planet));
    assert!(state.to_selected);
    assert!(!state.mission);
    assert_eq!(state.combat_report, None);
    assert_eq!(state.mission_info.origin, planet);

    state.to_selected = false;
    assert!(!focus_colony(usize::MAX, &model.map, player, &mut state));
    assert!(!state.to_selected);
    model.map.get_mut(planet).owned = Some(model.players[1].id);
    assert!(!focus_colony(planet, &model.map, player, &mut state));
    assert!(!state.to_selected, "a stale notification must not navigate to a lost colony");
    model.map.get_mut(planet).owned = Some(player.id);
    model.map.get_mut(planet).is_destroyed = true;
    assert!(!focus_colony(planet, &model.map, player, &mut state));
    assert!(!state.to_selected);
}

#[test]
fn return_toast_opens_the_reports_panel_without_selecting_a_hidden_report() {
    let mut state = UiState {
        planet_selected: Some(4),
        mission_report: Some(17),
        combat_report: Some(9),
        ..default()
    };

    open_mission_reports(&mut state, None);

    assert_eq!(state.planet_selected, None);
    assert!(state.mission);
    assert_eq!(state.mission_tab, MissionTab::MissionReports);
    assert_eq!(state.mission_report, Some(17), "the last visible report remains selected");
    assert_eq!(state.combat_report, None);
}

#[test]
fn space_dock_notification_fits_on_one_line_at_normal_game_width() {
    let context = egui::Context::default();
    context.style_mut_of(egui::Theme::Dark, |style| {
        style.text_styles.insert(
            egui::TextStyle::Small,
            egui::FontId::new(18.0, egui::FontFamily::Proportional),
        );
    });
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1600.0, 900.0));
    let mut messages = Messages::default();
    messages.push(&MessageMsg::info("A Space Dock has been built on planet Darian."));
    for _ in 0..3 {
        notification_frame(&context, &messages, screen, vec![]);
    }
    let dock = notification_frame(&context, &messages, screen, vec![])[0];

    let mut short = Messages::default();
    short.push(&MessageMsg::info("Turn 2 started."));
    for _ in 0..3 {
        notification_frame(&context, &short, screen, vec![]);
    }
    let single_line = notification_frame(&context, &short, screen, vec![])[0];

    assert_eq!(dock.height(), single_line.height());
    assert!(dock.width() > 360.0);
    let scale = notification_scale(screen.size());
    assert!(dock.width() <= MAX_NOTIFICATION_WIDTH * scale);
}

#[test]
fn public_world_toast_centers_planets_and_moons_without_opening_hidden_information() {
    let mut model = GameModel::new([9; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = &model.players[0];
    let planet = model.players[1].home_planet;
    let moon = model.map.planets.iter().find(|world| world.is_moon()).unwrap().id;
    for world in [planet, moon] {
        let mut state = UiState {
            planet_selected: Some(player.home_planet),
            mission: true,
            combat_report: Some(3),
            ..default()
        };

        assert!(focus_planet(world, &model.map, &mut state));
        assert_eq!(state.planet_selected, None);
        assert_eq!(state.focus_planet, Some(world));
        assert!(state.to_selected);
        assert!(!state.mission);
        assert_eq!(state.combat_report, None);
    }

    let mut state = UiState {
        to_selected: false,
        ..default()
    };
    assert!(!focus_planet(usize::MAX, &model.map, &mut state));
    assert!(!state.to_selected);
    model.map.get_mut(planet).is_destroyed = true;
    assert!(!focus_planet(planet, &model.map, &mut state));
}

#[test]
fn revoked_protection_toast_opens_a_home_destination_draft_until_fleet_leaves() {
    let mut model = GameModel::new([11; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let local = model.players[0].id;
    let home = model.players[0].home_planet;
    let protected = model.players[1].home_planet;
    let controller = model.players[1].id;
    model.players[0].protection_intel.insert(protected, controller);
    let fighter = Unit::Ship(Ship::LightFighter);
    model.map.get_mut(protected).army.dock_protector(local, Army::from([(fighter, 2)]));
    let player = &model.players[0];
    let planet = model.map.get(protected);
    assert!(revoked_protection_fleet(planet, player));

    let mut state = UiState::default();
    assert!(open_revoked_protection_mission(protected, &model.map, player, &mut state));
    assert_eq!(state.focus_planet, Some(protected));
    assert!(state.to_selected);
    assert!(state.mission);
    assert_eq!(state.mission_tab, MissionTab::NewMission);
    assert_eq!((state.mission_info.origin, state.mission_info.destination), (protected, home));
    assert_eq!(state.mission_info.objective, Icon::Deploy);

    model.map.get_mut(protected).army.remove_protector(local);
    assert!(!revoked_protection_fleet(model.map.get(protected), &model.players[0]));
    assert!(!open_revoked_protection_mission(protected, &model.map, &model.players[0], &mut state));
}

#[test]
fn railgun_toast_focuses_and_fully_zooms_out_even_when_the_target_was_destroyed() {
    let mut model = GameModel::new([10; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let target = model
        .map
        .planets
        .iter()
        .find(|planet| planet.is_moon())
        .map(|planet| planet.id)
        .expect("the generated map should contain a moon");
    model.map.get_mut(target).is_destroyed = true;
    let mut state = UiState {
        planet_selected: Some(model.players[0].home_planet),
        mission: true,
        combat_report: Some(3),
        ..default()
    };

    assert!(focus_railgun_target(target, &model.map, &mut state));
    assert_eq!(state.planet_selected, None);
    assert_eq!(state.focus_planet, Some(target));
    assert_eq!(state.focus_zoom, Some(MAX_ZOOM));
    assert!(state.to_selected);
    assert!(!state.mission);
    assert_eq!(state.combat_report, None);
}

#[test]
fn enemy_detection_toast_opens_the_enemy_missions_panel() {
    let mut state = UiState {
        planet_selected: Some(4),
        mission_tab: MissionTab::MissionReports,
        combat_report: Some(9),
        ..default()
    };

    open_enemy_missions(&mut state);

    assert_eq!(state.planet_selected, None);
    assert!(state.mission);
    assert_eq!(state.mission_tab, MissionTab::EnemyMissions);
    assert_eq!(state.combat_report, None);
}
