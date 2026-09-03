use futures_lite::future::block_on;

use super::*;
use crate::core::identity::GameCode;
use crate::core::simulation::{resolve_turn, GameModel, GameRules};
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
            recovery_hash: recovery.hash().0,
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
                    recovery_hash: recovery.hash().0,
                },
            ))
            .unwrap();
            assert_eq!(joined.membership.player_id, u64::from(slot));
            let duplicate = block_on(backend.join_game(
                &session,
                JoinGameRequest {
                    code: created.game.code.clone(),
                    display_name: "Ignored".to_string(),
                    recovery_hash: recovery.hash().0,
                },
            ))
            .unwrap();
            assert_eq!(duplicate.disposition, JoinDisposition::Reconnected);
        }
        let (extra, extra_recovery) = identity(&backend);
        assert!(matches!(
            block_on(backend.join_game(
                &extra,
                JoinGameRequest {
                    code: created.game.code,
                    display_name: "Extra".to_string(),
                    recovery_hash: extra_recovery.hash().0,
                },
            )),
            Err(BackendError::GameFull)
        ));
    }
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
            recovery_hash: joiner_recovery.hash().0,
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
            recovery_hash: joiner_recovery.hash().0,
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
    let (restored, replacement) = identity(&backend);
    let mut expected = Vec::new();
    for (name, color) in [("Nova", PlayerColor::new(4)), ("Orion", None)] {
        let mut model = GameModel::new([7; 32], GameRules::default()).unwrap();
        model.player_mut(1).unwrap().color = color;
        let created = block_on(backend.create_game(
            &host,
            CreateGameRequest {
                code: generate_game_code().unwrap(),
                display_name: name.to_string(),
                recovery_hash: recovery.hash().0,
                persisted: PersistedGame::new(model),
            },
        ))
        .unwrap();
        start_with_guest(&backend, &host, &created.game);
        expected.push((created.game.id.clone(), name, color.unwrap_or(PlayerColor::for_player(1))));
        let listed = block_on(backend.list_games(&host)).unwrap();
        let summary = listed.iter().find(|summary| summary.id == created.game.id).unwrap();
        assert_eq!(summary.display_name, name);
        assert_eq!(summary.player_color, expected.last().unwrap().2);
        block_on(backend.recover_player(
            &restored,
            RecoverPlayerRequest {
                code: created.game.code,
                recovery_hash: recovery.hash().0,
                replacement_recovery_hash: replacement.hash().0,
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
            stored.finished_at = Some(Instant::now() - Duration::from_secs(age * 60 * 60));
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
                    recovery_hash: recovery.hash().0,
                }
            )),
            Err(BackendError::GameNotFound)
        ));
        assert!(matches!(
            block_on(backend.recover_player(
                &stranger,
                RecoverPlayerRequest {
                    code: game.code,
                    recovery_hash: recovery.hash().0,
                    replacement_recovery_hash: RecoveryCode::generate().unwrap().hash().0,
                }
            )),
            Err(BackendError::GameNotFound)
        ));
    }
}

#[test]
fn saving_or_reconnecting_does_not_restart_finished_game_retention() {
    let backend = InMemoryBackend::new();
    let (session, recovery) = identity(&backend);
    let created = create(&backend, &session, &recovery, 2);
    let finished_at = {
        let mut state = backend.lock().unwrap();
        let stored = state.games.get_mut(&created.game.id).unwrap();
        let mut finished = stored.record.persisted.clone();
        finished.state.status = MatchStatus::Finished;
        commit_state(stored, finished);
        assert!(stored.finished_at.is_some());
        let at = Instant::now() - Duration::from_secs(47 * 60 * 60);
        stored.finished_at = Some(at);
        at
    };
    block_on(backend.set_connected(&session, &created.game.id, true)).unwrap();
    let loaded = block_on(backend.load_game(&session, &created.game.id)).unwrap();
    block_on(backend.save_game(&session, &loaded.id, loaded.revision, loaded.persisted)).unwrap();
    assert_eq!(backend.lock().unwrap().games[&created.game.id].finished_at, Some(finished_at));
    assert_eq!(block_on(backend.list_games(&session)).unwrap().len(), 1);
    backend.lock().unwrap().games.get_mut(&created.game.id).unwrap().finished_at =
        Some(Instant::now() - FINISHED_GAME_RETENTION);
    assert!(block_on(backend.list_games(&session)).unwrap().is_empty());
    assert!(matches!(
        block_on(backend.load_game(&session, &created.game.id)),
        Err(BackendError::GameNotFound)
    ));
}

#[test]
/// Recovery replaces the user, rotates the secret, and invalidates old access.
fn recovers_from_another_identity_and_rotates_code() {
    let backend = InMemoryBackend::new();
    let (old_session, recovery) = identity(&backend);
    let created = create(&backend, &old_session, &recovery, 2);
    let (new_session, replacement) = identity(&backend);
    let recovered = block_on(backend.recover_player(
        &new_session,
        RecoverPlayerRequest {
            code: created.game.code.clone(),
            recovery_hash: recovery.hash().0.clone(),
            replacement_recovery_hash: replacement.hash().0,
        },
    ))
    .unwrap();
    assert_eq!(recovered.membership.user_id, new_session.user_id);
    assert_eq!(recovered.membership.identity_version, 2);
    assert!(matches!(
        block_on(backend.load_game(&old_session, &created.game.id)),
        Err(BackendError::Forbidden)
    ));
    let (third_session, third_replacement) = identity(&backend);
    assert!(matches!(
        block_on(backend.recover_player(
            &third_session,
            RecoverPlayerRequest {
                code: created.game.code,
                recovery_hash: recovery.hash().0,
                replacement_recovery_hash: third_replacement.hash().0,
            },
        )),
        Err(BackendError::InvalidRecoveryCode)
    ));
}

#[test]
fn recovery_protects_live_players_and_releases_abandoned_or_departed_players() {
    let backend = InMemoryBackend::new();
    let (host, original) = identity(&backend);
    let lobby = create(&backend, &host, &original, 2).game;
    let game = start_with_guest(&backend, &host, &lobby);
    block_on(backend.set_connected(&host, &game.id, true)).unwrap();
    let before = block_on(backend.load_game(&host, &game.id)).unwrap();
    let (second, replacement) = identity(&backend);
    let request = RecoverPlayerRequest {
        code: game.code.clone(),
        recovery_hash: original.hash().0,
        replacement_recovery_hash: replacement.hash().0,
    };
    assert_eq!(
        block_on(backend.recover_player(&second, request.clone())).err(),
        Some(BackendError::RecoveryCodeInUse)
    );
    assert_eq!(
        serde_json::to_value(block_on(backend.load_game(&host, &game.id)).unwrap()).unwrap(),
        serde_json::to_value(before).unwrap()
    );
    assert_eq!(backend.lock().unwrap().games[&game.id].recovery_hashes[&1], original.hash().0);

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

    let (third, next_code) = identity(&backend);
    assert_eq!(
        block_on(backend.recover_player(&third, request)).err(),
        Some(BackendError::InvalidRecoveryCode)
    );
    let next_request = RecoverPlayerRequest {
        code: game.code.clone(),
        recovery_hash: replacement.hash().0,
        replacement_recovery_hash: next_code.hash().0,
    };
    // No separate presence call is needed to protect a just-recovered slot.
    assert_eq!(
        block_on(backend.recover_player(&third, next_request.clone())).err(),
        Some(BackendError::RecoveryCodeInUse)
    );
    assert!(block_on(backend.load_game(&second, &game.id)).is_ok());
    block_on(backend.set_connected(&second, &game.id, false)).unwrap();
    assert!(block_on(backend.recover_player(&third, next_request)).is_ok());
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
            let (auth, replacement) = identity(&backend);
            let barrier = barrier.clone();
            let request = RecoverPlayerRequest {
                code: game.code.clone(),
                recovery_hash: code.hash().0,
                replacement_recovery_hash: replacement.hash().0,
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
            .filter(|(_, result)| matches!(result, Err(BackendError::InvalidRecoveryCode)))
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
/// Recovery distinguishes unknown games, malformed hashes, existing members, and bad secrets.
fn reports_typed_recovery_failures() {
    let backend = InMemoryBackend::new();
    let (creator, recovery) = identity(&backend);
    let created = create(&backend, &creator, &recovery, 2);
    let (stranger, replacement) = identity(&backend);

    assert!(matches!(
        block_on(backend.recover_player(
            &stranger,
            RecoverPlayerRequest {
                code: GameCode::new("ABCDEF"),
                recovery_hash: recovery.hash().0.clone(),
                replacement_recovery_hash: replacement.hash().0.clone(),
            },
        )),
        Err(BackendError::GameNotFound)
    ));
    assert!(matches!(
        block_on(backend.recover_player(
            &stranger,
            RecoverPlayerRequest {
                code: created.game.code.clone(),
                recovery_hash: "not-a-sha256-hash".to_string(),
                replacement_recovery_hash: replacement.hash().0.clone(),
            },
        )),
        Err(BackendError::InvalidData(_))
    ));
    assert!(matches!(
        block_on(backend.recover_player(
            &creator,
            RecoverPlayerRequest {
                code: created.game.code.clone(),
                recovery_hash: recovery.hash().0.clone(),
                replacement_recovery_hash: replacement.hash().0.clone(),
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
                recovery_hash: unrelated.hash().0,
                replacement_recovery_hash: replacement.hash().0,
            },
        )),
        Err(BackendError::InvalidRecoveryCode)
    ));
}

#[test]
/// Any member may save, while simultaneous stale writes are rejected.
fn saves_by_multiple_players_use_optimistic_revisions() {
    let backend = InMemoryBackend::new();
    let (creator, creator_recovery) = identity(&backend);
    let created = create(&backend, &creator, &creator_recovery, 2);
    let (joiner, joiner_recovery) = identity(&backend);
    block_on(backend.join_game(
        &joiner,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Joiner".to_string(),
            recovery_hash: joiner_recovery.hash().0,
        },
    ))
    .unwrap();
    let loaded = block_on(backend.load_game(&creator, &created.game.id)).unwrap();
    let creator_saved = block_on(backend.save_game(
        &creator,
        &created.game.id,
        loaded.revision,
        loaded.persisted.clone(),
    ))
    .unwrap();
    let saved = block_on(backend.save_game(
        &joiner,
        &created.game.id,
        creator_saved.revision,
        loaded.persisted.clone(),
    ))
    .unwrap();
    assert_eq!(saved.revision, loaded.revision + 2);
    assert!(matches!(
        block_on(backend.save_game(
            &creator,
            &created.game.id,
            loaded.revision,
            loaded.persisted,
        )),
        Err(BackendError::Conflict { expected, actual })
            if expected == loaded.revision && actual == saved.revision
    ));
}

#[test]
/// A saved snapshot is discoverable and exact after restoring the member's local session.
fn resumes_exact_saved_state() {
    let backend = InMemoryBackend::new();
    let (creator, recovery) = identity(&backend);
    let created = create(&backend, &creator, &recovery, 2);
    let started = start_with_guest(&backend, &creator, &created.game);
    let changed = started.persisted.clone();
    let expected_metal = changed.state.players[0].resources.metal;
    let saved =
        block_on(backend.save_game(&creator, &created.game.id, started.revision, changed)).unwrap();

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
            recovery_hash: recovery.hash().0,
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
                    recovery_hash: recovery.hash().0,
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
                        recovery_hash: recovery.hash().0,
                        replacement_recovery_hash: replacement.hash().0,
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
                    recovery_hash: replacement.hash().0,
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
                recovery_hash: replacement.hash().0,
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
/// Invalid complete-state snapshots are rejected before they can advance a revision.
fn rejects_malformed_saved_state() {
    let backend = InMemoryBackend::new();
    let (creator, recovery) = identity(&backend);
    let created = create(&backend, &creator, &recovery, 2);
    let mut malformed = created.game.persisted.clone();
    malformed.state.players.pop();
    assert!(matches!(
        block_on(backend.save_game(&creator, &created.game.id, created.game.revision, malformed,)),
        Err(BackendError::InvalidData(_))
    ));
    assert_eq!(
        block_on(backend.load_game(&creator, &created.game.id)).unwrap().revision,
        created.game.revision
    );
}

#[test]
/// Two genuinely concurrent saves serialize to one success and one revision conflict.
fn simultaneous_saves_accept_exactly_one_writer() {
    use std::sync::{Arc, Barrier};

    let backend = InMemoryBackend::new();
    let (creator, recovery) = identity(&backend);
    let created = create(&backend, &creator, &recovery, 2);
    let barrier = Arc::new(Barrier::new(3));
    let mut workers = Vec::new();
    for _ in 0..2 {
        let backend = backend.clone();
        let creator = creator.clone();
        let game_id = created.game.id.clone();
        let persisted = created.game.persisted.clone();
        let barrier = Arc::clone(&barrier);
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            block_on(backend.save_game(&creator, &game_id, 0, persisted))
        }));
    }
    barrier.wait();
    let results = workers.into_iter().map(|worker| worker.join().unwrap()).collect::<Vec<_>>();
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
            recovery_hash: joiner_recovery.hash().0,
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
    block_on(backend.submit_turn(&joiner, &active.id, second)).unwrap();
    let submissions =
        block_on(backend.load_turn_submissions(&creator, &active.id, active.persisted.state.turn))
            .unwrap();
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
            recovery_hash: joiner_recovery.hash().0,
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

    let submissions =
        block_on(backend.load_turn_submissions(&creator, &active.id, active.persisted.state.turn))
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
            recovery_hash: joiner_recovery.hash().0,
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
