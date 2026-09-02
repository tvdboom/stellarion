//! Production Supabase Auth and PostgREST/RPC implementation of the backend contract.

use std::collections::HashSet;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::core::identity::{GameCode, GameId, PlayerId, UserId};
use crate::core::simulation::{
    MatchStatus, PersistedGame, TurnSubmission, MAX_COMMANDS_PER_SUBMISSION,
};
use crate::multiplayer::backend::{BackendError, BackendFuture, MultiplayerBackend};
use crate::multiplayer::model::{
    AuthSession, CreateGameRequest, EventBatch, GameMembership, GameRecord, GameSummary,
    JoinGameRequest, MembershipResult, RecoverPlayerRequest, StoredTurnSubmission,
    SubmissionDisposition,
};
use crate::platform::config::SupabaseConfig;

/// Browser-compatible Supabase client that ships only a public publishable key.
pub struct SupabaseBackend {
    config: SupabaseConfig,
    client: reqwest::Client,
    request_timeout: std::time::Duration,
}

impl SupabaseBackend {
    /// Creates a production backend from validated public configuration.
    pub fn new(config: SupabaseConfig) -> Result<Self, BackendError> {
        let config = SupabaseConfig::new(config.url, config.publishable_key)
            .map_err(|error| BackendError::InvalidData(error.to_string()))?;
        Ok(Self {
            config,
            client: reqwest::Client::new(),
            request_timeout: std::time::Duration::from_secs(30),
        })
    }

    /// Calls a security-definer database function with the caller's JWT and decodes JSON.
    async fn rpc<Request: Serialize + ?Sized, Response: DeserializeOwned>(
        &self,
        session: &AuthSession,
        function: &str,
        request: &Request,
    ) -> Result<Response, BackendError> {
        let response = self
            .client
            .post(self.config.endpoint(&format!("rest/v1/rpc/{function}")))
            .header("apikey", &self.config.publishable_key)
            .bearer_auth(&session.access_token)
            .header("Content-Type", "application/json")
            .json(request)
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(network_error)?;
        decode_response(response).await
    }

    /// Exchanges a refresh token for a new anonymous-auth access token.
    async fn refresh(&self, session: &AuthSession) -> Result<AuthSession, BackendError> {
        let response = self
            .client
            .post(self.config.endpoint("auth/v1/token?grant_type=refresh_token"))
            .header("apikey", &self.config.publishable_key)
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "refresh_token": session.refresh_token }))
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(network_error)?;
        decode_auth(response).await
    }

    /// Creates a new anonymous Supabase Auth identity.
    async fn sign_in_anonymously(&self) -> Result<AuthSession, BackendError> {
        let response = self
            .client
            .post(self.config.endpoint("auth/v1/signup"))
            .header("apikey", &self.config.publishable_key)
            .header("Authorization", format!("Bearer {}", self.config.publishable_key))
            .header("Content-Type", "application/json")
            .json(&serde_json::json!({ "data": { "client": "stellarion" } }))
            .timeout(self.request_timeout)
            .send()
            .await
            .map_err(network_error)?;
        decode_auth(response).await
    }
}

impl MultiplayerBackend for SupabaseBackend {
    /// Refreshes a stored anonymous session when possible, otherwise creates a new identity.
    fn authenticate<'a>(
        &'a self,
        stored: Option<&'a AuthSession>,
    ) -> BackendFuture<'a, AuthSession> {
        Box::pin(async move {
            if let Some(session) = stored {
                match self.refresh(session).await {
                    Ok(refreshed) => return Ok(refreshed),
                    Err(BackendError::Unauthenticated) => {},
                    Err(error) => return Err(error),
                }
            }
            self.sign_in_anonymously().await
        })
    }

    /// Rotates an existing refresh token while preserving the anonymous user identity.
    fn refresh_session<'a>(&'a self, session: &'a AuthSession) -> BackendFuture<'a, AuthSession> {
        Box::pin(async move {
            let refreshed = self.refresh(session).await?;
            if refreshed.user_id != session.user_id {
                return Err(BackendError::Protocol(
                    "Supabase refreshed a different anonymous user identity".to_string(),
                ));
            }
            Ok(refreshed)
        })
    }

    /// Creates a fresh game through the atomic `stellarion_create_game` database function.
    fn create_game<'a>(
        &'a self,
        session: &'a AuthSession,
        request: CreateGameRequest,
    ) -> BackendFuture<'a, MembershipResult> {
        Box::pin(async move {
            validate_persisted(&request.persisted)?;
            if request.persisted.state.status != MatchStatus::Lobby {
                return Err(BackendError::InvalidGameStatus);
            }
            let payload = CreateGameRpc {
                code: request.code.0,
                display_name: request.display_name,
                recovery_hash: request.recovery_hash,
                max_players: request.persisted.state.rules.player_count,
                persisted: request.persisted,
            };
            let result = self.rpc(session, "stellarion_create_game", &payload).await?;
            validate_membership_result(result, &session.user_id)
        })
    }

    /// Joins or reconnects through the atomic `stellarion_join_game` database function.
    fn join_game<'a>(
        &'a self,
        session: &'a AuthSession,
        request: JoinGameRequest,
    ) -> BackendFuture<'a, MembershipResult> {
        Box::pin(async move {
            let result = self
                .rpc(
                    session,
                    "stellarion_join_game",
                    &JoinGameRpc {
                        code: request.code.0,
                        display_name: request.display_name,
                        recovery_hash: request.recovery_hash,
                    },
                )
                .await?;
            validate_membership_result(result, &session.user_id)
        })
    }

    /// Verifies and rotates a recovery hash inside one database transaction.
    fn recover_player<'a>(
        &'a self,
        session: &'a AuthSession,
        request: RecoverPlayerRequest,
    ) -> BackendFuture<'a, MembershipResult> {
        Box::pin(async move {
            let result = self
                .rpc(
                    session,
                    "stellarion_recover_player",
                    &RecoverPlayerRpc {
                        code: request.code.0,
                        recovery_hash: request.recovery_hash,
                        replacement_recovery_hash: request.replacement_recovery_hash,
                    },
                )
                .await?;
            validate_membership_result(result, &session.user_id)
        })
    }

    /// Lists resumable games through an RLS-aware database function.
    fn list_games<'a>(&'a self, session: &'a AuthSession) -> BackendFuture<'a, Vec<GameSummary>> {
        Box::pin(async move {
            let summaries =
                self.rpc(session, "stellarion_list_games", &serde_json::json!({})).await?;
            validate_summaries(summaries)
        })
    }

    /// Loads the latest state and current membership mapping.
    fn load_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
    ) -> BackendFuture<'a, GameRecord> {
        Box::pin(async move {
            let record = self
                .rpc(
                    session,
                    "stellarion_load_game",
                    &GameIdRpc {
                        game_id: &game_id.0,
                    },
                )
                .await?;
            validate_game_record(record, Some(game_id), Some(&session.user_id))
        })
    }

    /// Starts a lobby with its current members and optimistic revision protection.
    fn start_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord> {
        Box::pin(async move {
            validate_persisted(&persisted)?;
            if persisted.state.status != MatchStatus::Active {
                return Err(BackendError::InvalidGameStatus);
            }
            let record = self
                .rpc(
                    session,
                    "stellarion_start_game",
                    &StateWriteRpc {
                        game_id: &game_id.0,
                        expected_revision,
                        persisted,
                    },
                )
                .await?;
            validate_game_record(record, Some(game_id), Some(&session.user_id))
        })
    }

    /// Releases a started match after the host and every member have reconnected.
    fn resume_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            let acknowledgement: PresenceRpcResponse = self
                .rpc(
                    session,
                    "stellarion_resume_game",
                    &GameIdRpc {
                        game_id: &game_id.0,
                    },
                )
                .await?;
            if acknowledgement.ok {
                Ok(())
            } else {
                Err(BackendError::Protocol(
                    "resume RPC did not acknowledge the release".to_string(),
                ))
            }
        })
    }

    /// Saves complete state from any member using compare-and-swap semantics.
    fn save_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord> {
        Box::pin(async move {
            validate_persisted(&persisted)?;
            let record = self
                .rpc(
                    session,
                    "stellarion_save_game",
                    &StateWriteRpc {
                        game_id: &game_id.0,
                        expected_revision,
                        persisted,
                    },
                )
                .await?;
            validate_game_record(record, Some(game_id), Some(&session.user_id))
        })
    }

    /// Inserts an idempotent command submission under a unique turn/player key.
    fn submit_turn<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        submission: TurnSubmission,
    ) -> BackendFuture<'a, SubmissionDisposition> {
        Box::pin(async move {
            validate_submission(&submission, None)?;
            let response: SubmissionRpcResponse = self
                .rpc(
                    session,
                    "stellarion_submit_turn",
                    &SubmitTurnRpc {
                        game_id: &game_id.0,
                        submission,
                    },
                )
                .await?;
            Ok(response.disposition)
        })
    }

    /// Clears readiness atomically unless everyone has already finished the turn.
    fn withdraw_turn<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        turn: u64,
        generation: u64,
    ) -> BackendFuture<'a, TurnSubmission> {
        Box::pin(async move {
            let draft = self
                .rpc(
                    session,
                    "stellarion_withdraw_turn",
                    &WithdrawTurnRpc {
                        game_id: &game_id.0,
                        turn,
                        generation,
                    },
                )
                .await?;
            validate_submission(&draft, Some(turn))?;
            Ok(draft)
        })
    }

    /// Loads ready submissions and recoverable drafts for this turn.
    fn load_turn_submissions<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        turn: u64,
    ) -> BackendFuture<'a, Vec<StoredTurnSubmission>> {
        Box::pin(async move {
            let submissions = self
                .rpc(
                    session,
                    "stellarion_load_turn_submissions",
                    &TurnRpc {
                        game_id: &game_id.0,
                        turn,
                    },
                )
                .await?;
            validate_stored_submissions(submissions, turn)
        })
    }

    /// Publishes a resolved snapshot only after the database rechecks revision and completeness.
    fn publish_resolution<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        resolved_turn: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord> {
        Box::pin(async move {
            validate_persisted(&persisted)?;
            if persisted.state.turn != resolved_turn.saturating_add(1) {
                return Err(BackendError::InvalidData(
                    "resolved snapshot turn does not follow the submitted turn".to_string(),
                ));
            }
            let record = self
                .rpc(
                    session,
                    "stellarion_publish_resolution",
                    &ResolutionRpc {
                        game_id: &game_id.0,
                        expected_revision,
                        resolved_turn,
                        persisted,
                    },
                )
                .await?;
            validate_game_record(record, Some(game_id), Some(&session.user_id))
        })
    }

    /// Replays durable events; the Bevy adapter also uses Supabase Realtime as a wake-up signal.
    fn subscribe<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        after_sequence: u64,
    ) -> BackendFuture<'a, EventBatch> {
        Box::pin(async move {
            let batch = self
                .rpc(
                    session,
                    "stellarion_events_since",
                    &EventsRpc {
                        game_id: &game_id.0,
                        after_sequence,
                    },
                )
                .await?;
            validate_event_batch(batch, game_id, after_sequence)
        })
    }

    /// Updates presence, deleting an unstarted lobby when its host disconnects.
    fn set_connected<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        connected: bool,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            let response: PresenceRpcResponse = self
                .rpc(
                    session,
                    "stellarion_set_connected",
                    &PresenceRpc {
                        game_id: &game_id.0,
                        connected,
                    },
                )
                .await?;
            if response.ok {
                Ok(())
            } else {
                Err(BackendError::Protocol(
                    "presence RPC did not acknowledge the update".to_string(),
                ))
            }
        })
    }
}

#[derive(Serialize)]
/// Serialized argument object for the create-game database RPC.
struct CreateGameRpc {
    #[serde(rename = "p_code")]
    code: String,
    #[serde(rename = "p_display_name")]
    display_name: String,
    #[serde(rename = "p_recovery_hash")]
    recovery_hash: String,
    #[serde(rename = "p_max_players")]
    max_players: u8,
    #[serde(rename = "p_persisted")]
    persisted: PersistedGame,
}

#[derive(Serialize)]
/// Serialized argument object for the join-game database RPC.
struct JoinGameRpc {
    #[serde(rename = "p_code")]
    code: String,
    #[serde(rename = "p_display_name")]
    display_name: String,
    #[serde(rename = "p_recovery_hash")]
    recovery_hash: String,
}

#[derive(Serialize)]
/// Serialized argument object for the player-recovery database RPC.
struct RecoverPlayerRpc {
    #[serde(rename = "p_code")]
    code: String,
    #[serde(rename = "p_recovery_hash")]
    recovery_hash: String,
    #[serde(rename = "p_replacement_recovery_hash")]
    replacement_recovery_hash: String,
}

#[derive(Serialize)]
/// Borrowed game identifier passed to a load RPC.
struct GameIdRpc<'a> {
    #[serde(rename = "p_game_id")]
    game_id: &'a str,
}

#[derive(Serialize)]
/// Borrowed game identifier, expected revision, and state passed to a CAS write.
struct StateWriteRpc<'a> {
    #[serde(rename = "p_game_id")]
    game_id: &'a str,
    #[serde(rename = "p_expected_revision")]
    expected_revision: u64,
    #[serde(rename = "p_persisted")]
    persisted: PersistedGame,
}

#[derive(Serialize)]
/// Borrowed game identifier and intentional command submission passed to the database.
struct SubmitTurnRpc<'a> {
    #[serde(rename = "p_game_id")]
    game_id: &'a str,
    #[serde(rename = "p_submission")]
    submission: TurnSubmission,
}

#[derive(Deserialize)]
/// Acknowledgement returned by the idempotent submission RPC.
struct SubmissionRpcResponse {
    disposition: SubmissionDisposition,
}

#[derive(Serialize)]
/// Borrowed game identifier and turn used to load canonical submissions.
struct TurnRpc<'a> {
    #[serde(rename = "p_game_id")]
    game_id: &'a str,
    #[serde(rename = "p_turn")]
    turn: u64,
}

#[derive(Serialize)]
struct WithdrawTurnRpc<'a> {
    #[serde(rename = "p_game_id")]
    game_id: &'a str,
    #[serde(rename = "p_turn")]
    turn: u64,
    #[serde(rename = "p_generation")]
    generation: u64,
}

#[derive(Serialize)]
/// CAS arguments used to publish one deterministic resolved turn.
struct ResolutionRpc<'a> {
    #[serde(rename = "p_game_id")]
    game_id: &'a str,
    #[serde(rename = "p_expected_revision")]
    expected_revision: u64,
    #[serde(rename = "p_resolved_turn")]
    resolved_turn: u64,
    #[serde(rename = "p_persisted")]
    persisted: PersistedGame,
}

#[derive(Serialize)]
/// Durable event replay cursor arguments.
struct EventsRpc<'a> {
    #[serde(rename = "p_game_id")]
    game_id: &'a str,
    #[serde(rename = "p_after_sequence")]
    after_sequence: u64,
}

#[derive(Serialize)]
/// Coarse connected/disconnected presence arguments.
struct PresenceRpc<'a> {
    #[serde(rename = "p_game_id")]
    game_id: &'a str,
    #[serde(rename = "p_connected")]
    connected: bool,
}

#[derive(Deserialize)]
/// Acknowledgement returned after updating coarse presence.
struct PresenceRpcResponse {
    ok: bool,
}

#[derive(Deserialize)]
/// Subset of a successful Supabase Auth session response consumed by the client.
struct AuthResponse {
    access_token: String,
    refresh_token: String,
    expires_at: Option<u64>,
    user: AuthUser,
}

#[derive(Deserialize)]
/// Authenticated user identity nested in an Auth response.
struct AuthUser {
    id: String,
}

#[derive(Deserialize)]
/// Structured fields returned by PostgREST and Supabase Auth failures.
struct SupabaseErrorBody {
    code: Option<serde_json::Value>,
    error_code: Option<String>,
    message: Option<String>,
    msg: Option<String>,
    details: Option<String>,
    hint: Option<String>,
}

/// Rejects a malformed core envelope before it is sent or installed locally.
fn validate_persisted(persisted: &PersistedGame) -> Result<(), BackendError> {
    persisted.validate().map_err(|error| BackendError::InvalidData(error.to_string()))
}

/// Validates a complete RPC game record and its membership cross-references.
fn validate_game_record(
    record: GameRecord,
    expected_game_id: Option<&GameId>,
    expected_user_id: Option<&UserId>,
) -> Result<GameRecord, BackendError> {
    validate_persisted(&record.persisted)?;
    if record.id.0.trim().is_empty() {
        return invalid_protocol("game record has an empty identifier");
    }
    if expected_game_id.is_some_and(|expected| expected != &record.id) {
        return invalid_protocol("game record identifier does not match the request");
    }
    validate_game_code(&record.code)?;
    if record.max_players != record.persisted.state.rules.player_count {
        return invalid_protocol("game capacity does not match persisted rules");
    }
    if record.status != record.persisted.state.status {
        return invalid_protocol("game status does not match persisted state");
    }
    if record.members.is_empty() || record.members.len() > usize::from(record.max_players) {
        return invalid_protocol("game membership count is outside its capacity");
    }
    if record.status != MatchStatus::Lobby
        && record.members.len() != usize::from(record.max_players)
    {
        return invalid_protocol("a started game does not contain every configured membership");
    }

    let model_players =
        record.persisted.state.players.iter().map(|player| player.id).collect::<HashSet<_>>();
    let mut player_ids = HashSet::with_capacity(record.members.len());
    let mut user_ids = HashSet::with_capacity(record.members.len());
    let mut creator_count = 0_usize;
    let mut previous_slot = 0;
    for member in &record.members {
        validate_membership(member, &record.id, &model_players)?;
        if member.player_id <= previous_slot {
            return invalid_protocol("members are not in stable player-slot order");
        }
        previous_slot = member.player_id;
        if !player_ids.insert(member.player_id) || !user_ids.insert(&member.user_id) {
            return invalid_protocol("game record contains a duplicate player or user mapping");
        }
        if member.is_creator {
            creator_count += 1;
            if member.player_id != 1 {
                return invalid_protocol("the creator must own player slot one");
            }
        }
    }
    let submitted = record.submitted_players.iter().copied().collect::<HashSet<_>>();
    if submitted.len() != record.submitted_players.len()
        || submitted.iter().any(|id| !player_ids.contains(id))
    {
        return invalid_protocol("invalid submitted-player list");
    }
    if creator_count != 1 {
        return invalid_protocol("game record must contain exactly one creator");
    }
    if expected_user_id.is_some_and(|user_id| !user_ids.contains(user_id)) {
        return invalid_protocol("game record omitted the authenticated membership");
    }
    Ok(record)
}

/// Validates one membership against its containing game and deterministic player slots.
fn validate_membership(
    member: &GameMembership,
    game_id: &GameId,
    model_players: &HashSet<PlayerId>,
) -> Result<(), BackendError> {
    if &member.game_id != game_id {
        return invalid_protocol("membership references a different game");
    }
    if !model_players.contains(&member.player_id) {
        return invalid_protocol("membership references an unknown player slot");
    }
    if member.user_id.0.trim().is_empty() {
        return invalid_protocol("membership has an empty authenticated user identifier");
    }
    let name_length = member.display_name.trim().chars().count();
    if !(1..=32).contains(&name_length) || member.display_name != member.display_name.trim() {
        return invalid_protocol("membership contains an invalid display name");
    }
    if member.identity_version == 0 {
        return invalid_protocol("membership identity version must be positive");
    }
    Ok(())
}

/// Validates a create, join, or recovery response and its returned caller mapping.
fn validate_membership_result(
    result: MembershipResult,
    expected_user_id: &UserId,
) -> Result<MembershipResult, BackendError> {
    let game = validate_game_record(result.game, None, Some(expected_user_id))?;
    if result.membership.user_id != *expected_user_id {
        return invalid_protocol("membership result belongs to a different authenticated user");
    }
    let canonical =
        game.members.iter().find(|member| member.user_id == *expected_user_id).ok_or_else(
            || {
                BackendError::Protocol(
                    "membership result omitted the authenticated user from the game".to_string(),
                )
            },
        )?;
    if canonical != &result.membership {
        return invalid_protocol("membership result differs from the canonical game member");
    }
    Ok(MembershipResult {
        game,
        ..result
    })
}

/// Validates lightweight resume entries returned for the authenticated identity.
fn validate_summaries(summaries: Vec<GameSummary>) -> Result<Vec<GameSummary>, BackendError> {
    let mut game_ids = HashSet::with_capacity(summaries.len());
    for summary in &summaries {
        if summary.id.0.trim().is_empty() || !game_ids.insert(&summary.id) {
            return invalid_protocol("resume list contains an empty or duplicate game identifier");
        }
        validate_game_code(&summary.code)?;
        let name_length = summary.display_name.trim().chars().count();
        if !(1..=32).contains(&name_length)
            || summary.display_name != summary.display_name.trim()
            || !summary.player_color.is_valid()
        {
            return invalid_protocol("resume entry contains an invalid player name or color");
        }
        if !(2..=4).contains(&summary.max_players)
            || summary.player_id == 0
            || summary.player_id > u64::from(summary.max_players)
            || summary.player_count == 0
            || summary.player_count > usize::from(summary.max_players)
            || summary.turn == 0
        {
            return invalid_protocol("resume entry contains invalid game boundaries");
        }
        if summary.status != MatchStatus::Lobby
            && summary.player_count != usize::from(summary.max_players)
        {
            return invalid_protocol("started resume entry does not contain every player");
        }
    }
    Ok(summaries)
}

/// Validates one game code against the database's Crockford share-code constraint.
fn validate_game_code(code: &GameCode) -> Result<(), BackendError> {
    const ALPHABET: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    if code.0.len() == 6 && code.0.bytes().all(|byte| ALPHABET.contains(&byte)) {
        Ok(())
    } else {
        invalid_protocol("game record contains an invalid share code")
    }
}

/// Validates the context-independent fields of a simultaneous-turn submission.
fn validate_submission(
    submission: &TurnSubmission,
    expected_turn: Option<u64>,
) -> Result<(), BackendError> {
    if submission.player_id == 0
        || submission.player_id > 4
        || submission.turn == 0
        || submission.generation > i64::MAX as u64
        || submission.commands.len() > MAX_COMMANDS_PER_SUBMISSION
        || expected_turn.is_some_and(|turn| submission.turn != turn)
    {
        return Err(BackendError::InvalidData(
            "turn submission contains an invalid player or turn".to_string(),
        ));
    }
    Ok(())
}

/// Validates canonical ordering, turn identity, and digests in loaded submissions.
fn validate_stored_submissions(
    submissions: Vec<StoredTurnSubmission>,
    expected_turn: u64,
) -> Result<Vec<StoredTurnSubmission>, BackendError> {
    let mut previous_player = 0;
    for stored in &submissions {
        validate_submission(&stored.submission, Some(expected_turn))?;
        if stored.submission.player_id <= previous_player {
            return invalid_protocol("turn submissions are duplicated or out of order");
        }
        previous_player = stored.submission.player_id;
        if stored.digest.len() != 64
            || !stored
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return invalid_protocol("turn submission contains an invalid SHA-256 digest");
        }
    }
    Ok(submissions)
}

/// Validates ordering and scoping of a durable notification replay batch.
fn validate_event_batch(
    batch: EventBatch,
    game_id: &GameId,
    after_sequence: u64,
) -> Result<EventBatch, BackendError> {
    if batch.events.len() > 256 {
        return invalid_protocol("event replay exceeded its documented batch bound");
    }
    let mut previous = after_sequence;
    for event in &batch.events {
        if &event.game_id != game_id || event.sequence <= previous || event.sequence > batch.cursor
        {
            return invalid_protocol("event replay is stale, mis-scoped, or out of order");
        }
        if event.player_id.is_some_and(|player_id| player_id == 0 || player_id > 4)
            || event.turn.is_some_and(|turn| turn == 0)
        {
            return invalid_protocol("event replay contains an invalid player or turn");
        }
        previous = event.sequence;
    }
    let expected_cursor = batch.events.last().map_or(after_sequence, |event| event.sequence);
    if batch.cursor != expected_cursor {
        return invalid_protocol("event replay cursor does not match its final event");
    }
    Ok(batch)
}

/// Constructs a protocol error for a semantically invalid successful response.
fn invalid_protocol<T>(message: impl Into<String>) -> Result<T, BackendError> {
    Err(BackendError::Protocol(message.into()))
}

/// Decodes a successful Auth response and maps rejected refresh credentials.
async fn decode_auth(response: reqwest::Response) -> Result<AuthSession, BackendError> {
    let auth: AuthResponse = decode_response(response).await?;
    if auth.user.id.trim().is_empty()
        || auth.access_token.trim().is_empty()
        || auth.refresh_token.trim().is_empty()
    {
        return Err(BackendError::Protocol(
            "Supabase Auth returned an incomplete anonymous session".to_string(),
        ));
    }
    Ok(AuthSession {
        user_id: UserId::new(auth.user.id),
        access_token: auth.access_token,
        refresh_token: auth.refresh_token,
        expires_at: auth.expires_at,
    })
}

/// Parses a successful JSON response or maps PostgREST/Auth failures into typed errors.
async fn decode_response<T: DeserializeOwned>(
    response: reqwest::Response,
) -> Result<T, BackendError> {
    let status = response.status();
    let body = response.text().await.map_err(network_error)?;
    if status.is_success() {
        return serde_json::from_str(&body)
            .map_err(|error| BackendError::Protocol(format!("invalid JSON response: {error}")));
    }
    let parsed = serde_json::from_str::<SupabaseErrorBody>(&body).ok();
    let detail = parsed
        .as_ref()
        .and_then(|error| error.message.as_ref().or(error.msg.as_ref()).cloned())
        .unwrap_or_else(|| body.chars().take(500).collect());
    Err(map_supabase_error(status, &detail, parsed.as_ref()))
}

/// Maps stable SQL error markers and HTTP statuses to the public backend error model.
fn map_supabase_error(
    status: reqwest::StatusCode,
    message: &str,
    body: Option<&SupabaseErrorBody>,
) -> BackendError {
    let response_code = body
        .and_then(|value| value.code.as_ref())
        .map(serde_json::Value::to_string)
        .unwrap_or_default();
    let marker = format!(
        "{} {} {} {} {}",
        response_code,
        body.and_then(|value| value.error_code.as_deref()).unwrap_or_default(),
        message,
        body.and_then(|value| value.details.as_deref()).unwrap_or_default(),
        body.and_then(|value| value.hint.as_deref()).unwrap_or_default(),
    );
    if marker.contains("PGRST202") && marker.contains("stellarion_") {
        BackendError::Configuration(
            "the Stellarion database schema is missing; run supabase/schema.sql in the Supabase SQL Editor"
                .to_string(),
        )
    } else if marker.contains("anonymous_provider_disabled") {
        BackendError::Configuration(
            "anonymous sign-ins are disabled for the configured Supabase project".to_string(),
        )
    } else if status == reqwest::StatusCode::UNAUTHORIZED || marker.contains("STLR_UNAUTHENTICATED")
    {
        BackendError::Unauthenticated
    } else if status == reqwest::StatusCode::FORBIDDEN || marker.contains("STLR_FORBIDDEN") {
        BackendError::Forbidden
    } else if marker.contains("STLR_GAME_NOT_FOUND") {
        BackendError::GameNotFound
    } else if marker.contains("STLR_CODE_COLLISION") {
        BackendError::GameCodeCollision
    } else if marker.contains("STLR_GAME_FULL") {
        BackendError::GameFull
    } else if marker.contains("STLR_INVALID_STATUS") {
        BackendError::InvalidGameStatus
    } else if marker.contains("STLR_INVALID_RECOVERY") {
        BackendError::InvalidRecoveryCode
    } else if marker.contains("STLR_RECOVERY_IN_USE") {
        BackendError::RecoveryCodeInUse
    } else if marker.contains("STLR_ALREADY_MEMBER") {
        BackendError::AlreadyMember
    } else if marker.contains("STLR_PLAYER_REMOVED") {
        BackendError::PlayerNoLongerInGame
    } else if let Some((player_id, turn)) = marker_pair(&marker, "STLR_DUPLICATE_SUBMISSION:") {
        BackendError::DuplicateSubmission {
            player_id,
            turn,
        }
    } else if let Some((expected, actual)) = marker_pair(&marker, "STLR_CONFLICT:") {
        BackendError::Conflict {
            expected,
            actual,
        }
    } else if marker.contains("STLR_TURN_INCOMPLETE") {
        BackendError::TurnIncomplete
    } else if marker.contains("STLR_TURN_COMMITTED") {
        BackendError::TurnCommitted
    } else if let Some((expected, actual)) = marker_pair(&marker, "STLR_STALE_SUBMISSION:") {
        BackendError::StaleSubmission {
            expected,
            actual,
        }
    } else if let Some(detail) =
        marker.split("STLR_INVALID_DATA:").nth(1).and_then(|value| value.split_whitespace().next())
    {
        BackendError::InvalidData(detail.to_string())
    } else if status.is_server_error() {
        BackendError::Offline(format!("Supabase returned {status}"))
    } else {
        BackendError::Protocol(format!("Supabase returned {status}: {message}"))
    }
}

/// Parses two colon-separated unsigned values following a stable SQL marker.
fn marker_pair(marker: &str, prefix: &str) -> Option<(u64, u64)> {
    let values = marker.split(prefix).nth(1)?.split_whitespace().next()?;
    let mut values = values.split(':');
    let first = values.next()?.parse().ok()?;
    let second = values.next()?.parse().ok()?;
    (values.next().is_none()).then_some((first, second))
}

/// Classifies transport and browser-fetch failures as retryable offline errors.
fn network_error(error: reqwest::Error) -> BackendError {
    #[cfg(not(target_arch = "wasm32"))]
    let is_connect = error.is_connect();
    #[cfg(target_arch = "wasm32")]
    let is_connect = false;
    if is_connect || error.is_timeout() || error.is_request() {
        BackendError::Offline(error.to_string())
    } else {
        BackendError::Protocol(error.to_string())
    }
}

#[cfg(test)]
#[path = "../../tests/multiplayer/supabase.rs"]
mod tests;
