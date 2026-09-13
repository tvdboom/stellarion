use super::*;
use crate::core::simulation::{GameModel, GameRules};

#[path = "ui_notifications.rs"]
mod notifications;

fn text_bounds(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == label => {
            Some(egui::Rect::from_min_size(text.pos, text.galley.size()))
        },
        _ => None,
    })
}

const INVITE_ICON_TEXTURE: egui::TextureId = egui::TextureId::User(91);

fn invite_icon(output: &egui::FullOutput) -> (egui::Rect, Color32) {
    output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == INVITE_ICON_TEXTURE => {
                Some((mesh.calc_bounds(), mesh.vertices[0].color))
            },
            _ => None,
        })
        .expect("allied attack invite icon")
}

fn fixture() -> (GameModel, Player, UiState, JointAttackInvitation) {
    let mut model = GameModel::new(
        [81; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let mut player = model.players[0].clone();
    player.resources.deuterium = 1_000_000;
    let army = Army::from([(Unit::probe(), 5), (Unit::Ship(Ship::Bomber), 5)]);
    model.map.get_mut(player.home_planet).army.extend(army.clone());
    let contribution = JointAttackContribution {
        player_id: player.id,
        origin: player.home_planet,
        army: army.clone(),
        bombing: BombingRaid::Economic,
        combat_probes: true,
    };
    let invitation = JointAttackInvitation {
        id: 91,
        revision: 0,
        turn: model.turn,
        inviter: 2,
        destination: model.players[2].home_planet,
        objective: Icon::Attack,
        bombing: BombingRaid::None,
        combat_probes: false,
        canceled: false,
        launched: false,
        participants: vec![
            JointAttackParticipant {
                player_id: 2,
                response: JointAttackResponse::Accepted,
                contribution: Some(JointAttackContribution {
                    player_id: 2,
                    origin: model.players[1].home_planet,
                    ..contribution.clone()
                }),
            },
            JointAttackParticipant {
                player_id: player.id,
                response: JointAttackResponse::Pending,
                contribution: Some(contribution),
            },
        ],
    };
    let state = UiState {
        joint_attack_open: Some(invitation.id),
        joint_attack_contribution: Mission {
            origin: player.home_planet,
            destination: invitation.destination,
            objective: invitation.objective,
            army,
            bombing: BombingRaid::Economic,
            combat_probes: true,
            ..default()
        },
        ..default()
    };
    (model, player, state, invitation)
}

fn session_for_model(model: &GameModel) -> MultiplayerSession {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::simulation::PersistedGame;
    use crate::multiplayer::model::{GameMembership, GameRecord};
    let members = model
        .players
        .iter()
        .map(|player| GameMembership {
            game_id: GameId("fixture".into()),
            player_id: player.id,
            user_id: UserId(format!("user-{}", player.id)),
            display_name: format!("Player {}", player.id),
            is_creator: player.id == 1,
            identity_version: 1,
            connected: true,
        })
        .collect::<Vec<_>>();
    let mut session = MultiplayerSession::default();
    session.membership = Some(members[0].clone());
    session.active_game = Some(GameRecord {
        id: GameId("fixture".into()),
        code: GameCode("ABCDEF".into()),
        revision: 0,
        saved_at: 0,
        max_players: model.players.len() as u8,
        status: model.status,
        persisted: PersistedGame::new(model.clone()),
        members,
        submitted_players: vec![],
    });
    session
}

fn pointer_click(pos: egui::Pos2, pressed: bool) -> Vec<egui::Event> {
    vec![
        egui::Event::PointerMoved(pos),
        egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        },
    ]
}

fn owner_frame(
    context: &egui::Context,
    world: &mut World,
    model: &mut GameModel,
    player: &mut Player,
    state: &mut UiState,
    session: &MultiplayerSession,
    size: egui::Vec2,
    events: Vec<egui::Event>,
    keyboard: &ButtonInput<KeyCode>,
) -> egui::FullOutput {
    let images = ImageIds([(Icon::AlliedAttack.asset_key(), INVITE_ICON_TEXTURE)].into());
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<MultiplayerRequest>,
        MessageWriter<SendMissionMsg>,
    )>::new(world);
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
            events,
            ..default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (mut requests, mut sends) = params.get_mut(world).unwrap();
                draw_new_mission(
                    ui,
                    &mut sends,
                    &[],
                    &Settings::default(),
                    state,
                    &mut model.map,
                    player,
                    session,
                    &mut requests,
                    false,
                    keyboard,
                    &images,
                );
            });
        },
    );
    output.textures_delta.clear();
    output
}

#[test]
fn invite_picker_confirms_once_or_discards_changes_without_launching_the_mission() {
    for size in [egui::vec2(1040.0, 800.0), egui::vec2(560.0, 460.0)] {
        for confirm in [true, false] {
            let (mut model, mut player, mut state, _) = fixture();
            state.mission_info = state.joint_attack_contribution.clone();
            let session = session_for_model(&model);
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let mut world = World::new();
            world.init_resource::<Messages<MultiplayerRequest>>();
            world.init_resource::<Messages<SendMissionMsg>>();
            let mut frame = |state: &mut UiState, events, keyboard: &ButtonInput<KeyCode>| {
                owner_frame(
                    &context,
                    &mut world,
                    &mut model,
                    &mut player,
                    state,
                    &session,
                    size,
                    events,
                    keyboard,
                )
            };
            let keyboard = ButtonInput::default();
            frame(&mut state, vec![], &keyboard);
            let output = frame(&mut state, vec![], &keyboard);
            assert!(text_bounds(&output, "Invite players").is_none());
            let (invite, idle_tint) = invite_icon(&output);
            let send = text_bounds(&output, "Send mission").unwrap();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            assert!(screen.contains_rect(invite), "invite footer at {size:?}: {invite:?}");
            assert!(screen.contains_rect(send), "send footer at {size:?}: {send:?}");
            assert!(invite.right() < send.left());
            assert!(invite.left() < 30.0, "invite stays at the bottom left");
            assert_eq!(invite.size(), egui::Vec2::splat(40.0));
            assert!(invite.y_range().contains(send.center().y));
            assert!(text_bounds(&output, "Allied mission").is_none());
            let hovered =
                frame(&mut state, vec![egui::Event::PointerMoved(invite.center())], &keyboard);
            let (hovered_rect, hover_tint) = invite_icon(&hovered);
            assert_eq!(hovered_rect, invite);
            assert!(hover_tint.r() > idle_tint.r(), "invite brightens on hover");
            frame(&mut state, pointer_click(invite.center(), true), &keyboard);
            frame(&mut state, pointer_click(invite.center(), false), &keyboard);
            let output = frame(&mut state, vec![], &keyboard);
            assert!(state.joint_attack_invite_selection.is_some());
            assert!(text_bounds(&output, "Player 1").is_none());
            let row = text_bounds(&output, "Player 2").unwrap();
            frame(&mut state, pointer_click(row.center(), true), &keyboard);
            let output = frame(&mut state, pointer_click(row.center(), false), &keyboard);
            assert!(state.joint_attack_invitees.is_empty());
            assert_eq!(state.joint_attack_invite_selection, Some([2].into()));
            let mut enter = ButtonInput::default();
            enter.press(KeyCode::Enter);
            frame(&mut state, vec![], &enter);
            let button = text_bounds(
                &output,
                if confirm {
                    "Confirm"
                } else {
                    "Cancel"
                },
            )
            .unwrap();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            assert!(screen.contains_rect(button));
            frame(&mut state, pointer_click(button.center(), true), &keyboard);
            frame(&mut state, pointer_click(button.center(), false), &keyboard);
            assert!(state.joint_attack_invite_selection.is_none());
            assert_eq!(state.allied_mission, confirm);
            let before_sync = world.resource_mut::<Messages<MultiplayerRequest>>().drain().count();
            assert_eq!(before_sync, 0, "selection must not publish before confirmation");
            owner_frame(
                &context,
                &mut world,
                &mut model,
                &mut player,
                &mut state,
                &session,
                size,
                vec![],
                &keyboard,
            );
            let requests =
                world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
            assert_eq!(requests.len(), usize::from(confirm));
            if confirm {
                assert!(matches!(&requests[0], MultiplayerRequest::CreateJointAttack(invitation)
                    if invitation.participants.iter().map(|p| p.player_id).collect::<Vec<_>>() == [1, 2]
                    && invitation.participants[1].response == JointAttackResponse::Pending));
            }
            assert_eq!(world.resource_mut::<Messages<SendMissionMsg>>().drain().count(), 0);
        }
    }
}

#[test]
fn invite_picker_keeps_confirmed_players_selected_while_new_choices_remain_editable() {
    for size in [egui::vec2(1040.0, 800.0), egui::vec2(560.0, 460.0)] {
        let (model, player, mut state, _) = fixture();
        state.mission_info = state.joint_attack_contribution.clone();
        state.allied_mission = true;
        state.joint_attack_invitees = [2].into();
        state.joint_attack_invite_selection = Some(state.joint_attack_invitees.clone());
        let session = session_for_model(&model);
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let frame = |state: &mut UiState, events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..default()
                },
                |ctx| {
                    let destination = model.map.get(state.mission_info.destination);
                    draw_joint_attack_invite_modal(
                        ctx,
                        state,
                        &session,
                        &player,
                        destination,
                        &ImageIds::default(),
                        true,
                    );
                },
            );
            output.textures_delta.clear();
            output
        };
        frame(&mut state, vec![]);
        let output = frame(&mut state, vec![]);
        let confirmed = text_bounds(&output, "Player 2").unwrap();
        let available = text_bounds(&output, "Player 3").unwrap();
        let confirm = text_bounds(&output, "Confirm").unwrap();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        for rect in [confirmed, available, confirm] {
            assert!(screen.contains_rect(rect), "invite picker at {size:?}: {rect:?}");
        }

        frame(&mut state, pointer_click(confirmed.center(), true));
        frame(&mut state, pointer_click(confirmed.center(), false));
        assert_eq!(state.joint_attack_invite_selection, Some([2].into()));

        // New choices can still be undone until the user confirms them.
        for expected in [vec![2, 3], vec![2], vec![2, 3]] {
            frame(&mut state, pointer_click(available.center(), true));
            frame(&mut state, pointer_click(available.center(), false));
            assert_eq!(state.joint_attack_invite_selection, Some(expected.into_iter().collect()));
            assert_eq!(state.joint_attack_invitees, [2].into());
        }
        frame(&mut state, pointer_click(confirm.center(), true));
        frame(&mut state, pointer_click(confirm.center(), false));
        assert!(state.joint_attack_invite_selection.is_none());
        assert_eq!(state.joint_attack_invitees, [2, 3].into());
        assert!(state.allied_mission);

        state.joint_attack_invite_selection = Some(state.joint_attack_invitees.clone());
        frame(&mut state, vec![]);
        let output = frame(&mut state, vec![]);
        for label in ["Player 2", "Player 3"] {
            let row = text_bounds(&output, label).unwrap();
            frame(&mut state, pointer_click(row.center(), true));
            frame(&mut state, pointer_click(row.center(), false));
            assert_eq!(state.joint_attack_invite_selection, Some([2, 3].into()));
        }
    }
}

#[test]
fn closed_owner_mission_has_a_toast_that_reopens_the_preserved_draft() {
    let (model, player, mut state, mut invitation) = fixture();
    invitation.inviter = player.id;
    invitation.participants.reverse();
    state.joint_attack_open = None;
    state.joint_attack_draft_id = Some(invitation.id);
    state.allied_mission = true;
    state.joint_attack_invitees = [2].into();
    state.joint_attack_owner_draft = Some(state.joint_attack_contribution.clone());
    let expected = state.joint_attack_owner_draft.clone();
    let mut session = session_for_model(&model);
    session.joint_attacks.push(invitation);
    let context = egui::Context::default();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<MultiplayerRequest>,
        MessageWriter<MessageMsg>,
    )>::new(&mut world);
    let mut reopen = None;
    for step in 0..5 {
        let events = if (2..4).contains(&step) {
            pointer_click(reopen.unwrap(), step == 2)
        } else {
            vec![]
        };
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1040.0, 800.0),
                )),
                events,
                ..default()
            },
            |ctx| {
                let (mut requests, mut messages) = params.get_mut(&mut world).unwrap();
                draw_joint_attack_notifications(
                    ctx,
                    &mut state,
                    &model.map,
                    &player,
                    &session,
                    &mut requests,
                    &mut messages,
                    &ImageIds::default(),
                );
            },
        );
        output.textures_delta.clear();
        if step == 1 {
            reopen = Some(text_bounds(&output, "Reopen mission").unwrap().center());
        }
        if step == 4 {
            assert!(text_bounds(&output, "Reopen mission").is_none());
        }
    }
    assert!(state.mission);
    assert_eq!(state.mission_tab, MissionTab::NewMission);
    assert_eq!(
        serde_json::to_value(&state.joint_attack_owner_draft).unwrap(),
        serde_json::to_value(&expected).unwrap()
    );
    assert_eq!(world.resource_mut::<Messages<MultiplayerRequest>>().drain().count(), 0);
    state.mission = false;
    state.joint_attack_owner_draft.as_mut().unwrap().army.insert(Unit::war_sun(), 17);
    let inspected_origin = model.players[2].home_planet;
    state.mission_info.origin = inspected_origin;
    for pending in [true, false] {
        session.joint_attack_update_pending = pending;
        let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
            let (mut requests, mut messages) = params.get_mut(&mut world).unwrap();
            draw_joint_attack_notifications(
                ctx,
                &mut state,
                &model.map,
                &player,
                &session,
                &mut requests,
                &mut messages,
                &ImageIds::default(),
            );
        });
        output.textures_delta.clear();
        let requests =
            world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
        assert_eq!(requests.len(), usize::from(!pending));
        if !pending {
            assert!(matches!(&requests[0], MultiplayerRequest::CreateJointAttack(invitation)
                if invitation.participants[0].contribution.as_ref().unwrap().army.amount(&Unit::war_sun()) == 17));
        }
        assert_eq!(state.mission_info.origin, inspected_origin);
    }
}

#[test]
fn invitees_see_current_owner_route_objective_fleet_and_combat_orders() {
    let (model, _, _, mut invitation) = fixture();
    let session = session_for_model(&model);
    let context = egui::Context::default();
    for changed in [false, true] {
        if changed {
            invitation.destination = model.players[0].home_planet;
            invitation.objective = Icon::Destroy;
            let owner = invitation.participants[0].contribution.as_mut().unwrap();
            owner.origin = model.players[2].home_planet;
            owner.army = Army::from([(Unit::war_sun(), 17)]);
            owner.bombing = BombingRaid::Industrial;
            owner.combat_probes = false;
        }
        let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                draw_joint_attack_owner_plan(
                    ui,
                    &invitation,
                    &model.map,
                    &session,
                    &ImageIds::default(),
                );
            });
        });
        output.textures_delta.clear();
        let owner = invitation.participants[0].contribution.as_ref().unwrap();
        for label in [
            format!("Origin: {}", model.map.get(owner.origin).name),
            format!("Target: {}", model.map.get(invitation.destination).name),
            format!("Objective: {}", invitation.objective.to_name()),
            format!("Bombing raid: {}", owner.bombing.to_name()),
            format!(
                "Combat Probes: {}",
                if owner.combat_probes {
                    "On"
                } else {
                    "Off"
                }
            ),
        ] {
            assert!(text_bounds(&output, &label).is_some(), "missing {label}");
        }
        assert!(text_bounds(
            &output,
            if changed {
                "17"
            } else {
                "5"
            }
        )
        .is_some());
    }
}

#[test]
fn joint_response_matches_mission_controls_and_keeps_disabled_reasons_on_accept() {
    for size in [egui::vec2(1040.0, 800.0), egui::vec2(560.0, 460.0)] {
        for blocked in 0..3 {
            let (mut model, player, mut state, mut invitation) = fixture();
            if blocked == 1 {
                invitation.destination = player.home_planet;
            }
            if blocked == 2 {
                model
                    .map
                    .get_mut(invitation.destination)
                    .dock_protecting_fleet(player.id, Army::from([(Unit::probe(), 1)]));
            }
            let context = egui::Context::default();
            let mut style = NordDark.custom_style();
            style.interaction.tooltip_delay = 0.0;
            context.set_global_style(style);
            let mut world = World::new();
            world.init_resource::<Messages<MultiplayerRequest>>();
            let mut params =
                bevy::ecs::system::SystemState::<MessageWriter<MultiplayerRequest>>::new(
                    &mut world,
                );
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            let mut accept = None;
            let mut tooltip_found = false;
            for step in 0..7 {
                let mut events = Vec::new();
                if step >= 2 {
                    let pos = accept.expect("Accept button");
                    events.push(egui::Event::PointerMoved(pos));
                    if step == 4 || step == 5 {
                        events.push(egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed: step == 4,
                            modifiers: egui::Modifiers::NONE,
                        });
                    }
                }
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(screen),
                        time: Some(step as f64 * 0.1),
                        events,
                        ..default()
                    },
                    |ctx| {
                        draw_joint_attack_response_panel(
                            ctx,
                            &mut state,
                            &model.map,
                            &player,
                            &MultiplayerSession::default(),
                            &mut params.get_mut(&mut world).unwrap(),
                            &ImageIds::default(),
                            &invitation,
                            model.map.get(invitation.destination),
                            JointAttackResponse::Pending,
                        );
                    },
                );
                output.textures_delta.clear();
                if step == 0 {
                    continue; // Modal areas use their first pass to measure their contents.
                }
                let rect = text_bounds(&output, "Accept").unwrap();
                assert!(screen.contains_rect(rect), "footer must fit {size:?}");
                accept = Some(rect.center());
                assert!(text_bounds(&output, "New mission").is_none());
                assert!(text_bounds(&output, "Your fleet").is_none());
                if size.x > 800.0 {
                    for label in [
                        "🎯 Objective:",
                        "⚔ Combat Probes:",
                        "💣 Bombing raid:",
                        "Fleet contributions",
                    ] {
                        assert!(text_bounds(&output, label).is_some(), "missing {label}");
                    }
                }
                let reason = match blocked {
                    1 => "You cannot attack your own planet.",
                    2 => "Recall your protection fleet before joining this attack.",
                    _ => "",
                };
                if step < 2 {
                    assert!(text_bounds(&output, reason).is_none());
                }
                tooltip_found |= text_bounds(&output, reason).is_some();
            }
            let accepted = world
                .resource_mut::<Messages<MultiplayerRequest>>()
                .drain()
                .filter(|request| {
                    matches!(
                        request,
                        MultiplayerRequest::RespondJointAttack {
                            response: JointAttackResponse::Accepted,
                            ..
                        }
                    )
                })
                .count();
            assert_eq!(accepted, usize::from(blocked == 0), "size {size:?}, blocked {blocked}");
            assert_eq!(state.joint_attack_open, Some(invitation.id), "Accept keeps the panel open");
            if blocked > 0 {
                assert!(tooltip_found, "disabled reason {blocked} must be visible on hover");
            }
        }
    }
}

#[test]
fn joint_strengths_show_live_drafts_and_hide_rejections() {
    let (_, player, _, mut invitation) = fixture();
    invitation.participants.push(JointAttackParticipant {
        player_id: 3,
        response: JointAttackResponse::Rejected,
        contribution: None,
    });
    let context = egui::Context::default();
    let local = Army::from([(Unit::war_sun(), 7)]);
    let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            draw_joint_attack_strengths(
                ui,
                &invitation,
                &MultiplayerSession::default(),
                Some((player.id, &local)),
            );
        });
    });
    output.textures_delta.clear();
    assert!(text_bounds(
        &output,
        &format!("Player 1 · Strength {} · Choosing", fleet_strength(&local))
    )
    .is_some());
    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text().contains("Player 3"))));
}

#[test]
fn joint_acceptance_can_be_undone_without_closing_the_editor() {
    let (model, player, mut state, invitation) = fixture();
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    let mut params =
        bevy::ecs::system::SystemState::<MessageWriter<MultiplayerRequest>>::new(&mut world);
    let mut undo = None;
    for step in 0..5 {
        let mut events = vec![];
        if step >= 2 {
            let pos = undo.unwrap();
            events.push(egui::Event::PointerMoved(pos));
            if step < 4 {
                events.push(egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed: step == 2,
                    modifiers: egui::Modifiers::NONE,
                });
            }
        }
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1040.0, 800.0),
                )),
                events,
                ..default()
            },
            |ctx| {
                draw_joint_attack_response_panel(
                    ctx,
                    &mut state,
                    &model.map,
                    &player,
                    &MultiplayerSession::default(),
                    &mut params.get_mut(&mut world).unwrap(),
                    &ImageIds::default(),
                    &invitation,
                    model.map.get(invitation.destination),
                    JointAttackResponse::Accepted,
                );
            },
        );
        output.textures_delta.clear();
        if step > 0 {
            undo = Some(text_bounds(&output, "Undo accept").unwrap().center());
        }
    }
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    assert!(matches!(&requests[0], MultiplayerRequest::RespondJointAttack {
        expected_revision: 0, response: JointAttackResponse::Pending, contribution: Some(fleet), ..
    } if fleet.bombing == BombingRaid::Economic && fleet.combat_probes));
    assert_eq!(state.joint_attack_open, Some(invitation.id));
}

#[test]
fn joint_owner_draft_survives_inspecting_another_planet() {
    let (mut model, mut player, mut state, mut invitation) = fixture();
    invitation.inviter = player.id;
    invitation.participants.reverse();
    state.joint_attack_draft_id = Some(invitation.id);
    state.joint_attack_owner_draft = Some(state.joint_attack_contribution.clone());
    state.mission_info.origin = model.players[1].home_planet;
    state.mission_info.army.clear();
    let mut session = MultiplayerSession::default();
    session.joint_attacks.push(invitation);
    let context = egui::Context::default();
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<SendMissionMsg>>();
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<MultiplayerRequest>,
        MessageWriter<SendMissionMsg>,
    )>::new(&mut world);
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1040.0, 800.0),
            )),
            ..default()
        },
        |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let (mut requests, mut sends) = params.get_mut(&mut world).unwrap();
                draw_new_mission(
                    ui,
                    &mut sends,
                    &[],
                    &Settings::default(),
                    &mut state,
                    &mut model.map,
                    &mut player,
                    &session,
                    &mut requests,
                    false,
                    &ButtonInput::default(),
                    &ImageIds::default(),
                );
            });
        },
    );
    output.textures_delta.clear();
    assert_eq!(state.mission_info.origin, player.home_planet);
    assert_eq!(state.mission_info.army, state.joint_attack_contribution.army);
    assert_eq!(state.joint_attack_owner_draft.as_ref().unwrap().origin, player.home_planet);
}

/// Opt-in visual fixture. Runs without backend access and writes only ignored screenshots.
#[cfg(target_os = "windows")]
#[test]
#[ignore = "requires a GPU; renders the allied mission panels into target/ui-joint"]
fn render_joint_mission_panels() {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::simulation::PersistedGame;
    use crate::multiplayer::model::{GameMembership, GameRecord};
    use bevy::app::AppExit;
    use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
    use bevy::winit::{WinitPlugin, WinitSettings};
    use bevy_egui::{EguiPlugin, EguiPrimaryContextPass};
    #[derive(Resource)]
    struct RenderCase {
        invitation: JointAttackInvitation,
        frame: usize,
    }
    let (model, player, mut state, mut invitation) = fixture();
    state.mission_info = state.joint_attack_contribution.clone();
    state.allied_mission = true;
    state.joint_attack_invitees.insert(2);
    state.joint_attack_draft_id = Some(invitation.id);
    // Owner preview uses the same data with roles swapped.
    invitation.inviter = 1;
    invitation.participants.reverse();
    invitation.participants[0].response = JointAttackResponse::Accepted;
    invitation.participants[1].response = JointAttackResponse::Pending;
    let members = (1..=3)
        .map(|id| GameMembership {
            game_id: GameId("fixture".into()),
            player_id: id,
            user_id: UserId(format!("user-{id}")),
            display_name: format!("Practice P{id}"),
            is_creator: id == 1,
            identity_version: 1,
            connected: true,
        })
        .collect::<Vec<_>>();
    let mut session = MultiplayerSession::default();
    session.membership = Some(members[0].clone());
    session.active_game = Some(GameRecord {
        id: GameId("fixture".into()),
        code: GameCode("ABCDEF".into()),
        revision: 0,
        saved_at: 0,
        max_players: 3,
        status: crate::core::simulation::MatchStatus::Active,
        persisted: PersistedGame::new(model.clone()),
        members,
        submitted_players: vec![],
    });
    session.joint_attacks.push(invitation.clone());
    let mut app = App::new();
    std::fs::create_dir_all("target/ui-joint").unwrap();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Allied mission fixture".into(),
                    resolution: (1040, 800).into(),
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
    .insert_resource(RenderCase {
        invitation,
        frame: 0,
    })
    .insert_resource(model.map)
    .insert_resource(player)
    .insert_resource(state)
    .insert_resource(session)
    .init_resource::<ImageIds>()
    .add_message::<MultiplayerRequest>()
    .add_message::<SendMissionMsg>()
    .add_message::<RecallMissionMsg>()
    .add_systems(Startup, |mut commands: Commands| {
        commands.spawn(Camera2d);
    })
    .add_systems(
        EguiPrimaryContextPass,
        (
            set_ui_style,
            |mut contexts: EguiContexts,
             mut ids: ResMut<ImageIds>,
             map: Res<Map>,
             state: Res<UiState>,
             player: Res<Player>,
             mut handles: Local<Vec<egui::TextureHandle>>| {
                if !handles.is_empty() {
                    return;
                }
                let Ok(ctx) = contexts.ctx_mut() else {
                    return;
                };
                let mut keys = Unit::ships()
                    .iter()
                    .map(Unit::to_lowername)
                    .collect::<std::collections::BTreeSet<_>>();
                keys.extend(Icon::iter().map(|icon| icon.asset_key()));
                keys.extend(["panel", "button", "button hover"].map(str::to_owned));
                keys.insert(map.get(state.mission_info.origin).image());
                keys.insert(map.get(state.mission_info.destination).image());
                keys.insert(state.mission_info.image(&player).to_owned());
                for category in ["ui", "icons", "mission", "ships", "planets"] {
                    let directory =
                        format!("{}/assets/images/{category}", env!("CARGO_MANIFEST_DIR"));
                    for file in std::fs::read_dir(directory).unwrap() {
                        let path = file.unwrap().path();
                        if path.extension().and_then(|s| s.to_str()) != Some("png") {
                            continue;
                        }
                        let key = path.file_stem().unwrap().to_str().unwrap().to_owned();
                        if !keys.contains(&key) {
                            continue;
                        }
                        let pixels = image::open(&path).unwrap().into_rgba8();
                        let image = egui::ColorImage::from_rgba_unmultiplied(
                            [pixels.width() as usize, pixels.height() as usize],
                            pixels.as_raw(),
                        );
                        let handle = ctx.load_texture(&key, image, egui::TextureOptions::LINEAR);
                        ids.0.insert(key, handle.id());
                        handles.push(handle);
                    }
                }
            },
            |mut contexts: EguiContexts,
             mut state: ResMut<UiState>,
             mut map: ResMut<Map>,
             mut player: ResMut<Player>,
             session: Res<MultiplayerSession>,
             images: Res<ImageIds>,
             mut case: ResMut<RenderCase>,
             mut requests: MessageWriter<MultiplayerRequest>,
             mut sends: MessageWriter<SendMissionMsg>,
             mut recalls: MessageWriter<RecallMissionMsg>,
             mut window: Single<&mut Window>,
             mut commands: Commands| {
                let Ok(ctx) = contexts.ctx_mut() else {
                    return;
                };
                if case.frame == 80 {
                    state.joint_attack_invite_selection = Some([2].into());
                }
                if case.frame == 160 {
                    state.joint_attack_invite_selection = None;
                    case.invitation.inviter = 2;
                    case.invitation.participants.reverse();
                    case.invitation.participants[0].response = JointAttackResponse::Accepted;
                    case.invitation.participants[1].response = JointAttackResponse::Pending;
                }
                if case.frame == 240 {
                    window.resolution.set(560.0, 460.0);
                    state.joint_attack_invite_selection = Some([2].into());
                }
                if case.frame == 320 {
                    state.joint_attack_invite_selection = None;
                }
                if case.frame < 160 || case.frame >= 240 {
                    let size = mission_panel_size(ctx.content_rect().size(), 2);
                    show_panel_modal(
                        ctx,
                        &images,
                        egui::Id::new("owner render"),
                        size,
                        |ui, rect, _| {
                            ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
                                draw_mission(
                                    ui,
                                    &[],
                                    &mut sends,
                                    &mut recalls,
                                    &Settings::default(),
                                    &mut state,
                                    &mut map,
                                    &mut player,
                                    &session,
                                    &mut requests,
                                    true,
                                    &ButtonInput::default(),
                                    &images,
                                    true,
                                );
                            });
                        },
                    );
                } else {
                    draw_joint_attack_response_panel(
                        ctx,
                        &mut state,
                        &map,
                        &player,
                        &session,
                        &mut requests,
                        &images,
                        &case.invitation,
                        map.get(case.invitation.destination),
                        JointAttackResponse::Pending,
                    );
                }
                case.frame += 1;
                if [60, 140, 220, 300, 380].contains(&case.frame) {
                    let path = format!(
                        "{}/target/ui-joint/{}.png",
                        env!("CARGO_MANIFEST_DIR"),
                        match case.frame {
                            60 => "owner",
                            140 => "invite-picker",
                            220 => "invitee",
                            300 => "invite-picker-compact",
                            _ => "owner-compact",
                        }
                    );
                    let mut capture = commands.spawn(Screenshot::primary_window());
                    capture.observe(save_to_disk(path));
                    if case.frame == 380 {
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
