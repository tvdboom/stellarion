//! Bevy adapter that turns menu/gameplay intent into asynchronous backend operations.

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

#[cfg(not(target_arch = "wasm32"))]
use std::sync::OnceLock;

use bevy::prelude::*;
use bevy::tasks::{IoTaskPool, Task};
use futures_lite::future::{block_on, poll_once};

use crate::core::identity::{GameCode, GameId};
use crate::core::player::PlayerColor;
use crate::core::simulation::{
    resolve_turn, GameModel, GameRules, MatchStatus, PersistedGame, TurnCommand, TurnSubmission,
    MAX_COMMANDS_PER_SUBMISSION, PLAYER_COUNT_RANGE,
};
use crate::core::states::AppState;
use crate::multiplayer::backend::{BackendError, MultiplayerBackend};
use crate::multiplayer::memory::InMemoryBackend;
use crate::multiplayer::model::{
    AuthSession, BackendEventKind, CreateGameRequest, EventBatch, GameMembership, GameRecord,
    GameSummary, JoinDisposition, JoinGameRequest, MembershipResult, RecoverPlayerRequest,
    SubmissionDisposition,
};
use crate::multiplayer::realtime::{RealtimeSignal, SupabaseRealtimeClient};
use crate::multiplayer::recovery::{generate_game_code, RecoveryCode};
use crate::multiplayer::supabase::SupabaseBackend;
use crate::platform::config::{ConfigError, SupabaseConfig};
use crate::platform::storage::{
    load_profile, save_profile, ClientProfile, ClientStorage, MemoryStorage,
};

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

/// Editable values shared by the multiplayer menu screens.
#[derive(Resource)]
pub struct MultiplayerForm {
    /// Lobby display name, persisted locally for convenience.
    pub display_name: String,
    /// Six-character game code entered by a joining player.
    pub game_code: String,
    /// High-entropy recovery code entered on a replacement device.
    pub recovery_code: String,
}

impl Default for MultiplayerForm {
    /// Uses a valid two-player form with no secrets filled in.
    fn default() -> Self {
        Self {
            display_name: "Commander".to_string(),
            game_code: String::new(),
            recovery_code: String::new(),
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
    /// Stable slot for the current identity in the selected game.
    pub membership: Option<GameMembership>,
    /// Newly issued plaintext recovery code, retained only on this client for display.
    pub issued_recovery_code: Option<String>,
    /// Last durable notification sequence applied for the selected game.
    pub event_cursor: u64,
    /// Transport condition rendered in the menu and HUD.
    pub connection: ConnectionStatus,
    /// Human-readable result or failure from the latest operation.
    pub notice: Option<String>,
    /// Actionable failure displayed inline in menus, never as a toast.
    pub menu_error: Option<String>,
    /// Whether local development is using the credential-free backend.
    pub mock_backend: bool,
    /// Whether the selected record is an isolated debug-only one-player match.
    pub local_practice: bool,
    /// Whether a foreground menu operation is still running.
    pub busy: bool,
    /// Whether an active match is paused in its all-players reconnection lobby.
    pub reconnect_lobby: bool,
    reload_needed: bool,
    resolve_needed: bool,
    resolving: bool,
    submitted_turn: Option<u64>,
    presence_needed: bool,
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

    /// Clears selection data while retaining authentication and resumable games.
    pub fn leave_selected_game(&mut self) {
        self.active_game = None;
        self.membership = None;
        self.issued_recovery_code = None;
        self.event_cursor = 0;
        self.reload_needed = false;
        self.resolve_needed = false;
        self.resolving = false;
        self.submitted_turn = None;
        self.presence_needed = false;
        self.reconnect_lobby = false;
        self.reauthentication_needed = false;
        self.local_practice = false;
    }
}

/// Commands accumulated by the Bevy UI for the current simultaneous turn.
#[derive(Resource, Clone, Default)]
pub struct PendingTurnCommands {
    /// Turn for which commands are being collected.
    pub turn: u64,
    /// Intentional commands in local interaction order.
    pub commands: Vec<TurnCommand>,
}

impl PendingTurnCommands {
    /// Resets the draft when a canonical next turn is installed.
    pub fn reset(&mut self, turn: u64) {
        self.turn = turn;
        self.commands.clear();
    }

    /// Appends one gameplay intent, returning false once the bounded draft is full.
    pub fn push(&mut self, command: TurnCommand) -> bool {
        if self.commands.len() >= MAX_COMMANDS_PER_SUBMISSION {
            return false;
        }
        self.commands.push(command);
        true
    }
}

/// Foreground operations requested by menu buttons or gameplay UI.
#[derive(Message)]
pub enum MultiplayerRequest {
    /// Creates and starts an isolated one-player match without contacting Supabase.
    #[cfg(debug_assertions)]
    StartLocalPractice {
        /// Deterministic rules selected in the local-practice setup screen.
        rules: GameRules,
    },
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
    /// Replaces a lost anonymous identity using a one-time recovery code.
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
    /// Saves the latest canonical snapshot from any member.
    SaveGame,
    /// Commits the local player's accumulated commands for this turn.
    SubmitTurn,
    /// Leaves the selected game locally and returns to the multiplayer menu.
    LeaveGame,
    /// Retries state/event synchronization after an offline error.
    Retry,
}

#[derive(Message)]
/// Requests replacement of an already-visible gameplay turn without a menu transition.
pub(crate) struct RefreshGameplayProjection;

/// Runtime backend and local profile storage hidden from gameplay systems.
#[derive(Resource)]
struct ClientRuntime {
    backend: Option<Arc<dyn MultiplayerBackend>>,
    realtime_config: Option<SupabaseConfig>,
    storage: Arc<dyn ClientStorage>,
    profile: ClientProfile,
    practice_return: Option<PracticeReturn>,
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
    Save,
    Submit,
    Events,
    Resolve,
    Presence,
    #[cfg(debug_assertions)]
    Practice,
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
        recovery_code: RecoveryCode,
    },
    #[cfg(debug_assertions)]
    PracticeReady {
        backend: Arc<dyn MultiplayerBackend>,
        auth: AuthSession,
        result: MembershipResult,
    },
    Games(Vec<GameSummary>),
    Record(Operation, GameRecord),
    Resumed,
    Submitted(SubmissionDisposition, u64),
    Events(EventBatch),
    ResolutionWaiting,
    SessionRefreshed(AuthSession),
    Reauthenticated(AuthSession),
    Presence,
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
            .insert_resource(EventPollTimer(Timer::new(
                Duration::from_secs(2),
                TimerMode::Repeating,
            )))
            .insert_resource(AuthRefreshTimer(Timer::new(
                Duration::from_secs(30),
                TimerMode::Repeating,
            )))
            .insert_non_send(SupabaseRealtimeClient::default())
            .add_message::<MultiplayerRequest>()
            .add_message::<RefreshGameplayProjection>()
            .add_systems(Startup, initialize_client)
            .add_systems(
                Update,
                (
                    process_requests,
                    poll_backend_tasks,
                    drive_reauthentication,
                    drive_auth_refresh,
                    drive_realtime,
                    poll_durable_events,
                    drive_reload,
                    drive_resolution,
                    drive_presence,
                    update_connection_indicator,
                )
                    .chain(),
            );
    }
}

/// Creates platform storage and starts anonymous authentication asynchronously.
fn initialize_client(mut commands: Commands, mut tasks: ResMut<BackendTasks>) {
    let storage = platform_storage();
    let profile = load_profile(storage.as_ref()).unwrap_or_default();
    let stored_session = profile.session.clone();
    commands.insert_resource(ClientRuntime {
        backend: None,
        realtime_config: None,
        storage,
        profile,
        practice_return: None,
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

/// Creates a complete one-player match on an isolated in-memory backend.
#[cfg(debug_assertions)]
async fn create_local_practice(
    rules: GameRules,
) -> Result<(Arc<dyn MultiplayerBackend>, AuthSession, MembershipResult), BackendError> {
    let backend = Arc::new(InMemoryBackend::new());
    let auth = backend.authenticate(None).await?;
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed).map_err(|error| BackendError::Protocol(error.to_string()))?;
    let recovery =
        RecoveryCode::generate().map_err(|error| BackendError::Protocol(error.to_string()))?;
    let model = GameModel::new(seed, rules)
        .map_err(|error| BackendError::InvalidData(error.to_string()))?;
    let mut result = backend
        .create_game(
            &auth,
            CreateGameRequest {
                code: generate_game_code()
                    .map_err(|error| BackendError::Protocol(error.to_string()))?,
                display_name: "Practice Player".to_string(),
                recovery_hash: recovery.hash().0,
                persisted: PersistedGame::new(model),
            },
        )
        .await?;
    let mut started = result.game.persisted.clone();
    started.state.start().map_err(|error| BackendError::InvalidData(error.to_string()))?;
    result.game = backend.start_game(&auth, &result.game.id, result.game.revision, started).await?;
    Ok((backend, auth, result))
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
    mut session: ResMut<MultiplayerSession>,
    mut runtime: ResMut<ClientRuntime>,
    pending: Res<PendingTurnCommands>,
    mut tasks: ResMut<BackendTasks>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for request in requests.read() {
        session.menu_error = None;
        if matches!(request, MultiplayerRequest::LeaveGame) {
            if session.local_practice {
                tasks.0.clear();
                session.leave_selected_game();
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
                continue;
            }
            if let (Some(backend), Some(auth), Some(record)) =
                (runtime.backend.clone(), session.auth.clone(), session.active_game.clone())
            {
                spawn_backend_task(&mut tasks, async move {
                    match backend.set_connected(&auth, &record.id, false).await {
                        Ok(()) => BackendOutput::Presence,
                        Err(error) => BackendOutput::Failed(Operation::Presence, error),
                    }
                });
            }
            session.leave_selected_game();
            next_state.set(AppState::MultiPlayerMenu);
            continue;
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
                    Ok((backend, auth, result)) => BackendOutput::PracticeReady {
                        backend,
                        auth,
                        result,
                    },
                    Err(error) => BackendOutput::Failed(Operation::Practice, error),
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
            request_error(&mut session, "Authentication is still initializing.");
            continue;
        };
        session.busy = true;
        session.notice = None;

        match request {
            #[cfg(debug_assertions)]
            MultiplayerRequest::StartLocalPractice {
                ..
            } => unreachable!("local practice is handled before online authentication"),
            MultiplayerRequest::CreateGame {
                display_name,
                rules,
            } => {
                let display_name = display_name.trim().to_string();
                runtime.profile.display_name.clone_from(&display_name);
                let _ = save_profile(runtime.storage.as_ref(), &runtime.profile);
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
                                    recovery_hash: recovery.hash().0,
                                    persisted: PersistedGame::new(model.clone()),
                                },
                            )
                            .await;
                        match result {
                            Ok(result) => {
                                return BackendOutput::Membership {
                                    operation: Operation::Create,
                                    result,
                                    recovery_code: recovery,
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
                let _ = save_profile(runtime.storage.as_ref(), &runtime.profile);
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
                    recovery_hash: recovery.hash().0,
                };
                spawn_backend_task(&mut tasks, async move {
                    match backend.join_game(&auth, request).await {
                        Ok(result) => BackendOutput::Membership {
                            operation: Operation::Join,
                            result,
                            recovery_code: recovery,
                        },
                        Err(error) => BackendOutput::Failed(Operation::Join, error),
                    }
                });
            },
            MultiplayerRequest::RecoverPlayer {
                code,
                recovery_code,
            } => {
                let supplied = match RecoveryCode::parse(recovery_code) {
                    Ok(code) => code,
                    Err(error) => {
                        request_error(&mut session, &error.to_string());
                        continue;
                    },
                };
                let replacement = match RecoveryCode::generate() {
                    Ok(code) => code,
                    Err(error) => {
                        request_error(&mut session, &error.to_string());
                        continue;
                    },
                };
                let request = RecoverPlayerRequest {
                    code: GameCode::new(code),
                    recovery_hash: supplied.hash().0,
                    replacement_recovery_hash: replacement.hash().0,
                };
                spawn_backend_task(&mut tasks, async move {
                    match backend.recover_player(&auth, request).await {
                        Ok(result) => BackendOutput::Membership {
                            operation: Operation::Recover,
                            result,
                            recovery_code: replacement,
                        },
                        Err(error) => BackendOutput::Failed(Operation::Recover, error),
                    }
                });
            },
            MultiplayerRequest::ResumeGame(game_id) => {
                let game_id = game_id.clone();
                spawn_backend_task(&mut tasks, async move {
                    match backend.load_game(&auth, &game_id).await {
                        Ok(record) => BackendOutput::Record(Operation::ResumeLoad, record),
                        Err(error) => BackendOutput::Failed(Operation::ResumeLoad, error),
                    }
                });
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
                let (Some(record), Some(membership)) =
                    (session.active_game.clone(), session.membership.clone())
                else {
                    request_error(&mut session, "No lobby player is selected.");
                    continue;
                };
                let persisted =
                    match recolored_lobby_snapshot(&record, membership.player_id, *color) {
                        Ok(persisted) => persisted,
                        Err(error) => {
                            request_error(&mut session, &error.to_string());
                            continue;
                        },
                    };
                spawn_backend_task(&mut tasks, async move {
                    match backend.save_game(&auth, &record.id, record.revision, persisted).await {
                        Ok(record) => BackendOutput::Record(Operation::Color, record),
                        Err(error) => BackendOutput::Failed(Operation::Color, error),
                    }
                });
            },
            MultiplayerRequest::SaveGame => {
                let Some(record) = session.active_game.clone() else {
                    request_error(&mut session, "No game is selected.");
                    continue;
                };
                spawn_backend_task(&mut tasks, async move {
                    match backend
                        .save_game(&auth, &record.id, record.revision, record.persisted.clone())
                        .await
                    {
                        Ok(record) => BackendOutput::Record(Operation::Save, record),
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
                let submission = TurnSubmission::new(
                    membership.player_id,
                    pending.turn,
                    pending.commands.clone(),
                );
                let submitted_turn = submission.turn;
                spawn_backend_task(&mut tasks, async move {
                    match backend.submit_turn(&auth, &record.id, submission).await {
                        Ok(disposition) => BackendOutput::Submitted(disposition, submitted_turn),
                        Err(error) => BackendOutput::Failed(Operation::Submit, error),
                    }
                });
            },
            MultiplayerRequest::LeaveGame | MultiplayerRequest::Retry => {},
        }
    }
}

/// Applies one member's lobby color while preventing indistinguishable active members.
fn recolored_lobby_snapshot(
    record: &GameRecord,
    player_id: u64,
    color: PlayerColor,
) -> Result<PersistedGame, BackendError> {
    if record.status != MatchStatus::Lobby || !color.is_valid() {
        return Err(BackendError::InvalidGameStatus);
    }
    if !record.members.iter().any(|member| member.player_id == player_id) {
        return Err(BackendError::PlayerNoLongerInGame);
    }
    if record.members.iter().any(|member| {
        member.player_id != player_id
            && record
                .persisted
                .state
                .player(member.player_id)
                .is_ok_and(|player| player.color() == color)
    }) {
        return Err(BackendError::InvalidData(
            "That color is already selected by another player.".to_string(),
        ));
    }

    let mut persisted = record.persisted.clone();
    let previous = persisted
        .state
        .player(player_id)
        .map_err(|error| BackendError::InvalidData(error.to_string()))?
        .color();
    let displaced_player = persisted
        .state
        .players
        .iter()
        .find(|player| player.id != player_id && player.color() == color)
        .map(|player| player.id);
    if let Some(displaced_player) = displaced_player {
        persisted
            .state
            .player_mut(displaced_player)
            .map_err(|error| BackendError::InvalidData(error.to_string()))?
            .color = Some(previous);
    }
    persisted
        .state
        .player_mut(player_id)
        .map_err(|error| BackendError::InvalidData(error.to_string()))?
        .color = Some(color);
    persisted.validate().map_err(|error| BackendError::InvalidData(error.to_string()))?;
    Ok(persisted)
}

/// Generates the real opening map once the creator chooses the lobby's final member count.
fn started_snapshot_for_members(
    record: &GameRecord,
    seed: [u8; 32],
) -> Result<PersistedGame, BackendError> {
    let player_count = u8::try_from(record.members.len())
        .map_err(|_| BackendError::InvalidData("too many lobby members".to_string()))?;
    if !PLAYER_COUNT_RANGE.contains(&player_count) || player_count > record.max_players {
        return Err(BackendError::InvalidGameStatus);
    }

    let mut rules = record.persisted.state.rules.clone();
    rules.player_count = player_count;
    let mut model = GameModel::new(seed, rules)
        .map_err(|error| BackendError::InvalidData(error.to_string()))?;
    for member in &record.members {
        let color = record
            .persisted
            .state
            .player(member.player_id)
            .map_err(|error| BackendError::InvalidData(error.to_string()))?
            .color();
        model
            .player_mut(member.player_id)
            .map_err(|error| BackendError::InvalidData(error.to_string()))?
            .color = Some(color);
    }
    model.start().map_err(|error| BackendError::InvalidData(error.to_string()))?;
    Ok(PersistedGame::new(model))
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
            "On a new device, choose Recover Player and enter your recovery code."
        )
        .to_string(),
        (Operation::Start, BackendError::InvalidGameStatus) => {
            "At least two players must be in the lobby before the host can start the game."
                .to_string()
        },
        (Operation::Resume, BackendError::InvalidGameStatus) => {
            "Every player must reconnect before the host can resume this game.".to_string()
        },
        (_, BackendError::InvalidGameStatus) => {
            "This action is not available for the game right now. Refresh the game and try again."
                .to_string()
        },
        _ => error.to_string(),
    }
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
) {
    let mut remaining = Vec::with_capacity(tasks.0.len());
    for mut task in std::mem::take(&mut tasks.0) {
        if let Some(output) = block_on(poll_once(&mut task)) {
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
                refresh_gameplay.write(RefreshGameplayProjection);
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
            }
            let _ = save_profile(runtime.storage.as_ref(), &runtime.profile);
            session.auth = Some(auth);
            session.games = games;
            session.mock_backend = mock_backend;
            session.connection = ConnectionStatus::Connected;
            session.notice = configuration_notice;
        },
        BackendOutput::Membership {
            operation,
            result,
            recovery_code,
        } => {
            let reconnected_without_rotation = matches!(operation, Operation::Join)
                && matches!(result.disposition, JoinDisposition::Reconnected);
            form.game_code = result.game.code.0.clone();
            session.issued_recovery_code = if reconnected_without_rotation {
                None
            } else {
                Some(recovery_code.expose().to_string())
            };
            session.reconnect_lobby = result.game.status == MatchStatus::Active
                && !matches!(operation, Operation::Create);
            install_membership(result, runtime, session, pending, next_state, gameplay_visible);
            session.notice = Some(match operation {
                Operation::Recover => {
                    "Player recovered. The old recovery code is now invalid.".to_string()
                },
                Operation::Join if reconnected_without_rotation => {
                    "Reconnected to the existing player on this device.".to_string()
                },
                Operation::Join => "Joined game successfully.".to_string(),
                _ => "Game created. Copy both codes before continuing.".to_string(),
            });
        },
        #[cfg(debug_assertions)]
        BackendOutput::PracticeReady {
            backend,
            auth,
            result,
        } => {
            runtime.backend = Some(backend);
            runtime.realtime_config = None;
            session.leave_selected_game();
            session.auth = Some(auth);
            session.games.clear();
            session.membership = Some(result.membership);
            session.mock_backend = true;
            session.local_practice = true;
            session.connection = ConnectionStatus::Connected;
            session.notice = None;
            install_record(result.game, runtime, session, pending, next_state, gameplay_visible);
        },
        BackendOutput::Games(games) => {
            session.games = games;
            session.connection = ConnectionStatus::Connected;
        },
        BackendOutput::Record(operation, record) => {
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
                _ => None,
            };
            session.resolving = false;
        },
        BackendOutput::Resumed => {
            session.reconnect_lobby = false;
            session.connection = ConnectionStatus::Connected;
            session.notice = Some("Everyone is connected. Resuming the game…".to_string());
            next_state.set(AppState::LoadingGame);
        },
        BackendOutput::Submitted(disposition, turn) => {
            session.submitted_turn = Some(turn);
            session.resolve_needed = true;
            session.connection = ConnectionStatus::Connected;
            session.notice = Some(if session.local_practice {
                "Resolving local turn…".to_string()
            } else {
                match disposition {
                    SubmissionDisposition::Inserted => {
                        "Turn submitted; waiting for other players.".to_string()
                    },
                    SubmissionDisposition::Duplicate => {
                        "Turn submission was already accepted.".to_string()
                    },
                }
            });
        },
        BackendOutput::Events(batch) => {
            let game_resumed = batch
                .events
                .iter()
                .any(|event| matches!(event.kind, BackendEventKind::GameResumed));
            session.event_cursor = batch.cursor;
            if !batch.events.is_empty() {
                session.reload_needed = true;
                session.resolve_needed = true;
            }
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
            session.notice = Some("Waiting for remaining turn submissions.".to_string());
        },
        BackendOutput::SessionRefreshed(auth) => {
            runtime.profile.session = Some(auth.clone());
            let _ = save_profile(runtime.storage.as_ref(), &runtime.profile);
            session.auth = Some(auth);
            session.connection = ConnectionStatus::Connected;
            session.reload_needed = session.has_active_game();
            session.presence_needed = session.has_active_game();
        },
        BackendOutput::Reauthenticated(auth) => {
            runtime.profile.session = Some(auth.clone());
            let _ = save_profile(runtime.storage.as_ref(), &runtime.profile);
            session.auth = Some(auth);
            session.games.clear();
            session.connection = ConnectionStatus::Connected;
            session.notice = Some(
                "A new anonymous session is ready. Use the recovery code to reclaim the previous player slot."
                    .to_string(),
            );
        },
        BackendOutput::Presence => {},
        BackendOutput::Failed(operation, error) => {
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
                session.reload_needed = true;
                session.resolve_needed = true;
            },
            RealtimeSignal::Connected => {
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
    let needs_gameplay_install =
        record_requires_gameplay_install(session.active_game.as_ref(), &record);
    if session.membership.as_ref().is_none_or(|membership| membership.game_id != record.id) {
        session.membership =
            session.auth.as_ref().and_then(|auth| record.membership_for(&auth.user_id)).cloned();
    }
    mark_selected_member_connected(session, &mut record);
    sync_game_summary(session, &record);
    if !session.local_practice {
        runtime.profile.remember_game(record.id.clone());
        let _ = save_profile(runtime.storage.as_ref(), &runtime.profile);
    }
    if needs_gameplay_install {
        pending.reset(record.persisted.state.turn);
    }
    let status = record.status;
    session.active_game = Some(record);
    session.reload_needed = false;
    session.presence_needed = !session.local_practice;
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

/// Keeps newly created and freshly loaded records immediately available on the resume screen.
fn sync_game_summary(session: &mut MultiplayerSession, record: &GameRecord) {
    let Some(membership) = &session.membership else {
        return;
    };
    let summary = GameSummary {
        id: record.id.clone(),
        code: record.code.clone(),
        revision: record.revision,
        status: record.status,
        turn: record.persisted.state.turn,
        player_id: membership.player_id,
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

/// Returns whether this client has committed the canonical turn that is still active.
fn local_submission_awaits_resolution(session: &MultiplayerSession) -> bool {
    session.active_game.as_ref().is_some_and(|record| {
        record.status == MatchStatus::Active
            && session.submitted_turn == Some(record.persisted.state.turn)
    })
}

/// Polls durable events periodically so missed/disconnected Realtime notifications are harmless.
fn poll_durable_events(
    time: Res<Time>,
    mut timer: ResMut<EventPollTimer>,
    runtime: Res<ClientRuntime>,
    session: Res<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    if !timer.0.tick(time.delta()).just_finished()
        || !tasks.0.is_empty()
        || !session.has_active_game()
    {
        return;
    }
    let (Some(backend), Some(auth), Some(record)) =
        (runtime.backend.clone(), session.auth.clone(), session.active_game.clone())
    else {
        return;
    };
    let cursor = session.event_cursor;
    spawn_backend_task(&mut tasks, async move {
        match backend.subscribe(&auth, &record.id, cursor).await {
            Ok(batch) => BackendOutput::Events(batch),
            Err(error) => BackendOutput::Failed(Operation::Events, error),
        }
    });
}

/// Announces coarse presence after selecting or reconnecting to a game.
fn drive_presence(
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    if !session.presence_needed || !tasks.0.is_empty() {
        return;
    }
    let (Some(backend), Some(auth), Some(record)) =
        (runtime.backend.clone(), session.auth.clone(), session.active_game.clone())
    else {
        return;
    };
    session.presence_needed = false;
    spawn_backend_task(&mut tasks, async move {
        match backend.set_connected(&auth, &record.id, true).await {
            Ok(()) => BackendOutput::Presence,
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
    let (Some(backend), Some(auth), Some(record)) =
        (runtime.backend.clone(), session.auth.clone(), session.active_game.clone())
    else {
        return;
    };
    session.reload_needed = false;
    spawn_backend_task(&mut tasks, async move {
        match backend.load_game(&auth, &record.id).await {
            Ok(record) => BackendOutput::Record(Operation::Load, record),
            Err(error) => BackendOutput::Failed(Operation::Load, error),
        }
    });
}

/// Attempts deterministic resolution after submission events; compare-and-swap accepts one winner.
fn drive_resolution(
    runtime: Res<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut tasks: ResMut<BackendTasks>,
) {
    if !session.resolve_needed
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
            Ok(submissions) => submissions,
            Err(error) => return BackendOutput::Failed(Operation::Resolve, error),
        };
        let required =
            record.persisted.state.players.iter().filter(|player| !player.spectator).count();
        if submissions.len() != required {
            return BackendOutput::ResolutionWaiting;
        }
        let mut model = record.persisted.state.clone();
        let commands = submissions.into_iter().map(|stored| stored.submission).collect::<Vec<_>>();
        if let Err(error) = resolve_turn(&mut model, &commands) {
            return BackendOutput::Failed(
                Operation::Resolve,
                BackendError::InvalidData(error.to_string()),
            );
        }
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
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::core::simulation::MatchStatus;

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

    #[cfg(debug_assertions)]
    #[test]
    /// Builds a started one-player backend record whose first submission advances immediately.
    fn local_practice_backend_advances_without_an_opponent() {
        let rules = GameRules {
            player_count: 1,
            practice_mode: true,
            ..GameRules::default()
        };
        let (backend, auth, result) = block_on(create_local_practice(rules)).unwrap();
        assert_eq!(result.game.status, MatchStatus::Active);
        assert_eq!(result.game.max_players, 1);
        assert_eq!(result.game.members.len(), 1);

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
            id: GameId::new(id),
            code: GameCode::new("ABCDEF"),
            revision,
            max_players: 2,
            status,
            persisted: PersistedGame::new(model),
            members: Vec::new(),
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
        assert_eq!(
            record_destination(MatchStatus::Lobby, false, true, false),
            Some(AppState::Lobby)
        );
        assert_eq!(
            record_destination(MatchStatus::Lobby, false, false, true),
            Some(AppState::Lobby)
        );
        assert_eq!(
            record_destination(MatchStatus::Active, true, true, false),
            Some(AppState::Lobby)
        );
    }

    #[test]
    /// A newly created lobby is resumable immediately, before another backend list refresh.
    fn selected_lobby_is_added_to_resume_list() {
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

        assert_eq!(session.games.len(), 1);
        assert_eq!(session.games[0].status, MatchStatus::Lobby);
        assert_eq!(session.games[0].player_id, 1);
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
            id,
            code: GameCode::new("ABCDEF"),
            revision: 0,
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
            recolored
                .state
                .players
                .iter()
                .map(|player| player.color())
                .collect::<HashSet<_>>()
                .len(),
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
}
