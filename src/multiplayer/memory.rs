//! Deterministic in-memory backend used by tests and credential-free local development.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::core::identity::{GameId, PlayerId, UserId};
use crate::core::simulation::{
    MatchStatus, PersistedGame, TurnSubmission, MAX_COMMANDS_PER_SUBMISSION,
};
use crate::multiplayer::backend::{BackendError, BackendFuture, MultiplayerBackend};
use crate::multiplayer::model::{
    AuthSession, BackendEvent, BackendEventKind, CreateGameRequest, EventBatch, GameMembership,
    GameRecord, GameSummary, JoinDisposition, JoinGameRequest, MembershipResult,
    RecoverPlayerRequest, StoredTurnSubmission, SubmissionDisposition,
};
use crate::multiplayer::recovery::generate_user_token;

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
    recovery_hashes: HashMap<PlayerId, String>,
    submissions: BTreeMap<(u64, PlayerId), StoredTurnSubmission>,
    events: Vec<BackendEvent>,
    connected_players: HashSet<PlayerId>,
}

impl InMemoryBackend {
    /// Creates an isolated backend with no users or games.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns a lock error as a protocol failure rather than panicking.
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, MemoryState>, BackendError> {
        self.inner
            .lock()
            .map_err(|_| BackendError::Protocol("in-memory backend lock was poisoned".to_string()))
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
            request.persisted.state.validate().map_err(invalid_game)?;
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
            };
            let max_players = request.persisted.state.rules.player_count;
            let record = GameRecord {
                id: game_id.clone(),
                code: request.code.clone(),
                revision: 0,
                max_players,
                status: MatchStatus::Lobby,
                persisted: request.persisted,
                members: vec![membership.clone()],
            };
            let mut stored = StoredGame {
                record,
                recovery_hashes: HashMap::from([(1, request.recovery_hash)]),
                submissions: BTreeMap::new(),
                events: Vec::new(),
                connected_players: HashSet::new(),
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
            let membership = stored
                .record
                .members
                .iter_mut()
                .find(|member| member.player_id == player_id)
                .ok_or(BackendError::PlayerNoLongerInGame)?;
            membership.user_id = user_id;
            membership.identity_version = membership.identity_version.saturating_add(1);
            let membership = membership.clone();
            stored.recovery_hashes.insert(player_id, request.replacement_recovery_hash);
            stored.connected_players.remove(&player_id);
            push_event(stored, BackendEventKind::PlayerRecovered, None, Some(player_id));
            Ok(MembershipResult {
                game: stored.record.clone(),
                membership,
                disposition: JoinDisposition::Reconnected,
            })
        })
    }

    /// Lists only games for which the calling identity has a current mapping.
    fn list_games<'a>(&'a self, session: &'a AuthSession) -> BackendFuture<'a, Vec<GameSummary>> {
        Box::pin(async move {
            let state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let mut summaries = state
                .games
                .values()
                .filter_map(|stored| {
                    let member =
                        stored.record.members.iter().find(|member| member.user_id == user_id)?;
                    Some(GameSummary {
                        id: stored.record.id.clone(),
                        code: stored.record.code.clone(),
                        revision: stored.record.revision,
                        status: stored.record.status,
                        turn: stored.record.persisted.state.turn,
                        player_id: member.player_id,
                        player_count: stored.record.members.len(),
                        max_players: stored.record.max_players,
                    })
                })
                .collect::<Vec<_>>();
            summaries.sort_by(|left, right| left.id.cmp(&right.id));
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

    /// Starts a full lobby when called by its creator.
    fn start_game<'a>(
        &'a self,
        session: &'a AuthSession,
        game_id: &'a GameId,
        expected_revision: u64,
        persisted: PersistedGame,
    ) -> BackendFuture<'a, GameRecord> {
        Box::pin(async move {
            persisted.state.validate().map_err(invalid_game)?;
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get_mut(game_id).ok_or(BackendError::GameNotFound)?;
            let member = authorize_member(stored, &user_id)?;
            if !member.is_creator
                || stored.record.status != MatchStatus::Lobby
                || stored.record.members.len() != usize::from(stored.record.max_players)
                || persisted.state.status != MatchStatus::Active
            {
                return Err(BackendError::InvalidGameStatus);
            }
            compare_revision(stored, expected_revision)?;
            commit_state(stored, persisted);
            push_event(stored, BackendEventKind::GameStarted, None, None);
            Ok(stored.record.clone())
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
            persisted.state.validate().map_err(invalid_game)?;
            let mut state = self.lock()?;
            let user_id = authenticated_user(&state, session)?;
            let stored = state.games.get_mut(game_id).ok_or(BackendError::GameNotFound)?;
            authorize_member(stored, &user_id)?;
            compare_revision(stored, expected_revision)?;
            if persisted.state.status != stored.record.status {
                return Err(BackendError::InvalidGameStatus);
            }
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
                return if existing.digest == digest {
                    Ok(SubmissionDisposition::Duplicate)
                } else {
                    Err(BackendError::DuplicateSubmission {
                        player_id: submission.player_id,
                        turn: submission.turn,
                    })
                };
            }
            let player_id = submission.player_id;
            let turn = submission.turn;
            stored.submissions.insert(
                key,
                StoredTurnSubmission {
                    submission,
                    digest,
                },
            );
            push_event(stored, BackendEventKind::TurnSubmitted, Some(turn), Some(player_id));
            Ok(SubmissionDisposition::Inserted)
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
            persisted.state.validate().map_err(invalid_game)?;
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
                .map(|((_, player_id), _)| *player_id)
                .collect::<HashSet<_>>();
            if required != submitted {
                return Err(BackendError::TurnIncomplete);
            }
            let finished = persisted.state.status == MatchStatus::Finished;
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

    /// Emits coarse connection presence for lobby/status UI.
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
            let player_id = authorize_member(stored, &user_id)?.player_id;
            let changed = if connected {
                stored.connected_players.insert(player_id)
            } else {
                stored.connected_players.remove(&player_id)
            };
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
    stored.record.revision = stored.record.revision.saturating_add(1);
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
mod tests {
    use futures_lite::future::block_on;

    use super::*;
    use crate::core::identity::GameCode;
    use crate::core::simulation::{resolve_turn, GameModel, GameRules};
    use crate::multiplayer::recovery::{generate_game_code, RecoveryCode};

    /// Creates a session and a matching recovery credential.
    fn identity(backend: &InMemoryBackend) -> (AuthSession, RecoveryCode) {
        (block_on(backend.authenticate(None)).unwrap(), RecoveryCode::generate().unwrap())
    }

    /// Creates a game with the requested exact player capacity.
    fn create(
        backend: &InMemoryBackend,
        session: &AuthSession,
        recovery: &RecoveryCode,
        count: u8,
    ) -> MembershipResult {
        let model = GameModel::new(
            [count; 32],
            GameRules {
                player_count: count,
                ..GameRules::default()
            },
        )
        .unwrap();
        block_on(backend.create_game(
            session,
            CreateGameRequest {
                code: generate_game_code().unwrap(),
                display_name: "Creator".to_string(),
                recovery_hash: recovery.hash().0,
                persisted: PersistedGame::new(model),
            },
        ))
        .unwrap()
    }

    #[test]
    /// Covers creation, 2/3/4-player joining, duplicate reconnect, and full lobbies.
    fn supports_all_lobby_sizes_and_duplicate_joining() {
        for count in 2..=4 {
            let backend = InMemoryBackend::new();
            let (creator, creator_recovery) = identity(&backend);
            let created = create(&backend, &creator, &creator_recovery, count);
            for slot in 2..=count {
                let (session, recovery) = identity(&backend);
                let joined = block_on(backend.join_game(
                    &session,
                    JoinGameRequest {
                        code: created.game.code.clone(),
                        display_name: format!("Player {slot}"),
                        recovery_hash: recovery.hash().0,
                    },
                ))
                .unwrap();
                assert_eq!(joined.membership.player_id, u64::from(slot));
                let duplicate = block_on(backend.join_game(
                    &session,
                    JoinGameRequest {
                        code: created.game.code.clone(),
                        display_name: "Ignored".to_string(),
                        recovery_hash: recovery.hash().0,
                    },
                ))
                .unwrap();
                assert_eq!(duplicate.disposition, JoinDisposition::Reconnected);
            }
            let (extra, extra_recovery) = identity(&backend);
            assert!(matches!(
                block_on(backend.join_game(
                    &extra,
                    JoinGameRequest {
                        code: created.game.code,
                        display_name: "Extra".to_string(),
                        recovery_hash: extra_recovery.hash().0,
                    },
                )),
                Err(BackendError::GameFull)
            ));
        }
    }

    #[test]
    /// Restores the same anonymous identity and automatically finds its player slot.
    fn reconnect_uses_authenticated_mapping() {
        let backend = InMemoryBackend::new();
        let (session, recovery) = identity(&backend);
        let created = create(&backend, &session, &recovery, 2);
        let restored = block_on(backend.authenticate(Some(&session))).unwrap();
        let games = block_on(backend.list_games(&restored)).unwrap();
        assert_eq!(games.len(), 1);
        assert_eq!(games[0].player_id, created.membership.player_id);
    }

    #[test]
    /// Recovery replaces the user, rotates the secret, and invalidates old access.
    fn recovers_from_another_identity_and_rotates_code() {
        let backend = InMemoryBackend::new();
        let (old_session, recovery) = identity(&backend);
        let created = create(&backend, &old_session, &recovery, 2);
        let (new_session, replacement) = identity(&backend);
        let recovered = block_on(backend.recover_player(
            &new_session,
            RecoverPlayerRequest {
                code: created.game.code.clone(),
                recovery_hash: recovery.hash().0.clone(),
                replacement_recovery_hash: replacement.hash().0,
            },
        ))
        .unwrap();
        assert_eq!(recovered.membership.user_id, new_session.user_id);
        assert_eq!(recovered.membership.identity_version, 2);
        assert!(matches!(
            block_on(backend.load_game(&old_session, &created.game.id)),
            Err(BackendError::Forbidden)
        ));
        let (third_session, third_replacement) = identity(&backend);
        assert!(matches!(
            block_on(backend.recover_player(
                &third_session,
                RecoverPlayerRequest {
                    code: created.game.code,
                    recovery_hash: recovery.hash().0,
                    replacement_recovery_hash: third_replacement.hash().0,
                },
            )),
            Err(BackendError::InvalidRecoveryCode)
        ));
    }

    #[test]
    /// Recovery distinguishes unknown games, malformed hashes, existing members, and bad secrets.
    fn reports_typed_recovery_failures() {
        let backend = InMemoryBackend::new();
        let (creator, recovery) = identity(&backend);
        let created = create(&backend, &creator, &recovery, 2);
        let (stranger, replacement) = identity(&backend);

        assert!(matches!(
            block_on(backend.recover_player(
                &stranger,
                RecoverPlayerRequest {
                    code: GameCode::new("ABCDEF"),
                    recovery_hash: recovery.hash().0.clone(),
                    replacement_recovery_hash: replacement.hash().0.clone(),
                },
            )),
            Err(BackendError::GameNotFound)
        ));
        assert!(matches!(
            block_on(backend.recover_player(
                &stranger,
                RecoverPlayerRequest {
                    code: created.game.code.clone(),
                    recovery_hash: "not-a-sha256-hash".to_string(),
                    replacement_recovery_hash: replacement.hash().0.clone(),
                },
            )),
            Err(BackendError::InvalidData(_))
        ));
        assert!(matches!(
            block_on(backend.recover_player(
                &creator,
                RecoverPlayerRequest {
                    code: created.game.code.clone(),
                    recovery_hash: recovery.hash().0.clone(),
                    replacement_recovery_hash: replacement.hash().0.clone(),
                },
            )),
            Err(BackendError::AlreadyMember)
        ));
        let unrelated = RecoveryCode::generate().unwrap();
        assert!(matches!(
            block_on(backend.recover_player(
                &stranger,
                RecoverPlayerRequest {
                    code: created.game.code,
                    recovery_hash: unrelated.hash().0,
                    replacement_recovery_hash: replacement.hash().0,
                },
            )),
            Err(BackendError::InvalidRecoveryCode)
        ));
    }

    #[test]
    /// Any member may save, while simultaneous stale writes are rejected.
    fn saves_by_multiple_players_use_optimistic_revisions() {
        let backend = InMemoryBackend::new();
        let (creator, creator_recovery) = identity(&backend);
        let created = create(&backend, &creator, &creator_recovery, 2);
        let (joiner, joiner_recovery) = identity(&backend);
        block_on(backend.join_game(
            &joiner,
            JoinGameRequest {
                code: created.game.code,
                display_name: "Joiner".to_string(),
                recovery_hash: joiner_recovery.hash().0,
            },
        ))
        .unwrap();
        let loaded = block_on(backend.load_game(&creator, &created.game.id)).unwrap();
        let creator_saved = block_on(backend.save_game(
            &creator,
            &created.game.id,
            loaded.revision,
            loaded.persisted.clone(),
        ))
        .unwrap();
        let saved = block_on(backend.save_game(
            &joiner,
            &created.game.id,
            creator_saved.revision,
            loaded.persisted.clone(),
        ))
        .unwrap();
        assert_eq!(saved.revision, loaded.revision + 2);
        assert!(matches!(
            block_on(backend.save_game(
                &creator,
                &created.game.id,
                loaded.revision,
                loaded.persisted,
            )),
            Err(BackendError::Conflict { expected, actual })
                if expected == loaded.revision && actual == saved.revision
        ));
    }

    #[test]
    /// A saved snapshot is discoverable and exact after restoring the member's local session.
    fn resumes_exact_saved_state() {
        let backend = InMemoryBackend::new();
        let (creator, recovery) = identity(&backend);
        let created = create(&backend, &creator, &recovery, 2);
        let mut changed = created.game.persisted.clone();
        changed.state.players[0].resources.metal = 42_424;
        let saved =
            block_on(backend.save_game(&creator, &created.game.id, created.game.revision, changed))
                .unwrap();

        let restored = block_on(backend.authenticate(Some(&creator))).unwrap();
        let listed = block_on(backend.list_games(&restored)).unwrap();
        assert_eq!(listed[0].revision, saved.revision);
        let resumed = block_on(backend.load_game(&restored, &created.game.id)).unwrap();
        assert_eq!(resumed.persisted.state.players[0].resources.metal, 42_424);
    }

    #[test]
    /// Invalid complete-state snapshots are rejected before they can advance a revision.
    fn rejects_malformed_saved_state() {
        let backend = InMemoryBackend::new();
        let (creator, recovery) = identity(&backend);
        let created = create(&backend, &creator, &recovery, 2);
        let mut malformed = created.game.persisted.clone();
        malformed.state.players.pop();
        assert!(matches!(
            block_on(backend.save_game(
                &creator,
                &created.game.id,
                created.game.revision,
                malformed,
            )),
            Err(BackendError::InvalidData(_))
        ));
        assert_eq!(
            block_on(backend.load_game(&creator, &created.game.id)).unwrap().revision,
            created.game.revision
        );
    }

    #[test]
    /// Two genuinely concurrent saves serialize to one success and one revision conflict.
    fn simultaneous_saves_accept_exactly_one_writer() {
        use std::sync::{Arc, Barrier};

        let backend = InMemoryBackend::new();
        let (creator, recovery) = identity(&backend);
        let created = create(&backend, &creator, &recovery, 2);
        let barrier = Arc::new(Barrier::new(3));
        let mut workers = Vec::new();
        for _ in 0..2 {
            let backend = backend.clone();
            let creator = creator.clone();
            let game_id = created.game.id.clone();
            let persisted = created.game.persisted.clone();
            let barrier = Arc::clone(&barrier);
            workers.push(std::thread::spawn(move || {
                barrier.wait();
                block_on(backend.save_game(&creator, &game_id, 0, persisted))
            }));
        }
        barrier.wait();
        let results = workers.into_iter().map(|worker| worker.join().unwrap()).collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(BackendError::Conflict { .. })))
                .count(),
            1
        );
    }

    #[test]
    /// Duplicate/stale submissions and competing resolvers have deterministic outcomes.
    fn coordinates_idempotent_submission_and_single_resolution() {
        let backend = InMemoryBackend::new();
        let (creator, creator_recovery) = identity(&backend);
        let created = create(&backend, &creator, &creator_recovery, 2);
        let (joiner, joiner_recovery) = identity(&backend);
        let joined = block_on(backend.join_game(
            &joiner,
            JoinGameRequest {
                code: created.game.code,
                display_name: "Joiner".to_string(),
                recovery_hash: joiner_recovery.hash().0,
            },
        ))
        .unwrap();
        let mut lobby = joined.game;
        lobby.persisted.state.start().unwrap();
        let active =
            block_on(backend.start_game(&creator, &lobby.id, lobby.revision, lobby.persisted))
                .unwrap();
        let first = TurnSubmission::new(1, active.persisted.state.turn, Vec::new());
        let second = TurnSubmission::new(2, active.persisted.state.turn, Vec::new());
        assert_eq!(
            block_on(backend.submit_turn(&creator, &active.id, first.clone())).unwrap(),
            SubmissionDisposition::Inserted
        );
        assert_eq!(
            block_on(backend.submit_turn(&creator, &active.id, first)).unwrap(),
            SubmissionDisposition::Duplicate
        );
        block_on(backend.submit_turn(&joiner, &active.id, second)).unwrap();
        let submissions = block_on(backend.load_turn_submissions(
            &creator,
            &active.id,
            active.persisted.state.turn,
        ))
        .unwrap();
        let mut next = active.persisted.state.clone();
        resolve_turn(
            &mut next,
            &submissions.iter().map(|stored| stored.submission.clone()).collect::<Vec<_>>(),
        )
        .unwrap();
        let accepted = block_on(backend.publish_resolution(
            &joiner,
            &active.id,
            active.revision,
            active.persisted.state.turn,
            PersistedGame::new(next.clone()),
        ))
        .unwrap();
        assert_eq!(accepted.persisted.state.turn, active.persisted.state.turn + 1);
        assert!(matches!(
            block_on(backend.publish_resolution(
                &creator,
                &active.id,
                active.revision,
                active.persisted.state.turn,
                PersistedGame::new(next),
            )),
            Err(BackendError::Conflict { .. })
        ));
        let stale = TurnSubmission::new(2, active.persisted.state.turn, Vec::new());
        assert!(matches!(
            block_on(backend.submit_turn(&joiner, &active.id, stale)),
            Err(BackendError::StaleSubmission { .. })
        ));
    }

    #[test]
    /// Concurrent player submissions persist once and concurrent resolvers accept one next state.
    fn simultaneous_submissions_and_resolvers_are_serialized() {
        use std::sync::{Arc, Barrier};

        let backend = InMemoryBackend::new();
        let (creator, creator_recovery) = identity(&backend);
        let created = create(&backend, &creator, &creator_recovery, 2);
        let (joiner, joiner_recovery) = identity(&backend);
        let joined = block_on(backend.join_game(
            &joiner,
            JoinGameRequest {
                code: created.game.code,
                display_name: "Joiner".to_string(),
                recovery_hash: joiner_recovery.hash().0,
            },
        ))
        .unwrap();
        let mut lobby = joined.game;
        lobby.persisted.state.start().unwrap();
        let active =
            block_on(backend.start_game(&creator, &lobby.id, lobby.revision, lobby.persisted))
                .unwrap();

        let barrier = Arc::new(Barrier::new(3));
        let mut submitters = Vec::new();
        for (session, player_id) in [(creator.clone(), 1), (joiner.clone(), 2)] {
            let backend = backend.clone();
            let game_id = active.id.clone();
            let barrier = Arc::clone(&barrier);
            let turn = active.persisted.state.turn;
            submitters.push(std::thread::spawn(move || {
                barrier.wait();
                block_on(backend.submit_turn(
                    &session,
                    &game_id,
                    TurnSubmission::new(player_id, turn, Vec::new()),
                ))
            }));
        }
        barrier.wait();
        for submitter in submitters {
            assert_eq!(submitter.join().unwrap().unwrap(), SubmissionDisposition::Inserted);
        }

        let submissions = block_on(backend.load_turn_submissions(
            &creator,
            &active.id,
            active.persisted.state.turn,
        ))
        .unwrap();
        let mut model = active.persisted.state.clone();
        resolve_turn(
            &mut model,
            &submissions.into_iter().map(|stored| stored.submission).collect::<Vec<_>>(),
        )
        .unwrap();
        let next = PersistedGame::new(model);

        let barrier = Arc::new(Barrier::new(3));
        let mut resolvers = Vec::new();
        for session in [creator, joiner] {
            let backend = backend.clone();
            let game_id = active.id.clone();
            let next = next.clone();
            let barrier = Arc::clone(&barrier);
            let revision = active.revision;
            let turn = active.persisted.state.turn;
            resolvers.push(std::thread::spawn(move || {
                barrier.wait();
                block_on(backend.publish_resolution(&session, &game_id, revision, turn, next))
            }));
        }
        barrier.wait();
        let results =
            resolvers.into_iter().map(|resolver| resolver.join().unwrap()).collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| matches!(result, Err(BackendError::Conflict { .. })))
                .count(),
            1
        );
    }

    #[test]
    /// An eliminated slot remains a member for resume/view access but cannot submit a turn.
    fn spectator_cannot_submit_turn() {
        let backend = InMemoryBackend::new();
        let (creator, creator_recovery) = identity(&backend);
        let created = create(&backend, &creator, &creator_recovery, 2);
        let (joiner, joiner_recovery) = identity(&backend);
        let joined = block_on(backend.join_game(
            &joiner,
            JoinGameRequest {
                code: created.game.code,
                display_name: "Joiner".to_string(),
                recovery_hash: joiner_recovery.hash().0,
            },
        ))
        .unwrap();
        let mut lobby = joined.game;
        let defeated_home = lobby.persisted.state.players[1].home_planet;
        let home = lobby.persisted.state.map.get_mut(defeated_home);
        home.owned = Some(1);
        home.controlled = Some(1);
        lobby.persisted.state.players[1].spectator = true;
        lobby.persisted.state.start().unwrap();
        let active =
            block_on(backend.start_game(&creator, &lobby.id, lobby.revision, lobby.persisted))
                .unwrap();

        assert!(matches!(
            block_on(backend.submit_turn(
                &joiner,
                &active.id,
                TurnSubmission::new(2, active.persisted.state.turn, Vec::new()),
            )),
            Err(BackendError::Forbidden)
        ));
    }

    #[test]
    /// A reconnecting client can replay missed events and then reload current state.
    fn reconnect_replays_notifications_and_loads_current_state() {
        let backend = InMemoryBackend::new();
        let (creator, recovery) = identity(&backend);
        let created = create(&backend, &creator, &recovery, 2);
        let initial = block_on(backend.subscribe(&creator, &created.game.id, 0)).unwrap();
        block_on(backend.set_connected(&creator, &created.game.id, true)).unwrap();
        block_on(backend.set_connected(&creator, &created.game.id, false)).unwrap();
        block_on(backend.set_connected(&creator, &created.game.id, false)).unwrap();
        let caught_up =
            block_on(backend.subscribe(&creator, &created.game.id, initial.cursor)).unwrap();
        assert_eq!(caught_up.events.len(), 2);
        assert_eq!(
            block_on(backend.load_game(&creator, &created.game.id)).unwrap().revision,
            created.game.revision
        );
    }
}
