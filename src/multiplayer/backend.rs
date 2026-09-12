//! Object-safe asynchronous backend contract used by Bevy and tests.

use std::future::Future;
use std::pin::Pin;

use thiserror::Error;

use crate::core::identity::{GameId, PlayerId};
use crate::core::player::PlayerColor;
use crate::core::simulation::{PersistedGame, TurnSubmission};
use crate::multiplayer::model::{
    AuthSession, CreateGameRequest, EventBatch, GameRecord, GameSummary, JoinGameRequest,
    JointAttackInvitation, JointAttackResponse, MembershipResult, ProtectionPermissionUpdate,
    RecoverPlayerRequest, SaveAcknowledgement, StoredTurnSubmission, SubmissionDisposition,
    TradeInvitation, TradeResponse,
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
    /// The supplied recovery code is invalid.
    #[error("recovery code is invalid")]
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

    /// Replaces an offline identity after recovery-code verification, claiming presence
    /// atomically so another recovery cannot displace the newly connected player. The stable
    /// per-player code is not changed by recovery.
    fn recover_player<'a>(
        &'a self,
        session: &'a AuthSession,
        request: RecoverPlayerRequest,
    ) -> BackendFuture<'a, MembershipResult>;

    /// Lists started games only; games expire 30 days after their last save or
    /// 48 hours after completion, whichever comes first.
    fn list_games<'a>(&'a self, session: &'a AuthSession) -> BackendFuture<'a, Vec<GameSummary>>;

    /// Loads the latest authoritative persisted state and membership list.
    fn load_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
    ) -> BackendFuture<'a, GameRecord>;

    /// Changes one planet's protection invitation immediately with a compact authenticated write.
    fn set_protection_permission<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        planet_id: usize,
        protector: PlayerId,
        allowed: bool,
    ) -> BackendFuture<'a, ProtectionPermissionUpdate>;

    /// Creates one private current-turn invitation containing attack information only.
    fn create_joint_attack<'a>(
        &'a self,
        _session: &'a AuthSession,
        _game_id: &'a GameId,
        _invitation: JointAttackInvitation,
    ) -> BackendFuture<'a, JointAttackInvitation> {
        Box::pin(async { Err(BackendError::Configuration("joint attacks are unavailable".into())) })
    }

    /// Accepts with an origin/fleet contribution or rejects one received invitation.
    fn respond_joint_attack<'a>(
        &'a self,
        _session: &'a AuthSession,
        _game_id: &'a GameId,
        _attack_id: u64,
        _response: JointAttackResponse,
        _contribution: Option<crate::core::simulation::JointAttackContribution>,
    ) -> BackendFuture<'a, JointAttackInvitation> {
        Box::pin(async { Err(BackendError::Configuration("joint attacks are unavailable".into())) })
    }

    /// Cancels an unlaunched invitation as its authenticated inviter.
    fn cancel_joint_attack<'a>(
        &'a self,
        _session: &'a AuthSession,
        _game_id: &'a GameId,
        _attack_id: u64,
    ) -> BackendFuture<'a, JointAttackInvitation> {
        Box::pin(async { Err(BackendError::Configuration("joint attacks are unavailable".into())) })
    }

    /// Loads private current-turn invitations involving the authenticated player.
    fn load_joint_attacks<'a>(
        &'a self,
        _session: &'a AuthSession,
        _game_id: &'a GameId,
    ) -> BackendFuture<'a, Vec<JointAttackInvitation>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Creates one private current-turn Trading Post negotiation.
    fn create_trade<'a>(
        &'a self,
        _session: &'a AuthSession,
        _game_id: &'a GameId,
        _invitation: TradeInvitation,
    ) -> BackendFuture<'a, TradeInvitation> {
        Box::pin(async { Err(BackendError::Configuration("trading is unavailable".into())) })
    }

    /// Updates one participant's offered resources and response.
    fn respond_trade<'a>(
        &'a self,
        _session: &'a AuthSession,
        _game_id: &'a GameId,
        _trade_id: u64,
        _resources: crate::core::resources::Resources,
        _response: TradeResponse,
    ) -> BackendFuture<'a, TradeInvitation> {
        Box::pin(async { Err(BackendError::Configuration("trading is unavailable".into())) })
    }

    /// Loads current-turn negotiations involving the authenticated player.
    fn load_trades<'a>(
        &'a self,
        _session: &'a AuthSession,
        _game_id: &'a GameId,
    ) -> BackendFuture<'a, Vec<TradeInvitation>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    /// Atomically claims an unoccupied empire color while the game is in its lobby.
    /// If another member claimed the color first, the current canonical record is returned.
    fn set_player_color<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        color: PlayerColor,
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

    /// Saves the caller's unfinished commands and renews the canonical checkpoint timestamp.
    /// The shared snapshot is guarded by, but does not advance, its revision.
    fn save_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        draft: TurnSubmission,
    ) -> BackendFuture<'a, SaveAcknowledgement>;

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

    /// Renews connection presence and returns the current compact membership roster.
    /// Presence expires after 15 seconds without a heartbeat.
    /// Disconnecting the host of an unstarted lobby permanently deletes that lobby and its data.
    fn set_connected<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        connected: bool,
    ) -> BackendFuture<'a, Vec<crate::multiplayer::model::GameMembership>>;
}
