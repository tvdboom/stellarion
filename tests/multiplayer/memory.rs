use futures_lite::future::block_on;
use std::sync::{Arc, Barrier};

use super::*;
use crate::core::identity::GameCode;
use crate::core::map::icon::Icon;
use crate::core::missions::BombingRaid;
use crate::core::player::PlayerColor;
use crate::core::simulation::{resolve_turn, GameModel, GameRules, TurnCommand};

#[test]
fn sync_suppresses_unchanged_rosters_and_preserves_private_replay() {
    let backend = InMemoryBackend::new();
    let (host, recovery) = identity(&backend);
    let created = create(&backend, &host, &recovery, 2);
    let (guest, recovery) = identity(&backend);
    block_on(backend.join_game(
        &guest,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Guest".into(),
            recovery_code: recovery.expose().into(),
        },
    ))
    .unwrap();
    let game_id = &created.game.id;
    let first = block_on(backend.sync_game(&host, game_id, 0, true, None)).unwrap();
    assert_eq!(first.members.as_ref().unwrap().len(), 2);
    let quiet = block_on(backend.sync_game(
        &host,
        game_id,
        first.batch.cursor,
        true,
        Some(&first.roster_token),
    ))
    .unwrap();
    assert!(quiet.members.is_none());
    assert!(quiet.batch.events.is_empty());
    assert!(!quiet.batch.resync_required);
    let last_seen = {
        let mut state = backend.inner.lock().unwrap();
        let stored = state.games.get_mut(game_id).unwrap();
        stored.connected_players.insert(1, Instant::now() - PLAYER_CONNECTION_TIMEOUT);
        stored.connected_players[&1]
    };
    let expired = block_on(backend.sync_game(
        &host,
        game_id,
        quiet.batch.cursor,
        false,
        Some(&quiet.roster_token),
    ))
    .unwrap();
    assert!(!expired.members.as_ref().unwrap()[0].connected);
    assert_ne!(expired.roster_token, quiet.roster_token);
    assert_eq!(backend.inner.lock().unwrap().games[game_id].connected_players[&1], last_seen);
    assert_eq!(backend.inner.lock().unwrap().games[game_id].record.saved_at, created.game.saved_at);

    {
        let mut state = backend.inner.lock().unwrap();
        let stored = state.games.get_mut(game_id).unwrap();
        for _ in 0..2050 {
            push_event(stored, BackendEventKind::TradeChanged, Some(1), Some(1));
        }
        assert_eq!(stored.events.len(), 2048);
    }
    let missed = block_on(backend.sync_game(&guest, game_id, 0, false, None)).unwrap();
    assert!(missed.batch.events.is_empty());
    assert!(missed.batch.cursor >= 256);
    assert!(missed.batch.resync_required);
    let visible = block_on(backend.sync_game(&host, game_id, 0, false, None)).unwrap();
    assert_eq!(visible.batch.events.len(), 256);
    assert_eq!(visible.batch.cursor, missed.batch.cursor);
    let (outsider, _) = identity(&backend);
    assert!(matches!(
        block_on(backend.sync_game(&outsider, game_id, 0, true, Some(&quiet.roster_token))),
        Err(BackendError::Forbidden)
    ));
}
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Unit};
use crate::multiplayer::model::JointAttackParticipant;
use crate::multiplayer::recovery::{generate_game_code, RecoveryCode};

/// Creates a session and a matching recovery credential.
fn identity(backend: &InMemoryBackend) -> (AuthSession, RecoveryCode) {
    (block_on(backend.authenticate(None)).unwrap(), RecoveryCode::generate().unwrap())
}

/// Creates a game with the requested exact player capacity.
fn create(
    backend: &InMemoryBackend,
    session: &AuthSession,
    recovery: &RecoveryCode,
    count: u8,
) -> MembershipResult {
    let model = GameModel::new(
        [count; 32],
        GameRules {
            player_count: count,
            ..GameRules::default()
        },
    )
    .unwrap();
    block_on(backend.create_game(
        session,
        CreateGameRequest {
            code: generate_game_code().unwrap(),
            display_name: "Creator".to_string(),
            recovery_code: recovery.expose().to_string(),
            persisted: PersistedGame::new(model),
        },
    ))
    .unwrap()
}

#[test]
/// Covers creation, 2/3/4-player joining, duplicate reconnect, and full lobbies.
fn supports_all_lobby_sizes_and_duplicate_joining() {
    for count in 2..=4 {
        let backend = InMemoryBackend::new();
        let (creator, creator_recovery) = identity(&backend);
        let created = create(&backend, &creator, &creator_recovery, count);
        for slot in 2..=count {
            let (session, recovery) = identity(&backend);
            let joined = block_on(backend.join_game(
                &session,
                JoinGameRequest {
                    code: created.game.code.clone(),
                    display_name: format!("Player {slot}"),
                    recovery_code: recovery.expose().to_string(),
                },
            ))
            .unwrap();
            assert_eq!(joined.membership.player_id, u64::from(slot));
            let duplicate = block_on(backend.join_game(
                &session,
                JoinGameRequest {
                    code: created.game.code.clone(),
                    display_name: "Ignored".to_string(),
                    recovery_code: RecoveryCode::generate().unwrap().expose().to_string(),
                },
            ))
            .unwrap();
            assert_eq!(duplicate.disposition, JoinDisposition::Reconnected);
            assert_eq!(duplicate.recovery_code, recovery.expose());
        }
        let (extra, extra_recovery) = identity(&backend);
        assert!(matches!(
            block_on(backend.join_game(
                &extra,
                JoinGameRequest {
                    code: created.game.code,
                    display_name: "Extra".to_string(),
                    recovery_code: extra_recovery.expose().to_string(),
                },
            )),
            Err(BackendError::GameFull)
        ));
    }
}

#[test]
fn display_names_accept_the_new_boundary_and_reject_longer_values() {
    let recovery = RecoveryCode::generate().unwrap();
    assert!(validate_name_and_code(&"N".repeat(MAX_DISPLAY_NAME_CHARS), recovery.expose()).is_ok());
    assert!(matches!(
        validate_name_and_code(&"N".repeat(MAX_DISPLAY_NAME_CHARS + 1), recovery.expose()),
        Err(BackendError::InvalidData(_))
    ));
}

#[test]
fn simultaneous_color_claims_have_one_winner_and_restore_the_loser() {
    let backend = InMemoryBackend::new();
    let (host, host_recovery) = identity(&backend);
    let (guest, guest_recovery) = identity(&backend);
    let created = create(&backend, &host, &host_recovery, 4);
    let joined = block_on(backend.join_game(
        &guest,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Guest".into(),
            recovery_code: guest_recovery.expose().to_string(),
        },
    ))
    .unwrap();
    let game_id = joined.game.id.clone();
    let initial_host = joined.game.persisted.state.player(1).unwrap().color();
    let initial_guest = joined.game.persisted.state.player(2).unwrap().color();
    let claimed = PlayerColor::new(5).unwrap();
    let barrier = Arc::new(Barrier::new(3));
    let claim = |session: AuthSession| {
        let backend = backend.clone();
        let game_id = game_id.clone();
        let barrier = barrier.clone();
        std::thread::spawn(move || {
            barrier.wait();
            block_on(backend.set_player_color(&session, &game_id, claimed)).unwrap()
        })
    };
    let host_claim = claim(host.clone());
    let guest_claim = claim(guest);
    barrier.wait();
    host_claim.join().unwrap();
    guest_claim.join().unwrap();

    let final_game = block_on(backend.load_game(&host, &game_id)).unwrap();
    let host_color = final_game.persisted.state.player(1).unwrap().color();
    let guest_color = final_game.persisted.state.player(2).unwrap().color();
    assert_eq!(usize::from(host_color == claimed) + usize::from(guest_color == claimed), 1);
    if host_color == claimed {
        assert_eq!(guest_color, initial_guest);
    } else {
        assert_eq!(host_color, initial_host);
    }
    final_game.persisted.validate().unwrap();
}

#[test]
/// A four-slot lobby may start with two members and then becomes an exact two-player game.
fn starts_with_current_lobby_members_instead_of_waiting_for_capacity() {
    let backend = InMemoryBackend::new();
    let (creator, creator_recovery) = identity(&backend);
    let created = create(&backend, &creator, &creator_recovery, 4);

    let mut premature = created.game.persisted.clone();
    premature.state.start().unwrap();
    assert!(matches!(
        block_on(backend.start_game(&creator, &created.game.id, created.game.revision, premature,)),
        Err(BackendError::InvalidGameStatus)
    ));

    let (joiner, joiner_recovery) = identity(&backend);
    let joined = block_on(backend.join_game(
        &joiner,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Joiner".to_string(),
            recovery_code: joiner_recovery.expose().to_string(),
        },
    ))
    .unwrap();
    let mut rules = joined.game.persisted.state.rules.clone();
    rules.player_count = 2;
    let mut started = GameModel::new([42; 32], rules).unwrap();
    started.start().unwrap();
    let active = block_on(backend.start_game(
        &creator,
        &joined.game.id,
        joined.game.revision,
        PersistedGame::new(started),
    ))
    .unwrap();

    assert_eq!(active.status, MatchStatus::Active);
    assert_eq!(active.max_players, 2);
    assert_eq!(active.members.len(), 2);
    assert_eq!(active.persisted.state.players.len(), 2);
    assert_eq!(active.persisted.state.rules.player_count, 2);
}

#[test]
fn protection_access_updates_canonical_state_immediately_with_a_compact_revision() {
    let backend = InMemoryBackend::new();
    let (host, host_recovery) = identity(&backend);
    let created = create(&backend, &host, &host_recovery, 3);
    let mut guests = Vec::new();
    let mut game = created.game;
    for slot in 2..=3 {
        let (guest, recovery) = identity(&backend);
        game = block_on(backend.join_game(
            &guest,
            JoinGameRequest {
                code: game.code.clone(),
                display_name: format!("Guest {slot}"),
                recovery_code: recovery.expose().to_string(),
            },
        ))
        .unwrap()
        .game;
        guests.push(guest);
    }
    game.persisted.state.start().unwrap();
    let active =
        block_on(backend.start_game(&host, &game.id, game.revision, game.persisted)).unwrap();
    let protected = active.persisted.state.player(1).unwrap().home_planet;
    let saved_at = active.saved_at;

    let granted =
        block_on(backend.set_protection_permission(&host, &active.id, protected, 2, true)).unwrap();
    assert_eq!(granted.revision, active.revision + 1);
    assert!(granted.allowed);
    let visible_to_guest = block_on(backend.load_game(&guests[0], &active.id)).unwrap();
    assert_eq!(visible_to_guest.saved_at, saved_at);
    assert!(visible_to_guest.persisted.state.map.get(protected).allows_protection(2));
    assert_eq!(
        visible_to_guest
            .persisted
            .state
            .player(2)
            .unwrap()
            .protection_controller(visible_to_guest.persisted.state.map.get(protected)),
        Some(1)
    );

    let protector_home = visible_to_guest.persisted.state.player(2).unwrap().home_planet;
    let fleet_unit = Unit::Ship(Ship::LightFighter);
    backend
        .lock()
        .unwrap()
        .games
        .get_mut(&active.id)
        .unwrap()
        .record
        .persisted
        .state
        .map
        .get_mut(protector_home)
        .army
        .insert(fleet_unit, 1);
    assert_eq!(
        block_on(backend.submit_turn(
            &guests[0],
            &active.id,
            TurnSubmission::new(
                2,
                visible_to_guest.persisted.state.turn,
                vec![TurnCommand::SendMission {
                    mission_id: 91,
                    origin: protector_home,
                    destination: protected,
                    objective: Icon::Protect,
                    army: Army::from([(fleet_unit, 1)]),
                    bombing: BombingRaid::None,
                    combat_probes: false,
                    deep_cover: false,
                    jump_gate: false,
                }],
            ),
        ))
        .unwrap(),
        SubmissionDisposition::Inserted
    );

    let revoked =
        block_on(backend.set_protection_permission(&host, &active.id, protected, 2, false))
            .unwrap();
    assert_eq!(revoked.revision, granted.revision + 1);
    let after = block_on(backend.load_game(&guests[0], &active.id)).unwrap();
    assert!(!after.persisted.state.map.get(protected).allows_protection(2));
    assert_eq!(
        after
            .persisted
            .state
            .player(2)
            .unwrap()
            .protection_controller(after.persisted.state.map.get(protected)),
        Some(1)
    );
    assert!(!after.submitted_players.contains(&2));
    let drafts = block_on(backend.load_turn_submissions(
        &guests[0],
        &active.id,
        after.persisted.state.turn,
        TurnSubmissionScope::All,
    ))
    .unwrap();
    assert_eq!(drafts.len(), 1);
    assert!(!drafts[0].ready);
    let replay = block_on(backend.subscribe(&guests[0], &active.id, 0)).unwrap();
    assert_eq!(
        replay
            .events
            .iter()
            .filter(|event| event.kind == BackendEventKind::ProtectionChanged)
            .count(),
        2
    );
    assert!(replay.events.iter().any(|event| {
        event.kind == BackendEventKind::TurnWithdrawn && event.player_id == Some(2)
    }));
}

#[test]
fn joint_attack_invitations_are_private_live_and_reject_attacking_your_own_world() {
    let backend = InMemoryBackend::new();
    let (host, host_recovery) = identity(&backend);
    let created = create(&backend, &host, &host_recovery, 3);
    let mut sessions = Vec::new();
    let mut game = created.game;
    for slot in 2..=3 {
        let (guest, recovery) = identity(&backend);
        game = block_on(backend.join_game(
            &guest,
            JoinGameRequest {
                code: game.code.clone(),
                display_name: format!("Guest {slot}"),
                recovery_code: recovery.expose().to_string(),
            },
        ))
        .unwrap()
        .game;
        sessions.push(guest);
    }
    game.persisted.state.start().unwrap();
    let active =
        block_on(backend.start_game(&host, &game.id, game.revision, game.persisted)).unwrap();
    let host_home = active.persisted.state.player(1).unwrap().home_planet;
    let guest_home = active.persisted.state.player(2).unwrap().home_planet;
    let outsider_home = active.persisted.state.player(3).unwrap().home_planet;
    let fighter = Unit::Ship(Ship::LightFighter);
    let contribution = |player_id, origin| crate::core::simulation::JointAttackContribution {
        player_id,
        origin,
        army: Army::from([(fighter, 1)]),
        bombing: crate::core::missions::BombingRaid::None,
        combat_probes: false,
    };
    let invitation = JointAttackInvitation {
        revision: 0,
        id: 501,
        turn: active.persisted.state.turn,
        inviter: 1,
        destination: outsider_home,
        objective: Icon::Attack,
        bombing: BombingRaid::None,
        combat_probes: false,
        canceled: false,
        launched: false,
        participants: vec![
            JointAttackParticipant {
                player_id: 1,
                response: JointAttackResponse::Accepted,
                contribution: Some(contribution(1, host_home)),
            },
            JointAttackParticipant {
                player_id: 2,
                response: JointAttackResponse::Pending,
                contribution: None,
            },
        ],
    };
    block_on(backend.create_joint_attack(&host, &active.id, invitation.clone())).unwrap();
    let mut removed_invitee = invitation.clone();
    removed_invitee.participants[1].player_id = 3;
    assert!(matches!(
        block_on(backend.create_joint_attack(&host, &active.id, removed_invitee)),
        Err(BackendError::InvalidData(field)) if field == "joint_attack_invitees"
    ));
    {
        let mut state = backend.lock().unwrap();
        let model = &mut state.games.get_mut(&active.id).unwrap().record.persisted.state;
        set_protection_permission_immediately(model, 3, outsider_home, 2, true).unwrap();
        model.map.get_mut(outsider_home).army.dock_protector(2, Army::from([(fighter, 1)]));
        assert!(model.map.get(outsider_home).blocks_hostile_action_by(2));
    }
    assert!(matches!(
        block_on(backend.respond_joint_attack(
            &sessions[0],
            &active.id,
            invitation.id,
            0,
            JointAttackResponse::Accepted,
            Some(contribution(2, guest_home)),
        )),
        Err(BackendError::Forbidden)
    ));
    {
        let mut state = backend.lock().unwrap();
        let model = &mut state.games.get_mut(&active.id).unwrap().record.persisted.state;
        set_protection_permission_immediately(model, 3, outsider_home, 2, false).unwrap();
        assert!(model.map.get(outsider_home).is_protected_by(2));
        assert!(!model.map.get(outsider_home).blocks_hostile_action_by(2));
    }
    assert_eq!(block_on(backend.load_joint_attacks(&host, &active.id)).unwrap().len(), 1);
    assert_eq!(block_on(backend.load_joint_attacks(&sessions[0], &active.id)).unwrap().len(), 1);
    assert!(block_on(backend.load_joint_attacks(&sessions[1], &active.id)).unwrap().is_empty());
    let outsider_replay = block_on(backend.subscribe(&sessions[1], &active.id, 0)).unwrap();
    assert!(!outsider_replay
        .events
        .iter()
        .any(|event| event.kind == BackendEventKind::JointAttackChanged));
    assert!(outsider_replay.cursor > 0);

    let accepted = block_on(backend.respond_joint_attack(
        &sessions[0],
        &active.id,
        invitation.id,
        0,
        JointAttackResponse::Accepted,
        Some(contribution(2, guest_home)),
    ))
    .unwrap();
    assert_eq!(accepted.participants[1].response, JointAttackResponse::Accepted);
    assert!(accepted.participants[1].contribution.is_some());
    let exact_contributions = accepted
        .participants
        .iter()
        .filter_map(|participant| participant.contribution.clone())
        .collect::<Vec<_>>();
    let exact_launch = TurnSubmission::new(
        1,
        active.persisted.state.turn,
        vec![TurnCommand::SendJointMission {
            attack_id: invitation.id,
            mission_id: 8_001,
            destination: invitation.destination,
            objective: invitation.objective,
            bombing: invitation.bombing.clone(),
            combat_probes: invitation.combat_probes,
            contributions: exact_contributions,
        }],
    );
    {
        let mut state = backend.lock().unwrap();
        let stored = state.games.get_mut(&active.id).unwrap();
        assert!(validate_joint_attack_commands(stored, &exact_launch).is_ok());
        let mut spoofed_launch = exact_launch.clone();
        if let TurnCommand::SendJointMission {
            contributions,
            ..
        } = &mut spoofed_launch.commands[0]
        {
            contributions.pop();
        }
        assert!(validate_joint_attack_commands(stored, &spoofed_launch).is_err());
        freeze_joint_attack_launches(stored, &exact_launch);
        stored.submissions.insert(
            (active.persisted.state.turn, 1),
            StoredTurnSubmission {
                submission: exact_launch.clone(),
                digest: "saved-joint-launch".to_owned(),
                ready: false,
            },
        );
    }
    assert!(matches!(
        block_on(backend.cancel_joint_attack(&host, &active.id, invitation.id)),
        Err(BackendError::Forbidden)
    ));
    assert!(matches!(
        block_on(backend.respond_joint_attack(
            &sessions[0],
            &active.id,
            invitation.id,
            0,
            JointAttackResponse::Rejected,
            None,
        )),
        Err(BackendError::Forbidden)
    ));

    let mut rejected_invitation = invitation.clone();
    rejected_invitation.id = 502;
    block_on(backend.create_joint_attack(&host, &active.id, rejected_invitation.clone())).unwrap();
    let rejected = block_on(backend.respond_joint_attack(
        &sessions[0],
        &active.id,
        rejected_invitation.id,
        0,
        JointAttackResponse::Rejected,
        None,
    ))
    .unwrap();
    assert_eq!(rejected.participants[1].response, JointAttackResponse::Rejected);
    assert!(rejected.participants[1].contribution.is_none());

    let mut canceled_invitation = invitation.clone();
    canceled_invitation.id = 504;
    block_on(backend.create_joint_attack(&host, &active.id, canceled_invitation.clone())).unwrap();
    let accepted_before_cancel = block_on(backend.respond_joint_attack(
        &sessions[0],
        &active.id,
        canceled_invitation.id,
        0,
        JointAttackResponse::Accepted,
        Some(contribution(2, guest_home)),
    ))
    .unwrap();
    assert!(matches!(
        block_on(backend.cancel_joint_attack(&sessions[0], &active.id, canceled_invitation.id,)),
        Err(BackendError::Forbidden)
    ));
    let canceled =
        block_on(backend.cancel_joint_attack(&host, &active.id, canceled_invitation.id)).unwrap();
    assert!(canceled.canceled);
    assert!(block_on(backend.load_joint_attacks(&sessions[0], &active.id))
        .unwrap()
        .iter()
        .any(|invitation| invitation.id == canceled.id && invitation.canceled));
    assert!(matches!(
        block_on(backend.respond_joint_attack(
            &sessions[0],
            &active.id,
            canceled_invitation.id,
            0,
            JointAttackResponse::Rejected,
            None,
        )),
        Err(BackendError::Forbidden)
    ));
    let canceled_launch = TurnSubmission::new(
        1,
        active.persisted.state.turn,
        vec![TurnCommand::SendJointMission {
            attack_id: canceled.id,
            mission_id: 8_002,
            destination: canceled.destination,
            objective: canceled.objective,
            bombing: canceled.bombing.clone(),
            combat_probes: canceled.combat_probes,
            contributions: accepted_before_cancel
                .participants
                .iter()
                .filter_map(|participant| participant.contribution.clone())
                .collect(),
        }],
    );
    {
        let state = backend.lock().unwrap();
        let stored = state.games.get(&active.id).unwrap();
        assert!(validate_joint_attack_commands(stored, &canceled_launch).is_err());
    }

    let mut own_target = invitation;
    own_target.id = 503;
    own_target.participants[1] = JointAttackParticipant {
        player_id: 3,
        response: JointAttackResponse::Pending,
        contribution: None,
    };
    block_on(backend.create_joint_attack(&host, &active.id, own_target.clone())).unwrap();
    assert!(matches!(
        block_on(backend.respond_joint_attack(
            &sessions[1],
            &active.id,
            own_target.id,
            0,
            JointAttackResponse::Accepted,
            Some(contribution(3, outsider_home)),
        )),
        Err(BackendError::Forbidden)
    ));
}

#[test]
/// Only the host can release an active match, and only after every member reconnects.
fn resumed_game_waits_for_every_connected_player() {
    let backend = InMemoryBackend::new();
    let (creator, creator_recovery) = identity(&backend);
    let created = create(&backend, &creator, &creator_recovery, 2);
    let (joiner, joiner_recovery) = identity(&backend);
    let joined = block_on(backend.join_game(
        &joiner,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Joiner".to_string(),
            recovery_code: joiner_recovery.expose().to_string(),
        },
    ))
    .unwrap();
    let mut model = GameModel::new([51; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let active = block_on(backend.start_game(
        &creator,
        &joined.game.id,
        joined.game.revision,
        PersistedGame::new(model),
    ))
    .unwrap();

    block_on(backend.set_connected(&creator, &active.id, true)).unwrap();
    assert_eq!(
        block_on(backend.resume_game(&creator, &active.id)),
        Err(BackendError::InvalidGameStatus)
    );
    block_on(backend.set_connected(&joiner, &active.id, true)).unwrap();
    assert_eq!(block_on(backend.resume_game(&joiner, &active.id)), Err(BackendError::Forbidden));
    // Both windows were online, but one disappears without sending a disconnect.
    // Reject stale presence at the RPC boundary even before anyone reloads the roster.
    for (player_id, auth) in [(1, &creator), (2, &joiner)] {
        let cursor = block_on(backend.subscribe(&creator, &active.id, 0)).unwrap().cursor;
        {
            let mut state = backend.inner.lock().unwrap();
            let stored = state.games.get_mut(&active.id).unwrap();
            assert!(stored.record.members.iter().all(|member| member.connected));
            stored.connected_players.insert(player_id, Instant::now() - PLAYER_CONNECTION_TIMEOUT);
        }
        assert_eq!(
            block_on(backend.resume_game(&creator, &active.id)),
            Err(BackendError::InvalidGameStatus)
        );
        let loaded = block_on(backend.load_game(&creator, &active.id)).unwrap();
        assert_eq!(loaded.revision, active.revision);
        assert_eq!(loaded.status, MatchStatus::Active);
        assert!(loaded
            .members
            .iter()
            .all(|member| member.connected == (member.player_id != player_id)));
        // Read-only loads and event polls must not renew the missing client's lease.
        assert!(block_on(backend.subscribe(&creator, &active.id, cursor))
            .unwrap()
            .events
            .is_empty());
        assert_eq!(
            block_on(backend.resume_game(&creator, &active.id)),
            Err(BackendError::InvalidGameStatus)
        );
        block_on(backend.set_connected(auth, &active.id, true)).unwrap();
        let reconnect = block_on(backend.subscribe(&creator, &active.id, cursor)).unwrap();
        assert_eq!(reconnect.events.len(), 1);
        assert_eq!(reconnect.events[0].kind, BackendEventKind::PlayerConnected);
        assert_eq!(reconnect.events[0].player_id, Some(player_id));
        block_on(backend.set_connected(auth, &active.id, true)).unwrap();
        assert!(block_on(backend.subscribe(&creator, &active.id, reconnect.cursor))
            .unwrap()
            .events
            .is_empty());
    }
    let before = block_on(backend.subscribe(&creator, &active.id, 0)).unwrap().cursor;
    block_on(backend.resume_game(&creator, &active.id)).unwrap();
    let release = block_on(backend.subscribe(&creator, &active.id, before)).unwrap();
    assert!(release.events.iter().any(|event| event.kind == BackendEventKind::GameResumed));
    assert!(block_on(backend.load_game(&creator, &active.id))
        .unwrap()
        .members
        .iter()
        .all(|member| member.connected));
}

#[test]
/// Restores the same anonymous identity and automatically finds its player slot.
fn reconnect_uses_authenticated_mapping() {
    let backend = InMemoryBackend::new();
    let (session, recovery) = identity(&backend);
    let created = create(&backend, &session, &recovery, 2);
    assert!(block_on(backend.list_games(&session)).unwrap().is_empty());
    start_with_guest(&backend, &session, &created.game);
    let restored = block_on(backend.authenticate(Some(&session))).unwrap();
    let games = block_on(backend.list_games(&restored)).unwrap();
    assert_eq!(games.len(), 1);
    assert_eq!(games[0].player_id, created.membership.player_id);
}

#[test]
fn resume_identity_is_specific_to_each_game_and_survives_recovery() {
    let backend = InMemoryBackend::new();
    let (host, recovery) = identity(&backend);
    let (restored, _) = identity(&backend);
    let mut expected = Vec::new();
    for (name, color) in
        [("Nova", PlayerColor::new(4).unwrap()), ("Orion", PlayerColor::for_player(1))]
    {
        let mut model = GameModel::new([7; 32], GameRules::default()).unwrap();
        model.player_mut(1).unwrap().color = color;
        let created = block_on(backend.create_game(
            &host,
            CreateGameRequest {
                code: generate_game_code().unwrap(),
                display_name: name.to_string(),
                recovery_code: recovery.expose().to_string(),
                persisted: PersistedGame::new(model),
            },
        ))
        .unwrap();
        start_with_guest(&backend, &host, &created.game);
        expected.push((created.game.id.clone(), name, color));
        let listed = block_on(backend.list_games(&host)).unwrap();
        let summary = listed.iter().find(|summary| summary.id == created.game.id).unwrap();
        assert_eq!(summary.display_name, name);
        assert_eq!(summary.recovery_code, recovery.expose());
        assert_eq!(summary.player_color, expected.last().unwrap().2);
        block_on(backend.recover_player(
            &restored,
            RecoverPlayerRequest {
                code: created.game.code,
                recovery_code: recovery.expose().to_string(),
            },
        ))
        .unwrap();
    }
    assert!(block_on(backend.list_games(&host)).unwrap().is_empty());
    let listed = block_on(backend.list_games(&restored)).unwrap();
    assert_eq!(listed.len(), expected.len());
    for (id, name, color) in expected {
        let summary = listed.iter().find(|summary| summary.id == id).unwrap();
        assert_eq!(summary.display_name, name);
        assert_eq!(summary.player_color, color);
    }
}

#[test]
fn expired_games_are_deleted_with_their_memberships_and_codes() {
    let backend = InMemoryBackend::new();
    let now = Instant::now() + Duration::from_secs(100 * 60 * 60);
    backend.lock().unwrap().now = Some(now);
    let (session, recovery) = identity(&backend);
    let (stranger, _) = identity(&backend);
    let mut expected = Vec::new();
    let mut expired = Vec::new();
    for (status, age, visible) in [
        (MatchStatus::Lobby, 96, false),
        (MatchStatus::Active, 96, true),
        (MatchStatus::Finished, 47, true),
        (MatchStatus::Finished, 48, false),
        (MatchStatus::Finished, 49, false),
    ] {
        let created = create(&backend, &session, &recovery, 2);
        {
            let mut state = backend.lock().unwrap();
            let stored = state.games.get_mut(&created.game.id).unwrap();
            stored.record.status = status;
            stored.record.persisted.state.status = status;
            stored.finished_at = Some(now - Duration::from_secs(age * 60 * 60));
        }
        if visible {
            expected.push(created.game.id.clone());
            let loaded = block_on(backend.load_game(&session, &created.game.id)).unwrap();
            assert!(loaded.membership_for(&session.user_id).is_some());
        } else if status == MatchStatus::Finished {
            expired.push(created.game);
        }
    }
    let listed = block_on(backend.list_games(&session)).unwrap();
    assert_eq!(listed.iter().map(|game| game.id.clone()).collect::<Vec<_>>(), expected);
    assert!(block_on(backend.list_games(&stranger)).unwrap().is_empty());
    assert_eq!(backend.lock().unwrap().games.len(), 3);
    assert_eq!(backend.lock().unwrap().codes.len(), 3);
    for game in expired {
        assert!(matches!(
            block_on(backend.load_game(&session, &game.id)),
            Err(BackendError::GameNotFound)
        ));
        assert!(matches!(
            block_on(backend.join_game(
                &session,
                JoinGameRequest {
                    code: game.code.clone(),
                    display_name: "Creator".to_string(),
                    recovery_code: recovery.expose().to_string(),
                }
            )),
            Err(BackendError::GameNotFound)
        ));
        assert!(matches!(
            block_on(backend.recover_player(
                &stranger,
                RecoverPlayerRequest {
                    code: game.code,
                    recovery_code: recovery.expose().to_string(),
                }
            )),
            Err(BackendError::GameNotFound)
        ));
    }
}

#[test]
fn saving_or_reconnecting_does_not_restart_finished_game_retention() {
    let backend = InMemoryBackend::new();
    let now = Instant::now() + FINISHED_GAME_RETENTION;
    backend.lock().unwrap().now = Some(now);
    let (session, recovery) = identity(&backend);
    let created = create(&backend, &session, &recovery, 2);
    let finished_at = {
        let mut state = backend.lock().unwrap();
        let stored = state.games.get_mut(&created.game.id).unwrap();
        let mut finished = stored.record.persisted.clone();
        finished.state.status = MatchStatus::Finished;
        commit_state(stored, finished);
        assert!(stored.finished_at.is_some());
        let at = now - Duration::from_secs(47 * 60 * 60);
        stored.finished_at = Some(at);
        at
    };
    block_on(backend.set_connected(&session, &created.game.id, true)).unwrap();
    let loaded = block_on(backend.load_game(&session, &created.game.id)).unwrap();
    assert!(matches!(
        block_on(backend.save_game(
            &session,
            &loaded.id,
            loaded.revision,
            TurnSubmission::new(1, loaded.persisted.state.turn, vec![]),
        )),
        Err(BackendError::InvalidGameStatus)
    ));
    assert_eq!(backend.lock().unwrap().games[&created.game.id].finished_at, Some(finished_at));
    assert_eq!(block_on(backend.list_games(&session)).unwrap().len(), 1);
    backend.lock().unwrap().games.get_mut(&created.game.id).unwrap().finished_at =
        Some(now - FINISHED_GAME_RETENTION);
    assert!(block_on(backend.list_games(&session)).unwrap().is_empty());
    assert!(matches!(
        block_on(backend.load_game(&session, &created.game.id)),
        Err(BackendError::GameNotFound)
    ));
}

#[test]
fn last_save_retention_applies_to_every_game_status() {
    let backend = InMemoryBackend::new();
    let (session, recovery) = identity(&backend);
    for status in [MatchStatus::Lobby, MatchStatus::Active, MatchStatus::Finished] {
        for days in [29, 30, 31] {
            let created = create(&backend, &session, &recovery, 2);
            {
                let mut state = backend.lock().unwrap();
                let stored = state.games.get_mut(&created.game.id).unwrap();
                stored.record.status = status;
                stored.record.persisted.state.status = status;
                stored.record.saved_at = current_unix_timestamp() - days * 24 * 60 * 60;
                stored.finished_at = (status == MatchStatus::Finished).then(Instant::now);
            }
            let loaded = block_on(backend.load_game(&session, &created.game.id));
            if days < 30 {
                assert!(loaded.is_ok());
            } else {
                assert!(matches!(loaded, Err(BackendError::GameNotFound)));
                assert!(!backend.lock().unwrap().codes.contains_key(&created.game.code));
            }
        }
    }
    assert_eq!(backend.lock().unwrap().games.len(), 3);
}

#[test]
fn only_snapshot_saves_extend_save_retention() {
    let backend = InMemoryBackend::new();
    let (session, recovery) = identity(&backend);
    let created = create(&backend, &session, &recovery, 2);
    let active = start_with_guest(&backend, &session, &created.game);
    let old_save = current_unix_timestamp() - 29 * 24 * 60 * 60;
    backend.lock().unwrap().games.get_mut(&created.game.id).unwrap().record.saved_at = old_save;
    block_on(backend.set_connected(&session, &created.game.id, true)).unwrap();
    let loaded = block_on(backend.load_game(&session, &created.game.id)).unwrap();
    assert_eq!(loaded.saved_at, old_save);
    block_on(backend.save_game(
        &session,
        &loaded.id,
        loaded.revision,
        TurnSubmission::new(1, active.persisted.state.turn, vec![]),
    ))
    .unwrap();
    assert!(backend.lock().unwrap().games[&created.game.id].record.saved_at > old_save);
}

#[test]
/// Recovery replaces the user while preserving the stable per-game code.
fn recovers_from_another_identity_and_preserves_code() {
    let backend = InMemoryBackend::new();
    let (old_session, recovery) = identity(&backend);
    let created = create(&backend, &old_session, &recovery, 2);
    let game = start_with_guest(&backend, &old_session, &created.game);
    let (new_session, _) = identity(&backend);
    let recovered = block_on(backend.recover_player(
        &new_session,
        RecoverPlayerRequest {
            code: game.code.clone(),
            recovery_code: recovery.expose().to_string(),
        },
    ))
    .unwrap();
    assert_eq!(recovered.membership.user_id, new_session.user_id);
    assert_eq!(recovered.membership.identity_version, 2);
    assert_eq!(recovered.recovery_code, recovery.expose());
    assert!(matches!(
        block_on(backend.load_game(&old_session, &game.id)),
        Err(BackendError::Forbidden)
    ));
    block_on(backend.set_connected(&new_session, &game.id, false)).unwrap();
    let (third_session, _) = identity(&backend);
    let recovered_again = block_on(backend.recover_player(
        &third_session,
        RecoverPlayerRequest {
            code: game.code,
            recovery_code: recovery.expose().to_string(),
        },
    ))
    .unwrap();
    assert_eq!(recovered_again.membership.identity_version, 3);
    assert_eq!(recovered_again.recovery_code, recovery.expose());
}

#[test]
fn recovery_protects_live_players_and_releases_abandoned_or_departed_players() {
    let backend = InMemoryBackend::new();
    let (host, original) = identity(&backend);
    let lobby = create(&backend, &host, &original, 2).game;
    let game = start_with_guest(&backend, &host, &lobby);
    block_on(backend.set_connected(&host, &game.id, true)).unwrap();
    let before = block_on(backend.load_game(&host, &game.id)).unwrap();
    let (second, _) = identity(&backend);
    let request = RecoverPlayerRequest {
        code: game.code.clone(),
        recovery_code: original.expose().to_string(),
    };
    assert_eq!(
        block_on(backend.recover_player(&second, request.clone())).err(),
        Some(BackendError::RecoveryCodeInUse)
    );
    assert_eq!(
        serde_json::to_value(block_on(backend.load_game(&host, &game.id)).unwrap()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert_eq!(backend.lock().unwrap().games[&game.id].recovery_codes[&1], original.expose());

    // Heartbeats extend the guard without generating another connected event.
    backend
        .lock()
        .unwrap()
        .games
        .get_mut(&game.id)
        .unwrap()
        .connected_players
        .insert(1, Instant::now() - PLAYER_CONNECTION_TIMEOUT / 2);
    let cursor = block_on(backend.subscribe(&host, &game.id, 0)).unwrap().cursor;
    block_on(backend.set_connected(&host, &game.id, true)).unwrap();
    assert!(block_on(backend.subscribe(&host, &game.id, cursor)).unwrap().events.is_empty());
    assert_eq!(
        block_on(backend.recover_player(&second, request.clone())).err(),
        Some(BackendError::RecoveryCodeInUse)
    );
    backend
        .lock()
        .unwrap()
        .games
        .get_mut(&game.id)
        .unwrap()
        .connected_players
        .insert(1, Instant::now() - PLAYER_CONNECTION_TIMEOUT);
    let recovered = block_on(backend.recover_player(&second, request.clone())).unwrap();
    assert!(recovered.membership.connected);
    assert_eq!(recovered.membership.player_id, 1);
    assert!(recovered.membership.is_creator);

    let (third, _) = identity(&backend);
    assert_eq!(
        block_on(backend.recover_player(&third, request.clone())).err(),
        Some(BackendError::RecoveryCodeInUse)
    );
    // No separate presence call is needed to protect a just-recovered slot.
    assert!(block_on(backend.load_game(&second, &game.id)).is_ok());
    block_on(backend.set_connected(&second, &game.id, false)).unwrap();
    let recovered_again = block_on(backend.recover_player(&third, request)).unwrap();
    assert_eq!(recovered_again.recovery_code, original.expose());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn simultaneous_recovery_claims_accept_one_player_without_displacing_them() {
    let backend = InMemoryBackend::new();
    let (host, code) = identity(&backend);
    let game = create(&backend, &host, &code, 2).game;
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let handles = (0..2)
        .map(|_| {
            let backend = backend.clone();
            let (auth, _) = identity(&backend);
            let barrier = barrier.clone();
            let request = RecoverPlayerRequest {
                code: game.code.clone(),
                recovery_code: code.expose().to_string(),
            };
            std::thread::spawn(move || {
                barrier.wait();
                let result = block_on(backend.recover_player(&auth, request));
                (auth, result)
            })
        })
        .collect::<Vec<_>>();
    let results = handles.into_iter().map(|handle| handle.join().unwrap()).collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|(_, result)| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|(_, result)| matches!(result, Err(BackendError::RecoveryCodeInUse)))
            .count(),
        1
    );
    let (winner, result) = results.iter().find(|(_, result)| result.is_ok()).unwrap();
    assert_eq!(
        block_on(backend.load_game(winner, &game.id)).unwrap().members[0],
        result.as_ref().unwrap().membership
    );
}

#[test]
/// Recovery distinguishes unknown games, malformed codes, existing members, and bad secrets.
fn reports_typed_recovery_failures() {
    let backend = InMemoryBackend::new();
    let (creator, recovery) = identity(&backend);
    let created = create(&backend, &creator, &recovery, 2);
    let (stranger, _) = identity(&backend);

    assert!(matches!(
        block_on(backend.recover_player(
            &stranger,
            RecoverPlayerRequest {
                code: GameCode::new("ABCDEF"),
                recovery_code: recovery.expose().to_string(),
            },
        )),
        Err(BackendError::GameNotFound)
    ));
    assert!(matches!(
        block_on(backend.recover_player(
            &stranger,
            RecoverPlayerRequest {
                code: created.game.code.clone(),
                recovery_code: "not-a-recovery-code".to_string(),
            },
        )),
        Err(BackendError::InvalidData(_))
    ));
    assert!(matches!(
        block_on(backend.recover_player(
            &creator,
            RecoverPlayerRequest {
                code: created.game.code.clone(),
                recovery_code: recovery.expose().to_string(),
            },
        )),
        Err(BackendError::AlreadyMember)
    ));
    let unrelated = RecoveryCode::generate().unwrap();
    assert!(matches!(
        block_on(backend.recover_player(
            &stranger,
            RecoverPlayerRequest {
                code: created.game.code,
                recovery_code: unrelated.expose().to_string(),
            },
        )),
        Err(BackendError::InvalidRecoveryCode)
    ));
}

#[test]
/// Any member may save without advancing the world, while stale snapshots are rejected.
fn saves_by_multiple_players_keep_the_world_revision_and_reject_stale_snapshots() {
    let backend = InMemoryBackend::new();
    let (creator, creator_recovery) = identity(&backend);
    let created = create(&backend, &creator, &creator_recovery, 2);
    let (joiner, joiner_recovery) = identity(&backend);
    let joined = block_on(backend.join_game(
        &joiner,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Joiner".to_string(),
            recovery_code: joiner_recovery.expose().to_string(),
        },
    ))
    .unwrap();
    let mut starting = joined.game.persisted.clone();
    starting.state.start().unwrap();
    let loaded =
        block_on(backend.start_game(&creator, &created.game.id, joined.game.revision, starting))
            .unwrap();
    let creator_saved = block_on(backend.save_game(
        &creator,
        &created.game.id,
        loaded.revision,
        TurnSubmission::new(1, loaded.persisted.state.turn, vec![]),
    ))
    .unwrap();
    let saved = block_on(backend.save_game(
        &joiner,
        &created.game.id,
        creator_saved.revision,
        TurnSubmission::new(2, loaded.persisted.state.turn, vec![]),
    ))
    .unwrap();
    assert_eq!(creator_saved.revision, loaded.revision);
    assert_eq!(saved.revision, loaded.revision);
    let stale_revision = loaded.revision.saturating_sub(1);
    assert!(matches!(
        block_on(backend.save_game(
            &creator,
            &created.game.id,
            stale_revision,
            TurnSubmission::new(1, loaded.persisted.state.turn, vec![]),
        )),
        Err(BackendError::Conflict { expected, actual })
            if expected == stale_revision && actual == loaded.revision
    ));
    let drafts = block_on(backend.load_turn_submissions(
        &creator,
        &created.game.id,
        loaded.persisted.state.turn,
        TurnSubmissionScope::All,
    ))
    .unwrap();
    assert_eq!(drafts.len(), 2);
    assert!(drafts.iter().all(|draft| !draft.ready));
}

#[test]
/// The canonical checkpoint remains discoverable and exact after restoring a local session.
fn resumes_exact_saved_state() {
    let backend = InMemoryBackend::new();
    let (creator, recovery) = identity(&backend);
    let created = create(&backend, &creator, &recovery, 2);
    let started = start_with_guest(&backend, &creator, &created.game);
    let expected_metal = started.persisted.state.players[0].resources.metal;
    let saved = block_on(backend.save_game(
        &creator,
        &created.game.id,
        started.revision,
        TurnSubmission::new(1, started.persisted.state.turn, vec![]),
    ))
    .unwrap();

    let restored = block_on(backend.authenticate(Some(&creator))).unwrap();
    let listed = block_on(backend.list_games(&restored)).unwrap();
    assert_eq!(listed[0].revision, saved.revision);
    assert!(saved.saved_at > 0);
    assert_eq!(listed[0].saved_at, saved.saved_at);
    let resumed = block_on(backend.load_game(&restored, &created.game.id)).unwrap();
    assert_eq!(resumed.saved_at, saved.saved_at);
    assert_eq!(resumed.persisted.state.players[0].resources.metal, expected_metal);
}

/// Starts a two-player test lobby through the normal join/start contract.
fn start_with_guest(
    backend: &InMemoryBackend,
    host: &AuthSession,
    lobby: &GameRecord,
) -> GameRecord {
    let (guest, recovery) = identity(backend);
    let joined = block_on(backend.join_game(
        &guest,
        JoinGameRequest {
            code: lobby.code.clone(),
            display_name: "Guest".to_string(),
            recovery_code: recovery.expose().to_string(),
        },
    ))
    .unwrap();
    let mut persisted = joined.game.persisted;
    persisted.state.start().unwrap();
    block_on(backend.start_game(host, &lobby.id, joined.game.revision, persisted)).unwrap()
}

#[test]
fn host_departure_erases_empty_and_occupied_lobbies() {
    for guests in [0, 1, 3] {
        let backend = InMemoryBackend::new();
        let (host, host_recovery) = identity(&backend);
        let lobby = create(&backend, &host, &host_recovery, 4).game;
        block_on(backend.set_connected(&host, &lobby.id, true)).unwrap();
        let mut members = vec![(host.clone(), host_recovery)];
        for index in 0..guests {
            let (guest, recovery) = identity(&backend);
            block_on(backend.join_game(
                &guest,
                JoinGameRequest {
                    code: lobby.code.clone(),
                    display_name: format!("Guest {index}"),
                    recovery_code: recovery.expose().to_string(),
                },
            ))
            .unwrap();
            block_on(backend.set_connected(&guest, &lobby.id, true)).unwrap();
            block_on(backend.set_connected(&guest, &lobby.id, false)).unwrap();
            assert!(block_on(backend.load_game(&host, &lobby.id)).is_ok());
            block_on(backend.set_connected(&guest, &lobby.id, true)).unwrap();
            members.push((guest, recovery));
        }
        let (stranger, replacement) = identity(&backend);
        assert_eq!(
            block_on(backend.set_connected(&stranger, &lobby.id, false)),
            Err(BackendError::Forbidden)
        );
        for (member, _) in &members {
            assert!(block_on(backend.list_games(member)).unwrap().is_empty());
        }
        block_on(backend.set_connected(&host, &lobby.id, false)).unwrap();
        assert!(backend.lock().unwrap().games.is_empty());
        assert!(backend.lock().unwrap().codes.is_empty());
        for (member, recovery) in members {
            assert!(matches!(
                block_on(backend.load_game(&member, &lobby.id)),
                Err(BackendError::GameNotFound)
            ));
            assert_eq!(
                block_on(backend.subscribe(&member, &lobby.id, 0)),
                Err(BackendError::GameNotFound)
            );
            assert!(block_on(backend.list_games(&member)).unwrap().is_empty());
            assert!(matches!(
                block_on(backend.recover_player(
                    &stranger,
                    RecoverPlayerRequest {
                        code: lobby.code.clone(),
                        recovery_code: recovery.expose().to_string(),
                    }
                )),
                Err(BackendError::GameNotFound)
            ));
        }
        assert!(matches!(
            block_on(backend.join_game(
                &stranger,
                JoinGameRequest {
                    code: lobby.code.clone(),
                    display_name: "Guest".to_string(),
                    recovery_code: replacement.expose().to_string(),
                }
            )),
            Err(BackendError::GameNotFound)
        ));
        // The old code is free again; there is no retained deletion record.
        assert!(block_on(backend.create_game(
            &host,
            CreateGameRequest {
                code: lobby.code,
                display_name: "Host".to_string(),
                recovery_code: replacement.expose().to_string(),
                persisted: lobby.persisted,
            }
        ))
        .is_ok());
    }
}

#[test]
fn host_departure_preserves_started_games() {
    let backend = InMemoryBackend::new();
    let (host, recovery) = identity(&backend);
    let lobby = create(&backend, &host, &recovery, 2).game;
    let started = start_with_guest(&backend, &host, &lobby);
    block_on(backend.set_connected(&host, &started.id, true)).unwrap();
    block_on(backend.set_connected(&host, &started.id, false)).unwrap();
    let loaded = block_on(backend.load_game(&host, &started.id)).unwrap();
    assert_eq!(loaded.status, MatchStatus::Active);
    assert_eq!(loaded.members.len(), 2);
    assert!(!loaded.membership_for(&host.user_id).unwrap().connected);
    assert_eq!(
        serde_json::to_value(&loaded.persisted).unwrap(),
        serde_json::to_value(&started.persisted).unwrap()
    );
    assert_eq!(block_on(backend.list_games(&host)).unwrap().len(), 1);
}

#[test]
/// Invalid player-scoped drafts are rejected before they can advance a revision.
fn rejects_malformed_saved_draft() {
    let backend = InMemoryBackend::new();
    let (creator, recovery) = identity(&backend);
    let created = create(&backend, &creator, &recovery, 2);
    let active = start_with_guest(&backend, &creator, &created.game);
    let malformed = TurnSubmission::new(0, active.persisted.state.turn, vec![]);
    assert!(matches!(
        block_on(backend.save_game(&creator, &created.game.id, active.revision, malformed,)),
        Err(BackendError::InvalidData(_))
    ));
    assert_eq!(
        block_on(backend.load_game(&creator, &created.game.id)).unwrap().revision,
        active.revision
    );
}

#[test]
/// Identical concurrent draft saves are idempotent and do not advance canonical revision.
fn simultaneous_saves_do_not_conflict_or_advance_the_world() {
    use std::sync::{Arc, Barrier};

    let backend = InMemoryBackend::new();
    let (creator, recovery) = identity(&backend);
    let created = create(&backend, &creator, &recovery, 2);
    let active = start_with_guest(&backend, &creator, &created.game);
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let backend = backend.clone();
        let creator = creator.clone();
        let game_id = created.game.id.clone();
        let revision = active.revision;
        let turn = active.persisted.state.turn;
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            block_on(backend.save_game(
                &creator,
                &game_id,
                revision,
                TurnSubmission::new(1, turn, vec![]),
            ))
        }));
    }
    barrier.wait();
    let results = workers.into_iter().map(|worker| worker.join().unwrap()).collect::<Vec<_>>();
    assert!(results.iter().all(Result::is_ok));
    assert!(results.iter().all(|result| result.as_ref().unwrap().revision == active.revision));
}

#[test]
/// Duplicate/stale submissions and competing resolvers have deterministic outcomes.
fn coordinates_idempotent_submission_and_single_resolution() {
    let backend = InMemoryBackend::new();
    let (creator, creator_recovery) = identity(&backend);
    let created = create(&backend, &creator, &creator_recovery, 2);
    let (joiner, joiner_recovery) = identity(&backend);
    let joined = block_on(backend.join_game(
        &joiner,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Joiner".to_string(),
            recovery_code: joiner_recovery.expose().to_string(),
        },
    ))
    .unwrap();
    let mut lobby = joined.game;
    lobby.persisted.state.start().unwrap();
    let active =
        block_on(backend.start_game(&creator, &lobby.id, lobby.revision, lobby.persisted)).unwrap();
    let first = TurnSubmission::new(1, active.persisted.state.turn, Vec::new());
    let second = TurnSubmission::new(2, active.persisted.state.turn, Vec::new());
    assert_eq!(
        block_on(backend.submit_turn(&creator, &active.id, first.clone())).unwrap(),
        SubmissionDisposition::Inserted
    );
    assert_eq!(
        block_on(backend.submit_turn(&creator, &active.id, first)).unwrap(),
        SubmissionDisposition::Duplicate
    );
    assert!(block_on(backend.load_turn_submissions(
        &creator,
        &active.id,
        active.persisted.state.turn,
        TurnSubmissionScope::Resolution,
    ))
    .unwrap()
    .is_empty());
    block_on(backend.submit_turn(&joiner, &active.id, second)).unwrap();
    let submissions = block_on(backend.load_turn_submissions(
        &creator,
        &active.id,
        active.persisted.state.turn,
        TurnSubmissionScope::All,
    ))
    .unwrap();
    assert_eq!(
        serde_json::to_value(
            block_on(backend.load_turn_submissions(
                &creator,
                &active.id,
                active.persisted.state.turn,
                TurnSubmissionScope::Resolution
            ))
            .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&submissions).unwrap(),
    );
    let mine = block_on(backend.load_turn_submissions(
        &joiner,
        &active.id,
        active.persisted.state.turn,
        TurnSubmissionScope::Mine,
    ))
    .unwrap();
    assert_eq!(mine.len(), 1);
    assert_eq!(mine[0].submission.player_id, 2);
    let mut next = active.persisted.state.clone();
    resolve_turn(
        &mut next,
        &submissions.iter().map(|stored| stored.submission.clone()).collect::<Vec<_>>(),
    )
    .unwrap();
    let accepted = block_on(backend.publish_resolution(
        &joiner,
        &active.id,
        active.revision,
        active.persisted.state.turn,
        PersistedGame::new(next.clone()),
    ))
    .unwrap();
    assert_eq!(accepted.persisted.state.turn, active.persisted.state.turn + 1);
    for scope in
        [TurnSubmissionScope::All, TurnSubmissionScope::Mine, TurnSubmissionScope::Resolution]
    {
        assert!(
            block_on(backend.load_turn_submissions(
                &creator,
                &active.id,
                active.persisted.state.turn,
                scope,
            ))
            .unwrap()
            .is_empty(),
            "resolved commands must not be retained"
        );
    }
    assert!(matches!(
        block_on(backend.publish_resolution(
            &creator,
            &active.id,
            active.revision,
            active.persisted.state.turn,
            PersistedGame::new(next),
        )),
        Err(BackendError::Conflict { .. })
    ));
    let stale = TurnSubmission::new(2, active.persisted.state.turn, Vec::new());
    assert!(matches!(
        block_on(backend.submit_turn(&joiner, &active.id, stale)),
        Err(BackendError::StaleSubmission { .. })
    ));
}

#[test]
/// Concurrent player submissions persist once and concurrent resolvers accept one next state.
fn simultaneous_submissions_and_resolvers_are_serialized() {
    use std::sync::{Arc, Barrier};

    let backend = InMemoryBackend::new();
    let (creator, creator_recovery) = identity(&backend);
    let created = create(&backend, &creator, &creator_recovery, 2);
    let (joiner, joiner_recovery) = identity(&backend);
    let joined = block_on(backend.join_game(
        &joiner,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Joiner".to_string(),
            recovery_code: joiner_recovery.expose().to_string(),
        },
    ))
    .unwrap();
    let mut lobby = joined.game;
    lobby.persisted.state.start().unwrap();
    let active =
        block_on(backend.start_game(&creator, &lobby.id, lobby.revision, lobby.persisted)).unwrap();

    let barrier = Arc::new(Barrier::new(3));
    let mut submitters = Vec::new();
    for (session, player_id) in [(creator.clone(), 1), (joiner.clone(), 2)] {
        let backend = backend.clone();
        let game_id = active.id.clone();
        let barrier = Arc::clone(&barrier);
        let turn = active.persisted.state.turn;
        submitters.push(std::thread::spawn(move || {
            barrier.wait();
            block_on(backend.submit_turn(
                &session,
                &game_id,
                TurnSubmission::new(player_id, turn, Vec::new()),
            ))
        }));
    }
    barrier.wait();
    for submitter in submitters {
        assert_eq!(submitter.join().unwrap().unwrap(), SubmissionDisposition::Inserted);
    }

    let submissions = block_on(backend.load_turn_submissions(
        &creator,
        &active.id,
        active.persisted.state.turn,
        TurnSubmissionScope::All,
    ))
    .unwrap();
    let mut model = active.persisted.state.clone();
    resolve_turn(
        &mut model,
        &submissions.into_iter().map(|stored| stored.submission).collect::<Vec<_>>(),
    )
    .unwrap();
    let next = PersistedGame::new(model);

    let barrier = Arc::new(Barrier::new(3));
    let mut resolvers = Vec::new();
    for session in [creator, joiner] {
        let backend = backend.clone();
        let game_id = active.id.clone();
        let next = next.clone();
        let barrier = Arc::clone(&barrier);
        let revision = active.revision;
        let turn = active.persisted.state.turn;
        resolvers.push(std::thread::spawn(move || {
            barrier.wait();
            block_on(backend.publish_resolution(&session, &game_id, revision, turn, next))
        }));
    }
    barrier.wait();
    let results =
        resolvers.into_iter().map(|resolver| resolver.join().unwrap()).collect::<Vec<_>>();
    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter(|result| matches!(result, Err(BackendError::Conflict { .. })))
            .count(),
        1
    );
}

#[test]
/// An eliminated slot remains a member for resume/view access but cannot submit a turn.
fn spectator_cannot_submit_turn() {
    let backend = InMemoryBackend::new();
    let (creator, creator_recovery) = identity(&backend);
    let created = create(&backend, &creator, &creator_recovery, 2);
    let (joiner, joiner_recovery) = identity(&backend);
    let joined = block_on(backend.join_game(
        &joiner,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Joiner".to_string(),
            recovery_code: joiner_recovery.expose().to_string(),
        },
    ))
    .unwrap();
    let mut lobby = joined.game;
    let defeated_home = lobby.persisted.state.players[1].home_planet;
    let home = lobby.persisted.state.map.get_mut(defeated_home);
    home.owned = Some(1);
    home.controlled = Some(1);
    lobby.persisted.state.players[1].spectator = true;
    lobby.persisted.state.start().unwrap();
    let defeated = lobby.persisted.clone();
    let active =
        block_on(backend.start_game(&creator, &lobby.id, lobby.revision, lobby.persisted)).unwrap();

    // Simulate an authoritative earlier elimination without using a client snapshot write.
    backend.lock().unwrap().games.get_mut(&active.id).unwrap().record.persisted = defeated;
    assert!(matches!(
        block_on(backend.submit_turn(
            &joiner,
            &active.id,
            TurnSubmission::new(2, active.persisted.state.turn, Vec::new()),
        )),
        Err(BackendError::Forbidden)
    ));
}

#[test]
/// A reconnecting client can replay missed events and then reload current state.
fn reconnect_replays_notifications_and_loads_current_state() {
    let backend = InMemoryBackend::new();
    let (creator, recovery) = identity(&backend);
    let created = create(&backend, &creator, &recovery, 2);
    let started = start_with_guest(&backend, &creator, &created.game);
    let initial = block_on(backend.subscribe(&creator, &created.game.id, 0)).unwrap();
    block_on(backend.set_connected(&creator, &created.game.id, true)).unwrap();
    block_on(backend.set_connected(&creator, &created.game.id, false)).unwrap();
    block_on(backend.set_connected(&creator, &created.game.id, false)).unwrap();
    let caught_up =
        block_on(backend.subscribe(&creator, &created.game.id, initial.cursor)).unwrap();
    assert_eq!(caught_up.events.len(), 2);
    assert_eq!(
        block_on(backend.load_game(&creator, &created.game.id)).unwrap().revision,
        started.revision
    );
}

#[test]
fn joint_planning_publishes_drafts_revises_consent_and_launches_without_pending_guests() {
    use crate::core::missions::BombingRaid;
    use crate::core::simulation::JointAttackContribution;
    let backend = InMemoryBackend::new();
    let (host, recovery) = identity(&backend);
    let created = create(&backend, &host, &recovery, 4);
    let mut game = created.game;
    let mut guests = Vec::new();
    for slot in 2..=4 {
        let (guest, recovery) = identity(&backend);
        game = block_on(backend.join_game(
            &guest,
            JoinGameRequest {
                code: game.code.clone(),
                display_name: format!("Guest {slot}"),
                recovery_code: recovery.expose().to_string(),
            },
        ))
        .unwrap()
        .game;
        guests.push(guest);
    }
    game.persisted.state.start().unwrap();
    let active =
        block_on(backend.start_game(&host, &game.id, game.revision, game.persisted)).unwrap();
    let model = &active.persisted.state;
    let fighter = Unit::Ship(Ship::LightFighter);
    {
        let mut state = backend.lock().unwrap();
        let model = &mut state.games.get_mut(&active.id).unwrap().record.persisted.state;
        for player in &mut model.players {
            player.resources.deuterium = 1_000_000;
            model.map.get_mut(player.home_planet).army.insert(fighter, 10);
        }
    }
    let contribution = |id| JointAttackContribution {
        player_id: id,
        origin: model.player(id).unwrap().home_planet,
        army: Army::from([(fighter, 1)]),
        bombing: BombingRaid::None,
        combat_probes: false,
    };
    let mut invitation = JointAttackInvitation {
        id: 998,
        revision: 0,
        turn: model.turn,
        inviter: 1,
        destination: model.player(4).unwrap().home_planet,
        objective: Icon::Attack,
        bombing: BombingRaid::None,
        combat_probes: false,
        canceled: false,
        launched: false,
        participants: vec![
            JointAttackParticipant {
                player_id: 1,
                response: JointAttackResponse::Accepted,
                contribution: Some(contribution(1)),
            },
            JointAttackParticipant {
                player_id: 2,
                response: JointAttackResponse::Pending,
                contribution: None,
            },
            JointAttackParticipant {
                player_id: 3,
                response: JointAttackResponse::Pending,
                contribution: None,
            },
        ],
    };
    let mut empty_proposal = invitation.clone();
    empty_proposal.participants[0].contribution.as_mut().unwrap().army.clear();
    assert!(matches!(
        block_on(backend.create_joint_attack(&host, &active.id, empty_proposal)),
        Err(BackendError::InvalidData(_))
    ));
    let mut non_ship_proposal = invitation.clone();
    non_ship_proposal.participants[0].contribution.as_mut().unwrap().army =
        Army::from([(Unit::Building(Building::TradingPost), 1)]);
    assert!(matches!(
        block_on(backend.create_joint_attack(&host, &active.id, non_ship_proposal)),
        Err(BackendError::InvalidData(_))
    ));
    block_on(backend.create_joint_attack(&host, &active.id, invitation.clone())).unwrap();
    assert!(block_on(backend.submit_turn(
        &host,
        &active.id,
        TurnSubmission::new(1, model.turn, vec![])
    ))
    .is_err());
    let mut guest_fleet = contribution(2);
    guest_fleet.combat_probes = true;
    let respond = |revision, response, fleet| {
        block_on(backend.respond_joint_attack(
            &guests[0],
            &active.id,
            invitation.id,
            revision,
            response,
            Some(fleet),
        ))
    };
    let mut conflicting_bombing = guest_fleet.clone();
    conflicting_bombing.bombing = BombingRaid::Industrial;
    assert!(matches!(
        respond(0, JointAttackResponse::Pending, conflicting_bombing),
        Err(BackendError::InvalidData(field)) if field == "joint_attack_contribution"
    ));
    let published = respond(0, JointAttackResponse::Pending, guest_fleet.clone()).unwrap();
    invitation.revision = published.revision;
    for viewer in [&host, &guests[1]] {
        let live = block_on(backend.load_joint_attacks(viewer, &active.id)).unwrap();
        assert_eq!(live[0].participants[1].contribution.as_ref(), Some(&guest_fleet));
        assert_eq!(live[0].participants[1].response, JointAttackResponse::Pending);
    }
    respond(invitation.revision, JointAttackResponse::Accepted, guest_fleet.clone()).unwrap();
    let undone =
        respond(invitation.revision, JointAttackResponse::Pending, guest_fleet.clone()).unwrap();
    assert_eq!(undone.participants[1].response, JointAttackResponse::Pending);
    respond(invitation.revision, JointAttackResponse::Accepted, guest_fleet.clone()).unwrap();
    let peer = block_on(backend.respond_joint_attack(
        &guests[1],
        &active.id,
        invitation.id,
        invitation.revision,
        JointAttackResponse::Accepted,
        Some(contribution(3)),
    ))
    .unwrap();
    assert_eq!(peer.participants[0].response, JointAttackResponse::Pending);
    assert_eq!(peer.participants[1].response, JointAttackResponse::Pending);
    invitation.revision = peer.revision;
    respond(invitation.revision, JointAttackResponse::Accepted, guest_fleet.clone()).unwrap();
    let mut edited_fleet = guest_fleet.clone();
    edited_fleet.army.insert(fighter, 2);
    let edited =
        respond(invitation.revision, JointAttackResponse::Accepted, edited_fleet.clone()).unwrap();
    assert_eq!(edited.participants[1].response, JointAttackResponse::Accepted);
    assert_eq!(edited.participants[2].response, JointAttackResponse::Pending);
    assert!(block_on(backend.respond_joint_attack(
        &guests[1],
        &active.id,
        invitation.id,
        invitation.revision,
        JointAttackResponse::Accepted,
        Some(contribution(3))
    ))
    .is_err());
    invitation.revision = edited.revision;
    block_on(backend.respond_joint_attack(
        &guests[1],
        &active.id,
        invitation.id,
        invitation.revision,
        JointAttackResponse::Accepted,
        Some(contribution(3)),
    ))
    .unwrap();
    edited_fleet.origin = contribution(3).origin;
    let routed = respond(invitation.revision, JointAttackResponse::Accepted, edited_fleet).unwrap();
    assert_eq!(routed.participants[1].response, JointAttackResponse::Accepted);
    assert_eq!(routed.participants[2].response, JointAttackResponse::Pending);
    invitation.revision = routed.revision;
    invitation.revision =
        respond(invitation.revision, JointAttackResponse::Accepted, guest_fleet.clone())
            .unwrap()
            .revision;
    // Every owner edit invalidates consent; identical publications preserve it.
    for change in 0..6 {
        let mut proposal = invitation.clone();
        match change {
            0 => proposal.destination = model.player(3).unwrap().home_planet,
            1 => proposal.objective = Icon::Destroy,
            2 => {
                proposal.participants[0].contribution.as_mut().unwrap().army.insert(fighter, 2);
            },
            3 => {
                proposal.participants[0].contribution.as_mut().unwrap().origin =
                    model.player(3).unwrap().home_planet
            },
            4 => {
                proposal.bombing = BombingRaid::Economic;
                proposal.participants[0].contribution.as_mut().unwrap().bombing =
                    BombingRaid::Economic;
            },
            5 => {
                proposal.combat_probes = true;
                proposal.participants[0].contribution.as_mut().unwrap().combat_probes = true;
            },
            _ => unreachable!(),
        }
        let revised = block_on(backend.create_joint_attack(&host, &active.id, proposal)).unwrap();
        assert_eq!(revised.participants[1].response, JointAttackResponse::Pending);
        assert!(revised
            .participants
            .iter()
            .filter_map(|item| item.contribution.as_ref())
            .all(|contribution| contribution.bombing == revised.bombing));
        assert!(respond(invitation.revision, JointAttackResponse::Accepted, guest_fleet.clone())
            .is_err());
        let live = block_on(backend.load_joint_attacks(&guests[0], &active.id)).unwrap();
        assert_eq!(
            serde_json::to_value(&live[0]).unwrap(),
            serde_json::to_value(&revised).unwrap()
        );
        invitation.revision = revised.revision;
        let restored =
            block_on(backend.create_joint_attack(&host, &active.id, invitation.clone())).unwrap();
        invitation.revision = restored.revision;
        respond(invitation.revision, JointAttackResponse::Accepted, guest_fleet.clone()).unwrap();
        let duplicate =
            block_on(backend.create_joint_attack(&host, &active.id, invitation.clone())).unwrap();
        assert_eq!(duplicate.participants[1].response, JointAttackResponse::Accepted);
    }
    // Only the inviter may revise the shared route; guests respond with their own fleet.
    let mut guest_proposal =
        block_on(backend.load_joint_attacks(&guests[0], &active.id)).unwrap().remove(0);
    let previous_destination = guest_proposal.destination;
    guest_proposal.destination = model
        .map
        .planets
        .iter()
        .find(|planet| {
            !planet.is_destroyed
                && planet.owned.is_none()
                && planet.controlled.is_none()
                && planet.id != previous_destination
        })
        .unwrap()
        .id;
    guest_proposal.objective = Icon::Destroy;
    guest_proposal.participants[1].response = JointAttackResponse::Accepted;
    assert!(block_on(backend.create_joint_attack(&guests[0], &active.id, guest_proposal.clone()))
        .is_err());
    assert!(block_on(backend.create_joint_attack(&guests[1], &active.id, guest_proposal)).is_err());
    invitation.objective = Icon::Attack;
    invitation.revision =
        block_on(backend.create_joint_attack(&host, &active.id, invitation.clone()))
            .unwrap()
            .revision;
    let base_revision = invitation.revision;
    // Changing only the mission type invalidates the old agreement and retains fleet drafts.
    invitation.objective = Icon::Colonize;
    let revised =
        block_on(backend.create_joint_attack(&host, &active.id, invitation.clone())).unwrap();
    assert_eq!(revised.revision, base_revision + 1);
    assert_eq!(revised.participants[1].response, JointAttackResponse::Pending);
    assert_eq!(revised.participants[1].contribution.as_ref(), Some(&guest_fleet));
    assert!(respond(0, JointAttackResponse::Accepted, guest_fleet.clone()).is_err());
    invitation.revision = base_revision + 1;
    invitation.objective = Icon::Attack;
    block_on(backend.create_joint_attack(&host, &active.id, invitation.clone())).unwrap();
    let accepted =
        respond(base_revision + 2, JointAttackResponse::Accepted, guest_fleet.clone()).unwrap();
    assert_eq!(accepted.participants[2].response, JointAttackResponse::Pending);
    let submission = TurnSubmission::new(
        1,
        model.turn,
        vec![TurnCommand::SendJointMission {
            attack_id: invitation.id,
            mission_id: 99_800,
            destination: invitation.destination,
            objective: Icon::Attack,
            bombing: BombingRaid::None,
            combat_probes: false,
            contributions: accepted
                .participants
                .iter()
                .filter(|item| {
                    item.player_id == 1 || item.response == JointAttackResponse::Accepted
                })
                .filter_map(|item| item.contribution.clone())
                .collect(),
        }],
    );
    block_on(backend.save_game(&host, &active.id, active.revision, submission)).unwrap();
    assert!(respond(base_revision + 2, JointAttackResponse::Pending, guest_fleet).is_err());
    assert!(block_on(backend.respond_joint_attack(
        &guests[1],
        &active.id,
        invitation.id,
        base_revision + 2,
        JointAttackResponse::Accepted,
        Some(contribution(3))
    ))
    .is_err());
    assert!(block_on(backend.create_joint_attack(&host, &active.id, invitation)).is_err());
    assert!(block_on(backend.load_joint_attacks(&host, &active.id)).unwrap()[0].launched);
}

#[cfg(all(feature = "app", debug_assertions))]
#[test]
fn local_practice_declines_open_trades_and_missions_but_preserves_committed_orders() {
    use crate::core::missions::BombingRaid;
    use crate::core::resources::Resources;
    use crate::core::simulation::JointAttackContribution;
    use crate::core::units::buildings::Building;
    use crate::multiplayer::client::close_local_practice_negotiations;
    use crate::multiplayer::model::TradeParticipant;

    let backend = InMemoryBackend::new();
    let (host, recovery) = identity(&backend);
    let mut game = create(&backend, &host, &recovery, 4).game;
    let mut players = vec![(host.clone(), 1)];
    for id in 2..=4 {
        let (auth, recovery) = identity(&backend);
        game = block_on(backend.join_game(
            &auth,
            JoinGameRequest {
                code: game.code.clone(),
                display_name: format!("Practice {id}"),
                recovery_code: recovery.expose().to_owned(),
            },
        ))
        .unwrap()
        .game;
        players.push((auth, id));
    }
    game.persisted.state.start().unwrap();
    let game =
        block_on(backend.start_game(&host, &game.id, game.revision, game.persisted)).unwrap();
    let homes =
        game.persisted.state.players.iter().map(|player| player.home_planet).collect::<Vec<_>>();
    {
        let mut storage = backend.lock().unwrap();
        let model = &mut storage.games.get_mut(&game.id).unwrap().record.persisted.state;
        for (i, home) in homes.iter().enumerate() {
            model.map.get_mut(*home).position = bevy::math::Vec2::new(i as f32 * 100.0, 0.0);
            model.map.get_mut(*home).army.insert(Unit::Building(Building::TradingPost), 3);
            model.map.get_mut(*home).army.insert(Unit::Ship(Ship::LightFighter), 10);
            model.players[i].resources = Resources::new(10_000, 10_000, 10_000);
        }
    }
    for (id, first, second) in [(881, 1, 2), (882, 3, 4)] {
        block_on(backend.create_trade(
            &players[first - 1].0,
            &game.id,
            TradeInvitation {
                id,
                revision: 0,
                turn: game.persisted.state.turn,
                proposer: first as u64,
                canceled: false,
                finalized: false,
                participants: [first, second].map(|player| TradeParticipant {
                    player_id: player as u64,
                    planet_id: homes[player - 1],
                    resources: if player == first {
                        Resources::new(100, 0, 0)
                    } else {
                        Resources::default()
                    },
                    response: if player == first {
                        TradeResponse::Accepted
                    } else {
                        TradeResponse::Pending
                    },
                }),
            },
        ))
        .unwrap();
    }
    let accepted = block_on(backend.respond_trade(
        &players[3].0,
        &game.id,
        882,
        0,
        Resources::new(0, 100, 0),
        TradeResponse::Accepted,
    ))
    .unwrap();
    let finalized = block_on(backend.respond_trade(
        &players[2].0,
        &game.id,
        882,
        accepted.revision,
        Resources::new(100, 0, 0),
        TradeResponse::Accepted,
    ))
    .unwrap();
    assert!(finalized.finalized);

    let contribution = |player_id: u64| JointAttackContribution {
        player_id,
        origin: homes[player_id as usize - 1],
        army: Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
        bombing: BombingRaid::None,
        combat_probes: false,
    };
    for (id, owner, guest) in [(901, 1, 2), (902, 2, 3)] {
        block_on(backend.create_joint_attack(
            &players[owner as usize - 1].0,
            &game.id,
            JointAttackInvitation {
                id,
                revision: 0,
                turn: game.persisted.state.turn,
                inviter: owner,
                destination: homes[3],
                objective: Icon::Attack,
                bombing: BombingRaid::None,
                combat_probes: false,
                canceled: false,
                launched: false,
                participants: vec![
                    JointAttackParticipant {
                        player_id: owner,
                        response: JointAttackResponse::Accepted,
                        contribution: Some(contribution(owner)),
                    },
                    JointAttackParticipant {
                        player_id: guest,
                        response: JointAttackResponse::Pending,
                        contribution: None,
                    },
                ],
            },
        ))
        .unwrap();
    }
    block_on(backend.respond_joint_attack(
        &players[1].0,
        &game.id,
        901,
        0,
        JointAttackResponse::Accepted,
        Some(contribution(2)),
    ))
    .unwrap();
    let draft = TurnSubmission::new(
        1,
        game.persisted.state.turn,
        vec![TurnCommand::SendJointMission {
            attack_id: 901,
            mission_id: 901,
            destination: homes[3],
            objective: Icon::Attack,
            bombing: BombingRaid::None,
            combat_probes: false,
            contributions: vec![contribution(1), contribution(2)],
        }],
    );
    let submissions = vec![(host.clone(), draft.clone())];
    block_on(close_local_practice_negotiations(&backend, &game.id, &players, &submissions))
        .unwrap();
    let attacks = block_on(backend.load_joint_attacks(&players[1].0, &game.id)).unwrap();
    assert!(!attacks.iter().find(|attack| attack.id == 901).unwrap().canceled);
    assert!(attacks.iter().find(|attack| attack.id == 902).unwrap().canceled);
    let trades = block_on(backend.load_trades(&host, &game.id)).unwrap();
    assert!(trades[0].canceled);
    assert_eq!(trades[0].participant(2).unwrap().response, TradeResponse::Rejected);
    assert_eq!(block_on(backend.load_trades(&players[2].0, &game.id)).unwrap(), vec![finalized]);

    let current = block_on(backend.load_game(&host, &game.id)).unwrap();
    block_on(backend.save_game(&host, &game.id, current.revision, draft)).unwrap();
    block_on(close_local_practice_negotiations(&backend, &game.id, &players, &submissions))
        .unwrap();
    let launched = block_on(backend.load_joint_attacks(&host, &game.id)).unwrap();
    assert!(launched[0].launched && !launched[0].canceled);
}

#[test]
fn trade_edits_are_live_reset_both_sides_and_require_current_consent_to_finalize() {
    use crate::core::resources::Resources;
    use crate::core::units::buildings::Building;
    use crate::multiplayer::model::TradeParticipant;
    let backend = InMemoryBackend::new();
    let (host, recovery) = identity(&backend);
    let created = create(&backend, &host, &recovery, 2);
    let (guest, recovery) = identity(&backend);
    let mut game = block_on(backend.join_game(
        &guest,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Guest".into(),
            recovery_code: recovery.expose().to_owned(),
        },
    ))
    .unwrap()
    .game;
    game.persisted.state.start().unwrap();
    let game =
        block_on(backend.start_game(&host, &game.id, game.revision, game.persisted)).unwrap();
    let homes =
        game.persisted.state.players.iter().map(|player| player.home_planet).collect::<Vec<_>>();
    {
        let mut storage = backend.lock().unwrap();
        let model = &mut storage.games.get_mut(&game.id).unwrap().record.persisted.state;
        for (i, home) in homes.iter().enumerate() {
            model.map.get_mut(*home).position = bevy::math::Vec2::new(i as f32 * 100.0, 0.0);
            model.map.get_mut(*home).army.insert(Unit::Building(Building::TradingPost), 3);
            model.players[i].resources = Resources::new(10_000, 10_000, 10_000);
        }
    }
    let offer = Resources::new(100, 0, 0);
    let mut trade = block_on(backend.create_trade(
        &host,
        &game.id,
        TradeInvitation {
            id: 881,
            revision: 0,
            turn: game.persisted.state.turn,
            proposer: 1,
            canceled: false,
            finalized: false,
            participants: [
                TradeParticipant {
                    player_id: 1,
                    planet_id: homes[0],
                    resources: offer,
                    response: TradeResponse::Accepted,
                },
                TradeParticipant {
                    player_id: 2,
                    planet_id: homes[1],
                    resources: Resources::default(),
                    response: TradeResponse::Pending,
                },
            ],
        },
    ))
    .unwrap();
    for (actor, player_id, resources) in [
        (&guest, 2, Resources::new(0, 200, 0)),
        (&host, 1, Resources::new(150, 0, 0)),
        (&guest, 2, Resources::default()),
        (&guest, 2, Resources::new(0, 250, 0)),
    ] {
        // The other player has accepted the previous version before each change.
        let other = if player_id == 1 {
            &guest
        } else {
            &host
        };
        let other_id = if player_id == 1 {
            2
        } else {
            1
        };
        let other_resources = trade.participant(other_id).unwrap().resources;
        trade = block_on(backend.respond_trade(
            other,
            &game.id,
            trade.id,
            trade.revision,
            other_resources,
            TradeResponse::Accepted,
        ))
        .unwrap();
        assert!(!trade.finalized);
        let prior = trade.revision;
        trade = block_on(backend.respond_trade(
            actor,
            &game.id,
            trade.id,
            prior,
            resources,
            TradeResponse::Pending,
        ))
        .unwrap();
        assert_eq!(trade.revision, prior + 1);
        assert!(trade.participants.iter().all(|party| party.response == TradeResponse::Pending));
        for viewer in [&host, &guest] {
            assert_eq!(block_on(backend.load_trades(viewer, &game.id)).unwrap()[0], trade);
        }
        assert!(block_on(backend.respond_trade(
            other,
            &game.id,
            trade.id,
            prior,
            other_resources,
            TradeResponse::Accepted
        ))
        .is_err());
        assert_eq!(backend.lock().unwrap().games[&game.id].record.persisted.state.trades.len(), 0);
    }
    for (actor, player_id) in [(&host, 1), (&guest, 2)] {
        trade = block_on(backend.respond_trade(
            actor,
            &game.id,
            trade.id,
            trade.revision,
            trade.participant(player_id).unwrap().resources,
            TradeResponse::Accepted,
        ))
        .unwrap();
        assert_eq!(trade.finalized, player_id == 2);
    }
    let storage = backend.lock().unwrap();
    let model = &storage.games[&game.id].record.persisted.state;
    assert_eq!(model.trades.len(), 1);
    let projected = crate::core::simulation::preview_commands(model, 1, &[]).unwrap();
    assert_eq!(projected.players[0].resources.metal, 9_850);
    assert_eq!(projected.players[1].resources.crystal, 9_750);
    drop(storage);
    assert!(block_on(backend.respond_trade(
        &host,
        &game.id,
        trade.id,
        trade.revision,
        offer,
        TradeResponse::Pending
    ))
    .is_err());
}
