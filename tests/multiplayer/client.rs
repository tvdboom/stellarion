use crate::core::simulation::{TurnCommand, MAX_COMMANDS_PER_SUBMISSION};
use std::collections::HashSet;

use super::*;
use crate::core::simulation::MatchStatus;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn saved_ready_orders_can_be_continued_edited_and_finished_through_client_tasks() {
    let backend = Arc::new(InMemoryBackend::new());
    let host = block_on(backend.authenticate(None)).unwrap();
    let guest = block_on(backend.authenticate(None)).unwrap();
    let created = block_on(backend.create_game(
        &host,
        CreateGameRequest {
            code: GameCode::new("ABCDEF"),
            display_name: "Host".into(),
            recovery_hash: "a".repeat(64),
            persisted: PersistedGame::new(GameModel::new([7; 32], GameRules::default()).unwrap()),
        },
    ))
    .unwrap();
    let joined = block_on(backend.join_game(
        &guest,
        JoinGameRequest {
            code: created.game.code,
            display_name: "Guest".into(),
            recovery_hash: "b".repeat(64),
        },
    ))
    .unwrap();
    let started = started_snapshot_for_members(&joined.game, [8; 32]).unwrap();
    let active =
        block_on(backend.start_game(&host, &joined.game.id, joined.game.revision, started))
            .unwrap();
    let order = TurnCommand::BuyUnits {
        planet_id: active.persisted.state.players[0].home_planet,
        unit: crate::core::units::Unit::probe(),
        count: 1,
    };
    block_on(backend.submit_turn(
        &host,
        &active.id,
        TurnSubmission::new(1, 1, vec![order.clone()]),
    ))
    .unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(State::new(AppState::Game))
        .init_resource::<NextState<AppState>>()
        .init_resource::<MultiplayerForm>()
        .init_resource::<BackendTasks>()
        .insert_resource(PendingTurnCommands {
            turn: 1,
            submission: SubmissionState::Loading,
            ..default()
        })
        .insert_resource(MultiplayerSession {
            auth: Some(host.clone()),
            active_game: Some(active.clone()),
            membership: Some(created.membership),
            restore_draft_needed: true,
            ..default()
        })
        .insert_resource(ClientRuntime {
            backend: Some(backend.clone()),
            realtime_config: None,
            storage: Arc::new(MemoryStorage::default()),
            profile: ClientProfile::default(),
            practice_return: None,
        })
        .add_message::<MultiplayerRequest>()
        .add_message::<MessageMsg>()
        .add_message::<RefreshGameplayProjection>()
        .add_message::<RefreshTurnDraft>()
        .add_systems(
            Update,
            (process_requests, poll_backend_tasks, drive_turn_draft, drive_resolution).chain(),
        );
    let settle = |app: &mut App| {
        for _ in 0..1000 {
            app.update();
            let session = app.world().resource::<MultiplayerSession>();
            if app.world().resource::<BackendTasks>().0.is_empty()
                && !session.restore_draft_needed
                && !session.resolving
                && !session.resolve_needed
                && !app.world().resource::<PendingTurnCommands>().resume_requested
            {
                return;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        panic!("client tasks failed to settle");
    };
    settle(&mut app);
    let pending = app.world().resource::<PendingTurnCommands>();
    assert_eq!(pending.submission, SubmissionState::Accepted);
    assert_eq!(pending.commands.len(), 1, "saved orders survive opening the game");
    app.world_mut().resource_mut::<PendingTurnCommands>().request_resume();
    settle(&mut app);
    let mut pending = app.world_mut().resource_mut::<PendingTurnCommands>();
    assert!(pending.is_editable());
    assert_eq!(pending.commands.len(), 1);
    assert!(pending.push(order));
    app.world_mut().write_message(MultiplayerRequest::SubmitTurn);
    settle(&mut app);
    assert_eq!(block_on(backend.load_game(&guest, &active.id)).unwrap().submitted_players, vec![1]);
    assert_eq!(app.world().resource::<PendingTurnCommands>().commands.len(), 2);
    block_on(backend.submit_turn(&guest, &active.id, TurnSubmission::new(2, 1, vec![]))).unwrap();
    app.world_mut().resource_mut::<MultiplayerSession>().resolve_needed = true;
    settle(&mut app);
    let pending = app.world().resource::<PendingTurnCommands>();
    assert!(pending.is_editable());
    assert_eq!(pending.turn, 2);
    assert!(pending.commands.is_empty());
}

#[test]
fn join_error_is_cleared_on_navigation_and_stays_cleared_when_returning() {
    let mut app = App::new();
    app.add_plugins(bevy::state::app::StatesPlugin)
        .init_state::<AppState>()
        .add_plugins(MultiplayerClientPlugin);
    app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::JoinGame);
    app.world_mut().run_schedule(StateTransition);
    app.world_mut().resource_mut::<MultiplayerSession>().menu_error =
        Some("This game has already started.".to_string());

    // It remains readable for as long as the player stays on the failed form.
    app.world_mut().run_schedule(StateTransition);
    assert!(app.world().resource::<MultiplayerSession>().menu_error.is_some());

    // Both Back and Escape request this same application-state transition.
    app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::MainMenu);
    app.world_mut().run_schedule(StateTransition);
    assert!(app.world().resource::<MultiplayerSession>().menu_error.is_none());
    app.world_mut().resource_mut::<NextState<AppState>>().set(AppState::JoinGame);
    app.world_mut().run_schedule(StateTransition);
    assert!(app.world().resource::<MultiplayerSession>().menu_error.is_none());
}

#[test]
fn connection_indicator_hides_brief_reconnects_and_resets_after_success() {
    let mut indicator = ConnectionIndicator::default();
    indicator.update(ConnectionStatus::Connected, Duration::ZERO);
    indicator.update(ConnectionStatus::Reconnecting, Duration::from_secs(2));
    assert_eq!(indicator.status, ConnectionStatus::Connected);
    indicator.update(ConnectionStatus::Connected, Duration::from_millis(16));
    indicator.update(ConnectionStatus::Reconnecting, Duration::from_secs(2));
    assert_eq!(indicator.status, ConnectionStatus::Connected);
    indicator.update(ConnectionStatus::Reconnecting, Duration::from_secs(1));
    assert_eq!(indicator.status, ConnectionStatus::Reconnecting);
    indicator.update(ConnectionStatus::Connected, Duration::from_millis(16));
    assert_eq!(indicator.status, ConnectionStatus::Connected);
    indicator.update(ConnectionStatus::Offline, Duration::from_millis(16));
    assert_eq!(indicator.status, ConnectionStatus::Offline);
}

#[test]
fn color_update_success_does_not_create_a_toast() {
    let mut runtime = ClientRuntime {
        backend: None,
        realtime_config: None,
        storage: Arc::new(MemoryStorage::default()),
        profile: ClientProfile::default(),
        practice_return: None,
    };
    let mut session = MultiplayerSession::default();
    let mut form = MultiplayerForm::default();
    let mut pending = PendingTurnCommands::default();
    let mut next = NextState::default();
    apply_output(
        BackendOutput::Record(Operation::Color, record("game-a", 1, 1, MatchStatus::Lobby)),
        &mut runtime,
        &mut session,
        &mut form,
        &mut pending,
        &mut next,
        false,
    );
    assert!(session.notice.is_none());
    assert_eq!(session.connection, ConnectionStatus::Connected);
}

#[test]
fn manual_save_reports_success_and_failure_but_autosave_stays_quiet() {
    let success = operation_notification(&BackendOutput::Record(
        Operation::Save,
        record("game-a", 2, 1, MatchStatus::Active),
    ))
    .unwrap();
    assert_eq!(success.message, "Game saved successfully.");
    assert_eq!(success.level, crate::core::messages::MessageLevel::Info);

    let error = BackendError::Offline("test outage".to_string());
    let failure =
        operation_notification(&BackendOutput::Failed(Operation::Save, error.clone())).unwrap();
    assert_eq!(failure.message, user_facing_backend_error(Operation::Save, &error));
    assert_eq!(failure.level, crate::core::messages::MessageLevel::Error);

    assert!(operation_notification(&BackendOutput::Record(
        Operation::Autosave,
        record("game-a", 2, 1, MatchStatus::Active),
    ))
    .is_none());
}

#[test]
fn rejected_turn_orders_are_visible_while_normal_waiting_stays_quiet() {
    for operation in [Operation::Submit, Operation::Resolve] {
        let notification = operation_notification(&BackendOutput::Failed(
            operation,
            BackendError::InvalidData("Required production level is unavailable.".into()),
        ))
        .unwrap();
        assert_eq!(notification.level, crate::core::messages::MessageLevel::Error);
        assert!(notification.message.contains("Could not end turn"));
        assert!(notification.message.contains("Required production level is unavailable."));
    }
    assert!(operation_notification(&BackendOutput::ResolutionWaiting).is_none());
    assert!(operation_notification(&BackendOutput::Failed(
        Operation::Resolve,
        BackendError::TurnIncomplete,
    ))
    .is_none());
}

#[test]
fn connected_enemy_becoming_disconnected_creates_one_warning_toast() {
    let mut current = record("game-a", 4, 7, MatchStatus::Active);
    current.members =
        vec![membership(&current.id, 1, "Host", true), membership(&current.id, 2, "Guest", true)];
    for (local, remote) in [(0, 1), (1, 0)] {
        let mut next = current.clone();
        next.members[remote].connected = false;
        let mut session = MultiplayerSession {
            active_game: Some(current.clone()),
            membership: Some(current.members[local].clone()),
            ..default()
        };
        let notifications = disconnected_player_notifications(
            &BackendOutput::Record(Operation::Load, next.clone()),
            &session,
        );
        assert_eq!(notifications.len(), 1);
        assert_eq!(
            notifications[0].message,
            format!("Player {} disconnected.", current.members[remote].display_name)
        );
        assert_eq!(notifications[0].level, crate::core::messages::MessageLevel::Warning);

        next.members[local].connected = false;
        let output = BackendOutput::Record(Operation::Load, next.clone());
        assert_eq!(
            disconnected_player_notifications(&output, &session).len(),
            1,
            "the local player's presence change must not create a toast"
        );
        session.active_game = Some(next);
        assert!(disconnected_player_notifications(&output, &session).is_empty());
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn host_and_guest_disconnects_refresh_before_due_event_polls() {
    for local in 0..2 {
        let backend = Arc::new(InMemoryBackend::new());
        let host = block_on(backend.authenticate(None)).unwrap();
        let guest = block_on(backend.authenticate(None)).unwrap();
        let created = block_on(backend.create_game(
            &host,
            CreateGameRequest {
                code: GameCode::new("ABCDEF"),
                display_name: "Host".into(),
                recovery_hash: "a".repeat(64),
                persisted: record("presence", 0, 1, MatchStatus::Lobby).persisted,
            },
        ))
        .unwrap();
        let joined = block_on(backend.join_game(
            &guest,
            JoinGameRequest {
                code: created.game.code,
                display_name: "Guest".into(),
                recovery_hash: "b".repeat(64),
            },
        ))
        .unwrap();
        let snapshot = started_snapshot_for_members(&joined.game, [8; 32]).unwrap();
        let active =
            block_on(backend.start_game(&host, &joined.game.id, joined.game.revision, snapshot))
                .unwrap();
        for auth in [&host, &guest] {
            block_on(backend.set_connected(auth, &active.id, true)).unwrap();
        }
        let auth = [&host, &guest][local];
        let remote = [&guest, &host][local];
        let active = block_on(backend.load_game(auth, &active.id)).unwrap();
        let peer = active.membership_for(&remote.user_id).unwrap().clone();
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::state::app::StatesPlugin))
            .init_state::<AppState>()
            .add_plugins(MultiplayerClientPlugin)
            .add_message::<MessageMsg>()
            .insert_resource(State::new(AppState::Game))
            .insert_resource(ClientRuntime {
                backend: Some(backend.clone()),
                realtime_config: None,
                storage: Arc::new(MemoryStorage::default()),
                profile: ClientProfile::default(),
                practice_return: None,
            })
            .insert_resource(MultiplayerSession {
                auth: Some(auth.clone()),
                membership: active.membership_for(&auth.user_id).cloned(),
                active_game: Some(active.clone()),
                reload_needed: true,
                ..default()
            });
        block_on(backend.set_connected(remote, &active.id, false)).unwrap();

        // Use the production Update schedule, bypassing startup's external authentication.
        // A due event poll every frame must never starve a roster refresh from a wake-up.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(16));
            app.world_mut().resource_mut::<Time<Real>>().advance_by(Duration::from_millis(16));
            app.world_mut().resource_mut::<EventPollTimer>().0.set_elapsed(Duration::from_secs(2));
            app.world_mut().run_schedule(Update);
            let session = app.world().resource::<MultiplayerSession>();
            let roster = session.active_game.as_ref().unwrap();
            if !roster.membership_for(&remote.user_id).unwrap().connected {
                assert!(roster.membership_for(&auth.user_id).unwrap().connected);
                assert_eq!(roster.status, MatchStatus::Active);
                assert_eq!(roster.revision, active.revision);
                break;
            }
            assert!(std::time::Instant::now() < deadline, "roster refresh was starved by polls");
            std::thread::sleep(Duration::from_millis(1));
        }
        let messages =
            app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].message, format!("Player {} disconnected.", peer.display_name));
    }
}

#[cfg(debug_assertions)]
#[test]
/// Builds a started one-player backend record whose first submission advances immediately.
fn local_practice_backend_advances_without_an_opponent() {
    let rules = GameRules {
        player_count: 1,
        practice_mode: true,
        ..GameRules::default()
    };
    let color = PlayerColor::new(4).unwrap();
    let (backend, auth, result) = block_on(create_local_practice(rules, color)).unwrap();
    assert_eq!(result.game.status, MatchStatus::Active);
    assert_eq!(result.game.max_players, 1);
    assert_eq!(result.game.members.len(), 1);
    assert_eq!(
        result.game.persisted.state.player(result.membership.player_id).unwrap().color(),
        color
    );

    let turn = result.game.persisted.state.turn;
    block_on(backend.submit_turn(
        &auth,
        &result.game.id,
        TurnSubmission::new(result.membership.player_id, turn, Vec::new()),
    ))
    .unwrap();
    let submissions =
        block_on(backend.load_turn_submissions(&auth, &result.game.id, turn)).unwrap();
    let mut model = result.game.persisted.state.clone();
    resolve_turn(
        &mut model,
        &submissions.into_iter().map(|stored| stored.submission).collect::<Vec<_>>(),
    )
    .unwrap();
    let advanced = block_on(backend.publish_resolution(
        &auth,
        &result.game.id,
        result.game.revision,
        turn,
        PersistedGame::new(model),
    ))
    .unwrap();
    assert_eq!(advanced.persisted.state.turn, turn + 1);
    assert_eq!(advanced.status, MatchStatus::Active);
    assert_eq!(
        advanced.persisted.state.player(result.membership.player_id).unwrap().color(),
        color
    );
}

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
pub(crate) fn local_practice_app() -> App {
    let rules = GameRules {
        player_count: 1,
        practice_mode: true,
        ..GameRules::default()
    };
    let (backend, auth, result) =
        block_on(create_local_practice(rules, PlayerColor::for_player(1))).unwrap();
    let mut app = App::new();
    app.add_plugins(MinimalPlugins)
        .insert_resource(State::new(AppState::Game))
        .init_resource::<NextState<AppState>>()
        .init_resource::<MultiplayerForm>()
        .init_resource::<MultiplayerSession>()
        .init_resource::<PendingTurnCommands>()
        .init_resource::<BackendTasks>()
        .insert_resource(ClientRuntime {
            backend: None,
            realtime_config: None,
            storage: Arc::new(MemoryStorage::default()),
            profile: ClientProfile::default(),
            practice_return: None,
        })
        .add_message::<MultiplayerRequest>()
        .add_message::<MessageMsg>()
        .add_message::<RefreshGameplayProjection>()
        .add_message::<RefreshTurnDraft>()
        .add_systems(
            Update,
            (
                process_requests,
                poll_backend_tasks,
                drive_turn_draft,
                drive_reload,
                drive_resolution,
            )
                .chain(),
        );
    spawn_backend_task(&mut app.world_mut().resource_mut::<BackendTasks>(), async move {
        BackendOutput::PracticeReady {
            backend,
            auth,
            result,
        }
    });
    app
}

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
pub(crate) fn settle_local_practice(app: &mut App) {
    for _ in 0..1000 {
        app.update();
        let session = app.world().resource::<MultiplayerSession>();
        assert!(session.menu_error.is_none(), "{:?}", session.menu_error);
        if app.world().resource::<BackendTasks>().0.is_empty()
            && !session.restore_draft_needed
            && !session.resolving
            && !session.resolve_needed
        {
            // Presentation may consume the final backend message in the following frame.
            app.update();
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("local practice client tasks failed to settle");
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
/// Native HTTP work always runs inside a live Tokio network reactor.
fn native_backend_tasks_have_a_network_reactor() {
    let runtime = native_backend_runtime().expect("native network runtime should initialize");
    let task = runtime.spawn(async {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(1))
            .build()
            .expect("test client should build");
        let _ = client.get("http://127.0.0.1:0").send().await;
        42
    });
    assert_eq!(block_on(task).expect("network task must not panic"), 42);
}

/// Builds a lightweight canonical record for installation-policy tests.
fn record(id: &str, revision: u64, turn: u64, status: MatchStatus) -> GameRecord {
    let mut model = GameModel::new([3; 32], GameRules::default()).unwrap();
    model.turn = turn;
    model.status = status;
    GameRecord {
        submitted_players: Vec::new(),
        id: GameId::new(id),
        code: GameCode::new("ABCDEF"),
        revision,
        saved_at: 1_700_000_000,
        max_players: 2,
        status,
        persisted: PersistedGame::new(model),
        members: Vec::new(),
    }
}

/// Builds one membership for presence-notification tests.
fn membership(
    game_id: &GameId,
    player_id: u64,
    display_name: &str,
    connected: bool,
) -> GameMembership {
    GameMembership {
        game_id: game_id.clone(),
        player_id,
        user_id: crate::core::identity::UserId::new(format!("user-{player_id}")),
        display_name: display_name.to_string(),
        is_creator: player_id == 1,
        identity_version: 1,
        connected,
    }
}

#[test]
/// Same-turn revisions preserve local commands while game/turn transitions rebuild the view.
fn same_turn_refresh_does_not_require_projection_reset() {
    let current = record("game-a", 4, 7, MatchStatus::Active);
    let saved = record("game-a", 5, 7, MatchStatus::Active);
    let next_turn = record("game-a", 6, 8, MatchStatus::Active);
    let other_game = record("game-b", 1, 7, MatchStatus::Active);

    assert!(!record_requires_gameplay_install(Some(&current), &saved));
    assert!(record_requires_gameplay_install(Some(&saved), &next_turn));
    assert!(record_requires_gameplay_install(Some(&current), &other_game));
    assert!(record_requires_gameplay_install(None, &current));
}

#[test]
/// Loading a not-yet-started game always returns its member to the shared lobby.
fn resumed_waiting_game_opens_lobby() {
    assert_eq!(record_destination(MatchStatus::Lobby, false, true, false), Some(AppState::Lobby));
    assert_eq!(record_destination(MatchStatus::Lobby, false, false, true), Some(AppState::Lobby));
    assert_eq!(record_destination(MatchStatus::Active, true, true, false), Some(AppState::Lobby));
}

#[test]
/// A redundant recovery request resolves to the same record result as Resume Game.
fn already_linked_recovery_opens_the_saved_game() {
    let backend: Arc<dyn MultiplayerBackend> = Arc::new(InMemoryBackend::new());
    let host = block_on(backend.authenticate(None)).unwrap();
    let player = block_on(backend.authenticate(None)).unwrap();
    let host_code = RecoveryCode::generate().unwrap();
    let player_code = RecoveryCode::generate().unwrap();
    let created = block_on(backend.create_game(
        &host,
        CreateGameRequest {
            code: GameCode::new("ABCDEF"),
            display_name: "Host".to_string(),
            recovery_hash: host_code.hash().0,
            persisted: PersistedGame::new(GameModel::new([3; 32], GameRules::default()).unwrap()),
        },
    ))
    .unwrap();
    let joined = block_on(backend.join_game(
        &player,
        JoinGameRequest {
            code: created.game.code.clone(),
            display_name: "Player".to_string(),
            recovery_hash: player_code.hash().0,
        },
    ))
    .unwrap();
    let snapshot = started_snapshot_for_members(&joined.game, [5; 32]).unwrap();
    let saved =
        block_on(backend.start_game(&host, &joined.game.id, joined.game.revision, snapshot))
            .unwrap();
    let listed = block_on(backend.list_games(&host)).unwrap();
    assert_eq!(linked_game_id(&listed, &GameCode::new(" abcdef ")), Some(saved.id.clone()));
    let replacement = RecoveryCode::generate().unwrap();

    let output = block_on(recover_or_resume_linked_game(
        backend,
        host,
        RecoverPlayerRequest {
            code: saved.code.clone(),
            recovery_hash: host_code.hash().0,
            replacement_recovery_hash: replacement.hash().0,
        },
        replacement,
    ));

    match output {
        BackendOutput::Record(operation, record) => {
            assert!(matches!(operation, Operation::ResumeLoad));
            assert_eq!(record.id, saved.id);
            assert_eq!(record.revision, saved.revision);
        },
        _ => panic!("an already-linked recovery must use the Resume Game result path"),
    }
}

#[test]
/// Recovery opens the same saved match for both roles; only its host can release the lobby.
fn recovered_host_and_player_resume_the_same_saved_game() {
    let backend = InMemoryBackend::new();
    let host = block_on(backend.authenticate(None)).unwrap();
    let player = block_on(backend.authenticate(None)).unwrap();
    let host_code = RecoveryCode::generate().unwrap();
    let player_code = RecoveryCode::generate().unwrap();
    let created = block_on(backend.create_game(
        &host,
        CreateGameRequest {
            code: GameCode::new("ABCDEF"),
            display_name: "Host".to_string(),
            recovery_hash: host_code.hash().0,
            persisted: PersistedGame::new(GameModel::new([3; 32], GameRules::default()).unwrap()),
        },
    ))
    .unwrap();
    let joined = block_on(backend.join_game(
        &player,
        JoinGameRequest {
            code: created.game.code.clone(),
            display_name: "Player".to_string(),
            recovery_hash: player_code.hash().0,
        },
    ))
    .unwrap();
    let snapshot = started_snapshot_for_members(&joined.game, [5; 32]).unwrap();
    let saved =
        block_on(backend.start_game(&host, &joined.game.id, joined.game.revision, snapshot))
            .unwrap();
    let mut recovered_sessions = Vec::new();
    for (original, code) in [(created.membership, host_code), (joined.membership, player_code)] {
        let auth = block_on(backend.authenticate(None)).unwrap();
        assert!(block_on(backend.list_games(&auth)).unwrap().is_empty());
        let replacement = RecoveryCode::generate().unwrap();
        let result = block_on(backend.recover_player(
            &auth,
            RecoverPlayerRequest {
                code: saved.code.clone(),
                recovery_hash: code.hash().0,
                replacement_recovery_hash: replacement.hash().0,
            },
        ))
        .unwrap();
        assert_eq!(result.game.id, saved.id);
        assert_eq!(result.game.revision, saved.revision);
        assert_eq!(
            serde_json::to_value(&result.game.persisted).unwrap(),
            serde_json::to_value(&saved.persisted).unwrap()
        );
        assert_eq!(result.membership.player_id, original.player_id);
        assert_eq!(result.membership.is_creator, original.is_creator);
        let mut runtime = ClientRuntime {
            backend: None,
            realtime_config: None,
            storage: Arc::new(MemoryStorage::default()),
            profile: ClientProfile::default(),
            practice_return: None,
        };
        let mut session = MultiplayerSession {
            auth: Some(auth.clone()),
            ..default()
        };
        let mut form = MultiplayerForm {
            recovery_code: code.expose().to_string(),
            ..default()
        };
        let mut pending = PendingTurnCommands::default();
        let mut next = NextState::default();
        let replacement_text = replacement.expose().to_string();
        apply_output(
            BackendOutput::Membership {
                operation: Operation::Recover,
                result,
                recovery_code: replacement,
            },
            &mut runtime,
            &mut session,
            &mut form,
            &mut pending,
            &mut next,
            false,
        );
        assert!(matches!(next, NextState::Pending(AppState::Lobby)));
        assert!(session.reconnect_lobby);
        assert_eq!(session.games[0].id, saved.id);
        assert_eq!(session.issued_recovery_code.as_deref(), Some(replacement_text.as_str()));
        assert!(form.recovery_code.is_empty());
        block_on(backend.set_connected(&auth, &saved.id, true)).unwrap();
        recovered_sessions.push((runtime, session));
    }
    let recovered_host = recovered_sessions[0].1.auth.as_ref().unwrap();
    let recovered_player = recovered_sessions[1].1.auth.as_ref().unwrap();
    assert_eq!(
        block_on(backend.resume_game(recovered_player, &saved.id)),
        Err(BackendError::Forbidden)
    );
    let cursor = block_on(backend.subscribe(recovered_player, &saved.id, 0)).unwrap().cursor;
    block_on(backend.resume_game(recovered_host, &saved.id)).unwrap();
    let events = block_on(backend.subscribe(recovered_player, &saved.id, cursor)).unwrap();
    let (runtime, session) = &mut recovered_sessions[1];
    let mut next = NextState::default();
    apply_output(
        BackendOutput::Events(events),
        runtime,
        session,
        &mut MultiplayerForm::default(),
        &mut PendingTurnCommands::default(),
        &mut next,
        false,
    );
    assert!(!session.reconnect_lobby);
    assert!(matches!(next, NextState::Pending(AppState::LoadingGame)));
}

#[test]
/// Creating a player remembers the accepted name immediately and after restarting.
fn created_player_name_is_available_for_later_joins() {
    let backend = Arc::new(InMemoryBackend::new());
    let auth = block_on(backend.authenticate(None)).unwrap();
    let recovery_code = RecoveryCode::generate().unwrap();
    let result = block_on(backend.create_game(
        &auth,
        CreateGameRequest {
            code: GameCode::new("ABCDEF"),
            display_name: "  Nova  ".to_string(),
            recovery_hash: recovery_code.hash().0,
            persisted: PersistedGame::new(GameModel::new([3; 32], GameRules::default()).unwrap()),
        },
    ))
    .unwrap();
    let mut runtime = ClientRuntime {
        backend: None,
        realtime_config: None,
        storage: Arc::new(MemoryStorage::default()),
        profile: ClientProfile::default(),
        practice_return: None,
    };
    let mut session = MultiplayerSession {
        auth: Some(auth.clone()),
        ..default()
    };
    let mut form = MultiplayerForm::default();
    apply_output(
        BackendOutput::Membership {
            operation: Operation::Create,
            result,
            recovery_code,
        },
        &mut runtime,
        &mut session,
        &mut form,
        &mut PendingTurnCommands::default(),
        &mut NextState::default(),
        false,
    );
    assert_eq!(form.saved_display_name.as_deref(), Some("Nova"));
    crate::platform::storage::save_profile(runtime.storage.as_ref(), &runtime.profile).unwrap();
    runtime.profile = load_profile(runtime.storage.as_ref()).unwrap();
    assert_eq!(runtime.profile.display_name, "Nova");

    let mut restored_form = MultiplayerForm::default();
    apply_output(
        BackendOutput::Initialized {
            backend,
            session: auth,
            games: Vec::new(),
            mock_backend: true,
            configuration_notice: None,
            realtime_config: None,
        },
        &mut runtime,
        &mut MultiplayerSession::default(),
        &mut restored_form,
        &mut PendingTurnCommands::default(),
        &mut NextState::default(),
        false,
    );
    assert_eq!(restored_form.saved_display_name.as_deref(), Some("Nova"));
    assert_eq!(restored_form.display_name, "Nova");
}

#[test]
/// Live lobbies are excluded; starting the match makes it resumable immediately.
fn selected_lobby_is_not_saved_until_started() {
    let lobby = record("game-a", 0, 1, MatchStatus::Lobby);
    let mut session = MultiplayerSession {
        membership: Some(GameMembership {
            game_id: lobby.id.clone(),
            player_id: 1,
            user_id: crate::core::identity::UserId::new("host"),
            display_name: "Host".to_string(),
            is_creator: true,
            identity_version: 1,
            connected: false,
        }),
        ..MultiplayerSession::default()
    };

    sync_game_summary(&mut session, &lobby);
    assert!(session.games.is_empty());
    let mut started = record("game-a", 1, 1, MatchStatus::Active);
    started.persisted.state.player_mut(1).unwrap().color = PlayerColor::new(4);
    sync_game_summary(&mut session, &started);
    assert_eq!(session.games.len(), 1);
    assert_eq!(session.games[0].status, MatchStatus::Active);
    assert_eq!(session.games[0].player_id, 1);
    assert_eq!(session.games[0].display_name, "Host");
    assert_eq!(session.games[0].player_color, PlayerColor::new(4).unwrap());
    started.persisted.state.player_mut(1).unwrap().color = None;
    sync_game_summary(&mut session, &started);
    assert_eq!(session.games.len(), 1);
    assert_eq!(session.games[0].player_color, PlayerColor::for_player(1));
    sync_game_summary(&mut session, &lobby);
    assert!(session.games.is_empty());
}

#[test]
fn lobby_departure_clears_guests_and_local_history() {
    for host in [true, false] {
        let lobby = record("closed-lobby", 0, 1, MatchStatus::Lobby);
        let mut runtime = ClientRuntime {
            backend: None,
            realtime_config: None,
            storage: Arc::new(MemoryStorage::default()),
            profile: ClientProfile::default(),
            practice_return: None,
        };
        let mut session = MultiplayerSession {
            membership: Some(GameMembership {
                game_id: lobby.id.clone(),
                player_id: if host {
                    1
                } else {
                    2
                },
                user_id: crate::core::identity::UserId::new("local-player"),
                display_name: "Player".to_string(),
                is_creator: host,
                identity_version: 1,
                connected: true,
            }),
            issued_recovery_code: Some("private-code".to_string()),
            ..default()
        };
        let mut form = MultiplayerForm {
            game_code: "ABCDEF".to_string(),
            recovery_code: "private-code".to_string(),
            ..default()
        };
        let mut pending = PendingTurnCommands::default();
        let mut next = NextState::default();
        install_record(lobby.clone(), &mut runtime, &mut session, &mut pending, &mut next, false);
        assert!(runtime.profile.recent_games.is_empty());
        assert!(session.games.is_empty());
        // A legacy profile may still contain the deleted lobby's identifier.
        runtime.profile.remember_game(lobby.id.clone());
        next = NextState::default();
        let output = if host {
            BackendOutput::Left(lobby.id)
        } else {
            BackendOutput::Failed(Operation::Events, BackendError::GameNotFound)
        };
        let notification = host_closed_lobby_notification(&output, &session);
        apply_output(output, &mut runtime, &mut session, &mut form, &mut pending, &mut next, false);
        assert!(!session.has_active_game());
        assert!(session.issued_recovery_code.is_none());
        assert!(!session.reload_needed && !session.presence_needed);
        assert!(runtime.profile.recent_games.is_empty());
        assert!(load_profile(runtime.storage.as_ref()).unwrap().recent_games.is_empty());
        assert!(form.game_code.is_empty() && form.recovery_code.is_empty());
        assert!(matches!(next, NextState::Pending(AppState::MainMenu)));
        if !host {
            let notification = notification.unwrap();
            assert_eq!(notification.message, "The host closed the lobby.");
            assert_eq!(notification.level, crate::core::messages::MessageLevel::Info);
            assert_eq!(notification.display_duration, Some(Duration::from_secs(2)));
            assert_eq!(session.notice.as_deref(), Some("The host closed the lobby."));
            assert!(session.menu_error.is_none());
        } else {
            assert!(notification.is_none());
        }
    }
}

#[test]
/// Quiet event streams still refresh expired peers without creating a heartbeat loop.
fn heartbeat_refreshes_resume_roster_without_renewing_again_immediately() {
    let mut game = record("game-a", 4, 7, MatchStatus::Active);
    game.members =
        vec![membership(&game.id, 1, "Host", false), membership(&game.id, 2, "Guest", true)];
    let mut runtime = ClientRuntime {
        backend: None,
        realtime_config: None,
        storage: Arc::new(MemoryStorage::default()),
        profile: ClientProfile::default(),
        practice_return: None,
    };
    let mut session = MultiplayerSession {
        membership: Some(game.members[0].clone()),
        reconnect_lobby: true,
        ..default()
    };
    let mut pending = PendingTurnCommands::default();
    let mut next = NextState::default();
    install_record(game.clone(), &mut runtime, &mut session, &mut pending, &mut next, false);
    assert!(session.presence_needed);

    // Simulate a successful heartbeat after the peer's last lease has expired.
    session.presence_needed = false;
    session.presence_elapsed = Duration::from_secs(1);
    apply_output(
        BackendOutput::Presence,
        &mut runtime,
        &mut session,
        &mut MultiplayerForm::default(),
        &mut pending,
        &mut next,
        false,
    );
    assert!(session.reload_needed);
    game.members[1].connected = false;
    let output = BackendOutput::Record(Operation::Load, game);
    assert_eq!(disconnected_player_notifications(&output, &session).len(), 1);
    apply_output(
        output,
        &mut runtime,
        &mut session,
        &mut MultiplayerForm::default(),
        &mut pending,
        &mut next,
        false,
    );
    let members = &session.active_game.as_ref().unwrap().members;
    assert!(members[0].connected);
    assert!(!members[1].connected);
    assert!(session.reconnect_lobby);
    assert!(!session.reload_needed && !session.presence_needed);
    assert_eq!(session.presence_elapsed, Duration::from_secs(1));
}

#[test]
/// The client entering a reconnection lobby is shown online before its presence RPC returns.
fn selected_player_is_optimistically_connected() {
    let mut game = record("game-a", 4, 7, MatchStatus::Active);
    let membership = GameMembership {
        game_id: game.id.clone(),
        player_id: 1,
        user_id: crate::core::identity::UserId::new("host"),
        display_name: "Host".to_string(),
        is_creator: true,
        identity_version: 1,
        connected: false,
    };
    game.members.push(membership.clone());
    let mut session = MultiplayerSession {
        membership: Some(membership),
        ..MultiplayerSession::default()
    };

    mark_selected_member_connected(&mut session, &mut game);

    assert!(session.membership.as_ref().unwrap().connected);
    assert!(game.members[0].connected);
}

#[test]
/// Join failures explain the lifecycle in player language and point to recovery paths.
fn started_game_join_error_is_actionable() {
    let message = user_facing_backend_error(Operation::Join, &BackendError::InvalidGameStatus);
    assert!(message.contains("already started"));
    assert!(message.contains("Resume Game"));
    assert!(!message.contains("required state"));
}

#[test]
fn rejected_recovery_keeps_the_player_on_the_form_with_an_actionable_error() {
    for error in [BackendError::RecoveryCodeInUse, BackendError::InvalidRecoveryCode] {
        let mut runtime = ClientRuntime {
            backend: None,
            realtime_config: None,
            storage: Arc::new(MemoryStorage::default()),
            profile: ClientProfile::default(),
            practice_return: None,
        };
        let mut session = MultiplayerSession {
            busy: true,
            ..default()
        };
        let mut form = MultiplayerForm {
            game_code: "ABCDEF".into(),
            recovery_code: "0123-4567-89AB-CDEF".into(),
            ..default()
        };
        let mut pending = PendingTurnCommands::default();
        let mut next = NextState::default();
        apply_output(
            BackendOutput::Failed(Operation::Recover, error),
            &mut runtime,
            &mut session,
            &mut form,
            &mut pending,
            &mut next,
            false,
        );
        assert!(matches!(next, NextState::Unchanged));
        assert!(!session.has_active_game());
        assert!(!session.busy);
        assert!(session.menu_error.as_ref().unwrap().contains("already"));
        assert!(session.menu_error.as_ref().unwrap().contains("own private recovery code"));
        assert_eq!(form.game_code, "ABCDEF");
        assert_eq!(form.recovery_code, "0123-4567-89AB-CDEF");
    }
}

#[test]
fn leaving_while_busy_discards_pending_loads_and_never_waits_for_authentication() {
    for (status, lost_access) in [
        (MatchStatus::Lobby, false),
        (MatchStatus::Active, false),
        (MatchStatus::Lobby, true),
        (MatchStatus::Active, true),
    ] {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .init_resource::<BackendTasks>()
            .init_resource::<MultiplayerForm>()
            .init_resource::<PendingTurnCommands>()
            .init_resource::<NextState<AppState>>()
            .add_message::<MultiplayerRequest>()
            .add_message::<MessageMsg>()
            .add_systems(Update, process_requests);
        let backend = Arc::new(InMemoryBackend::new());
        let host = block_on(backend.authenticate(None)).unwrap();
        let stranger = block_on(backend.authenticate(None)).unwrap();
        let created = block_on(backend.create_game(
            &host,
            CreateGameRequest {
                code: GameCode::new("ABCDEF"),
                display_name: "Host".into(),
                recovery_hash: RecoveryCode::generate().unwrap().hash().0,
                persisted: record("leave-game", 0, 1, MatchStatus::Lobby).persisted,
            },
        ))
        .unwrap();
        let mut game = created.game;
        game.status = status;
        app.insert_resource(ClientRuntime {
            backend: lost_access.then_some(backend),
            realtime_config: None,
            storage: Arc::new(MemoryStorage::default()),
            profile: ClientProfile::default(),
            practice_return: None,
        });
        app.insert_resource(MultiplayerSession {
            auth: lost_access.then_some(stranger),
            active_game: Some(game.clone()),
            membership: Some(created.membership),
            busy: true,
            reconnect_lobby: status == MatchStatus::Active,
            menu_error: Some("You no longer have access".into()),
            ..default()
        });
        app.world_mut().resource_mut::<BackendTasks>().0.push(
            IoTaskPool::get().spawn(async move { BackendOutput::Record(Operation::Load, game) }),
        );
        app.world_mut().write_message(MultiplayerRequest::LeaveGame);
        // A click queued on the old screen must not open it again.
        app.world_mut().write_message(MultiplayerRequest::ResumeGame(GameId::new("leave-game")));
        app.update();
        let session = app.world().resource::<MultiplayerSession>();
        assert!(session.active_game.is_none());
        assert!(!session.busy && !session.reconnect_lobby);
        assert!(session.menu_error.is_none());
        let cleanup = std::mem::take(&mut app.world_mut().resource_mut::<BackendTasks>().0);
        assert_eq!(cleanup.len(), usize::from(lost_access));
        for task in cleanup {
            assert!(matches!(block_on(task), BackendOutput::DepartureFinished));
        }
        assert!(matches!(
            app.world().resource::<NextState<AppState>>(),
            NextState::Pending(AppState::MainMenu)
        ));
        app.update();
        assert!(app.world().resource::<MultiplayerSession>().active_game.is_none());
    }
}

#[test]
/// Empty durable polls keep a submitted current turn eligible for resolution retries.
fn submitted_current_turn_awaits_resolution_until_projection_advances() {
    let mut session = MultiplayerSession {
        active_game: Some(record("game-a", 4, 7, MatchStatus::Active)),
        submitted_turn: Some(7),
        ..MultiplayerSession::default()
    };
    assert!(local_submission_awaits_resolution(&session));

    session.active_game.as_mut().unwrap().persisted.state.turn = 8;
    assert!(!local_submission_awaits_resolution(&session));
    session.active_game.as_mut().unwrap().persisted.state.turn = 7;
    session.active_game.as_mut().unwrap().status = MatchStatus::Finished;
    assert!(!local_submission_awaits_resolution(&session));
}

#[test]
/// Starting rebuilds the provisional four-slot lobby for the members actually present.
fn start_snapshot_uses_current_members() {
    let mut model = GameModel::new(
        [4; 32],
        GameRules {
            player_count: 4,
            ..GameRules::default()
        },
    )
    .unwrap();
    model.status = MatchStatus::Lobby;
    model.player_mut(1).unwrap().color = PlayerColor::new(4);
    model.player_mut(2).unwrap().color = PlayerColor::new(5);
    let id = GameId::new("dynamic-lobby");
    let members = (1..=2)
        .map(|player_id| GameMembership {
            game_id: id.clone(),
            player_id,
            user_id: crate::core::identity::UserId::new(format!("user-{player_id}")),
            display_name: format!("Player {player_id}"),
            is_creator: player_id == 1,
            identity_version: 1,
            connected: false,
        })
        .collect();
    let lobby = GameRecord {
        submitted_players: Vec::new(),
        id,
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 1_700_000_000,
        max_players: 4,
        status: MatchStatus::Lobby,
        persisted: PersistedGame::new(model),
        members,
    };

    let started = started_snapshot_for_members(&lobby, [9; 32]).unwrap();
    assert_eq!(started.state.status, MatchStatus::Active);
    assert_eq!(started.state.rules.player_count, 2);
    assert_eq!(started.state.players.len(), 2);
    assert_eq!(started.state.player(1).unwrap().color(), PlayerColor::new(4).unwrap());
    assert_eq!(started.state.player(2).unwrap().color(), PlayerColor::new(5).unwrap());
}

#[test]
/// Lobby recoloring swaps provisional slots and rejects colors owned by current members.
fn lobby_colors_remain_unique() {
    let mut lobby = record("game-a", 0, 1, MatchStatus::Lobby);
    lobby.members = (1..=2)
        .map(|player_id| GameMembership {
            game_id: lobby.id.clone(),
            player_id,
            user_id: crate::core::identity::UserId::new(format!("user-{player_id}")),
            display_name: format!("Player {player_id}"),
            is_creator: player_id == 1,
            identity_version: 1,
            connected: false,
        })
        .collect();

    let player_two_default = lobby.persisted.state.player(2).unwrap().color();
    assert!(matches!(
        recolored_lobby_snapshot(&lobby, 1, player_two_default),
        Err(BackendError::InvalidData(_))
    ));

    let free_color = PlayerColor::new(4).unwrap();
    let recolored = recolored_lobby_snapshot(&lobby, 1, free_color).unwrap();
    assert_eq!(recolored.state.player(1).unwrap().color(), free_color);
    assert_eq!(
        recolored.state.players.iter().map(|player| player.color()).collect::<HashSet<_>>().len(),
        recolored.state.players.len()
    );
}

#[test]
/// Local command drafts stop growing at the same limit enforced by simulation and storage.
fn pending_turn_commands_are_bounded() {
    let mut pending = PendingTurnCommands::default();
    for _ in 0..MAX_COMMANDS_PER_SUBMISSION {
        assert!(pending.push(TurnCommand::AbandonPlanet {
            planet_id: 0
        }));
    }
    assert!(!pending.push(TurnCommand::AbandonPlanet {
        planet_id: 0
    }));
    assert_eq!(pending.commands.len(), MAX_COMMANDS_PER_SUBMISSION);
}
