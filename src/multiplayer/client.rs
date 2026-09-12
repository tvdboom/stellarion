//! Bevy adapter that turns menu/gameplay intent into asynchronous backend operations.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use futures_lite::future::{block_on, poll_once};
use rand::RngExt;

use crate::core::identity::{GameCode, GameId};
use crate::core::messages::{MessageAction, MessageMsg};
use crate::core::player::PlayerColor;
use crate::core::simulation::{
    resolved_turn, set_protection_permission_immediately, GameModel, GameRules, MatchStatus,
    PersistedGame, TurnSubmission,
};
use crate::core::states::AppState;
use crate::multiplayer::authority::started_snapshot_for_members;
use crate::multiplayer::backend::{BackendError, MultiplayerBackend};
use crate::multiplayer::memory::InMemoryBackend;
use crate::multiplayer::model::{
    AuthSession, BackendEventKind, CreateGameRequest, EventBatch, GameMembership, GameRecord,
    GameSummary, JoinDisposition, JoinGameRequest, MembershipResult, ProtectionPermissionUpdate,
    RecoverPlayerRequest, SaveAcknowledgement, TradeInvitation, TradeResponse,
};
use crate::multiplayer::realtime::{RealtimeSignal, SupabaseRealtimeClient};
use crate::multiplayer::recovery::{generate_game_code, RecoveryCode};
use crate::multiplayer::supabase::SupabaseBackend;
use crate::platform::config::{ConfigError, SupabaseConfig};
use crate::platform::storage::{load_profile, ClientProfile, ClientStorage, MemoryStorage};

#[cfg(target_arch = "wasm32")]
use crate::platform::storage::BrowserStorage;
#[cfg(not(target_arch = "wasm32"))]
use crate::platform::storage::NativeStorage;

/// Current user-facing transport condition.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum ConnectionStatus {
    /// Authentication and configuration are still loading.
    #[default]
    Initializing,
    /// Backend requests and durable event replay are available.
    Connected,
    /// A transient failure occurred and the client is retrying from persisted state.
    Reconnecting,
    /// No backend request can currently be completed.
    Offline,
    /// A compare-and-swap write lost a race and current state is being reloaded.
    SyncConflict,
}

impl ConnectionStatus {
    /// Returns concise text suitable for a non-obstructive status badge.
    pub fn label(self) -> &'static str {
        match self {
            Self::Initializing => "Connecting…",
            Self::Connected => "Connected",
            Self::Reconnecting => "Reconnecting…",
            Self::Offline => "Offline",
            Self::SyncConflict => "Sync conflict",
        }
    }
}

const RECONNECT_STATUS_GRACE: Duration = Duration::from_secs(3);
const PRESENCE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(5);
const EVENT_POLL_INTERVAL_CONNECTED: Duration = Duration::from_secs(30);
const EVENT_POLL_INTERVAL_FALLBACK: Duration = Duration::from_secs(2);
const HOST_CLOSED_LOBBY_NOTICE: &str = "The host closed the lobby.";
const HOST_CLOSED_LOBBY_NOTICE_DURATION: Duration = Duration::from_secs(2);
const GAME_UNAVAILABLE_NOTICE: &str = "This game is no longer available.";
const GAME_SAVED_NOTICE: &str = "Shared game and your current turn draft saved.";
const WAITING_FOR_PLAYERS_NOTICE: &str = "Waiting for other players to finish their turn.";

/// Stable connection feedback that does not flash during a brief recovery attempt.
#[derive(Resource, Default)]
pub struct ConnectionIndicator {
    /// Debounced status displayed in the menu footer.
    pub status: ConnectionStatus,
    reconnecting_for: Duration,
}

impl ConnectionIndicator {
    /// Keeps an established connection visually steady until recovery exceeds the grace period.
    fn update(&mut self, observed: ConnectionStatus, elapsed: Duration) {
        if observed == ConnectionStatus::Reconnecting && self.status == ConnectionStatus::Connected
        {
            self.reconnecting_for = self.reconnecting_for.saturating_add(elapsed);
            if self.reconnecting_for < RECONNECT_STATUS_GRACE {
                return;
            }
        } else {
            self.reconnecting_for = Duration::ZERO;
        }
        self.status = observed;
    }
}

/// Debounces only connection presentation; networking and retry state remain immediate.
fn update_connection_indicator(
    time: Res<Time<Real>>,
    session: Res<MultiplayerSession>,
    mut indicator: ResMut<ConnectionIndicator>,
) {
    indicator.update(session.connection, time.delta());
}

/// Values shared by the game setup and multiplayer menu screens.
#[derive(Resource)]
pub struct MultiplayerForm {
    /// Lobby display name, persisted locally for convenience.
    pub display_name: String,
    /// Last accepted name, separate from the editable setup form.
    pub saved_display_name: Option<String>,
    /// Six-character game code entered by a joining player.
    pub game_code: String,
    /// High-entropy recovery code entered on a replacement device.
    pub recovery_code: String,
    /// Number of locally controlled empires in the next practice match.
    #[cfg(debug_assertions)]
    pub practice_player_count: u8,
}

impl Default for MultiplayerForm {
    /// Suggests a random name until the locally cached profile is restored.
    fn default() -> Self {
        const NAMES: [&str; 6] = [
            "Commander Nova",
            "Captain Orion",
            "Admiral Vega",
            "Commander Astra",
            "Captain Lyra",
            "Admiral Polaris",
        ];
        Self {
            display_name: NAMES[rand::rng().random_range(0..NAMES.len())].to_string(),
            saved_display_name: None,
            game_code: String::new(),
            recovery_code: String::new(),
            #[cfg(debug_assertions)]
            practice_player_count: 2,
        }
    }
}

/// Canonical multiplayer session data displayed by menus and consumed by gameplay.
#[derive(Resource, Default)]
pub struct MultiplayerSession {
    /// Current anonymous authentication session.
    pub auth: Option<AuthSession>,
    /// Resumable games belonging to the current identity.
    pub games: Vec<GameSummary>,
    /// Latest persisted record for the selected game.
    pub active_game: Option<GameRecord>,
    /// Private current-turn coordinated attacks involving the selected player.
    pub joint_attacks: Vec<crate::multiplayer::model::JointAttackInvitation>,
    /// Private current-turn Trading Post negotiations involving the selected player.
    pub trades: Vec<TradeInvitation>,
    /// Stable slot for the current identity in the selected game.
    pub membership: Option<GameMembership>,
    /// Selected player's stable recovery code, returned by membership and resume operations.
    pub issued_recovery_code: Option<String>,
    /// Last durable notification sequence applied for the selected game.
    pub event_cursor: u64,
    /// Transport condition rendered in the menu and HUD.
    pub connection: ConnectionStatus,
    /// Human-readable result or failure from the latest operation.
    pub notice: Option<String>,
    /// Actionable failure displayed persistently in the menu UI.
    pub menu_error: Option<String>,
    /// Whether local development is using the credential-free backend.
    pub mock_backend: bool,
    /// Whether the selected record is an isolated debug-only locally controlled match.
    pub local_practice: bool,
    /// Whether a foreground menu operation is still running.
    pub busy: bool,
    /// Whether one compact protection patch is still in flight.
    pub(crate) protection_update_pending: bool,
    /// Whether an invitation create/response/load operation is in flight.
    pub(crate) joint_attack_update_pending: bool,
    /// Whether a trade create/response/load operation is in flight.
    pub(crate) trade_update_pending: bool,
    /// Whether an active match is paused in its all-players reconnection lobby.
    pub reconnect_lobby: bool,
    reload_needed: bool,
    resolve_needed: bool,
    resolving: bool,
    restore_draft_needed: bool,
    joint_attack_reload_needed: bool,
    trade_reload_needed: bool,
    event_poll_needed: bool,
    submitted_turn: Option<u64>,
    presence_needed: bool,
    presence_elapsed: Duration,
    reauthentication_needed: bool,
    auth_refresh_needed: bool,
}

impl MultiplayerSession {
    /// Returns whether a selected game is an active Supabase/mock multiplayer match.
    pub fn has_active_game(&self) -> bool {
        self.active_game.is_some() && self.membership.is_some()
    }

    /// Returns the canonical color for a player in the selected game.
    pub fn player_color(&self, player_id: u64) -> PlayerColor {
        self.active_game
            .as_ref()
            .and_then(|record| record.persisted.state.player(player_id).ok())
            .map(|player| player.color())
            .unwrap_or_else(|| PlayerColor::for_player(player_id))
    }

    /// Returns the lobby name associated with a stable player slot.
    pub fn player_name(&self, player_id: u64) -> Option<&str> {
        self.active_game
            .as_ref()?
            .members
            .iter()
            .find(|member| member.player_id == player_id)
            .map(|member| member.display_name.as_str())
    }

    /// Clears selection data while retaining authentication and resumable games.
    pub fn leave_selected_game(&mut self) {
        self.active_game = None;
        self.membership = None;
        self.issued_recovery_code = None;
        self.event_cursor = 0;
        self.reload_needed = false;
        self.resolve_needed = false;
        self.resolving = false;
        self.restore_draft_needed = false;
        self.joint_attack_reload_needed = false;
        self.trade_reload_needed = false;
        self.event_poll_needed = false;
        self.submitted_turn = None;
        self.protection_update_pending = false;
        self.joint_attacks.clear();
        self.joint_attack_update_pending = false;
        self.trades.clear();
        self.trade_update_pending = false;
        self.presence_needed = false;
        self.presence_elapsed = Duration::ZERO;
        self.reconnect_lobby = false;
        self.reauthentication_needed = false;
        self.local_practice = false;
    }
}

mod profile;
mod submission;
pub(crate) use submission::COMMAND_LIMIT_REACHED_MESSAGE;
pub use submission::{PendingTurnCommands, SubmissionState};

/// Foreground operations requested by menu buttons or gameplay UI.
#[derive(Message)]
pub enum MultiplayerRequest {
    /// Creates and starts an isolated locally controlled match without contacting Supabase.
    #[cfg(debug_assertions)]
    StartLocalPractice {
        /// Deterministic rules selected in the local-practice setup screen.
        rules: GameRules,
    },
    /// Changes which local-practice empire is projected and accepts commands.
    #[cfg(debug_assertions)]
    SwitchLocalPracticePlayer(crate::core::identity::PlayerId),
    /// Submits every local-practice draft and resolves the simultaneous turn.
    #[cfg(debug_assertions)]
    AdvanceLocalPracticeTurn,
    /// Creates a lobby from the current settings.
    CreateGame {
        /// Name shown to other lobby members.
        display_name: String,
        /// Deterministic rules chosen by the creator.
        rules: GameRules,
    },
    /// Joins a lobby by share code.
    JoinGame {
        /// Name shown to other lobby members.
        display_name: String,
        /// User-entered human-friendly code.
        code: String,
    },
    /// Opens a linked game or replaces a lost identity using its stable recovery code.
    RecoverPlayer {
        /// Human-friendly game code.
        code: String,
        /// High-entropy code shown when the slot was created or last recovered.
        recovery_code: String,
    },
    /// Loads a selected resumable game by backend identifier.
    ResumeGame(GameId),
    /// Releases the selected active match after every existing player reconnects.
    ResumeActiveGame,
    /// Refreshes the authenticated user's resumable-game list.
    RefreshGames,
    /// Starts a lobby with its current members as its creator.
    StartGame,
    /// Changes the current member's empire color while the game is still in its lobby.
    SetPlayerColor(PlayerColor),
    /// Immediately grants or revokes another player's access to protect one controlled world.
    SetProtectionPermission {
        /// World controlled by the current player.
        planet_id: usize,
        /// Foreign player receiving or losing access.
        protector: crate::core::identity::PlayerId,
        /// Whether access is enabled.
        allowed: bool,
    },
    /// Creates a private invitation from a completed hostile mission draft.
    CreateJointAttack(crate::multiplayer::model::JointAttackInvitation),
    /// Accepts with the selected fleet or rejects an invitation.
    RespondJointAttack {
        /// Stable invitation identifier.
        attack_id: u64,
        /// New response state.
        response: crate::multiplayer::model::JointAttackResponse,
        /// Origin and army supplied only for acceptance.
        contribution: Option<crate::core::simulation::JointAttackContribution>,
    },
    /// Cancels an invitation before its inviter launches the allied mission.
    CancelJointAttack {
        /// Stable invitation identifier.
        attack_id: u64,
    },
    /// Reloads private invitations after a durable wake-up.
    RefreshJointAttacks,
    /// Creates a private bilateral Trading Post negotiation.
    CreateTrade(TradeInvitation),
    /// Updates this player's resources and response in one trade.
    RespondTrade {
        /// Stable trade identifier.
        trade_id: u64,
        /// Resources offered by the current player.
        resources: crate::core::resources::Resources,
        /// Acceptance or rejection of the latest draft.
        response: TradeResponse,
    },
    /// Reloads private trade negotiations after a durable wake-up.
    RefreshTrades,
    /// Saves the latest canonical snapshot from any member and reports the result to the player.
    SaveGame,
    /// Marks the local player ready to finish this turn.
    SubmitTurn,
    /// Returns to the main menu, deleting an unstarted lobby when its host leaves.
    LeaveGame,
    /// Retries state/event synchronization after an offline error.
    Retry,
}

#[derive(Clone, Copy, Message)]
/// Requests replacement of an already-visible gameplay projection without a menu transition.
pub(crate) enum RefreshGameplayProjection {
    /// Installs a newly loaded or resolved canonical turn with its normal presentation.
    CanonicalTurn,
    /// Reprojects the same practice turn for another locally controlled empire, optionally
    /// presenting that turn when this is the empire's first view of it.
    #[cfg(debug_assertions)]
    PracticePlayer {
        present_turn: bool,
    },
}

/// Restores local orders without replaying turn-boundary presentation or clearing selection.
#[derive(Message)]
pub(crate) struct RefreshTurnDraft;

/// Runtime backend and local profile storage hidden from gameplay systems.
#[derive(Resource)]
struct ClientRuntime {
    backend: Option<Arc<dyn MultiplayerBackend>>,
    realtime_config: Option<SupabaseConfig>,
    storage: Arc<dyn ClientStorage>,
    profile: ClientProfile,
    practice_return: Option<PracticeReturn>,
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    practice_players: Vec<PracticePlayer>,
}

/// One locally controlled identity and its independent editable practice draft.
#[cfg_attr(not(debug_assertions), allow(dead_code))]
#[derive(Clone)]
struct PracticePlayer {
    auth: AuthSession,
    membership: GameMembership,
    pending: PendingTurnCommands,
    /// Latest canonical turn whose presentation this locally controlled empire has seen.
    presented_turn: u64,
}

/// Online/mock state temporarily replaced while a local-practice match is active.
struct PracticeReturn {
    backend: Option<Arc<dyn MultiplayerBackend>>,
    realtime_config: Option<SupabaseConfig>,
    auth: Option<AuthSession>,
    games: Vec<GameSummary>,
    mock_backend: bool,
    connection: ConnectionStatus,
    notice: Option<String>,
}

/// In-flight tasks polled without blocking Bevy's main thread.
#[derive(Resource, Default)]
struct BackendTasks(Vec<Task<BackendOutput>>);

/// Purpose of an asynchronous operation, used for error and retry policy.
#[derive(Clone, Copy)]
enum Operation {
    Initialize,
    RefreshAuth,
    Reauthenticate,
    Create,
    Join,
    Recover,
    List,
    Load,
    ResumeLoad,
    Resume,
    Start,
    Color,
    Protection,
    JointAttack,
    Trade,
    Save,
    Submit,
    Withdraw,
    RestoreDraft,
    Events,
    Resolve,
    Presence,
    #[cfg(debug_assertions)]
    Practice,
    #[cfg(debug_assertions)]
    PracticeTurn,
}

/// Values returned by background backend tasks.
enum BackendOutput {
    Initialized {
        backend: Arc<dyn MultiplayerBackend>,
        session: AuthSession,
        games: Vec<GameSummary>,
        mock_backend: bool,
        configuration_notice: Option<String>,
        realtime_config: Option<SupabaseConfig>,
    },
    Membership {
        operation: Operation,
        result: MembershipResult,
        color_notice: Option<String>,
    },
    #[cfg(debug_assertions)]
    PracticeReady {
        backend: Arc<dyn MultiplayerBackend>,
        result: MembershipResult,
        players: Vec<PracticePlayer>,
    },
    Games(Vec<GameSummary>),
    ResumeLoaded(GameRecord, String),
    Record(Operation, GameRecord),
    Saved(SaveAcknowledgement),
    ProtectionChanged(ProtectionPermissionUpdate),
    JointAttackChanged(crate::multiplayer::model::JointAttackInvitation),
    JointAttacksLoaded(Vec<crate::multiplayer::model::JointAttackInvitation>),
    TradeChanged(TradeInvitation),
    TradesLoaded(Vec<TradeInvitation>),
    Resumed,
    Submitted(u64),
    Withdrawn(TurnSubmission),
    DraftLoaded(u64, Option<crate::multiplayer::model::StoredTurnSubmission>),
    Events(EventBatch),
    ResolutionWaiting,
    SessionRefreshed(AuthSession),
    Reauthenticated(AuthSession),
    Presence(Vec<GameMembership>),
    Left(GameId),
    DepartureFinished,
    Failed(Operation, BackendError),
    #[cfg(not(target_arch = "wasm32"))]
    TaskFailed(String),
}

/// Timer used for durable catch-up even when a Realtime wake-up is missed.
#[derive(Resource)]
struct EventPollTimer(Timer);

/// Timer that checks whether the current access token is close to expiry.
#[derive(Resource)]
struct AuthRefreshTimer(Timer);

/// Registers authentication, backend coordination, recovery, and sync systems.
pub struct MultiplayerClientPlugin;

impl Plugin for MultiplayerClientPlugin {
    /// Adds resources and asynchronous orchestration without coupling the core simulation to Bevy.
    fn build(&self, app: &mut App) {
        app.init_resource::<MultiplayerForm>()
            .init_resource::<MultiplayerSession>()
            .init_resource::<ConnectionIndicator>()
            .init_resource::<PendingTurnCommands>()
            .init_resource::<BackendTasks>()
            .init_resource::<profile::ProfileWrites>()
            .insert_resource(EventPollTimer(Timer::new(
                EVENT_POLL_INTERVAL_FALLBACK,
                TimerMode::Repeating,
            )))
            .insert_resource(AuthRefreshTimer(Timer::new(
                Duration::from_secs(30),
                TimerMode::Repeating,
            )))
            .insert_non_send(SupabaseRealtimeClient::default())
            .add_message::<MultiplayerRequest>()
            .add_message::<RefreshGameplayProjection>()
            .add_message::<RefreshTurnDraft>()
            .add_systems(Startup, initialize_client)
            .add_systems(OnExit(AppState::JoinGame), clear_join_error)
            .add_systems(OnExit(AppState::RecoverPlayer), clear_join_error)
            .add_systems(
                Update,
                (
                    process_requests,
                    poll_backend_tasks,
                    drive_turn_draft,
                    profile::sync_combat_preferences,
                    profile::flush_profile,
                    drive_reauthentication,
                    drive_auth_refresh,
                    drive_realtime,
                    drive_reload,
                    drive_joint_attack_reload,
                    drive_trade_reload,
                    drive_presence,
                    poll_durable_events,
                    drive_resolution,
                    update_connection_indicator,
                )
                    .chain(),
            );
    }
}

/// Page-local feedback must not follow the player through Back or Escape navigation.
fn clear_join_error(mut session: ResMut<MultiplayerSession>) {
    session.menu_error = None;
}

/// Creates platform storage and starts anonymous authentication asynchronously.
fn initialize_client(
    mut commands: Commands,
    mut tasks: ResMut<BackendTasks>,
    mut settings: ResMut<crate::core::settings::Settings>,
) {
    let storage = platform_storage();
    let profile = load_profile(storage.as_ref()).unwrap_or_default();
    settings.apply_combat_preferences(profile.combat_preferences);
    let stored_session = profile.session.clone();
    commands.insert_resource(ClientRuntime {
        backend: None,
        realtime_config: None,
        storage,
        profile,
        practice_return: None,
        practice_players: Vec::new(),
    });

    spawn_backend_task(&mut tasks, async move {
        let (backend, mock_backend, configuration_notice, realtime_config) = select_backend().await;
        match backend.authenticate(stored_session.as_ref()).await {
            Ok(session) => match backend.list_games(&session).await {
                Ok(games) => BackendOutput::Initialized {
                    backend,
                    session,
                    games,
                    mock_backend,
                    configuration_notice,
                    realtime_config,
                },
                Err(error) => BackendOutput::Failed(Operation::Initialize, error),
            },
            Err(error) => BackendOutput::Failed(Operation::Initialize, error),
        }
    });
}

/// Selects Stellarion's Supabase backend unless an isolated mock was explicitly requested.
async fn select_backend(
) -> (Arc<dyn MultiplayerBackend>, bool, Option<String>, Option<SupabaseConfig>) {
    if mock_requested() {
        return (
            Arc::new(InMemoryBackend::new()),
            true,
            Some(
                "Using the in-memory backend by request; games last only for this run.".to_string(),
            ),
            None,
        );
    }
    match SupabaseConfig::load() {
        Ok(config) => match SupabaseBackend::new(config.clone()) {
            Ok(backend) => (Arc::new(backend), false, None, Some(config)),
            Err(error) => (
                Arc::new(InMemoryBackend::new()),
                true,
                Some(format!(
                    "{}. Using the in-memory backend because the built-in Supabase configuration is invalid.",
                    ConfigError::Invalid(error.to_string())
                )),
                None,
            ),
        },
        Err(error) => (
            Arc::new(InMemoryBackend::new()),
            true,
            Some(format!(
                "{error}. Using the in-memory backend because the built-in Supabase configuration is invalid."
            )),
            None,
        ),
    }
}

/// Creates a complete locally controlled match on an isolated in-memory backend.
#[cfg(debug_assertions)]
async fn create_local_practice(
    rules: GameRules,
) -> Result<(Arc<dyn MultiplayerBackend>, MembershipResult, Vec<PracticePlayer>), BackendError> {
    let backend = Arc::new(InMemoryBackend::new());
    let host_auth = backend.authenticate(None).await?;
    let player_count = rules.player_count;
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed).map_err(|error| BackendError::Protocol(error.to_string()))?;
    let recovery =
        RecoveryCode::generate().map_err(|error| BackendError::Protocol(error.to_string()))?;
    let model = GameModel::new(seed, rules)
        .map_err(|error| BackendError::InvalidData(error.to_string()))?;
    let mut result = backend
        .create_game(
            &host_auth,
            CreateGameRequest {
                code: generate_game_code()
                    .map_err(|error| BackendError::Protocol(error.to_string()))?,
                display_name: "Practice P1".to_string(),
                recovery_code: recovery.expose().to_string(),
                persisted: PersistedGame::new(model),
            },
        )
        .await?;
    let mut players = vec![PracticePlayer {
        auth: host_auth.clone(),
        membership: result.membership.clone(),
        pending: PendingTurnCommands::default(),
        presented_turn: 0,
    }];
    for player_id in 2..=u64::from(player_count) {
        let auth = backend.authenticate(None).await?;
        let recovery =
            RecoveryCode::generate().map_err(|error| BackendError::Protocol(error.to_string()))?;
        let joined = backend
            .join_game(
                &auth,
                JoinGameRequest {
                    code: result.game.code.clone(),
                    display_name: format!("Practice P{player_id}"),
                    recovery_code: recovery.expose().to_string(),
                },
            )
            .await?;
        result.game = joined.game;
        players.push(PracticePlayer {
            auth,
            membership: joined.membership,
            pending: PendingTurnCommands::default(),
            presented_turn: 0,
        });
    }
    let mut started = result.game.persisted.clone();
    started.state.start().map_err(|error| BackendError::InvalidData(error.to_string()))?;
    result.game =
        backend.start_game(&host_auth, &result.game.id, result.game.revision, started).await?;
    for player in &players {
        backend.set_connected(&player.auth, &result.game.id, true).await?;
    }
    result.game = backend.load_game(&host_auth, &result.game.id).await?;
    for player in &mut players {
        player.membership = result
            .game
            .membership_for(&player.auth.user_id)
            .cloned()
            .ok_or(BackendError::PlayerNoLongerInGame)?;
        player.pending.reset(result.game.persisted.state.turn);
        // Initial loading is intentionally quiet; there is no resolved turn to replay yet.
        player.presented_turn = result.game.persisted.state.turn;
    }
    result.membership = players[0].membership.clone();
    Ok((backend, result, players))
}

/// Retains the selected empire's independent draft before changing practice projections.
#[cfg(debug_assertions)]
fn store_selected_practice_draft(
    runtime: &mut ClientRuntime,
    session: &MultiplayerSession,
    pending: &PendingTurnCommands,
) {
    let Some(player_id) = session.membership.as_ref().map(|member| member.player_id) else {
        return;
    };
    if let Some(player) =
        runtime.practice_players.iter_mut().find(|player| player.membership.player_id == player_id)
    {
        player.pending = pending.clone();
    }
}

/// Builds one complete stable submission set from the locally controlled practice drafts.
#[cfg(debug_assertions)]
fn local_practice_submissions(
    record: &GameRecord,
    players: &[PracticePlayer],
) -> Result<Vec<(AuthSession, TurnSubmission)>, BackendError> {
    if record.status != MatchStatus::Active {
        return Err(BackendError::InvalidGameStatus);
    }
    record
        .persisted
        .state
        .players
        .iter()
        .filter(|player| !player.spectator)
        .map(|model_player| {
            let player = players
                .iter()
                .find(|player| player.membership.player_id == model_player.id)
                .ok_or(BackendError::PlayerNoLongerInGame)?;
            if player.pending.turn != record.persisted.state.turn {
                return Err(BackendError::StaleSubmission {
                    expected: record.persisted.state.turn,
                    actual: player.pending.turn,
                });
            }
            if player.pending.resume_requested || !player.pending.queued_commands.is_empty() {
                return Err(BackendError::InvalidData(format!(
                    "Practice Player {} is still returning to an editable turn",
                    model_player.id
                )));
            }
            if !matches!(player.pending.submission, SubmissionState::Draft | SubmissionState::Retry)
            {
                return Err(BackendError::InvalidData(format!(
                    "Practice Player {} already finished this turn",
                    model_player.id
                )));
            }
            let mut submission = TurnSubmission::new(
                model_player.id,
                player.pending.turn,
                player.pending.commands.clone(),
            );
            submission.generation = player.pending.generation;
            Ok((player.auth.clone(), submission))
        })
        .collect()
}

/// Returns whether local development explicitly selected the mock backend.
fn mock_requested() -> bool {
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::env::var("STELLARION_BACKEND").is_ok_and(|value| value.eq_ignore_ascii_case("mock"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        option_env!("STELLARION_BACKEND").is_some_and(|value| value.eq_ignore_ascii_case("mock"))
    }
}

/// Creates browser localStorage or a platform application-data store with a safe fallback.
fn platform_storage() -> Arc<dyn ClientStorage> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        NativeStorage::new()
            .map(|storage| Arc::new(storage) as Arc<dyn ClientStorage>)
            .unwrap_or_else(|_| Arc::new(MemoryStorage::default()))
    }
    #[cfg(target_arch = "wasm32")]
    {
        BrowserStorage::new()
            .map(|storage| Arc::new(storage) as Arc<dyn ClientStorage>)
            .unwrap_or_else(|_| Arc::new(MemoryStorage::default()))
    }
}

/// Converts foreground requests into independent browser-compatible backend futures.
fn process_requests(
    mut requests: MessageReader<MultiplayerRequest>,
    mut messages: MessageWriter<MessageMsg>,
    mut refresh_gameplay: MessageWriter<RefreshGameplayProjection>,
    mut session: ResMut<MultiplayerSession>,
    mut runtime: ResMut<ClientRuntime>,
    mut pending: ResMut<PendingTurnCommands>,
    mut form: ResMut<MultiplayerForm>,
    mut tasks: ResMut<BackendTasks>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    #[cfg(not(debug_assertions))]
    let _ = &mut refresh_gameplay;
    for request in requests.read() {
        session.menu_error = None;
        if matches!(request, MultiplayerRequest::LeaveGame) {
            // Take ownership of pending work so none of its results can restore the lobby.
            let outstanding = std::mem::take(&mut tasks.0);
            session.busy = false;
            if session.local_practice {
                #[cfg(debug_assertions)]
                runtime.practice_players.clear();
                session.leave_selected_game();
                pending.reset(0);
                if let Some(previous) = runtime.practice_return.take() {
                    runtime.backend = previous.backend;
                    runtime.realtime_config = previous.realtime_config;
                    session.auth = previous.auth;
                    session.games = previous.games;
                    session.mock_backend = previous.mock_backend;
                    session.connection = previous.connection;
                    session.notice = previous.notice;
                }
                next_state.set(AppState::MainMenu);
                requests.clear();
                return;
            }
            let departure = session.active_game.as_ref().map(|record| record.id.clone());
            if let (Some(backend), Some(auth), Some(record)) =
                (runtime.backend.clone(), session.auth.clone(), session.active_game.clone())
            {
                spawn_backend_task(&mut tasks, async move {
                    // Finish earlier presence/start requests before disconnecting. Their
                    // results are discarded; leaving locally never waits for this cleanup.
                    for task in outstanding {
                        let _ = task.await;
                    }
                    let _ = backend.set_connected(&auth, &record.id, false).await;
                    BackendOutput::DepartureFinished
                });
            }
            if let Some(game_id) = departure {
                apply_output(
                    BackendOutput::Left(game_id),
                    &mut runtime,
                    &mut session,
                    &mut form,
                    &mut pending,
                    &mut next_state,
                    false,
                );
            } else {
                session.leave_selected_game();
                session.notice = None;
                pending.reset(0);
                next_state.set(AppState::MainMenu);
            }
            // Ignore any clicks queued on the screen that was just left.
            requests.clear();
            return;
        }
        #[cfg(debug_assertions)]
        if let MultiplayerRequest::StartLocalPractice {
            rules,
        } = request
        {
            runtime.practice_return = Some(PracticeReturn {
                backend: runtime.backend.clone(),
                realtime_config: runtime.realtime_config.clone(),
                auth: session.auth.clone(),
                games: session.games.clone(),
                mock_backend: session.mock_backend,
                connection: session.connection,
                notice: session.notice.clone(),
            });
            session.busy = true;
            session.notice = None;
            let rules = rules.clone();
            spawn_backend_task(&mut tasks, async move {
                match create_local_practice(rules).await {
                    Ok((backend, result, players)) => BackendOutput::PracticeReady {
                        backend,
                        result,
                        players,
                    },
                    Err(error) => BackendOutput::Failed(Operation::Practice, error),
                }
            });
            continue;
        }
        #[cfg(debug_assertions)]
        if let MultiplayerRequest::SwitchLocalPracticePlayer(player_id) = request {
            if !session.local_practice || !tasks.0.is_empty() {
                continue;
            }
            store_selected_practice_draft(&mut runtime, &session, &pending);
            if session.membership.as_ref().map(|member| member.player_id) == Some(*player_id) {
                continue;
            }
            let current_turn = session
                .active_game
                .as_ref()
                .map_or(pending.turn, |record| record.persisted.state.turn);
            let Some(player) = runtime
                .practice_players
                .iter_mut()
                .find(|player| player.membership.player_id == *player_id)
            else {
                request_error(&mut session, "That practice player is unavailable.");
                continue;
            };
            let present_turn = player.presented_turn != current_turn;
            player.presented_turn = current_turn;
            let player = player.clone();
            session.auth = Some(player.auth);
            session.membership = Some(player.membership);
            session.submitted_turn = matches!(
                player.pending.submission,
                SubmissionState::Sending | SubmissionState::Accepted | SubmissionState::Retry
            )
            .then_some(player.pending.turn);
            session.restore_draft_needed = false;
            session.resolve_needed = false;
            session.notice = None;
            *pending = player.pending;
            refresh_gameplay.write(RefreshGameplayProjection::PracticePlayer {
                present_turn,
            });
            continue;
        }
        #[cfg(debug_assertions)]
        if matches!(request, MultiplayerRequest::AdvanceLocalPracticeTurn) {
            if !session.local_practice || !tasks.0.is_empty() {
                continue;
            }
            store_selected_practice_draft(&mut runtime, &session, &pending);
            let Some(record) = session.active_game.clone() else {
                request_error(&mut session, "No local practice game is selected.");
                continue;
            };
            let submissions = match local_practice_submissions(&record, &runtime.practice_players) {
                Ok(submissions) => submissions,
                Err(error) => {
                    request_error(&mut session, &error.to_string());
                    messages.write(MessageMsg::error(error.to_string()));
                    continue;
                },
            };
            let commands =
                submissions.iter().map(|(_, submission)| submission.clone()).collect::<Vec<_>>();
            let next = match resolved_turn(&record.persisted.state, &commands) {
                Ok((next, _)) => PersistedGame::new(next),
                Err(error) => {
                    request_error(&mut session, &error.to_string());
                    messages.write(MessageMsg::error(error.to_string()));
                    continue;
                },
            };
            let Some(backend) = runtime.backend.clone() else {
                request_error(&mut session, "The local practice backend is unavailable.");
                continue;
            };
            for player in &mut runtime.practice_players {
                if submissions
                    .iter()
                    .any(|(_, submission)| submission.player_id == player.membership.player_id)
                {
                    player.pending.submission = SubmissionState::Sending;
                }
            }
            if let Some(current) = session.membership.as_ref().and_then(|membership| {
                runtime
                    .practice_players
                    .iter()
                    .find(|player| player.membership.player_id == membership.player_id)
            }) {
                *pending = current.pending.clone();
            }
            let Some(resolver) = runtime.practice_players.first().map(|player| player.auth.clone())
            else {
                request_error(&mut session, "No local practice players are available.");
                continue;
            };
            session.busy = true;
            session.notice = Some("Resolving every local player's turn…".to_string());
            spawn_backend_task(&mut tasks, async move {
                for (auth, submission) in submissions {
                    if let Err(error) = backend.submit_turn(&auth, &record.id, submission).await {
                        return BackendOutput::Failed(Operation::PracticeTurn, error);
                    }
                }
                match backend
                    .publish_resolution(
                        &resolver,
                        &record.id,
                        record.revision,
                        record.persisted.state.turn,
                        next,
                    )
                    .await
                {
                    Ok(record) => BackendOutput::Record(Operation::PracticeTurn, record),
                    Err(error) => BackendOutput::Failed(Operation::PracticeTurn, error),
                }
            });
            continue;
        }
        if matches!(request, MultiplayerRequest::Retry) {
            session.connection = ConnectionStatus::Reconnecting;
            session.reload_needed = session.has_active_game();
            if !session.has_active_game() {
                spawn_list(&runtime, &session, &mut tasks);
            }
            continue;
        }

        let (Some(backend), Some(auth)) = (runtime.backend.clone(), session.auth.clone()) else {
            let error = "Authentication is still initializing.";
            request_error(&mut session, error);
            if matches!(request, MultiplayerRequest::SaveGame) {
                messages.write(MessageMsg::error(error));
            }
            continue;
        };
        session.busy = true;
        session.notice = None;

        match request {
            #[cfg(debug_assertions)]
            MultiplayerRequest::StartLocalPractice {
                ..
            } => unreachable!("local practice is handled before online authentication"),
            #[cfg(debug_assertions)]
            MultiplayerRequest::SwitchLocalPracticePlayer(_)
            | MultiplayerRequest::AdvanceLocalPracticeTurn => {
                unreachable!("local practice controls are handled before online authentication")
            },
            MultiplayerRequest::CreateGame {
                display_name,
                rules,
            } => {
                let display_name = display_name.trim().to_string();
                runtime.profile.display_name.clone_from(&display_name);
                let rules = rules.clone();
                spawn_backend_task(&mut tasks, async move {
                    let mut seed = [0_u8; 32];
                    if let Err(error) = getrandom::fill(&mut seed) {
                        return BackendOutput::Failed(
                            Operation::Create,
                            BackendError::Protocol(error.to_string()),
                        );
                    }
                    let recovery = match RecoveryCode::generate() {
                        Ok(code) => code,
                        Err(error) => {
                            return BackendOutput::Failed(
                                Operation::Create,
                                BackendError::Protocol(error.to_string()),
                            )
                        },
                    };
                    let model = match GameModel::new(seed, rules) {
                        Ok(model) => model,
                        Err(error) => {
                            return BackendOutput::Failed(
                                Operation::Create,
                                BackendError::InvalidData(error.to_string()),
                            )
                        },
                    };
                    for _ in 0..8 {
                        let code = match generate_game_code() {
                            Ok(code) => code,
                            Err(error) => {
                                return BackendOutput::Failed(
                                    Operation::Create,
                                    BackendError::Protocol(error.to_string()),
                                )
                            },
                        };
                        let result = backend
                            .create_game(
                                &auth,
                                CreateGameRequest {
                                    code,
                                    display_name: display_name.clone(),
                                    recovery_code: recovery.expose().to_string(),
                                    persisted: PersistedGame::new(model.clone()),
                                },
                            )
                            .await;
                        match result {
                            Ok(result) => {
                                return BackendOutput::Membership {
                                    operation: Operation::Create,
                                    result,
                                    color_notice: None,
                                }
                            },
                            Err(BackendError::GameCodeCollision) => {},
                            Err(error) => return BackendOutput::Failed(Operation::Create, error),
                        }
                    }
                    BackendOutput::Failed(Operation::Create, BackendError::GameCodeCollision)
                });
            },
            MultiplayerRequest::JoinGame {
                display_name,
                code,
            } => {
                runtime.profile.display_name = display_name.trim().to_string();
                let recovery = match RecoveryCode::generate() {
                    Ok(recovery) => recovery,
                    Err(error) => {
                        request_error(&mut session, &error.to_string());
                        continue;
                    },
                };
                let request = JoinGameRequest {
                    code: GameCode::new(code),
                    display_name: display_name.trim().to_string(),
                    recovery_code: recovery.expose().to_string(),
                };
                spawn_backend_task(&mut tasks, async move {
                    match backend.join_game(&auth, request).await {
                        Ok(result) => BackendOutput::Membership {
                            operation: Operation::Join,
                            result,
                            color_notice: None,
                        },
                        Err(error) => BackendOutput::Failed(Operation::Join, error),
                    }
                });
            },
            MultiplayerRequest::RecoverPlayer {
                code,
                recovery_code,
            } => {
                let code = GameCode::new(code);
                // Existing membership needs no recovery credential.
                if let Some(summary) = linked_game(&session.games, &code) {
                    spawn_backend_task(
                        &mut tasks,
                        load_game_for_resume(
                            backend,
                            auth,
                            summary.id.clone(),
                            summary.recovery_code.clone(),
                        ),
                    );
                    continue;
                }
                let supplied = match RecoveryCode::parse(recovery_code) {
                    Ok(code) => code,
                    Err(error) => {
                        request_error(&mut session, &error.to_string());
                        continue;
                    },
                };
                let request = RecoverPlayerRequest {
                    code,
                    recovery_code: supplied.expose().to_string(),
                };
                spawn_backend_task(
                    &mut tasks,
                    recover_or_resume_linked_game(backend, auth, request),
                );
            },
            MultiplayerRequest::ResumeGame(game_id) => {
                let Some(summary) = session.games.iter().find(|game| &game.id == game_id) else {
                    request_error(&mut session, GAME_UNAVAILABLE_NOTICE);
                    continue;
                };
                spawn_backend_task(
                    &mut tasks,
                    load_game_for_resume(
                        backend,
                        auth,
                        summary.id.clone(),
                        summary.recovery_code.clone(),
                    ),
                );
            },
            MultiplayerRequest::ResumeActiveGame => {
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No game is selected.");
                    continue;
                };
                spawn_backend_task(&mut tasks, async move {
                    match backend.resume_game(&auth, &record.id).await {
                        Ok(()) => BackendOutput::Resumed,
                        Err(error) => BackendOutput::Failed(Operation::Resume, error),
                    }
                });
            },
            MultiplayerRequest::RefreshGames => {
                spawn_backend_task(&mut tasks, async move {
                    match backend.list_games(&auth).await {
                        Ok(games) => BackendOutput::Games(games),
                        Err(error) => BackendOutput::Failed(Operation::List, error),
                    }
                });
            },
            MultiplayerRequest::StartGame => {
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No lobby is selected.");
                    continue;
                };
                let mut seed = [0_u8; 32];
                if let Err(error) = getrandom::fill(&mut seed) {
                    request_error(&mut session, &error.to_string());
                    continue;
                }
                let persisted = match started_snapshot_for_members(&record, seed) {
                    Ok(persisted) => persisted,
                    Err(error) => {
                        request_error(&mut session, &error.to_string());
                        continue;
                    },
                };
                spawn_backend_task(&mut tasks, async move {
                    match backend.start_game(&auth, &record.id, record.revision, persisted).await {
                        Ok(record) => BackendOutput::Record(Operation::Start, record),
                        Err(error) => BackendOutput::Failed(Operation::Start, error),
                    }
                });
            },
            MultiplayerRequest::SetPlayerColor(color) => {
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No lobby player is selected.");
                    continue;
                };
                let color = *color;
                spawn_backend_task(&mut tasks, async move {
                    match backend.set_player_color(&auth, &record.id, color).await {
                        Ok(record) => BackendOutput::Record(Operation::Color, record),
                        Err(error) => BackendOutput::Failed(Operation::Color, error),
                    }
                });
            },
            MultiplayerRequest::SetProtectionPermission {
                planet_id,
                protector,
                allowed,
            } => {
                if session.protection_update_pending {
                    continue;
                }
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No active game is selected.");
                    continue;
                };
                session.protection_update_pending = true;
                let (planet_id, protector, allowed) = (*planet_id, *protector, *allowed);
                spawn_backend_task(&mut tasks, async move {
                    match backend
                        .set_protection_permission(&auth, &record.id, planet_id, protector, allowed)
                        .await
                    {
                        Ok(update) => BackendOutput::ProtectionChanged(update),
                        Err(error) => BackendOutput::Failed(Operation::Protection, error),
                    }
                });
            },
            MultiplayerRequest::CreateJointAttack(invitation) => {
                if session.joint_attack_update_pending {
                    continue;
                }
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No active game is selected.");
                    continue;
                };
                session.joint_attack_update_pending = true;
                let invitation = invitation.clone();
                spawn_backend_task(&mut tasks, async move {
                    match backend.create_joint_attack(&auth, &record.id, invitation).await {
                        Ok(invitation) => BackendOutput::JointAttackChanged(invitation),
                        Err(error) => BackendOutput::Failed(Operation::JointAttack, error),
                    }
                });
            },
            MultiplayerRequest::RespondJointAttack {
                attack_id,
                response,
                contribution,
            } => {
                if session.joint_attack_update_pending {
                    continue;
                }
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No active game is selected.");
                    continue;
                };
                session.joint_attack_update_pending = true;
                let (attack_id, response, contribution) =
                    (*attack_id, *response, contribution.clone());
                spawn_backend_task(&mut tasks, async move {
                    match backend
                        .respond_joint_attack(&auth, &record.id, attack_id, response, contribution)
                        .await
                    {
                        Ok(invitation) => BackendOutput::JointAttackChanged(invitation),
                        Err(error) => BackendOutput::Failed(Operation::JointAttack, error),
                    }
                });
            },
            MultiplayerRequest::CancelJointAttack {
                attack_id,
            } => {
                if session.joint_attack_update_pending {
                    continue;
                }
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No active game is selected.");
                    continue;
                };
                session.joint_attack_update_pending = true;
                let attack_id = *attack_id;
                spawn_backend_task(&mut tasks, async move {
                    match backend.cancel_joint_attack(&auth, &record.id, attack_id).await {
                        Ok(invitation) => BackendOutput::JointAttackChanged(invitation),
                        Err(error) => BackendOutput::Failed(Operation::JointAttack, error),
                    }
                });
            },
            MultiplayerRequest::RefreshJointAttacks => {
                if session.joint_attack_update_pending {
                    continue;
                }
                let Some(record) = session.active_game.clone() else {
                    continue;
                };
                session.joint_attack_update_pending = true;
                spawn_backend_task(&mut tasks, async move {
                    match backend.load_joint_attacks(&auth, &record.id).await {
                        Ok(invitations) => BackendOutput::JointAttacksLoaded(invitations),
                        Err(error) => BackendOutput::Failed(Operation::JointAttack, error),
                    }
                });
            },
            MultiplayerRequest::CreateTrade(invitation) => {
                if session.trade_update_pending {
                    continue;
                }
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No active game is selected.");
                    continue;
                };
                session.trade_update_pending = true;
                let invitation = invitation.clone();
                spawn_backend_task(&mut tasks, async move {
                    match backend.create_trade(&auth, &record.id, invitation).await {
                        Ok(invitation) => BackendOutput::TradeChanged(invitation),
                        Err(error) => BackendOutput::Failed(Operation::Trade, error),
                    }
                });
            },
            MultiplayerRequest::RespondTrade {
                trade_id,
                resources,
                response,
            } => {
                if session.trade_update_pending {
                    continue;
                }
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No active game is selected.");
                    continue;
                };
                session.trade_update_pending = true;
                let (trade_id, resources, response) = (*trade_id, *resources, *response);
                spawn_backend_task(&mut tasks, async move {
                    match backend
                        .respond_trade(&auth, &record.id, trade_id, resources, response)
                        .await
                    {
                        Ok(invitation) => BackendOutput::TradeChanged(invitation),
                        Err(error) => BackendOutput::Failed(Operation::Trade, error),
                    }
                });
            },
            MultiplayerRequest::RefreshTrades => {
                if session.trade_update_pending {
                    continue;
                }
                let Some(record) = session.active_game.clone() else {
                    continue;
                };
                session.trade_update_pending = true;
                spawn_backend_task(&mut tasks, async move {
                    match backend.load_trades(&auth, &record.id).await {
                        Ok(invitations) => BackendOutput::TradesLoaded(invitations),
                        Err(error) => BackendOutput::Failed(Operation::Trade, error),
                    }
                });
            },
            MultiplayerRequest::SaveGame => {
                let (Some(record), Some(membership)) =
                    (session.active_game.clone(), session.membership.clone())
                else {
                    let error = "No game is selected.";
                    request_error(&mut session, error);
                    messages.write(MessageMsg::error(error));
                    continue;
                };
                if pending.turn != record.persisted.state.turn {
                    let error = "The local command draft is stale; reload the game before saving.";
                    request_error(&mut session, error);
                    messages.write(MessageMsg::error(error));
                    continue;
                }
                if pending.resume_requested || !pending.queued_commands.is_empty() {
                    let error = "Wait for Continue Turn to finish before saving the updated draft.";
                    request_error(&mut session, error);
                    messages.write(MessageMsg::error(error));
                    continue;
                }
                let mut draft = TurnSubmission::new(
                    membership.player_id,
                    pending.turn,
                    pending.commands.clone(),
                );
                draft.generation = pending.generation;
                spawn_backend_task(&mut tasks, async move {
                    match backend.save_game(&auth, &record.id, record.revision, draft).await {
                        Ok(acknowledgement) => BackendOutput::Saved(acknowledgement),
                        Err(error) => BackendOutput::Failed(Operation::Save, error),
                    }
                });
            },
            MultiplayerRequest::SubmitTurn => {
                let (Some(record), Some(membership)) =
                    (session.active_game.clone(), session.membership.clone())
                else {
                    request_error(&mut session, "No active player slot is selected.");
                    continue;
                };
                if pending.turn != record.persisted.state.turn {
                    request_error(
                        &mut session,
                        "The local command draft is stale; reload the game.",
                    );
                    continue;
                }
                if !pending.begin_submission() {
                    session.busy = !tasks.0.is_empty();
                    continue;
                }
                let mut submission = TurnSubmission::new(
                    membership.player_id,
                    pending.turn,
                    pending.commands.clone(),
                );
                submission.generation = pending.generation;
                let submitted_turn = submission.turn;
                spawn_backend_task(&mut tasks, async move {
                    match backend.submit_turn(&auth, &record.id, submission).await {
                        Ok(_) => BackendOutput::Submitted(submitted_turn),
                        Err(error) => BackendOutput::Failed(Operation::Submit, error),
                    }
                });
            },
            MultiplayerRequest::LeaveGame | MultiplayerRequest::Retry => {},
        }
    }
}

/// Resets the busy flag and records an immediate request-validation failure.
fn request_error(session: &mut MultiplayerSession, message: &str) {
    session.busy = false;
    session.notice = Some(message.to_string());
    session.menu_error = Some(message.to_string());
}

/// Translates backend categories into actionable menu copy without leaking storage terminology.
fn user_facing_backend_error(operation: Operation, error: &BackendError) -> String {
    match (operation, error) {
        (Operation::Join, BackendError::InvalidGameStatus) => concat!(
            "This game has already started. To continue as an existing player, choose Resume Game. ",
            "If the game isn't listed, choose Recover Game there and enter your game and recovery codes."
        )
        .to_string(),
        (Operation::Recover, BackendError::GameNotFound) => {
            "No saved game matches this game code. Check the game code and try again.".to_string()
        },
        (Operation::Recover, BackendError::InvalidRecoveryCode) => {
            "This recovery code is invalid. Each player needs their own private recovery code. Check both codes and try again.".to_string()
        },
        (Operation::Recover, BackendError::RecoveryCodeInUse) => {
            "This recovery code is already in use. Use your own private recovery code. To move this player here, leave the other window first; after an unexpected close, wait a minute.".to_string()
        },
        (Operation::Start, BackendError::InvalidGameStatus) => {
            "At least two players must be in the lobby before the host can start the game."
                .to_string()
        },
        (Operation::Resume, BackendError::InvalidGameStatus) => {
            "Every player must reconnect before the host can resume this game.".to_string()
        },
        (Operation::Save, BackendError::GameNotFound) => {
            GAME_UNAVAILABLE_NOTICE.to_string()
        },
        (_, BackendError::InvalidGameStatus) => {
            "This action is not available for the game right now. Refresh the game and try again."
                .to_string()
        },
        (_, BackendError::Forbidden) => {
            "You don't have access to this action in this game. Return to Resume Game and reconnect as your own player.".to_string()
        },
        _ => error.to_string(),
    }
}

/// Reports explicit saves and rejected turn orders in the gameplay HUD.
fn operation_notification(output: &BackendOutput) -> Option<MessageMsg> {
    match output {
        BackendOutput::Membership {
            color_notice: Some(notice),
            ..
        } => Some(MessageMsg::warning(notice.clone())),
        BackendOutput::Saved(_) => Some(MessageMsg::info(GAME_SAVED_NOTICE)),
        BackendOutput::Failed(Operation::Save, error) => {
            Some(MessageMsg::error(user_facing_backend_error(Operation::Save, error)))
        },
        BackendOutput::Failed(Operation::Protection, error) => Some(MessageMsg::error(format!(
            "Could not update protection access: {}",
            user_facing_backend_error(Operation::Protection, error)
        ))),
        BackendOutput::Failed(Operation::JointAttack, error) => Some(MessageMsg::error(format!(
            "Could not update the joint attack: {}",
            user_facing_backend_error(Operation::JointAttack, error)
        ))),
        BackendOutput::Failed(Operation::Trade, error) => Some(MessageMsg::error(format!(
            "Could not update the trade: {}",
            user_facing_backend_error(Operation::Trade, error)
        ))),
        BackendOutput::Failed(
            Operation::Submit | Operation::Resolve,
            error @ BackendError::InvalidData(_),
        ) => Some(MessageMsg::error(format!("Could not end turn: {error}"))),
        _ => None,
    }
}

/// Announces protection invitations and revocations to the affected player.
fn protection_permission_notifications(
    output: &BackendOutput,
    session: &MultiplayerSession,
) -> Vec<MessageMsg> {
    let BackendOutput::Record(_, next) = output else {
        return Vec::new();
    };
    let (Some(previous), Some(local_player)) = (
        session.active_game.as_ref().filter(|game| game.id == next.id),
        session.membership.as_ref().map(|membership| membership.player_id),
    ) else {
        return Vec::new();
    };
    next.persisted
        .state
        .map
        .planets
        .iter()
        .filter_map(|planet| {
            let before = previous
                .persisted
                .state
                .map
                .try_get(planet.id)
                .is_some_and(|planet| planet.protection_permissions.contains(&local_player));
            let after = planet.protection_permissions.contains(&local_player);
            (before != after).then(|| {
                let message = if after {
                    MessageMsg::info(format!("You can now protect planet {}.", planet.name))
                } else {
                    MessageMsg::warning(format!(
                        "Protection access to planet {} was revoked. Any protecting fleet is returning home.",
                        planet.name
                    ))
                };
                message.with_action(MessageAction::FocusPlanet(planet.id))
            })
        })
        .collect()
}

fn revoked_protection_targets(output: &BackendOutput, session: &MultiplayerSession) -> Vec<usize> {
    let BackendOutput::Record(_, next) = output else {
        return Vec::new();
    };
    let (Some(previous), Some(local_player)) = (
        session.active_game.as_ref().filter(|game| game.id == next.id),
        session.membership.as_ref().map(|membership| membership.player_id),
    ) else {
        return Vec::new();
    };
    previous
        .persisted
        .state
        .map
        .planets
        .iter()
        .filter(|planet| planet.protection_permissions.contains(&local_player))
        .filter(|planet| {
            next.persisted
                .state
                .map
                .try_get(planet.id)
                .is_none_or(|next| !next.protection_permissions.contains(&local_player))
        })
        .map(|planet| planet.id)
        .collect()
}

/// Treats removal of a waiting lobby as a brief status update, not an actionable failure.
fn host_closed_lobby_notification(
    output: &BackendOutput,
    session: &MultiplayerSession,
) -> Option<MessageMsg> {
    let host_closed_lobby = matches!(output, BackendOutput::Failed(_, BackendError::GameNotFound))
        && session.active_game.as_ref().is_some_and(|record| record.status == MatchStatus::Lobby);
    host_closed_lobby.then(|| {
        MessageMsg::info(HOST_CLOSED_LOBBY_NOTICE).with_duration(HOST_CLOSED_LOBBY_NOTICE_DURATION)
    })
}

/// Reports opposing players whose canonical presence changed from connected to disconnected.
fn disconnected_player_notifications(
    output: &BackendOutput,
    session: &MultiplayerSession,
) -> Vec<MessageMsg> {
    if session.local_practice {
        return Vec::new();
    }
    let (game_id, status, members) = match output {
        BackendOutput::Record(_, next) => (&next.id, next.status, next.members.as_slice()),
        BackendOutput::Presence(members) => {
            let Some(game) = &session.active_game else {
                return Vec::new();
            };
            (&game.id, game.status, members.as_slice())
        },
        _ => return Vec::new(),
    };
    let Some(previous) = session.active_game.as_ref().filter(|game| &game.id == game_id) else {
        return Vec::new();
    };
    if status == MatchStatus::Lobby {
        return Vec::new();
    }
    let local_player_id = session.membership.as_ref().map(|member| member.player_id);
    members
        .iter()
        .filter(|member| Some(member.player_id) != local_player_id && !member.connected)
        .filter(|member| {
            previous
                .members
                .iter()
                .any(|previous| previous.player_id == member.player_id && previous.connected)
        })
        .map(|member| MessageMsg::warning(format!("Player {} disconnected.", member.display_name)))
        .collect()
}

/// Polls task futures once per frame and applies completed backend results.
fn poll_backend_tasks(
    mut tasks: ResMut<BackendTasks>,
    mut runtime: ResMut<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut form: ResMut<MultiplayerForm>,
    mut pending: ResMut<PendingTurnCommands>,
    mut next_state: ResMut<NextState<AppState>>,
    app_state: Res<State<AppState>>,
    mut refresh_gameplay: MessageWriter<RefreshGameplayProjection>,
    mut refresh_draft: MessageWriter<RefreshTurnDraft>,
    mut messages: MessageWriter<MessageMsg>,
) {
    let mut remaining = Vec::with_capacity(tasks.0.len());
    for mut task in std::mem::take(&mut tasks.0) {
        if let Some(output) = block_on(poll_once(&mut task)) {
            let restore_draft = matches!(&output, BackendOutput::Withdrawn(draft) if draft.turn == pending.turn)
                || matches!(&output, BackendOutput::DraftLoaded(turn, _) if *turn == pending.turn);
            let notification = operation_notification(&output);
            let lobby_closed_notification = host_closed_lobby_notification(&output, &session);
            let presence_notifications = disconnected_player_notifications(&output, &session);
            let protection_notifications = protection_permission_notifications(&output, &session);
            let revoked_targets = revoked_protection_targets(&output, &session);
            let refresh_protection = matches!(
                &output,
                BackendOutput::ProtectionChanged(_)
                    | BackendOutput::Failed(Operation::Protection, _)
            ) || !protection_notifications.is_empty();
            let refresh_trade = matches!(
                &output,
                BackendOutput::TradeChanged(invitation) if invitation.finalized
            ) || matches!(&output, BackendOutput::Record(Operation::Load, _));
            let gameplay_visible = *app_state.get() == AppState::Game;
            let previous_projection = session
                .active_game
                .as_ref()
                .map(|record| (record.id.clone(), record.status, record.persisted.state.turn));
            apply_output(
                output,
                &mut runtime,
                &mut session,
                &mut form,
                &mut pending,
                &mut next_state,
                gameplay_visible,
            );
            if !revoked_targets.is_empty() {
                let revoked = |command: &crate::core::simulation::TurnCommand| {
                    matches!(
                        command,
                        crate::core::simulation::TurnCommand::SendMission {
                            destination,
                            objective: crate::core::map::icon::Icon::Protect,
                            ..
                        } if revoked_targets.contains(destination)
                    )
                };
                pending.commands.retain(|command| !revoked(command));
                pending.queued_commands.retain(|command| !revoked(command));
            }
            if gameplay_visible && restore_draft {
                refresh_draft.write(RefreshTurnDraft);
            }
            if gameplay_visible && refresh_protection {
                refresh_draft.write(RefreshTurnDraft);
            }
            if gameplay_visible && refresh_trade {
                refresh_draft.write(RefreshTurnDraft);
            }
            if let Some(notification) = notification {
                messages.write(notification);
            }
            if let Some(notification) = lobby_closed_notification {
                messages.write(notification);
            }
            for notification in presence_notifications {
                messages.write(notification);
            }
            for notification in protection_notifications {
                messages.write(notification);
            }
            let current_projection = session
                .active_game
                .as_ref()
                .map(|record| (record.id.clone(), record.status, record.persisted.state.turn));
            if gameplay_visible
                && previous_projection != current_projection
                && current_projection
                    .as_ref()
                    .is_some_and(|(_, status, _)| !matches!(status, MatchStatus::Lobby))
            {
                refresh_gameplay.write(RefreshGameplayProjection::CanonicalTurn);
            }
        } else {
            remaining.push(task);
        }
    }
    tasks.0 = remaining;
}

/// Applies one completed backend operation and schedules durable recovery when needed.
fn apply_output(
    output: BackendOutput,
    runtime: &mut ClientRuntime,
    session: &mut MultiplayerSession,
    form: &mut MultiplayerForm,
    pending: &mut PendingTurnCommands,
    next_state: &mut NextState<AppState>,
    gameplay_visible: bool,
) {
    if matches!(output, BackendOutput::DepartureFinished) {
        return;
    }
    session.busy = false;
    match output {
        BackendOutput::Initialized {
            backend,
            session: auth,
            games,
            mock_backend,
            configuration_notice,
            realtime_config,
        } => {
            runtime.backend = Some(backend);
            runtime.realtime_config = realtime_config;
            runtime.profile.session = Some(auth.clone());
            if !runtime.profile.display_name.is_empty() {
                form.display_name.clone_from(&runtime.profile.display_name);
                form.saved_display_name = Some(runtime.profile.display_name.clone());
            }
            session.auth = Some(auth);
            session.games =
                games.into_iter().filter(|game| game.status != MatchStatus::Lobby).collect();
            runtime
                .profile
                .recent_games
                .retain(|id| session.games.iter().any(|game| &game.id == id));
            session.mock_backend = mock_backend;
            session.connection = ConnectionStatus::Connected;
            session.notice = configuration_notice;
        },
        BackendOutput::Membership {
            operation,
            result,
            color_notice,
        } => {
            let reconnected = matches!(result.disposition, JoinDisposition::Reconnected);
            form.display_name.clone_from(&result.membership.display_name);
            form.saved_display_name = Some(result.membership.display_name.clone());
            runtime.profile.display_name.clone_from(&result.membership.display_name);
            form.game_code = result.game.code.0.clone();
            session.issued_recovery_code = Some(result.recovery_code.clone());
            session.reconnect_lobby = result.game.status == MatchStatus::Active
                && !matches!(operation, Operation::Create);
            install_membership(result, runtime, session, pending, next_state, gameplay_visible);
            session.notice = Some(color_notice.unwrap_or_else(|| match operation {
                Operation::Recover => {
                    form.recovery_code.clear();
                    "Game recovered. Your recovery code remains unchanged.".to_string()
                },
                Operation::Join if reconnected => {
                    "Reconnected to the existing player on this device.".to_string()
                },
                Operation::Join => "Joined game successfully.".to_string(),
                _ => "Game created. Copy both codes before continuing.".to_string(),
            }));
        },
        #[cfg(debug_assertions)]
        BackendOutput::PracticeReady {
            backend,
            result,
            players,
        } => {
            runtime.backend = Some(backend);
            runtime.realtime_config = None;
            session.leave_selected_game();
            runtime.practice_players = players;
            session.auth = runtime.practice_players.first().map(|player| player.auth.clone());
            session.games.clear();
            session.membership = Some(result.membership);
            session.mock_backend = true;
            session.local_practice = true;
            session.connection = ConnectionStatus::Connected;
            session.notice = None;
            install_record(result.game, runtime, session, pending, next_state, gameplay_visible);
        },
        BackendOutput::Games(games) => {
            session.games =
                games.into_iter().filter(|game| game.status != MatchStatus::Lobby).collect();
            session.connection = ConnectionStatus::Connected;
        },
        BackendOutput::ResumeLoaded(record, recovery_code) => {
            session.issued_recovery_code = Some(recovery_code);
            session.reconnect_lobby = record.status == MatchStatus::Active;
            install_record(record, runtime, session, pending, next_state, gameplay_visible);
            session.connection = ConnectionStatus::Connected;
            session.notice = None;
            session.resolving = false;
        },
        BackendOutput::Left(game_id) => {
            if let Some(record) = session.active_game.as_ref().filter(|record| record.id == game_id)
            {
                if record.status == MatchStatus::Lobby {
                    forget_game(runtime, session, &game_id);
                    form.game_code.clear();
                    form.recovery_code.clear();
                }
                session.leave_selected_game();
                session.connection = ConnectionStatus::Connected;
                session.notice = None;
                session.menu_error = None;
                pending.reset(0);
                next_state.set(AppState::MainMenu);
            }
        },
        BackendOutput::Record(operation, record) => {
            #[cfg(debug_assertions)]
            let installed_turn = record.persisted.state.turn;
            #[cfg(debug_assertions)]
            if matches!(operation, Operation::PracticeTurn) {
                let selected_player = session.membership.as_ref().map(|member| member.player_id);
                for player in &mut runtime.practice_players {
                    player.pending.reset(installed_turn);
                    if Some(player.membership.player_id) == selected_player {
                        player.presented_turn = installed_turn;
                    }
                }
                session.submitted_turn = None;
                session.resolve_needed = false;
            }
            if matches!(operation, Operation::ResumeLoad) {
                session.reconnect_lobby = record.status == MatchStatus::Active;
            }
            install_record(record, runtime, session, pending, next_state, gameplay_visible);
            session.connection = ConnectionStatus::Connected;
            session.notice = match operation {
                Operation::Color => None,
                Operation::Save => Some("Game saved successfully.".to_string()),
                Operation::Resolve => {
                    Some("All submissions resolved; the next turn is ready.".to_string())
                },
                #[cfg(debug_assertions)]
                Operation::PracticeTurn => {
                    Some(format!("All practice players advanced to turn {}.", installed_turn))
                },
                _ => None,
            };
            session.resolving = false;
        },
        BackendOutput::Saved(acknowledgement) => {
            let active_id = session.active_game.as_mut().map(|record| {
                record.revision = acknowledgement.revision;
                record.saved_at = acknowledgement.saved_at;
                record.id.clone()
            });
            if let Some(active_id) = active_id {
                if let Some(summary) = session.games.iter_mut().find(|game| game.id == active_id) {
                    summary.revision = acknowledgement.revision;
                    summary.saved_at = acknowledgement.saved_at;
                }
            }
            session.connection = ConnectionStatus::Connected;
            session.notice = Some(GAME_SAVED_NOTICE.to_string());
        },
        BackendOutput::ProtectionChanged(update) => {
            session.protection_update_pending = false;
            if let Some(record) = session.active_game.as_mut() {
                if record.persisted.state.turn == update.turn {
                    match set_protection_permission_immediately(
                        &mut record.persisted.state,
                        update.controller,
                        update.planet_id,
                        update.protector,
                        update.allowed,
                    ) {
                        Ok(_) => record.revision = update.revision,
                        Err(_) => session.reload_needed = true,
                    }
                } else {
                    session.reload_needed = true;
                }
            }
            session.connection = ConnectionStatus::Connected;
            session.notice = None;
        },
        BackendOutput::JointAttackChanged(invitation) => {
            session.joint_attack_update_pending = false;
            if let Some(existing) =
                session.joint_attacks.iter_mut().find(|item| item.id == invitation.id)
            {
                *existing = invitation;
            } else {
                session.joint_attacks.push(invitation);
                session.joint_attacks.sort_by_key(|item| item.id);
            }
            session.connection = ConnectionStatus::Connected;
            session.notice = None;
        },
        BackendOutput::JointAttacksLoaded(invitations) => {
            session.joint_attack_update_pending = false;
            session.joint_attacks = invitations;
            session.connection = ConnectionStatus::Connected;
        },
        BackendOutput::TradeChanged(invitation) => {
            session.trade_update_pending = false;
            let finalized = invitation.finalized;
            if let Some(existing) = session.trades.iter_mut().find(|item| item.id == invitation.id)
            {
                *existing = invitation;
            } else {
                session.trades.push(invitation);
                session.trades.sort_by_key(|item| item.id);
            }
            session.trade_reload_needed |= finalized;
            session.reload_needed |= finalized;
            session.connection = ConnectionStatus::Connected;
            session.notice = None;
        },
        BackendOutput::TradesLoaded(invitations) => {
            session.trade_update_pending = false;
            session.trades = invitations;
            session.connection = ConnectionStatus::Connected;
        },
        BackendOutput::Resumed => {
            session.reconnect_lobby = false;
            session.connection = ConnectionStatus::Connected;
            session.notice = Some("Everyone is connected. Resuming the game…".to_string());
            next_state.set(AppState::LoadingGame);
        },
        BackendOutput::Submitted(turn) => {
            if pending.turn == turn {
                pending.submission = SubmissionState::Accepted;
            }
            session.submitted_turn = Some(turn);
            session.resolve_needed = true;
            session.connection = ConnectionStatus::Connected;
            session.notice = Some(if session.local_practice {
                "Resolving local turn…".to_string()
            } else {
                WAITING_FOR_PLAYERS_NOTICE.to_string()
            });
        },
        BackendOutput::Withdrawn(draft) => {
            if draft.turn == pending.turn {
                // A ready request that never arrived has no server-side orders to restore.
                // The local draft remains the source in that case.
                if !draft.commands.is_empty() || pending.commands.is_empty() {
                    pending.commands = draft.commands;
                }
                pending.commands.append(&mut pending.queued_commands);
                pending.generation = draft.generation;
                pending.submission = SubmissionState::Draft;
                pending.resume_requested = false;
                session.submitted_turn = None;
                session.resolve_needed = false;
                if let (Some(record), Some(member)) =
                    (&mut session.active_game, &session.membership)
                {
                    record.submitted_players.retain(|id| *id != member.player_id);
                }
            }
            session.connection = ConnectionStatus::Connected;
            session.notice = None;
        },
        BackendOutput::DraftLoaded(turn, stored) => {
            if pending.turn == turn {
                let resume_requested = pending.resume_requested;
                pending.reset(turn);
                if let Some(stored) = stored {
                    pending.commands = stored.submission.commands;
                    pending.generation = stored.submission.generation;
                    if stored.ready {
                        pending.submission = SubmissionState::Accepted;
                        pending.resume_requested = resume_requested;
                        session.submitted_turn = Some(turn);
                        session.resolve_needed = true;
                    }
                }
            }
            session.restore_draft_needed = false;
        },
        BackendOutput::Events(batch) => {
            session.restore_draft_needed |= pending.submission == SubmissionState::Loading;
            let mut game_resumed = false;
            let mut state_reload = false;
            let mut roster_refresh = false;
            let local_player = session.membership.as_ref().map(|member| member.player_id);
            if let Some(record) = &mut session.active_game {
                let current_turn = record.persisted.state.turn;
                for event in &batch.events {
                    match event.kind {
                        BackendEventKind::PlayerJoined
                        | BackendEventKind::PlayerRecovered
                        | BackendEventKind::PlayerConnected
                        | BackendEventKind::PlayerDisconnected => roster_refresh = true,
                        BackendEventKind::GameResumed => game_resumed = true,
                        BackendEventKind::TurnSubmitted if event.turn == Some(current_turn) => {
                            if let Some(player_id) = event.player_id {
                                if !record.submitted_players.contains(&player_id) {
                                    record.submitted_players.push(player_id);
                                    record.submitted_players.sort_unstable();
                                }
                            }
                            session.resolve_needed = true;
                        },
                        BackendEventKind::TurnWithdrawn if event.turn == Some(current_turn) => {
                            if let Some(player_id) = event.player_id {
                                record.submitted_players.retain(|id| *id != player_id);
                                if Some(player_id) == local_player {
                                    pending.submission = SubmissionState::Draft;
                                    session.submitted_turn = None;
                                    session.resolve_needed = false;
                                }
                            }
                        },
                        BackendEventKind::ProtectionChanged => {
                            state_reload |=
                                event.revision.is_none_or(|revision| revision > record.revision);
                        },
                        BackendEventKind::JointAttackChanged => {
                            session.joint_attack_reload_needed = true;
                        },
                        BackendEventKind::TradeChanged => {
                            session.trade_reload_needed = true;
                            state_reload |=
                                event.revision.is_some_and(|revision| revision > record.revision);
                        },
                        BackendEventKind::StateChanged
                        | BackendEventKind::GameStarted
                        | BackendEventKind::TurnResolved
                        | BackendEventKind::GameFinished => {
                            state_reload |=
                                event.revision.is_none_or(|revision| revision > record.revision);
                        },
                        BackendEventKind::TurnSubmitted | BackendEventKind::TurnWithdrawn => {},
                    }
                }
            }
            session.event_cursor = batch.cursor;
            session.reload_needed |= state_reload;
            session.presence_needed |= roster_refresh;
            // A submission event can be consumed before the first resolution attempt observes
            // every row. Keep the local submitter eligible to retry on each durable poll, even
            // when the next batch is empty, until a canonical next turn is installed.
            session.resolve_needed |= local_submission_awaits_resolution(session);
            session.connection = ConnectionStatus::Connected;
            if game_resumed && session.reconnect_lobby {
                session.reconnect_lobby = false;
                session.notice = Some("The host resumed the game.".to_string());
                next_state.set(AppState::LoadingGame);
            }
        },
        BackendOutput::ResolutionWaiting => {
            session.resolving = false;
            session.notice = Some(WAITING_FOR_PLAYERS_NOTICE.to_string());
        },
        BackendOutput::SessionRefreshed(auth) => {
            runtime.profile.session = Some(auth.clone());
            session.auth = Some(auth);
            session.connection = ConnectionStatus::Connected;
            session.reload_needed = session.has_active_game();
            session.presence_needed = session.has_active_game();
        },
        BackendOutput::Reauthenticated(auth) => {
            runtime.profile.session = Some(auth.clone());
            session.auth = Some(auth);
            session.games.clear();
            session.connection = ConnectionStatus::Connected;
            session.notice = Some(
                "A new anonymous session is ready. Use the recovery code to reclaim the previous player slot."
                    .to_string(),
            );
        },
        BackendOutput::Presence(members) => {
            let authenticated_user = session.auth.as_ref().map(|auth| auth.user_id.clone());
            let membership = session.active_game.as_mut().and_then(|record| {
                record.members = members;
                authenticated_user
                    .as_ref()
                    .and_then(|user_id| record.membership_for(user_id))
                    .cloned()
            });
            if authenticated_user.is_some() {
                session.membership = membership;
            }
            session.connection = ConnectionStatus::Connected;
        },
        BackendOutput::DepartureFinished => {},
        BackendOutput::Failed(operation, error) => {
            if matches!(operation, Operation::Protection) {
                session.protection_update_pending = false;
            }
            if matches!(operation, Operation::JointAttack) {
                session.joint_attack_update_pending = false;
            }
            if matches!(operation, Operation::Trade) {
                session.trade_update_pending = false;
            }
            if matches!(operation, Operation::Withdraw) {
                pending.resume_requested = false;
                pending.submission = if matches!(
                    error,
                    BackendError::TurnCommitted
                        | BackendError::StaleSubmission { .. }
                        | BackendError::InvalidGameStatus
                ) {
                    session.reload_needed = true;
                    session.resolve_needed = true;
                    SubmissionState::Accepted
                } else {
                    SubmissionState::ResumeRetry
                };
            }
            if matches!(operation, Operation::RestoreDraft) {
                // Retry with the next durable poll, rather than hammering an offline backend.
                session.restore_draft_needed = false;
            }
            if matches!(operation, Operation::Submit) {
                pending.submission = match &error {
                    BackendError::InvalidData(_) => SubmissionState::Draft,
                    BackendError::DuplicateSubmission {
                        ..
                    } => SubmissionState::Accepted,
                    _ => SubmissionState::Retry,
                };
            }
            session.resolving = false;
            session.notice = Some(user_facing_backend_error(operation, &error));
            session.menu_error.clone_from(&session.notice);
            if matches!(operation, Operation::ResumeLoad) {
                session.reconnect_lobby = false;
            }
            #[cfg(debug_assertions)]
            if matches!(operation, Operation::Practice) {
                runtime.practice_return = None;
            }
            #[cfg(debug_assertions)]
            if matches!(operation, Operation::PracticeTurn) {
                for player in &mut runtime.practice_players {
                    if player.pending.submission == SubmissionState::Sending {
                        player.pending.submission = SubmissionState::Retry;
                    }
                }
                if let Some(player_id) = session.membership.as_ref().map(|member| member.player_id)
                {
                    if let Some(player) = runtime
                        .practice_players
                        .iter()
                        .find(|player| player.membership.player_id == player_id)
                    {
                        *pending = player.pending.clone();
                    }
                }
            }
            match error {
                _ if matches!(operation, Operation::Initialize) => {
                    session.connection = ConnectionStatus::Offline;
                },
                BackendError::Conflict {
                    ..
                } => {
                    session.connection = ConnectionStatus::SyncConflict;
                    session.reload_needed = true;
                },
                BackendError::GameNotFound
                    if matches!(
                        operation,
                        Operation::Load
                            | Operation::Events
                            | Operation::Presence
                            | Operation::Color
                            | Operation::Protection
                            | Operation::JointAttack
                            | Operation::Start
                            | Operation::Resume
                            | Operation::Save
                            | Operation::Submit
                            | Operation::Withdraw
                            | Operation::RestoreDraft
                            | Operation::Resolve
                    ) =>
                {
                    if let Some(record) = &session.active_game {
                        let id = record.id.clone();
                        let was_lobby = record.status == MatchStatus::Lobby;
                        forget_game(runtime, session, &id);
                        session.leave_selected_game();
                        form.game_code.clear();
                        form.recovery_code.clear();
                        pending.reset(0);
                        session.notice = Some(if was_lobby {
                            HOST_CLOSED_LOBBY_NOTICE.to_string()
                        } else {
                            GAME_UNAVAILABLE_NOTICE.to_string()
                        });
                        if was_lobby {
                            session.menu_error = None;
                        } else {
                            session.menu_error.clone_from(&session.notice);
                        }
                        next_state.set(AppState::MainMenu);
                    }
                    session.connection = ConnectionStatus::Connected;
                },
                BackendError::Unauthenticated if matches!(operation, Operation::RefreshAuth) => {
                    if let Some(record) = &session.active_game {
                        form.game_code = record.code.0.clone();
                    }
                    if let Some(code) = &session.issued_recovery_code {
                        form.recovery_code.clone_from(code);
                    }
                    session.leave_selected_game();
                    session.reauthentication_needed = true;
                    session.connection = ConnectionStatus::Reconnecting;
                    session.notice = Some(
                        "This anonymous session expired and could not be renewed. Creating a replacement identity for player recovery."
                            .to_string(),
                    );
                    next_state.set(AppState::RecoverPlayer);
                },
                BackendError::Unauthenticated => {
                    session.auth_refresh_needed = true;
                    session.connection = ConnectionStatus::Reconnecting;
                    session.notice =
                        Some("The session expired; renewing authentication…".to_string());
                },
                BackendError::Offline(_) if matches!(operation, Operation::RefreshAuth) => {
                    session.auth_refresh_needed = true;
                    session.connection = ConnectionStatus::Offline;
                },
                BackendError::Offline(_) if matches!(operation, Operation::Reauthenticate) => {
                    session.reauthentication_needed = true;
                    session.connection = ConnectionStatus::Offline;
                },
                BackendError::Offline(_) => {
                    session.connection = ConnectionStatus::Offline;
                    session.reload_needed = session.has_active_game();
                },
                BackendError::TurnIncomplete if matches!(operation, Operation::Resolve) => {
                    session.connection = ConnectionStatus::Connected;
                },
                _ => session.connection = ConnectionStatus::Connected,
            }
            if matches!(operation, Operation::Initialize) {
                next_state.set(AppState::MainMenu);
            }
        },
        #[cfg(not(target_arch = "wasm32"))]
        BackendOutput::TaskFailed(error) => {
            session.protection_update_pending = false;
            #[cfg(debug_assertions)]
            if session.local_practice {
                for player in &mut runtime.practice_players {
                    if player.pending.submission == SubmissionState::Sending {
                        player.pending.submission = SubmissionState::Retry;
                    }
                }
            }
            if pending.submission == SubmissionState::Sending {
                pending.submission = SubmissionState::Retry;
            }
            if pending.submission == SubmissionState::Resuming {
                pending.submission = SubmissionState::ResumeRetry;
                pending.resume_requested = false;
            }
            session.resolving = false;
            session.connection = ConnectionStatus::Offline;
            session.notice = Some(format!("Background network task failed: {error}"));
            session.menu_error.clone_from(&session.notice);
            if runtime.backend.is_none() {
                next_state.set(AppState::MainMenu);
            }
        },
    }
}

/// Creates a replacement anonymous identity only after renewal is conclusively rejected.
fn drive_reauthentication(
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    if !session.reauthentication_needed || !tasks.0.is_empty() {
        return;
    }
    let Some(backend) = runtime.backend.clone() else {
        return;
    };
    session.reauthentication_needed = false;
    spawn_backend_task(&mut tasks, async move {
        match backend.authenticate(None).await {
            Ok(auth) => BackendOutput::Reauthenticated(auth),
            Err(error) => BackendOutput::Failed(Operation::Reauthenticate, error),
        }
    });
}

/// Refreshes the access token shortly before expiry while retaining the same user identifier.
fn drive_auth_refresh(
    time: Res<Time>,
    mut timer: ResMut<AuthRefreshTimer>,
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    let periodic_check = timer.0.tick(time.delta()).just_finished();
    if (!periodic_check && !session.auth_refresh_needed) || !tasks.0.is_empty() {
        return;
    }
    let (Some(backend), Some(auth)) = (runtime.backend.clone(), session.auth.clone()) else {
        return;
    };
    if !session.auth_refresh_needed {
        let (Some(expires_at), Some(now)) = (auth.expires_at, unix_timestamp()) else {
            return;
        };
        if expires_at > now.saturating_add(5 * 60) {
            return;
        }
    }
    session.auth_refresh_needed = false;
    spawn_backend_task(&mut tasks, async move {
        match backend.refresh_session(&auth).await {
            Ok(refreshed) => BackendOutput::SessionRefreshed(refreshed),
            Err(error) => BackendOutput::Failed(Operation::RefreshAuth, error),
        }
    });
}

#[cfg(not(target_arch = "wasm32"))]
/// Returns current Unix time for native token-expiry checks.
fn unix_timestamp() -> Option<u64> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
}

#[cfg(target_arch = "wasm32")]
/// Returns current browser time for token-expiry checks.
fn unix_timestamp() -> Option<u64> {
    let seconds = js_sys::Date::now() / 1_000.0;
    (seconds.is_finite() && seconds >= 0.0).then_some(seconds as u64)
}

/// Uses authenticated Realtime messages only as low-latency hints for the durable replay path.
fn drive_realtime(
    time: Res<Time>,
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut realtime: NonSendMut<SupabaseRealtimeClient>,
) {
    let game_id = session.active_game.as_ref().map(|record| &record.id);
    let signals = realtime.update(
        time.delta(),
        runtime.realtime_config.as_ref(),
        session.auth.as_ref(),
        game_id,
    );
    for signal in signals {
        match signal {
            RealtimeSignal::Wakeup => {
                session.event_poll_needed = true;
            },
            RealtimeSignal::Connected => {
                session.event_poll_needed = true;
                if matches!(session.connection, ConnectionStatus::Reconnecting) {
                    session.connection = ConnectionStatus::Connected;
                }
            },
            RealtimeSignal::Disconnected(reason) => {
                if session.has_active_game()
                    && !matches!(session.connection, ConnectionStatus::Offline)
                {
                    session.connection = ConnectionStatus::Reconnecting;
                    debug!("Realtime reconnecting ({reason}); durable polling remains active.");
                }
            },
        }
    }
}

/// Installs a create/join/recovery response and persists the identity convenience profile.
fn install_membership(
    result: MembershipResult,
    runtime: &mut ClientRuntime,
    session: &mut MultiplayerSession,
    pending: &mut PendingTurnCommands,
    next_state: &mut NextState<AppState>,
    gameplay_visible: bool,
) {
    session.membership = Some(result.membership);
    install_record(result.game, runtime, session, pending, next_state, gameplay_visible);
}

/// Makes a validated backend record canonical and selects lobby or gameplay loading state.
fn install_record(
    mut record: GameRecord,
    runtime: &mut ClientRuntime,
    session: &mut MultiplayerSession,
    pending: &mut PendingTurnCommands,
    next_state: &mut NextState<AppState>,
    gameplay_visible: bool,
) {
    let newly_selected = session.active_game.as_ref().is_none_or(|game| game.id != record.id);
    let needs_gameplay_install =
        record_requires_gameplay_install(session.active_game.as_ref(), &record);
    if session.membership.as_ref().is_none_or(|membership| membership.game_id != record.id) {
        session.membership =
            session.auth.as_ref().and_then(|auth| record.membership_for(&auth.user_id)).cloned();
    }
    mark_selected_member_connected(session, &mut record);
    sync_game_summary(session, &record);
    if !session.local_practice {
        if record.status == MatchStatus::Lobby {
            runtime.profile.recent_games.retain(|id| id != &record.id);
        } else {
            runtime.profile.remember_game(record.id.clone());
        }
    }
    if needs_gameplay_install {
        pending.reset(record.persisted.state.turn);
        // Recover both ready orders and withdrawn drafts before allowing new commands.
        session.restore_draft_needed = record.status == MatchStatus::Active;
        if session.restore_draft_needed {
            pending.submission = SubmissionState::Loading;
        }
    }
    // Same-turn refreshes can predate a readiness change. Only its ordered write
    // response (or the initial draft restore) may change the local draft state.
    let status = record.status;
    session.active_game = Some(record);
    session.joint_attack_reload_needed = true;
    session.trade_reload_needed = true;
    session.reload_needed = false;
    // Refreshing presence must not immediately schedule another heartbeat/reload pair.
    session.presence_needed |= newly_selected && !session.local_practice;
    if let Some(destination) = record_destination(
        status,
        session.reconnect_lobby,
        needs_gameplay_install,
        gameplay_visible,
    ) {
        next_state.set(destination);
    }
    if status != MatchStatus::Lobby && needs_gameplay_install {
        session.submitted_turn = None;
    }
}

/// Optimistically reflects the local client while its authoritative presence request is in flight.
fn mark_selected_member_connected(session: &mut MultiplayerSession, record: &mut GameRecord) {
    let Some(selected) = session.membership.as_mut() else {
        return;
    };
    selected.connected = true;
    if let Some(member) =
        record.members.iter_mut().find(|member| member.player_id == selected.player_id)
    {
        member.connected = true;
    }
}

/// Removes an unavailable game from the local list and convenience profile.
fn forget_game(runtime: &mut ClientRuntime, session: &mut MultiplayerSession, game_id: &GameId) {
    session.games.retain(|game| &game.id != game_id);
    runtime.profile.recent_games.retain(|id| id != game_id);
}

/// Keeps started matches available on the resume screen without retaining live lobbies.
fn sync_game_summary(session: &mut MultiplayerSession, record: &GameRecord) {
    if record.status == MatchStatus::Lobby {
        session.games.retain(|game| game.id != record.id);
        return;
    }
    let Some(membership) = &session.membership else {
        return;
    };
    let Some(recovery_code) = &session.issued_recovery_code else {
        return;
    };
    let Ok(player) = record.persisted.state.player(membership.player_id) else {
        return;
    };
    let summary = GameSummary {
        id: record.id.clone(),
        code: record.code.clone(),
        revision: record.revision,
        saved_at: record.saved_at,
        status: record.status,
        turn: record.persisted.state.turn,
        player_id: membership.player_id,
        display_name: membership.display_name.clone(),
        recovery_code: recovery_code.clone(),
        player_color: player.color(),
        player_count: record.members.len(),
        max_players: record.max_players,
    };
    if let Some(existing) = session.games.iter_mut().find(|game| game.id == record.id) {
        *existing = summary;
    } else {
        session.games.insert(0, summary);
    }
}

/// Chooses the user-visible destination for a loaded record by its lifecycle.
fn record_destination(
    status: MatchStatus,
    reconnect_lobby: bool,
    needs_gameplay_install: bool,
    gameplay_visible: bool,
) -> Option<AppState> {
    match status {
        MatchStatus::Lobby => Some(AppState::Lobby),
        MatchStatus::Active if reconnect_lobby => Some(AppState::Lobby),
        MatchStatus::Active | MatchStatus::Finished
            if needs_gameplay_install && !gameplay_visible =>
        {
            Some(AppState::LoadingGame)
        },
        MatchStatus::Active | MatchStatus::Finished => None,
    }
}

/// Returns whether a backend record represents a new projection rather than a same-turn refresh.
fn record_requires_gameplay_install(previous: Option<&GameRecord>, next: &GameRecord) -> bool {
    previous.is_none_or(|previous| {
        previous.id != next.id
            || previous.status != next.status
            || previous.persisted.state.turn != next.persisted.state.turn
    })
}

/// Recovers saved orders or clears readiness after a gameplay action or Continue turn interaction.
/// Serialize this with ready writes so their payload stays fixed until delivery completes.
fn drive_turn_draft(
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut pending: ResMut<PendingTurnCommands>,
    mut tasks: ResMut<BackendTasks>,
) {
    if !tasks.0.is_empty() || (!session.restore_draft_needed && !pending.resume_requested) {
        return;
    }
    let (Some(backend), Some(auth), Some(record), Some(member)) = (
        runtime.backend.clone(),
        session.auth.clone(),
        session.active_game.as_ref(),
        session.membership.as_ref(),
    ) else {
        return;
    };
    if record.status != MatchStatus::Active || pending.turn != record.persisted.state.turn {
        return;
    }
    let restoring = session.restore_draft_needed;
    let game_id = record.id.clone();
    let player_id = member.player_id;
    session.restore_draft_needed = false;
    let turn = pending.turn;
    if !restoring {
        pending.submission = SubmissionState::Resuming;
    }
    spawn_backend_task(&mut tasks, async move {
        let operation = if restoring {
            Operation::RestoreDraft
        } else {
            Operation::Withdraw
        };
        let stored = match backend.load_turn_submissions(&auth, &game_id, turn).await {
            Ok(submissions) => {
                submissions.into_iter().find(|s| s.submission.player_id == player_id)
            },
            Err(error) => return BackendOutput::Failed(operation, error),
        };
        if restoring {
            return BackendOutput::DraftLoaded(turn, stored);
        }
        let generation = stored.as_ref().map_or(0, |s| s.submission.generation);
        match backend.withdraw_turn(&auth, &game_id, turn, generation).await {
            Ok(draft) => BackendOutput::Withdrawn(draft),
            Err(error) => BackendOutput::Failed(Operation::Withdraw, error),
        }
    });
}

/// Returns whether this client is ready for the canonical turn that is still active.
fn local_submission_awaits_resolution(session: &MultiplayerSession) -> bool {
    session.active_game.as_ref().is_some_and(|record| {
        record.status == MatchStatus::Active
            && session.submitted_turn == Some(record.persisted.state.turn)
    })
}

/// Polls durable events periodically so missed/disconnected Realtime notifications are harmless.
fn poll_durable_events(
    time: Res<Time<Real>>,
    mut timer: ResMut<EventPollTimer>,
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    realtime: NonSend<SupabaseRealtimeClient>,
    mut tasks: ResMut<BackendTasks>,
) {
    let interval = if runtime.realtime_config.is_some() && realtime.is_connected() {
        EVENT_POLL_INTERVAL_CONNECTED
    } else {
        EVENT_POLL_INTERVAL_FALLBACK
    };
    if timer.0.duration() != interval {
        timer.0.set_duration(interval);
        timer.0.reset();
    }
    let periodically_due = timer.0.tick(time.delta()).just_finished();
    if !session.has_active_game() || session.local_practice {
        session.event_poll_needed = false;
        timer.0.reset();
        return;
    }
    if (!periodically_due && !session.event_poll_needed) || !tasks.0.is_empty() {
        return;
    }
    let (Some(backend), Some(auth), Some(game_id)) = (
        runtime.backend.clone(),
        session.auth.clone(),
        session.active_game.as_ref().map(|r| r.id.clone()),
    ) else {
        return;
    };
    let cursor = session.event_cursor;
    session.event_poll_needed = false;
    timer.0.reset();
    spawn_backend_task(&mut tasks, async move {
        match backend.subscribe(&auth, &game_id, cursor).await {
            Ok(batch) => BackendOutput::Events(batch),
            Err(error) => BackendOutput::Failed(Operation::Events, error),
        }
    });
}

/// Renews presence while a game is open so recovery can distinguish live and abandoned clients.
fn drive_presence(
    time: Res<Time<Real>>,
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    if !session.has_active_game() || session.local_practice {
        session.presence_elapsed = Duration::ZERO;
        return;
    }
    session.presence_elapsed = session.presence_elapsed.saturating_add(time.delta());
    if (!session.presence_needed && session.presence_elapsed < PRESENCE_HEARTBEAT_INTERVAL)
        || !tasks.0.is_empty()
    {
        return;
    }
    let (Some(backend), Some(auth), Some(game_id)) = (
        runtime.backend.clone(),
        session.auth.clone(),
        session.active_game.as_ref().map(|r| r.id.clone()),
    ) else {
        return;
    };
    session.presence_needed = false;
    session.presence_elapsed = Duration::ZERO;
    spawn_backend_task(&mut tasks, async move {
        match backend.set_connected(&auth, &game_id, true).await {
            Ok(members) => BackendOutput::Presence(members),
            Err(error) => BackendOutput::Failed(Operation::Presence, error),
        }
    });
}

/// Reloads the current record after events, reconnects, or optimistic-concurrency conflicts.
fn drive_reload(
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    if !session.reload_needed || !tasks.0.is_empty() {
        return;
    }
    let (Some(backend), Some(auth), Some(game_id)) = (
        runtime.backend.clone(),
        session.auth.clone(),
        session.active_game.as_ref().map(|r| r.id.clone()),
    ) else {
        return;
    };
    session.reload_needed = false;
    spawn_backend_task(&mut tasks, async move {
        match backend.load_game(&auth, &game_id).await {
            Ok(record) => BackendOutput::Record(Operation::Load, record),
            Err(error) => BackendOutput::Failed(Operation::Load, error),
        }
    });
}

/// Loads only private attack invitations after record installation or a durable wake-up.
fn drive_joint_attack_reload(
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    if !session.joint_attack_reload_needed
        || session.joint_attack_update_pending
        || !tasks.0.is_empty()
    {
        return;
    }
    let (Some(backend), Some(auth), Some(game_id)) = (
        runtime.backend.clone(),
        session.auth.clone(),
        session.active_game.as_ref().map(|record| record.id.clone()),
    ) else {
        return;
    };
    session.joint_attack_reload_needed = false;
    session.joint_attack_update_pending = true;
    spawn_backend_task(&mut tasks, async move {
        match backend.load_joint_attacks(&auth, &game_id).await {
            Ok(invitations) => BackendOutput::JointAttacksLoaded(invitations),
            Err(error) => BackendOutput::Failed(Operation::JointAttack, error),
        }
    });
}

/// Loads only private Trading Post negotiations after record installation or a durable wake-up.
fn drive_trade_reload(
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    if !session.trade_reload_needed || session.trade_update_pending || !tasks.0.is_empty() {
        return;
    }
    let (Some(backend), Some(auth), Some(game_id)) = (
        runtime.backend.clone(),
        session.auth.clone(),
        session.active_game.as_ref().map(|record| record.id.clone()),
    ) else {
        return;
    };
    session.trade_reload_needed = false;
    session.trade_update_pending = true;
    spawn_backend_task(&mut tasks, async move {
        match backend.load_trades(&auth, &game_id).await {
            Ok(invitations) => BackendOutput::TradesLoaded(invitations),
            Err(error) => BackendOutput::Failed(Operation::Trade, error),
        }
    });
}

/// Attempts deterministic resolution after submission events; compare-and-swap accepts one winner.
fn drive_resolution(
    runtime: Res<ClientRuntime>,
    pending: Res<PendingTurnCommands>,
    mut session: ResMut<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    if pending.resume_requested
        || !pending.queued_commands.is_empty()
        || !session.resolve_needed
        || session.reconnect_lobby
        || session.resolving
        || !tasks.0.is_empty()
    {
        return;
    }
    let (Some(backend), Some(auth), Some(record)) =
        (runtime.backend.clone(), session.auth.clone(), session.active_game.clone())
    else {
        return;
    };
    if record.status != crate::core::simulation::MatchStatus::Active {
        session.resolve_needed = false;
        return;
    }
    session.resolve_needed = false;
    session.resolving = true;
    spawn_backend_task(&mut tasks, async move {
        let turn = record.persisted.state.turn;
        let submissions = match backend.load_turn_submissions(&auth, &record.id, turn).await {
            Ok(submissions) => {
                submissions.into_iter().filter(|stored| stored.ready).collect::<Vec<_>>()
            },
            Err(error) => return BackendOutput::Failed(Operation::Resolve, error),
        };
        let required =
            record.persisted.state.players.iter().filter(|player| !player.spectator).count();
        if submissions.len() != required {
            return BackendOutput::ResolutionWaiting;
        }
        let commands = submissions.into_iter().map(|stored| stored.submission).collect::<Vec<_>>();
        let model = match resolved_turn(&record.persisted.state, &commands) {
            Ok((model, _)) => model,
            Err(error) => {
                return BackendOutput::Failed(
                    Operation::Resolve,
                    BackendError::InvalidData(error.to_string()),
                )
            },
        };
        match backend
            .publish_resolution(&auth, &record.id, record.revision, turn, PersistedGame::new(model))
            .await
        {
            Ok(record) => BackendOutput::Record(Operation::Resolve, record),
            Err(error) => BackendOutput::Failed(Operation::Resolve, error),
        }
    });
}

/// Starts a resumable game-list request when no selected record exists.
fn spawn_list(runtime: &ClientRuntime, session: &MultiplayerSession, tasks: &mut BackendTasks) {
    let (Some(backend), Some(auth)) = (runtime.backend.clone(), session.auth.clone()) else {
        return;
    };
    spawn_backend_task(tasks, async move {
        match backend.list_games(&auth).await {
            Ok(games) => BackendOutput::Games(games),
            Err(error) => BackendOutput::Failed(Operation::List, error),
        }
    });
}

/// Resolves a normalized game code to the same backend entry used by Resume Game.
fn linked_game<'a>(games: &'a [GameSummary], code: &GameCode) -> Option<&'a GameSummary> {
    games.iter().find(|game| &game.code == code)
}

/// Loads one already-linked game through the shared Resume Game result path.
async fn load_game_for_resume(
    backend: Arc<dyn MultiplayerBackend>,
    auth: AuthSession,
    game_id: GameId,
    recovery_code: String,
) -> BackendOutput {
    match backend.load_game(&auth, &game_id).await {
        Ok(record) => BackendOutput::ResumeLoaded(record, recovery_code),
        Err(error) => BackendOutput::Failed(Operation::ResumeLoad, error),
    }
}

/// Recovers an unlinked slot, or opens the current membership if recovery is redundant.
async fn recover_or_resume_linked_game(
    backend: Arc<dyn MultiplayerBackend>,
    auth: AuthSession,
    request: RecoverPlayerRequest,
) -> BackendOutput {
    let code = request.code.clone();
    match backend.recover_player(&auth, request).await {
        Ok(result) => BackendOutput::Membership {
            operation: Operation::Recover,
            result,
            color_notice: None,
        },
        Err(BackendError::AlreadyMember) => {
            let games = match backend.list_games(&auth).await {
                Ok(games) => games,
                Err(error) => return BackendOutput::Failed(Operation::Recover, error),
            };
            let Some(summary) = linked_game(&games, &code) else {
                // Lobbies and expired matches are intentionally absent from Resume Game.
                return BackendOutput::Failed(Operation::Recover, BackendError::GameNotFound);
            };
            load_game_for_resume(backend, auth, summary.id.clone(), summary.recovery_code.clone())
                .await
        },
        Err(error) => BackendOutput::Failed(Operation::Recover, error),
    }
}

/// Spawns a Send task natively and a browser-local task on WebAssembly.
#[cfg(not(target_arch = "wasm32"))]
fn spawn_backend_task(
    tasks: &mut BackendTasks,
    future: impl Future<Output = BackendOutput> + Send + 'static,
) {
    tasks.0.push(IoTaskPool::get().spawn(async move {
        let runtime = match native_backend_runtime() {
            Ok(runtime) => runtime,
            Err(error) => return BackendOutput::TaskFailed(error),
        };
        match runtime.spawn(future).await {
            Ok(output) => output,
            Err(error) => BackendOutput::TaskFailed(error.to_string()),
        }
    }));
}

/// Supplies native HTTP futures with the Tokio reactor required by reqwest.
#[cfg(not(target_arch = "wasm32"))]
fn native_backend_runtime() -> Result<&'static tokio::runtime::Runtime, String> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    match RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("stellarion-network")
            .enable_all()
            .build()
            .map_err(|error| error.to_string())
    }) {
        Ok(runtime) => Ok(runtime),
        Err(error) => Err(error.clone()),
    }
}

/// Spawns a browser-local future without imposing a native-only `Send` bound.
#[cfg(target_arch = "wasm32")]
fn spawn_backend_task(
    tasks: &mut BackendTasks,
    future: impl Future<Output = BackendOutput> + 'static,
) {
    tasks.0.push(IoTaskPool::get().spawn_local(future));
}

#[cfg(test)]
#[path = "../../tests/multiplayer/client.rs"]
pub(crate) mod tests;
