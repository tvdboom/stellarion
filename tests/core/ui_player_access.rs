use super::*;
use crate::core::identity::{GameCode, GameId, UserId};
use crate::core::simulation::{GameModel, GameRules, PersistedGame};
use crate::multiplayer::model::{GameMembership, GameRecord};

fn session() -> MultiplayerSession {
    let model = GameModel::new(
        [23; 32],
        GameRules {
            player_count: 4,
            ..default()
        },
    )
    .unwrap();
    let members = model
        .players
        .iter()
        .map(|player| GameMembership {
            game_id: GameId::new("access"),
            player_id: player.id,
            user_id: UserId::new(format!("player-{}", player.id)),
            display_name: format!("Player {}", player.id),
            is_creator: player.id == 1,
            identity_version: 1,
            connected: true,
        })
        .collect::<Vec<_>>();
    let mut session = MultiplayerSession::default();
    session.membership = Some(members[0].clone());
    session.active_game = Some(GameRecord {
        id: GameId::new("access"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: 4,
        status: model.status,
        persisted: PersistedGame::new(model),
        members,
        submitted_players: vec![],
    });
    session
}

fn text_rect(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == label => Some(text.visual_bounding_rect()),
        _ => None,
    })
}

#[test]
fn hover_picker_crosses_gap_toggles_rows_and_closes_when_left_at_multiple_scales() {
    for (size, scale) in [(egui::vec2(900.0, 650.0), 1.0), (egui::vec2(360.0, 340.0), 0.7)] {
        let session = session();
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut selected = BTreeSet::new();
        let mut pass = 0;
        let mut screen_icon = egui::Rect::NOTHING;
        let mut frame =
            |selected: &mut BTreeSet<PlayerId>, events: Vec<egui::Event>, editable: bool| {
                pass += 1;
                let mut changes = vec![];
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                        time: Some(pass as f64 * 0.1),
                        events,
                        ..default()
                    },
                    |context| {
                        draw_panel_on_context(
                            context,
                            "access test",
                            "panel",
                            (20.0, 90.0),
                            (80.0, 90.0),
                            0.0,
                            scale,
                            &ImageIds::default(),
                            |ui| {
                                let rect = egui::Rect::from_min_size(
                                    ui.min_rect().min + egui::vec2(18.0, 25.0),
                                    egui::vec2(25.0, 25.0),
                                );
                                let response = ui
                                    .interact(rect, ui.id().with("access icon"), Sense::hover())
                                    .on_hover_cursor(CursorIcon::Default);
                                assert!(!response.clicked());
                                screen_icon = context
                                    .layer_transform_to_global(ui.layer_id())
                                    .unwrap_or_default()
                                    .mul_rect(rect);
                                let (_, edits) = draw_player_access_picker(
                                    &response,
                                    "Manage protection access",
                                    "Choose players.",
                                    &session,
                                    1,
                                    selected,
                                    editable,
                                );
                                changes = edits;
                            },
                        );
                    },
                );
                for (player, allowed) in &changes {
                    if *allowed {
                        selected.insert(*player);
                    } else {
                        selected.remove(player);
                    }
                }
                output.textures_delta.clear();
                (output, changes, screen_icon)
            };
        let (output, _, _) = frame(&mut selected, vec![], true);
        assert!(text_rect(&output, "Manage protection access").is_none());
        let (_, _, icon) = frame(&mut selected, vec![], true);
        frame(&mut selected, vec![egui::Event::PointerMoved(icon.center())], true);
        let (output, _, _) = frame(&mut selected, vec![], true);
        assert_eq!(output.platform_output.cursor_icon, CursorIcon::Default);
        let title = text_rect(&output, "Manage protection access").unwrap();
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(title));
        assert!(text_rect(&output, "Player 1").is_none());
        let row = text_rect(&output, "Player 2").unwrap();
        assert!(
            row.left() > icon.right() || row.right() < icon.left(),
            "picker must open beside the icon"
        );
        let bridge = egui::pos2((icon.right() + title.left()) * 0.5, icon.center().y);
        frame(&mut selected, vec![egui::Event::PointerMoved(bridge)], true);
        frame(&mut selected, vec![egui::Event::PointerMoved(row.center())], true);
        let button = |pressed| egui::Event::PointerButton {
            pos: row.center(),
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&mut selected, vec![button(true)], true);
        let (output, changes, _) = frame(&mut selected, vec![button(false)], true);
        assert_eq!(changes, vec![(2, true)]);
        assert!(text_rect(&output, "Manage protection access").is_some());
        frame(&mut selected, vec![button(true)], true);
        let (_, changes, _) = frame(&mut selected, vec![button(false)], true);
        assert_eq!(changes, vec![(2, false)]);
        frame(&mut selected, vec![button(true)], false);
        let (_, changes, _) = frame(&mut selected, vec![button(false)], false);
        assert!(changes.is_empty());
        frame(
            &mut selected,
            vec![egui::Event::PointerMoved(size.to_pos2() - egui::vec2(5.0, 5.0))],
            true,
        );
        frame(&mut selected, vec![], true);
        frame(&mut selected, vec![], true);
        let (output, _, _) = frame(&mut selected, vec![], true);
        assert!(text_rect(&output, "Manage protection access").is_none());
    }
}

#[test]
fn picker_filters_spectators_and_preserves_player_colors() {
    let mut session = session();
    session.active_game.as_mut().unwrap().persisted.state.players[2].spectator = true;
    let context = egui::Context::default();
    let mut output = egui::FullOutput::default();
    for _ in 0..3 {
        output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(500.0, 300.0),
                )),
                events: vec![egui::Event::PointerMoved(egui::pos2(22.0, 22.0))],
                ..default()
            },
            |ui| {
                let response = ui.interact(
                    egui::Rect::from_min_size(egui::pos2(10.0, 10.0), egui::vec2(25.0, 25.0)),
                    ui.id().with("access"),
                    Sense::hover(),
                );
                draw_player_access_picker(
                    &response,
                    "Manage friendly players",
                    "Observe without combat.",
                    &session,
                    1,
                    &[2].into(),
                    true,
                );
            },
        );
        output.textures_delta.clear();
    }
    assert!(text_rect(&output, "Player 1").is_none());
    assert!(text_rect(&output, "Player 3").is_none());
    for player in [2, 4] {
        assert!(text_rect(&output, &format!("Player {player}")).is_some());
        let color = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == format!("Player {player}") => {
                    text.galley.job.sections.first().map(|section| section.format.color)
                },
                _ => None,
            })
            .unwrap();
        assert_eq!(color, session.player_color(player).color().to_color32());
    }
}

#[test]
fn short_viewport_scrolls_player_rows_and_flips_picker_from_the_right_edge() {
    let session = session();
    let context = egui::Context::default();
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(360.0, 180.0));
    let icon = egui::Rect::from_min_size(egui::pos2(325.0, 120.0), egui::vec2(25.0, 25.0));
    let mut last_row = egui::Rect::NOTHING;
    let mut visible_after_scroll = false;
    for frame in 0..7 {
        let pointer = if frame < 3 {
            icon.center()
        } else {
            egui::pos2(180.0, 110.0)
        };
        let mut events = vec![egui::Event::PointerMoved(pointer)];
        if frame == 4 {
            events.push(egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -250.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            });
        }
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                events,
                time: Some(frame as f64 * 0.1),
                ..default()
            },
            |ui| {
                let response = ui.interact(icon, ui.id().with("edge access"), Sense::hover());
                draw_player_access_picker(
                    &response,
                    "Manage friendly players",
                    "Choose players.",
                    &session,
                    1,
                    &BTreeSet::new(),
                    true,
                );
            },
        );
        output.textures_delta.clear();
        if frame >= 2 {
            let title = text_rect(&output, "Manage friendly players").unwrap();
            assert!(viewport.contains_rect(title));
            assert!(title.right() < icon.left(), "picker must flip beside the right edge");
        }
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                if text.galley.text() == "Player 4" {
                    let row = text.visual_bounding_rect();
                    if frame == 2 {
                        last_row = row;
                        assert!(!shape.clip_rect.contains(row.center()));
                    } else if frame == 6 {
                        visible_after_scroll = shape.clip_rect.contains(row.center());
                        assert!(row.top() < last_row.top());
                    }
                }
            }
        }
    }
    assert!(visible_after_scroll, "scrolling should reveal the last player");
}
