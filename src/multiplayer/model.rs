//! Transport-neutral authentication, membership, revision, and event data types.

use serde::{Deserialize, Serialize};

use crate::core::identity::{GameCode, GameId, PlayerId, UserId};
use crate::core::simulation::{MatchStatus, PersistedGame, TurnSubmission};

/// Restorable anonymous-auth session returned by Supabase or the mock backend.
#[derive(Clone, Serialize, Deserialize)]
pub struct AuthSession {
    /// Stable authenticated user identifier.
    pub user_id: UserId,
    /// Bearer token used for authenticated backend calls.
    pub access_token: String,
    /// Refresh token persisted in client-local storage.
    pub refresh_token: String,
    /// Unix timestamp at which the access token expires, when known.
    pub expires_at: Option<u64>,
}

impl AuthSession {
    /// Creates a session, primarily for injected and in-memory backends.
    pub fn new(
        user_id: UserId,
        access_token: impl Into<String>,
        refresh_token: impl Into<String>,
    ) -> Self {
        Self {
            user_id,
            access_token: access_token.into(),
            refresh_token: refresh_token.into(),
            expires_at: None,
        }
    }
}

/// One authenticated user's mapping to a stable player slot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GameMembership {
    /// Game containing the slot.
    pub game_id: GameId,
    /// Stable gameplay identifier used in ownership references.
    pub player_id: PlayerId,
    /// Currently associated authenticated identity.
    pub user_id: UserId,
    /// Name shown in the lobby.
    pub display_name: String,
    /// Whether this member created the game and may start its lobby.
    pub is_creator: bool,
    /// Incremented whenever recovery replaces the associated identity.
    pub identity_version: u64,
    /// Whether this player currently has the game selected on a connected client.
    #[serde(default)]
    pub connected: bool,
}

/// Complete backend record returned when loading a game.
#[derive(Clone, Serialize, Deserialize)]
pub struct GameRecord {
    /// Backend-assigned game identifier.
    pub id: GameId,
    /// Human-friendly join code.
    pub code: GameCode,
    /// Optimistic-concurrency revision.
    pub revision: u64,
    /// Join capacity in a lobby, then the finalized player count after start.
    pub max_players: u8,
    /// Persisted gameplay lifecycle status.
    pub status: MatchStatus,
    /// Versioned deterministic game snapshot.
    pub persisted: PersistedGame,
    /// Current authenticated memberships.
    pub members: Vec<GameMembership>,
}

impl GameRecord {
    /// Returns the membership for one authenticated identity.
    pub fn membership_for(&self, user_id: &UserId) -> Option<&GameMembership> {
        self.members.iter().find(|member| &member.user_id == user_id)
    }
}

/// Lightweight item displayed in the resume-game list.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GameSummary {
    /// Backend game identifier.
    pub id: GameId,
    /// Shareable game code.
    pub code: GameCode,
    /// Current optimistic revision.
    pub revision: u64,
    /// Current lifecycle status.
    pub status: MatchStatus,
    /// Current turn from the persisted snapshot.
    pub turn: u64,
    /// Calling user's stable player slot.
    pub player_id: PlayerId,
    /// Current lobby membership count.
    pub player_count: usize,
    /// Lobby join capacity or finalized active player count.
    pub max_players: u8,
}

/// Data required to create a fresh game and its creator membership.
#[derive(Clone, Serialize, Deserialize)]
pub struct CreateGameRequest {
    /// Candidate human-friendly code; the caller retries collisions.
    pub code: GameCode,
    /// Creator name shown in the lobby.
    pub display_name: String,
    /// Hash of the creator's high-entropy recovery secret.
    pub recovery_hash: String,
    /// Initial versioned deterministic lobby state.
    pub persisted: PersistedGame,
}

/// Data required to claim the next available slot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JoinGameRequest {
    /// Human-friendly code locating the lobby.
    pub code: GameCode,
    /// Name shown in the lobby.
    pub display_name: String,
    /// Hash of the joining player's recovery secret.
    pub recovery_hash: String,
}

/// Data required to replace a lost authenticated identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecoverPlayerRequest {
    /// Human-friendly code locating the game.
    pub code: GameCode,
    /// Hash derived from the recovery code entered by the player.
    pub recovery_hash: String,
    /// Newly generated hash that invalidates the used recovery code.
    pub replacement_recovery_hash: String,
}

/// Whether joining created a membership or reused the caller's existing mapping.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinDisposition {
    /// A new player slot was claimed.
    Joined,
    /// The same authenticated identity already owned a slot.
    Reconnected,
}

/// Game and identity mapping returned by create, join, or recovery operations.
#[derive(Clone, Serialize, Deserialize)]
pub struct MembershipResult {
    /// Current complete game record.
    pub game: GameRecord,
    /// Calling user's mapping inside the game.
    pub membership: GameMembership,
    /// How the mapping was obtained.
    pub disposition: JoinDisposition,
}

/// Result of an idempotent turn-submission write.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionDisposition {
    /// The submission was inserted for the first time.
    Inserted,
    /// An identical retry was already stored.
    Duplicate,
}

/// Persisted submission plus its canonical content digest.
#[derive(Clone, Serialize, Deserialize)]
pub struct StoredTurnSubmission {
    /// Gameplay command payload.
    pub submission: TurnSubmission,
    /// SHA-256 of canonical serialized submission data.
    pub digest: String,
}

/// Monotonic notification emitted by the persistence backend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BackendEvent {
    /// Per-game cursor used to discard stale or duplicated notifications.
    pub sequence: u64,
    /// Game whose persisted state changed.
    pub game_id: GameId,
    /// Semantic event category.
    pub kind: BackendEventKind,
    /// Revision current after the event, when applicable.
    pub revision: Option<u64>,
    /// Turn associated with the event, when applicable.
    pub turn: Option<u64>,
    /// Player associated with the event, when applicable.
    pub player_id: Option<PlayerId>,
}

/// Notification categories used by Realtime and the in-memory backend.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BackendEventKind {
    /// A player claimed a lobby slot.
    PlayerJoined,
    /// A recovery operation replaced an authenticated identity.
    PlayerRecovered,
    /// A client reported itself connected.
    PlayerConnected,
    /// A client reported itself offline.
    PlayerDisconnected,
    /// The host released an active match after every player reconnected.
    GameResumed,
    /// A player committed commands for the current turn.
    TurnSubmitted,
    /// Persisted state or revision changed.
    StateChanged,
    /// The lobby transitioned to active play.
    GameStarted,
    /// A deterministic resolution published the next turn.
    TurnResolved,
    /// The game reached its terminal state.
    GameFinished,
}

/// Batch returned by a resumable subscription cursor.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventBatch {
    /// Events strictly newer than the requested cursor.
    pub events: Vec<BackendEvent>,
    /// Highest observed cursor, or the input cursor when no event was available.
    pub cursor: u64,
}
