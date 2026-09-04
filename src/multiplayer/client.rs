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
use crate::core::messages::MessageMsg;
use crate::core::player::PlayerColor;
use crate::core::simulation::{
    resolve_turn, GameModel, GameRules, MatchStatus, PersistedGame, TurnSubmission,
};
use crate::core::states::AppState;
use crate::multiplayer::authority::{recolored_lobby_snapshot, started_snapshot_for_members};
use crate::multiplayer::backend::{BackendError, MultiplayerBackend};
use crate::multiplayer::memory::InMemoryBackend;
use crate::multiplayer::model::{
    AuthSession, BackendEventKind, CreateGameRequest, EventBatch, GameMembership, GameRecord,
    GameSummary, JoinDisposition, JoinGameRequest, MembershipResult, RecoverPlayerRequest,
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
const PRESENCE_HEARTBEAT_INTERVAL: Duration = Duration::from_secs(3);
const HOST_CLOSED_LOBBY_NOTICE: &str = "The host closed the lobby.";
const HOST_CLOSED_LOBBY_NOTICE_DURATION: Duration = Duration::from_secs(2);

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
    /// Previously chosen name reused when joining, separate from an unsent create-form edit.
    pub saved_display_name: Option<String>,
    /// Six-character game code entered by a joining player.
    pub game_code: String,
    /// High-entropy recovery code entered on a replacement device.
    pub recovery_code: String,
    /// Empire color selected for the next local practice match.
    #[cfg(debug_assertions)]
    pub practice_color: PlayerColor,
}

impl Default for MultiplayerForm {
    /// Uses a valid two-player form with no secrets filled in.
    fn default() -> Self {
        Self {
            display_name: "Commander".to_string(),
            saved_display_name: None,
            game_code: String::new(),
            recovery_code: String::new(),
            #[cfg(debug_assertions)]
            practice_color: PlayerColor::for_player(1),
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
    /// Actionable failure displayed persistently in the menu UI.
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
    restore_draft_needed: bool,
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
        self.submitted_turn = None;
        self.presence_needed = false;
        self.presence_elapsed = Duration::ZERO;
        self.reconnect_lobby = false;
        self.reauthentication_needed = false;
        self.local_practice = false;
    }
}

mod profile;
mod submission;
pub use submission::{PendingTurnCommands, SubmissionState};

/// Foreground operations requested by menu buttons or gameplay UI.
#[derive(Message)]
pub enum MultiplayerRequest {
    /// Creates and starts an isolated one-player match without contacting Supabase.
    #[cfg(debug_assertions)]
    StartLocalPractice {
        /// Deterministic rules selected in the local-practice setup screen.
        rules: GameRules,
        /// Empire color selected in the local-practice setup screen.
        player_color: PlayerColor,
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
    /// Opens a linked game or replaces a lost identity using a one-time recovery code.
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
    /// Saves the latest canonical snapshot from any member and reports the result to the player.
    SaveGame,
    /// Saves the latest canonical snapshot without showing foreground feedback.
    AutosaveGame,
    /// Marks the local player ready to finish this turn.
    SubmitTurn,
    /// Returns to the main menu, deleting an unstarted lobby when its host leaves.
    LeaveGame,
    /// Retries state/event synchronization after an offline error.
    Retry,
}

#[derive(Message)]
/// Requests replacement of an already-visible gameplay turn without a menu transition.
pub(crate) struct RefreshGameplayProjection;

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
    Autosave,
    Submit,
    Withdraw,
    RestoreDraft,
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
    Submitted(u64),
    Withdrawn(TurnSubmission),
    DraftLoaded(u64, Option<crate::multiplayer::model::StoredTurnSubmission>),
    Events(EventBatch),
    ResolutionWaiting,
    SessionRefreshed(AuthSession),
    Reauthenticated(AuthSession),
    Presence,
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
                    profile::flush_profile,
                    drive_reauthentication,
                    drive_auth_refresh,
                    drive_realtime,
                    drive_presence,
                    // Apply pending roster changes before another event poll can occupy the task queue.
                    drive_reload,
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
    player_color: PlayerColor,
) -> Result<(Arc<dyn MultiplayerBackend>, AuthSession, MembershipResult), BackendError> {
    let backend = Arc::new(InMemoryBackend::new());
    let auth = backend.authenticate(None).await?;
    let mut seed = [0_u8; 32];
    getrandom::fill(&mut seed).map_err(|error| BackendError::Protocol(error.to_string()))?;
    let recovery =
        RecoveryCode::generate().map_err(|error| BackendError::Protocol(error.to_string()))?;
    let mut model = GameModel::new(seed, rules)
        .map_err(|error| BackendError::InvalidData(error.to_string()))?;
    model.player_mut(1).map_err(|error| BackendError::InvalidData(error.to_string()))?.color =
        Some(player_color);
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
    mut messages: MessageWriter<MessageMsg>,
    mut session: ResMut<MultiplayerSession>,
    mut runtime: ResMut<ClientRuntime>,
    mut pending: ResMut<PendingTurnCommands>,
    mut form: ResMut<MultiplayerForm>,
    mut tasks: ResMut<BackendTasks>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    for request in requests.read() {
        session.menu_error = None;
        if matches!(request, MultiplayerRequest::LeaveGame) {
            // Take ownership of pending work so none of its results can restore the lobby.
            let outstanding = std::mem::take(&mut tasks.0);
            session.busy = false;
            if session.local_practice {
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
            player_color,
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
            let player_color = *player_color;
            spawn_backend_task(&mut tasks, async move {
                match create_local_practice(rules, player_color).await {
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
                let code = GameCode::new(code);
                // Existing membership needs no recovery credential or secret rotation.
                if let Some(game_id) = linked_game_id(&session.games, &code) {
                    spawn_backend_task(&mut tasks, load_game_for_resume(backend, auth, game_id));
                    continue;
                }
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
                    code,
                    recovery_hash: supplied.hash().0,
                    replacement_recovery_hash: replacement.hash().0,
                };
                spawn_backend_task(
                    &mut tasks,
                    recover_or_resume_linked_game(backend, auth, request, replacement),
                );
            },
            MultiplayerRequest::ResumeGame(game_id) => {
                let game_id = game_id.clone();
                spawn_backend_task(&mut tasks, load_game_for_resume(backend, auth, game_id));
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
            MultiplayerRequest::SaveGame | MultiplayerRequest::AutosaveGame => {
                let operation = if matches!(request, MultiplayerRequest::SaveGame) {
                    Operation::Save
                } else {
                    Operation::Autosave
                };
                let Some(record) = session.active_game.clone() else {
                    let error = "No game is selected.";
                    request_error(&mut session, error);
                    if matches!(operation, Operation::Save) {
                        messages.write(MessageMsg::error(error));
                    }
                    continue;
                };
                spawn_backend_task(&mut tasks, async move {
                    match backend
                        .save_game(&auth, &record.id, record.revision, record.persisted.clone())
                        .await
                    {
                        Ok(record) => BackendOutput::Record(operation, record),
                        Err(error) => BackendOutput::Failed(operation, error),
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
            "This recovery code is invalid or has already been used. Each player needs their own private recovery code. Check both codes and use your latest recovery code.".to_string()
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
            "This game is no longer available.".to_string()
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
        BackendOutput::Record(Operation::Save, _) => {
            Some(MessageMsg::info("Game saved successfully."))
        },
        BackendOutput::Failed(Operation::Save, error) => {
            Some(MessageMsg::error(user_facing_backend_error(Operation::Save, error)))
        },
        BackendOutput::Failed(
            Operation::Submit | Operation::Resolve,
            error @ BackendError::InvalidData(_),
        ) => Some(MessageMsg::error(format!("Could not end turn: {error}"))),
        _ => None,
    }
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
    let BackendOutput::Record(_, next) = output else {
        return Vec::new();
    };
    let Some(previous) = session.active_game.as_ref().filter(|game| game.id == next.id) else {
        return Vec::new();
    };
    if next.status == MatchStatus::Lobby {
        return Vec::new();
    }
    let local_player_id = session.membership.as_ref().map(|member| member.player_id);
    next.members
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
            if gameplay_visible && restore_draft {
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
            recovery_code,
        } => {
            let reconnected_without_rotation = matches!(operation, Operation::Join)
                && matches!(result.disposition, JoinDisposition::Reconnected);
            form.display_name.clone_from(&result.membership.display_name);
            form.saved_display_name = Some(result.membership.display_name.clone());
            runtime.profile.display_name.clone_from(&result.membership.display_name);
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
                    form.recovery_code.clear();
                    "Game recovered. Save your new recovery code; it replaces the code you just used.".to_string()
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
            session.games =
                games.into_iter().filter(|game| game.status != MatchStatus::Lobby).collect();
            session.connection = ConnectionStatus::Connected;
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
            if matches!(operation, Operation::ResumeLoad) {
                session.reconnect_lobby = record.status == MatchStatus::Active;
            }
            install_record(record, runtime, session, pending, next_state, gameplay_visible);
            session.connection = ConnectionStatus::Connected;
            session.notice = match operation {
                Operation::Color => None,
                Operation::Save | Operation::Autosave => {
                    Some("Game saved successfully.".to_string())
                },
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
                "Waiting for other players to finish their turn.".to_string()
            });
        },
        BackendOutput::Withdrawn(draft) => {
            if draft.turn == pending.turn {
                // A ready request that never arrived has no server-side orders to restore.
                // The local draft remains the source in that case.
                if !draft.commands.is_empty() || pending.commands.is_empty() {
                    pending.commands = draft.commands;
                }
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
            session.notice = Some("Waiting for other players to finish their turn.".to_string());
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
        BackendOutput::Presence => {
            // Lease expiry creates no durable event. Refresh after each heartbeat so an
            // abandoned peer becomes offline even while the event stream stays quiet.
            session.reload_needed = session.has_active_game();
        },
        BackendOutput::DepartureFinished => {},
        BackendOutput::Failed(operation, error) => {
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
                            | Operation::Start
                            | Operation::Resume
                            | Operation::Save
                            | Operation::Autosave
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
                            "This game is no longer available.".to_string()
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
    let summary = GameSummary {
        id: record.id.clone(),
        code: record.code.clone(),
        revision: record.revision,
        saved_at: record.saved_at,
        status: record.status,
        turn: record.persisted.state.turn,
        player_id: membership.player_id,
        display_name: membership.display_name.clone(),
        player_color: record.persisted.state.player(membership.player_id).map_or_else(
            |_| PlayerColor::for_player(membership.player_id),
            |player| player.color(),
        ),
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

/// Recovers saved orders or clears readiness after a planet/Continue turn interaction.
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
        session.active_game.clone(),
        session.membership.clone(),
    ) else {
        return;
    };
    if record.status != MatchStatus::Active || pending.turn != record.persisted.state.turn {
        return;
    }
    let restoring = session.restore_draft_needed;
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
        let stored = match backend.load_turn_submissions(&auth, &record.id, turn).await {
            Ok(submissions) => {
                submissions.into_iter().find(|s| s.submission.player_id == member.player_id)
            },
            Err(error) => return BackendOutput::Failed(operation, error),
        };
        if restoring {
            return BackendOutput::DraftLoaded(turn, stored);
        }
        let generation = stored.as_ref().map_or(0, |s| s.submission.generation);
        match backend.withdraw_turn(&auth, &record.id, turn, generation).await {
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
    let (Some(backend), Some(auth), Some(record)) =
        (runtime.backend.clone(), session.auth.clone(), session.active_game.clone())
    else {
        return;
    };
    session.presence_needed = false;
    session.presence_elapsed = Duration::ZERO;
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

/// Resolves a normalized game code to the same backend identifier used by Resume Game.
fn linked_game_id(games: &[GameSummary], code: &GameCode) -> Option<GameId> {
    games.iter().find(|game| &game.code == code).map(|game| game.id.clone())
}

/// Loads one already-linked game through the shared Resume Game result path.
async fn load_game_for_resume(
    backend: Arc<dyn MultiplayerBackend>,
    auth: AuthSession,
    game_id: GameId,
) -> BackendOutput {
    match backend.load_game(&auth, &game_id).await {
        Ok(record) => BackendOutput::Record(Operation::ResumeLoad, record),
        Err(error) => BackendOutput::Failed(Operation::ResumeLoad, error),
    }
}

/// Recovers an unlinked slot, or opens the current membership if recovery is redundant.
async fn recover_or_resume_linked_game(
    backend: Arc<dyn MultiplayerBackend>,
    auth: AuthSession,
    request: RecoverPlayerRequest,
    replacement: RecoveryCode,
) -> BackendOutput {
    let code = request.code.clone();
    match backend.recover_player(&auth, request).await {
        Ok(result) => BackendOutput::Membership {
            operation: Operation::Recover,
            result,
            recovery_code: replacement,
        },
        Err(BackendError::AlreadyMember) => {
            let games = match backend.list_games(&auth).await {
                Ok(games) => games,
                Err(error) => return BackendOutput::Failed(Operation::Recover, error),
            };
            let Some(game_id) = linked_game_id(&games, &code) else {
                // Lobbies and expired matches are intentionally absent from Resume Game.
                return BackendOutput::Failed(Operation::Recover, BackendError::GameNotFound);
            };
            load_game_for_resume(backend, auth, game_id).await
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
