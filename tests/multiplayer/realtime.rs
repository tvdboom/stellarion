use super::*;

/// Builds a valid per-game Realtime target fixture for protocol tests.
fn target() -> RealtimeTarget {
    RealtimeTarget {
        config: SupabaseConfig::new("https://project.supabase.co", "public key").unwrap(),
        access_token: "user-jwt".to_string(),
        game_id: GameId::new("00000000-0000-0000-0000-000000000001"),
    }
}

#[test]
/// Hosted endpoints become WSS and public keys are safe query values.
fn builds_realtime_endpoint() {
    assert_eq!(
        websocket_url(&target().config).unwrap(),
        "wss://project.supabase.co/realtime/v1/websocket?apikey=public%20key&vsn=1.0.0"
    );
}

#[test]
/// Join JSON contains the authenticated, game-filtered event subscription.
fn joins_only_the_selected_game_events() {
    let message: Value = serde_json::from_str(&join_message(&target(), "7").unwrap()).unwrap();
    assert_eq!(message["event"], "phx_join");
    assert_eq!(message["payload"]["access_token"], "user-jwt");
    assert_eq!(
        message["payload"]["config"]["postgres_changes"][0]["filter"],
        "game_id=eq.00000000-0000-0000-0000-000000000001"
    );
}

#[test]
/// Only change notifications become wake-ups; their row payload is never interpreted.
fn treats_realtime_as_a_wakeup_only() {
    let change =
        r#"{"event":"postgres_changes","payload":{"data":{"record":{"persisted":"untrusted"}}}}"#;
    assert!(matches!(classify_message(change, Some("1")), MessageKind::Wakeup));
    assert!(matches!(classify_message("not-json", Some("1")), MessageKind::Ignore));
}

#[test]
/// A rejected join is surfaced so the caller can retain polling and back off reconnects.
fn detects_rejected_join() {
    let reply = r#"{"event":"phx_reply","ref":"1","payload":{"status":"error","response":{"reason":"denied"}}}"#;
    assert!(matches!(
        classify_message(reply, Some("1")),
        MessageKind::Failure(reason) if reason == "denied"
    ));
}
