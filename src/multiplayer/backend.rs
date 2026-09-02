//! Object-safe asynchronous backend contract used by Bevy and tests.

use std::future::Future;
use std::pin::Pin;

use thiserror::Error;

use crate::core::identity::{GameId, PlayerId};
use crate::core::simulation::{PersistedGame, TurnSubmission};
use crate::multiplayer::model::{
    AuthSession, CreateGameRequest, EventBatch, GameRecord, GameSummary, JoinGameRequest,
    MembershipResult, RecoverPlayerRequest, StoredTurnSubmission, SubmissionDisposition,
};

/// A heartbeat lease shared by displayed presence, resume readiness, and recovery protection.
/// Keep this aligned with `stellarion_connection_is_live` in `supabase/schema.sql`.
pub(crate) const PLAYER_CONNECTION_TIMEOUT: std::time::Duration =
    std::time::Duration::from_secs(15);

/// Sendable backend future used by native multithreaded task pools.
#[cfg(not(target_arch = "wasm32"))]
pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BackendError>> + Send + 'a>>;

/// Browser-local backend future, which may retain JavaScript handles.
#[cfg(target_arch = "wasm32")]
pub type BackendFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BackendError>> + 'a>>;

/// Typed failures shared by Supabase and in-memory implementations.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum BackendError {
    /// Credentials are missing, expired, or no longer map to a member.
    #[error("authentication is required or has expired")]
    Unauthenticated,
    /// The caller is authenticated but not authorized for this operation.
    #[error("the authenticated user is not allowed to perform this operation")]
    Forbidden,
    /// No game matches the supplied identifier or code.
    #[error("game not found")]
    GameNotFound,
    /// The candidate share code collided with an existing game.
    #[error("game code already exists")]
    GameCodeCollision,
    /// The lobby has no unclaimed player slot.
    #[error("game is full")]
    GameFull,
    /// The operation requires a lobby or active match in a different state.
    #[error("game is not in the required state")]
    InvalidGameStatus,
    /// The supplied recovery code is invalid or has already been rotated.
    #[error("recovery code is invalid or has already been used")]
    InvalidRecoveryCode,
    /// A valid recovery code belongs to a player whose connection is still live.
    #[error("this recovery code is already in use by a connected player")]
    RecoveryCodeInUse,
    /// The authenticated user already maps to another slot in this game.
    #[error("authenticated user is already a member of this game")]
    AlreadyMember,
    /// The requested player slot no longer belongs to the game.
    #[error("player no longer belongs to this game")]
    PlayerNoLongerInGame,
    /// A write lost an optimistic-concurrency race.
    #[error("revision conflict: expected {expected}, current revision is {actual}")]
    Conflict {
        /// Revision sent with the write.
        expected: u64,
        /// Current persisted revision.
        actual: u64,
    },
    /// A different payload already occupies this player's turn row.
    #[error("a different submission already exists for player {player_id} on turn {turn}")]
    DuplicateSubmission {
        /// Stable player slot.
        player_id: PlayerId,
        /// Conflicting turn.
        turn: u64,
    },
    /// Submission is older or newer than the persisted turn.
    #[error("stale turn submission: expected turn {expected}, received {actual}")]
    StaleSubmission {
        /// Current persisted turn.
        expected: u64,
        /// Submitted turn.
        actual: u64,
    },
    /// Not every active player has submitted yet.
    #[error("turn is still waiting for one or more players")]
    TurnIncomplete,
    /// Everyone is ready, so the current turn can no longer be edited.
    #[error("everyone has finished; the next turn is starting")]
    TurnCommitted,
    /// Persisted JSON or a request violated a validated invariant.
    #[error("invalid data: {0}")]
    InvalidData(String),
    /// Network connection is temporarily unavailable.
    #[error("backend is offline: {0}")]
    Offline(String),
    /// The selected hosted backend is missing a required deployment setting.
    #[error("online multiplayer is unavailable: {0}")]
    Configuration(String),
    /// Backend returned an unexpected response.
    #[error("backend protocol error: {0}")]
    Protocol(String),
}

/// Authentication, persistence, turn coordination, recovery, and notifications.
pub trait MultiplayerBackend: Send + Sync {
    /// Restores a persisted session or creates a new anonymous identity.
    fn authenticate<'a>(
        &'a self,
        stored: Option<&'a AuthSession>,
    ) -> BackendFuture<'a, AuthSession>;

    /// Renews one known identity without silently creating a replacement user.
    fn refresh_session<'a>(&'a self, session: &'a AuthSession) -> BackendFuture<'a, AuthSession>;

    /// Creates a game and registers the caller in player slot one.
    fn create_game<'a>(
        &'a self,
        session: &'a AuthSession,
        request: CreateGameRequest,
    ) -> BackendFuture<'a, MembershipResult>;

    /// Claims the next available slot or reconnects an existing identity.
    fn join_game<'a>(
        &'a self,
        session: &'a AuthSession,
        request: JoinGameRequest,
    ) -> BackendFuture<'a, MembershipResult>;

    /// Replaces an offline identity after secret verification and rotation, claiming presence
    /// atomically so another recovery cannot displace the newly connected player.
    fn recover_player<'a>(
        &'a self,
        session: &'a AuthSession,
        request: RecoverPlayerRequest,
    ) -> BackendFuture<'a, MembershipResult>;

    /// Lists started games only; games completed at least 48 hours ago expire and are deleted.
    fn list_games<'a>(&'a self, session: &'a AuthSession) -> BackendFuture<'a, Vec<GameSummary>>;

    /// Loads the latest authoritative persisted state and membership list.
    fn load_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
    ) -> BackendFuture<'a, GameRecord>;

    /// Starts a lobby using its current members and an optimistic revision check.
    fn start_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord>;

    /// Releases an active match from its reconnection lobby once every member is online.
    fn resume_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
    ) -> BackendFuture<'a, ()>;

    /// Acknowledges canonical state or changes only the caller's lobby color, with revision checks.
    fn save_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord>;

    /// Marks a command draft ready, with idempotent retries for each readiness generation.
    fn submit_turn<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        submission: TurnSubmission,
    ) -> BackendFuture<'a, SubmissionDisposition>;

    /// Clears the caller's readiness while others are still playing, returning the saved draft.
    /// A generation prevents late requests from changing a newer readiness decision.
    fn withdraw_turn<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        turn: u64,
        generation: u64,
    ) -> BackendFuture<'a, TurnSubmission>;

    /// Loads ready submissions and withdrawn drafts for one turn in stable player order.
    fn load_turn_submissions<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        turn: u64,
    ) -> BackendFuture<'a, Vec<StoredTurnSubmission>>;

    /// Publishes one deterministic resolution only if revision and submissions still match.
    fn publish_resolution<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        resolved_turn: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord>;

    /// Receives notifications newer than a resumable sequence cursor.
    fn subscribe<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        after_sequence: u64,
    ) -> BackendFuture<'a, EventBatch>;

    /// Renews connection presence; it expires after 15 seconds without a heartbeat.
    /// Disconnecting the host of an unstarted lobby permanently deletes that lobby and its data.
    fn set_connected<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        connected: bool,
    ) -> BackendFuture<'a, ()>;
}
