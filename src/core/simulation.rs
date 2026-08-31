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
use crate::core::player::{Player, MAX_REPORTS_PER_PLAYER};
use crate::core::random::DeterministicRngState;
use crate::core::resources::ResourceName;
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::{Amount, Army, Price, Unit};
use crate::utils::NameFromEnum;

/// Current JSON persistence schema version.
pub const PERSISTED_SCHEMA_VERSION: u32 = 1;

/// Supported number of players in a multiplayer game.
pub const PLAYER_COUNT_RANGE: std::ops::RangeInclusive<u8> = 2..=4;

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
    /// Exact number of slots that must join before the game starts.
    pub player_count: u8,
}

impl GameRules {
    /// Validates supported setting boundaries before map generation.
    pub fn validate(&self) -> Result<(), GameError> {
        if !PLAYER_COUNT_RANGE.contains(&self.player_count) {
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
        if self.players.iter().enumerate().any(|(slot, player)| player.id != (slot + 1) as u64) {
            return Err(GameError::MalformedState(
                "player identifiers must be contiguous slots starting at one".to_string(),
            ));
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
    /// Commands in the intentional order selected by that player.
    pub commands: Vec<TurnCommand>,
}

impl TurnSubmission {
    /// Creates a submission for a specific player and turn.
    pub fn new(player_id: PlayerId, turn: u64, commands: Vec<TurnCommand>) -> Self {
        Self {
            player_id,
            turn,
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
    /// Configured player count is outside 2..=4.
    #[error("player count {0} is outside the supported 2..=4 range")]
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

    for _ in 0..count {
        let planet = &model.map.planets[planet_index];
        if !(planet.owned == Some(player_id)
            || (planet.is_moon() && planet.controlled == Some(player_id)))
        {
            return invalid(player_id, "player cannot produce on this planet");
        }
        let player = &model.players[player_index];
        if player.resources < unit.price() {
            return invalid(player_id, "not enough resources for purchase");
        }
        match unit {
            Unit::Building(_) => {
                let current = planet.army.amount(&unit);
                if current >= Building::MAX_LEVEL || planet.buy.contains(&unit) {
                    return invalid(player_id, "building is at maximum level or already queued");
                }
                if planet.is_moon()
                    && unit.consumes_field()
                    && planet.fields_consumed() >= planet.max_fields()
                {
                    return invalid(player_id, "no lunar field is available");
                }
            },
            Unit::Ship(ship) => {
                if ship.production() > planet.army.amount(&Unit::Building(Building::Shipyard))
                    || planet.fleet_production().saturating_add(ship.production())
                        > planet.max_fleet_production()
                {
                    return invalid(
                        player_id,
                        "shipyard level or production capacity is insufficient",
                    );
                }
            },
            Unit::Defense(defense) => {
                let required_building = if defense.is_missile() {
                    Building::MissileSilo
                } else {
                    Building::Factory
                };
                if defense.production() > planet.army.amount(&Unit::Building(required_building))
                    || planet.battery_production().saturating_add(defense.production())
                        > planet.max_battery_production()
                {
                    return invalid(player_id, "defense production requirements are not met");
                }
                if defense.is_missile()
                    && planet
                        .missile_capacity()
                        .saturating_add(planet.buy.iter().filter(|queued| **queued == unit).count())
                        >= planet.max_missile_capacity()
                {
                    return invalid(player_id, "missile silo is full");
                }
                if defense == Defense::SpaceDock
                    && (planet.army.amount(&unit) > 0 || planet.buy.contains(&unit))
                {
                    return invalid(player_id, "only one space dock is allowed");
                }
            },
        }
        model.players[player_index].resources -= unit.price();
        model.map.planets[planet_index].buy.push(unit);
    }
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
    if origin.controlled != Some(player_id)
        || destination.is_destroyed
        || (destination.is_moon() && objective.on_planet_only())
        || !objective.condition(origin)
    {
        return invalid(player_id, "mission objective is not available for these planets");
    }
    if army.iter().any(|(unit, count)| *count == 0 || origin.army.amount(unit) < *count) {
        return invalid(player_id, "mission contains unavailable units");
    }
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
    let fuel = mission.fuel_consumption(&model.map);
    let player_index = model
        .players
        .iter()
        .position(|player| player.id == player_id)
        .ok_or(GameError::UnknownPlayer(player_id))?;
    if model.players[player_index].resources.deuterium < fuel {
        return invalid(player_id, "mission requires more deuterium than the player owns");
    }
    if jump_gate
        && (origin.army.amount(&Unit::Building(Building::JumpGate)) == 0
            || destination.army.amount(&Unit::Building(Building::JumpGate)) == 0
            || mission.jump_cost().saturating_add(origin.jump_gate) > origin.max_jump_capacity())
    {
        return invalid(player_id, "jump gate requirements or capacity are not met");
    }

    model.players[player_index].resources.deuterium -= fuel;
    let origin = &mut model.map.planets[origin_index];
    if jump_gate {
        origin.jump_gate = origin.jump_gate.saturating_add(mission.jump_cost());
    }
    for (unit, count) in army {
        if let Some(available) = origin.army.get_mut(unit) {
            *available = available.saturating_sub(*count);
        }
    }
    if !origin.has_fleet() && origin.owned != Some(player_id) && !origin.is_moon() {
        origin.controlled = None;
    }
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
                            new_missions.push(Mission::new_with_id(
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
                            ));
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
                            new_missions.push(Mission::new_with_id(
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
                            ));
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
    if playing.len() > 1 {
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
    if (destination.controlled == Some(mission.owner)
        && !matches!(mission.objective, Icon::Deploy | Icon::MissileStrike | Icon::Colonize))
        || (destination.owned == Some(mission.owner) && mission.objective == Icon::Colonize)
    {
        mission.objective = Icon::Deploy;
    }
    if destination.controlled != Some(mission.owner) && mission.objective == Icon::Deploy {
        mission.objective = Icon::Attack;
    }
    let owned = map.planets.iter().filter(|planet| planet.owned == Some(mission.owner)).count();
    let max_owned =
        (map.planets().len() as f32 * colonizable_percent as f32 / 100.0).ceil() as usize;
    if mission.objective == Icon::Colonize && owned >= max_owned {
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
mod tests {
    use super::*;

    /// Creates and starts a deterministic model for unit tests.
    fn started_model(player_count: u8) -> GameModel {
        let mut model = GameModel::new(
            [player_count; 32],
            GameRules {
                player_count,
                ..GameRules::default()
            },
        )
        .unwrap();
        model.start().unwrap();
        model
    }

    #[test]
    /// Proves identical state and submissions produce byte-identical next state.
    fn resolution_is_deterministic() {
        let mut left = started_model(2);
        let mut right = left.clone();
        let submissions = left
            .players
            .iter()
            .map(|player| TurnSubmission::new(player.id, left.turn, Vec::new()))
            .collect::<Vec<_>>();

        assert_eq!(resolve_turn(&mut left, &submissions), resolve_turn(&mut right, &submissions));
        assert_eq!(
            serde_json::to_vec(&PersistedGame::new(left)).unwrap(),
            serde_json::to_vec(&PersistedGame::new(right)).unwrap()
        );
    }

    #[test]
    /// Validates every supported multiplayer player count.
    fn supports_two_through_four_players() {
        for count in 2..=4 {
            let model = started_model(count);
            assert_eq!(model.players.len(), usize::from(count));
            assert_eq!(
                model.players.iter().map(|player| player.id).collect::<HashSet<_>>().len(),
                usize::from(count)
            );
        }
    }

    #[test]
    /// Rejects player counts outside the public contract.
    fn rejects_player_count_boundaries() {
        for count in [0, 1, 5, u8::MAX] {
            let result = GameModel::new(
                [count; 32],
                GameRules {
                    player_count: count,
                    ..GameRules::default()
                },
            );
            assert!(matches!(result, Err(GameError::InvalidPlayerCount(value)) if value == count));
        }
    }

    #[test]
    /// Rejects missing, duplicate, and stale simultaneous submissions.
    fn validates_submission_set() {
        let model = started_model(2);
        let first = TurnSubmission::new(1, model.turn, Vec::new());
        assert!(matches!(
            resolve_turn(&mut model.clone(), std::slice::from_ref(&first)),
            Err(GameError::MissingSubmission(2))
        ));
        assert!(matches!(
            resolve_turn(&mut model.clone(), &[first.clone(), first]),
            Err(GameError::DuplicateSubmission(1))
        ));
        let stale = model
            .players
            .iter()
            .map(|player| TurnSubmission::new(player.id, 0, Vec::new()))
            .collect::<Vec<_>>();
        assert!(matches!(
            resolve_turn(&mut model.clone(), &stale),
            Err(GameError::StaleTurn { .. })
        ));

        let oversized = TurnSubmission::new(
            1,
            model.turn,
            vec![
                TurnCommand::AbandonPlanet {
                    planet_id: 0
                };
                MAX_COMMANDS_PER_SUBMISSION + 1
            ],
        );
        let second = TurnSubmission::new(2, model.turn, Vec::new());
        assert!(matches!(
            resolve_turn(&mut model.clone(), &[oversized, second]),
            Err(GameError::InvalidCommand {
                player_id: 1,
                ..
            })
        ));
    }

    #[test]
    /// Rejects unsupported envelopes and broken cross-references without partially loading them.
    fn rejects_malformed_persisted_state() {
        let persisted = PersistedGame::new(started_model(2));
        let mut wrong_schema = persisted.to_json().unwrap();
        wrong_schema["schema_version"] = serde_json::json!(999);
        assert!(matches!(
            PersistedGame::from_json(wrong_schema),
            Err(GameError::UnsupportedSchema(999))
        ));

        let mut missing_home = persisted.to_json().unwrap();
        missing_home["state"]["players"][0]["home_planet"] = serde_json::json!(u64::MAX);
        assert!(matches!(
            PersistedGame::from_json(missing_home),
            Err(GameError::MalformedState(_))
        ));

        let mut duplicate_player = persisted.to_json().unwrap();
        duplicate_player["state"]["players"][1]["id"] = serde_json::json!(1);
        assert!(matches!(
            PersistedGame::from_json(duplicate_player),
            Err(GameError::MalformedState(_))
        ));

        let mut with_report = started_model(2);
        let origin = with_report.map.get(with_report.players[0].home_planet).clone();
        let destination = with_report.map.get(with_report.players[1].home_planet).clone();
        let mission = Mission::new_with_id(
            7,
            1,
            1,
            &origin,
            &destination,
            Icon::Attack,
            Army::new(),
            BombingRaid::None,
            false,
            false,
            None,
        );
        with_report.players[0].push_report(crate::core::combat::report::MissionReport {
            id: 7,
            turn: 1,
            mission,
            planet: destination.clone(),
            scout_probes: 0,
            surviving_attacker: Army::new(),
            surviving_defender: destination.army.clone(),
            planet_colonized: false,
            planet_destroyed: false,
            destination_owned: destination.owned,
            destination_controlled: destination.controlled,
            combat_report: None,
            hidden: false,
        });
        let mut invalid_report = PersistedGame::new(with_report).to_json().unwrap();
        invalid_report["state"]["players"][0]["reports"][0]["mission"]["destination"] =
            serde_json::json!(u64::MAX);
        assert!(matches!(
            PersistedGame::from_json(invalid_report),
            Err(GameError::MalformedState(_))
        ));
    }

    #[test]
    /// Losing the final opposing home world completes the match with one stable winner.
    fn resolution_completes_game() {
        let mut model = started_model(2);
        let defeated_home = model.players[1].home_planet;
        let planet = model.map.get_mut(defeated_home);
        planet.owned = Some(1);
        planet.controlled = Some(1);
        model.players[1].spectator = true;
        let submissions = model
            .players
            .iter()
            .filter(|player| !player.spectator)
            .map(|player| TurnSubmission::new(player.id, model.turn, Vec::new()))
            .collect::<Vec<_>>();

        let result = resolve_turn(&mut model, &submissions).unwrap();
        assert!(result.finished);
        assert_eq!(result.winner, Some(1));
        assert_eq!(model.status, MatchStatus::Finished);
    }

    proptest::proptest! {
        #![proptest_config(proptest::test_runner::Config::with_cases(16))]

        #[test]
        /// Round-trips arbitrary compact seeds without changing valid state.
        fn serialized_state_round_trips(seed in proptest::prelude::any::<u64>()) {
            let mut bytes = [0_u8; 32];
            bytes[..8].copy_from_slice(&seed.to_le_bytes());
            let model = GameModel::new(bytes, GameRules::default()).unwrap();
            let persisted = PersistedGame::new(model);
            let json = persisted.to_json().unwrap();
            let loaded = PersistedGame::from_json(json).unwrap();
            proptest::prop_assert_eq!(
                serde_json::to_vec(&persisted).unwrap(),
                serde_json::to_vec(&loaded).unwrap()
            );
        }

        #[test]
        /// Arbitrary valid games resolve deterministically and retain ownership/unit invariants.
        fn arbitrary_empty_turns_preserve_invariants(
            seed in proptest::array::uniform32(proptest::prelude::any::<u8>()),
            player_count in 2_u8..=4,
        ) {
            let rules = GameRules {
                player_count,
                ..GameRules::default()
            };
            let mut left = GameModel::new(seed, rules).unwrap();
            left.start().unwrap();
            let mut right = left.clone();
            let submissions = left
                .players
                .iter()
                .map(|player| TurnSubmission::new(player.id, left.turn, Vec::new()))
                .collect::<Vec<_>>();

            let left_result = resolve_turn(&mut left, &submissions).unwrap();
            let right_result = resolve_turn(&mut right, &submissions).unwrap();
            proptest::prop_assert_eq!(left_result, right_result);
            proptest::prop_assert_eq!(
                serde_json::to_vec(&left).unwrap(),
                serde_json::to_vec(&right).unwrap()
            );
            left.validate().unwrap();

            let player_ids = left.players.iter().map(|player| player.id).collect::<HashSet<_>>();
            proptest::prop_assert_eq!(player_ids.len(), usize::from(player_count));
            for planet in &left.map.planets {
                for owner in [planet.owned, planet.controlled].into_iter().flatten() {
                    proptest::prop_assert!(player_ids.contains(&owner));
                }
                for count in planet.army.values() {
                    proptest::prop_assert!(
                        serde_json::to_value(count).unwrap().as_u64().is_some()
                    );
                }
            }
        }
    }
}
