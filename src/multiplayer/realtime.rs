//! Lightweight Supabase Realtime wake-ups with durable RPC replay as the source of truth.

use std::time::Duration;

use ewebsock::{Options, WsEvent, WsMessage, WsReceiver, WsSender};
use serde_json::{json, Value};

use crate::core::identity::GameId;
use crate::multiplayer::model::AuthSession;
use crate::platform::config::SupabaseConfig;

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(25);
const MAX_FRAME_BYTES: usize = 256 * 1024;

/// A transport hint produced by the Realtime socket.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RealtimeSignal {
    /// The Postgres event table changed; callers must replay durable events through RPC.
    Wakeup,
    /// The authenticated Postgres Changes channel joined successfully.
    Connected,
    /// The channel is unavailable and will retry while durable polling remains active.
    Disconnected(String),
}

#[derive(Clone, Eq, PartialEq)]
/// Active per-game Realtime endpoint, identity, and subscription scope.
struct RealtimeTarget {
    config: SupabaseConfig,
    access_token: String,
    game_id: GameId,
}

/// Open WebSocket plus heartbeat state for one Realtime target.
struct RealtimeSocket {
    sender: WsSender,
    receiver: WsReceiver,
    opened: bool,
    joined: bool,
    join_reference: Option<String>,
    next_reference: u64,
}

/// One non-blocking socket that follows the currently selected game.
///
/// The type intentionally contains no gameplay state. An incoming message can only wake the
/// durable `stellarion_events_since`/`stellarion_load_game` path, so forged, duplicated, or missed
/// WebSocket payloads cannot become canonical.
pub struct SupabaseRealtimeClient {
    target: Option<RealtimeTarget>,
    socket: Option<RealtimeSocket>,
    retry_remaining: Duration,
    retry_attempt: u8,
    heartbeat_elapsed: Duration,
}

impl Default for SupabaseRealtimeClient {
    /// Constructs the default value and its gameplay-safe initial state.
    fn default() -> Self {
        Self {
            target: None,
            socket: None,
            retry_remaining: Duration::ZERO,
            retry_attempt: 0,
            heartbeat_elapsed: Duration::ZERO,
        }
    }
}

impl SupabaseRealtimeClient {
    /// Advances connection, join, heartbeat, and retry state without blocking the Bevy frame.
    pub fn update(
        &mut self,
        elapsed: Duration,
        config: Option<&SupabaseConfig>,
        session: Option<&AuthSession>,
        game_id: Option<&GameId>,
    ) -> Vec<RealtimeSignal> {
        let desired = match (config, session, game_id) {
            (Some(config), Some(session), Some(game_id)) => Some(RealtimeTarget {
                config: config.clone(),
                access_token: session.access_token.clone(),
                game_id: game_id.clone(),
            }),
            _ => None,
        };

        if desired != self.target {
            self.socket = None;
            self.target = desired;
            self.retry_remaining = Duration::ZERO;
            self.retry_attempt = 0;
            self.heartbeat_elapsed = Duration::ZERO;
        }
        if self.target.is_none() {
            return Vec::new();
        }

        let mut signals = Vec::new();
        if self.socket.is_none() {
            self.retry_remaining = self.retry_remaining.saturating_sub(elapsed);
            if !self.retry_remaining.is_zero() {
                return signals;
            }
            if let Err(error) = self.connect() {
                self.schedule_retry();
                signals.push(RealtimeSignal::Disconnected(error));
                return signals;
            }
        }

        let mut events = Vec::new();
        if let Some(socket) = &self.socket {
            while let Some(event) = socket.receiver.try_recv() {
                events.push(event);
            }
        }

        let mut disconnect_reason = None;
        for event in events {
            match event {
                WsEvent::Opened => {
                    if let Err(error) = self.join_channel() {
                        disconnect_reason = Some(error);
                        break;
                    }
                },
                WsEvent::Message(WsMessage::Text(message)) => {
                    match classify_message(
                        &message,
                        self.socket.as_ref().and_then(|socket| socket.join_reference.as_deref()),
                    ) {
                        MessageKind::Wakeup => signals.push(RealtimeSignal::Wakeup),
                        MessageKind::Joined => {
                            if let Some(socket) = &mut self.socket {
                                socket.joined = true;
                            }
                            self.retry_attempt = 0;
                            signals.push(RealtimeSignal::Connected);
                        },
                        MessageKind::Failure(reason) => {
                            disconnect_reason = Some(reason);
                            break;
                        },
                        MessageKind::Ignore => {},
                    }
                },
                WsEvent::Error(error) => {
                    disconnect_reason = Some(error);
                    break;
                },
                WsEvent::Closed => {
                    disconnect_reason = Some("Realtime connection closed".to_string());
                    break;
                },
                WsEvent::Message(
                    WsMessage::Binary(_)
                    | WsMessage::Ping(_)
                    | WsMessage::Pong(_)
                    | WsMessage::Unknown(_),
                ) => {},
            }
        }

        if let Some(reason) = disconnect_reason {
            self.socket = None;
            self.schedule_retry();
            signals.push(RealtimeSignal::Disconnected(reason));
            return signals;
        }

        if self.socket.as_ref().is_some_and(|socket| socket.opened) {
            self.heartbeat_elapsed += elapsed;
            if self.heartbeat_elapsed >= HEARTBEAT_INTERVAL {
                self.heartbeat_elapsed = Duration::ZERO;
                self.send_heartbeat();
            }
        }
        signals
    }

    /// Opens a transport for the selected target. Joining waits for the `Opened` event.
    fn connect(&mut self) -> Result<(), String> {
        let target =
            self.target.as_ref().ok_or_else(|| "Realtime has no selected target".to_string())?;
        let url = websocket_url(&target.config)?;
        let options = Options {
            max_incoming_frame_size: MAX_FRAME_BYTES,
            ..Options::default()
        };
        let (sender, receiver) = ewebsock::connect(url, options)?;
        self.socket = Some(RealtimeSocket {
            sender,
            receiver,
            opened: false,
            joined: false,
            join_reference: None,
            next_reference: 1,
        });
        Ok(())
    }

    /// Sends the authenticated Postgres Changes subscription after the socket handshake.
    fn join_channel(&mut self) -> Result<(), String> {
        let target =
            self.target.as_ref().ok_or_else(|| "Realtime target disappeared".to_string())?;
        let socket =
            self.socket.as_mut().ok_or_else(|| "Realtime socket disappeared".to_string())?;
        socket.opened = true;
        let reference = socket.next_reference.to_string();
        socket.next_reference = socket.next_reference.wrapping_add(1);
        let message = join_message(target, &reference)?;
        socket.join_reference = Some(reference);
        socket.sender.send(WsMessage::Text(message));
        self.heartbeat_elapsed = Duration::ZERO;
        Ok(())
    }

    /// Keeps the Phoenix transport alive independently from database event traffic.
    fn send_heartbeat(&mut self) {
        let Some(socket) = &mut self.socket else {
            return;
        };
        let reference = socket.next_reference.to_string();
        socket.next_reference = socket.next_reference.wrapping_add(1);
        let message = json!({
            "topic": "phoenix",
            "event": "heartbeat",
            "payload": {},
            "ref": reference,
            "join_ref": Value::Null,
        });
        socket.sender.send(WsMessage::Text(message.to_string()));
    }

    /// Applies bounded exponential backoff after a transport or channel failure.
    fn schedule_retry(&mut self) {
        let seconds = (1_u64 << self.retry_attempt.min(4)).min(30);
        self.retry_remaining = Duration::from_secs(seconds);
        self.retry_attempt = self.retry_attempt.saturating_add(1).min(5);
        self.heartbeat_elapsed = Duration::ZERO;
    }
}

/// Builds the hosted/local Supabase WebSocket endpoint without exposing a service credential.
fn websocket_url(config: &SupabaseConfig) -> Result<String, String> {
    let base = if let Some(host) = config.url.strip_prefix("https://") {
        format!("wss://{host}")
    } else if let Some(host) = config.url.strip_prefix("http://") {
        format!("ws://{host}")
    } else {
        return Err("Supabase URL must use HTTP or HTTPS".to_string());
    };
    Ok(format!(
        "{base}/realtime/v1/websocket?apikey={}&vsn=1.0.0",
        percent_encode(&config.publishable_key)
    ))
}

/// Serializes a Phoenix v1 channel join for the event table filtered to one game.
fn join_message(target: &RealtimeTarget, reference: &str) -> Result<String, String> {
    serde_json::to_string(&json!({
        "topic": format!("realtime:stellarion-events:{}", target.game_id.0),
        "event": "phx_join",
        "payload": {
            "config": {
                "broadcast": {
                    "ack": false,
                    "self": false,
                    "replication_ready": true,
                },
                "presence": { "enabled": false },
                "postgres_changes": [{
                    "event": "INSERT",
                    "schema": "public",
                    "table": "stellarion_game_events",
                    "filter": format!("game_id=eq.{}", target.game_id.0),
                    "select": ["sequence", "game_id"],
                }],
                "private": false,
            },
            "access_token": target.access_token,
        },
        "ref": reference,
        "join_ref": reference,
    }))
    .map_err(|error| format!("failed to serialize Realtime join: {error}"))
}

/// Semantically relevant categories decoded from Realtime protocol messages.
enum MessageKind {
    Wakeup,
    Joined,
    Failure(String),
    Ignore,
}

/// Reduces untrusted wire JSON to transport hints; row contents are deliberately ignored.
fn classify_message(message: &str, join_reference: Option<&str>) -> MessageKind {
    let Ok(envelope) = serde_json::from_str::<Value>(message) else {
        return MessageKind::Ignore;
    };
    let event = envelope.get("event").and_then(Value::as_str).unwrap_or_default();
    match event {
        "postgres_changes" => MessageKind::Wakeup,
        "phx_error" | "phx_close" => {
            MessageKind::Failure(format!("Realtime channel reported {event}"))
        },
        "phx_reply" if envelope.get("ref").and_then(Value::as_str) == join_reference => {
            match envelope.pointer("/payload/status").and_then(Value::as_str) {
                Some("ok") => MessageKind::Joined,
                Some("error") => MessageKind::Failure(
                    envelope
                        .pointer("/payload/response/reason")
                        .and_then(Value::as_str)
                        .unwrap_or("Realtime channel join was rejected")
                        .to_string(),
                ),
                _ => MessageKind::Ignore,
            }
        },
        "system"
            if envelope.pointer("/payload/status").and_then(Value::as_str) == Some("error") =>
        {
            MessageKind::Failure(
                envelope
                    .pointer("/payload/message")
                    .and_then(Value::as_str)
                    .unwrap_or("Realtime Postgres subscription failed")
                    .to_string(),
            )
        },
        _ => MessageKind::Ignore,
    }
}

/// Percent-encodes a query value using the RFC 3986 unreserved set.
fn percent_encode(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~') {
            encoded.push(char::from(byte));
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

#[cfg(test)]
#[path = "../../tests/multiplayer/realtime.rs"]
mod tests;
