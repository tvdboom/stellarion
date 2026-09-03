/// Every gameplay write uses the SQL RPC contract installed by schema.sql.
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn gameplay_writes_use_authenticated_postgrest_rpcs() {
    use std::io::{BufRead, BufReader, Read, Write};
    use std::time::Duration;

    let functions = [
        "stellarion_create_game",
        "stellarion_start_game",
        "stellarion_save_game",
        "stellarion_submit_turn",
        "stellarion_publish_resolution",
    ];
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let server = std::thread::spawn(move || {
        for function in functions {
            let (mut stream, _) = listener.accept().unwrap();
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut reader = BufReader::new(&mut stream);
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            assert_eq!(line, format!("POST /rest/v1/rpc/{function} HTTP/1.1\r\n"));
            let mut headers = String::new();
            loop {
                line.clear();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
                headers.push_str(&line.to_ascii_lowercase());
            }
            assert!(headers.contains("authorization: bearer user-test-token\r\n"));
            assert!(headers.contains("apikey: public-test-key\r\n"));
            let mut body = [0; 2];
            reader.read_exact(&mut body).unwrap();
            assert_eq!(&body, b"{}");
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").unwrap();
        }
    });
    let backend = SupabaseBackend::new(
        SupabaseConfig::new(format!("http://{address}"), "public-test-key").unwrap(),
    )
    .unwrap();
    let session = AuthSession {
        user_id: UserId::new("test-user"),
        access_token: "user-test-token".to_string(),
        refresh_token: "refresh-test-token".to_string(),
        expires_at: None,
    };
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    for function in functions {
        let response: serde_json::Value =
            runtime.block_on(backend.rpc(&session, function, &serde_json::json!({}))).unwrap();
        assert_eq!(response, serde_json::json!({}));
    }
    server.join().unwrap();
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn deadline_covers_a_response_body_that_never_finishes() {
    use std::io::{Read, Write};
    use std::time::Duration;
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let (finished, wait) = std::sync::mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        assert!(stream.read(&mut [0; 4096]).unwrap() > 0);
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n{").unwrap();
        let _ = wait.recv_timeout(Duration::from_secs(2));
    });
    let mut backend = SupabaseBackend::new(
        SupabaseConfig::new(format!("http://{address}"), "public-test-key").unwrap(),
    )
    .unwrap();
    backend.request_timeout = Duration::from_millis(100);
    let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
    let result = runtime.block_on(backend.sign_in_anonymously());
    let _ = finished.send(());
    server.join().unwrap();
    assert!(matches!(result, Err(BackendError::Offline(_))));
}

use super::*;
use crate::core::simulation::{GameModel, GameRules};
use crate::multiplayer::model::{BackendEvent, BackendEventKind, JoinDisposition};

const SCHEMA: &str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/supabase/schema.sql"));

/// Builds a valid two-player lobby response for transport-boundary tests.
fn record() -> GameRecord {
    let persisted = PersistedGame::new(
        GameModel::new(
            [7; 32],
            GameRules {
                player_count: 2,
                ..GameRules::default()
            },
        )
        .unwrap(),
    );
    let id = GameId::new("00000000-0000-0000-0000-000000000007");
    GameRecord {
        submitted_players: Vec::new(),
        id: id.clone(),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 1_700_000_000,
        max_players: 2,
        status: MatchStatus::Lobby,
        persisted,
        members: vec![GameMembership {
            game_id: id,
            player_id: 1,
            user_id: UserId::new("00000000-0000-0000-0000-000000000001"),
            display_name: "Creator".to_string(),
            is_creator: true,
            identity_version: 1,
            connected: false,
        }],
    }
}

/// Ensures successful RPC payloads still undergo semantic core and membership validation.
#[test]
fn validates_successful_transport_payloads() {
    let valid = record();
    assert!(validate_game_record(valid.clone(), Some(&valid.id), None).is_ok());

    let mut wrong_status = valid.clone();
    wrong_status.status = MatchStatus::Finished;
    assert!(matches!(
        validate_game_record(wrong_status, None, None),
        Err(BackendError::Protocol(_))
    ));

    let mut duplicate = valid.clone();
    duplicate.members.push(duplicate.members[0].clone());
    assert!(matches!(validate_game_record(duplicate, None, None), Err(BackendError::Protocol(_))));

    let result = MembershipResult {
        membership: valid.members[0].clone(),
        game: valid,
        disposition: JoinDisposition::Joined,
    };
    assert!(validate_membership_result(
        result,
        &UserId::new("00000000-0000-0000-0000-000000000001")
    )
    .is_ok());
}

/// Resume payloads validate the saved name and palette index before rendering.
#[test]
fn validates_resume_player_identity() {
    let payload = serde_json::json!({
        "id": "game-1", "code": "ABCDEF", "revision": 1, "saved_at": 1700000000,
        "status": "active",
        "turn": 6, "player_id": 2, "player_count": 2, "max_players": 2,
        "display_name": "Nova", "player_color": 4
    });
    let summary: GameSummary = serde_json::from_value(payload.clone()).unwrap();
    assert!(validate_summaries(vec![summary]).is_ok());
    for (field, value) in [
        ("display_name", serde_json::json!("")),
        ("display_name", serde_json::json!(" Nova ")),
        ("display_name", serde_json::json!("N".repeat(33))),
        ("player_color", serde_json::json!(6)),
    ] {
        let mut invalid = payload.clone();
        invalid[field] = value;
        let summary = serde_json::from_value(invalid).unwrap();
        assert!(matches!(validate_summaries(vec![summary]), Err(BackendError::Protocol(_))));
    }
}

/// Rejects stale, out-of-order, or cross-game event replay responses.
#[test]
fn validates_durable_event_batches() {
    let game_id = GameId::new("game-1");
    let event = BackendEvent {
        sequence: 4,
        game_id: game_id.clone(),
        kind: BackendEventKind::StateChanged,
        revision: Some(2),
        turn: Some(1),
        player_id: None,
    };
    assert!(validate_event_batch(
        EventBatch {
            events: vec![event.clone()],
            cursor: 4,
        },
        &game_id,
        3,
    )
    .is_ok());
    assert!(matches!(
        validate_event_batch(
            EventBatch {
                events: vec![event],
                cursor: 5,
            },
            &game_id,
            3,
        ),
        Err(BackendError::Protocol(_))
    ));
}

/// Maps every stable SQL marker into a typed client error.
#[test]
fn maps_sql_error_markers() {
    let status = reqwest::StatusCode::BAD_REQUEST;
    let mapped = |message| map_supabase_error(status, message, None);
    assert_eq!(mapped("STLR_UNAUTHENTICATED"), BackendError::Unauthenticated);
    assert_eq!(mapped("STLR_FORBIDDEN"), BackendError::Forbidden);
    assert_eq!(mapped("STLR_GAME_NOT_FOUND"), BackendError::GameNotFound);
    assert_eq!(mapped("STLR_CODE_COLLISION"), BackendError::GameCodeCollision);
    assert_eq!(mapped("STLR_GAME_FULL"), BackendError::GameFull);
    assert_eq!(mapped("STLR_INVALID_STATUS"), BackendError::InvalidGameStatus);
    assert_eq!(mapped("STLR_INVALID_RECOVERY"), BackendError::InvalidRecoveryCode);
    assert_eq!(mapped("STLR_RECOVERY_IN_USE"), BackendError::RecoveryCodeInUse);
    assert_eq!(mapped("STLR_ALREADY_MEMBER"), BackendError::AlreadyMember);
    assert_eq!(mapped("STLR_PLAYER_REMOVED"), BackendError::PlayerNoLongerInGame);
    assert_eq!(
        mapped("STLR_CONFLICT:7:8"),
        BackendError::Conflict {
            expected: 7,
            actual: 8,
        }
    );
    assert_eq!(
        mapped("STLR_DUPLICATE_SUBMISSION:2:9"),
        BackendError::DuplicateSubmission {
            player_id: 2,
            turn: 9,
        }
    );
    assert_eq!(
        mapped("STLR_STALE_SUBMISSION:10:9"),
        BackendError::StaleSubmission {
            expected: 10,
            actual: 9,
        }
    );
    assert_eq!(mapped("STLR_TURN_INCOMPLETE"), BackendError::TurnIncomplete);
    assert_eq!(mapped("STLR_TURN_COMMITTED"), BackendError::TurnCommitted);
    assert_eq!(
        mapped("STLR_INVALID_DATA:submission"),
        BackendError::InvalidData("submission".to_string())
    );
}

#[test]
/// Preserves Supabase Auth's numeric code and maps its anonymous-provider marker.
fn maps_disabled_anonymous_provider() {
    let body = serde_json::from_str::<SupabaseErrorBody>(
        r#"{"code":422,"error_code":"anonymous_provider_disabled","msg":"Anonymous sign-ins are disabled"}"#,
    )
    .unwrap();
    assert_eq!(body.code, Some(serde_json::json!(422)));
    assert_eq!(
        map_supabase_error(
            reqwest::StatusCode::UNPROCESSABLE_ENTITY,
            body.msg.as_deref().unwrap(),
            Some(&body),
        ),
        BackendError::Configuration(
            "anonymous sign-ins are disabled for the configured Supabase project".to_string()
        )
    );
}

#[test]
/// Maps a missing Stellarion RPC to the deployment step instead of raw PostgREST text.
fn maps_missing_database_schema() {
    let body = serde_json::from_str::<SupabaseErrorBody>(
        r#"{"code":"PGRST202","message":"Could not find the function public.stellarion_list_games without parameters in the schema cache"}"#,
    )
    .unwrap();
    assert_eq!(
        map_supabase_error(
            reqwest::StatusCode::NOT_FOUND,
            body.message.as_deref().unwrap(),
            Some(&body),
        ),
        BackendError::Configuration(
            "the Stellarion database schema is missing; run supabase/schema.sql in the Supabase SQL Editor"
                .to_string()
        )
    );
}

/// Guards the fresh schema's RPC surface, RLS, grants, and Realtime publication.
#[test]
fn schema_contains_the_complete_secure_contract() {
    for table in [
        "stellarion_games",
        "stellarion_game_players",
        "stellarion_turn_submissions",
        "stellarion_game_events",
    ] {
        assert!(SCHEMA.contains(&format!("alter table public.{table} enable row level security")));
        assert!(SCHEMA
            .contains(&format!("revoke all on table public.{table} from anon, authenticated")));
    }
    for policy in [
        "stellarion_games_member_select",
        "stellarion_players_member_select",
        "stellarion_submissions_member_select",
        "stellarion_events_member_select",
    ] {
        assert!(SCHEMA.contains(&format!("create policy {policy}")));
    }
    for rpc in [
        "stellarion_create_game",
        "stellarion_join_game",
        "stellarion_recover_player",
        "stellarion_list_games",
        "stellarion_load_game",
        "stellarion_start_game",
        "stellarion_resume_game",
        "stellarion_save_game",
        "stellarion_submit_turn",
        "stellarion_withdraw_turn",
        "stellarion_load_turn_submissions",
        "stellarion_publish_resolution",
        "stellarion_events_since",
        "stellarion_set_connected",
    ] {
        assert!(SCHEMA.contains(&format!("create function public.{rpc}")));
        assert!(SCHEMA.contains(&format!("grant execute on function public.{rpc}")));
    }
    assert!(SCHEMA.contains("alter publication supabase_realtime"));
    assert!(SCHEMA.contains("add table public.stellarion_game_events"));
    assert!(SCHEMA.contains("alter table public.stellarion_game_events replica identity full"));
    assert!(SCHEMA.contains("grant select on table public.stellarion_game_events to authenticated"));
    assert_eq!(SCHEMA.matches("revision is distinct from p_expected_revision").count(), 3);
    assert!(SCHEMA.contains("p_after_sequence is null or p_after_sequence < 0"));
    assert!(SCHEMA.contains("v_planet_total > 160"));
    assert!(SCHEMA.contains("jsonb_array_length(v_missions) > 4096"));
    assert!(SCHEMA.contains("pg_column_size(p_persisted) > 67108864"));
    assert!(SCHEMA.contains("jsonb_array_length(entry -> 'reports') > 512"));
    assert!(SCHEMA.contains("jsonb_array_length(p_submission -> 'commands') > 1024"));
    assert!(SCHEMA.contains("pg_column_size(p_submission) > 1048576"));
    assert!(!SCHEMA.contains("stellarion_trusted_write"));
    assert!(!SCHEMA.contains("sb_secret_"));
    assert!(!SCHEMA.contains("drop policy"));
    assert!(SCHEMA.trim_end().ends_with("commit;"));
}
