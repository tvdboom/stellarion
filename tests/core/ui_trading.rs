use super::*;
use crate::core::simulation::{GameModel, GameRules};
use std::collections::BTreeMap;

fn trading_context() -> egui::Context {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
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

fn trading_panel_frame(
    context: &egui::Context,
    world: &mut World,
    state: &mut UiState,
    model: &GameModel,
    size: egui::Vec2,
    events: Vec<egui::Event>,
) -> egui::FullOutput {
    trading_ui_frame(
        context,
        world,
        state,
        model,
        &MultiplayerSession::default(),
        size,
        events,
        false,
    )
}

fn trading_ui_frame(
    context: &egui::Context,
    world: &mut World,
    state: &mut UiState,
    model: &GameModel,
    session: &MultiplayerSession,
    size: egui::Vec2,
    events: Vec<egui::Event>,
    notifications: bool,
) -> egui::FullOutput {
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<MultiplayerRequest>,
        MessageWriter<MessageMsg>,
    )>::new(world);
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..default()
        },
        |_| {
            let (mut requests, mut messages) = params.get_mut(world).unwrap();
            let draw = if notifications {
                draw_trade_notifications
            } else {
                draw_trade_panel
            };
            draw(
                context,
                state,
                &model.map,
                &model.players[0],
                session,
                &mut requests,
                &mut messages,
                &ImageIds(
                    ResourceName::iter()
                        .enumerate()
                        .map(|(index, resource)| {
                            (
                                resource.to_lowername().to_owned(),
                                egui::TextureId::User(index as u64 + 1),
                            )
                        })
                        .collect(),
                ),
            );
        },
    );
    output.textures_delta.clear();
    output
}

fn trade_invitation(model: &GameModel) -> TradeInvitation {
    TradeInvitation {
        id: 19,
        revision: 0,
        turn: model.turn,
        proposer: model.players[1].id,
        canceled: false,
        finalized: false,
        participants: [
            TradeParticipant {
                player_id: model.players[0].id,
                planet_id: model.players[0].home_planet,
                resources: Resources::new(100, 200, 300),
                response: TradeResponse::Pending,
            },
            TradeParticipant {
                player_id: model.players[1].id,
                planet_id: model.players[1].home_planet,
                resources: Resources::new(300, 200, 100),
                response: TradeResponse::Accepted,
            },
        ],
    }
}

#[test]
fn trade_edits_wait_for_send_offer_and_accepted_offers_withdraw_once() {
    let mut model = trading_panel_game();
    for player in &mut model.players {
        player.resources = Resources::new(10_000, 10_000, 10_000);
    }
    let local_home = model.players[0].home_planet;
    model.map.get_mut(local_home).army.insert(Unit::Building(Building::TradingPost), 3);
    let mut session = MultiplayerSession::default();
    session.trades.push(trade_invitation(&model));
    let mut state = UiState {
        trade_open: Some(19),
        ..default()
    };
    let context = trading_context();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let size = egui::vec2(640.0, 600.0);
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    world.resource_mut::<Messages<MultiplayerRequest>>().clear();
    state.trade_resources.metal += 10;
    // Adjusting a pending offer, including during a drag, stays local.
    let events = vec![egui::Event::PointerButton {
        pos: egui::pos2(10.0, 10.0),
        button: egui::PointerButton::Primary,
        pressed: true,
        modifiers: egui::Modifiers::NONE,
    }];
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, events, false);
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(requests.is_empty());
    state.trade_resources.metal = 120;
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    let output =
        trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    let send = panel_labels(&output)["Send offer"].center();
    for pressed in [false, true, false] {
        trading_ui_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            &session,
            size,
            vec![
                egui::Event::PointerMoved(send),
                egui::Event::PointerButton {
                    pos: send,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
            false,
        );
    }
    assert_eq!(state.trade_open, Some(19));
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(
        requests.as_slice(),
        [MultiplayerRequest::RespondTrade {
            expected_revision: 0,
            response: TradeResponse::Accepted,
            resources,
            ..
        }] if resources.metal == 120
    ));
    session.trades[0].participants[0].resources = state.trade_resources;
    session.trades[0].participants[0].response = TradeResponse::Accepted;
    state.trade_resources.metal = 130;
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::RespondTrade {
        resources, response: TradeResponse::Pending, ..
    }] if resources.metal == 120));
    session.trades[0].participants[0].response = TradeResponse::Pending;
    state.trade_resources.metal = 140;
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    state.trade_open = None;
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], true);
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    assert_eq!(state.trade_draft_id, Some(19));
}

#[test]
fn closing_a_trade_keeps_an_unsent_draft_local() {
    let model = trading_panel_game();
    let mut session = MultiplayerSession::default();
    session.trades.push(trade_invitation(&model));
    session.trade_update_pending = true;
    let original = session.trades[0].participants[0].resources;
    let mut state = UiState {
        trade_draft_id: Some(19),
        trade_resources: original,
        ..default()
    };
    let context = trading_context();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let size = egui::vec2(640.0, 600.0);
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], true);
    assert_eq!(state.trade_draft_id, Some(19));
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());

    // A server update while the panel is closed never publishes the local draft.
    session.trade_update_pending = false;
    session.trades[0].revision = 1;
    session.trades[0].participants[0].resources.metal += 50;
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], true);
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    assert_eq!(state.trade_resources, original);
    assert_eq!(state.trade_draft_id, Some(19));
}

#[test]
fn trade_review_statuses_fit_without_scrolling() {
    for size in [egui::vec2(640.0, 480.0), egui::vec2(360.0, 640.0)] {
        for (finalized, canceled, pending) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (false, false, true),
        ] {
            let model = trading_panel_game();
            let mut trade = trade_invitation(&model);
            trade.finalized = finalized;
            trade.canceled = canceled;
            let mut session = MultiplayerSession::default();
            session.trades.push(trade);
            session.trade_update_pending = pending;
            let mut state = UiState {
                trade_open: Some(19),
                ..default()
            };
            let context = trading_context();
            let mut world = World::new();
            world.init_resource::<Messages<MultiplayerRequest>>();
            world.init_resource::<Messages<MessageMsg>>();
            trading_ui_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                &session,
                size,
                Vec::new(),
                false,
            );
            let output = trading_ui_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                &session,
                size,
                Vec::new(),
                false,
            );
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape {
                    let rect = text.galley.rect.translate(text.pos.to_vec2());
                    assert!(
                        shape.clip_rect.contains_rect(rect),
                        "{} must fit at {size:?} (finalized={finalized}): {rect:?}, clip {:?}",
                        text.galley.text(),
                        shape.clip_rect
                    );
                }
            }
        }
    }
}

#[test]
fn trade_badges_show_each_players_role_and_response() {
    for local_index in [0, 1] {
        for (proposer_response, recipient_response, canceled, finalized, expected) in [
            (
                TradeResponse::Accepted,
                TradeResponse::Pending,
                false,
                false,
                ["Proposed", "Pending"],
            ),
            (
                TradeResponse::Accepted,
                TradeResponse::Rejected,
                true,
                false,
                ["Proposed", "Rejected"],
            ),
            (
                TradeResponse::Pending,
                TradeResponse::Accepted,
                false,
                false,
                ["Pending", "Confirmed"],
            ),
            (
                TradeResponse::Accepted,
                TradeResponse::Accepted,
                false,
                true,
                ["Accepted", "Accepted"],
            ),
        ] {
            let mut model = trading_panel_game();
            let mut trade = trade_invitation(&model);
            let proposer = trade.proposer;
            let recipient = model.players[0].id;
            trade.participants[1].response = proposer_response;
            trade.participants[0].response = recipient_response;
            trade.canceled = canceled;
            trade.finalized = finalized;
            let mut session = MultiplayerSession::default();
            session.trades.push(trade);
            model.players.swap(0, local_index);
            let mut state = UiState {
                trade_open: Some(19),
                ..default()
            };
            let context = trading_context();
            let mut world = World::new();
            world.init_resource::<Messages<MultiplayerRequest>>();
            world.init_resource::<Messages<MessageMsg>>();
            let size = egui::vec2(640.0, 480.0);
            trading_ui_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                &session,
                size,
                vec![],
                false,
            );
            let output = trading_ui_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                &session,
                size,
                vec![],
                false,
            );
            let labels = panel_labels(&output);
            for (player_id, badge) in [(proposer, expected[0]), (recipient, expected[1])] {
                let heading = labels[&format!("Player {player_id}")];
                let matching = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == badge => {
                            Some(text.galley.rect.translate(text.pos.to_vec2()))
                        },
                        _ => None,
                    })
                    .filter(|rect| (rect.center().y - heading.center().y).abs() < 2.0)
                    .count();
                assert_eq!(matching, 1, "Player {player_id} should show {badge}");
            }
        }
    }
}

#[test]
fn proposer_cannot_accept_until_the_other_player_offers_resources() {
    let mut model = trading_panel_game();
    model.players[1].resources = Resources::new(1000, 1000, 1000);
    let mut trade = trade_invitation(&model);
    trade.participants[0].resources = Resources::default();
    trade.participants[1].resources = Resources::new(150, 0, 0);
    let mut session = MultiplayerSession::default();
    session.trades.push(trade);
    model.players.swap(0, 1);
    let mut state = UiState {
        trade_open: Some(19),
        ..default()
    };
    let context = trading_context();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let size = egui::vec2(640.0, 480.0);
    let frame = |world: &mut World, state: &mut UiState, session: &MultiplayerSession, events| {
        trading_ui_frame(&context, world, state, &model, session, size, events, false)
    };
    frame(&mut world, &mut state, &session, vec![]);
    let output = frame(&mut world, &mut state, &session, vec![]);
    let accept = panel_labels(&output)["Accept"].center();
    for pressed in [true, false] {
        frame(
            &mut world,
            &mut state,
            &session,
            vec![
                egui::Event::PointerMoved(accept),
                egui::Event::PointerButton {
                    pos: accept,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: default(),
                },
            ],
        );
    }
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());

    session.trades[0].participants[0].resources = Resources::new(100, 0, 0);
    session.trades[0].participants[1].response = TradeResponse::Pending;
    frame(&mut world, &mut state, &session, vec![]);
    for pressed in [true, false] {
        frame(
            &mut world,
            &mut state,
            &session,
            vec![
                egui::Event::PointerMoved(accept),
                egui::Event::PointerButton {
                    pos: accept,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: default(),
                },
            ],
        );
    }
    assert!(world.resource_mut::<Messages<MultiplayerRequest>>().drain().any(|request| {
        matches!(
            request,
            MultiplayerRequest::RespondTrade {
                trade_id: 19,
                response: TradeResponse::Accepted,
                ..
            }
        )
    }));
}

#[test]
fn confirmed_trades_close_the_matching_panel_and_emit_one_two_second_toast_for_either_player() {
    for local_index in [0, 1] {
        for panel_open in [false, true] {
            let mut model = trading_panel_game();
            for player in &mut model.players {
                player.resources = Resources::new(100_000, 100_000, 100_000);
            }
            let mut trade = trade_invitation(&model);
            let other_player = model.players[1 - local_index].id;
            trade.participants[local_index].response = TradeResponse::Accepted;
            trade.participants[1 - local_index].response = TradeResponse::Pending;
            let resources = trade.participants[local_index].resources;
            model.players.swap(0, local_index);
            let mut session = MultiplayerSession::default();
            session.trades.push(trade);
            let mut state = UiState {
                trade_open: panel_open.then_some(19),
                trade_draft_id: Some(19),
                trade_resources: resources,
                ..default()
            };
            let context = trading_context();
            let mut world = World::new();
            world.init_resource::<Messages<MultiplayerRequest>>();
            world.init_resource::<Messages<MessageMsg>>();
            let size = egui::vec2(640.0, 480.0);

            // One player's acceptance still waits for the other side.
            trading_ui_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                &session,
                size,
                Vec::new(),
                true,
            );
            assert_eq!(state.trade_open, panel_open.then_some(19));
            assert!(world.resource::<Messages<MessageMsg>>().is_empty());

            session.trades[0].finalized = true;
            for participant in &mut session.trades[0].participants {
                participant.response = TradeResponse::Accepted;
            }
            for _ in 0..3 {
                let output = trading_ui_frame(
                    &context,
                    &mut world,
                    &mut state,
                    &model,
                    &session,
                    size,
                    Vec::new(),
                    true,
                );
                assert_eq!(state.trade_open, None);
                assert_eq!(state.trading_post_open, None);
                assert_eq!(state.trade_draft_id, None);
                assert!(panel_labels(&output).is_empty(), "no modal or persistent trade toast");
            }
            assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
            let notices = world.resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
            assert_eq!(notices.len(), 1);
            assert_eq!(notices[0].message, format!("Trade with Player {other_player} successful."));
            assert_eq!(notices[0].level, crate::core::messages::MessageLevel::Info);
            assert_eq!(notices[0].action, Some(MessageAction::OpenTrade(19)));
            assert_eq!(notices[0].display_duration, Some(std::time::Duration::from_secs(2)));
        }
    }
}

#[test]
fn rejected_trades_emit_one_normal_timed_notification_without_buttons() {
    let model = trading_panel_game();
    let mut trade = trade_invitation(&model);
    trade.canceled = true;
    trade.participants[1].response = TradeResponse::Rejected;
    let mut session = MultiplayerSession::default();
    session.trades.push(trade);
    let context = trading_context();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let mut state = UiState::default();
    for _ in 0..3 {
        let output = trading_ui_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            &session,
            egui::vec2(640.0, 480.0),
            Vec::new(),
            true,
        );
        assert!(!panel_labels(&output).contains_key("Dismiss"));
        assert!(panel_labels(&output).is_empty());
    }
    let messages = world.resource::<Messages<MessageMsg>>();
    let mut cursor = messages.get_cursor();
    let notices = cursor.read(messages).collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].message, "The trade with Player 2 was rejected.");
    assert_eq!(notices[0].level, crate::core::messages::MessageLevel::Info);
    assert!(notices[0].action.is_none());
    assert!(notices[0].display_duration.is_none());
}

#[test]
fn toast_buttons_change_only_color_on_hover() {
    for label in ["Open", "Reject", "Review", "Dismiss"] {
        let context = trading_context();
        let render = |position: egui::Pos2| {
            let mut button = egui::Rect::NOTHING;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(640.0, 480.0),
                    )),
                    events: vec![egui::Event::PointerMoved(position)],
                    ..default()
                },
                |ui| {
                    trade_toast(ui, "Player 2 proposed a resource trade.", |ui| {
                        button = trade_button(ui, label, true).rect;
                    });
                },
            );
            output.textures_delta.clear();
            (button, output)
        };
        render(egui::pos2(600.0, 450.0));
        let (rest, normal) = render(egui::pos2(600.0, 450.0));
        render(rest.center());
        let (hover, hovered) = render(rest.center());
        assert_eq!(rest, hover, "{label} must keep its hitbox");
        assert_eq!(panel_labels(&normal), panel_labels(&hovered));
        let frames = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) => {
                        Some((rect.rect, rect.corner_radius, rect.stroke.width))
                    },
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            frames(&normal),
            frames(&hovered),
            "{label} must keep the button and toast geometry"
        );
        let button_fill = |output: &egui::FullOutput| {
            output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.contains(rest.center()) && rect.rect.width() < 200.0 =>
                {
                    Some(rect.fill)
                },
                _ => None,
            })
        };
        assert_ne!(button_fill(&normal), button_fill(&hovered), "{label} must still change color");
    }
}

fn panel_labels(output: &egui::FullOutput) -> BTreeMap<String, egui::Rect> {
    output
        .shapes
        .iter()
        .filter_map(|shape| {
            if let egui::Shape::Text(text) = &shape.shape {
                Some((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                ))
            } else {
                None
            }
        })
        .collect()
}

fn resource_image_rects(output: &egui::FullOutput) -> Vec<egui::Rect> {
    output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.brush.as_ref().is_some_and(|brush| {
                    matches!(brush.fill_texture_id, egui::TextureId::User(1..=3))
                }) =>
            {
                Some(rect.rect)
            },
            egui::Shape::Mesh(mesh) if matches!(mesh.texture_id, egui::TextureId::User(1..=3)) => {
                Some(mesh.calc_bounds())
            },
            _ => None,
        })
        .collect()
}

fn assert_trade_resource_borders(output: &egui::FullOutput, scale: f32) {
    let borders = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.stroke.color == RESOURCE_IMAGE_BORDER_COLOR
                    && (rect.stroke.width - 2.0 * scale).abs() < 0.1 =>
            {
                Some(rect.rect)
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    let images = resource_image_rects(output);
    assert_eq!(borders.len(), 6);
    assert_eq!(images.len(), 6);
    for image in images {
        assert!(borders.iter().any(|border| border.contains_rect(image)));
    }
}

fn assert_resource_art_is_full_brightness(output: &egui::FullOutput) {
    // The modal fades in as a whole; white with its current alpha still leaves art untinted.
    let untinted = |color: Color32| color == Color32::from_white_alpha(color.a());
    for shape in &output.shapes {
        match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.brush.as_ref().is_some_and(|brush| {
                    matches!(brush.fill_texture_id, egui::TextureId::User(1..=3))
                }) =>
            {
                assert!(untinted(rect.fill));
            },
            egui::Shape::Mesh(mesh) if matches!(mesh.texture_id, egui::TextureId::User(1..=3)) => {
                assert!(mesh.vertices.iter().all(|vertex| untinted(vertex.color)));
            },
            _ => {},
        }
    }
}

#[test]
fn trade_headings_keep_the_sender_first_with_each_players_name_and_color() {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::simulation::PersistedGame;
    use crate::multiplayer::model::{GameMembership, GameRecord};

    for local_index in [0, 1] {
        let mut model = trading_panel_game();
        let trade = trade_invitation(&model);
        let sender = trade.proposer;
        let recipient =
            trade.participants.iter().find(|p| p.player_id != sender).unwrap().player_id;
        let mut session = MultiplayerSession::default();
        session.active_game = Some(GameRecord {
            id: GameId::new("trade-layout"),
            code: GameCode::new("ABCDEF"),
            revision: 0,
            saved_at: 0,
            max_players: model.players.len() as u8,
            status: model.status,
            persisted: PersistedGame::new(model.clone()),
            members: model
                .players
                .iter()
                .map(|player| GameMembership {
                    game_id: GameId::new("trade-layout"),
                    player_id: player.id,
                    user_id: UserId::new(format!("user-{}", player.id)),
                    display_name: format!("Practice P{}", player.id),
                    is_creator: player.id == 1,
                    identity_version: 1,
                    connected: true,
                })
                .collect(),
            submitted_players: Vec::new(),
        });
        session.trades.push(trade);
        model.players.swap(0, local_index);
        let mut state = UiState {
            trade_open: Some(19),
            ..default()
        };
        let context = trading_context();
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<MessageMsg>>();
        let size = egui::vec2(640.0, 600.0);
        trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
        let output = trading_ui_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            &session,
            size,
            vec![],
            false,
        );
        let labels = panel_labels(&output);
        assert!(
            labels[&format!("Practice P{sender}")].bottom()
                < labels[&format!("Practice P{recipient}")].top()
        );
        assert!(!labels.keys().any(|label| label.contains("offer")));
        let own_label = labels[&format!("Practice P{}", model.players[0].id)];
        let summary = labels.iter().find(|(label, _)| label.ends_with(" selected")).unwrap().1;
        assert!(own_label.bottom() < summary.top());
        if model.players[0].id == sender {
            assert!(summary.bottom() < labels[&format!("Practice P{recipient}")].top());
        }
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                for id in [sender, recipient] {
                    if text.galley.text() == format!("Practice P{id}") {
                        assert!(text.galley.job.sections.iter().all(|section| {
                            section.format.color == session.player_color(id).color().to_color32()
                        }));
                    }
                }
                if text.galley.text().ends_with(" total")
                    || text.galley.text().ends_with(" selected")
                {
                    assert!(text.galley.job.sections.iter().all(|section| section
                        .format
                        .font_id
                        .size
                        == 15.0));
                }
            }
        }
    }
}

#[test]
fn resource_tiles_select_all_stock_and_clear_only_the_clicked_type() {
    for size in [egui::vec2(542.0, 546.0), egui::vec2(360.0, 640.0)] {
        let modal_scale = game_modal_scale(size, egui::vec2(560.0, 402.0));
        let mut model = trading_panel_game();
        model.players[0].resources = Resources::new(3200, 1700, 0);
        let mut session = MultiplayerSession::default();
        let mut trade = trade_invitation(&model);
        trade.participants[0].resources = Resources::new(100, 200, 0);
        session.trades.push(trade);
        let mut state = UiState {
            trade_open: Some(19),
            ..default()
        };
        let context = trading_context();
        context.global_style_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<MessageMsg>>();
        for _ in 0..2 {
            trading_ui_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                &session,
                size,
                vec![],
                false,
            );
        }
        for (index, resource) in ResourceName::iter().enumerate() {
            // Egui suppresses tooltips for movement immediately following a click.
            for _ in 0..8 {
                trading_ui_frame(
                    &context,
                    &mut world,
                    &mut state,
                    &model,
                    &session,
                    size,
                    vec![],
                    false,
                );
            }
            for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary] {
                let output = trading_ui_frame(
                    &context,
                    &mut world,
                    &mut state,
                    &model,
                    &session,
                    size,
                    vec![],
                    false,
                );
                assert_resource_art_is_full_brightness(&output);
                assert_trade_resource_borders(&output, modal_scale);
                let other_position = resource_image_rects(&output)[index].center();
                let mut read_only = output;
                for _ in 0..3 {
                    read_only = trading_ui_frame(
                        &context,
                        &mut world,
                        &mut state,
                        &model,
                        &session,
                        size,
                        vec![egui::Event::PointerMoved(other_position)],
                        false,
                    );
                }
                assert_eq!(read_only.platform_output.cursor_icon, CursorIcon::Default);
                let position = resource_image_rects(&read_only)[index + 3].center();
                let mut hovered = read_only;
                for _ in 0..3 {
                    hovered = trading_ui_frame(
                        &context,
                        &mut world,
                        &mut state,
                        &model,
                        &session,
                        size,
                        vec![egui::Event::PointerMoved(position)],
                        false,
                    );
                }
                assert_eq!(hovered.platform_output.cursor_icon, CursorIcon::PointingHand);
                assert_resource_art_is_full_brightness(&hovered);
                assert_trade_resource_borders(&hovered, modal_scale);
                if button == egui::PointerButton::Primary {
                    assert!(!panel_labels(&hovered).contains_key(&resource.to_name()));
                }
                let mut expected = state.trade_resources;
                *expected.get_mut(&resource) = if button == egui::PointerButton::Primary {
                    model.players[0].resources.get(&resource)
                } else {
                    0
                };
                for pressed in [true, false] {
                    trading_ui_frame(
                        &context,
                        &mut world,
                        &mut state,
                        &model,
                        &session,
                        size,
                        vec![
                            egui::Event::PointerMoved(position),
                            egui::Event::PointerButton {
                                pos: position,
                                button,
                                pressed,
                                modifiers: default(),
                            },
                        ],
                        false,
                    );
                }
                assert_eq!(state.trade_resources, expected, "{resource:?}, {button:?}, {size:?}");
                assert_eq!(state.trade_open, Some(19));
                assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
            }
        }
    }
}

#[test]
fn settled_trade_resource_tiles_do_not_change_historical_amounts() {
    for canceled in [false, true] {
        let mut model = trading_panel_game();
        model.players[0].resources = Resources::default();
        let mut session = MultiplayerSession::default();
        let mut trade = trade_invitation(&model);
        trade.finalized = !canceled;
        trade.canceled = canceled;
        let expected = trade.participants[0].resources;
        session.trades.push(trade);
        let mut state = UiState {
            trade_open: Some(19),
            ..default()
        };
        let context = trading_context();
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<MessageMsg>>();
        let size = egui::vec2(640.0, 600.0);
        trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
        let output = trading_ui_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            &session,
            size,
            vec![],
            false,
        );
        assert_resource_art_is_full_brightness(&output);
        for rect in resource_image_rects(&output).into_iter().skip(3) {
            for button in [egui::PointerButton::Primary, egui::PointerButton::Secondary] {
                for pressed in [true, false] {
                    trading_ui_frame(
                        &context,
                        &mut world,
                        &mut state,
                        &model,
                        &session,
                        size,
                        vec![
                            egui::Event::PointerMoved(rect.center()),
                            egui::Event::PointerButton {
                                pos: rect.center(),
                                button,
                                pressed,
                                modifiers: default(),
                            },
                        ],
                        false,
                    );
                }
                assert_eq!(state.trade_resources, expected);
            }
        }
        assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    }
}

#[test]
fn scaled_trade_panels_keep_offers_and_footer_visible() {
    let mut model = trading_panel_game();
    model.players[0].resources = Resources::new(1000, 1000, 1000);
    let mut session = MultiplayerSession::default();
    session.trades.push(trade_invitation(&model));
    let mut state = UiState {
        trade_open: Some(19),
        ..default()
    };
    let context = trading_context();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let size = egui::vec2(640.0, 360.0);
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    let before =
        trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    let before_labels = panel_labels(&before);
    let selected_is_visible = |output: &egui::FullOutput| {
        output.shapes.iter().any(|shape| matches!(
            &shape.shape, egui::Shape::Text(text)
                if text.galley.text() == "600 / 1000 selected"
                    && shape.clip_rect.contains_rect(text.galley.rect.translate(text.pos.to_vec2()))
        ))
    };
    assert!(selected_is_visible(&before));
    let scroll_position = resource_image_rects(&before)[0].center();
    let mut after = before;
    let mut selected_remained_visible = true;
    for _ in 0..10 {
        after = trading_ui_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            &session,
            size,
            vec![
                egui::Event::PointerMoved(scroll_position),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -100.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: default(),
                },
            ],
            false,
        );
        selected_remained_visible &= selected_is_visible(&after);
    }
    let after_labels = panel_labels(&after);
    for action in ["Close", "Reject", "Accept"] {
        assert_eq!(before_labels[action], after_labels[action]);
    }
    assert!(
        selected_remained_visible,
        "the last offer must remain visible: before={before_labels:?}, after={after_labels:?}"
    );
}

fn trading_panel_game() -> GameModel {
    let mut model = GameModel::new([31; 32], GameRules::default()).unwrap();
    let home = model.players[0].home_planet;
    let enemy = model.players[1].home_planet;
    model.map.get_mut(home).position = Vec2::ZERO;
    model.map.get_mut(enemy).position = Vec2::X * Planet::SIZE * 3.0;
    model.map.get_mut(home).army.insert(Unit::Building(Building::TradingPost), 2);
    model.map.get_mut(enemy).army.insert(Unit::Building(Building::TradingPost), 1);
    model
}

#[test]
fn projected_trading_post_cannot_offer_until_it_exists_in_the_saved_turn() {
    use crate::core::identity::{GameCode, GameId};
    use crate::core::simulation::PersistedGame;
    use crate::multiplayer::model::GameRecord;

    let mut model = trading_panel_game();
    let enemy = model.players[1].home_planet;
    model.map.get_mut(enemy).position = Vec2::X * Planet::SIZE * 1.5;
    let mut saved = model.clone();
    let home = model.players[0].home_planet;
    saved.map.get_mut(home).army.remove(&Unit::Building(Building::TradingPost));
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: GameId::new("unsaved-post"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: model.players.len() as u8,
        status: model.status,
        persisted: PersistedGame::new(saved),
        members: Vec::new(),
        submitted_players: Vec::new(),
    });
    let mut state = UiState {
        trading_post_open: Some(enemy),
        ..default()
    };
    let context = trading_context();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let size = egui::vec2(562.0, 441.0);
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    let output =
        trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    let labels = panel_labels(&output);
    assert!(labels.keys().any(|label| label.starts_with("This route is not available")));
    assert!(!labels.contains_key("Send offer"));
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());

    let game = session.active_game.as_mut().unwrap();
    game.persisted = PersistedGame::new(model.clone());
    game.submitted_players.push(model.players[1].id);
    let output =
        trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    let labels = panel_labels(&output);
    assert!(labels.keys().any(|label| label.starts_with("One of the players")));
    assert!(!labels.contains_key("Send offer"));
}

#[test]
fn closing_an_unsent_trade_then_sending_a_new_offer_uses_only_the_new_draft() {
    use crate::core::identity::{GameCode, GameId};
    use crate::core::simulation::PersistedGame;
    use crate::multiplayer::model::GameRecord;

    let mut model = trading_panel_game();
    model.start().unwrap();
    let home = model.players[0].home_planet;
    let enemy = model.players[1].home_planet;
    let alternate = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.id != home && planet.id != enemy)
        .unwrap()
        .id;
    model.players[0].resources = Resources::new(10_000, 10_000, 10_000);
    for planet_id in [home, enemy, alternate] {
        let planet = model.map.get_mut(planet_id);
        planet.army.insert(Unit::Building(Building::TradingPost), 3);
        if planet_id != home {
            planet.owned = Some(model.players[1].id);
            planet.position = Vec2::X * Planet::SIZE * 4.5;
        }
    }
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: GameId::new("unsent-trade"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: model.players.len() as u8,
        status: model.status,
        persisted: PersistedGame::new(model.clone()),
        members: Vec::new(),
        submitted_players: Vec::new(),
    });
    for next_post in [enemy, alternate] {
        let context = trading_context();
        let size = egui::vec2(640.0, 600.0);
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<MessageMsg>>();
        let mut state = UiState {
            trading_post_open: Some(enemy),
            ..default()
        };
        let frame = |state: &mut UiState, world: &mut World, events| {
            trading_ui_frame(&context, world, state, &model, &session, size, events, true)
        };
        let click = |state: &mut UiState, world: &mut World, label: &str| {
            frame(state, world, vec![]);
            let position = panel_labels(&frame(state, world, vec![]))[label].center();
            for pressed in [true, false] {
                frame(
                    state,
                    world,
                    vec![
                        egui::Event::PointerMoved(position),
                        egui::Event::PointerButton {
                            pos: position,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: default(),
                        },
                    ],
                );
            }
        };
        frame(&mut state, &mut world, vec![]);
        state.trade_resources = Resources::new(321, 0, 0);
        click(&mut state, &mut world, "Close");
        assert!(state.trading_post_open.is_none());
        assert!(state.trade_open.is_none());
        assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());

        state.trading_post_open = Some(next_post);
        frame(&mut state, &mut world, vec![]);
        assert_eq!(state.trade_resources, Resources::default());
        let resources = Resources::new(100, 0, 0);
        state.trade_resources = resources;
        click(&mut state, &mut world, "Send offer");
        let requests =
            world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
        let [MultiplayerRequest::CreateTrade(invitation)] = requests.as_slice() else {
            panic!("only the second draft should create an offer");
        };
        assert!(invitation.id > 0);
        assert_eq!(invitation.turn, model.turn);
        assert_eq!(invitation.revision, 0);
        assert_eq!(invitation.proposer, model.players[0].id);
        assert!(!invitation.canceled && !invitation.finalized);
        assert_eq!(invitation.participants[0].planet_id, home);
        assert_eq!(invitation.participants[0].resources, resources);
        assert_eq!(invitation.participants[0].response, TradeResponse::Accepted);
        assert_eq!(invitation.participants[1].planet_id, next_post);
        assert_eq!(invitation.participants[1].resources, Resources::default());
        assert_eq!(invitation.participants[1].response, TradeResponse::Pending);
    }
}

#[test]
fn any_visible_post_of_the_same_player_reopens_the_accepted_trade_until_the_turn_ends() {
    use crate::core::identity::{GameCode, GameId};
    use crate::core::simulation::PersistedGame;
    use crate::multiplayer::model::GameRecord;

    let mut model = trading_panel_game();
    model.start().unwrap();
    model.turn = 5;
    let player_id = model.players[0].id;
    let other_id = model.players[1].id;
    let home = model.players[0].home_planet;
    let enemy = model.players[1].home_planet;
    model.map.get_mut(enemy).position = Vec2::X * Planet::SIZE * 1.5;
    let alternate = model.map.planets.iter().find(|planet| planet.owned.is_none()).unwrap().id;
    let post = model.map.get_mut(alternate);
    post.owned = Some(other_id);
    post.position = Vec2::X * Planet::SIZE * 3.0;
    post.army.insert(Unit::Building(Building::TradingPost), 1);
    assert_eq!(
        visible_trading_post_owner(&model.map, player_id, model.map.get(alternate)),
        Some(other_id)
    );
    assert!(trading_posts_are_adjacent(&model.map, player_id, home, other_id, alternate));

    let mut accepted = trade_invitation(&model);
    accepted.finalized = true;
    for participant in &mut accepted.participants {
        participant.response = TradeResponse::Accepted;
    }
    let mut previous_turn = accepted.clone();
    previous_turn.id += 1;
    previous_turn.turn -= 1;
    let mut session = MultiplayerSession::default();
    session.trades = vec![previous_turn, accepted.clone()];
    session.active_game = Some(GameRecord {
        id: GameId::new("trade-review"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: model.players.len() as u8,
        status: model.status,
        persisted: PersistedGame::new(model.clone()),
        members: Vec::new(),
        submitted_players: Vec::new(),
    });
    // Reserved resources must not clamp the historical offer when it is reopened.
    model.players[0].resources = Resources::default();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let size = egui::vec2(640.0, 480.0);

    for post in [enemy, alternate] {
        let context = trading_context();
        let mut state = UiState {
            trading_post_open: Some(post),
            ..default()
        };
        trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], true);
        let output = trading_ui_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            &session,
            size,
            vec![],
            true,
        );
        let labels = panel_labels(&output);
        assert_eq!(state.trade_open, Some(accepted.id));
        assert_eq!(state.trading_post_open, None);
        assert_eq!(state.trade_resources, accepted.participant(player_id).unwrap().resources);
        assert!(labels.contains_key("Accepted"));
        assert_eq!(
            output
                .shapes
                .iter()
                .filter(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Accepted"))
                .count(),
            2,
        );
        assert!(labels.contains_key("Close"));
        for action in ["Send offer", "Accept", "Reject"] {
            assert!(!labels.contains_key(action), "a settled trade must only allow review");
        }
    }
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());

    session.active_game.as_mut().unwrap().persisted.state.turn += 1;
    let context = trading_context();
    let mut state = UiState {
        trading_post_open: Some(enemy),
        ..default()
    };
    trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    let output =
        trading_ui_frame(&context, &mut world, &mut state, &model, &session, size, vec![], false);
    assert_eq!(state.trade_open, None);
    assert!(panel_labels(&output).contains_key("Send offer"));
    assert!(!panel_labels(&output).contains_key("Accepted"));
}

#[test]
fn unavailable_saved_trade_route_panel_explains_and_closes_at_small_sizes() {
    use crate::core::identity::{GameCode, GameId};
    use crate::core::simulation::PersistedGame;
    use crate::multiplayer::model::GameRecord;

    for size in [egui::vec2(1280.0, 800.0), egui::vec2(640.0, 480.0), egui::vec2(360.0, 640.0)] {
        let model = trading_panel_game();
        let enemy = model.players[1].home_planet;
        let mut saved = model.clone();
        saved
            .map
            .get_mut(model.players[0].home_planet)
            .army
            .remove(&Unit::Building(Building::TradingPost));
        let mut session = MultiplayerSession::default();
        session.active_game = Some(GameRecord {
            id: GameId::new("unavailable-route"),
            code: GameCode::new("ABCDEF"),
            revision: 0,
            saved_at: 0,
            max_players: model.players.len() as u8,
            status: model.status,
            persisted: PersistedGame::new(saved),
            members: Vec::new(),
            submitted_players: Vec::new(),
        });
        let mut state = UiState {
            trading_post_open: Some(enemy),
            ..default()
        };
        let context = trading_context();
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<MessageMsg>>();
        trading_ui_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            &session,
            size,
            Vec::new(),
            false,
        );
        let output = trading_ui_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            &session,
            size,
            Vec::new(),
            false,
        );
        let labels = panel_labels(&output);
        let message = "This route is not available in the saved turn yet. Complete both Trading Posts and advance the turn before trading.";
        assert!(labels.contains_key(message));
        assert!(!labels.contains_key("Send offer"));
        assert!(!labels.contains_key("Player 1"));
        assert_eq!(state.trading_post_open, Some(enemy));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let panel_height = 280.0 * game_modal_scale(size, egui::vec2(560.0, 280.0));
        let panel_top = (size.y - panel_height) * 0.5;
        let bar_bottom = panel_top + panel_height * TRADE_PANEL_TOP_BAR_FRACTION;
        assert!(labels["Trading Post"].top() >= panel_top - 2.0);
        assert!(labels["Trading Post"].bottom() <= bar_bottom + 2.0);
        for text in ["Trading Post", message, "Close"] {
            assert!(screen.contains_rect(labels[text]), "{text} must fit at {size:?}");
        }
        assert!(labels["Trading Post"].bottom() < labels[message].top());
        assert!(labels[message].bottom() < labels["Close"].top());

        let position = labels["Close"].center();
        for pressed in [true, false] {
            trading_ui_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                &session,
                size,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: default(),
                    },
                ],
                false,
            );
        }
        assert_eq!(state.trading_post_open, None);
        assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    }
}

#[test]
fn trading_panel_opens_when_either_post_reaches_the_other() {
    let mut model = trading_panel_game();
    let home = model.players[0].home_planet;
    let enemy = model.players[1].home_planet;
    let mut state = UiState {
        trading_post_open: Some(enemy),
        ..default()
    };
    let context = trading_context();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    for (distance, level, owned, visible, can_trade) in [
        (1.5, 0, true, false, false),
        (1.51, 1, true, false, false),
        (1.5, 1, false, false, false),
        (3.0, 2, true, true, true),
        (3.01, 2, true, false, false),
        (1.49, 1, true, true, true),
        (1.5, 1, true, true, true),
    ] {
        state.trading_post_open = Some(enemy);
        model.map.get_mut(enemy).position = Vec2::X * Planet::SIZE * distance;
        let local = model.map.get_mut(home);
        local.owned = owned.then_some(1);
        local.army.insert(Unit::Building(Building::TradingPost), level);
        local.buy = if level > 0 {
            Vec::new()
        } else {
            vec![Unit::Building(Building::TradingPost)]
        };
        trading_panel_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            egui::vec2(640.0, 480.0),
            Vec::new(),
        );
        let output = trading_panel_frame(
            &context,
            &mut world,
            &mut state,
            &model,
            egui::vec2(640.0, 480.0),
            Vec::new(),
        );
        let labels = panel_labels(&output);
        assert_eq!(labels.contains_key("Player 1"), can_trade);
        assert_eq!(labels.contains_key("Send offer"), can_trade);
        assert_eq!(
            labels.keys().any(|text| text.starts_with("You need a completed Trading Post")),
            visible && !can_trade
        );
        if can_trade {
            assert!(labels["Trading Post"].bottom() < labels["Player 1"].top());
        }
        assert_eq!(state.trading_post_open, visible.then_some(enemy));
    }
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
}

#[test]
fn trading_offer_and_footer_fit_inside_the_panel_at_small_sizes() {
    for size in [
        egui::vec2(542.0, 546.0),
        egui::vec2(640.0, 480.0),
        egui::vec2(562.0, 441.0),
        egui::vec2(360.0, 640.0),
    ] {
        let mut model = trading_panel_game();
        let home = model.players[0].home_planet;
        let enemy = model.players[1].home_planet;
        model.map.get_mut(home).army.insert(Unit::Building(Building::TradingPost), 1);
        model.map.get_mut(enemy).position = Vec2::X * Planet::SIZE * 1.5;
        let mut state = UiState {
            trading_post_open: Some(enemy),
            ..default()
        };
        let context = trading_context();
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<MessageMsg>>();
        trading_panel_frame(&context, &mut world, &mut state, &model, size, Vec::new());
        let output =
            trading_panel_frame(&context, &mut world, &mut state, &model, size, Vec::new());
        let labels = panel_labels(&output);
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let modal_scale = game_modal_scale(size, egui::vec2(560.0, 402.0));
        let panel_height = 402.0 * modal_scale;
        let panel_top = (size.y - panel_height) * 0.5 + TRADE_PANEL_VERTICAL_OFFSET * modal_scale;
        let bar_bottom = panel_top + panel_height * TRADE_PANEL_TOP_BAR_FRACTION;
        assert!(labels["Trading Post"].top() >= panel_top - 2.0);
        assert!(labels["Trading Post"].bottom() <= bar_bottom + 2.0);
        for text in ["Trading Post", "Player 1", "Player 2", "Close", "Send offer"] {
            assert!(screen.contains_rect(labels[text]), "{text} must fit at {size:?}");
        }
        assert!(!labels
            .keys()
            .any(|text| text.contains('↔') || text.starts_with("Your outgoing limit:")));
        let icons = resource_image_rects(&output);
        let inputs = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "0" => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                },
                _ => None,
            })
            .collect::<Vec<_>>();
        assert_eq!(icons.len(), 6);
        assert_eq!(inputs.len(), 6);
        assert!(labels["Player 1"].left() <= icons[0].left());
        assert!((labels["Player 1"].center().y - labels["Draft"].center().y).abs() < 2.0);
        assert!(labels["Trading Post"].bottom() + 20.0 * modal_scale <= labels["Player 1"].top());
        assert!(labels["Player 1"].bottom() < labels["Player 2"].top());
        for (index, (icon, input)) in icons.iter().zip(&inputs).take(3).enumerate() {
            assert!(screen.contains_rect(*icon));
            assert!(screen.contains_rect(*input));
            assert!((icon.center().y - input.center().y).abs() < 2.0);
            assert!(icon.right() < input.left());
            assert!(labels["Player 1"].bottom() + 8.0 * modal_scale <= icon.top());
            assert!(icon.bottom() + 10.0 * modal_scale <= labels["0 / 500 selected"].top());
            assert!(input.bottom() < labels["Close"].top());
            if index > 0 {
                assert!(inputs[index - 1].right() < icon.left());
                assert!((icons[index - 1].center().y - icon.center().y).abs() < 1.0);
            }
        }
        assert!(labels["Draft"].left() > icons[2].center().x);
        for shape in &output.shapes {
            if let egui::Shape::Text(text) = &shape.shape {
                let rect = text.galley.rect.translate(text.pos.to_vec2());
                assert!(
                    shape.clip_rect.contains_rect(rect),
                    "{} must not be clipped at {size:?}: {rect:?}",
                    text.galley.text()
                );
            }
        }
        let panel_bottom =
            (size.y + panel_height) * 0.5 + TRADE_PANEL_VERTICAL_OFFSET * modal_scale;
        assert!(labels["Close"].bottom() < panel_bottom - 12.0 * modal_scale);
        assert!(labels["Send offer"].bottom() < panel_bottom - 12.0 * modal_scale);
        for pressed in [true, false] {
            let position = labels["Send offer"].center();
            trading_panel_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                size,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: default(),
                    },
                ],
            );
        }
        assert_eq!(state.trading_post_open, Some(enemy));
        for pressed in [true, false] {
            let position = labels["Close"].center();
            trading_panel_frame(
                &context,
                &mut world,
                &mut state,
                &model,
                size,
                vec![
                    egui::Event::PointerMoved(position),
                    egui::Event::PointerButton {
                        pos: position,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: default(),
                    },
                ],
            );
        }
        assert_eq!(state.trading_post_open, None);
        assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    }
}
