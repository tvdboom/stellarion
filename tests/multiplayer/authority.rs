use crate::core::identity::GameCode;
use crate::core::simulation::{GameModel, GameRules, PersistedGame, TurnCommand, TurnSubmission};
use crate::core::units::Unit;
use crate::multiplayer::authority::resolved_snapshot;
use crate::multiplayer::backend::{BackendError, MultiplayerBackend};
use crate::multiplayer::memory::InMemoryBackend;
use crate::multiplayer::model::{AuthSession, CreateGameRequest, GameRecord, JoinGameRequest};
use futures_lite::future::block_on;

fn started() -> (InMemoryBackend, AuthSession, AuthSession, GameRecord) {
    let backend = InMemoryBackend::new();
    let host = block_on(backend.authenticate(None)).unwrap();
    let guest = block_on(backend.authenticate(None)).unwrap();
    let mut candidate = PersistedGame::new(GameModel::new([7; 32], GameRules::default()).unwrap());
    let expected_metal = candidate.state.players[0].resources.metal;
    candidate.state.players[0].resources.metal = 123456789;
    let created = block_on(backend.create_game(
        &host,
        CreateGameRequest {
            code: GameCode::new("ABC123"),
            display_name: "host".into(),
            recovery_hash: "a".repeat(64),
            persisted: candidate,
        },
    ))
    .unwrap();
    assert_eq!(created.game.persisted.state.players[0].resources.metal, expected_metal);
    let joined = block_on(backend.join_game(
        &guest,
        JoinGameRequest {
            code: created.game.code,
            display_name: "guest".into(),
            recovery_hash: "b".repeat(64),
        },
    ))
    .unwrap();
    let mut candidate = joined.game.persisted;
    candidate.state.start().unwrap();
    candidate.state.players[0].resources.metal = 123456789;
    let active =
        block_on(backend.start_game(&host, &joined.game.id, joined.game.revision, candidate))
            .unwrap();
    assert_eq!(active.persisted.state.players[0].resources.metal, expected_metal);
    (backend, host, guest, active)
}

#[test]
fn invalid_orders_cannot_poison_an_immutable_submission_slot() {
    let (backend, host, guest, active) = started();
    let invalid = TurnSubmission::new(
        1,
        1,
        vec![TurnCommand::BuyUnits {
            planet_id: active.persisted.state.players[0].home_planet,
            unit: Unit::war_sun(),
            count: 1000,
        }],
    );
    assert!(matches!(
        block_on(backend.submit_turn(&host, &active.id, invalid)),
        Err(BackendError::InvalidData(_))
    ));
    assert!(block_on(backend.load_turn_submissions(&host, &active.id, 1)).unwrap().is_empty());
    let submissions = vec![TurnSubmission::new(1, 1, vec![]), TurnSubmission::new(2, 1, vec![])];
    block_on(backend.submit_turn(&host, &active.id, submissions[0].clone())).unwrap();
    block_on(backend.submit_turn(&guest, &active.id, submissions[1].clone())).unwrap();
    assert_eq!(
        block_on(backend.load_game(&host, &active.id)).unwrap().submitted_players,
        vec![1, 2]
    );
    let canonical = resolved_snapshot(&active, &submissions).unwrap();
    let next =
        block_on(backend.publish_resolution(&host, &active.id, active.revision, 1, canonical))
            .unwrap();
    assert_eq!(next.persisted.state.turn, 2);
    assert!(next.submitted_players.is_empty());
}

#[test]
fn member_writes_cannot_change_resources_schema_turn_or_lifecycle() {
    let (backend, host, guest, active) = started();
    for mutation in 0..4 {
        let mut forged = active.persisted.clone();
        match mutation {
            0 => forged.state.players[0].resources.metal += 1,
            1 => forged.schema_version = 999,
            2 => forged.state.turn += 1,
            _ => forged.state.status = crate::core::simulation::MatchStatus::Lobby,
        }
        assert!(block_on(backend.save_game(&guest, &active.id, active.revision, forged)).is_err());
    }
    let submissions = vec![TurnSubmission::new(1, 1, vec![]), TurnSubmission::new(2, 1, vec![])];
    block_on(backend.submit_turn(&host, &active.id, submissions[0].clone())).unwrap();
    block_on(backend.submit_turn(&guest, &active.id, submissions[1].clone())).unwrap();
    let mut forged = resolved_snapshot(&active, &submissions).unwrap();
    forged.state.players[0].resources.metal += 1;
    assert!(matches!(
        block_on(backend.publish_resolution(&guest, &active.id, active.revision, 1, forged)),
        Err(BackendError::Forbidden)
    ));
    assert_eq!(block_on(backend.load_game(&host, &active.id)).unwrap().revision, active.revision);
}

#[test]
fn continuing_preserves_orders_and_requires_every_player_to_finish_again() {
    let (backend, host, guest, active) = started();
    let home = active.persisted.state.players[0].home_planet;
    let original = TurnSubmission::new(
        1,
        1,
        vec![TurnCommand::BuyUnits {
            planet_id: home,
            unit: Unit::probe(),
            count: 1,
        }],
    );
    block_on(backend.submit_turn(&host, &active.id, original.clone())).unwrap();
    let mut draft = block_on(backend.withdraw_turn(&host, &active.id, 1, 0)).unwrap();
    assert_eq!(
        serde_json::to_value(&draft.commands).unwrap(),
        serde_json::to_value(&original.commands).unwrap()
    );
    assert_eq!(draft.generation, 1);
    assert_eq!(block_on(backend.withdraw_turn(&host, &active.id, 1, 0)).unwrap().generation, 1);
    assert!(block_on(backend.load_game(&guest, &active.id)).unwrap().submitted_players.is_empty());
    assert!(matches!(
        block_on(backend.submit_turn(&host, &active.id, original)),
        Err(BackendError::DuplicateSubmission { .. })
    ));
    let guest_orders = TurnSubmission::new(2, 1, vec![]);
    block_on(backend.submit_turn(&guest, &active.id, guest_orders.clone())).unwrap();
    let provisional = resolved_snapshot(&active, &[draft.clone(), guest_orders.clone()]).unwrap();
    assert!(matches!(
        block_on(backend.publish_resolution(&guest, &active.id, active.revision, 1, provisional)),
        Err(BackendError::TurnIncomplete)
    ));
    draft.commands.push(TurnCommand::BuyUnits {
        planet_id: home,
        unit: Unit::probe(),
        count: 1,
    });
    block_on(backend.submit_turn(&host, &active.id, draft.clone())).unwrap();
    assert_eq!(
        block_on(backend.submit_turn(&host, &active.id, draft.clone())).unwrap(),
        crate::multiplayer::model::SubmissionDisposition::Duplicate
    );
    assert!(matches!(
        block_on(backend.withdraw_turn(&host, &active.id, 1, 1)),
        Err(BackendError::TurnCommitted)
    ));
    let canonical = resolved_snapshot(&active, &[draft, guest_orders]).unwrap();
    let next =
        block_on(backend.publish_resolution(&guest, &active.id, active.revision, 1, canonical))
            .unwrap();
    assert_eq!(next.persisted.state.turn, 2);
    assert!(next.submitted_players.is_empty());
}

#[test]
fn withdrawal_before_delayed_ready_preserves_a_tombstone_and_authorization() {
    let (backend, host, guest, active) = started();
    let outsider = block_on(backend.authenticate(None)).unwrap();
    assert!(matches!(
        block_on(backend.withdraw_turn(&outsider, &active.id, 1, 0)),
        Err(BackendError::Forbidden)
    ));
    assert!(matches!(
        block_on(backend.withdraw_turn(&host, &active.id, 2, 0)),
        Err(BackendError::StaleSubmission { .. })
    ));
    let draft = block_on(backend.withdraw_turn(&host, &active.id, 1, 0)).unwrap();
    assert_eq!(draft.generation, 1);
    assert!(matches!(
        block_on(backend.submit_turn(&host, &active.id, TurnSubmission::new(1, 1, vec![]))),
        Err(BackendError::DuplicateSubmission { .. })
    ));
    block_on(backend.submit_turn(&host, &active.id, draft)).unwrap();
    assert!(matches!(
        block_on(backend.withdraw_turn(&host, &active.id, 1, 0)),
        Err(BackendError::DuplicateSubmission { .. })
    ));
    block_on(backend.withdraw_turn(&guest, &active.id, 1, 0)).unwrap();
    assert_eq!(block_on(backend.load_game(&host, &active.id)).unwrap().submitted_players, vec![1]);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn last_player_ready_and_continue_turn_have_one_atomic_winner() {
    let (backend, host, guest, active) = started();
    block_on(backend.submit_turn(&host, &active.id, TurnSubmission::new(1, 1, vec![]))).unwrap();
    let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
    let ready_backend = backend.clone();
    let ready_id = active.id.clone();
    let ready_barrier = barrier.clone();
    let ready = std::thread::spawn(move || {
        ready_barrier.wait();
        block_on(ready_backend.submit_turn(&guest, &ready_id, TurnSubmission::new(2, 1, vec![])))
            .unwrap();
    });
    barrier.wait();
    let continued = block_on(backend.withdraw_turn(&host, &active.id, 1, 0));
    ready.join().unwrap();
    let record = block_on(backend.load_game(&host, &active.id)).unwrap();
    match continued {
        Ok(_) => assert_eq!(record.submitted_players, vec![2]),
        Err(BackendError::TurnCommitted) => assert_eq!(record.submitted_players, vec![1, 2]),
        Err(error) => panic!("unexpected readiness race result: {error}"),
    }
}
