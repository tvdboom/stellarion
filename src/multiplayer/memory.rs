//! Deterministic in-memory backend used by tests and credential-free local development.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use bevy::platform::time::Instant;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::core::identity::{GameId, PlayerId, UserId};
use crate::core::player::PlayerColor;
use crate::core::simulation::{
    MatchStatus, PersistedGame, TurnSubmission, MAX_COMMANDS_PER_SUBMISSION,
};
use crate::multiplayer::authority::{
    initial_snapshot, resolved_snapshot, same_snapshot, started_snapshot_for_members,
    validate_incoming, validate_save,
};
use crate::multiplayer::backend::{
    BackendError, BackendFuture, MultiplayerBackend, PLAYER_CONNECTION_TIMEOUT,
};
use crate::multiplayer::model::{
    AuthSession, BackendEvent, BackendEventKind, CreateGameRequest, EventBatch, GameMembership,
    GameRecord, GameSummary, JoinDisposition, JoinGameRequest, MembershipResult,
    RecoverPlayerRequest, StoredTurnSubmission, SubmissionDisposition,
};
use crate::multiplayer::recovery::generate_user_token;

/// How long completed games and their related records are retained.
const FINISHED_GAME_RETENTION: Duration = Duration::from_secs(48 * 60 * 60);

#[cfg(not(target_arch = "wasm32"))]
fn current_unix_timestamp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

#[cfg(target_arch = "wasm32")]
fn current_unix_timestamp() -> u64 {
    let seconds = js_sys::Date::now() / 1_000.0;
    if seconds.is_finite() && seconds >= 0.0 {
        seconds as u64
    } else {
        0
    }
}

/// Thread-safe in-memory implementation with Supabase-like uniqueness and revision semantics.
#[derive(Clone, Default)]
pub struct InMemoryBackend {
    inner: Arc<Mutex<MemoryState>>,
}

#[derive(Default)]
/// All isolated users, games, codes, and session tokens owned by the mock backend.
struct MemoryState {
    next_game_id: u64,
    sessions: HashMap<String, UserId>,
    games: HashMap<GameId, StoredGame>,
    codes: HashMap<crate::core::identity::GameCode, GameId>,
}

/// Canonical mock game plus hidden recovery hashes, submissions, and durable events.
struct StoredGame {
    record: GameRecord,
    finished_at: Option<Instant>,
    recovery_hashes: HashMap<PlayerId, String>,
    submissions: BTreeMap<(u64, PlayerId), StoredTurnSubmission>,
    events: Vec<BackendEvent>,
    connected_players: HashMap<PlayerId, Instant>,
}

impl InMemoryBackend {
    /// Creates an isolated backend with no users or games.
    pub fn new() -> Self {
        Self::default()
    }

    /// Removes expired mock records on access; hosted cleanup runs on the database scheduler.
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, MemoryState>, BackendError> {
        let mut state = self.inner.lock().map_err(|_| {
            BackendError::Protocol("in-memory backend lock was poisoned".to_string())
        })?;
        state.games.retain(|_, stored| {
            stored.record.status != MatchStatus::Finished
                || stored.finished_at.is_none_or(|at| at.elapsed() < FINISHED_GAME_RETENTION)
        });
        // Project the heartbeat lease on every access, including loads and resume checks.
        // A client that vanishes cannot leave its saved connection flag true forever.
        let now = Instant::now();
        for stored in state.games.values_mut() {
            for member in &mut stored.record.members {
                member.connected =
                    stored.connected_players.get(&member.player_id).is_some_and(|last_seen| {
                        now.saturating_duration_since(*last_seen) < PLAYER_CONNECTION_TIMEOUT
                    });
            }
        }
        let MemoryState {
            games,
            codes,
            ..
        } = &mut *state;
        codes.retain(|_, game_id| games.contains_key(game_id));
        Ok(state)
    }
}

impl MultiplayerBackend for InMemoryBackend {
    /// Restores a known mock session or creates a new anonymous user.
    fn authenticate<'a>(
        &'a self,
        stored: Option<&'a AuthSession>,
    ) -> BackendFuture<'a, AuthSession> {
        Box::pin(async move {
            let mut state = self.lock()?;
            if let Some(session) = stored {
                if state.sessions.get(&session.access_token) == Some(&session.user_id) {
                    return Ok(session.clone());
                }
            }
            let token =
                generate_user_token().map_err(|error| BackendError::Protocol(error.to_string()))?;
            let user_id = UserId::new(format!("mock-user-{token}"));
            let access_token = format!("mock-access-{token}");
            state.sessions.insert(access_token.clone(), user_id.clone());
            Ok(AuthSession::new(user_id, access_token, format!("mock-refresh-{token}")))
        })
    }

    /// Verifies and returns the existing mock session without creating another user.
    fn refresh_session<'a>(&'a self, session: &'a AuthSession) -> BackendFuture<'a, AuthSession> {
        Box::pin(async move {
            let state = self.lock()?;
            authenticated_user(&state, session)?;
            Ok(session.clone())
        })
    }

    /// Creates an isolated lobby and slot-one membership.
    fn create_game<'a>(
        &'a self,
        session: &'a AuthSession,
        request: CreateGameRequest,
    ) -> BackendFuture<'a, MembershipResult> {
        Box::pin(async move {
            validate_name_and_hash(&request.display_name, &request.recovery_hash)?;
            request.persisted.validate().map_err(invalid_game)?;
            if request.persisted.state.status != MatchStatus::Lobby {
                return Err(BackendError::InvalidGameStatus);
            }

            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            if state.codes.contains_key(&request.code) {
                return Err(BackendError::GameCodeCollision);
            }
            state.next_game_id = state.next_game_id.saturating_add(1);
            let game_id = GameId::new(format!("memory-game-{}", state.next_game_id));
            let membership = GameMembership {
                game_id: game_id.clone(),
                player_id: 1,
                user_id,
                display_name: request.display_name.trim().to_string(),
                is_creator: true,
                identity_version: 1,
                connected: false,
            };
            let max_players = request.persisted.state.rules.player_count;
            let canonical = initial_snapshot(&request.persisted, request.persisted.state.rng.seed)?;
            let record = GameRecord {
                submitted_players: Vec::new(),
                id: game_id.clone(),
                code: request.code.clone(),
                revision: 0,
                saved_at: current_unix_timestamp(),
                max_players,
                status: MatchStatus::Lobby,
                persisted: canonical,
                members: vec![membership.clone()],
            };
            let mut stored = StoredGame {
                record,
                finished_at: None,
                recovery_hashes: HashMap::from([(1, request.recovery_hash)]),
                submissions: BTreeMap::new(),
                events: Vec::new(),
                connected_players: HashMap::new(),
            };
            push_event(&mut stored, BackendEventKind::PlayerJoined, None, Some(1));
            let game = stored.record.clone();
            state.codes.insert(request.code, game_id.clone());
            state.games.insert(game_id, stored);
            Ok(MembershipResult {
                game,
                membership,
                disposition: JoinDisposition::Joined,
            })
        })
    }

    /// Claims the lowest unoccupied player slot or returns an existing mapping.
    fn join_game<'a>(
        &'a self,
        session: &'a AuthSession,
        request: JoinGameRequest,
    ) -> BackendFuture<'a, MembershipResult> {
        Box::pin(async move {
            validate_name_and_hash(&request.display_name, &request.recovery_hash)?;
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let game_id =
                state.codes.get(&request.code).cloned().ok_or(BackendError::GameNotFound)?;
            let stored = state.games.get_mut(&game_id).ok_or(BackendError::GameNotFound)?;
            if let Some(existing) =
                stored.record.members.iter().find(|member| member.user_id == user_id).cloned()
            {
                return Ok(MembershipResult {
                    game: stored.record.clone(),
                    membership: existing,
                    disposition: JoinDisposition::Reconnected,
                });
            }
            if stored.record.status != MatchStatus::Lobby {
                return Err(BackendError::InvalidGameStatus);
            }
            if stored.record.members.len() >= usize::from(stored.record.max_players) {
                return Err(BackendError::GameFull);
            }
            let occupied =
                stored.record.members.iter().map(|member| member.player_id).collect::<HashSet<_>>();
            let player_id = (1..=u64::from(stored.record.max_players))
                .find(|candidate| !occupied.contains(candidate))
                .ok_or(BackendError::GameFull)?;
            let membership = GameMembership {
                game_id,
                player_id,
                user_id,
                display_name: request.display_name.trim().to_string(),
                is_creator: false,
                identity_version: 1,
                connected: false,
            };
            stored.record.members.push(membership.clone());
            stored.record.members.sort_by_key(|member| member.player_id);
            stored.recovery_hashes.insert(player_id, request.recovery_hash);
            push_event(stored, BackendEventKind::PlayerJoined, None, Some(player_id));
            Ok(MembershipResult {
                game: stored.record.clone(),
                membership,
                disposition: JoinDisposition::Joined,
            })
        })
    }

    /// Verifies and rotates a recovery hash before replacing the associated user.
    fn recover_player<'a>(
        &'a self,
        session: &'a AuthSession,
        request: RecoverPlayerRequest,
    ) -> BackendFuture<'a, MembershipResult> {
        Box::pin(async move {
            validate_hash(&request.recovery_hash)?;
            validate_hash(&request.replacement_recovery_hash)?;
            if request.recovery_hash == request.replacement_recovery_hash {
                return Err(BackendError::InvalidData("recovery_rotation".to_string()));
            }
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let game_id =
                state.codes.get(&request.code).cloned().ok_or(BackendError::GameNotFound)?;
            let stored = state.games.get_mut(&game_id).ok_or(BackendError::GameNotFound)?;
            if stored.record.members.iter().any(|member| member.user_id == user_id) {
                return Err(BackendError::AlreadyMember);
            }
            let supplied = request.recovery_hash.as_bytes();
            let player_id = stored
                .recovery_hashes
                .iter()
                .find_map(|(player_id, expected)| {
                    bool::from(expected.as_bytes().ct_eq(supplied)).then_some(*player_id)
                })
                .ok_or(BackendError::InvalidRecoveryCode)?;
            if stored
                .connected_players
                .get(&player_id)
                .is_some_and(|last_seen| last_seen.elapsed() < PLAYER_CONNECTION_TIMEOUT)
            {
                return Err(BackendError::RecoveryCodeInUse);
            }
            if stored
                .recovery_hashes
                .values()
                .any(|hash| hash == &request.replacement_recovery_hash)
            {
                return Err(BackendError::InvalidRecoveryCode);
            }
            let membership = stored
                .record
                .members
                .iter_mut()
                .find(|member| member.player_id == player_id)
                .ok_or(BackendError::PlayerNoLongerInGame)?;
            membership.user_id = user_id;
            membership.identity_version = membership.identity_version.saturating_add(1);
            membership.connected = true;
            let membership = membership.clone();
            stored.recovery_hashes.insert(player_id, request.replacement_recovery_hash);
            stored.connected_players.insert(player_id, Instant::now());
            push_event(stored, BackendEventKind::PlayerRecovered, None, Some(player_id));
            Ok(MembershipResult {
                game: stored.record.clone(),
                membership,
                disposition: JoinDisposition::Reconnected,
            })
        })
    }

    /// Lists started games for current memberships after expired games have been removed.
    fn list_games<'a>(&'a self, session: &'a AuthSession) -> BackendFuture<'a, Vec<GameSummary>> {
        Box::pin(async move {
            let state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let mut summaries = state
                .games
                .values()
                .filter(|stored| stored.record.status != MatchStatus::Lobby)
                .filter_map(|stored| {
                    let member =
                        stored.record.members.iter().find(|member| member.user_id == user_id)?;
                    Some(GameSummary {
                        id: stored.record.id.clone(),
                        code: stored.record.code.clone(),
                        revision: stored.record.revision,
                        saved_at: stored.record.saved_at,
                        status: stored.record.status,
                        turn: stored.record.persisted.state.turn,
                        player_id: member.player_id,
                        display_name: member.display_name.clone(),
                        player_color: stored
                            .record
                            .persisted
                            .state
                            .player(member.player_id)
                            .map_or_else(
                                |_| PlayerColor::for_player(member.player_id),
                                |player| player.color(),
                            ),
                        player_count: stored.record.members.len(),
                        max_players: stored.record.max_players,
                    })
                })
                .collect::<Vec<_>>();
            summaries.sort_by(|left, right| {
                right.saved_at.cmp(&left.saved_at).then_with(|| left.id.cmp(&right.id))
            });
            Ok(summaries)
        })
    }

    /// Loads state only for a current member.
    fn load_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
    ) -> BackendFuture<'a, GameRecord> {
        Box::pin(async move {
            let state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get(game_id).ok_or(BackendError::GameNotFound)?;
            authorize_member(stored, &user_id)?;
            Ok(stored.record.clone())
        })
    }

    /// Starts a lobby with its current contiguous member set when called by its creator.
    fn start_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord> {
        Box::pin(async move {
            persisted.validate().map_err(invalid_game)?;
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get_mut(game_id).ok_or(BackendError::GameNotFound)?;
            let member = authorize_member(stored, &user_id)?;
            let member_count = stored.record.members.len();
            let valid_member_count = if persisted.state.rules.practice_mode {
                member_count == 1 && stored.record.max_players == 1
            } else {
                (2..=usize::from(stored.record.max_players)).contains(&member_count)
            };
            if !member.is_creator
                || stored.record.status != MatchStatus::Lobby
                || !valid_member_count
                || persisted.state.status != MatchStatus::Active
                || usize::from(persisted.state.rules.player_count) != member_count
            {
                return Err(BackendError::InvalidGameStatus);
            }
            compare_revision(stored, expected_revision)?;
            let persisted = started_snapshot_for_members(&stored.record, persisted.state.rng.seed)?;
            stored.record.max_players = persisted.state.rules.player_count;
            commit_state(stored, persisted);
            push_event(stored, BackendEventKind::GameStarted, None, None);
            Ok(stored.record.clone())
        })
    }

    /// Releases a started match only after its host and every existing player reconnect.
    fn resume_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get_mut(game_id).ok_or(BackendError::GameNotFound)?;
            let member = authorize_member(stored, &user_id)?;
            if !member.is_creator {
                return Err(BackendError::Forbidden);
            }
            if stored.record.status != MatchStatus::Active
                || !stored.record.members.iter().all(|member| member.connected)
            {
                return Err(BackendError::InvalidGameStatus);
            }
            push_event(stored, BackendEventKind::GameResumed, None, None);
            Ok(())
        })
    }

    /// Saves from any member and rejects stale revisions.
    fn save_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord> {
        Box::pin(async move {
            persisted.validate().map_err(invalid_game)?;
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get_mut(game_id).ok_or(BackendError::GameNotFound)?;
            let player_id = authorize_member(stored, &user_id)?.player_id;
            compare_revision(stored, expected_revision)?;
            if persisted.state.status != stored.record.status {
                return Err(BackendError::InvalidGameStatus);
            }
            validate_save(&stored.record, player_id, &persisted)?;
            commit_state(stored, persisted);
            push_event(stored, BackendEventKind::StateChanged, None, None);
            Ok(stored.record.clone())
        })
    }

    /// Stores one submission and treats an identical retry as success.
    fn submit_turn<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        submission: TurnSubmission,
    ) -> BackendFuture<'a, SubmissionDisposition> {
        Box::pin(async move {
            if submission.commands.len() > MAX_COMMANDS_PER_SUBMISSION {
                return Err(BackendError::InvalidData(format!(
                    "turn submission exceeds {MAX_COMMANDS_PER_SUBMISSION} commands"
                )));
            }
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get_mut(game_id).ok_or(BackendError::GameNotFound)?;
            let member = authorize_member(stored, &user_id)?;
            if stored.record.status != MatchStatus::Active {
                return Err(BackendError::InvalidGameStatus);
            }
            if member.player_id != submission.player_id {
                return Err(BackendError::Forbidden);
            }
            let is_active_player = stored
                .record
                .persisted
                .state
                .players
                .iter()
                .any(|player| player.id == member.player_id && !player.spectator);
            if !is_active_player {
                return Err(BackendError::Forbidden);
            }
            if submission.turn != stored.record.persisted.state.turn {
                return Err(BackendError::StaleSubmission {
                    expected: stored.record.persisted.state.turn,
                    actual: submission.turn,
                });
            }
            let digest = submission_digest(&submission)?;
            let key = (submission.turn, submission.player_id);
            if let Some(existing) = stored.submissions.get(&key) {
                if existing.submission.generation != submission.generation
                    || (existing.ready && existing.digest != digest)
                {
                    return Err(BackendError::DuplicateSubmission {
                        player_id: submission.player_id,
                        turn: submission.turn,
                    });
                }
                if existing.ready {
                    return Ok(SubmissionDisposition::Duplicate);
                }
            } else if submission.generation != 0 {
                return Err(BackendError::InvalidData("unknown readiness generation".into()));
            }
            let existing = stored
                .submissions
                .range((submission.turn, 0)..=(submission.turn, PlayerId::MAX))
                .filter(|(_, stored)| stored.ready)
                .map(|(_, stored)| stored.submission.clone())
                .collect::<Vec<_>>();
            validate_incoming(&stored.record, &existing, &submission)?;
            let player_id = submission.player_id;
            let turn = submission.turn;
            stored.submissions.insert(
                key,
                StoredTurnSubmission {
                    submission,
                    digest,
                    ready: true,
                },
            );
            stored.record.submitted_players.push(player_id);
            stored.record.submitted_players.sort_unstable();
            push_event(stored, BackendEventKind::TurnSubmitted, Some(turn), Some(player_id));
            Ok(SubmissionDisposition::Inserted)
        })
    }

    /// Withdraws readiness under the same lock as the final ready and resolution writes.
    fn withdraw_turn<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        turn: u64,
        generation: u64,
    ) -> BackendFuture<'a, TurnSubmission> {
        Box::pin(async move {
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get_mut(game_id).ok_or(BackendError::GameNotFound)?;
            let player_id = authorize_member(stored, &user_id)?.player_id;
            if stored.record.status != MatchStatus::Active {
                return Err(BackendError::InvalidGameStatus);
            }
            if !stored
                .record
                .persisted
                .state
                .players
                .iter()
                .any(|p| p.id == player_id && !p.spectator)
            {
                return Err(BackendError::Forbidden);
            }
            if turn != stored.record.persisted.state.turn {
                return Err(BackendError::StaleSubmission {
                    expected: stored.record.persisted.state.turn,
                    actual: turn,
                });
            }
            let key = (turn, player_id);
            let mut draft = if let Some(existing) = stored.submissions.get(&key) {
                if !existing.ready && generation <= existing.submission.generation {
                    return Ok(existing.submission.clone());
                }
                if generation != existing.submission.generation {
                    return Err(BackendError::DuplicateSubmission {
                        player_id,
                        turn,
                    });
                }
                existing.submission.clone()
            } else {
                if generation != 0 {
                    return Err(BackendError::InvalidData("unknown readiness generation".into()));
                }
                TurnSubmission::new(player_id, turn, Vec::new())
            };
            if stored
                .record
                .persisted
                .state
                .players
                .iter()
                .filter(|p| !p.spectator)
                .all(|p| stored.submissions.get(&(turn, p.id)).is_some_and(|s| s.ready))
            {
                return Err(BackendError::TurnCommitted);
            }
            draft.generation = generation
                .checked_add(1)
                .filter(|g| *g <= i64::MAX as u64)
                .ok_or_else(|| BackendError::InvalidData("readiness generation overflow".into()))?;
            // Keep a withdrawn draft so late ready requests cannot resurrect it and a
            // reconnecting player can recover their orders even after a lost response.
            stored.submissions.insert(
                key,
                StoredTurnSubmission {
                    digest: submission_digest(&draft)?,
                    submission: draft.clone(),
                    ready: false,
                },
            );
            stored.record.submitted_players.retain(|id| *id != player_id);
            push_event(stored, BackendEventKind::TurnWithdrawn, Some(turn), Some(player_id));
            Ok(draft)
        })
    }

    /// Returns persisted submissions in stable player order.
    fn load_turn_submissions<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        turn: u64,
    ) -> BackendFuture<'a, Vec<StoredTurnSubmission>> {
        Box::pin(async move {
            let state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get(game_id).ok_or(BackendError::GameNotFound)?;
            authorize_member(stored, &user_id)?;
            Ok(stored
                .submissions
                .range((turn, 0)..=(turn, PlayerId::MAX))
                .map(|(_, submission)| submission.clone())
                .collect())
        })
    }

    /// Publishes one accepted next state; competing resolvers lose the revision race.
    fn publish_resolution<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        resolved_turn: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord> {
        Box::pin(async move {
            persisted.validate().map_err(invalid_game)?;
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get_mut(game_id).ok_or(BackendError::GameNotFound)?;
            authorize_member(stored, &user_id)?;
            compare_revision(stored, expected_revision)?;
            if stored.record.status != MatchStatus::Active
                || stored.record.persisted.state.turn != resolved_turn
                || persisted.state.turn != resolved_turn.saturating_add(1)
            {
                return Err(BackendError::StaleSubmission {
                    expected: stored.record.persisted.state.turn,
                    actual: resolved_turn,
                });
            }
            let required = stored
                .record
                .persisted
                .state
                .players
                .iter()
                .filter(|player| !player.spectator)
                .map(|player| player.id)
                .collect::<HashSet<_>>();
            let submitted = stored
                .submissions
                .range((resolved_turn, 0)..=(resolved_turn, PlayerId::MAX))
                .filter(|(_, stored)| stored.ready)
                .map(|((_, player_id), _)| *player_id)
                .collect::<HashSet<_>>();
            if required != submitted {
                return Err(BackendError::TurnIncomplete);
            }
            let submissions = stored
                .submissions
                .range((resolved_turn, 0)..=(resolved_turn, PlayerId::MAX))
                .filter(|(_, stored)| stored.ready)
                .map(|(_, stored)| stored.submission.clone())
                .collect::<Vec<_>>();
            let canonical = resolved_snapshot(&stored.record, &submissions)?;
            if !same_snapshot(&canonical, &persisted)? {
                return Err(BackendError::Forbidden);
            }
            let finished = persisted.state.status == MatchStatus::Finished;
            stored.record.submitted_players.clear();
            commit_state(stored, persisted);
            push_event(
                stored,
                if finished {
                    BackendEventKind::GameFinished
                } else {
                    BackendEventKind::TurnResolved
                },
                Some(resolved_turn.saturating_add(1)),
                None,
            );
            let oldest_retained = resolved_turn.saturating_sub(8);
            stored.submissions.retain(|(turn, _), _| *turn >= oldest_retained);
            Ok(stored.record.clone())
        })
    }

    /// Returns all missed events after a caller-owned cursor.
    fn subscribe<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        after_sequence: u64,
    ) -> BackendFuture<'a, EventBatch> {
        Box::pin(async move {
            let state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get(game_id).ok_or(BackendError::GameNotFound)?;
            authorize_member(stored, &user_id)?;
            let events = stored
                .events
                .iter()
                .filter(|event| event.sequence > after_sequence)
                .take(256)
                .cloned()
                .collect::<Vec<_>>();
            let cursor = events.last().map_or(after_sequence, |event| event.sequence);
            Ok(EventBatch {
                events,
                cursor,
            })
        })
    }

    /// Deletes a lobby when its host leaves; started games retain their memberships.
    fn set_connected<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        connected: bool,
    ) -> BackendFuture<'a, ()> {
        Box::pin(async move {
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get_mut(game_id).ok_or(BackendError::GameNotFound)?;
            let member = authorize_member(stored, &user_id)?;
            if !connected && stored.record.status == MatchStatus::Lobby && member.is_creator {
                let code = stored.record.code.clone();
                state.games.remove(game_id);
                state.codes.remove(&code);
                return Ok(());
            }
            let player_id = member.player_id;
            let changed = member.connected != connected;
            if connected {
                stored.connected_players.insert(player_id, Instant::now());
            } else {
                stored.connected_players.remove(&player_id);
            }
            if let Some(member) =
                stored.record.members.iter_mut().find(|member| member.player_id == player_id)
            {
                member.connected = connected;
            }
            if changed {
                push_event(
                    stored,
                    if connected {
                        BackendEventKind::PlayerConnected
                    } else {
                        BackendEventKind::PlayerDisconnected
                    },
                    None,
                    Some(player_id),
                );
            }
            Ok(())
        })
    }
}

/// Validates a session token against the in-memory session registry.
fn authenticated_user(state: &MemoryState, session: &AuthSession) -> Result<UserId, BackendError> {
    match state.sessions.get(&session.access_token) {
        Some(user_id) if user_id == &session.user_id => Ok(user_id.clone()),
        _ => Err(BackendError::Unauthenticated),
    }
}

/// Returns the caller's membership or a non-enumerating authorization error.
fn authorize_member<'a>(
    stored: &'a StoredGame,
    user_id: &UserId,
) -> Result<&'a GameMembership, BackendError> {
    stored
        .record
        .members
        .iter()
        .find(|member| &member.user_id == user_id)
        .ok_or(BackendError::Forbidden)
}

/// Validates lobby display names and recovery hashes.
fn validate_name_and_hash(display_name: &str, recovery_hash: &str) -> Result<(), BackendError> {
    let length = display_name.trim().chars().count();
    if !(1..=32).contains(&length) {
        return Err(BackendError::InvalidData(
            "display name must contain 1..=32 characters".to_string(),
        ));
    }
    validate_hash(recovery_hash)
}

/// Ensures only complete SHA-256 hex hashes enter storage.
fn validate_hash(hash: &str) -> Result<(), BackendError> {
    if hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(BackendError::InvalidData("recovery hash must be a SHA-256 hex value".to_string()))
    }
}

/// Converts core validation failures into transport-neutral data errors.
fn invalid_game(error: crate::core::simulation::GameError) -> BackendError {
    BackendError::InvalidData(error.to_string())
}

/// Enforces optimistic concurrency for every state write.
fn compare_revision(stored: &StoredGame, expected: u64) -> Result<(), BackendError> {
    if stored.record.revision == expected {
        Ok(())
    } else {
        Err(BackendError::Conflict {
            expected,
            actual: stored.record.revision,
        })
    }
}

/// Commits validated state and increments the revision exactly once.
fn commit_state(stored: &mut StoredGame, persisted: PersistedGame) {
    if persisted.state.status == MatchStatus::Finished && stored.finished_at.is_none() {
        stored.finished_at = Some(Instant::now());
    }
    stored.record.revision = stored.record.revision.saturating_add(1);
    stored.record.saved_at = current_unix_timestamp();
    stored.record.status = persisted.state.status;
    stored.record.persisted = persisted;
}

/// Computes the idempotency digest used by the unique submission row.
fn submission_digest(submission: &TurnSubmission) -> Result<String, BackendError> {
    let bytes = serde_json::to_vec(submission)
        .map_err(|error| BackendError::InvalidData(error.to_string()))?;
    let mut hasher = Sha256::new();
    hasher.update(b"stellarion-turn-submission-v1");
    hasher.update(bytes);
    Ok(hex::encode(hasher.finalize()))
}

/// Appends an ordered notification that can later be replayed after reconnect.
fn push_event(
    stored: &mut StoredGame,
    kind: BackendEventKind,
    turn: Option<u64>,
    player_id: Option<PlayerId>,
) {
    let sequence = stored.events.last().map_or(1, |event| event.sequence.saturating_add(1));
    stored.events.push(BackendEvent {
        sequence,
        game_id: stored.record.id.clone(),
        kind,
        revision: Some(stored.record.revision),
        turn,
        player_id,
    });
    let excess = stored.events.len().saturating_sub(2_048);
    if excess > 0 {
        stored.events.drain(..excess);
    }
}

#[cfg(test)]
#[path = "../../tests/multiplayer/memory.rs"]
mod tests;
