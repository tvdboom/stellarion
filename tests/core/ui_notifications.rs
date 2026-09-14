use super::*;
use crate::core::audio::PlayAudioMsg;
use crate::core::messages::MessagesPlugin;
use crate::core::states::AppState;
use crate::multiplayer::model::{TradeInvitation, TradeParticipant, TradeResponse};
use bevy_egui::{EguiContext, EguiPrimaryContextPass, EguiUserTextures, PrimaryEguiContext};

#[test]
fn negotiation_toasts_are_shown_once_per_player_across_ui_rebuilds() {
    let (model, _, _, mut invitation) = fixture();
    invitation.canceled = true;
    invitation.participants.push(JointAttackParticipant {
        player_id: 3,
        response: JointAttackResponse::Pending,
        contribution: None,
    });
    let mut session = session_for_model(&model);
    session.local_practice = true;
    session.joint_attacks.push(invitation.clone());
    invitation.id = 93;
    invitation.canceled = false;
    invitation.inviter = 1;
    invitation.participants[0].response = JointAttackResponse::Rejected;
    session.joint_attacks.push(invitation);
    session.trades.push(TradeInvitation {
        id: 92,
        revision: 0,
        turn: model.turn,
        proposer: 1,
        canceled: true,
        finalized: false,
        participants: [1, 3].map(|player_id| TradeParticipant {
            player_id,
            planet_id: model.player(player_id).unwrap().home_planet,
            resources: Resources::default(),
            response: TradeResponse::Rejected,
        }),
    });
    let context = egui::Context::default();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<MultiplayerRequest>,
        MessageWriter<MessageMsg>,
    )>::new(&mut world);
    for (player_id, expected) in [(1, 3), (2, 0), (3, 2), (1, 0), (3, 0), (1, 0)] {
        let player = model.player(player_id).unwrap();
        let mut state = UiState::default();
        let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
            let (mut requests, mut messages) = params.get_mut(&mut world).unwrap();
            draw_joint_attack_notifications(
                ctx,
                &mut state,
                &model.map,
                player,
                &session,
                &mut requests,
                &mut messages,
                &ImageIds::default(),
            );
            draw_trade_notifications(
                ctx,
                &mut state,
                &model.map,
                player,
                &session,
                &mut requests,
                &mut messages,
                &ImageIds::default(),
            );
        });
        output.textures_delta.clear();
        let notices = world.resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
        assert_eq!(notices.len(), expected, "player {player_id}");
        if expected > 0 {
            assert!(notices[0].message.contains("canceled the allied attack"));
            assert!(notices.last().unwrap().message.contains("was rejected"));
        }
    }
    session.active_game.as_mut().unwrap().persisted.state.turn += 1;
    assert!(!first_negotiation_notice(
        &context,
        &session,
        1,
        NegotiationNotice::MissionCanceled(91)
    ));
    session.active_game.as_mut().unwrap().id = crate::core::identity::GameId::new("another-game");
    assert!(first_negotiation_notice(
        &context,
        &session,
        1,
        NegotiationNotice::MissionCanceled(91)
    ));
}

#[test]
fn mission_trade_and_rejection_toasts_keep_separate_padded_frames() {
    for size in [egui::vec2(1600.0, 900.0), egui::vec2(560.0, 460.0), egui::vec2(320.0, 640.0)] {
        let toast_scale = crate::core::messages::notification_scale(size);
        let (mut model, player, mut state, mut invitation) = fixture();
        model.map.get_mut(invitation.destination).name = "Io".into();
        invitation.inviter = player.id;
        invitation.participants.reverse();
        invitation.participants[0].response = JointAttackResponse::Accepted;
        invitation.participants[1].response = JointAttackResponse::Rejected;
        state.joint_attack_open = None;
        let mut session = session_for_model(&model);
        session.joint_attacks.push(invitation);
        session.trades.push(TradeInvitation {
            id: 92,
            revision: 0,
            turn: model.turn,
            proposer: 2,
            canceled: false,
            finalized: false,
            participants: [player.id, 2].map(|player_id| TradeParticipant {
                player_id,
                planet_id: model.players[(player_id - 1) as usize].home_planet,
                resources: Resources::default(),
                response: TradeResponse::Pending,
            }),
        });
        let mut app = App::new();
        app.init_resource::<EguiUserTextures>()
            .init_resource::<Time>()
            .insert_resource(State::new(AppState::Game))
            .insert_resource(State::new(GameState::Playing))
            .add_message::<MessageMsg>()
            .add_message::<PlayAudioMsg>()
            .add_message::<MultiplayerRequest>()
            .add_plugins(MessagesPlugin);
        let mut egui_context = EguiContext::default();
        let context = egui_context.get_mut().clone();
        context.set_global_style(NordDark.custom_style());
        context.add_font(egui::epaint::text::FontInsert::new(
            "firasans",
            FontData::from_static(include_bytes!("../../assets/fonts/FiraSans-Bold.ttf")),
            vec![egui::epaint::text::InsertFontFamily {
                family: FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Highest,
            }],
        ));
        app.world_mut().spawn((egui_context, PrimaryEguiContext));
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        let frame = |app: &mut App, state: &mut UiState, events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..default()
                },
                |_| {
                    let mut params = bevy::ecs::system::SystemState::<(
                        MessageWriter<MultiplayerRequest>,
                        MessageWriter<MessageMsg>,
                    )>::new(app.world_mut());
                    let (mut requests, mut messages) = params.get_mut(app.world_mut()).unwrap();
                    draw_joint_attack_notifications(
                        &context,
                        state,
                        &model.map,
                        &player,
                        &session,
                        &mut requests,
                        &mut messages,
                        &ImageIds::default(),
                    );
                    draw_trade_notifications(
                        &context,
                        state,
                        &model.map,
                        &player,
                        &session,
                        &mut requests,
                        &mut messages,
                        &ImageIds::default(),
                    );
                    app.world_mut().run_schedule(EguiPrimaryContextPass);
                },
            );
            output.textures_delta.clear();
            output
        };
        for _ in 0..3 {
            frame(&mut app, &mut state, vec![]);
        }
        let output = frame(&mut app, &mut state, vec![]);
        let labels = [
            "Your allied attack mission on Io is waiting.",
            "Player 2 proposed a resource trade.",
            "Player 2 rejected the joint attack on Io.",
        ];
        let frames = labels.map(|label| {
            let text = text_bounds(&output, label).unwrap();
            let rect = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.corner_radius == egui::CornerRadius::same(5) * toast_scale
                            && rect.rect.contains_rect(text) =>
                    {
                        Some(rect.rect)
                    },
                    _ => None,
                })
                .unwrap();
            assert!(screen.contains_rect(rect), "toast outside {size:?}: {rect:?}");
            assert!(
                rect.contains_rect(text.expand2(egui::vec2(12.0, 8.0) * toast_scale)),
                "missing text padding at {size:?}: {label}, frame {rect:?}, text {text:?}"
            );
            rect
        });
        for pair in frames.windows(2) {
            assert!(
                pair[1].top() >= pair[0].bottom() + 6.0 * toast_scale - 0.2,
                "overlap at {size:?}: {frames:?}"
            );
            assert!((pair[0].right() - pair[1].right()).abs() < 1.0);
        }

        let reopen = text_bounds(&output, "Reopen mission").unwrap().center();
        frame(&mut app, &mut state, pointer_click(reopen, true));
        frame(&mut app, &mut state, pointer_click(reopen, false));
        assert!(state.mission, "the mission toast must remain clickable at {size:?}");
        // Opening its editor removes the owner toast; the remaining stack closes the gap.
        for _ in 0..3 {
            frame(&mut app, &mut state, vec![]);
        }
        let trade = context.memory(|memory| memory.area_rect("trade_notifications")).unwrap();
        let trade = context
            .layer_transform_to_global(egui::LayerId::new(
                egui::Order::Tooltip,
                egui::Id::new("trade_notifications"),
            ))
            .unwrap_or(egui::emath::TSTransform::IDENTITY)
            .mul_rect(trade);
        assert!((trade.top() - frames[0].top()).abs() < 1.0, "stale gap at {size:?}: {trade:?}");

        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(30));
        let output = frame(&mut app, &mut state, vec![]);
        assert!(text_bounds(&output, labels[2]).is_none(), "the rejection toast must expire");
    }
}
