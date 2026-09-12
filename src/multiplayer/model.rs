//! Transport-neutral authentication, membership, revision, and event data types.

use serde::{Deserialize, Serialize};

use crate::core::identity::{GameCode, GameId, PlayerId, UserId};
use crate::core::map::icon::Icon;
use crate::core::missions::BombingRaid;
use crate::core::player::PlayerColor;
use crate::core::simulation::{
    JointAttackContribution, MatchStatus, PersistedGame, TurnSubmission,
};
use crate::core::trading::TradeParty;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
/// One player's current response to a bilateral trade draft.
pub enum TradeResponse {
    /// This player must review or re-confirm the latest resource amounts.
    #[default]
    Pending,
    /// This player accepts the currently displayed amounts.
    Accepted,
    /// This player rejected the trade for the current turn.
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// One player's editable side of a bilateral trade invitation.
pub struct TradeParticipant {
    /// Player contributing this side of the exchange.
    pub player_id: PlayerId,
    /// Owned world whose Trading Post supplies the player's capacity.
    pub planet_id: usize,
    /// Resources offered to the other participant.
    pub resources: crate::core::resources::Resources,
    /// Confirmation state for the latest two-sided draft.
    pub response: TradeResponse,
}

impl TradeParticipant {
    /// Converts an accepted participant into the deterministic settlement shape.
    pub fn party(&self) -> TradeParty {
        TradeParty {
            player_id: self.player_id,
            planet_id: self.planet_id,
            resources: self.resources,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Private current-turn Trading Post negotiation shared by exactly two players.
pub struct TradeInvitation {
    /// Stable negotiation and eventual settlement identifier.
    pub id: u64,
    /// Turn during which this trade may be completed.
    pub turn: u64,
    /// Player who opened the other participant's Trading Post.
    pub proposer: PlayerId,
    /// Whether either participant rejected the negotiation.
    pub canceled: bool,
    /// Whether both latest resource amounts were accepted and reserved.
    pub finalized: bool,
    /// Two distinct players stored in ascending player-slot order.
    pub participants: [TradeParticipant; 2],
}

impl TradeInvitation {
    /// Returns one participant by stable player slot.
    pub fn participant(&self, player_id: PlayerId) -> Option<&TradeParticipant> {
        self.participants.iter().find(|participant| participant.player_id == player_id)
    }
}

/// Response state visible to every participant in a joint-attack invitation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JointAttackResponse {
    /// The invited player has not made a decision yet.
    #[default]
    Pending,
    /// The invited player accepted with the stored contribution.
    Accepted,
    /// The invited player declined this operation.
    Rejected,
}

/// One invited player and their optional accepted fleet contribution.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointAttackParticipant {
    /// Invited stable player slot.
    pub player_id: PlayerId,
    /// Current decision shown in the shared invitation panel.
    pub response: JointAttackResponse,
    /// Accepted origin and army; absent while pending or rejected.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub contribution: Option<JointAttackContribution>,
}

/// Private, current-turn attack information shared with explicitly invited players.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointAttackInvitation {
    /// Stable operation identifier.
    pub id: u64,
    /// Turn on which the invitation and eventual launch were prepared.
    pub turn: u64,
    /// Player who owns the objective and conquest claim.
    pub inviter: PlayerId,
    /// Target planet.
    pub destination: usize,
    /// Shared Colonize, Attack, or Destroy objective.
    pub objective: Icon,
    /// Inviter-selected bombing policy.
    pub bombing: BombingRaid,
    /// Inviter-selected probe combat policy.
    pub combat_probes: bool,
    /// Whether the inviter canceled this draft before launching the mission.
    pub canceled: bool,
    /// Inviter contribution followed by every invited player in stable slot order.
    pub participants: Vec<JointAttackParticipant>,
}

/// Maximum number of Unicode characters allowed in a player's displayed name.
pub const MAX_DISPLAY_NAME_CHARS: usize = 16;

/// Restorable anonymous-auth session returned by Supabase or the mock backend.
#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthSession {
    /// Stable authenticated user identifier.
    pub user_id: UserId,
    /// Bearer token used for authenticated backend calls.
    pub access_token: String,
    /// Refresh token persisted in client-local storage.
    pub refresh_token: String,
    /// Unix timestamp at which the access token expires, when known.
    #[serde(deserialize_with = "crate::serialization::required_option")]
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
#[serde(deny_unknown_fields)]
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
    /// Whether this player's selected client has a current heartbeat lease.
    pub connected: bool,
}

/// Complete backend record returned when loading a game.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameRecord {
    /// Backend-assigned game identifier.
    pub id: GameId,
    /// Human-friendly join code.
    pub code: GameCode,
    /// Optimistic-concurrency revision.
    pub revision: u64,
    /// Unix timestamp of the most recent authoritative snapshot save.
    pub saved_at: u64,
    /// Join capacity in a lobby, then the finalized player count after start.
    pub max_players: u8,
    /// Persisted gameplay lifecycle status.
    pub status: MatchStatus,
    /// Deterministic game snapshot.
    pub persisted: PersistedGame,
    /// Current authenticated memberships.
    pub members: Vec<GameMembership>,
    /// Players currently ready to finish the turn.
    pub submitted_players: Vec<PlayerId>,
}

impl GameRecord {
    /// Returns the membership for one authenticated identity.
    pub fn membership_for(&self, user_id: &UserId) -> Option<&GameMembership> {
        self.members.iter().find(|member| &member.user_id == user_id)
    }
}

/// Compact acknowledgement for a saved player draft and canonical-state checkpoint.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveAcknowledgement {
    /// Unchanged canonical revision guarded by the save.
    pub revision: u64,
    /// Unix timestamp at which the snapshot lease was renewed.
    pub saved_at: u64,
}

/// Compact acknowledgement for an immediate protection-access change.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProtectionPermissionUpdate {
    /// Canonical revision after applying the permission and any immediate recalls.
    pub revision: u64,
    /// Current turn to which the returned patch belongs.
    pub turn: u64,
    /// World whose access list changed.
    pub planet_id: usize,
    /// Controller who changed the invitation.
    pub controller: PlayerId,
    /// Foreign player whose access changed.
    pub protector: PlayerId,
    /// Whether access is now enabled.
    pub allowed: bool,
}

/// Lightweight item displayed in the resume-game list.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameSummary {
    /// Backend game identifier.
    pub id: GameId,
    /// Shareable game code.
    pub code: GameCode,
    /// Current optimistic revision.
    pub revision: u64,
    /// Unix timestamp of the most recent authoritative snapshot save.
    pub saved_at: u64,
    /// Current lifecycle status.
    pub status: MatchStatus,
    /// Current turn from the persisted snapshot.
    pub turn: u64,
    /// Calling user's stable player slot.
    pub player_id: PlayerId,
    /// Calling user's saved name in this game.
    pub display_name: String,
    /// Calling user's stable recovery code for this game.
    pub recovery_code: String,
    /// Calling user's selected empire color.
    pub player_color: PlayerColor,
    /// Current lobby membership count.
    pub player_count: usize,
    /// Lobby join capacity or finalized active player count.
    pub max_players: u8,
}

/// Data required to create a fresh game and its creator membership.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CreateGameRequest {
    /// Candidate human-friendly code; the caller retries collisions.
    pub code: GameCode,
    /// Creator name shown in the lobby.
    pub display_name: String,
    /// Creator's stable recovery code for this game.
    pub recovery_code: String,
    /// Initial deterministic lobby state.
    pub persisted: PersistedGame,
}

/// Data required to claim the next available slot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JoinGameRequest {
    /// Human-friendly code locating the lobby.
    pub code: GameCode,
    /// Name shown in the lobby.
    pub display_name: String,
    /// Joining player's stable recovery code for this game.
    pub recovery_code: String,
}

/// Data required to replace a lost authenticated identity.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverPlayerRequest {
    /// Human-friendly code locating the game.
    pub code: GameCode,
    /// Stable recovery code belonging to the player slot.
    pub recovery_code: String,
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
#[serde(deny_unknown_fields)]
pub struct MembershipResult {
    /// Current complete game record.
    pub game: GameRecord,
    /// Calling user's mapping inside the game.
    pub membership: GameMembership,
    /// Calling user's stable recovery code for this game.
    pub recovery_code: String,
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
#[serde(deny_unknown_fields)]
pub struct StoredTurnSubmission {
    /// Gameplay command payload.
    pub submission: TurnSubmission,
    /// SHA-256 of canonical serialized submission data.
    pub digest: String,
    /// Whether the player is ready; withdrawn drafts cannot participate in resolution.
    pub ready: bool,
}

/// Monotonic notification emitted by the persistence backend.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendEvent {
    /// Per-game cursor used to discard stale or duplicated notifications.
    pub sequence: u64,
    /// Game whose persisted state changed.
    pub game_id: GameId,
    /// Semantic event category.
    pub kind: BackendEventKind,
    /// Revision current after the event, when applicable.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub revision: Option<u64>,
    /// Turn associated with the event, when applicable.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub turn: Option<u64>,
    /// Player associated with the event, when applicable.
    #[serde(deserialize_with = "crate::serialization::required_option")]
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
    /// A player is ready to finish the current turn.
    TurnSubmitted,
    /// A player continued the turn before everyone was ready.
    TurnWithdrawn,
    /// Protection access changed immediately outside turn submission.
    ProtectionChanged,
    /// A private joint-attack invitation or response changed.
    JointAttackChanged,
    /// A private Trading Post negotiation or finalized exchange changed.
    TradeChanged,
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
#[serde(deny_unknown_fields)]
pub struct EventBatch {
    /// Events strictly newer than the requested cursor.
    pub events: Vec<BackendEvent>,
    /// Highest observed cursor, or the input cursor when no event was available.
    pub cursor: u64,
}
