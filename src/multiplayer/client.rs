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
use crate::core::simulation::{
    resolve_turn, GameModel, GameRules, PersistedGame, TurnCommand, TurnSubmission,
    MAX_COMMANDS_PER_SUBMISSION,
};
use crate::core::states::AppState;
use crate::multiplayer::backend::{BackendError, MultiplayerBackend};
use crate::multiplayer::memory::InMemoryBackend;
use crate::multiplayer::model::{
    AuthSession, CreateGameRequest, EventBatch, GameMembership, GameRecord, GameSummary,
    JoinGameRequest, MembershipResult, RecoverPlayerRequest, SubmissionDisposition,
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

/// Editable values shared by the multiplayer menu screens.
#[derive(Resource)]
pub struct MultiplayerForm {
    /// Lobby display name, persisted locally for convenience.
    pub display_name: String,
    /// Six-character game code entered by a joining player.
    pub game_code: String,
    /// High-entropy recovery code entered on a replacement device.
    pub recovery_code: String,
    /// Exact lobby capacity selected by the creator.
    pub player_count: u8,
}

impl Default for MultiplayerForm {
    /// Uses a valid two-player form with no secrets filled in.
    fn default() -> Self {
        Self {
            display_name: "Commander".to_string(),
            game_code: String::new(),
            recovery_code: String::new(),
            player_count: 2,
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
    /// Whether local development is using the credential-free backend.
    pub mock_backend: bool,
    /// Whether a foreground menu operation is still running.
    pub busy: bool,
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
        self.reauthentication_needed = false;
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
    /// Refreshes the authenticated user's resumable-game list.
    RefreshGames,
    /// Starts a full lobby as its creator.
    StartGame,
    /// Saves the latest canonical snapshot from any member.
    SaveGame,
    /// Commits the local player's accumulated commands for this turn.
    SubmitTurn,
    /// Leaves the selected game locally and returns to the multiplayer menu.
    LeaveGame,
    /// Retries state/event synchronization after an offline error.
    Retry,
}

/// Runtime backend and local profile storage hidden from gameplay systems.
#[derive(Resource)]
struct ClientRuntime {
    backend: Option<Arc<dyn MultiplayerBackend>>,
    realtime_config: Option<SupabaseConfig>,
    storage: Arc<dyn ClientStorage>,
    profile: ClientProfile,
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
    Start,
    Save,
    Submit,
    Events,
    Resolve,
    Presence,
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
    Games(Vec<GameSummary>),
    Record(Operation, GameRecord),
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

/// Selects Supabase when public configuration exists, otherwise an isolated mock backend.
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
    match SupabaseConfig::load().await {
        Ok(config) => match SupabaseBackend::new(config.clone()) {
            Ok(backend) => (Arc::new(backend), false, None, Some(config)),
            Err(error) => (
                Arc::new(InMemoryBackend::new()),
                true,
                Some(format!(
                    "{}. Using the in-memory backend until public Supabase configuration is provided.",
                    ConfigError::Invalid(error.to_string())
                )),
                None,
            ),
        },
        Err(ConfigError::Missing) => (Arc::new(InMemoryBackend::new()), true, None, None),
        Err(error) => (
            Arc::new(InMemoryBackend::new()),
            true,
            Some(format!(
                "{error}. Using the in-memory backend until public Supabase configuration is provided."
            )),
            None,
        ),
    }
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
        if matches!(request, MultiplayerRequest::LeaveGame) {
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
        if matches!(request, MultiplayerRequest::Retry) {
            session.connection = ConnectionStatus::Reconnecting;
            session.reload_needed = session.has_active_game();
            if !session.has_active_game() {
                spawn_list(&runtime, &session, &mut tasks);
            }
            continue;
        }

        let (Some(backend), Some(auth)) = (runtime.backend.clone(), session.auth.clone()) else {
            session.notice = Some("Authentication is still initializing.".to_string());
            continue;
        };
        session.busy = true;
        session.notice = None;

        match request {
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
                        session.busy = false;
                        session.notice = Some(error.to_string());
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
                        session.busy = false;
                        session.notice = Some(error.to_string());
                        continue;
                    },
                };
                let replacement = match RecoveryCode::generate() {
                    Ok(code) => code,
                    Err(error) => {
                        session.busy = false;
                        session.notice = Some(error.to_string());
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
                        Ok(record) => BackendOutput::Record(Operation::Load, record),
                        Err(error) => BackendOutput::Failed(Operation::Load, error),
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
                let mut persisted = record.persisted.clone();
                if let Err(error) = persisted.state.start() {
                    request_error(&mut session, &error.to_string());
                    continue;
                }
                spawn_backend_task(&mut tasks, async move {
                    match backend.start_game(&auth, &record.id, record.revision, persisted).await {
                        Ok(record) => BackendOutput::Record(Operation::Start, record),
                        Err(error) => BackendOutput::Failed(Operation::Start, error),
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

/// Resets the busy flag and records an immediate request-validation failure.
fn request_error(session: &mut MultiplayerSession, message: &str) {
    session.busy = false;
    session.notice = Some(message.to_string());
}

/// Polls task futures once per frame and applies completed backend results.
fn poll_backend_tasks(
    mut tasks: ResMut<BackendTasks>,
    mut runtime: ResMut<ClientRuntime>,
    mut session: ResMut<MultiplayerSession>,
    mut form: ResMut<MultiplayerForm>,
    mut pending: ResMut<PendingTurnCommands>,
    mut next_state: ResMut<NextState<AppState>>,
) {
    let mut remaining = Vec::with_capacity(tasks.0.len());
    for mut task in std::mem::take(&mut tasks.0) {
        if let Some(output) = block_on(poll_once(&mut task)) {
            apply_output(
                output,
                &mut runtime,
                &mut session,
                &mut form,
                &mut pending,
                &mut next_state,
            );
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
            let code = recovery_code.expose().to_string();
            form.game_code = result.game.code.0.clone();
            session.issued_recovery_code = Some(code);
            install_membership(result, runtime, session, pending, next_state);
            session.notice = Some(match operation {
                Operation::Recover => {
                    "Player recovered. The old recovery code is now invalid.".to_string()
                },
                Operation::Join => "Joined game successfully.".to_string(),
                _ => "Game created. Copy both codes before continuing.".to_string(),
            });
        },
        BackendOutput::Games(games) => {
            session.games = games;
            session.connection = ConnectionStatus::Connected;
        },
        BackendOutput::Record(operation, record) => {
            install_record(record, runtime, session, pending, next_state);
            session.connection = ConnectionStatus::Connected;
            session.notice = match operation {
                Operation::Save => Some("Game saved at a new revision.".to_string()),
                Operation::Resolve => {
                    Some("All submissions resolved; the next turn is ready.".to_string())
                },
                _ => None,
            };
            session.resolving = false;
        },
        BackendOutput::Submitted(disposition, turn) => {
            session.submitted_turn = Some(turn);
            session.resolve_needed = true;
            session.connection = ConnectionStatus::Connected;
            session.notice = Some(match disposition {
                SubmissionDisposition::Inserted => {
                    "Turn submitted; waiting for other players.".to_string()
                },
                SubmissionDisposition::Duplicate => {
                    "Turn submission was already accepted.".to_string()
                },
            });
        },
        BackendOutput::Events(batch) => {
            session.event_cursor = batch.cursor;
            if !batch.events.is_empty() {
                session.reload_needed = true;
                session.resolve_needed = true;
            }
            session.connection = ConnectionStatus::Connected;
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
            session.notice = Some(error.to_string());
            match error {
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
                    session.notice = Some(format!(
                        "Realtime reconnecting ({reason}); durable polling remains active."
                    ));
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
) {
    session.membership = Some(result.membership);
    install_record(result.game, runtime, session, pending, next_state);
}

/// Makes a validated backend record canonical and selects lobby or gameplay loading state.
fn install_record(
    record: GameRecord,
    runtime: &mut ClientRuntime,
    session: &mut MultiplayerSession,
    pending: &mut PendingTurnCommands,
    next_state: &mut NextState<AppState>,
) {
    let needs_gameplay_install =
        record_requires_gameplay_install(session.active_game.as_ref(), &record);
    if session.membership.is_none() {
        if let Some(auth) = &session.auth {
            session.membership = record.membership_for(&auth.user_id).cloned();
        }
    }
    runtime.profile.remember_game(record.id.clone());
    let _ = save_profile(runtime.storage.as_ref(), &runtime.profile);
    if needs_gameplay_install {
        pending.reset(record.persisted.state.turn);
    }
    let status = record.status;
    session.active_game = Some(record);
    session.reload_needed = false;
    session.presence_needed = true;
    if status == crate::core::simulation::MatchStatus::Lobby {
        next_state.set(AppState::Lobby);
    } else if needs_gameplay_install {
        session.submitted_turn = None;
        next_state.set(AppState::LoadingGame);
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
    session.connection = ConnectionStatus::Reconnecting;
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
    if !session.resolve_needed || session.resolving || !tasks.0.is_empty() {
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
    use super::*;
    use crate::core::simulation::MatchStatus;

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
