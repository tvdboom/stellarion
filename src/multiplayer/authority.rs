//! Deterministic snapshot and command validation shared by clients and the mock backend.

use crate::core::identity::PlayerId;
use crate::core::player::PlayerColor;
use crate::core::simulation::{
    resolve_turn, validate_submission_batch, GameModel, MatchStatus, PersistedGame, TurnSubmission,
    PLAYER_COUNT_RANGE,
};
use crate::multiplayer::backend::BackendError;
use crate::multiplayer::model::GameRecord;

/// Creates canonical starting resources and worlds, accepting only the requested rules.
pub fn initial_snapshot(
    candidate: &PersistedGame,
    seed: [u8; 32],
) -> Result<PersistedGame, BackendError> {
    candidate.validate().map_err(invalid)?;
    if candidate.state.status != MatchStatus::Lobby {
        return Err(BackendError::InvalidGameStatus);
    }
    let mut model = GameModel::new(seed, candidate.state.rules.clone()).map_err(invalid)?;
    let color = candidate.state.player(1).map_err(invalid)?.color();
    let previous = model.player(1).map_err(invalid)?.color();
    for player in &mut model.players {
        if player.color() == color {
            player.color = Some(previous);
        }
    }
    model.player_mut(1).map_err(invalid)?.color = Some(color);
    Ok(PersistedGame::new(model))
}

/// A save may acknowledge canonical state or change only the caller's lobby color.
pub fn validate_save(
    record: &GameRecord,
    player_id: PlayerId,
    candidate: &PersistedGame,
) -> Result<(), BackendError> {
    candidate.validate().map_err(invalid)?;
    if same_snapshot(&record.persisted, candidate)? {
        return Ok(());
    }
    if record.status != MatchStatus::Lobby {
        return Err(BackendError::Forbidden);
    }
    let color = candidate.state.player(player_id).map_err(invalid)?.color();
    let expected = recolored_lobby_snapshot(record, player_id, color)?;
    if !same_snapshot(&expected, candidate)? {
        return Err(BackendError::Forbidden);
    }
    Ok(())
}

/// Validates an incoming immutable row against every row already accepted this turn.
pub fn validate_incoming(
    record: &GameRecord,
    existing: &[TurnSubmission],
    incoming: &TurnSubmission,
) -> Result<(), BackendError> {
    if let Some(prior) = existing.iter().find(|s| s.player_id == incoming.player_id) {
        return if serde_json::to_value(prior).map_err(invalid)?
            == serde_json::to_value(incoming).map_err(invalid)?
        {
            Ok(())
        } else {
            Err(BackendError::DuplicateSubmission {
                player_id: incoming.player_id,
                turn: incoming.turn,
            })
        };
    }
    let mut batch = existing.to_vec();
    batch.push(incoming.clone());
    validate_submission_batch(&record.persisted.state, &batch).map_err(invalid)
}

/// Computes the next canonical snapshot exclusively from stored state and accepted commands.
pub fn resolved_snapshot(
    record: &GameRecord,
    submissions: &[TurnSubmission],
) -> Result<PersistedGame, BackendError> {
    let mut next = record.persisted.clone();
    resolve_turn(&mut next.state, submissions).map_err(invalid)?;
    next.validate().map_err(invalid)?;
    Ok(next)
}

/// Compares serialized model data without relying on floating-point equality derives.
pub fn same_snapshot(left: &PersistedGame, right: &PersistedGame) -> Result<bool, BackendError> {
    Ok(serde_json::to_value(left).map_err(invalid)?
        == serde_json::to_value(right).map_err(invalid)?)
}

fn invalid(error: impl std::fmt::Display) -> BackendError {
    BackendError::InvalidData(error.to_string())
}

/// Applies one member's lobby color while preventing indistinguishable active members.
pub fn recolored_lobby_snapshot(
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
pub fn started_snapshot_for_members(
    record: &GameRecord,
    seed: [u8; 32],
) -> Result<PersistedGame, BackendError> {
    let player_count = u8::try_from(record.members.len())
        .map_err(|_| BackendError::InvalidData("too many lobby members".to_string()))?;
    if !(PLAYER_COUNT_RANGE.contains(&player_count)
        || record.persisted.state.rules.practice_mode && player_count == 1)
        || player_count > record.max_players
    {
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

#[cfg(test)]
#[path = "../../tests/multiplayer/authority.rs"]
mod tests;
