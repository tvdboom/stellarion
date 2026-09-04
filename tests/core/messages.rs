use super::*;
use crate::core::simulation::{GameModel, GameRules};

fn notification_frame(
    context: &egui::Context,
    messages: &Messages,
    screen: egui::Rect,
    events: Vec<egui::Event>,
) -> Vec<egui::Rect> {
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(screen),
            events,
            ..default()
        },
        |ui| {
            draw_notifications(ui.ctx(), messages, true);
        },
    );
    output.textures_delta.clear();
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.corner_radius == egui::CornerRadius::same(5) => {
                Some(rect.rect)
            },
            _ => None,
        })
        .collect()
}

#[test]
fn notifications_stack_separately_and_shrink_when_long_messages_expire() {
    let context = egui::Context::default();
    let long = "Battle at planet Ganymede ended in a draw; the attacking fleet is returning to its planet of origin.";
    for size in [egui::vec2(1600.0, 900.0), egui::vec2(320.0, 320.0), egui::vec2(360.0, 640.0)] {
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
        assert!(rects[1].top() >= rects[0].bottom() + 5.0, "boxes must not overlap: {rects:?}");
        assert!((rects[0].right() - rects[1].right()).abs() < 1.0);
        assert!(
            rects[1].width() < rects[0].width() * 0.6,
            "short toast inherited long toast width: {rects:?}"
        );
        assert!(rects[1].height() < rects[0].height());

        messages.0.pop_front();
        for _ in 0..3 {
            notification_frame(&context, &messages, screen, vec![]);
        }
        let compact = notification_frame(&context, &messages, screen, vec![]);
        assert_eq!(compact.len(), 1);
        assert!((compact[0].top() - 70.0).abs() < 1.0);
        assert!((compact[0].width() - rects[1].width()).abs() < 1.0);
        let area =
            context.memory(|m| m.area_rect(egui::Id::new("stellarion_notifications"))).unwrap();
        assert!(
            (area.width() - compact[0].width()).abs() < 1.0,
            "expired toast left a fixed-width area"
        );
        assert!((area.height() - compact[0].height()).abs() < 1.0);
    }
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
                |ui| clicked = draw_notifications(ui.ctx(), &messages, true),
            );
            output.textures_delta.clear();
            clicked
        };
        frame(vec![]);
        frame(vec![]);
        let rect = context
            .memory(|memory| memory.area_rect(egui::Id::new("stellarion_notifications")))
            .unwrap();
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
            |ui| clicked = draw_notifications(ui.ctx(), &messages, true),
        );
        output.textures_delta.clear();
        clicked
    };
    frame(vec![]);
    frame(vec![]);
    let rect = context
        .memory(|memory| memory.area_rect(egui::Id::new("stellarion_notifications")))
        .unwrap();
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
