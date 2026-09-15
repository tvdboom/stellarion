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

fn text_font_size(output: &egui::FullOutput, label: &str) -> Option<f32> {
    output.shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.text() == label => {
            text.galley.job.sections.first().map(|section| section.format.font_id.size)
        },
        _ => None,
    })
}

const INVITE_ICON_TEXTURE: egui::TextureId = egui::TextureId::User(91);
const FLEET_ICON_TEXTURE: egui::TextureId = egui::TextureId::User(92);

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

fn textured_shape_bounds(
    shape: &egui::Shape,
    texture: egui::TextureId,
    minimum_width: f32,
) -> Option<egui::Rect> {
    match shape {
        egui::Shape::Mesh(mesh)
            if mesh.texture_id == texture && mesh.calc_bounds().width() > minimum_width =>
        {
            Some(mesh.calc_bounds())
        },
        egui::Shape::Vec(shapes) => {
            shapes.iter().find_map(|shape| textured_shape_bounds(shape, texture, minimum_width))
        },
        _ => None,
    }
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
        bombing: BombingRaid::Economic,
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

#[test]
fn invited_player_sees_the_owners_bombing_objective_as_fixed() {
    let (model, player, mut state, invitation) = fixture();
    state.joint_attack_contribution.bombing = BombingRaid::Industrial;
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            draw_mission_details(
                ui,
                &mut state.joint_attack_contribution,
                &model.map,
                &player,
                model.turn as usize,
                &ImageIds::default(),
                Some(&invitation),
            );
        });
    });
    output.textures_delta.clear();

    assert_eq!(state.joint_attack_contribution.bombing, BombingRaid::Economic);
    assert!(text_bounds(&output, "Economic").is_some());
    assert!(text_bounds(&output, "Industrial").is_none());
}

#[test]
fn joint_fleet_picker_uses_live_ships_and_reserves_other_accepted_attacks() {
    let (model, player, _, mut invitation) = fixture();
    let origin = player.home_planet;
    let bomber = Unit::Ship(Ship::Bomber);
    let mut projected = model.map.clone();
    projected.get_mut(origin).army.insert(bomber, 12);
    let mut session = session_for_model(&model);

    assert_eq!(
        mission_origin_army_after_allied_reservations(
            &projected,
            &session,
            player.id,
            origin,
            Some(invitation.id),
        )
        .amount(&bomber),
        12,
        "inviting players must not hide ships available in the live mission editor",
    );
    assert_eq!(
        mission_origin_army_after_allied_reservations(
            &projected, &session, player.id, origin, None,
        )
        .amount(&bomber),
        12,
        "an unsent proposal must show the same ships as a published one",
    );

    invitation.participants[1].response = JointAttackResponse::Accepted;
    invitation.participants[1].contribution.as_mut().unwrap().army = Army::from([(bomber, 3)]);
    session.joint_attacks.push(invitation.clone());
    assert_eq!(
        mission_origin_army_after_allied_reservations(
            &projected, &session, player.id, origin, None,
        )
        .amount(&bomber),
        9,
        "ordinary missions reserve accepted allied fleets from the projected army",
    );

    let mut second = invitation;
    second.id += 1;
    second.participants[1].contribution.as_mut().unwrap().army = Army::from([(bomber, 2)]);
    session.joint_attacks.push(second);
    assert_eq!(
        mission_origin_army_after_allied_reservations(
            &projected,
            &session,
            player.id,
            origin,
            Some(session.joint_attacks[0].id),
        )
        .amount(&bomber),
        10,
        "another allied launch reserves its accepted ships without hiding live ships",
    );
    session.joint_attacks[1].launched = true;
    projected.get_mut(origin).army.insert(bomber, 3);
    assert_eq!(
        mission_origin_army_after_allied_reservations(
            &projected,
            &session,
            player.id,
            origin,
            Some(session.joint_attacks[0].id),
        )
        .amount(&bomber),
        3,
        "a launched invitation is already reflected in the projected map",
    );
}

#[test]
fn invitation_opens_at_an_available_fleet_when_home_has_no_ships() {
    let (mut model, player, mut state, mut invitation) = fixture();
    model.map.get_mut(player.home_planet).army.controller_mut().clear();
    let other_origin = model
        .map
        .planets
        .iter()
        .find(|planet| {
            !planet.is_destroyed
                && planet.id != player.home_planet
                && planet.id != invitation.destination
        })
        .unwrap()
        .id;
    let other = model.map.get_mut(other_origin);
    other.owned = Some(player.id);
    other.controlled = Some(player.id);
    other.army.extend(Army::from([(Unit::Ship(Ship::Bomber), 3)]));
    invitation.participants[1].contribution = None;
    state.joint_attack_loaded = None;
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
    assert_eq!(state.joint_attack_contribution.origin, other_origin);
    assert_eq!(state.joint_attack_contribution.army.amount(&Unit::Ship(Ship::Bomber)), 0);
}

#[test]
fn allied_details_share_the_slowest_eta_but_keep_each_routes_cost_and_movement() {
    let (mut model, _, _, mut invitation) = fixture();
    model.map.get_mut(invitation.destination).position = Vec2::ZERO;
    for (index, participant) in invitation.participants.iter_mut().enumerate() {
        participant.response = JointAttackResponse::Accepted;
        let contribution = participant.contribution.as_mut().unwrap();
        model.map.get_mut(contribution.origin).position = Vec2::X
            * Planet::SIZE
            * if index == 0 {
                30.0
            } else {
                5.0
            };
    }
    let turn = invitation.turn as usize;
    let routes = invitation
        .participants
        .iter()
        .map(|participant| {
            let contribution = participant.contribution.as_ref().unwrap();
            Mission::new_with_id(
                invitation.id,
                turn,
                participant.player_id,
                model.map.get(contribution.origin),
                model.map.get(invitation.destination),
                invitation.objective,
                contribution.army.clone(),
                contribution.bombing.clone(),
                contribution.combat_probes,
                false,
                None,
            )
        })
        .collect::<Vec<_>>();
    let duration = routes.iter().map(|mission| mission.duration(&model.map)).max().unwrap();
    let context = egui::Context::default();
    let mut movements = Vec::new();
    for mut route in routes.clone() {
        let preview = joint_mission_preview(&route, &model.map, route.owner, turn, &invitation);
        assert_eq!(preview.duration(&model.map), duration);
        assert_eq!(preview.fuel_consumption(&model.map), route.fuel_consumption(&model.map));
        assert_eq!(preview.distance(&model.map), route.distance(&model.map));
        movements.push(preview.next_turn_movement(&model.map));
        let owner = route.owner;
        let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                draw_mission_details(
                    ui,
                    &mut route,
                    &model.map,
                    model.player(owner).unwrap(),
                    turn,
                    &ImageIds::default(),
                    Some(&invitation),
                );
            });
        });
        output.textures_delta.clear();
        assert!(text_bounds(
            &output,
            &format!("⏱ Duration: +{duration} turns ({})", turn + duration)
        )
        .is_some());
        assert!(!output.shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Text(text) if text.galley.text().contains("Your fleet:")
        )));
        assert!(text_bounds(
            &output,
            &format!("⛽ Fuel consumption: {}", route.fuel_consumption(&model.map))
        )
        .is_some());
    }
    assert!(movements[0] > movements[1]);
    let mut no_fleet = routes[1].clone();
    no_fleet.army.clear();
    assert_eq!(no_fleet.duration(&model.map), 0);
    let preview = joint_mission_preview(&no_fleet, &model.map, no_fleet.owner, turn, &invitation);
    assert_eq!(preview.duration(&model.map), routes[0].duration(&model.map));
    let no_fleet_owner = no_fleet.owner;
    let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            draw_mission_details(
                ui,
                &mut no_fleet,
                &model.map,
                model.player(no_fleet_owner).unwrap(),
                turn,
                &ImageIds::default(),
                Some(&invitation),
            );
        });
    });
    output.textures_delta.clear();
    assert!(text_bounds(
        &output,
        &format!(
            "⏱ Duration: +{} turns ({})",
            routes[0].duration(&model.map),
            turn + routes[0].duration(&model.map)
        )
    )
    .is_some());
    invitation.participants[0].response = JointAttackResponse::Rejected;
    let shorter = &routes[1];
    let preview = joint_mission_preview(shorter, &model.map, shorter.owner, turn, &invitation);
    assert_eq!(preview.duration(&model.map), shorter.duration(&model.map));
}

#[test]
fn published_allied_launch_closes_the_joiners_response_panel() {
    let (model, player, mut state, mut invitation) = fixture();
    invitation.launched = true;
    let mut session = session_for_model(&model);
    session.joint_attacks.push(invitation.clone());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<MultiplayerRequest>,
        MessageWriter<MessageMsg>,
    )>::new(&mut world);
    let context = egui::Context::default();
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
    assert!(state.joint_attack_open.is_none());
    assert!(text_bounds(&output, "Joint Attack").is_none());
    assert_eq!(world.resource_mut::<Messages<MultiplayerRequest>>().drain().count(), 0);
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
    let images = ImageIds(
        [
            (Icon::AlliedAttack.asset_key(), INVITE_ICON_TEXTURE),
            ("fleet".to_string(), FLEET_ICON_TEXTURE),
        ]
        .into(),
    );
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
fn inviting_players_keeps_live_ship_cards_clickable_and_allows_a_proposal() {
    let (mut model, mut player, mut state, _) = fixture();
    let bomber = Unit::Ship(Ship::Bomber);
    state.joint_attack_open = None;
    state.mission_info = state.joint_attack_contribution.clone();
    state.mission_info.army.clear();
    state.joint_attack_invitees.insert(2);
    state.allied_mission = true;
    let mut session = session_for_model(&model);
    session
        .active_game
        .as_mut()
        .unwrap()
        .persisted
        .state
        .map
        .get_mut(player.home_planet)
        .army
        .controller_mut()
        .clear();
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<SendMissionMsg>>();
    let keyboard = ButtonInput::default();
    let size = egui::vec2(1040.0, 800.0);
    let mut frame = |world: &mut World, state: &mut UiState, events| {
        owner_frame(
            &context,
            world,
            &mut model,
            &mut player,
            state,
            &session,
            size,
            events,
            &keyboard,
        )
    };
    frame(&mut world, &mut state, vec![]);
    let output = frame(&mut world, &mut state, vec![]);
    let bomber_count = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "5" => Some(text.pos),
            _ => None,
        })
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .expect("available Bomber count");
    let card_center = egui::pos2(bomber_count.x + 30.0, bomber_count.y - 28.0);
    for pressed in [true, false] {
        frame(&mut world, &mut state, pointer_click(card_center, pressed));
    }
    assert_eq!(state.mission_info.army.amount(&bomber), 5);
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    let output = frame(&mut world, &mut state, vec![]);
    let send = text_bounds(&output, "Send proposal").unwrap().center();
    for pressed in [true, false] {
        frame(&mut world, &mut state, pointer_click(send, pressed));
    }
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::CreateJointAttack(invitation)]
        if invitation.participants[0].contribution.as_ref().unwrap().army.amount(&bomber) == 5));
    assert!(state.joint_attack_proposal_notice);
}

#[test]
fn enter_confirms_a_fleet_number_before_a_second_press_sends_the_mission() {
    let cases = [
        (egui::vec2(1040.0, 800.0), Unit::probe()),
        (egui::vec2(1040.0, 800.0), Unit::Ship(Ship::HeavyFighter)),
        (egui::vec2(560.0, 460.0), Unit::probe()),
        (egui::vec2(560.0, 460.0), Unit::Ship(Ship::HeavyFighter)),
        // At the narrowest supported size, later fleet rows are reached by scrolling.
        // Keep the keyboard-focus regression on the initially visible Probe row.
        (egui::vec2(360.0, 460.0), Unit::probe()),
    ];
    for (size, unit) in cases {
        let (mut model, mut player, mut state, _) = fixture();
        model.map.get_mut(player.home_planet).army.insert(unit, 5);
        state.mission_info = state.joint_attack_contribution.clone();
        state.mission_info.army.insert(unit, 3);
        let session = session_for_model(&model);
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<SendMissionMsg>>();
        let mut frame = |world: &mut World, state: &mut UiState, events, keyboard| {
            owner_frame(
                &context,
                world,
                &mut model,
                &mut player,
                state,
                &session,
                size,
                events,
                keyboard,
            )
        };
        let idle = ButtonInput::default();
        frame(&mut world, &mut state, vec![], &idle);
        let output = frame(&mut world, &mut state, vec![], &idle);
        let number = text_bounds(&output, "3")
            .unwrap_or_else(|| panic!("fleet amount must be visible at {size:?} for {unit:?}"))
            .center();
        frame(&mut world, &mut state, pointer_click(number, true), &idle);
        frame(&mut world, &mut state, pointer_click(number, false), &idle);
        frame(&mut world, &mut state, vec![], &idle);
        assert!(context.memory(|memory| memory.focused().is_some()));

        let enter_event = |pressed| egui::Event::Key {
            key: egui::Key::Enter,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut enter = ButtonInput::default();
        enter.press(KeyCode::Enter);
        frame(
            &mut world,
            &mut state,
            vec![egui::Event::Text("2".into()), enter_event(true)],
            &enter,
        );
        assert_eq!(state.mission_info.army.amount(&unit), 2);
        assert!(context.memory(|memory| memory.focused().is_none()));
        assert_eq!(
            world.resource_mut::<Messages<SendMissionMsg>>().drain().count(),
            0,
            "Enter must only confirm the edited {unit:?} count at {size:?}"
        );

        frame(&mut world, &mut state, vec![enter_event(false)], &idle);
        frame(&mut world, &mut state, vec![enter_event(true)], &enter);
        let sent = world.resource_mut::<Messages<SendMissionMsg>>().drain().collect::<Vec<_>>();
        assert_eq!(sent.len(), 1, "Enter still sends when no number is being edited");
        assert_eq!(sent[0].mission.army.amount(&unit), 2);
    }
}

#[test]
fn owner_can_send_without_waiting_and_cancels_invitations_only_for_solo_launches() {
    for size in [egui::vec2(1040.0, 800.0), egui::vec2(560.0, 460.0)] {
        for responses in [
            [JointAttackResponse::Pending, JointAttackResponse::Pending],
            [JointAttackResponse::Rejected, JointAttackResponse::Rejected],
            [JointAttackResponse::Pending, JointAttackResponse::Rejected],
            [JointAttackResponse::Accepted, JointAttackResponse::Pending],
        ] {
            for use_keyboard in [false, true] {
                let (mut model, mut player, mut state, mut invitation) = fixture();
                invitation.inviter = player.id;
                invitation.destination = model
                    .map
                    .planets
                    .iter()
                    .find(|planet| planet.owned.is_none() && !planet.is_moon())
                    .unwrap()
                    .id;
                invitation.participants.reverse();
                invitation.participants[0].response =
                    if responses.contains(&JointAttackResponse::Accepted) {
                        JointAttackResponse::Pending
                    } else {
                        JointAttackResponse::Accepted
                    };
                invitation.participants[1].response = responses[0];
                if responses[0] == JointAttackResponse::Rejected {
                    invitation.participants[1].contribution = None;
                }
                invitation.participants.push(JointAttackParticipant {
                    player_id: 3,
                    response: responses[1],
                    contribution: None,
                });
                state.joint_attack_open = None;
                state.mission = true;
                restore_joint_attack_owner_draft(&mut state, &invitation);
                state.mission_info = state.joint_attack_owner_draft.clone().unwrap();
                invitation.bombing = state.mission_info.bombing.clone();
                invitation.combat_probes = state.mission_info.combat_probes;
                let expected_army = state.mission_info.army.clone();
                let mut session = session_for_model(&model);
                session.joint_attacks.push(invitation.clone());
                let context = egui::Context::default();
                context.set_global_style(NordDark.custom_style());
                let mut world = World::new();
                world.init_resource::<Messages<MultiplayerRequest>>();
                world.init_resource::<Messages<SendMissionMsg>>();
                let mut frame = |world: &mut World, state: &mut UiState, events, keyboard| {
                    owner_frame(
                        &context,
                        world,
                        &mut model,
                        &mut player,
                        state,
                        &session,
                        size,
                        events,
                        keyboard,
                    )
                };
                let idle = ButtonInput::default();
                frame(&mut world, &mut state, vec![], &idle);
                let output = frame(&mut world, &mut state, vec![], &idle);
                let send = text_bounds(&output, "Send mission").unwrap();
                assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains_rect(send));
                assert_eq!(world.resource_mut::<Messages<SendMissionMsg>>().drain().count(), 0);
                assert_eq!(world.resource_mut::<Messages<MultiplayerRequest>>().drain().count(), 0);

                if use_keyboard {
                    let mut enter = ButtonInput::default();
                    enter.press(KeyCode::Enter);
                    frame(&mut world, &mut state, vec![], &enter);
                } else {
                    frame(&mut world, &mut state, pointer_click(send.center(), true), &idle);
                    frame(&mut world, &mut state, pointer_click(send.center(), false), &idle);
                }
                let sent =
                    world.resource_mut::<Messages<SendMissionMsg>>().drain().collect::<Vec<_>>();
                assert_eq!(
                    sent.len(),
                    1,
                    "responses {responses:?}, keyboard {use_keyboard}, {size:?}"
                );
                assert_eq!(sent[0].mission.army, expected_army);
                assert_eq!(sent[0].mission.origin, player.home_planet);
                assert_eq!(sent[0].mission.destination, invitation.destination);
                assert_eq!(sent[0].mission.objective, invitation.objective);
                let requests = world
                    .resource_mut::<Messages<MultiplayerRequest>>()
                    .drain()
                    .collect::<Vec<_>>();
                if responses.contains(&JointAttackResponse::Accepted) {
                    let launch = sent[0].joint_attack.as_ref().unwrap();
                    assert_eq!(launch.attack_id, invitation.id);
                    assert_eq!(launch.contributions.len(), 2);
                    assert_eq!(launch.contributions[1].player_id, 2);
                    assert!(sent[0].mission.joint_attack.is_some());
                    assert_eq!(sent[0].cancel_joint_attack, None);
                    assert!(requests.is_empty(), "accepted fleets must keep their joint launch");
                } else {
                    assert!(sent[0].joint_attack.is_none());
                    assert!(sent[0].mission.joint_attack.is_none());
                    assert_eq!(sent[0].cancel_joint_attack, Some(invitation.id));
                    assert!(requests.is_empty(), "cancellation follows successful local dispatch");
                }
                assert!(!state.mission);
                assert!(!state.allied_mission);
                assert!(state.joint_attack_draft_id.is_none());
                assert!(state.joint_attack_owner_draft.is_none());
                assert!(state.joint_attack_invitees.is_empty());
            }
        }
    }
}

#[test]
fn invite_picker_opens_on_hover_and_applies_each_choice_without_launching() {
    for size in [egui::vec2(1040.0, 800.0), egui::vec2(560.0, 460.0)] {
        let (mut model, mut player, mut state, _) = fixture();
        state.mission_info = state.joint_attack_contribution.clone();
        let mut session = session_for_model(&model);
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<SendMissionMsg>>();
        let keyboard = ButtonInput::default();
        {
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
            frame(&mut state, vec![], &keyboard);
            let output = frame(&mut state, vec![], &keyboard);
            assert!(text_bounds(&output, "Invite players").is_none());
            let (invite, idle_tint) = invite_icon(&output);
            let send = text_bounds(&output, "Send mission").unwrap();
            let strength_icon = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == FLEET_ICON_TEXTURE => {
                        Some(mesh.calc_bounds())
                    },
                    _ => None,
                })
                .unwrap();
            let strength =
                text_bounds(&output, &state.mission_info.army.total_production().to_string())
                    .unwrap();
            let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
            assert!(screen.contains_rect(invite), "invite footer at {size:?}: {invite:?}");
            assert!(screen.contains_rect(send), "send footer at {size:?}: {send:?}");
            assert!(strength_icon.right() < strength.left(), "icon precedes value");
            assert!(strength.right() < send.left(), "strength precedes send button");
            assert!(send.right() < screen.right() - 15.0, "send button has a right inset");
            assert!(invite.right() < send.left());
            assert!(invite.left() < 36.0, "invite stays at the bottom left");
            assert_eq!(invite.size(), egui::Vec2::splat(40.0));
            if size.x >= 620.0 {
                assert!(invite.y_range().contains(send.center().y));
            } else {
                assert!(invite.bottom() < send.top(), "compact footer stacks the invite row");
            }
            assert!(text_bounds(&output, "Allied mission").is_none());
            let unopened_panel = egui::pos2(invite.right() + 60.0, invite.top() - 60.0);
            let away =
                frame(&mut state, vec![egui::Event::PointerMoved(unopened_panel)], &keyboard);
            assert!(text_bounds(&away, "Invite players").is_none());
            assert!(state.joint_attack_invite_panel_rect.is_none());
            let hovered =
                frame(&mut state, vec![egui::Event::PointerMoved(invite.center())], &keyboard);
            let (hovered_rect, hover_tint) = invite_icon(&hovered);
            assert_eq!(hovered_rect, invite);
            assert!(hover_tint.r() > idle_tint.r(), "invite brightens on hover");
            let picker = state.joint_attack_invite_panel_rect.unwrap().1;
            assert!(picker.bottom() < invite.bottom(), "picker sits above the icon's bottom edge");
            let output = frame(&mut state, vec![], &keyboard);
            assert!(text_bounds(&output, "Player 1").is_none());
            assert!(text_bounds(&output, "Cancel").is_none());
            assert!(text_bounds(&output, "Confirm").is_none());
            let row = text_bounds(&output, "Player 2").unwrap();
            assert!(row.left() > invite.right(), "picker opens beside the icon");
            assert!(screen.contains_rect(row));
            frame(&mut state, pointer_click(row.center(), true), &keyboard);
            frame(&mut state, pointer_click(row.center(), false), &keyboard);
            assert_eq!(state.joint_attack_invitees, [2].into());
        }
        let before_sync = world.resource_mut::<Messages<MultiplayerRequest>>().drain().count();
        assert_eq!(before_sync, 0, "selection remains a draft");
        let output = owner_frame(
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
        assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
        let send = text_bounds(&output, "Send proposal").unwrap().center();
        owner_frame(
            &context,
            &mut world,
            &mut model,
            &mut player,
            &mut state,
            &session,
            size,
            pointer_click(send, true),
            &keyboard,
        );
        owner_frame(
            &context,
            &mut world,
            &mut model,
            &mut player,
            &mut state,
            &session,
            size,
            pointer_click(send, false),
            &keyboard,
        );
        let requests =
            world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
        assert_eq!(requests.len(), 1);
        assert!(matches!(&requests[0], MultiplayerRequest::CreateJointAttack(invitation)
                if invitation.participants.iter().map(|p| p.player_id).collect::<Vec<_>>() == [1, 2]
                && invitation.participants[1].response == JointAttackResponse::Pending));
        assert_eq!(world.resource_mut::<Messages<SendMissionMsg>>().drain().count(), 0);
        session.joint_attack_update_pending = true;
        let away = owner_frame(
            &context,
            &mut world,
            &mut model,
            &mut player,
            &mut state,
            &session,
            size,
            vec![egui::Event::PointerMoved(egui::pos2(size.x - 8.0, 8.0))],
            &keyboard,
        );
        assert!(text_bounds(&away, "Player 2").is_some());
        assert!(state.joint_attack_invite_panel_rect.is_none());
        assert_eq!(state.joint_attack_invitees, [2].into());
    }
}

#[test]
fn unsent_joint_mission_can_be_canceled_and_replaced_by_a_solo_mission() {
    for size in [egui::vec2(1040.0, 800.0), egui::vec2(560.0, 460.0)] {
        let (mut model, mut player, mut state, _) = fixture();
        let solo_mission = state.joint_attack_contribution.clone();
        state.joint_attack_open = None;
        state.mission = true;
        state.mission_info = solo_mission.clone();
        state.joint_attack_invitees.insert(2);
        state.allied_mission = true;
        let session = session_for_model(&model);
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<SendMissionMsg>>();
        let keyboard = ButtonInput::default();
        let mut frame = |world: &mut World, state: &mut UiState, events| {
            owner_frame(
                &context,
                world,
                &mut model,
                &mut player,
                state,
                &session,
                size,
                events,
                &keyboard,
            )
        };

        frame(&mut world, &mut state, vec![]);
        state.mission = false;
        state.mission = true;
        let output = frame(&mut world, &mut state, vec![]);
        assert!(text_bounds(&output, "Send proposal").is_some());
        let cancel = text_bounds(&output, "Cancel mission").unwrap().center();
        assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, size).contains(cancel));
        frame(&mut world, &mut state, pointer_click(cancel, true));
        frame(&mut world, &mut state, pointer_click(cancel, false));

        assert!(!state.mission);
        assert!(!state.allied_mission);
        assert!(state.joint_attack_invitees.is_empty());
        assert!(state.joint_attack_draft_id.is_none());
        assert!(state.joint_attack_owner_draft.is_none());
        assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
        assert!(world.resource::<Messages<SendMissionMsg>>().is_empty());

        state.mission = true;
        state.mission_info = solo_mission.clone();
        let output = frame(&mut world, &mut state, vec![]);
        assert!(text_bounds(&output, "Cancel mission").is_none());
        let send = text_bounds(&output, "Send mission").unwrap().center();
        frame(&mut world, &mut state, pointer_click(send, true));
        frame(&mut world, &mut state, pointer_click(send, false));
        let sent = world.resource_mut::<Messages<SendMissionMsg>>().drain().collect::<Vec<_>>();
        assert!(matches!(sent.as_slice(), [message] if message.joint_attack.is_none()));
        assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    }
}

#[test]
fn owner_cannot_publish_a_joint_proposal_without_selected_ships() {
    let (mut model, mut player, mut state, _) = fixture();
    state.joint_attack_open = None;
    state.mission_info = state.joint_attack_contribution.clone();
    state.mission_info.army.clear();
    state.joint_attack_invitees.insert(2);
    state.allied_mission = true;
    let session = session_for_model(&model);
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<SendMissionMsg>>();
    let size = egui::vec2(1040.0, 800.0);
    let idle = ButtonInput::default();
    let mut frame = |world: &mut World, state: &mut UiState, events, keyboard| {
        owner_frame(
            &context,
            world,
            &mut model,
            &mut player,
            state,
            &session,
            size,
            events,
            keyboard,
        )
    };
    frame(&mut world, &mut state, vec![], &idle);
    let output = frame(&mut world, &mut state, vec![], &idle);
    let send = text_bounds(&output, "Send proposal").unwrap().center();
    for pressed in [true, false] {
        frame(&mut world, &mut state, pointer_click(send, pressed), &idle);
    }
    let mut enter = ButtonInput::default();
    enter.press(KeyCode::Enter);
    frame(&mut world, &mut state, vec![], &enter);
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    assert!(state.joint_attack_draft_id.is_none());

    state.mission_info.army.insert(Unit::Ship(Ship::Bomber), 1);
    let output = frame(&mut world, &mut state, vec![], &idle);
    let send = text_bounds(&output, "Send proposal").unwrap().center();
    for pressed in [true, false] {
        frame(&mut world, &mut state, pointer_click(send, pressed), &idle);
    }
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::CreateJointAttack(invitation)]
        if invitation.participants[0].contribution.as_ref().unwrap().army.amount(&Unit::Ship(Ship::Bomber)) == 1));
}

#[test]
fn invited_player_response_does_not_open_the_owner_invite_picker() {
    let (mut model, mut player, mut state, invitation) = fixture();
    state.mission_info = state.joint_attack_contribution.clone();
    state.mission = true;
    let mut session = session_for_model(&model);
    session.joint_attacks.push(invitation);
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<SendMissionMsg>>();
    let keyboard = ButtonInput::default();
    let size = egui::vec2(1040.0, 800.0);
    let output = owner_frame(
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
    let icon = invite_icon(&output).0;
    let output = owner_frame(
        &context,
        &mut world,
        &mut model,
        &mut player,
        &mut state,
        &session,
        size,
        vec![egui::Event::PointerMoved(icon.center())],
        &keyboard,
    );
    assert!(text_bounds(&output, "Invite players").is_none());
    assert!(state.joint_attack_invite_panel_rect.is_none());
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
}

#[test]
fn invite_picker_opens_over_scaled_mission_window_with_no_fleet_selected() {
    let (mut model, mut player, mut state, _) = fixture();
    state.mission_info = state.joint_attack_contribution.clone();
    state.mission_info.army = Army::default();
    let session = session_for_model(&model);
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<SendMissionMsg>>();
    let images = ImageIds(
        [
            (Icon::AlliedAttack.asset_key(), INVITE_ICON_TEXTURE),
            ("fleet".to_string(), FLEET_ICON_TEXTURE),
            ("mission panel".to_string(), egui::TextureId::User(93)),
        ]
        .into(),
    );
    let size = egui::vec2(1040.0, 800.0);
    let mut frame = |state: &mut UiState, events| {
        let mut params = bevy::ecs::system::SystemState::<(
            MessageWriter<MultiplayerRequest>,
            MessageWriter<SendMissionMsg>,
        )>::new(&mut world);
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..default()
            },
            |ctx| {
                draw_panel_on_context(
                    ctx,
                    "mission",
                    "mission panel",
                    (110.0, 80.0),
                    (900.0, 700.0),
                    0.0,
                    0.8,
                    &images,
                    |ui| {
                        let (mut requests, mut sends) = params.get_mut(&mut world).unwrap();
                        draw_new_mission(
                            ui,
                            &mut sends,
                            &[],
                            &Settings::default(),
                            state,
                            &mut model.map,
                            &mut player,
                            &session,
                            &mut requests,
                            false,
                            &ButtonInput::default(),
                            &images,
                        );
                    },
                );
            },
        );
        output.textures_delta.clear();
        output
    };
    frame(&mut state, vec![]);
    let output = frame(&mut state, vec![]);
    let screen_icon = invite_icon(&output).0;
    let transform = context
        .layer_transform_to_global(egui::LayerId::new(
            egui::Order::Middle,
            egui::Id::new("mission"),
        ))
        .unwrap();
    let logical_icon = transform.inverse().mul_rect(screen_icon);
    assert!(screen_icon.center().distance(logical_icon.center()) > 10.0);
    frame(&mut state, vec![egui::Event::PointerMoved(screen_icon.center())]);
    let output = frame(&mut state, vec![]);
    assert!(
        text_bounds(&output, "Invite players").is_some(),
        "logical={logical_icon:?}, screen={screen_icon:?}, panel={:?}, hovered_icon={:?}",
        state.joint_attack_invite_panel_rect,
        invite_icon(&output),
    );
    assert!(state.joint_attack_invite_panel_rect.is_some());
}

#[test]
fn invite_picker_toggles_draft_invitees_and_locks_them_after_send() {
    for size in [egui::vec2(1040.0, 800.0), egui::vec2(560.0, 460.0), egui::vec2(360.0, 460.0)] {
        let (model, player, mut state, _) = fixture();
        state.mission_info = state.joint_attack_contribution.clone();
        state.allied_mission = true;
        state.joint_attack_invitees = [2].into();
        let mut session = session_for_model(&model);
        session.joint_attack_update_pending = true;
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let icon =
            egui::Rect::from_min_size(egui::pos2(12.0, size.y - 54.0), egui::Vec2::splat(40.0));
        let frame = |state: &mut UiState, events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..default()
                },
                |ctx| {
                    draw_joint_attack_invite_picker(ctx, state, &session, &player, icon, true);
                },
            );
            output.textures_delta.clear();
            output
        };
        frame(&mut state, vec![egui::Event::PointerMoved(icon.center())]);
        let output = frame(&mut state, vec![]);
        let confirmed = text_bounds(&output, "Player 2").unwrap();
        let available = text_bounds(&output, "Player 3").unwrap();
        let panel = state.joint_attack_invite_panel_rect.unwrap().1;
        assert!(panel.contains(available.center()), "last row stays inside hover bounds");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
        for rect in [confirmed, available] {
            assert!(screen.contains_rect(rect), "invite picker at {size:?}: {rect:?}");
        }

        frame(&mut state, pointer_click(confirmed.center(), true));
        frame(&mut state, pointer_click(confirmed.center(), false));
        assert!(state.joint_attack_invitees.is_empty());
        assert!(!state.allied_mission);

        frame(&mut state, pointer_click(confirmed.center(), true));
        frame(&mut state, pointer_click(confirmed.center(), false));
        assert_eq!(state.joint_attack_invitees, [2].into());
        assert!(state.allied_mission);

        frame(&mut state, pointer_click(available.center(), true));
        frame(&mut state, pointer_click(available.center(), false));
        assert_eq!(state.joint_attack_invitees, [2, 3].into());

        // Sending the first proposal assigns its persistent draft id and makes
        // every invited player irrevocable for the rest of the mission.
        state.joint_attack_draft_id = Some(17);
        frame(&mut state, pointer_click(confirmed.center(), true));
        frame(&mut state, pointer_click(confirmed.center(), false));
        assert_eq!(state.joint_attack_invitees, [2, 3].into());
        assert!(state.allied_mission);
        let away =
            frame(&mut state, vec![egui::Event::PointerMoved(egui::pos2(size.x - 8.0, 8.0))]);
        assert!(text_bounds(&away, "Player 2").is_none());
    }
}

#[test]
fn clicking_an_invited_player_keeps_the_published_offer() {
    let (mut model, mut player, mut state, mut invitation) = fixture();
    invitation.inviter = player.id;
    invitation.participants.reverse();
    invitation.participants[0].response = JointAttackResponse::Accepted;
    state.mission_info = state.joint_attack_contribution.clone();
    state.joint_attack_draft_id = Some(invitation.id);
    state.joint_attack_owner_draft = Some(state.mission_info.clone());
    state.joint_attack_invitees.insert(2);
    state.allied_mission = true;
    let mut session = session_for_model(&model);
    session.joint_attacks.push(invitation.clone());
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<SendMissionMsg>>();
    let keyboard = ButtonInput::default();
    let size = egui::vec2(560.0, 460.0);
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
    let output = owner_frame(
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
    let (icon, _) = invite_icon(&output);
    owner_frame(
        &context,
        &mut world,
        &mut model,
        &mut player,
        &mut state,
        &session,
        size,
        vec![egui::Event::PointerMoved(icon.center())],
        &keyboard,
    );
    let output = owner_frame(
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
    let row = output
        .shapes
        .iter()
        .rev()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "Player 2" => {
                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                (rect.center().y < icon.top()).then_some(rect)
            },
            _ => None,
        })
        .unwrap();
    let panel = state.joint_attack_invite_panel_rect.unwrap().1;
    assert!(panel.contains(row.center()), "picker row {row:?} outside panel {panel:?}");
    for pressed in [true, false] {
        owner_frame(
            &context,
            &mut world,
            &mut model,
            &mut player,
            &mut state,
            &session,
            size,
            pointer_click(row.center(), pressed),
            &keyboard,
        );
    }
    assert_eq!(state.joint_attack_invitees, [2].into());
    assert!(state.allied_mission);
    assert_eq!(state.joint_attack_draft_id, Some(invitation.id));
    restore_joint_attack_owner_draft(&mut state, &invitation);
    assert_eq!(state.joint_attack_invitees, [2].into());
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(!requests
        .iter()
        .any(|request| matches!(request, MultiplayerRequest::CancelJointAttack { .. })));
}

#[test]
fn added_invitees_wait_for_send_proposal_after_an_update() {
    let (model, player, mut state, mut invitation) = fixture();
    state.mission_info = state.joint_attack_contribution.clone();
    state.allied_mission = true;
    state.joint_attack_invitees = [2, 3].into();
    invitation.inviter = player.id;
    invitation.participants.reverse();
    invitation.participants[0].response = JointAttackResponse::Accepted;
    invitation.participants[0].contribution = Some(JointAttackContribution {
        player_id: player.id,
        origin: state.mission_info.origin,
        army: state.mission_info.army.clone(),
        bombing: state.mission_info.bombing.clone(),
        combat_probes: state.mission_info.combat_probes,
    });
    let mut session = session_for_model(&model);
    session.joint_attack_update_pending = true;
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    let mut params =
        bevy::ecs::system::SystemState::<MessageWriter<MultiplayerRequest>>::new(&mut world);

    {
        let mut requests = params.get_mut(&mut world).unwrap();
        assert!(!sync_allied_mission(
            &mut state,
            model.turn as usize,
            &player,
            &session,
            &mut requests,
            Some(&invitation),
            None,
        ));
    }
    assert_eq!(world.resource_mut::<Messages<MultiplayerRequest>>().drain().count(), 0);

    session.joint_attack_update_pending = false;
    {
        let mut requests = params.get_mut(&mut world).unwrap();
        assert!(!sync_allied_mission(
            &mut state,
            model.turn as usize,
            &player,
            &session,
            &mut requests,
            Some(&invitation),
            None,
        ));
    }
    assert_eq!(world.resource_mut::<Messages<MultiplayerRequest>>().drain().count(), 0);
    let available = mission_origin_army_after_allied_reservations(
        &model.map,
        &session,
        player.id,
        state.mission_info.origin,
        Some(invitation.id),
    );
    {
        let mut requests = params.get_mut(&mut world).unwrap();
        sync_allied_mission(
            &mut state,
            model.turn as usize,
            &player,
            &session,
            &mut requests,
            Some(&invitation),
            Some(&available),
        );
    }
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::CreateJointAttack(updated)]
        if updated.id == invitation.id
        && updated.participants.iter().map(|item| item.player_id).collect::<Vec<_>>() == [1, 2, 3]));
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
    for tab in [
        MissionTab::NewMission,
        MissionTab::ActiveMissions,
        MissionTab::EnemyMissions,
        MissionTab::MissionReports,
    ] {
        state.mission_tab = tab;
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
        assert!(text_bounds(&output, "Reopen mission").is_none(), "hide toast on {tab:?}");
    }
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
        assert!(requests.is_empty(), "closed owner drafts must wait for Send proposal");
        assert_eq!(state.mission_info.origin, inspected_origin);
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
                model.map.get_mut(invitation.destination).protection_permissions.insert(player.id);
            }
            state.joint_attack_contribution.destination = invitation.destination;
            state.joint_attack_contribution.objective = invitation.objective;
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
            let images = ImageIds([("fleet".to_string(), FLEET_ICON_TEXTURE)].into());
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
                            &images,
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
                let reject = text_bounds(&output, "Reject").unwrap();
                let scale = mission_panel_scale(size);
                let own_strength = output
                    .shapes
                    .iter()
                    .find_map(|shape| {
                        textured_shape_bounds(&shape.shape, FLEET_ICON_TEXTURE, 18.0 * scale)
                    })
                    .expect("own fleet strength badge");
                assert!(own_strength.right() < reject.left());
                assert!((own_strength.center().y - reject.center().y).abs() < 5.0);
                let panel = egui::Rect::from_center_size(
                    screen.center(),
                    mission_panel_size(size / scale) * scale,
                )
                .expand(8.0);
                assert!(text_bounds(&output, "Close").is_none());
                for label in ["Reject", "Accept"] {
                    let text = text_bounds(&output, label).unwrap();
                    let button = egui::Rect::from_center_size(
                        text.center(),
                        egui::vec2(180.0, 50.0) * scale,
                    );
                    assert!(
                        panel.contains_rect(button),
                        "{label} {button:?} must stay inside {panel:?} at {size:?}"
                    );
                }
                if size.x > 800.0 {
                    let reject = text_bounds(&output, "Reject").unwrap();
                    assert!(
                        rect.center().x - reject.center().x >= 195.0 * scale,
                        "response actions need a visible horizontal gap"
                    );
                }
                accept = Some(rect.center());
                let title = text_bounds(&output, "Joint Attack").unwrap();
                assert!((title.center().x - panel.center().x).abs() < 1.0);
                assert!(title.center().y < panel.top() + 40.0 * scale);
                assert_eq!(text_font_size(&output, "Joint Attack"), Some(18.0));
                assert!(text_bounds(&output, "New mission").is_none());
                assert!(text_bounds(&output, "Your fleet").is_none());
                if size.x > 800.0 {
                    for label in
                        ["🎯 Objective:", "⚔ Combat Probes:", "💣 Bombing raid:", "Accepted"]
                    {
                        assert!(text_bounds(&output, label).is_some(), "missing {label}");
                    }
                    for label in ["💣 Bombing raid:", "Economic"] {
                        let shape = output
                            .shapes
                            .iter()
                            .find(|shape| {
                                matches!(&shape.shape,
                                egui::Shape::Text(text) if text.galley.text() == label)
                            })
                            .unwrap();
                        assert!(
                            shape.clip_rect.contains_rect(text_bounds(&output, label).unwrap()),
                            "{label} must fit inside the mission details"
                        );
                    }
                    assert!(text_bounds(&output, "Pending").is_none());
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
fn invited_support_fleets_do_not_need_the_owners_colony_ship_or_war_sun() {
    for (objective, owner_ship) in
        [(Icon::Colonize, Unit::colony_ship()), (Icon::Destroy, Unit::war_sun())]
    {
        let (model, player, mut state, mut invitation) = fixture();
        invitation.objective = objective;
        if objective == Icon::Colonize {
            invitation.destination = model
                .map
                .planets
                .iter()
                .find(|planet| planet.owned.is_none() && !planet.is_moon() && !planet.is_destroyed)
                .unwrap()
                .id;
        }
        invitation.participants[0].contribution.as_mut().unwrap().army =
            Army::from([(owner_ship, 1)]);
        state.joint_attack_contribution.objective = objective;
        state.joint_attack_contribution.destination = invitation.destination;
        state.joint_attack_contribution.army = Army::from([(Unit::Ship(Ship::Bomber), 1)]);
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        let size = egui::vec2(1040.0, 800.0);
        let frame = |world: &mut World, state: &mut UiState, events| {
            let mut params =
                bevy::ecs::system::SystemState::<MessageWriter<MultiplayerRequest>>::new(world);
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                    events,
                    ..default()
                },
                |ctx| {
                    draw_joint_attack_response_panel(
                        ctx,
                        state,
                        &model.map,
                        &player,
                        &MultiplayerSession::default(),
                        &mut params.get_mut(world).unwrap(),
                        &ImageIds::default(),
                        &invitation,
                        model.map.get(invitation.destination),
                        JointAttackResponse::Pending,
                    );
                },
            );
            output.textures_delta.clear();
            output
        };
        frame(&mut world, &mut state, vec![]);
        let output = frame(&mut world, &mut state, vec![]);
        let send = text_bounds(&output, "Send proposal").unwrap().center();
        for pressed in [true, false] {
            frame(&mut world, &mut state, pointer_click(send, pressed));
        }
        let requests =
            world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
        assert!(
            matches!(requests.as_slice(), [MultiplayerRequest::RespondJointAttack {
            response: JointAttackResponse::Accepted,
            contribution: Some(contribution),
            ..
        }] if contribution.army.amount(&Unit::Ship(Ship::Bomber)) == 1),
            "guest support should be accepted for {objective:?}"
        );
    }
}

#[test]
fn joint_strengths_show_compact_other_player_roster() {
    let (_, player, _, mut invitation) = fixture();
    invitation.participants.push(JointAttackParticipant {
        player_id: 3,
        response: JointAttackResponse::Pending,
        contribution: None,
    });
    invitation.participants.push(JointAttackParticipant {
        player_id: 4,
        response: JointAttackResponse::Rejected,
        contribution: None,
    });
    let context = egui::Context::default();
    let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            let roster = egui::Rect::from_min_size(egui::pos2(55.0, 10.0), egui::vec2(175.0, 80.0));
            ui.scope_builder(UiBuilder::new().max_rect(roster), |ui| {
                draw_joint_attack_strengths(
                    ui,
                    roster.width(),
                    &invitation,
                    &MultiplayerSession::default(),
                    player.id,
                    true,
                );
            });
        });
    });
    output.textures_delta.clear();
    let strength = fleet_strength(&invitation.participants[0].contribution.as_ref().unwrap().army);
    let accepted_name = text_bounds(&output, "Player 2").unwrap();
    let accepted_strength = text_bounds(&output, &format!(" ({strength})")).unwrap();
    let pending_name = text_bounds(&output, "Player 3").unwrap();
    let accepted = text_bounds(&output, "Accepted").unwrap();
    let pending = text_bounds(&output, "Pending").unwrap();
    assert!(accepted_name.left() >= 55.0);
    assert!(accepted_name.top() < pending_name.top());
    assert!(accepted_strength.left() >= accepted_name.right());
    assert!(accepted.left() > accepted_strength.right());
    assert!(pending.left() > pending_name.right());
    assert!((pending.center().y - pending_name.center().y).abs() < 2.0);
    assert!((pending.bottom() - 90.0).abs() < 5.0, "bottom row should align with the icon");
    assert!(text_bounds(&output, "Player 1 (0)").is_none());
    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text().contains("Player 4"))));
}

#[test]
fn mission_strength_follows_the_available_footer_action() {
    for size in [egui::vec2(1040.0, 800.0), egui::vec2(560.0, 460.0)] {
        let mut baseline: Option<(egui::Pos2, egui::Pos2, egui::Pos2)> = None;
        let mut cancel_baseline: Option<egui::Pos2> = None;
        for guest_count in 0..=3 {
            let (mut model, mut player, mut state, mut invitation) = fixture();
            state.mission_info = state.joint_attack_contribution.clone();
            let mut session = session_for_model(&model);
            if guest_count > 0 {
                invitation.inviter = player.id;
                invitation.participants.reverse();
                for player_id in 3..=(guest_count + 1) as u64 {
                    invitation.participants.push(JointAttackParticipant {
                        player_id,
                        response: JointAttackResponse::Pending,
                        contribution: None,
                    });
                }
                state.joint_attack_draft_id = Some(invitation.id);
                state.joint_attack_owner_draft = Some(state.mission_info.clone());
                state.joint_attack_invitees = (2..=(guest_count + 1) as u64).collect();
                state.allied_mission = true;
                session.joint_attacks.push(invitation);
                session.joint_attack_update_pending = true;
            }
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let mut world = World::new();
            world.init_resource::<Messages<MultiplayerRequest>>();
            world.init_resource::<Messages<SendMissionMsg>>();
            let idle = ButtonInput::default();
            owner_frame(
                &context,
                &mut world,
                &mut model,
                &mut player,
                &mut state,
                &session,
                size,
                vec![],
                &idle,
            );
            let output = owner_frame(
                &context,
                &mut world,
                &mut model,
                &mut player,
                &mut state,
                &session,
                size,
                vec![],
                &idle,
            );
            let invite = invite_icon(&output).0;
            let send = text_bounds(&output, "Send mission")
                .or_else(|| text_bounds(&output, "Send proposal"))
                .unwrap();
            let fleet = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) if mesh.texture_id == FLEET_ICON_TEXTURE => {
                        Some(mesh.calc_bounds())
                    },
                    _ => None,
                })
                .min_by(|a, b| {
                    (a.center().y - send.center().y)
                        .abs()
                        .total_cmp(&(b.center().y - send.center().y).abs())
                        .then_with(|| b.center().x.total_cmp(&a.center().x))
                })
                .unwrap();
            if guest_count > 0 {
                let cancel = text_bounds(&output, "Cancel mission").unwrap();
                assert!(fleet.right() < cancel.left() && cancel.right() < send.left());
                assert!(
                    cancel.center().x - 90.0 - fleet.right() < 60.0,
                    "badge should sit beside cancel at {size:?}"
                );
                assert!((fleet.center().y - cancel.center().y).abs() < 25.0);
                if let Some((previous_invite, previous_send, previous_fleet)) = baseline {
                    assert_eq!(invite.center(), previous_invite);
                    assert_eq!(send.center(), previous_send);
                    assert_eq!(fleet.center(), previous_fleet);
                } else {
                    baseline = Some((invite.center(), send.center(), fleet.center()));
                }
                if let Some(previous_cancel) = cancel_baseline {
                    assert_eq!(cancel.center(), previous_cancel, "cancel button moved at {size:?}");
                } else {
                    cancel_baseline = Some(cancel.center());
                }
            } else {
                assert!(text_bounds(&output, "Cancel mission").is_none());
                assert!(fleet.right() < send.left());
                assert!(
                    send.center().x - 90.0 - fleet.right() < 60.0,
                    "badge should sit beside send at {size:?}: fleet {fleet:?}, send {send:?}"
                );
                assert!((fleet.center().y - send.center().y).abs() < 25.0);
            }
        }
    }
}

#[test]
fn owner_roster_sits_beside_invite_icon_at_supported_panel_widths() {
    for size in [egui::vec2(1040.0, 800.0), egui::vec2(560.0, 460.0), egui::vec2(360.0, 460.0)] {
        let (mut model, mut player, mut state, mut invitation) = fixture();
        invitation.inviter = player.id;
        invitation.participants.reverse();
        state.joint_attack_draft_id = Some(invitation.id);
        state.joint_attack_owner_draft = Some(state.joint_attack_contribution.clone());
        state.allied_mission = true;
        state.joint_attack_invitees.insert(2);
        let mut session = session_for_model(&model);
        session.active_game.as_mut().unwrap().members[1].display_name = "Practice P2".into();
        session.joint_attacks.push(invitation.clone());
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut world = World::new();
        world.init_resource::<Messages<MultiplayerRequest>>();
        world.init_resource::<Messages<SendMissionMsg>>();
        owner_frame(
            &context,
            &mut world,
            &mut model,
            &mut player,
            &mut state,
            &session,
            size,
            vec![],
            &ButtonInput::default(),
        );
        let output = owner_frame(
            &context,
            &mut world,
            &mut model,
            &mut player,
            &mut state,
            &session,
            size,
            vec![],
            &ButtonInput::default(),
        );
        let (icon, _) = invite_icon(&output);
        let strength =
            fleet_strength(&invitation.participants[1].contribution.as_ref().unwrap().army);
        let name = text_bounds(&output, "Practice P2").unwrap();
        let badge = text_bounds(&output, "Accepted").unwrap();
        let fleet = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == FLEET_ICON_TEXTURE => {
                    let rect = mesh.calc_bounds();
                    (rect.left() > name.right() && rect.right() < badge.left()).then_some(rect)
                },
                _ => None,
            })
            .unwrap();
        let strength_text = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == strength.to_string() => {
                    let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                    ((rect.center().y - name.center().y).abs() < 3.0).then_some(rect)
                },
                _ => None,
            })
            .unwrap();
        assert!(name.left() - icon.right() >= 11.0, "size {size:?}: name {name:?}, icon {icon:?}");
        assert!(
            fleet.left() - name.right() >= 7.0,
            "size {size:?}: fleet {fleet:?}, name {name:?}"
        );
        let name_clip = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Practice P2" => {
                    Some(shape.clip_rect)
                },
                _ => None,
            })
            .unwrap();
        assert!(
            name_clip.right() + 1.0 >= name.right(),
            "size {size:?}: full name {name:?} must be visible inside {name_clip:?}"
        );
        assert!(
            strength_text.left() >= fleet.right(),
            "size {size:?}: strength {strength_text:?}, fleet {fleet:?}"
        );
        assert!(
            badge.left() - strength_text.right() >= 12.0,
            "size {size:?}: badge {badge:?}, strength {strength_text:?}"
        );
        assert!(
            (badge.bottom() - icon.bottom()).abs() < 5.0,
            "size {size:?}: badge {badge:?}, icon {icon:?}"
        );
        assert!(text_bounds(&output, "Player 1").is_none());
    }
}

#[test]
fn owner_roster_shows_every_selected_invitee_and_fleet_strength() {
    let (_, player, _, mut invitation) = fixture();
    invitation.inviter = player.id;
    invitation.participants.reverse();
    let selected = [2, 3].into();
    let context = egui::Context::default();
    let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            paint_owner_joint_attack_roster(
                ui,
                62.0,
                125.0,
                260.0,
                Some(&invitation),
                Some(&selected),
                &MultiplayerSession::default(),
                player.id,
                &ImageIds::default(),
            );
        });
    });
    output.textures_delta.clear();
    assert!(text_bounds(&output, "Player 2").is_some());
    assert!(text_bounds(&output, "Player 3").is_some());
    let strength = fleet_strength(&invitation.participants[1].contribution.as_ref().unwrap().army);
    assert!(text_bounds(&output, &strength.to_string()).is_some());
    assert!(!output.shapes.iter().any(|shape| matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text().contains("+1t"))));
}

#[test]
fn owner_roster_columns_stay_aligned_with_different_names_and_fleet_strengths() {
    let (model, player, _, mut invitation) = fixture();
    invitation.inviter = player.id;
    invitation.participants.reverse();
    invitation.participants[1].contribution.as_mut().unwrap().army =
        Army::from([(Unit::Ship(Ship::Bomber), 100)]);
    let large_strength =
        fleet_strength(&invitation.participants[1].contribution.as_ref().unwrap().army);
    invitation.participants.push(JointAttackParticipant {
        player_id: 3,
        response: JointAttackResponse::Pending,
        contribution: None,
    });
    let mut session = session_for_model(&model);
    let members = &mut session.active_game.as_mut().unwrap().members;
    members[1].display_name = "P2".into();
    members[2].display_name = "An Extremely Long Commander Name".into();
    let images = ImageIds([("fleet".to_string(), FLEET_ICON_TEXTURE)].into());
    let context = egui::Context::default();
    let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            paint_owner_joint_attack_roster(
                ui,
                62.0,
                125.0,
                260.0,
                Some(&invitation),
                None,
                &session,
                player.id,
                &images,
            );
        });
    });
    output.textures_delta.clear();
    let fleet_icons = output
        .shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == FLEET_ICON_TEXTURE => {
                Some(mesh.calc_bounds())
            },
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(fleet_icons.len(), 2);
    assert!((fleet_icons[0].left() - fleet_icons[1].left()).abs() < 0.1);
    let accepted = text_bounds(&output, "Accepted").unwrap();
    let pending = text_bounds(&output, "Pending").unwrap();
    assert_eq!(text_font_size(&output, "P2"), Some(JOINT_ATTACK_ROSTER_NAME_SIZE));
    assert_eq!(text_font_size(&output, "Accepted"), Some(JOINT_ATTACK_ROSTER_STATUS_SIZE),);
    assert!((accepted.center().x - pending.center().x).abs() < 0.1);
    let large_count = text_bounds(&output, &large_strength.to_string()).unwrap();
    let zero_count = text_bounds(&output, "0").unwrap();
    assert!((large_count.left() - zero_count.left()).abs() < 0.1);
    assert!(large_count.right() < accepted.left());
    let long_name_clip = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.text() == "An Extremely Long Commander Name" => {
                Some(shape.clip_rect)
            },
            _ => None,
        })
        .unwrap();
    assert!(long_name_clip.right() <= fleet_icons[0].left() - 7.0);
}

#[test]
fn guest_roster_strength_column_follows_the_longest_name() {
    let (model, _, _, mut invitation) = fixture();
    invitation.participants.push(JointAttackParticipant {
        player_id: 3,
        response: JointAttackResponse::Pending,
        contribution: None,
    });
    let mut session = session_for_model(&model);
    let members = &mut session.active_game.as_mut().unwrap().members;
    members[0].display_name = "A".into();
    members[2].display_name = "Practice P4".into();
    let images = ImageIds([("fleet".to_string(), FLEET_ICON_TEXTURE)].into());
    let context = egui::Context::default();
    let mut output = context.run_ui(egui::RawInput::default(), |ctx| {
        egui::CentralPanel::default().show(ctx, |ui| {
            paint_owner_joint_attack_roster(
                ui,
                20.0,
                120.0,
                270.0,
                Some(&invitation),
                None,
                &session,
                invitation.inviter,
                &images,
            );
        });
    });
    output.textures_delta.clear();
    let longest_name_right = ["A", "Practice P4"]
        .into_iter()
        .filter_map(|name| text_bounds(&output, name).map(|rect| rect.right()))
        .fold(0.0_f32, f32::max);
    let fleet_x = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == FLEET_ICON_TEXTURE => {
                Some(mesh.calc_bounds().left())
            },
            _ => None,
        })
        .unwrap();
    assert!((fleet_x - longest_name_right - JOINT_ATTACK_ROSTER_NAME_GAP).abs() < 2.0);
}

#[test]
fn invited_player_roster_keeps_the_full_name_near_the_left_edge() {
    let (model, player, mut state, invitation) = fixture();
    let mut session = session_for_model(&model);
    session.active_game.as_mut().unwrap().members[1].display_name = "Practice P1".into();
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    let mut params =
        bevy::ecs::system::SystemState::<MessageWriter<MultiplayerRequest>>::new(&mut world);
    let mut frame = || {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1040.0, 800.0),
                )),
                ..default()
            },
            |ctx| {
                draw_joint_attack_response_panel(
                    ctx,
                    &mut state,
                    &model.map,
                    &player,
                    &session,
                    &mut params.get_mut(&mut world).unwrap(),
                    &ImageIds::default(),
                    &invitation,
                    model.map.get(invitation.destination),
                    JointAttackResponse::Pending,
                );
            },
        );
        output.textures_delta.clear();
        output
    };
    frame();
    let output = frame();
    let name = text_bounds(&output, "Practice P1").unwrap();
    let viewport = egui::vec2(1040.0, 800.0);
    let scale = mission_panel_scale(viewport);
    let panel = egui::Rect::from_center_size(
        egui::Rect::from_min_size(egui::Pos2::ZERO, viewport).center(),
        mission_panel_size(viewport / scale) * scale,
    );
    assert!(name.left() >= panel.left() + 20.0 * scale);
    assert!(name.left() < panel.left() + 30.0 * scale, "name should start beside the panel edge");
    assert!(name.bottom() > panel.bottom() - 32.0 * scale);
    let shape = output
        .shapes
        .iter()
        .find(|shape| {
            matches!(&shape.shape,
        egui::Shape::Text(text) if text.galley.text() == "Practice P1")
        })
        .unwrap();
    assert!(
        shape.clip_rect.left() <= name.left() + 1.0
            && shape.clip_rect.right() + 1.0 >= name.right(),
        "full player name must be visible: {name:?} in {:?}",
        shape.clip_rect,
    );
    assert!(text_bounds(&output, "Accept").is_some());
}

#[test]
fn compact_invitee_panel_keeps_the_roster_inside_its_footer() {
    let (model, player, mut state, invitation) = fixture();
    let mut session = session_for_model(&model);
    session.active_game.as_mut().unwrap().members[1].display_name = "Practice P2".into();
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    let size = egui::vec2(560.0, 460.0);
    let mut frame = |state: &mut UiState| {
        let mut params =
            bevy::ecs::system::SystemState::<MessageWriter<MultiplayerRequest>>::new(&mut world);
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                ..default()
            },
            |ctx| {
                draw_joint_attack_response_panel(
                    ctx,
                    state,
                    &model.map,
                    &player,
                    &session,
                    &mut params.get_mut(&mut world).unwrap(),
                    &ImageIds::default(),
                    &invitation,
                    model.map.get(invitation.destination),
                    JointAttackResponse::Pending,
                );
            },
        );
        output.textures_delta.clear();
        output
    };
    frame(&mut state);
    let output = frame(&mut state);
    let name = text_bounds(&output, "Practice P2").expect("compact roster name");
    let scale = mission_panel_scale(size);
    let panel = egui::Rect::from_center_size(
        egui::Rect::from_min_size(egui::Pos2::ZERO, size).center(),
        mission_panel_size(size / scale) * scale,
    );
    assert!(panel.contains_rect(name), "roster name {name:?} outside {panel:?}");
}

#[test]
fn joined_attack_toast_reopens_or_rejects_and_blocks_duplicate_rejection() {
    for width in [1040.0, 360.0] {
        for (label, pending) in [("Reopen", false), ("Reject", false), ("Reject", true)] {
            let (model, player, mut state, mut invitation) = fixture();
            state.joint_attack_open = None;
            invitation.participants[1].response = JointAttackResponse::Accepted;
            let mut session = session_for_model(&model);
            session.joint_attacks.push(invitation.clone());
            session.joint_attack_update_pending = pending;
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let mut world = World::new();
            world.init_resource::<Messages<MultiplayerRequest>>();
            world.init_resource::<Messages<MessageMsg>>();
            let mut params = bevy::ecs::system::SystemState::<(
                MessageWriter<MultiplayerRequest>,
                MessageWriter<MessageMsg>,
            )>::new(&mut world);
            let mut button = None;
            for step in 0..4 {
                let events = if step >= 2 {
                    pointer_click(button.unwrap(), step == 2)
                } else {
                    vec![]
                };
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(width, 800.0),
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
                    let reopen = text_bounds(&output, "Reopen").unwrap();
                    let reject = text_bounds(&output, "Reject").unwrap();
                    assert!(reject.right() < reopen.left());
                    assert!((reject.center().y - reopen.center().y).abs() < 1.0);
                    assert!(reopen.right() < width);
                    assert!(text_bounds(&output, "Review").is_none());
                    button = Some(text_bounds(&output, label).unwrap().center());
                }
            }
            let requests =
                world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
            if label == "Reopen" {
                assert_eq!(state.joint_attack_open, Some(invitation.id));
                assert!(requests.is_empty());
            } else if pending {
                assert_eq!(state.joint_attack_open, None);
                assert!(requests.is_empty());
            } else {
                assert!(matches!(
                    requests.as_slice(),
                    [MultiplayerRequest::RespondJointAttack {
                        attack_id: 91,
                        expected_revision: 0,
                        response: JointAttackResponse::Rejected,
                        contribution: None,
                    }]
                ));
            }
        }
    }
}

#[test]
fn accepted_joint_fleet_withdraws_once_when_edited() {
    let (model, player, mut state, mut invitation) = fixture();
    invitation.participants[1].response = JointAttackResponse::Accepted;
    state.joint_attack_loaded = Some(invitation.id);
    let mut session = session_for_model(&model);
    session.joint_attacks.push(invitation);
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let mut params = bevy::ecs::system::SystemState::<(
        MessageWriter<MultiplayerRequest>,
        MessageWriter<MessageMsg>,
    )>::new(&mut world);
    let mut accept = None;
    for step in 0..5 {
        if step == 4 {
            state.joint_attack_contribution.army.insert(Unit::Ship(Ship::Bomber), 3);
        }
        let mut events = vec![];
        if step >= 2 {
            let pos = accept.unwrap();
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
        if step > 0 && step < 4 {
            accept = Some(text_bounds(&output, "Accept").unwrap().center());
            assert!(text_bounds(&output, "Send proposal").is_none());
            assert!(text_bounds(&output, "Close").is_none());
            assert!(text_bounds(&output, "Reject").is_some());
            assert!(text_bounds(&output, "Undo accept").is_none());
        } else if step == 4 {
            assert!(text_bounds(&output, "Send proposal").is_some());
        }
    }
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert_eq!(requests.len(), 1);
    assert!(matches!(&requests[0], MultiplayerRequest::RespondJointAttack {
        expected_revision: 0, response: JointAttackResponse::Pending, contribution: Some(fleet), ..
    } if fleet.army.amount(&Unit::Ship(Ship::Bomber)) == 5
        && fleet.bombing == BombingRaid::Economic && fleet.combat_probes));
    assert_eq!(state.joint_attack_open, Some(91));
}

#[test]
fn pending_joint_fleet_changes_wait_for_send_proposal() {
    let (model, player, mut state, invitation) = fixture();
    state.joint_attack_loaded = Some(invitation.id);
    state.joint_attack_contribution.army.insert(Unit::Ship(Ship::Bomber), 3);
    let mut session = session_for_model(&model);
    session.joint_attacks.push(invitation);
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<MessageMsg>>();
    let size = egui::vec2(1040.0, 800.0);
    let frame = |world: &mut World, state: &mut UiState, events| {
        let mut params = bevy::ecs::system::SystemState::<(
            MessageWriter<MultiplayerRequest>,
            MessageWriter<MessageMsg>,
        )>::new(world);
        context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..default()
            },
            |ctx| {
                let (mut requests, mut messages) = params.get_mut(world).unwrap();
                draw_joint_attack_notifications(
                    ctx,
                    state,
                    &model.map,
                    &player,
                    &session,
                    &mut requests,
                    &mut messages,
                    &ImageIds::default(),
                );
            },
        )
    };
    let mut output = frame(&mut world, &mut state, vec![]);
    output.textures_delta.clear();
    output = frame(&mut world, &mut state, vec![]);
    output.textures_delta.clear();
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    let button = text_bounds(&output, "Send proposal").unwrap().center();
    for pressed in [true, false] {
        let mut output = frame(&mut world, &mut state, pointer_click(button, pressed));
        output.textures_delta.clear();
    }
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::RespondJointAttack {
        response: JointAttackResponse::Accepted, contribution: Some(fleet), ..
    }] if fleet.army.amount(&Unit::Ship(Ship::Bomber)) == 3));
    let mut output = frame(&mut world, &mut state, vec![]);
    output.textures_delta.clear();
    let notices = world.resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
    assert!(matches!(notices.as_slice(), [notice]
        if notice.message == "Joint attack proposal sent."
            && notice.display_duration == Some(std::time::Duration::from_secs(3))));
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

#[test]
fn owner_changes_withdraw_acceptance_until_new_proposal() {
    let (mut model, mut player, mut state, mut invitation) = fixture();
    invitation.inviter = player.id;
    invitation.participants.reverse();
    invitation.participants[0].response = JointAttackResponse::Accepted;
    state.mission = true;
    restore_joint_attack_owner_draft(&mut state, &invitation);
    state.joint_attack_owner_draft.as_mut().unwrap().army.insert(Unit::Ship(Ship::Bomber), 3);
    let mut session = session_for_model(&model);
    session.joint_attacks.push(invitation);
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<SendMissionMsg>>();
    let size = egui::vec2(1040.0, 800.0);
    let idle = ButtonInput::default();
    let output = owner_frame(
        &context,
        &mut world,
        &mut model,
        &mut player,
        &mut state,
        &session,
        size,
        vec![],
        &idle,
    );
    assert_eq!(state.mission_info.army.amount(&Unit::Ship(Ship::Bomber)), 3);
    assert!(state.allied_mission);
    assert!(!allied_synced_for_owner(&state, session.joint_attacks.first()));
    let pending = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    let kinds = pending
        .iter()
        .map(|request| match request {
            MultiplayerRequest::RespondJointAttack {
                response: JointAttackResponse::Pending,
                ..
            } => "pending",
            MultiplayerRequest::CreateJointAttack(_) => "create",
            MultiplayerRequest::CancelJointAttack {
                ..
            } => "cancel",
            _ => "other",
        })
        .collect::<Vec<_>>();
    assert_eq!(kinds, ["pending"], "an edited owner offer must withdraw acceptance once");
    assert!(matches!(
        pending.as_slice(),
        [MultiplayerRequest::RespondJointAttack {
            response: JointAttackResponse::Pending,
            ..
        }]
    ));
    session.joint_attacks[0].participants[0].response = JointAttackResponse::Pending;
    let button = text_bounds(&output, "Send proposal").unwrap().center();
    for pressed in [true, false] {
        owner_frame(
            &context,
            &mut world,
            &mut model,
            &mut player,
            &mut state,
            &session,
            size,
            pointer_click(button, pressed),
            &idle,
        );
    }
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::CreateJointAttack(updated)]
        if updated.participants[0].contribution.as_ref().unwrap().army.amount(&Unit::Ship(Ship::Bomber)) == 3));
    assert!(world.resource::<Messages<SendMissionMsg>>().is_empty());
}

#[test]
fn owner_route_and_objective_edits_wait_for_a_revised_proposal() {
    let (mut model, mut player, mut state, mut invitation) = fixture();
    let destination = model
        .map
        .planets
        .iter()
        .find(|planet| planet.owned.is_none() && !planet.is_moon() && !planet.is_destroyed)
        .unwrap()
        .id;
    model.map.get_mut(player.home_planet).army.insert(Unit::colony_ship(), 2);
    invitation.inviter = player.id;
    invitation.participants.reverse();
    invitation.participants[0].response = JointAttackResponse::Accepted;
    state.joint_attack_open = None;
    restore_joint_attack_owner_draft(&mut state, &invitation);
    let draft = state.joint_attack_owner_draft.as_mut().unwrap();
    draft.destination = destination;
    draft.objective = Icon::Colonize;
    draft.army = Army::from([(Unit::colony_ship(), 1)]);
    let mut session = session_for_model(&model);
    session.joint_attacks.push(invitation.clone());
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    world.init_resource::<Messages<SendMissionMsg>>();
    let size = egui::vec2(1040.0, 800.0);
    let idle = ButtonInput::default();
    let output = owner_frame(
        &context,
        &mut world,
        &mut model,
        &mut player,
        &mut state,
        &session,
        size,
        vec![],
        &idle,
    );
    assert_eq!(state.mission_info.destination, destination);
    assert_eq!(state.mission_info.objective, Icon::Colonize);
    assert_eq!(state.mission_info.army.amount(&Unit::colony_ship()), 1);
    assert!(text_bounds(&output, "Send proposal").is_some());
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(
        requests.as_slice(),
        [MultiplayerRequest::RespondJointAttack {
            response: JointAttackResponse::Pending,
            ..
        }]
    ));
    session.joint_attacks[0].participants[0].response = JointAttackResponse::Pending;
    let output = owner_frame(
        &context,
        &mut world,
        &mut model,
        &mut player,
        &mut state,
        &session,
        size,
        vec![],
        &idle,
    );
    let send = text_bounds(&output, "Send proposal").unwrap().center();
    for pressed in [true, false] {
        owner_frame(
            &context,
            &mut world,
            &mut model,
            &mut player,
            &mut state,
            &session,
            size,
            pointer_click(send, pressed),
            &idle,
        );
    }
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::CreateJointAttack(updated)]
        if updated.id == invitation.id
            && updated.destination == destination
            && updated.objective == Icon::Colonize
            && updated.participants[0].contribution.as_ref().unwrap().army.amount(&Unit::colony_ship()) == 1));
    assert!(world.resource::<Messages<SendMissionMsg>>().is_empty());
}

#[test]
fn invited_player_cannot_change_the_shared_target_or_objective() {
    let (mut model, player, mut state, invitation) = fixture();
    let new_target = model
        .map
        .planets
        .iter()
        .find(|planet| {
            !planet.is_destroyed
                && !planet.is_moon()
                && planet.id != invitation.destination
                && planet.owned != Some(player.id)
                && planet.owned != Some(invitation.inviter)
        })
        .unwrap()
        .id;
    model.map.get_mut(player.home_planet).army.insert(Unit::war_sun(), 1);
    state.joint_attack_loaded = Some(invitation.id);
    state.joint_attack_contribution.destination = new_target;
    state.joint_attack_contribution.objective = Icon::Destroy;
    state.joint_attack_contribution.army.insert(Unit::war_sun(), 1);
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    let size = egui::vec2(1040.0, 800.0);
    let frame = |world: &mut World, state: &mut UiState, events| {
        let mut params =
            bevy::ecs::system::SystemState::<MessageWriter<MultiplayerRequest>>::new(world);
        context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, size)),
                events,
                ..default()
            },
            |ctx| {
                draw_joint_attack_response_panel(
                    ctx,
                    state,
                    &model.map,
                    &player,
                    &MultiplayerSession::default(),
                    &mut params.get_mut(world).unwrap(),
                    &ImageIds::default(),
                    &invitation,
                    model.map.get(invitation.destination),
                    JointAttackResponse::Pending,
                );
            },
        )
    };
    let mut output = frame(&mut world, &mut state, vec![]);
    output.textures_delta.clear();
    output = frame(&mut world, &mut state, vec![]);
    output.textures_delta.clear();
    assert_eq!(state.joint_attack_contribution.destination, invitation.destination);
    assert_eq!(state.joint_attack_contribution.objective, invitation.objective);
    assert!(world.resource::<Messages<MultiplayerRequest>>().is_empty());
    let send = text_bounds(&output, "Send proposal").unwrap().center();
    for pressed in [true, false] {
        let mut output = frame(&mut world, &mut state, pointer_click(send, pressed));
        output.textures_delta.clear();
    }
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::RespondJointAttack {
        response: JointAttackResponse::Accepted,
        contribution: Some(contribution),
        ..
    }] if contribution.army.amount(&Unit::war_sun()) == 1));
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
    .add_message::<MessageMsg>()
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
             mut session: ResMut<MultiplayerSession>,
             images: Res<ImageIds>,
             mut case: ResMut<RenderCase>,
             mut requests: MessageWriter<MultiplayerRequest>,
             mut messages: MessageWriter<MessageMsg>,
             mut sends: MessageWriter<SendMissionMsg>,
             mut recalls: MessageWriter<RecallMissionMsg>,
             mut window: Single<&mut Window>,
             mut commands: Commands| {
                let Ok(ctx) = contexts.ctx_mut() else {
                    return;
                };
                if case.frame == 160 {
                    case.invitation.inviter = 2;
                    case.invitation.participants.reverse();
                    case.invitation.participants[0].response = JointAttackResponse::Accepted;
                    case.invitation.participants[1].response = JointAttackResponse::Pending;
                }
                if case.frame == 240 {
                    window.resolution.set(560.0, 460.0);
                }
                if case.frame == 400 {
                    window.resolution.set(1040.0, 800.0);
                    case.invitation.participants[1].response = JointAttackResponse::Accepted;
                    session.joint_attacks[0] = case.invitation.clone();
                    state.joint_attack_open = None;
                    state.joint_attack_draft_id = None;
                    state.joint_attack_owner_draft = None;
                    state.mission = false;
                }
                if case.frame == 480 {
                    window.resolution.set(360.0, 460.0);
                }
                if case.frame >= 400 {
                    draw_joint_attack_notifications(
                        ctx,
                        &mut state,
                        &map,
                        &player,
                        &session,
                        &mut requests,
                        &mut messages,
                        &images,
                    );
                } else if case.frame < 160 || case.frame >= 320 {
                    let size = mission_panel_size(ctx.content_rect().size());
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
                if [60, 220, 300, 380, 460, 540].contains(&case.frame) {
                    let path = format!(
                        "{}/target/ui-joint/{}.png",
                        env!("CARGO_MANIFEST_DIR"),
                        match case.frame {
                            60 => "owner",
                            220 => "invitee",
                            300 => "invitee-compact",
                            380 => "owner-compact",
                            460 => "joined-toast",
                            _ => "joined-toast-compact",
                        }
                    );
                    let mut capture = commands.spawn(Screenshot::primary_window());
                    capture.observe(save_to_disk(path));
                    if case.frame == 540 {
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
