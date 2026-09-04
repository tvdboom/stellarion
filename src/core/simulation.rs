//! Deterministic gameplay state and simultaneous-turn resolution without Bevy systems.

use std::collections::HashSet;

use rand::seq::SliceRandom;
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::combat::resolution::{
    resolve_combat_with_rng, MAX_COMBAT_ROUNDS, MAX_SHOTS_PER_UNIT_PER_ROUND,
};
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::missions::{BombingRaid, Mission};
use crate::core::orders::{purchase_limit, validate_mission};
use crate::core::player::{Player, MAX_REPORTS_PER_PLAYER};
use crate::core::random::DeterministicRngState;
use crate::core::resources::ResourceName;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Price, Unit};
use crate::utils::NameFromEnum;

/// Current JSON persistence schema version.
pub const PERSISTED_SCHEMA_VERSION: u32 = 1;

/// Supported number of players in a multiplayer game.
pub const PLAYER_COUNT_RANGE: std::ops::RangeInclusive<u8> = 2..=MAX_MULTIPLAYER_PLAYERS;

/// Maximum number of members who may join an open multiplayer lobby.
pub const MAX_MULTIPLAYER_PLAYERS: u8 = 4;

/// Maximum number of intentional commands accepted from one player for one turn.
pub const MAX_COMMANDS_PER_SUBMISSION: usize = 1024;

/// Maximum number of simultaneous in-flight missions retained in persisted state.
pub const MAX_ACTIVE_MISSIONS: usize = 4096;

/// Gameplay settings that affect deterministic state transitions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GameRules {
    /// Number of non-moon planets generated for each player.
    pub planets_per_player: usize,
    /// Percentage of map planets that one player may own.
    pub colonizable_percent: usize,
    /// Number of moons as a percentage of non-moon planets.
    pub moons_percent: usize,
    /// Number of generated player slots in this snapshot.
    pub player_count: u8,
    /// Allows the debug-only one-player practice flow to remain active without opponents.
    #[serde(default, skip_serializing_if = "is_false")]
    pub practice_mode: bool,
}

/// Keeps the normal multiplayer JSON shape unchanged when practice mode is disabled.
fn is_false(value: &bool) -> bool {
    !*value
}

impl GameRules {
    /// Validates supported setting boundaries before map generation.
    pub fn validate(&self) -> Result<(), GameError> {
        let valid_player_count = if self.practice_mode {
            self.player_count == 1
        } else {
            PLAYER_COUNT_RANGE.contains(&self.player_count)
        };
        if !valid_player_count {
            return Err(GameError::InvalidPlayerCount(self.player_count));
        }
        if !(5..=20).contains(&self.planets_per_player) {
            return Err(GameError::InvalidSettings(
                "planets_per_player must be in 5..=20".to_string(),
            ));
        }
        if self.colonizable_percent == 0 || self.colonizable_percent > 100 {
            return Err(GameError::InvalidSettings(
                "colonizable_percent must be in 1..=100".to_string(),
            ));
        }
        if self.moons_percent > 100 {
            return Err(GameError::InvalidSettings("moons_percent must be in 0..=100".to_string()));
        }
        Ok(())
    }
}

impl Default for GameRules {
    /// Uses the original Stellarion defaults for a two-player match.
    fn default() -> Self {
        Self {
            planets_per_player: 10,
            colonizable_percent: 25,
            moons_percent: 30,
            player_count: 2,
            practice_mode: false,
        }
    }
}

/// Lifecycle state persisted with a game.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatchStatus {
    /// Players are still joining.
    #[default]
    Lobby,
    /// Players are submitting simultaneous turns.
    Active,
    /// At most one empire remains.
    Finished,
}

/// Complete deterministic gameplay snapshot persisted in Supabase.
#[derive(Clone, Serialize, Deserialize)]
pub struct GameModel {
    /// All configured player slots in stable identifier order.
    pub players: Vec<Player>,
    /// Complete map, ownership, production queues, and stationed units.
    pub map: Map,
    /// All in-flight missions, including missions hidden from some players.
    pub missions: Vec<Mission>,
    /// Current turn awaiting submissions, starting at one.
    pub turn: u64,
    /// Persisted deterministic random stream cursor.
    pub rng: DeterministicRngState,
    /// Rules selected by the creator.
    pub rules: GameRules,
    /// Current match lifecycle state.
    pub status: MatchStatus,
}

impl GameModel {
    /// Generates a complete lobby snapshot and deterministic home planets.
    pub fn new(seed: [u8; 32], rules: GameRules) -> Result<Self, GameError> {
        rules.validate()?;
        let mut rng_state = DeterministicRngState::new(seed);
        let mut rng = rng_state.next_rng();
        let planet_count = rules
            .planets_per_player
            .checked_mul(usize::from(rules.player_count))
            .ok_or_else(|| GameError::InvalidSettings("planet count overflow".to_string()))?;
        let mut map = Map::new_with_rng(planet_count, rules.moons_percent, &mut rng);
        let home_planets = choose_home_planets(&map, usize::from(rules.player_count), &mut rng)?;
        let mut players = Vec::with_capacity(home_planets.len());
        for (slot, planet_id) in home_planets.into_iter().enumerate() {
            let player_id = (slot + 1) as u64;
            map.get_mut(planet_id).make_home_planet(player_id);
            players.push(Player::new(player_id, planet_id));
        }

        let model = Self {
            players,
            map,
            missions: Vec::new(),
            turn: 1,
            rng: rng_state,
            rules,
            status: MatchStatus::Lobby,
        };
        model.validate()?;
        Ok(model)
    }

    /// Marks a full lobby ready to accept turn submissions.
    pub fn start(&mut self) -> Result<(), GameError> {
        if self.status != MatchStatus::Lobby {
            return Err(GameError::InvalidPhase {
                expected: MatchStatus::Lobby,
                actual: self.status,
            });
        }
        self.status = MatchStatus::Active;
        Ok(())
    }

    /// Returns one player by stable player-slot identifier.
    pub fn player(&self, player_id: PlayerId) -> Result<&Player, GameError> {
        self.players
            .iter()
            .find(|player| player.id == player_id)
            .ok_or(GameError::UnknownPlayer(player_id))
    }

    /// Returns one mutable player by stable player-slot identifier.
    pub fn player_mut(&mut self, player_id: PlayerId) -> Result<&mut Player, GameError> {
        self.players
            .iter_mut()
            .find(|player| player.id == player_id)
            .ok_or(GameError::UnknownPlayer(player_id))
    }

    /// Validates cross-references and boundaries in a deserialized snapshot.
    pub fn validate(&self) -> Result<(), GameError> {
        self.rules.validate()?;
        if self.turn == 0 {
            return Err(GameError::MalformedState("turn must be at least one".to_string()));
        }
        if self.players.len() != usize::from(self.rules.player_count) {
            return Err(GameError::MalformedState(
                "player vector does not match configured player count".to_string(),
            ));
        }

        let player_ids = self.players.iter().map(|player| player.id).collect::<HashSet<_>>();
        let player_colors =
            self.players.iter().map(|player| player.color()).collect::<HashSet<_>>();
        if self.players.iter().enumerate().any(|(slot, player)| player.id != (slot + 1) as u64) {
            return Err(GameError::MalformedState(
                "player identifiers must be contiguous slots starting at one".to_string(),
            ));
        }
        if player_colors.len() != self.players.len() {
            return Err(GameError::MalformedState("player colors must be distinct".to_string()));
        }
        if self.map.planets.is_empty()
            || self.map.planets.iter().enumerate().any(|(index, planet)| planet.id != index)
        {
            return Err(GameError::MalformedState(
                "planet identifiers must be contiguous indices starting at zero".to_string(),
            ));
        }
        let planet_ids = self.map.planets.iter().map(|planet| planet.id).collect::<HashSet<_>>();

        for player in &self.players {
            if player.color.is_some_and(|color| !color.is_valid()) {
                return Err(GameError::MalformedState(format!(
                    "player {} references an unsupported color",
                    player.id
                )));
            }
            if player.reports.len() > MAX_REPORTS_PER_PLAYER {
                return Err(GameError::MalformedState(format!(
                    "player {} report history exceeds {MAX_REPORTS_PER_PLAYER} entries",
                    player.id
                )));
            }
            if !planet_ids.contains(&player.home_planet) {
                return Err(GameError::MalformedState(format!(
                    "player {} references a missing home planet",
                    player.id
                )));
            }
            if self.status != MatchStatus::Finished {
                let owns_home = self.map.get(player.home_planet).owned == Some(player.id);
                if player.spectator == owns_home {
                    return Err(GameError::MalformedState(format!(
                        "player {} spectator status does not match home ownership",
                        player.id
                    )));
                }
            }

            let mut report_ids = HashSet::with_capacity(player.reports.len());
            for report in &player.reports {
                let mission = &report.mission;
                let references_known_players = [
                    mission.origin_owned,
                    mission.origin_controlled,
                    report.planet.owned,
                    report.planet.controlled,
                    report.destination_owned,
                    report.destination_controlled,
                ]
                .into_iter()
                .flatten()
                .all(|id| player_ids.contains(&id));
                let combat_is_bounded = report.combat_report.as_ref().is_none_or(|combat| {
                    combat.rounds.len() <= MAX_COMBAT_ROUNDS
                        && combat.rounds.iter().all(|round| {
                            round.destroy_probability.is_finite()
                                && (0.0..=1.0).contains(&round.destroy_probability)
                                && round.attacker.iter().chain(&round.defender).all(|unit| {
                                    unit.shots.len() <= MAX_SHOTS_PER_UNIT_PER_ROUND + 1
                                })
                        })
                });
                if report.id == 0
                    || !report_ids.insert(report.id)
                    || !player_ids.contains(&mission.owner)
                    || !planet_ids.contains(&mission.origin)
                    || !planet_ids.contains(&mission.destination)
                    || report.planet.id != mission.destination
                    || !mission.objective.is_mission()
                    || !mission.has_valid_return_objective()
                    || !mission.position.is_finite()
                    || mission.send > report.turn
                    || !u64::try_from(report.turn).is_ok_and(|turn| turn <= self.turn)
                    || !references_known_players
                    || !combat_is_bounded
                {
                    return Err(GameError::MalformedState(format!(
                        "player {} report {} contains invalid or unbounded history",
                        player.id, report.id
                    )));
                }
            }
        }
        for planet in &self.map.planets {
            for owner in [planet.owned, planet.controlled].into_iter().flatten() {
                if !player_ids.contains(&owner) {
                    return Err(GameError::MalformedState(format!(
                        "planet {} references unknown player {owner}",
                        planet.id
                    )));
                }
            }
        }
        if self.missions.len() > MAX_ACTIVE_MISSIONS {
            return Err(GameError::MalformedState(format!(
                "active mission count exceeds {MAX_ACTIVE_MISSIONS} entries"
            )));
        }
        let mut mission_ids = HashSet::with_capacity(self.missions.len());
        for mission in &self.missions {
            let origin_references_known_players = [mission.origin_owned, mission.origin_controlled]
                .into_iter()
                .flatten()
                .all(|id| player_ids.contains(&id));
            if !player_ids.contains(&mission.owner)
                || !planet_ids.contains(&mission.origin)
                || !planet_ids.contains(&mission.destination)
                || mission.origin == mission.destination
                || !mission.objective.is_mission()
                || !mission.has_valid_return_objective()
                || mission.id == 0
                || !mission_ids.insert(mission.id)
                || !mission.position.is_finite()
                || !origin_references_known_players
            {
                return Err(GameError::MalformedState(format!(
                    "mission {} contains an invalid reference",
                    mission.id
                )));
            }
        }
        Ok(())
    }
}

/// Versioned envelope stored in the database JSON column.
#[derive(Clone, Serialize, Deserialize)]
pub struct PersistedGame {
    /// Version of the serialized schema, independent of database revision.
    pub schema_version: u32,
    /// Complete deterministic game state.
    pub state: GameModel,
}

impl PersistedGame {
    /// Wraps current game state in the latest persistence schema.
    pub fn new(state: GameModel) -> Self {
        Self {
            schema_version: PERSISTED_SCHEMA_VERSION,
            state,
        }
    }

    /// Decodes and validates a JSON value without panicking on malformed data.
    pub fn from_json(value: serde_json::Value) -> Result<Self, GameError> {
        let persisted: Self = serde_json::from_value(value)
            .map_err(|error| GameError::MalformedState(error.to_string()))?;
        persisted.validate()?;
        Ok(persisted)
    }

    /// Validates the schema envelope and every core cross-reference after transport decoding.
    pub fn validate(&self) -> Result<(), GameError> {
        if self.schema_version != PERSISTED_SCHEMA_VERSION {
            return Err(GameError::UnsupportedSchema(self.schema_version));
        }
        self.state.validate()
    }

    /// Serializes this envelope to the database JSON representation.
    pub fn to_json(&self) -> Result<serde_json::Value, GameError> {
        serde_json::to_value(self).map_err(|error| GameError::MalformedState(error.to_string()))
    }
}

/// Intentional gameplay command submitted by one player.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TurnCommand {
    /// Adds debug-only testing resources and units to a local turn draft.
    PracticeBoost {
        /// Limits the boost to the player's owned or controlled worlds when true.
        owned_worlds_only: bool,
    },
    /// Queues one or more identical units on a controlled planet.
    BuyUnits {
        /// Planet on which production is queued.
        planet_id: PlanetId,
        /// Unit to produce.
        unit: Unit,
        /// Number of units to queue.
        count: usize,
    },
    /// Converts resources through a laboratory on a controlled moon.
    ConvertResources {
        /// Moon containing the laboratory.
        planet_id: PlanetId,
        /// Resource consumed.
        from: ResourceName,
        /// Resource produced.
        to: ResourceName,
        /// Amount consumed.
        amount: usize,
    },
    /// Abandons a non-home owned planet.
    AbandonPlanet {
        /// Planet to abandon.
        planet_id: PlanetId,
    },
    /// Consumes a colony ship already stationed on a controlled planet.
    ColonizePlanet {
        /// Planet to colonize.
        planet_id: PlanetId,
    },
    /// Dispatches a validated mission command.
    SendMission {
        /// Client-selected command identifier used for idempotency and reports.
        mission_id: u64,
        /// Origin planet.
        origin: PlanetId,
        /// Destination planet.
        destination: PlanetId,
        /// Mission objective.
        objective: Icon,
        /// Ships or missiles dispatched.
        army: Army,
        /// Optional bomber target class.
        bombing: BombingRaid,
        /// Whether probes remain in combat after round one.
        combat_probes: bool,
        /// Whether to use a jump gate.
        jump_gate: bool,
    },
}

/// All commands one player commits for one simultaneous turn.
#[derive(Clone, Serialize, Deserialize)]
pub struct TurnSubmission {
    /// Stable player slot submitting the commands.
    pub player_id: PlayerId,
    /// Turn to which the commands apply.
    pub turn: u64,
    /// Readiness attempt, advanced when the player continues an unfinished turn.
    /// This orders network retries only; it does not affect deterministic gameplay.
    #[serde(default)]
    pub generation: u64,
    /// Commands in the intentional order selected by that player.
    pub commands: Vec<TurnCommand>,
}

impl TurnSubmission {
    /// Creates a submission for a specific player and turn.
    pub fn new(player_id: PlayerId, turn: u64, commands: Vec<TurnCommand>) -> Self {
        Self {
            player_id,
            turn,
            generation: 0,
            commands,
        }
    }
}

/// Summary of one accepted deterministic turn resolution.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TurnResult {
    /// Newly available turn after resolution.
    pub turn: u64,
    /// Whether the match ended during this resolution.
    pub finished: bool,
    /// Sole remaining player when the match ended, if any.
    pub winner: Option<PlayerId>,
}

/// Typed deterministic simulation failure.
#[derive(Clone, Debug, Error, Eq, PartialEq)]
pub enum GameError {
    /// Configured player count is unsupported for multiplayer or local-practice rules.
    #[error("player count {0} is unsupported by these game rules")]
    InvalidPlayerCount(u8),
    /// A creator-selected setting is unsupported.
    #[error("invalid game settings: {0}")]
    InvalidSettings(String),
    /// Persisted state uses an unsupported schema.
    #[error("unsupported persisted schema version {0}")]
    UnsupportedSchema(u32),
    /// Persisted state failed structural validation.
    #[error("malformed persisted game state: {0}")]
    MalformedState(String),
    /// A state transition was requested in the wrong phase.
    #[error("expected match phase {expected:?}, found {actual:?}")]
    InvalidPhase {
        /// Required phase.
        expected: MatchStatus,
        /// Current phase.
        actual: MatchStatus,
    },
    /// A submission references a player outside this game.
    #[error("unknown player {0}")]
    UnknownPlayer(PlayerId),
    /// Two submissions used the same player identifier.
    #[error("duplicate submission for player {0}")]
    DuplicateSubmission(PlayerId),
    /// A required active player has not submitted.
    #[error("missing submission for player {0}")]
    MissingSubmission(PlayerId),
    /// A submission targets an old or future turn.
    #[error("submission targets turn {actual}; current turn is {expected}")]
    StaleTurn {
        /// Current model turn.
        expected: u64,
        /// Submitted turn.
        actual: u64,
    },
    /// A submitted gameplay command violates the current state.
    #[error("invalid command for player {player_id}: {reason}")]
    InvalidCommand {
        /// Player whose command failed.
        player_id: PlayerId,
        /// Human-readable validation detail.
        reason: String,
    },
}

/// Resolves a complete simultaneous turn atomically and deterministically.
pub fn resolve_turn(
    state: &mut GameModel,
    submissions: &[TurnSubmission],
) -> Result<TurnResult, GameError> {
    if state.status != MatchStatus::Active {
        return Err(GameError::InvalidPhase {
            expected: MatchStatus::Active,
            actual: state.status,
        });
    }
    state.validate()?;

    let required = state
        .players
        .iter()
        .filter(|player| !player.spectator)
        .map(|player| player.id)
        .collect::<HashSet<_>>();
    let mut seen = HashSet::with_capacity(submissions.len());
    for submission in submissions {
        if submission.turn != state.turn {
            return Err(GameError::StaleTurn {
                expected: state.turn,
                actual: submission.turn,
            });
        }
        if !required.contains(&submission.player_id) {
            return Err(GameError::UnknownPlayer(submission.player_id));
        }
        if submission.commands.len() > MAX_COMMANDS_PER_SUBMISSION {
            return invalid(
                submission.player_id,
                format!("submission exceeds {MAX_COMMANDS_PER_SUBMISSION} commands"),
            );
        }
        if !seen.insert(submission.player_id) {
            return Err(GameError::DuplicateSubmission(submission.player_id));
        }
    }
    if let Some(missing) = required.difference(&seen).next() {
        return Err(GameError::MissingSubmission(*missing));
    }

    let mut working = state.clone();
    let mut ordered = submissions.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|submission| submission.player_id);
    for submission in ordered {
        for command in &submission.commands {
            apply_command(&mut working, submission.player_id, command)?;
        }
    }

    advance_simulation(&mut working)?;
    working.validate()?;
    let winner = if working.status == MatchStatus::Finished {
        working
            .players
            .iter()
            .find(|player| player.owns(working.map.get(player.home_planet)))
            .map(|player| player.id)
    } else {
        None
    };
    let result = TurnResult {
        turn: working.turn,
        finished: working.status == MatchStatus::Finished,
        winner,
    };
    *state = working;
    Ok(result)
}

/// Validates a partial submission set by resolving it with empty orders for missing players.
/// A backend must perform this before making any submission immutable. It also checks shared
/// mission limits and collisions across already accepted players' commands.
pub fn validate_submission_batch(
    state: &GameModel,
    submissions: &[TurnSubmission],
) -> Result<(), GameError> {
    let mut complete = submissions.to_vec();
    for player in state.players.iter().filter(|player| !player.spectator) {
        if !complete.iter().any(|submission| submission.player_id == player.id) {
            complete.push(TurnSubmission::new(player.id, state.turn, Vec::new()));
        }
    }
    resolve_turn(&mut state.clone(), &complete).map(|_| ())
}

/// Projects one player's orders without advancing the simultaneous turn.
/// Used when restoring a saved draft; other players' orders remain invisible.
pub fn preview_commands(
    state: &GameModel,
    player_id: PlayerId,
    commands: &[TurnCommand],
) -> Result<GameModel, GameError> {
    let mut preview = state.clone();
    for command in commands {
        apply_command(&mut preview, player_id, command)?;
    }
    Ok(preview)
}

/// Selects well-separated home planets without an unbounded rejection loop.
fn choose_home_planets<R: Rng + ?Sized>(
    map: &Map,
    count: usize,
    rng: &mut R,
) -> Result<Vec<PlanetId>, GameError> {
    let planets = map.planets();
    if count == 0 || planets.len() < count {
        return Err(GameError::InvalidSettings(
            "not enough planets for configured player slots".to_string(),
        ));
    }
    let first = rng.random_range(0..planets.len());
    let mut selected = vec![planets[first].id];
    while selected.len() < count {
        let candidate = planets
            .iter()
            .filter(|planet| !selected.contains(&planet.id))
            .max_by(|left, right| {
                let score = |planet: &&Planet| {
                    selected
                        .iter()
                        .map(|id| planet.position.distance(map.get(*id).position))
                        .fold(f32::INFINITY, f32::min)
                };
                score(left).total_cmp(&score(right)).then_with(|| right.id.cmp(&left.id))
            })
            .ok_or_else(|| {
                GameError::InvalidSettings("failed to choose home planets".to_string())
            })?;
        selected.push(candidate.id);
    }
    Ok(selected)
}

/// Applies one validated player command to a working turn snapshot.
fn apply_command(
    model: &mut GameModel,
    player_id: PlayerId,
    command: &TurnCommand,
) -> Result<(), GameError> {
    match command {
        TurnCommand::PracticeBoost {
            owned_worlds_only,
        } => apply_practice_boost(model, player_id, *owned_worlds_only),
        TurnCommand::BuyUnits {
            planet_id,
            unit,
            count,
        } => apply_purchase(model, player_id, *planet_id, *unit, *count),
        TurnCommand::ConvertResources {
            planet_id,
            from,
            to,
            amount,
        } => apply_conversion(model, player_id, *planet_id, *from, *to, *amount),
        TurnCommand::AbandonPlanet {
            planet_id,
        } => apply_abandon(model, player_id, *planet_id),
        TurnCommand::ColonizePlanet {
            planet_id,
        } => apply_colonize(model, player_id, *planet_id),
        TurnCommand::SendMission {
            mission_id,
            origin,
            destination,
            objective,
            army,
            bombing,
            combat_probes,
            jump_gate,
        } => apply_mission(
            model,
            player_id,
            *mission_id,
            *origin,
            *destination,
            *objective,
            army,
            bombing.clone(),
            *combat_probes,
            *jump_gate,
        ),
    }
}

/// Keeps testing shortcuts in the same ordered draft as the orders that depend on them.
fn apply_practice_boost(
    model: &mut GameModel,
    player_id: PlayerId,
    owned_worlds_only: bool,
) -> Result<(), GameError> {
    model.player_mut(player_id)?.resources += 1_000usize;
    for planet in model.map.planets.iter_mut().filter(|planet| {
        !planet.is_destroyed
            && (!owned_worlds_only
                || planet.owned == Some(player_id)
                || planet.controlled == Some(player_id))
    }) {
        // Testing shortcuts bypass costs and capacity, but never create a unit on a world where
        // that unit cannot normally be constructed.
        for unit in Unit::all_valid(planet.is_moon()).into_iter().flatten() {
            let amount = planet.army.entry(unit).or_default();
            if unit.is_building() {
                *amount = Building::MAX_LEVEL;
            } else {
                *amount = amount.saturating_add(3);
            }
        }
    }
    Ok(())
}

/// Queues a validated purchase and deducts its resources.
fn apply_purchase(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    unit: Unit,
    count: usize,
) -> Result<(), GameError> {
    if count == 0 {
        return invalid(player_id, "purchase count must be positive");
    }
    let player_index = model
        .players
        .iter()
        .position(|player| player.id == player_id)
        .ok_or(GameError::UnknownPlayer(player_id))?;
    let planet_index = model
        .map
        .planets
        .iter()
        .position(|planet| planet.id == planet_id)
        .ok_or_else(|| invalid_error(player_id, "purchase planet does not exist"))?;

    let limit =
        purchase_limit(&model.players[player_index], &model.map.planets[planet_index], unit)
            .map_err(|error| invalid_error(player_id, error.to_string()))?;
    if count > limit {
        return invalid(player_id, "purchase exceeds available resources or production capacity");
    }
    model.players[player_index].resources -= unit.price() * count;
    model.map.planets[planet_index].buy.extend(std::iter::repeat_n(unit, count));
    Ok(())
}

/// Applies one laboratory conversion after validating ownership and rates.
fn apply_conversion(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    from: ResourceName,
    to: ResourceName,
    amount: usize,
) -> Result<(), GameError> {
    if amount == 0 || from == to {
        return invalid(
            player_id,
            "resource conversion must use distinct resources and a positive amount",
        );
    }
    let planet = model
        .map
        .planets
        .iter()
        .find(|planet| planet.id == planet_id)
        .ok_or_else(|| invalid_error(player_id, "laboratory planet does not exist"))?;
    let laboratory = planet.army.amount(&Unit::Building(Building::Laboratory));
    if !planet.is_moon() || planet.controlled != Some(player_id) || laboratory == 0 {
        return invalid(player_id, "a controlled moon with a laboratory is required");
    }
    let player = model
        .players
        .iter_mut()
        .find(|player| player.id == player_id)
        .ok_or(GameError::UnknownPlayer(player_id))?;
    if player.resources.get(&from) < amount {
        return invalid(player_id, "not enough resources to convert");
    }
    let divisor = 1.0 + 0.5 * (Building::MAX_LEVEL.saturating_sub(laboratory)) as f32;
    let gain = (amount as f32 / divisor) as usize;
    *player.resources.get_mut(&from) -= amount;
    *player.resources.get_mut(&to) = player.resources.get(&to).saturating_add(gain);
    Ok(())
}

/// Applies a validated planet-abandon command.
fn apply_abandon(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
) -> Result<(), GameError> {
    let player = model.player(player_id)?;
    if player.home_planet == planet_id {
        return invalid(player_id, "the home planet cannot be abandoned");
    }
    let planet = model
        .map
        .planets
        .iter_mut()
        .find(|planet| planet.id == planet_id)
        .ok_or_else(|| invalid_error(player_id, "abandon planet does not exist"))?;
    if planet.owned != Some(player_id) || !planet.buy.is_empty() {
        return invalid(player_id, "planet is not owned or has queued production");
    }
    planet.abandon();
    Ok(())
}

/// Applies direct colonization of a controlled planet.
fn apply_colonize(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
) -> Result<(), GameError> {
    let max_owned = ((model.map.planets().len() as f32 * model.rules.colonizable_percent as f32
        / 100.0)
        .ceil()) as usize;
    let owned = model.map.planets.iter().filter(|planet| planet.owned == Some(player_id)).count();
    let planet = model
        .map
        .planets
        .iter_mut()
        .find(|planet| planet.id == planet_id)
        .ok_or_else(|| invalid_error(player_id, "colonization planet does not exist"))?;
    if planet.is_moon()
        || planet.controlled != Some(player_id)
        || planet.owned == Some(player_id)
        || owned >= max_owned
        || planet.army.amount(&Unit::colony_ship()) == 0
    {
        return invalid(player_id, "planet cannot currently be colonized");
    }
    if let Some(count) = planet.army.get_mut(&Unit::colony_ship()) {
        *count = count.saturating_sub(1);
    }
    planet.colonize(player_id);
    Ok(())
}

/// Rebuilds and dispatches a mission from authoritative map data.
#[allow(clippy::too_many_arguments)]
fn apply_mission(
    model: &mut GameModel,
    player_id: PlayerId,
    mission_id: u64,
    origin_id: PlanetId,
    destination_id: PlanetId,
    objective: Icon,
    army: &Army,
    bombing: BombingRaid,
    combat_probes: bool,
    jump_gate: bool,
) -> Result<(), GameError> {
    if model.missions.len() >= MAX_ACTIVE_MISSIONS {
        return invalid(
            player_id,
            format!("active mission limit of {MAX_ACTIVE_MISSIONS} reached"),
        );
    }
    if mission_id == 0 || model.missions.iter().any(|mission| mission.id == mission_id) {
        return invalid(player_id, "mission identifier is missing or already used");
    }
    let army = army
        .iter()
        .filter(|(_, count)| **count > 0)
        .map(|(unit, count)| (*unit, *count))
        .collect::<Army>();
    if origin_id == destination_id || !objective.is_mission() || !army.has_army() {
        return invalid(player_id, "mission origin, destination, objective, or army is invalid");
    }
    let origin_index = model
        .map
        .planets
        .iter()
        .position(|planet| planet.id == origin_id)
        .ok_or_else(|| invalid_error(player_id, "mission origin does not exist"))?;
    let destination = model
        .map
        .planets
        .iter()
        .find(|planet| planet.id == destination_id)
        .ok_or_else(|| invalid_error(player_id, "mission destination does not exist"))?
        .clone();
    let origin = &model.map.planets[origin_index];
    let turn = usize::try_from(model.turn)
        .map_err(|_| invalid_error(player_id, "turn cannot be represented on this platform"))?;
    let mission = Mission::new_with_id(
        mission_id,
        turn,
        player_id,
        origin,
        &destination,
        objective,
        army.clone(),
        bombing,
        combat_probes,
        jump_gate,
        None,
    );
    validate_mission(model.player(player_id)?, origin, &destination, &mission)
        .map_err(|error| invalid_error(player_id, error.to_string()))?;
    let fuel = mission.fuel_consumption(&model.map);
    let player_index = model
        .players
        .iter()
        .position(|player| player.id == player_id)
        .ok_or(GameError::UnknownPlayer(player_id))?;
    if model.players[player_index].resources.deuterium < fuel {
        return invalid(player_id, "mission requires more deuterium than the player owns");
    }
    model.players[player_index].resources.deuterium -= fuel;
    let origin = &mut model.map.planets[origin_index];
    if jump_gate {
        origin.jump_gate = origin.jump_gate.saturating_add(mission.jump_cost());
    }
    for (unit, count) in &army {
        if let Some(available) = origin.army.get_mut(unit) {
            *available = available.saturating_sub(*count);
        }
    }
    origin.release_control_if_vacant();
    model.missions.push(mission);
    Ok(())
}

/// Advances production, missions, combat, reports, and victory state by one turn.
fn advance_simulation(model: &mut GameModel) -> Result<(), GameError> {
    model.turn = model.turn.saturating_add(1);
    let turn = usize::try_from(model.turn).map_err(|_| {
        GameError::MalformedState("turn exceeds this platform's limits".to_string())
    })?;
    let mut rng = model.rng.next_rng();

    for planet in &mut model.map.planets {
        planet.produce();
        planet.jump_gate = 0;
    }
    for player in &mut model.players {
        player.resources += player.resource_production(&model.map.planets);
    }

    let mut player_order = model.players.iter().map(|player| player.id).collect::<Vec<_>>();
    player_order.shuffle(&mut rng);
    let planet_ids = model.map.planets.iter().map(|planet| planet.id).collect::<Vec<_>>();
    let mut new_missions = Vec::new();
    let mut used_mission_ids = model.missions.iter().map(|mission| mission.id).collect();

    for mission in &mut model.missions {
        check_mission(mission, &model.map, turn, model.rules.colonizable_percent);
    }

    for player_id in player_order {
        for planet_id in &planet_ids {
            loop {
                let arrived = model
                    .missions
                    .iter()
                    .filter(|mission| {
                        mission.owner == player_id
                            && mission.destination == *planet_id
                            && mission.turns_to_destination(&model.map) < 2
                    })
                    .cloned()
                    .collect::<Vec<_>>();
                if arrived.is_empty() {
                    break;
                }

                for mission in regroup_missions(&arrived) {
                    let new_origin = model.map.get(mission.check_origin(&model.map)).clone();
                    let destination = model.map.get_mut(mission.destination);
                    let mut report = resolve_combat_with_rng(turn, &mission, destination, &mut rng);
                    report.mission.logs.push_str(&format!(
                        "\n- ({turn}) Mission arrived in {}.",
                        destination.name
                    ));

                    if report.scout_probes > 0 {
                        if mission.objective == Icon::Spy {
                            report.mission.logs.push_str(&format!(
                                "\n- ({turn}) Spied on planet {}.",
                                destination.name
                            ));
                            new_missions.push(
                                Mission::new_with_id(
                                    next_unique_mission_id(&mut rng, &mut used_mission_ids)?,
                                    turn,
                                    report.mission.owner,
                                    destination,
                                    &new_origin,
                                    Icon::Deploy,
                                    report.surviving_attacker.clone(),
                                    BombingRaid::None,
                                    false,
                                    false,
                                    Some(format!(
                                        "{}\n- ({turn}) Returning to planet {}.",
                                        report.mission.logs, new_origin.name
                                    )),
                                )
                                .with_return_objective(Icon::Spy),
                            );
                        } else if report.mission.objective != Icon::Destroy
                            || report.winner() != Some(mission.owner)
                        {
                            new_missions.push(Mission::new_with_id(
                                next_unique_mission_id(&mut rng, &mut used_mission_ids)?,
                                turn,
                                mission.owner,
                                destination,
                                &new_origin,
                                Icon::Deploy,
                                Army::from([(Unit::probe(), report.scout_probes)]),
                                BombingRaid::None,
                                false,
                                false,
                                None,
                            ));
                        }
                    }

                    if matches!(
                        report.mission.objective,
                        Icon::Attack | Icon::Colonize | Icon::Destroy
                    ) {
                        destination.army.retain(|unit, _| !unit.is_building());
                        destination.army.extend(
                            report
                                .surviving_defender
                                .iter()
                                .filter(|(unit, _)| unit.is_building())
                                .map(|(unit, count)| (*unit, *count)),
                        );
                    }
                    if report.is_stalemate() {
                        // Defenders keep the world; surviving attackers retreat without duplicating scouts.
                        let retreat = report
                            .surviving_attacker
                            .iter()
                            .filter_map(|(unit, count)| {
                                let count = if *unit == Unit::probe() {
                                    count.saturating_sub(report.scout_probes)
                                } else {
                                    *count
                                };
                                (count > 0).then_some((*unit, count))
                            })
                            .collect::<Army>();
                        if retreat.has_army() {
                            let return_mission = Mission::new_with_id(
                                next_unique_mission_id(&mut rng, &mut used_mission_ids)?,
                                turn,
                                mission.owner,
                                destination,
                                &new_origin,
                                Icon::Deploy,
                                retreat,
                                BombingRaid::None,
                                false,
                                false,
                                Some(format!(
                                    "{}\n- ({turn}) Combat stalemate; returning to {}.",
                                    report.mission.logs, new_origin.name
                                )),
                            );
                            new_missions.push(if mission.objective == Icon::Destroy {
                                return_mission.with_return_objective(Icon::Destroy)
                            } else {
                                return_mission
                            });
                        }
                    }

                    if report.winner() == Some(mission.owner) {
                        if report.mission.objective == Icon::Destroy {
                            if report.planet_destroyed {
                                destination.destroy();
                                report.mission.logs.push_str(&format!(
                                    "\n- ({turn}) Planet {} destroyed.",
                                    destination.name
                                ));
                            } else {
                                report.mission.logs.push_str(&format!(
                                    "\n- ({turn}) Failed to destroy planet {}.",
                                    destination.name
                                ));
                            }
                            new_missions.push(
                                Mission::new_with_id(
                                    next_unique_mission_id(&mut rng, &mut used_mission_ids)?,
                                    turn,
                                    report.mission.owner,
                                    destination,
                                    &new_origin,
                                    Icon::Deploy,
                                    report.surviving_attacker.clone(),
                                    BombingRaid::None,
                                    false,
                                    false,
                                    Some(format!(
                                        "{}\n- ({turn}) Returning to planet {}.",
                                        report.mission.logs, new_origin.name
                                    )),
                                )
                                .with_return_objective(Icon::Destroy),
                            );
                        } else if report.planet_colonized {
                            if let Some(count) =
                                report.surviving_attacker.get_mut(&Unit::colony_ship())
                            {
                                *count = count.saturating_sub(1);
                            }
                            destination.colonize(mission.owner);
                            report.mission.logs.push_str(&format!(
                                "\n- ({turn}) Planet {} colonized.",
                                destination.name
                            ));
                            if !destination.has_buildings() {
                                destination.army.insert(Unit::Building(Building::MetalMine), 1);
                                destination.army.insert(Unit::Building(Building::CrystalMine), 1);
                                destination
                                    .army
                                    .insert(Unit::Building(Building::DeuteriumSynthesizer), 1);
                            }
                        }

                        if !(mission.objective == Icon::Deploy
                            || (mission.objective == Icon::Colonize
                                && destination.controlled == Some(mission.owner)))
                        {
                            destination.army.retain(|unit, _| unit.is_building());
                        }
                        if mission.objective != Icon::Destroy {
                            destination.control_with_rng(mission.owner, &mut rng);
                            destination.dock(
                                report
                                    .surviving_attacker
                                    .iter()
                                    .map(|(unit, count)| {
                                        (
                                            *unit,
                                            if *unit == Unit::probe() {
                                                count.saturating_sub(report.scout_probes)
                                            } else {
                                                *count
                                            },
                                        )
                                    })
                                    .collect(),
                            );
                        }
                    } else {
                        destination.army.clone_from(&report.surviving_defender);
                    }

                    report.destination_owned = destination.owned;
                    report.destination_controlled = destination.controlled;
                    for player in &mut model.players {
                        if report.planet.controlled == Some(player.id)
                            || report.mission.owner == player.id
                        {
                            player.push_report(report.clone());
                        }
                    }
                }

                let arrived_ids = arrived.iter().map(|mission| mission.id).collect::<HashSet<_>>();
                for mission in &mut model.missions {
                    check_mission(mission, &model.map, turn, model.rules.colonizable_percent);
                }
                model.missions.retain(|mission| !arrived_ids.contains(&mission.id));
            }
        }
    }

    for mission in &mut model.missions {
        mission.advance(&model.map);
    }
    if new_missions.len() > MAX_ACTIVE_MISSIONS.saturating_sub(model.missions.len()) {
        return Err(GameError::MalformedState(format!(
            "mission resolution exceeds {MAX_ACTIVE_MISSIONS} active missions"
        )));
    }
    model.missions.extend(new_missions);

    let mut playing = Vec::new();
    for player in &mut model.players {
        player.spectator = !player.owns(model.map.get(player.home_planet));
        if !player.spectator {
            playing.push(player.id);
        }
    }
    if playing.len() > 1 || (model.rules.practice_mode && playing.len() == 1) {
        let eliminated = model
            .players
            .iter()
            .filter(|player| player.spectator)
            .map(|player| player.id)
            .collect::<HashSet<_>>();
        for planet in &mut model.map.planets {
            if planet.controlled.is_some_and(|id| eliminated.contains(&id)) {
                planet.clean();
            }
        }
        model.missions.retain(|mission| !eliminated.contains(&mission.owner));
    } else {
        model.status = MatchStatus::Finished;
        for player in &mut model.players {
            player.spectator = true;
        }
    }
    Ok(())
}

/// Merges same-player missions by objective and original gameplay priority.
fn regroup_missions(missions: &[Mission]) -> Vec<Mission> {
    let mut deploy: Option<Mission> = None;
    let mut missile: Option<Mission> = None;
    let mut spy: Option<Mission> = None;
    let mut rest: Option<Mission> = None;
    for mission in missions {
        let target = match mission.objective {
            Icon::MissileStrike => &mut missile,
            Icon::Spy => &mut spy,
            Icon::Deploy => &mut deploy,
            _ => &mut rest,
        };
        if let Some(grouped) = target {
            grouped.merge(mission);
        } else {
            *target = Some(mission.clone());
        }
    }
    [deploy, missile, spy, rest].into_iter().flatten().collect()
}

/// Allocates a nonzero mission identifier with finite randomized and sequential fallbacks.
fn next_unique_mission_id<R: Rng + ?Sized>(
    rng: &mut R,
    used: &mut HashSet<u64>,
) -> Result<u64, GameError> {
    for _ in 0..8 {
        let candidate = rng.random();
        if candidate != 0 && used.insert(candidate) {
            return Ok(candidate);
        }
    }
    let search_end = u64::try_from(used.len()).unwrap_or(u64::MAX - 1).saturating_add(1);
    if let Some(candidate) = (1..=search_end).find(|candidate| !used.contains(candidate)) {
        used.insert(candidate);
        return Ok(candidate);
    }
    Err(GameError::MalformedState("no unique mission identifier is available".to_string()))
}

/// Updates a mission whose destination ownership changed earlier in the turn.
fn check_mission(mission: &mut Mission, map: &Map, turn: usize, colonizable_percent: usize) {
    let old_objective = mission.objective;
    let destination = map.get(mission.destination);
    // Colonization intent survives friendly ownership changes while the fleet travels.
    if destination.controlled == Some(mission.owner)
        && !matches!(mission.objective, Icon::Deploy | Icon::MissileStrike | Icon::Colonize)
    {
        mission.objective = Icon::Deploy;
    }
    if destination.controlled != Some(mission.owner) && mission.objective == Icon::Deploy {
        mission.objective = Icon::Attack;
    }
    let owned = map.planets.iter().filter(|planet| planet.owned == Some(mission.owner)).count();
    let max_owned =
        (map.planets().len() as f32 * colonizable_percent as f32 / 100.0).ceil() as usize;
    if mission.objective == Icon::Colonize
        && destination.owned != Some(mission.owner)
        && owned >= max_owned
    {
        mission.objective = if destination.controlled == Some(mission.owner) {
            Icon::Deploy
        } else {
            Icon::Attack
        };
    }
    if destination.is_destroyed {
        mission.destination = mission.check_origin(map);
        mission.objective = Icon::Deploy;
        mission.logs.push_str(&format!(
            "\n- ({turn}) Destination changed to planet {}.",
            map.get(mission.destination).name
        ));
    }
    if old_objective != mission.objective {
        if mission.objective != Icon::Deploy {
            mission.return_objective = None;
        }
        mission.logs.push_str(&format!(
            "\n- ({turn}) Objective changed to {}.",
            mission.objective.to_name()
        ));
    }
}

/// Builds a typed invalid-command result without mutating the original model.
fn invalid<T>(player_id: PlayerId, reason: impl Into<String>) -> Result<T, GameError> {
    Err(invalid_error(player_id, reason))
}

/// Builds the error value shared by validation helpers and closures.
fn invalid_error(player_id: PlayerId, reason: impl Into<String>) -> GameError {
    GameError::InvalidCommand {
        player_id,
        reason: reason.into(),
    }
}

#[cfg(test)]
#[path = "../../tests/core/simulation.rs"]
mod tests;
