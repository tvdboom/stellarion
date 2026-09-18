//! Deterministic gameplay state and simultaneous-turn resolution without Bevy systems.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use rand::seq::SliceRandom;
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::combat::report::MissionReport;
use crate::core::combat::resolution::{
    resolve_combat_with_retreat_with_rng, MAX_COMBAT_ROUNDS, MAX_SHOTS_PER_UNIT_PER_ROUND,
};
use crate::core::constants::{
    ORBITAL_RAILGUN_DESTRUCTION_BASIS_POINTS_PER_LEVEL, ORBITAL_RAILGUN_FIRE_DEUTERIUM_COST,
    ORBITAL_RAILGUN_FIRE_ENERGY_COST,
    ORBITAL_RAILGUN_OVERLOADED_SHIELD_REDUCTION_BASIS_POINTS_PER_LEVEL,
    ORBITAL_RAILGUN_RANGE_PER_LEVEL, ORBITAL_RAILGUN_SHIELD_REDUCTION_BASIS_POINTS_PER_LEVEL,
};
use crate::core::energy::EnergyGrid;
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{
    Garrison, IndependentPopulation, Planet, PlanetId, ShieldOverloadState,
};
use crate::core::missions::{BombingRaid, FleetCombatOrders, JointAttackMission, Mission};
use crate::core::orders::{conversion_output, purchase_limit, validate_mission};
use crate::core::player::{Player, MAX_REPORTS_PER_PLAYER};
use crate::core::random::DeterministicRngState;
use crate::core::recycling::recycler_production;
use crate::core::resources::{ResourceName, Resources};
use crate::core::trading::{
    trading_post_capacity, trading_posts_are_adjacent, ResourceLoan, ResourceLoanTerm,
    TradeAgreement, TRADE_RESOURCES_PER_LEVEL,
};
use crate::core::units::buildings::{Building, FleetWithdrawal};
use crate::core::units::defense::Defense;
use crate::core::units::fauna::encounter_formation;
use crate::core::units::operations::{mine_building, MineMode, SenatePolicy, SpaceDockMode};
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Price, Unit};
use crate::utils::NameFromEnum;

/// Supported number of players in a multiplayer game.
pub const PLAYER_COUNT_RANGE: std::ops::RangeInclusive<u8> = 2..=MAX_MULTIPLAYER_PLAYERS;

/// Maximum number of members who may join an open multiplayer lobby.
pub const MAX_MULTIPLAYER_PLAYERS: u8 = 4;

/// Maximum number of intentional commands accepted from one player for one turn.
pub const MAX_COMMANDS_PER_SUBMISSION: usize = 1024;

/// Maximum number of simultaneous in-flight missions retained in persisted state.
pub const MAX_ACTIVE_MISSIONS: usize = 4096;

/// Maximum number of bilateral player pairs in a four-player turn.
pub const MAX_TRADES_PER_TURN: usize = 6;

const NO_UNIQUE_MISSION_ID: &str = "no unique mission identifier is available";

/// Supported per-player planet ownership percentages shown by match setup.
pub const COLONIZABLE_PERCENT_OPTIONS: [usize; 3] = [25, 35, 50];

/// Supported per-travel-turn space-fauna encounter percentages.
pub const SPACE_FAUNA_PERCENT_OPTIONS: [usize; 3] = [0, 15, 30];

/// Gameplay settings that affect deterministic state transitions.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameRules {
    /// Number of non-moon planets generated for each player.
    pub planets_per_player: usize,
    /// Percentage of map planets that one player may own.
    pub colonizable_percent: usize,
    /// Number of moons as a percentage of non-moon planets.
    pub moons_percent: usize,
    /// Chance that an eligible in-flight mission meets fauna on a travel turn.
    pub space_fauna_percent: usize,
    /// Whether unclaimed planets may reveal a fixed independent garrison on first contact.
    #[serde(default)]
    pub independent_populations: bool,
    /// Number of generated player slots in this snapshot.
    pub player_count: u8,
    /// Allows the debug-only local practice flow, including a solo game without a victory condition.
    pub practice_mode: bool,
}

impl GameRules {
    /// Validates supported setting boundaries before map generation.
    pub fn validate(&self) -> Result<(), GameError> {
        let valid_player_count = if self.practice_mode {
            (1..=MAX_MULTIPLAYER_PLAYERS).contains(&self.player_count)
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
        if !COLONIZABLE_PERCENT_OPTIONS.contains(&self.colonizable_percent) {
            return Err(GameError::InvalidSettings(
                "colonizable_percent must be 25, 35, or 50".to_string(),
            ));
        }
        if self.moons_percent > 100 {
            return Err(GameError::InvalidSettings("moons_percent must be in 0..=100".to_string()));
        }
        if !SPACE_FAUNA_PERCENT_OPTIONS.contains(&self.space_fauna_percent) {
            return Err(GameError::InvalidSettings(
                "space_fauna_percent must be 0, 15, or 30".to_string(),
            ));
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
            space_fauna_percent: 15,
            independent_populations: false,
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
    /// Elimination or territorial control has ended the match.
    Finished,
}

/// Public outcome of every Orbital Railgun shot combined against one world.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrbitalStrike {
    /// Turn on which the synchronized strike became visible.
    pub turn: u64,
    /// Stable origin worlds whose Railguns contributed one shot each.
    pub origins: Vec<PlanetId>,
    /// World struck by the unified beam, including moons.
    pub target: PlanetId,
    /// Combined deterministic destruction chance in hundredths of one percent.
    pub chance_basis_points: u16,
    /// Whether the combined roll permanently destroyed the target.
    pub destroyed: bool,
}

/// Complete deterministic gameplay snapshot persisted in Supabase.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GameModel {
    /// All configured player slots in stable identifier order.
    pub players: Vec<Player>,
    /// Complete map, ownership, production queues, and stationed units.
    pub map: Map,
    /// All in-flight missions, including missions hidden from some players.
    pub missions: Vec<Mission>,
    /// Public Orbital Railgun outcomes from the most recently resolved turn.
    pub orbital_strikes: Vec<OrbitalStrike>,
    /// Bilateral exchanges accepted during the current planning turn.
    pub trades: Vec<TradeAgreement>,
    /// At most one outstanding Resource Hub loan for each owned Trading Post.
    pub resource_loans: Vec<ResourceLoan>,
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
    /// Whether enough non-spectator empires remain to coordinate a joint attack.
    pub(crate) fn joint_attacks_enabled(&self) -> bool {
        self.players.iter().filter(|player| !player.spectator).count() >= 3
    }

    /// Territorial target, based on surviving non-moon worlds and starting player slots.
    pub fn planets_to_win(&self) -> usize {
        let planets = self
            .map
            .planets
            .iter()
            .filter(|planet| !planet.is_moon() && !planet.is_destroyed)
            .count();
        let players = u128::from(self.rules.player_count.max(1));
        ((planets as u128 * (players + 1)).div_ceil(2 * players)) as usize
    }

    /// Returns a surviving empire that has reached the territorial target.
    pub fn territorial_winner(&self) -> Option<PlayerId> {
        if self.rules.practice_mode && self.rules.player_count == 1 {
            return None;
        }
        let target = self.planets_to_win();
        self.players.iter().find_map(|player| {
            (player.owns(self.map.get(player.home_planet))
                && self
                    .map
                    .planets
                    .iter()
                    .filter(|planet| {
                        !planet.is_moon()
                            && !planet.is_destroyed
                            && planet.controlled == Some(player.id)
                    })
                    .count()
                    >= target)
                .then_some(player.id)
        })
    }

    /// Winner of a completed match, or none for an active match or mutual elimination.
    pub fn winner(&self) -> Option<PlayerId> {
        if self.status != MatchStatus::Finished {
            return None;
        }
        self.territorial_winner().or_else(|| {
            let mut survivors =
                self.players.iter().filter(|player| player.owns(self.map.get(player.home_planet)));
            let first = survivors.next()?;
            survivors.next().is_none().then_some(first.id)
        })
    }

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
        if rules.independent_populations {
            for planet in
                map.planets.iter_mut().filter(|planet| !planet.is_moon() && planet.owned.is_none())
            {
                planet.independent_population = IndependentPopulation::Unrevealed;
            }
        }

        let model = Self {
            players,
            map,
            missions: Vec::new(),
            orbital_strikes: Vec::new(),
            trades: Vec::new(),
            resource_loans: Vec::new(),
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

    /// Returns resources already reserved from one player by finalized current-turn trades.
    pub fn trade_outgoing(&self, player_id: PlayerId) -> Resources {
        self.trades
            .iter()
            .filter_map(|trade| trade.party(player_id).map(|party| party.resources))
            .sum()
    }

    /// Returns resources due to one player when the current turn resolves.
    pub fn trade_incoming(&self, player_id: PlayerId) -> Resources {
        self.trades.iter().map(|trade| trade.incoming(player_id)).sum()
    }

    /// Returns the fixed Resource Hub bundle attempted when the current turn resolves.
    pub fn resource_hub_repayment_due(&self, player_id: PlayerId) -> Resources {
        self.resource_loans
            .iter()
            .filter(|loan| loan.player_id == player_id && loan.due_turn <= self.turn)
            .map(ResourceLoan::repayment)
            .sum()
    }

    /// Whether a missed repayment prevents this empire from opening any new Resource Hub loan.
    pub fn resource_hub_borrowing_blocked(&self, player_id: PlayerId) -> bool {
        self.resource_loans
            .iter()
            .any(|loan| loan.player_id == player_id && loan.due_turn < self.turn)
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
        let map_size = self.map.rect.size();
        if !self.map.rect.min.is_finite()
            || !self.map.rect.max.is_finite()
            || !map_size.is_finite()
            || map_size.x <= 0.0
            || map_size.y <= 0.0
            || self.map.planets.iter().any(|planet| !planet.position.is_finite())
        {
            return Err(GameError::MalformedState(
                "map bounds and world positions must be finite with positive map dimensions"
                    .to_string(),
            ));
        }
        let planet_ids = self.map.planets.iter().map(|planet| planet.id).collect::<HashSet<_>>();

        if self.trades.len() > MAX_TRADES_PER_TURN {
            return Err(GameError::MalformedState(format!(
                "trade count exceeds {MAX_TRADES_PER_TURN} player pairs"
            )));
        }
        let mut trade_ids = HashSet::with_capacity(self.trades.len());
        let mut trade_pairs = HashSet::with_capacity(self.trades.len());
        for trade in &self.trades {
            let [first, second] = &trade.parties;
            let pair = (first.player_id, second.player_id);
            if self.status != MatchStatus::Active
                || trade.id == 0
                || trade.turn != self.turn
                || !trade_ids.insert(trade.id)
                || first.player_id >= second.player_id
                || !trade_pairs.insert(pair)
                || !player_ids.contains(&first.player_id)
                || !player_ids.contains(&second.player_id)
                || self.player(first.player_id).is_ok_and(|player| player.spectator)
                || self.player(second.player_id).is_ok_and(|player| player.spectator)
                || first.resources.is_empty()
                || second.resources.is_empty()
                || first.resources.total()
                    > self
                        .map
                        .try_get(first.planet_id)
                        .map_or(0, |planet| trading_post_capacity(planet, first.player_id))
                || second.resources.total()
                    > self
                        .map
                        .try_get(second.planet_id)
                        .map_or(0, |planet| trading_post_capacity(planet, second.player_id))
                || !trading_posts_are_adjacent(
                    &self.map,
                    first.player_id,
                    first.planet_id,
                    second.player_id,
                    second.planet_id,
                )
            {
                return Err(GameError::MalformedState(format!(
                    "trade {} contains an invalid agreement",
                    trade.id
                )));
            }
        }
        for player in &self.players {
            let outgoing = self.trade_outgoing(player.id);
            let incoming = self.trade_incoming(player.id);
            if !player.resources.contains(outgoing)
                || player.resources.metal.checked_add(incoming.metal).is_none()
                || player.resources.crystal.checked_add(incoming.crystal).is_none()
                || player.resources.deuterium.checked_add(incoming.deuterium).is_none()
            {
                return Err(GameError::MalformedState(format!(
                    "player {} cannot settle current trades",
                    player.id
                )));
            }
        }

        let mut loan_posts = HashSet::with_capacity(self.resource_loans.len());
        if self.resource_loans.len() > self.map.planets.len() {
            return Err(GameError::MalformedState(
                "resource loan count exceeds planet count".to_string(),
            ));
        }
        for loan in &self.resource_loans {
            let expected_due = loan.issued_turn.checked_add(loan.term.turns());
            if !player_ids.contains(&loan.player_id)
                || !planet_ids.contains(&loan.planet_id)
                || !loan_posts.insert(loan.planet_id)
                || loan.issued_turn == 0
                || loan.issued_turn >= self.turn
                || expected_due != Some(loan.due_turn)
                || loan.principal.is_empty()
                || loan.principal.total()
                    > TRADE_RESOURCES_PER_LEVEL.saturating_mul(Building::MAX_LEVEL)
            {
                return Err(GameError::MalformedState(format!(
                    "player {} has an invalid Resource Market loan",
                    loan.player_id
                )));
            }
        }

        for player in &self.players {
            let mut acquired = HashSet::new();
            if player.world_acquisition_order.first() != Some(&player.home_planet)
                || player
                    .world_acquisition_order
                    .iter()
                    .any(|id| !planet_ids.contains(id) || !acquired.insert(*id))
            {
                return Err(GameError::MalformedState(format!(
                    "player {} has an invalid world acquisition order",
                    player.id
                )));
            }
            if !player.color.is_valid() {
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
            if player.protection_intel.iter().any(|(planet, controller)| {
                !planet_ids.contains(planet) || !player_ids.contains(controller)
            }) {
                return Err(GameError::MalformedState(format!(
                    "player {} contains invalid protection intelligence",
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
                    mission.protected_player,
                    report.planet.owned,
                    report.planet.controlled,
                    report.destination_owned,
                    report.destination_controlled,
                ]
                .into_iter()
                .flatten()
                .chain(report.planet.protection_permissions.iter().copied())
                .chain(report.planet.army.protector_ids())
                .chain(report.surviving_defender.protector_ids())
                .all(|id| player_ids.contains(&id));
                let defending_forces_are_valid = valid_protection_fleets(
                    &report.planet.army,
                    report.planet.controlled.or(report.planet.owned),
                    &player_ids,
                ) && valid_protection_fleets(
                    &report.surviving_defender,
                    report.planet.controlled.or(report.planet.owned),
                    &player_ids,
                );
                let joint_attack_is_valid =
                    valid_joint_attack(mission, &player_ids, &planet_ids, true);
                let combat_is_bounded = report.combat_report.as_ref().is_none_or(|combat| {
                    combat.rounds.len() <= MAX_COMBAT_ROUNDS
                        && combat.defender_retreat.as_ref().is_none_or(|retreat| {
                            planet_ids.contains(&retreat.home_planet)
                                && retreat.home_planet != report.planet.id
                                && retreat
                                    .after_round
                                    .is_none_or(|index| index < combat.rounds.len())
                                && retreat.ships.has_army()
                                && retreat.ships.keys().all(Unit::is_ship)
                        })
                        && combat.rounds.iter().all(|round| {
                            round.destroy_probability.is_finite()
                                && (0.0..=1.0).contains(&round.destroy_probability)
                                && round.attacker.iter().chain(&round.defender).all(|unit| {
                                    unit.owner.is_none_or(|owner| player_ids.contains(&owner))
                                        && unit.shots.len() <= MAX_SHOTS_PER_UNIT_PER_ROUND + 1
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
                    || (mission.deep_cover && mission.objective != Icon::Spy)
                    || !mission.position.is_finite()
                    || mission.send > report.turn
                    || !u64::try_from(report.turn).is_ok_and(|turn| turn <= self.turn)
                    || !references_known_players
                    || !defending_forces_are_valid
                    || !joint_attack_is_valid
                    || !combat_is_bounded
                {
                    return Err(GameError::MalformedState(format!(
                        "player {} report {} contains invalid or unbounded history",
                        player.id, report.id
                    )));
                }
            }
        }
        let protection_enabled =
            self.players.iter().filter(|player| !player.spectator).count() >= 3;
        for planet in &self.map.planets {
            let independent_state_valid = match planet.independent_population {
                IndependentPopulation::Empty => true,
                IndependentPopulation::Unrevealed => {
                    !planet.is_moon()
                        && !planet.is_destroyed
                        && planet.owned.is_none()
                        && planet.controlled.is_none()
                        && planet.army.is_empty()
                        && planet.buy.is_empty()
                },
                IndependentPopulation::Inhabited => {
                    !planet.is_moon()
                        && !planet.is_destroyed
                        && planet.owned.is_none()
                        && planet.controlled.is_none()
                        && planet.army.protector_ids().next().is_none()
                        && planet.buy.is_empty()
                        && independent_population_army_is_valid(planet.army.controller())
                },
            };
            if planet
                .operations
                .mines
                .iter()
                .any(|mine| mine.recovering && mine.mode != MineMode::Suspended)
                || planet.operations.space_dock_locked_until
                    > self.turn.saturating_add(SpaceDockMode::COMMITMENT_TURNS)
                || (planet.operations.space_dock_selection_pending
                    && self.turn < planet.operations.space_dock_locked_until)
                || planet.operations.senate_locked_until
                    > self.turn.saturating_add(SenatePolicy::COMMITMENT_TURNS)
                || (planet.operations.senate_selection_pending
                    && self.turn < planet.operations.senate_locked_until)
            {
                return Err(GameError::MalformedState(format!(
                    "planet {} contains invalid operating state",
                    planet.id
                )));
            }
            for owner in [planet.owned, planet.controlled].into_iter().flatten() {
                if !player_ids.contains(&owner) {
                    return Err(GameError::MalformedState(format!(
                        "planet {} references unknown player {owner}",
                        planet.id
                    )));
                }
            }
            let permissions_valid = planet.protection_permissions.iter().all(|protector| {
                player_ids.contains(protector) && planet.controlled != Some(*protector)
            });
            let protecting_fleets_valid =
                valid_protection_fleets(&planet.army, planet.controlled, &player_ids);
            if !independent_state_valid {
                return Err(GameError::MalformedState(format!(
                    "planet {} contains invalid independent population state",
                    planet.id
                )));
            }
            if !permissions_valid
                || !protecting_fleets_valid
                || (!protection_enabled
                    && (!planet.protection_permissions.is_empty()
                        || planet.army.protector_ids().next().is_some()))
                || (planet.is_destroyed
                    && (!planet.protection_permissions.is_empty()
                        || planet.army.protector_ids().next().is_some()))
            {
                return Err(GameError::MalformedState(format!(
                    "planet {} contains invalid protection state",
                    planet.id
                )));
            }
        }
        for strike in &self.orbital_strikes {
            let mut origins = HashSet::with_capacity(strike.origins.len());
            if strike.turn != self.turn
                || strike.origins.is_empty()
                || strike.chance_basis_points > 10_000
                || !planet_ids.contains(&strike.target)
                || strike
                    .origins
                    .iter()
                    .any(|origin| !planet_ids.contains(origin) || !origins.insert(*origin))
                || (strike.destroyed && !self.map.get(strike.target).is_destroyed)
            {
                return Err(GameError::MalformedState(
                    "orbital strike history contains an invalid reference or outcome".to_string(),
                ));
            }
        }
        if self.missions.len() > MAX_ACTIVE_MISSIONS {
            return Err(GameError::MalformedState(format!(
                "active mission count exceeds {MAX_ACTIVE_MISSIONS} entries"
            )));
        }
        let mut mission_ids = HashSet::with_capacity(self.missions.len());
        for mission in &self.missions {
            let mission_references_known_players =
                [mission.origin_owned, mission.origin_controlled, mission.protected_player]
                    .into_iter()
                    .flatten()
                    .all(|id| player_ids.contains(&id));
            let joint_attack_is_valid =
                valid_joint_attack(mission, &player_ids, &planet_ids, false);
            if !player_ids.contains(&mission.owner)
                || !planet_ids.contains(&mission.origin)
                || !planet_ids.contains(&mission.destination)
                || mission.origin == mission.destination
                || !mission.objective.is_mission()
                || !mission.has_valid_return_objective()
                || (mission.deep_cover && mission.objective != Icon::Spy)
                || mission.id == 0
                || !mission_ids.insert(mission.id)
                || !mission.position.is_finite()
                || !mission_references_known_players
                || !joint_attack_is_valid
                || mission.send == 0
                || !u64::try_from(mission.send).is_ok_and(|turn| turn <= self.turn)
                || !u64::try_from(mission.travel_turns).is_ok_and(|age| age <= self.turn)
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

fn valid_joint_attack(
    mission: &Mission,
    player_ids: &HashSet<PlayerId>,
    planet_ids: &HashSet<PlanetId>,
    resolved: bool,
) -> bool {
    let Some(attack) = &mission.joint_attack else {
        return true;
    };
    let valid_army = |army: &Army| {
        army.has_army() && army.iter().all(|(unit, count)| unit.is_ship() && *count > 0)
    };
    attack.id > 0
        && matches!(mission.objective, Icon::Colonize | Icon::Attack | Icon::Destroy)
        && attack.arrival_turn >= mission.send
        && player_ids.contains(&attack.leader)
        && attack.attackers.len() >= 2
        && attack.attackers.contains_key(&attack.leader)
        && attack.attackers.contains_key(&mission.owner)
        && attack.attackers.keys().eq(attack.origins.keys())
        && attack.attackers.keys().eq(attack.combat_orders.keys())
        && attack.attackers.iter().all(|(player, army)| {
            player_ids.contains(player)
                && valid_army(army)
                && attack.origins.get(player).is_some_and(|origin| planet_ids.contains(origin))
        })
        && attack
            .survivors
            .iter()
            .all(|(player, army)| attack.attackers.contains_key(player) && valid_army(army))
        && attack.scouts.iter().all(|(owner, count)| {
            *count > 0
                && *count
                    <= attack.survivors.get(owner).map_or(0, |army| army.amount(&Unit::probe()))
        })
        && (resolved || (attack.survivors.is_empty() && attack.scouts.is_empty()))
}

fn valid_protection_fleets(
    garrison: &Garrison,
    controller: Option<PlayerId>,
    player_ids: &HashSet<PlayerId>,
) -> bool {
    let mut has_protector = false;
    let fleets_are_valid = garrison.protectors().all(|(owner, army)| {
        has_protector = true;
        player_ids.contains(&owner)
            && controller != Some(owner)
            && army.has_army()
            && army.iter().all(|(unit, count)| unit.is_ship() && *count > 0)
    });
    fleets_are_valid && (!has_protector || controller.is_some())
}

/// Restricts generated inhabitants to their four level-one colony buildings, combat ships no
/// stronger than Cruisers, and ground units no stronger than Gauss Cannons.
fn independent_population_army_is_valid(army: &Army) -> bool {
    let mut has_combat_unit = false;
    army.iter().all(|(unit, count)| {
        if *count == 0 {
            return false;
        }
        match unit {
            Unit::Building(
                Building::MetalMine
                | Building::CrystalMine
                | Building::DeuteriumSynthesizer
                | Building::Reactor,
            ) => *count == 1,
            Unit::Ship(
                Ship::LightFighter | Ship::HeavyFighter | Ship::Destroyer | Ship::Cruiser,
            ) => {
                has_combat_unit = true;
                true
            },
            Unit::Defense(
                Defense::Crawler
                | Defense::RepairTruck
                | Defense::RocketLauncher
                | Defense::LightLaser
                | Defense::HeavyLaser
                | Defense::GaussCannon,
            ) => {
                has_combat_unit = true;
                true
            },
            _ => false,
        }
    }) && has_combat_unit
}

/// Validated snapshot stored in the database JSON column.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PersistedGame {
    /// Complete deterministic game state.
    pub state: GameModel,
}

impl PersistedGame {
    /// Wraps current game state for persistence.
    pub fn new(state: GameModel) -> Self {
        Self {
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

    /// Validates every core cross-reference after transport decoding.
    pub fn validate(&self) -> Result<(), GameError> {
        self.state.validate()
    }

    /// Serializes this envelope to the database JSON representation.
    pub fn to_json(&self) -> Result<serde_json::Value, GameError> {
        serde_json::to_value(self).map_err(|error| GameError::MalformedState(error.to_string()))
    }
}

/// Intentional gameplay command submitted by one player.
#[derive(Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TurnCommand {
    /// Borrows one bundle through an owned completed Trading Post.
    BorrowResources {
        /// Owned planet whose Trading Post determines the combined borrowing capacity.
        planet_id: PlanetId,
        /// Metal, Crystal, and Deuterium added before the remaining draft is applied.
        resources: Resources,
        /// Fixed repayment delay and premium.
        term: ResourceLoanTerm,
    },
    /// Repays the complete fixed amount of one Trading Post loan before it is due.
    RepayResourceLoanEarly {
        /// Trading Post whose outstanding loan is repaid.
        planet_id: PlanetId,
    },
    /// Selects the operating mode of a completed mine or synthesizer.
    SetMineMode {
        /// Owned planet containing the extraction building.
        planet_id: PlanetId,
        /// Resource whose extraction setting changes.
        resource: ResourceName,
        /// Normal, Intensive, or Suspended operation.
        mode: MineMode,
    },
    /// Chooses bulk or selective recovery for a completed Recycler.
    SetRecyclerFocus {
        /// Owned planet containing the Recycler.
        planet_id: PlanetId,
        /// Selected resource, or `None` for bulk recovery.
        resource: Option<ResourceName>,
    },
    /// Commits a completed Space Dock to a specialization for three turns.
    SetSpaceDockMode {
        /// Owned planet containing the Space Dock.
        planet_id: PlanetId,
        /// Industrial production or Bastion defense.
        mode: SpaceDockMode,
    },
    /// Selects the home-world Senate's empire-wide production policy.
    SetSenatePolicy {
        /// Owned home planet containing a completed Senate.
        planet_id: PlanetId,
        /// Ship production or defense production across the empire.
        policy: SenatePolicy,
    },
    /// Adds testing resources and units to owned planets and buildings to controlled moons.
    PracticeBoost,
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
    /// Selects the resource specialized by a completed planetary Terraformer.
    SetTerraformerFocus {
        /// Planet containing the Terraformer.
        planet_id: PlanetId,
        /// Resource receiving the specialization bonus, or `None` to switch the building off.
        resource: Option<ResourceName>,
    },
    /// Enables or disables a completed planetary Command Relay.
    SetCommandRelay {
        /// Planet containing the Relay.
        planet_id: PlanetId,
        /// Whether the Relay should broadcast deceptive telemetry.
        active: bool,
    },
    /// Enables or cancels a one-turn Planetary Shield overload before resolution.
    SetPlanetaryShieldOverload {
        /// Planet containing the completed shield.
        planet_id: PlanetId,
        /// Whether the shield should be overloaded for the next turn.
        active: bool,
    },
    /// Sets the standing fleet withdrawal order on an administered colony.
    SetFleetWithdrawal {
        /// Owned non-home planet containing the Administration.
        planet_id: PlanetId,
        /// Loss threshold, immediate withdrawal, or off.
        withdrawal: FleetWithdrawal,
    },
    /// Fires every owned Orbital Railgun that can reach one target during this simultaneous turn.
    FireOrbitalRailguns {
        /// Non-allied planet reached by at least one owned Railgun.
        target: PlanetId,
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
        /// Whether the Spy mission attempts to avoid combat using the origin's Command Relay.
        deep_cover: bool,
        /// Whether to use a jump gate.
        jump_gate: bool,
    },
    /// Launches every explicitly accepted contribution as one synchronized hostile mission.
    SendJointMission {
        /// Stable invitation identifier.
        attack_id: u64,
        /// Client-selected identifier for the lead contingent and shared report.
        mission_id: u64,
        /// Target selected by the inviter.
        destination: PlanetId,
        /// Inviter-selected hostile objective.
        objective: Icon,
        /// Optional shared bomber target class.
        bombing: BombingRaid,
        /// Whether probes remain in the combined combat force.
        combat_probes: bool,
        /// Accepted fleets, including the inviter, in player-slot order.
        contributions: Vec<JointAttackContribution>,
    },
    /// Recalls an active fleet, canceling its launch entirely when it was sent this turn.
    RecallMission {
        /// Stable identifier of the active mission to recall.
        mission_id: u64,
    },
    /// Recalls an entire protection fleet already stationed on another player's world.
    RecallProtection {
        /// Client-selected identifier for the protected return mission.
        mission_id: u64,
        /// World currently holding the player's protection fleet.
        planet_id: PlanetId,
    },
}

/// One player's accepted fleet in a coordinated hostile mission.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointAttackContribution {
    /// Commander retaining ownership of these ships.
    pub player_id: PlayerId,
    /// World from which this fleet departs.
    pub origin: PlanetId,
    /// Ships committed by this commander.
    pub army: Army,
    /// Shared bombing policy copied from the allied-attack owner's selection.
    pub bombing: BombingRaid,
    /// Whether this commander's Probes remain in combat.
    pub combat_probes: bool,
}

/// All commands one player commits for one simultaneous turn.
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnSubmission {
    /// Stable player slot submitting the commands.
    pub player_id: PlayerId,
    /// Turn to which the commands apply.
    pub turn: u64,
    /// Readiness attempt, advanced when the player continues an unfinished turn.
    /// This orders network retries only; it does not affect deterministic gameplay.
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
#[serde(deny_unknown_fields)]
pub struct TurnResult {
    /// Newly available turn after resolution.
    pub turn: u64,
    /// Whether the match ended during this resolution.
    pub finished: bool,
    /// Sole remaining player when the match ended, if any.
    #[serde(deserialize_with = "crate::serialization::required_option")]
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
    let (next, result) = resolved_turn(state, submissions)?;
    *state = next;
    Ok(result)
}

/// Resolves into one owned working copy so validation and publication can borrow the input.
/// All callers share this path; failures leave the original snapshot untouched.
pub(crate) fn resolved_turn(
    state: &GameModel,
    submissions: &[TurnSubmission],
) -> Result<(GameModel, TurnResult), GameError> {
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
    if let Some(missing) = required.difference(&seen).min() {
        return Err(GameError::MissingSubmission(*missing));
    }

    let mut working = state.clone();
    working.orbital_strikes.clear();
    // Finalized trades reserve both players' outgoing balances before any ordered spending.
    // Delivery waits until every draft has applied, keeping received resources for the next turn.
    reserve_trade_resources(&mut working)?;
    let mut ordered = submissions.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|submission| submission.player_id);
    // Testing boosts create completed infrastructure used by later commands in the same draft.
    // Apply them before Resource Hub borrowing and joint-fleet reservations so the authoritative
    // resolution matches the draft preview shown by the client.
    for submission in &ordered {
        for command in submission
            .commands
            .iter()
            .filter(|command| matches!(command, TurnCommand::PracticeBoost))
        {
            apply_command(&mut working, submission.player_id, command)?;
        }
    }
    // Resource Hub credit is available to the rest of the same draft, regardless of where the
    // command appears in the player's interaction order.
    for submission in &ordered {
        for command in submission
            .commands
            .iter()
            .filter(|command| matches!(command, TurnCommand::BorrowResources { .. }))
        {
            apply_command(&mut working, submission.player_id, command)?;
        }
    }
    // Accepted joint fleets are reservations. Apply them before ordinary per-player orders so
    // resolution never depends on whether the inviter has a lower or higher player slot.
    for submission in &ordered {
        for command in submission
            .commands
            .iter()
            .filter(|command| matches!(command, TurnCommand::SendJointMission { .. }))
        {
            apply_command(&mut working, submission.player_id, command)?;
        }
    }
    for submission in ordered {
        let ordinary = submission.commands.iter().filter(|command| {
            !matches!(
                command,
                TurnCommand::BorrowResources { .. }
                    | TurnCommand::PracticeBoost
                    | TurnCommand::SendJointMission { .. }
            )
        });
        apply_commands(&mut working, submission.player_id, ordinary)?;
    }

    deliver_trade_resources(&mut working)?;
    advance_simulation(&mut working)?;
    working.validate()?;
    let winner = working.winner();
    let result = TurnResult {
        turn: working.turn,
        finished: working.status == MatchStatus::Finished,
        winner,
    };
    Ok((working, result))
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
    resolved_turn(state, &complete).map(|_| ())
}

/// Projects one player's orders without advancing the simultaneous turn.
/// Used when restoring a saved draft; other players' orders remain invisible.
pub fn preview_commands(
    state: &GameModel,
    player_id: PlayerId,
    commands: &[TurnCommand],
) -> Result<GameModel, GameError> {
    preview_commands_with_allied_launches(state, player_id, commands, &[])
}

/// Projects published allied launches alongside the local draft, before ordinary orders.
/// Invitations carry only shared launch orders; opponents' private drafts remain invisible.
pub(crate) fn preview_commands_with_allied_launches(
    state: &GameModel,
    player_id: PlayerId,
    commands: &[TurnCommand],
    allied_launches: &[(PlayerId, TurnCommand)],
) -> Result<GameModel, GameError> {
    let mut preview = state.clone();
    preview.orbital_strikes.clear();
    reserve_trade_resources(&mut preview)?;
    // Match full-turn resolution: testing setup must exist before a Resource Hub or shared fleet
    // validates the completed infrastructure and units shown in the draft preview.
    for command in commands.iter().filter(|command| matches!(command, TurnCommand::PracticeBoost)) {
        apply_command(&mut preview, player_id, command)?;
    }
    for command in
        commands.iter().filter(|command| matches!(command, TurnCommand::BorrowResources { .. }))
    {
        apply_command(&mut preview, player_id, command)?;
    }
    let mut launched = BTreeSet::new();
    for command in commands {
        if let TurnCommand::SendJointMission {
            attack_id,
            ..
        } = command
        {
            apply_command(&mut preview, player_id, command)?;
            launched.insert(*attack_id);
        }
    }
    for (owner, command) in allied_launches {
        if let TurnCommand::SendJointMission {
            attack_id,
            ..
        } = command
        {
            if launched.insert(*attack_id) {
                apply_command(&mut preview, *owner, command)?;
            }
        }
    }
    apply_commands(
        &mut preview,
        player_id,
        commands.iter().filter(|command| {
            !matches!(
                command,
                TurnCommand::BorrowResources { .. }
                    | TurnCommand::PracticeBoost
                    | TurnCommand::SendJointMission { .. }
            )
        }),
    )?;
    Ok(preview)
}

/// Adds one backend-authenticated bilateral agreement without partially changing the snapshot.
pub fn add_trade_agreement_immediately(
    model: &mut GameModel,
    agreement: TradeAgreement,
) -> Result<(), GameError> {
    model.trades.push(agreement);
    if let Err(error) = model.validate() {
        model.trades.pop();
        return Err(error);
    }
    Ok(())
}

fn reserve_trade_resources(model: &mut GameModel) -> Result<(), GameError> {
    for player in &mut model.players {
        let outgoing: Resources = model
            .trades
            .iter()
            .filter_map(|trade| trade.party(player.id).map(|party| party.resources))
            .sum();
        if !player.resources.contains(outgoing) {
            return Err(GameError::MalformedState(format!(
                "player {} cannot reserve current trades",
                player.id
            )));
        }
        player.resources.metal -= outgoing.metal;
        player.resources.crystal -= outgoing.crystal;
        player.resources.deuterium -= outgoing.deuterium;
    }
    Ok(())
}

fn deliver_trade_resources(model: &mut GameModel) -> Result<(), GameError> {
    for player in &mut model.players {
        let incoming: Resources = model.trades.iter().map(|trade| trade.incoming(player.id)).sum();
        player.resources.metal =
            player.resources.metal.checked_add(incoming.metal).ok_or_else(|| {
                GameError::MalformedState("trade metal balance overflow".to_string())
            })?;
        player.resources.crystal =
            player.resources.crystal.checked_add(incoming.crystal).ok_or_else(|| {
                GameError::MalformedState("trade crystal balance overflow".to_string())
            })?;
        player.resources.deuterium =
            player.resources.deuterium.checked_add(incoming.deuterium).ok_or_else(|| {
                GameError::MalformedState("trade deuterium balance overflow".to_string())
            })?;
    }
    model.trades.clear();
    Ok(())
}

/// Applies one player's ordered draft while retaining transient state needed to undo a launch.
fn apply_commands<'a>(
    model: &mut GameModel,
    player_id: PlayerId,
    commands: impl IntoIterator<Item = &'a TurnCommand>,
) -> Result<(), GameError> {
    let mut launch_permissions = BTreeMap::<u64, BTreeSet<PlayerId>>::new();
    for command in commands {
        if let TurnCommand::SendMission {
            mission_id,
            origin,
            ..
        } = command
        {
            if let Some(origin) = model.map.try_get(*origin) {
                launch_permissions.insert(*mission_id, origin.protection_permissions.clone());
            }
        }

        let canceled_origin = if let TurnCommand::RecallMission {
            mission_id,
        } = command
        {
            model
                .missions
                .iter()
                .find(|mission| {
                    mission.id == *mission_id
                        && mission.owner == player_id
                        && usize::try_from(model.turn).is_ok_and(|turn| mission.send == turn)
                        && mission.travel_turns == 0
                })
                .map(|mission| mission.origin)
        } else {
            None
        };
        apply_command(model, player_id, command)?;

        if let (
            TurnCommand::RecallMission {
                mission_id,
            },
            Some(origin),
        ) = (command, canceled_origin)
        {
            if let Some(permissions) = launch_permissions.remove(mission_id) {
                model.map.get_mut(origin).protection_permissions = permissions;
            }
        }
    }
    Ok(())
}

/// Selects well-separated home planets without an unbounded rejection loop.
fn choose_home_planets<R: Rng + ?Sized>(
    map: &Map,
    count: usize,
    rng: &mut R,
) -> Result<Vec<PlanetId>, GameError> {
    let all_planets = map.planets();
    let temperate = all_planets
        .iter()
        .copied()
        .filter(|planet| {
            map.solar_band(planet.id) == Some(crate::core::map::planet::SolarBand::Temperate)
        })
        .collect::<Vec<_>>();
    let planets = if temperate.len() >= count {
        temperate
    } else {
        all_planets
    };
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
        TurnCommand::BorrowResources {
            planet_id,
            resources,
            term,
        } => apply_resource_loan(model, player_id, *planet_id, *resources, *term),
        TurnCommand::RepayResourceLoanEarly {
            planet_id,
        } => apply_early_resource_loan_repayment(model, player_id, *planet_id),
        TurnCommand::PracticeBoost => apply_testing_boost(model, player_id),
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
        TurnCommand::SetTerraformerFocus {
            planet_id,
            resource,
        } => apply_terraformer_focus(model, player_id, *planet_id, *resource),
        TurnCommand::SetMineMode {
            planet_id,
            resource,
            mode,
        } => apply_mine_mode(model, player_id, *planet_id, *resource, *mode),
        TurnCommand::SetRecyclerFocus {
            planet_id,
            resource,
        } => apply_recycler_focus(model, player_id, *planet_id, *resource),
        TurnCommand::SetSpaceDockMode {
            planet_id,
            mode,
        } => apply_space_dock_mode(model, player_id, *planet_id, *mode),
        TurnCommand::SetSenatePolicy {
            planet_id,
            policy,
        } => apply_senate_policy(model, player_id, *planet_id, *policy),
        TurnCommand::SetCommandRelay {
            planet_id,
            active,
        } => apply_command_relay(model, player_id, *planet_id, *active),
        TurnCommand::SetPlanetaryShieldOverload {
            planet_id,
            active,
        } => apply_planetary_shield_overload(model, player_id, *planet_id, *active),
        TurnCommand::SetFleetWithdrawal {
            planet_id,
            withdrawal,
        } => apply_fleet_withdrawal(model, player_id, *planet_id, *withdrawal),
        TurnCommand::FireOrbitalRailguns {
            target,
        } => apply_orbital_railgun_fire(model, player_id, *target),
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
            deep_cover,
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
            *deep_cover,
            *jump_gate,
        ),
        TurnCommand::SendJointMission {
            attack_id,
            mission_id,
            destination,
            objective,
            bombing,
            combat_probes,
            contributions,
        } => apply_joint_mission(
            model,
            player_id,
            *attack_id,
            *mission_id,
            *destination,
            *objective,
            bombing.clone(),
            *combat_probes,
            contributions,
        ),
        TurnCommand::RecallMission {
            mission_id,
        } => apply_recall(model, player_id, *mission_id),
        TurnCommand::RecallProtection {
            mission_id,
            planet_id,
        } => apply_protection_recall(model, player_id, *mission_id, *planet_id),
    }
}

/// Converts a persisted turn for command handling while preserving the acting player's context.
fn command_turn(model: &GameModel, player_id: PlayerId) -> Result<usize, GameError> {
    usize::try_from(model.turn)
        .map_err(|_| invalid_error(player_id, "turn cannot be represented on this platform"))
}

/// Converts a persisted turn while validating platform compatibility for simulation state.
fn simulation_turn(turn: u64) -> Result<usize, GameError> {
    usize::try_from(turn)
        .map_err(|_| GameError::MalformedState("turn exceeds this platform's limits".to_string()))
}

fn apply_resource_loan(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    resources: Resources,
    term: ResourceLoanTerm,
) -> Result<(), GameError> {
    let player = model.player(player_id)?;
    if model.resource_hub_borrowing_blocked(player_id) {
        return invalid(player_id, "overdue Resource Market repayments block all new borrowing");
    }
    if player.spectator || model.resource_loans.iter().any(|loan| loan.planet_id == planet_id) {
        return invalid(
            player_id,
            "this Trading Post already has an outstanding Resource Market loan",
        );
    }
    let capacity =
        model.map.try_get(planet_id).map_or(0, |planet| trading_post_capacity(planet, player_id));
    if resources.is_empty() || resources.total() > capacity {
        return invalid(
            player_id,
            "Resource Market borrowing exceeds the selected Trading Post's capacity",
        );
    }
    let due_turn = model
        .turn
        .checked_add(term.turns())
        .ok_or_else(|| invalid_error(player_id, "Resource Market repayment turn is exhausted"))?;
    let balance = model.player(player_id)?.resources;
    let updated =
        Resources::new(
            balance.metal.checked_add(resources.metal).ok_or_else(|| {
                invalid_error(player_id, "Resource Market metal balance overflow")
            })?,
            balance.crystal.checked_add(resources.crystal).ok_or_else(|| {
                invalid_error(player_id, "Resource Market crystal balance overflow")
            })?,
            balance.deuterium.checked_add(resources.deuterium).ok_or_else(|| {
                invalid_error(player_id, "Resource Market deuterium balance overflow")
            })?,
        );
    model.player_mut(player_id)?.resources = updated;
    model.resource_loans.push(ResourceLoan {
        player_id,
        planet_id,
        issued_turn: model.turn,
        due_turn,
        principal: resources,
        term,
    });
    Ok(())
}

fn apply_early_resource_loan_repayment(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
) -> Result<(), GameError> {
    let (index, repayment) = model
        .resource_loans
        .iter()
        .enumerate()
        .find(|(_, loan)| loan.player_id == player_id && loan.planet_id == planet_id)
        .map(|(index, loan)| (index, loan.repayment()))
        .ok_or_else(|| invalid_error(player_id, "this Trading Post has no Resource Market loan"))?;
    let player = model.player_mut(player_id)?;
    if !player.resources.contains(repayment) {
        return invalid(player_id, "not enough resources for early Resource Market repayment");
    }
    player.resources -= repayment;
    model.resource_loans.remove(index);
    Ok(())
}

/// Rejects a command before it would exceed the persisted active-mission bound.
fn ensure_mission_capacity(
    model: &GameModel,
    player_id: PlayerId,
    additional: usize,
) -> Result<(), GameError> {
    if model.missions.len().saturating_add(additional) > MAX_ACTIVE_MISSIONS {
        return invalid(
            player_id,
            format!("active mission limit of {MAX_ACTIVE_MISSIONS} reached"),
        );
    }
    Ok(())
}

/// Builds the same deterministic homeward mission for every protection-return path.
enum ProtectionReturnReason {
    Recalled,
    AccessRevoked,
}

fn protection_return_mission(
    mission_id: u64,
    turn: usize,
    owner: PlayerId,
    origin: &Planet,
    home: &Planet,
    army: Army,
    reason: ProtectionReturnReason,
) -> Mission {
    let log = match reason {
        ProtectionReturnReason::Recalled => format!(
            "- ({turn}) Protection fleet recalled from {}; returning to home planet {}.",
            origin.name, home.name
        ),
        ProtectionReturnReason::AccessRevoked => format!(
            "- ({turn}) Protection access at {} canceled; returning to home planet {}.",
            origin.name, home.name
        ),
    };
    Mission::new_with_id(
        mission_id,
        turn,
        owner,
        origin,
        home,
        Icon::Deploy,
        army,
        BombingRaid::None,
        false,
        false,
        Some(log),
    )
    .with_return_objective(Icon::Protect)
}

/// Recalls one player's entire stationed protection fleet directly to their home planet.
fn apply_protection_recall(
    model: &mut GameModel,
    player_id: PlayerId,
    mission_id: u64,
    planet_id: PlanetId,
) -> Result<(), GameError> {
    ensure_mission_capacity(model, player_id, 1)?;
    if mission_id == 0 || model.missions.iter().any(|mission| mission.id == mission_id) {
        return invalid(player_id, "protection recall identifier is missing or already used");
    }
    let home_planet = model.player(player_id)?.home_planet;
    let origin = model
        .map
        .try_get(planet_id)
        .ok_or_else(|| invalid_error(player_id, "protection recall world does not exist"))?
        .clone();
    if origin.id == home_planet
        || origin.is_destroyed
        || !origin.is_protected_by(player_id)
        || model.map.get(home_planet).is_destroyed
    {
        return invalid(player_id, "no protection fleet can be recalled from this world");
    }
    let turn = command_turn(model, player_id)?;
    let army = model
        .map
        .get_mut(planet_id)
        .army
        .remove_protector(player_id)
        .ok_or_else(|| invalid_error(player_id, "protection fleet is no longer stationed"))?;
    let home = model.map.get(home_planet);
    let mission = protection_return_mission(
        mission_id,
        turn,
        player_id,
        &origin,
        home,
        army,
        ProtectionReturnReason::Recalled,
    );
    model.missions.push(mission);
    Ok(())
}

/// Builds a surviving fleet's standard return leg after a completed special objective.
fn objective_return_mission(
    mission_id: u64,
    turn: usize,
    owner: PlayerId,
    origin: &Planet,
    home: &Planet,
    army: Army,
    prior_logs: &str,
    return_objective: Icon,
) -> Mission {
    Mission::new_with_id(
        mission_id,
        turn,
        owner,
        origin,
        home,
        Icon::Deploy,
        army,
        BombingRaid::None,
        false,
        false,
        Some(format!("{prior_logs}\n- ({turn}) Returning to planet {}.", home.name)),
    )
    .with_return_objective(return_objective)
}

/// Changes a controller-owned, planet-specific invitation for one foreign protection fleet.
/// Applies one protection invitation immediately. Stationed fleets remain until their owner
/// dispatches them or the next turn sends any remainder home.
///
/// Multiplayer backends call this through their narrow permission operation instead of storing
/// the choice in a complete turn submission. Both backends preserve the same stationed fleet.
pub fn set_protection_permission_immediately(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    protector: PlayerId,
    allowed: bool,
) -> Result<bool, GameError> {
    if model.players.iter().filter(|player| !player.spectator).count() < 3 {
        return invalid(player_id, "protection requires at least three active players");
    }
    if protector == player_id
        || model.player(protector).is_err()
        || model.player(protector).is_ok_and(|player| player.spectator)
    {
        return invalid(player_id, "protection permission references an invalid player");
    }
    let planet = model
        .map
        .try_get(planet_id)
        .ok_or_else(|| invalid_error(player_id, "protection planet does not exist"))?;
    if planet.is_destroyed || planet.controlled != Some(player_id) {
        return invalid(player_id, "only the current controller can change protection access");
    }
    let changed = planet.protection_permissions.contains(&protector) != allowed;
    let home_planet = model.player(protector)?.home_planet;

    if !changed {
        return Ok(false);
    }

    if allowed {
        model.player_mut(protector)?.protection_intel.insert(planet_id, player_id);
        let planet = model.map.get_mut(planet_id);
        planet.protection_permissions.insert(protector);
        return Ok(true);
    }
    model.map.get_mut(planet_id).protection_permissions.remove(&protector);

    let turn = command_turn(model, player_id)?;
    let (map, missions) = (&model.map, &mut model.missions);
    for mission in missions.iter_mut().filter(|mission| {
        mission.owner == protector
            && mission.destination == planet_id
            && mission.objective == Icon::Protect
    }) {
        check_mission(mission, map, turn, 0, Some(home_planet));
    }

    Ok(true)
}

/// Cancels a just-launched mission or reverses an older one without charging resources.
fn apply_recall(
    model: &mut GameModel,
    player_id: PlayerId,
    mission_id: u64,
) -> Result<(), GameError> {
    model.player(player_id)?;
    let turn = command_turn(model, player_id)?;
    let mission_index = model
        .missions
        .iter()
        .position(|mission| mission.id == mission_id)
        .ok_or_else(|| invalid_error(player_id, "mission does not exist"))?;
    let mission = &model.missions[mission_index];
    if mission.owner != player_id {
        return invalid(player_id, "mission belongs to another player");
    }
    if mission.is_returning() {
        return invalid(player_id, "mission is already returning");
    }
    if mission.joint_attack.is_some() {
        return invalid(player_id, "allied attacks cannot be recalled once launched");
    }
    if !mission.objective.is_recallable() {
        return invalid(player_id, "missile strikes cannot be recalled once launched");
    }
    if mission.recall_blocked_by_revoked_protection(&model.map, player_id) {
        return invalid(player_id, "a fleet cannot return to a world after protection was revoked");
    }

    if mission.send == turn && mission.travel_turns == 0 {
        let mission = model.missions.remove(mission_index);
        let fuel = mission.dispatch_fuel_consumption(&model.map, model.player(player_id)?);
        let player = model.player_mut(player_id)?;
        player.resources.deuterium = player.resources.deuterium.saturating_add(fuel);

        let origin = model.map.get_mut(mission.origin);
        if mission.jump_gate {
            origin.jump_gate = origin.jump_gate.saturating_sub(mission.jump_cost());
        }
        origin.owned = mission.origin_owned;
        origin.controlled = mission.origin_controlled;
        if mission.origin_owned == Some(player_id) || mission.origin_controlled == Some(player_id) {
            origin.dock(mission.army);
        } else {
            origin.dock_protecting_fleet(player_id, mission.army);
        }
        return Ok(());
    }

    model.missions[mission_index].recall(&model.map, turn);
    Ok(())
}

/// Keeps testing shortcuts in the same ordered draft as the orders that depend on them.
fn apply_testing_boost(model: &mut GameModel, player_id: PlayerId) -> Result<(), GameError> {
    let home_planet = model.player(player_id)?.home_planet;
    let senate_level_limit =
        Player::senate_level_limit(&model.map, model.rules.colonizable_percent);
    model.player_mut(player_id)?.resources += 1_000usize;
    for planet in model.map.planets.iter_mut().filter(|planet| {
        !planet.is_destroyed
            && (planet.owned == Some(player_id)
                || (planet.is_moon() && planet.controlled == Some(player_id)))
    }) {
        // Controlled moons receive buildings only; their fleets are not part of the planet boost.
        // Directly setting every lunar building intentionally bypasses the moon's field capacity.
        let unit_groups = if planet.is_moon() {
            vec![Unit::lunar_buildings()]
        } else {
            Unit::all_valid(false)
        };
        for unit in unit_groups.into_iter().flatten() {
            if unit == Unit::Building(Building::Senate) && planet.id != home_planet {
                continue;
            }
            if unit == Unit::Building(Building::ColonialAdministration)
                && model.players.iter().any(|player| player.home_planet == planet.id)
            {
                continue;
            }
            if let Unit::Building(building) = unit {
                planet.record_surface_building(building);
            }
            let amount = planet.army.entry(unit).or_default();
            match unit {
                Unit::Building(Building::Senate) => *amount = senate_level_limit,
                Unit::Defense(crate::core::units::defense::Defense::SpaceDock) => *amount = 1,
                Unit::Building(_) => *amount = Building::MAX_LEVEL,
                Unit::Ship(_) | Unit::Defense(_) => *amount = amount.saturating_add(3),
                Unit::Fauna(_) => {},
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
    let senate_level_limit =
        Player::senate_level_limit(&model.map, model.rules.colonizable_percent);
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

    let limit = purchase_limit(
        &model.players[player_index],
        &model.map.planets[planet_index],
        unit,
        senate_level_limit,
        model.players[player_index].senate_support(&model.map),
    )
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
    let gain = conversion_output(amount, laboratory);
    *player.resources.get_mut(&from) -= amount;
    *player.resources.get_mut(&to) = player.resources.get(&to).saturating_add(gain);
    Ok(())
}

/// Validates ownership and a completed structure before changing its operation.
fn operating_planet(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    unit: Unit,
) -> Result<&mut Planet, GameError> {
    model.player(player_id)?;
    let planet = model
        .map
        .planets
        .iter_mut()
        .find(|planet| planet.id == planet_id)
        .ok_or_else(|| invalid_error(player_id, "operating planet does not exist"))?;
    if planet.is_destroyed
        || planet.is_moon()
        || planet.owned != Some(player_id)
        || !planet.has(&unit)
    {
        return Err(invalid_error(
            player_id,
            "an owned planet with the completed structure is required",
        ));
    }
    Ok(planet)
}

fn apply_mine_mode(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    resource: ResourceName,
    mode: MineMode,
) -> Result<(), GameError> {
    let planet =
        operating_planet(model, player_id, planet_id, Unit::Building(mine_building(resource)))?;
    let operation = planet.operations.mine_mut(resource);
    if operation.recovering && mode != MineMode::Suspended {
        return invalid(player_id, "intensive extraction requires a full suspended recovery turn");
    }
    operation.mode = mode;
    Ok(())
}

fn apply_recycler_focus(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    resource: Option<ResourceName>,
) -> Result<(), GameError> {
    let planet = operating_planet(model, player_id, planet_id, Unit::Building(Building::Recycler))?;
    planet.operations.recycler_focus = resource;
    Ok(())
}

fn apply_space_dock_mode(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    mode: SpaceDockMode,
) -> Result<(), GameError> {
    let turn = model.turn;
    let senate = model.player(player_id)?.senate_support(&model.map);
    let planet = operating_planet(model, player_id, planet_id, Unit::space_dock())?;
    if planet.operations.space_dock == mode {
        return Ok(());
    }
    if turn < planet.operations.space_dock_locked_until {
        return invalid(player_id, "Space Dock specialization is committed for three turns");
    }
    if planet.fleet_production()
        > planet
            .max_fleet_production_in_mode(mode)
            .saturating_add(senate.bonus(planet, SenatePolicy::Expansion))
    {
        return invalid(player_id, "queued ships require the Space Dock's Industrial production");
    }
    turn.checked_add(1 + SpaceDockMode::COMMITMENT_TURNS)
        .ok_or_else(|| invalid_error(player_id, "Space Dock commitment exceeds the turn limit"))?;
    planet.operations.space_dock = mode;
    planet.operations.space_dock_selection_pending = true;
    Ok(())
}

/// Validates every owned world's queue before switching an empire-wide production bonus.
fn apply_senate_policy(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    policy: SenatePolicy,
) -> Result<(), GameError> {
    let player = model.player(player_id)?;
    if player.home_planet != planet_id {
        return invalid(player_id, "Senate policy can only be changed on the home planet");
    }
    let mut support = player.senate_support(&model.map);
    support.policy = policy;
    let turn = model.turn;
    let planet = operating_planet(model, player_id, planet_id, Unit::Building(Building::Senate))?;
    if planet.operations.senate == policy {
        return Ok(());
    }
    if turn < planet.operations.senate_locked_until {
        return invalid(player_id, "Senate policy is committed for three turns");
    }
    if !support.supports_queues(&model.map) {
        return invalid(player_id, "queued units require the current Senate production bonus");
    }
    turn.checked_add(1 + SenatePolicy::COMMITMENT_TURNS)
        .ok_or_else(|| invalid_error(player_id, "Senate commitment exceeds the turn limit"))?;
    let planet = model.map.get_mut(planet_id);
    planet.operations.senate = policy;
    planet.operations.senate_selection_pending = true;
    Ok(())
}

/// Applies a Terraformer specialization after validating its owner and infrastructure.
fn apply_terraformer_focus(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    resource: Option<ResourceName>,
) -> Result<(), GameError> {
    model.player(player_id)?;
    let planet = model
        .map
        .planets
        .iter_mut()
        .find(|planet| planet.id == planet_id)
        .ok_or_else(|| invalid_error(player_id, "terraformer planet does not exist"))?;
    if planet.is_destroyed
        || planet.owned != Some(player_id)
        || !(planet.has(&Unit::Building(Building::Terraformer))
            || planet.buy.contains(&Unit::Building(Building::Terraformer)))
    {
        return invalid(player_id, "an owned planet with a Terraformer is required");
    }
    planet.terraformer_focus = resource;
    Ok(())
}

/// Applies a Command Relay switch after validating its owner and infrastructure.
fn apply_command_relay(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    active: bool,
) -> Result<(), GameError> {
    model.player(player_id)?;
    let planet = model
        .map
        .planets
        .iter_mut()
        .find(|planet| planet.id == planet_id)
        .ok_or_else(|| invalid_error(player_id, "command-relay planet does not exist"))?;
    if planet.is_destroyed
        || planet.owned != Some(player_id)
        || !(planet.has(&Unit::Building(Building::CommandRelay))
            || planet.buy.contains(&Unit::Building(Building::CommandRelay)))
    {
        return invalid(player_id, "an owned planet with a Command Relay is required");
    }
    planet.command_relay_active = active;
    Ok(())
}

/// Arms or cancels a shield overload while enforcing its mandatory cooldown turn.
fn apply_planetary_shield_overload(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    active: bool,
) -> Result<(), GameError> {
    model.player(player_id)?;
    let planet = model
        .map
        .planets
        .iter_mut()
        .find(|planet| planet.id == planet_id)
        .ok_or_else(|| invalid_error(player_id, "planetary-shield planet does not exist"))?;
    if planet.is_destroyed
        || planet.owned != Some(player_id)
        || !planet.has(&Unit::planetary_shield())
    {
        return invalid(player_id, "an owned planet with a completed Planetary Shield is required");
    }
    planet.shield_overload = match (planet.shield_overload, active) {
        (ShieldOverloadState::Ready, true) => ShieldOverloadState::Overloaded,
        (ShieldOverloadState::Overloaded, false) => ShieldOverloadState::Ready,
        (ShieldOverloadState::Cooldown, _) => {
            return invalid(player_id, "the Planetary Shield is cooling down this turn");
        },
        _ => return invalid(player_id, "the Planetary Shield overload is already set"),
    };
    Ok(())
}

/// Returns whether one completed Railgun level can reach a valid non-allied world.
pub fn orbital_railgun_can_target(
    origin: &Planet,
    target: &Planet,
    player_id: PlayerId,
    level: usize,
) -> bool {
    level > 0
        && origin.id != target.id
        && !origin.is_destroyed
        && !target.is_destroyed
        && !origin.is_moon()
        && origin.owned == Some(player_id)
        && target.owned != Some(player_id)
        && target.controlled != Some(player_id)
        && origin.position.distance(target.position)
            <= ORBITAL_RAILGUN_RANGE_PER_LEVEL
                * level.min(Building::MAX_LEVEL) as f32
                * Planet::SIZE
}

/// Returns every owned Railgun world whose completed level can reach the target.
pub fn orbital_railgun_origins(map: &Map, player_id: PlayerId, target: PlanetId) -> Vec<PlanetId> {
    let Some(target) = map.try_get(target) else {
        return Vec::new();
    };
    map.planets
        .iter()
        .filter_map(|origin| {
            let level = origin
                .army
                .amount(&Unit::Building(Building::OrbitalRailgun))
                .min(Building::MAX_LEVEL);
            orbital_railgun_can_target(origin, target, player_id, level).then_some(origin.id)
        })
        .collect()
}

/// Returns the resource cost for every Railgun participating in one synchronized strike.
pub fn orbital_railgun_fire_cost(firing_railguns: usize) -> Resources {
    Resources::new(0, 0, ORBITAL_RAILGUN_FIRE_DEUTERIUM_COST.saturating_mul(firing_railguns))
}

/// Returns the temporary grid demand for every Railgun participating in one strike.
pub fn orbital_railgun_fire_energy_cost(firing_railguns: usize) -> usize {
    ORBITAL_RAILGUN_FIRE_ENERGY_COST.saturating_mul(firing_railguns)
}

/// Returns the combined destruction chance after target size and Planetary Shield modifiers.
///
/// Every firing Railgun level contributes five percentage points. Planet size modifies the total
/// by minus two points for the largest worlds through plus two for the smallest worlds. Each
/// Planetary Shield level removes one point, or two points while overloaded.
pub fn orbital_railgun_destruction_basis_points(
    map: &Map,
    origins: &[PlanetId],
    target: PlanetId,
) -> u16 {
    let firing_levels = origins.iter().fold(0usize, |total, origin| {
        total.saturating_add(
            map.try_get(*origin)
                .map(|planet| {
                    planet
                        .army
                        .amount(&Unit::Building(Building::OrbitalRailgun))
                        .min(Building::MAX_LEVEL)
                })
                .unwrap_or_default(),
        )
    });
    let Some(target) = map.try_get(target) else {
        return 0;
    };
    if firing_levels == 0 {
        return 0;
    }

    let size_modifier = target.death_ray_size_modifier_basis_points();
    let shield_levels =
        target.army.amount(&Unit::Building(Building::PlanetaryShield)).min(Building::MAX_LEVEL);
    let shield_reduction_per_level = if target.shield_overload.is_overloaded() {
        ORBITAL_RAILGUN_OVERLOADED_SHIELD_REDUCTION_BASIS_POINTS_PER_LEVEL
    } else {
        ORBITAL_RAILGUN_SHIELD_REDUCTION_BASIS_POINTS_PER_LEVEL
    };
    let base_chance =
        firing_levels.saturating_mul(ORBITAL_RAILGUN_DESTRUCTION_BASIS_POINTS_PER_LEVEL);
    let chance = if size_modifier < 0 {
        base_chance.saturating_sub(usize::from(size_modifier.unsigned_abs()))
    } else {
        base_chance.saturating_add(size_modifier as usize)
    };
    let chance =
        chance.saturating_sub(shield_levels.saturating_mul(shield_reduction_per_level)).min(10_000);
    u16::try_from(chance).unwrap_or(10_000)
}

/// Validates and pays for every in-range owned Railgun, then records one synchronized event.
fn apply_orbital_railgun_fire(
    model: &mut GameModel,
    player_id: PlayerId,
    target: PlanetId,
) -> Result<(), GameError> {
    if model.map.try_get(target).is_none() {
        return invalid(player_id, "orbital-railgun target does not exist");
    }
    if model.map.get(target).blocks_hostile_action_by(player_id) {
        return invalid(player_id, "a player cannot fire on a world they are protecting");
    }
    let origins = orbital_railgun_origins(&model.map, player_id, target);
    if origins.is_empty() {
        return invalid(player_id, "no owned Orbital Railgun can reach this target");
    }
    let already_fired = model
        .orbital_strikes
        .iter()
        .flat_map(|strike| &strike.origins)
        .any(|fired| model.map.get(*fired).owned == Some(player_id));
    if already_fired {
        return invalid(player_id, "Orbital Railguns can fire only once per turn");
    }
    let cost = orbital_railgun_fire_cost(origins.len());
    let player = model.player_mut(player_id)?;
    if player.resources.deuterium < cost.deuterium {
        return invalid(player_id, "not enough deuterium to fire the Orbital Railguns");
    }
    player.resources -= cost;
    model.orbital_strikes.push(OrbitalStrike {
        turn: model.turn.saturating_add(1),
        origins,
        target,
        chance_basis_points: 0,
        destroyed: false,
    });
    model.map.get_mut(target).protection_permissions.remove(&player_id);
    Ok(())
}

/// Rolls every target group from the same pre-impact snapshot, then applies destruction together.
fn resolve_orbital_railgun_strikes<R: Rng + ?Sized>(model: &mut GameModel, rng: &mut R) {
    let mut groups = BTreeMap::<PlanetId, Vec<PlanetId>>::new();
    for shot in std::mem::take(&mut model.orbital_strikes) {
        groups.entry(shot.target).or_default().extend(shot.origins);
    }

    let mut outcomes = Vec::with_capacity(groups.len());
    for (target, mut origins) in groups {
        origins.sort_unstable();
        origins.dedup();
        let chance_basis_points =
            orbital_railgun_destruction_basis_points(&model.map, &origins, target);
        let destroyed = rng.random_range(0_u16..10_000) < chance_basis_points;
        outcomes.push(OrbitalStrike {
            turn: model.turn,
            origins,
            target,
            chance_basis_points,
            destroyed,
        });
    }
    for outcome in &outcomes {
        if outcome.destroyed {
            model.map.get_mut(outcome.target).destroy();
        }
    }
    model.orbital_strikes = outcomes;
}

/// Validates the colony and its projected completed level before changing the standing order.
fn apply_fleet_withdrawal(
    model: &mut GameModel,
    player_id: PlayerId,
    planet_id: PlanetId,
    withdrawal: FleetWithdrawal,
) -> Result<(), GameError> {
    let player = model.player(player_id)?;
    if player.spectator || player.home_planet == planet_id {
        return invalid(player_id, "fleet withdrawal requires an owned non-home colony");
    }
    let planet = model
        .map
        .try_get_mut(planet_id)
        .ok_or_else(|| invalid_error(player_id, "withdrawal planet does not exist"))?;
    let administration = Unit::Building(Building::ColonialAdministration);
    let level =
        planet.army.amount(&administration) + usize::from(planet.buy.contains(&administration));
    if planet.is_destroyed
        || planet.is_moon()
        || planet.owned != Some(player_id)
        || level == 0
        || level < withdrawal.minimum_level()
    {
        return invalid(
            player_id,
            "this withdrawal setting requires a higher Colonial Administration level",
        );
    }
    planet.fleet_withdrawal = withdrawal;
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
    let max_owned = colony_limit(model, player_id)?;
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
        || planet.army.controller().amount(&Unit::colony_ship()) == 0
    {
        return invalid(player_id, "planet cannot currently be colonized");
    }
    if let Some(count) = planet.army.get_mut(&Unit::colony_ship()) {
        *count = count.saturating_sub(1);
    }
    planet.colonize(player_id);
    if let Some(player) = model.players.iter_mut().find(|player| player.id == player_id) {
        player.record_world_acquisition(planet_id);
    }
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
    deep_cover: bool,
    jump_gate: bool,
) -> Result<(), GameError> {
    ensure_mission_capacity(model, player_id, 1)?;
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
        .ok_or_else(|| invalid_error(player_id, "mission destination does not exist"))?;
    let origin = &model.map.planets[origin_index];
    let turn = command_turn(model, player_id)?;
    let mission = Mission::new_with_id(
        mission_id,
        turn,
        player_id,
        origin,
        destination,
        objective,
        army,
        bombing,
        combat_probes,
        jump_gate,
        None,
    )
    .with_deep_cover(deep_cover);
    validate_mission(model.player(player_id)?, &model.map, origin, destination, &mission)
        .map_err(|error| invalid_error(player_id, error.to_string()))?;
    let fuel = mission.dispatch_fuel_consumption(&model.map, model.player(player_id)?);
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
    let source_army = origin
        .mission_origin_army_mut(player_id)
        .ok_or_else(|| invalid_error(player_id, "mission origin is no longer available"))?;
    for (unit, count) in &mission.army {
        if let Some(available) = source_army.get_mut(unit) {
            *available = available.saturating_sub(*count);
        }
    }
    source_army.retain(|_, count| *count > 0);
    origin.army.retain_protectors(|_, army| army.has_army());
    origin.release_control_if_vacant();
    if mission.objective.is_hostile_action() {
        model.map.get_mut(destination_id).protection_permissions.remove(&player_id);
    }
    model.missions.push(mission);
    Ok(())
}

/// Validates and launches every accepted contingent of one coordinated hostile mission.
#[allow(clippy::too_many_arguments)]
fn apply_joint_mission(
    model: &mut GameModel,
    leader: PlayerId,
    attack_id: u64,
    mission_id: u64,
    destination_id: PlanetId,
    objective: Icon,
    bombing: BombingRaid,
    combat_probes: bool,
    contributions: &[JointAttackContribution],
) -> Result<(), GameError> {
    if !model.joint_attacks_enabled() {
        return invalid(leader, "joint attacks require at least three active players");
    }
    if attack_id == 0
        || mission_id == 0
        || !matches!(objective, Icon::Colonize | Icon::Attack | Icon::Destroy)
        || contributions.is_empty()
        || contributions.first().is_none_or(|entry| entry.player_id != leader)
    {
        return invalid(leader, "joint attack identity, leader, or objective is invalid");
    }
    ensure_mission_capacity(model, leader, contributions.len())?;
    let destination = model
        .map
        .try_get(destination_id)
        .ok_or_else(|| invalid_error(leader, "joint attack destination does not exist"))?
        .clone();
    let mut seen = BTreeSet::new();
    let mut prepared = Vec::with_capacity(contributions.len());
    let turn = command_turn(model, leader)?;
    let mut arrival_turn = turn;

    for (index, contribution) in contributions.iter().enumerate() {
        let contingent_id = mission_id
            .checked_add(index as u64)
            .ok_or_else(|| invalid_error(leader, "joint attack mission identifier is exhausted"))?;
        if model.missions.iter().any(|mission| mission.id == contingent_id)
            || !seen.insert(contribution.player_id)
            || !contribution.army.has_army()
        {
            return invalid(leader, "joint attack contains duplicate identities or an empty fleet");
        }
        let participant = model.player(contribution.player_id)?;
        if participant.spectator
            || destination.owned == Some(contribution.player_id)
            || destination.controlled == Some(contribution.player_id)
            || destination.blocks_hostile_action_by(contribution.player_id)
        {
            return invalid(
                leader,
                "a participant cannot jointly attack their own or protected world",
            );
        }
        let origin = model
            .map
            .try_get(contribution.origin)
            .ok_or_else(|| invalid_error(leader, "joint attack origin does not exist"))?;
        if origin.id == destination_id || !origin.can_launch_mission(contribution.player_id) {
            return invalid(leader, "joint attack origin is not available to its participant");
        }
        let available = origin.mission_origin_army(contribution.player_id).ok_or_else(|| {
            invalid_error(leader, "joint attack origin is not available to its participant")
        })?;
        if contribution.army.iter().any(|(unit, count)| *count == 0 || !unit.is_ship()) {
            return invalid(leader, "joint attack contributions must contain positive ship counts");
        }
        if contribution.bombing != bombing {
            return invalid(leader, "all allied fleets must use the leader's bombing objective");
        }
        if let Some((unit, count)) =
            contribution.army.iter().find(|(unit, count)| available.amount(unit) < **count)
        {
            let unit_name = unit.to_name();
            return invalid(
                leader,
                format!(
                    "allied attack: player {} selected {} {}{}, but the origin has only {} available",
                    contribution.player_id,
                    count,
                    unit_name,
                    if *count == 1 { "" } else { "s" },
                    available.amount(unit),
                ),
            );
        }
        if index == 0
            && (!objective.condition_for_army(&contribution.army)
                || contribution.combat_probes != combat_probes)
        {
            return invalid(leader, "the inviter's fleet does not meet the selected objective");
        }
        let contingent = Mission::new_with_id(
            contingent_id,
            turn,
            contribution.player_id,
            origin,
            &destination,
            objective,
            contribution.army.clone(),
            bombing.clone(),
            contribution.combat_probes,
            false,
            Some(format!("- ({turn}) Joined coordinated mission to {}.", destination.name)),
        );
        arrival_turn = arrival_turn.max(turn.saturating_add(contingent.duration(&model.map)));
        prepared.push(contingent);
    }

    for contingent in &prepared {
        if model.player(contingent.owner)?.resources.deuterium
            < contingent.fuel_consumption(&model.map)
        {
            return invalid(leader, "a joint attack participant lacks the required deuterium");
        }
    }

    let attackers = prepared
        .iter()
        .map(|contingent| (contingent.owner, contingent.army.clone()))
        .collect::<BTreeMap<_, _>>();
    let origins = prepared
        .iter()
        .map(|contingent| (contingent.owner, contingent.origin))
        .collect::<BTreeMap<_, _>>();
    for mut contingent in prepared {
        let fuel = contingent.fuel_consumption(&model.map);
        model.player_mut(contingent.owner)?.resources.deuterium -= fuel;
        let source = model
            .map
            .get_mut(contingent.origin)
            .mission_origin_army_mut(contingent.owner)
            .ok_or_else(|| invalid_error(leader, "joint attack origin is no longer available"))?;
        for (unit, count) in &contingent.army {
            if let Some(available) = source.get_mut(unit) {
                *available = available.saturating_sub(*count);
            }
        }
        source.retain(|_, count| *count > 0);
        model.map.get_mut(contingent.origin).army.retain_protectors(|_, army| army.has_army());
        contingent.joint_attack = Some(JointAttackMission {
            id: attack_id,
            leader,
            arrival_turn,
            attackers: attackers.clone(),
            survivors: BTreeMap::new(),
            scouts: BTreeMap::new(),
            origins: origins.clone(),
            combat_orders: contributions
                .iter()
                .map(|item| {
                    (
                        item.player_id,
                        FleetCombatOrders {
                            bombing: item.bombing.clone(),
                            combat_probes: item.combat_probes,
                        },
                    )
                })
                .collect(),
        });
        model.map.get_mut(contingent.origin).release_control_if_vacant();
        model.missions.push(contingent);
    }
    let destination = model.map.get_mut(destination_id);
    for attacker in seen {
        destination.protection_permissions.remove(&attacker);
    }
    Ok(())
}

fn resolution_energy_grid(
    player: &Player,
    map: &Map,
    action_energy_demand: &BTreeMap<PlayerId, usize>,
) -> EnergyGrid {
    let grid = player.energy_grid(map);
    grid.with_action_demand(action_energy_demand.get(&player.id).copied().unwrap_or_default())
}

const INDEPENDENT_POPULATION_EMPTY_PERCENT: usize = 20;

/// Resolves one planet's first-contact roll. The result is stored on the planet before combat, so
/// every later observer sees the same inhabitants and combat casualties remain authoritative.
fn reveal_independent_population<R: Rng + ?Sized>(
    turn: usize,
    objective: Icon,
    planet: &mut Planet,
    rng: &mut R,
) {
    if planet.independent_population != IndependentPopulation::Unrevealed
        || !matches!(objective, Icon::Spy | Icon::Colonize | Icon::Attack)
    {
        return;
    }

    if rng.random_range(0..100) < INDEPENDENT_POPULATION_EMPTY_PERCENT {
        planet.independent_population = IndependentPopulation::Empty;
        return;
    }

    planet.independent_population = IndependentPopulation::Inhabited;
    planet.record_surface_building(Building::MetalMine);
    planet.army = independent_population_garrison(turn, rng).into();
}

/// Generates bounded neutral formations. Five-turn tiers improve the mix until turn 21, while
/// hard caps keep the strongest possible units at Cruiser and Gauss Cannon.
fn independent_population_garrison<R: Rng + ?Sized>(turn: usize, rng: &mut R) -> Army {
    let tier = turn.saturating_sub(1).div_euclid(5).min(4);
    let mut army = Army::from([
        (Unit::Building(Building::MetalMine), 1),
        (Unit::Building(Building::CrystalMine), 1),
        (Unit::Building(Building::DeuteriumSynthesizer), 1),
        (Unit::Building(Building::Reactor), 1),
    ]);
    let formation = rng.random_range(0..6);
    let mut add = |unit: Unit, base: usize, per_tier: usize, spread: usize| {
        let count = base
            .saturating_add(tier.saturating_mul(per_tier))
            .saturating_add(rng.random_range(0..=spread));
        army.insert(unit, count);
    };

    match formation {
        0 => {
            add(Unit::Ship(Ship::LightFighter), 3, 2, 3);
            add(Unit::Defense(Defense::RocketLauncher), 4, 2, 4);
            add(Unit::Defense(Defense::Crawler), 1, 1, 2);
        },
        1 => {
            add(Unit::Ship(Ship::LightFighter), 2, 1, 2);
            add(Unit::Ship(Ship::HeavyFighter), 1, 1, 1);
            add(Unit::Defense(Defense::RocketLauncher), 2, 1, 2);
            add(Unit::Defense(Defense::LightLaser), 2, 1, 2);
        },
        2 => {
            add(Unit::Ship(Ship::LightFighter), 2, 1, 2);
            if tier == 0 {
                add(Unit::Ship(Ship::HeavyFighter), 1, 0, 1);
            } else {
                add(Unit::Ship(Ship::Destroyer), 1, 0, usize::from(tier >= 3));
            }
            add(Unit::Defense(Defense::RocketLauncher), 3, 1, 3);
            if tier > 0 {
                add(Unit::Defense(Defense::HeavyLaser), 1, 0, 1);
            }
        },
        3 => {
            add(Unit::Ship(Ship::LightFighter), 1, 1, 2);
            add(Unit::Defense(Defense::RocketLauncher), 5, 2, 3);
            add(Unit::Defense(Defense::LightLaser), 2, 1, 2);
            if tier > 0 {
                add(Unit::Defense(Defense::HeavyLaser), 1, 1, 1);
            }
            if tier >= 2 {
                add(Unit::Defense(Defense::GaussCannon), 1, 0, usize::from(tier >= 4));
            }
        },
        4 => {
            if tier >= 2 {
                add(Unit::Ship(Ship::Cruiser), 1, 0, usize::from(tier >= 4));
            } else if tier == 1 {
                add(Unit::Ship(Ship::Destroyer), 1, 0, 1);
            } else {
                add(Unit::Ship(Ship::HeavyFighter), 1, 0, 1);
            }
            add(Unit::Ship(Ship::LightFighter), 2, 1, 2);
            add(Unit::Defense(Defense::RocketLauncher), 2, 1, 2);
            if tier >= 2 {
                add(Unit::Defense(Defense::GaussCannon), 1, 0, 1);
            } else {
                add(Unit::Defense(Defense::LightLaser), 1, 1, 1);
            }
        },
        _ => {
            add(Unit::Ship(Ship::LightFighter), 2, 1, 2);
            add(Unit::Ship(Ship::HeavyFighter), 1, 1, 1);
            if tier >= 1 {
                add(Unit::Ship(Ship::Destroyer), 1, 0, usize::from(tier >= 4));
            }
            if tier >= 3 {
                add(Unit::Ship(Ship::Cruiser), 1, 0, 0);
            }
            add(Unit::Defense(Defense::RocketLauncher), 3, 1, 2);
            add(Unit::Defense(Defense::LightLaser), 1, 1, 1);
            if tier >= 2 {
                add(Unit::Defense(Defense::GaussCannon), 1, 0, 0);
            }
            add(Unit::Defense(Defense::RepairTruck), 1, 0, usize::from(tier >= 3));
        },
    }
    army
}

/// Advances production, missions, combat, reports, and victory state by one turn.
fn advance_simulation(model: &mut GameModel) -> Result<(), GameError> {
    let recycling_turn = simulation_turn(model.turn)?;
    model.turn = model
        .turn
        .checked_add(1)
        .ok_or_else(|| GameError::MalformedState("turn counter is exhausted".to_string()))?;
    let turn = simulation_turn(model.turn)?;
    let mut rng = model.rng.next_rng();

    // Firing can deepen an energy shortage. Capture the owners before simultaneous impacts can
    // destroy an origin, then apply the one-turn demand to production and combat power below.
    let action_energy_demand = model.orbital_strikes.iter().fold(
        BTreeMap::<PlayerId, usize>::new(),
        |mut demand, strike| {
            if let Some(player_id) = strike
                .origins
                .first()
                .and_then(|origin| model.map.try_get(*origin))
                .and_then(|planet| planet.owned)
            {
                let strike_demand = orbital_railgun_fire_energy_cost(strike.origins.len());
                demand
                    .entry(player_id)
                    .and_modify(|total| *total = total.saturating_add(strike_demand))
                    .or_insert(strike_demand);
            }
            demand
        },
    );
    let mut action_energy_demand = action_energy_demand;
    for player in &model.players {
        let demand = action_energy_demand.entry(player.id).or_default();
        *demand = demand.saturating_add(crate::core::energy::jump_gate_energy_demand(
            &model.missions,
            player.id,
            recycling_turn,
        ));
    }

    // Resolve every paid shot from the same pre-impact state. A railgun destroyed by another
    // simultaneous strike therefore still contributes the shot committed during planning.
    resolve_orbital_railgun_strikes(model, &mut rng);

    for planet in &mut model.map.planets {
        planet.produce();
        planet.jump_gate = 0;
    }
    let recycler_output = model
        .players
        .iter()
        .map(|player| {
            (
                player.id,
                recycler_production(
                    &model.map,
                    &model.players,
                    player.id,
                    recycling_turn,
                    &mut rng,
                ),
            )
        })
        .collect::<BTreeMap<_, _>>();
    for player in &mut model.players {
        let energy = resolution_energy_grid(player, &model.map, &action_energy_demand);
        let powered_production = energy.scale_resources(player.raw_resource_production(&model.map));
        let salvage = recycler_output.get(&player.id).copied().unwrap_or_default();
        // Recycler craft use their own salvage systems rather than the planetary power grid.
        // A brownout therefore reduces mine output without reducing an already recovered haul.
        player.resources += powered_production + salvage;
    }
    settle_resource_hub_loans(model, model.turn.saturating_sub(1));

    // Lock grids before any missions resolve. Conquest and destruction affect the next turn,
    // never a later battle in the current randomized mission order.
    let energy_grids = model
        .players
        .iter()
        .map(|player| {
            (player.id, resolution_energy_grid(player, &model.map, &action_energy_demand))
        })
        .collect::<std::collections::HashMap<_, _>>();

    let mut player_order = model.players.iter().map(|player| player.id).collect::<Vec<_>>();
    player_order.shuffle(&mut rng);
    let planet_ids = model.map.planets.iter().map(|planet| planet.id).collect::<Vec<_>>();
    let mut new_missions = Vec::new();
    let mut fleeing_missions = Vec::new();
    let mut used_mission_ids = model.missions.iter().map(|mission| mission.id).collect();

    check_missions(model, turn)?;
    resolve_space_fauna_encounters(model, turn, &mut rng);
    recall_unpermitted_protecting_fleets(
        model,
        turn,
        &mut rng,
        &mut used_mission_ids,
        &mut new_missions,
    )?;
    resolve_protect_arrivals(model, turn, &mut rng);
    consolidate_joint_attack_arrivals(model);

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
                    let retreat_destination = model
                        .map
                        .get(mission.destination)
                        .controlled
                        .and_then(|owner| model.players.iter().find(|player| player.id == owner))
                        .filter(|player| {
                            !player.spectator && player.home_planet != mission.destination
                        })
                        .map(|player| (player.id, model.map.get(player.home_planet)))
                        .filter(|(owner, home)| !home.is_destroyed && home.owned == Some(*owner))
                        .map(|(owner, home)| (owner, home.clone()));
                    let joint_origins = mission
                        .joint_attack
                        .as_ref()
                        .map(|attack| {
                            attack
                                .origins
                                .iter()
                                .map(|(owner, origin)| (*owner, model.map.get(*origin).clone()))
                                .collect::<BTreeMap<_, _>>()
                        })
                        .unwrap_or_default();
                    // Use the actual departure world, not the fallback return destination.
                    // Completed levels at arrival govern cover even if relay deception is off.
                    let origin_relay = model
                        .map
                        .get(mission.origin)
                        .army
                        .amount(&Unit::Building(Building::CommandRelay));
                    let destination = model.map.get_mut(mission.destination);
                    reveal_independent_population(turn, mission.objective, destination, &mut rng);
                    let deep_cover_succeeds = mission.objective == Icon::Spy
                        && mission.deep_cover
                        && origin_relay
                            > destination.army.amount(&Unit::Building(Building::CommandRelay));
                    let relay_diverts_spy = mission.objective == Icon::Spy
                        && !mission.deep_cover
                        && destination.command_relay_diverts(mission.army.amount(&Unit::probe()));
                    let energy = destination
                        .controlled
                        .or(destination.owned)
                        .and_then(|owner| energy_grids.get(&owner))
                        .copied()
                        .unwrap_or_default();
                    let mut report = if deep_cover_succeeds || relay_diverts_spy {
                        resolve_spy_without_combat(turn, &mission, destination, &mut rng)
                    } else {
                        resolve_combat_with_retreat_with_rng(
                            turn,
                            &mission,
                            destination,
                            energy,
                            retreat_destination.as_ref().map(|(_, home)| home.id),
                            &mut rng,
                        )
                    };
                    if let Some((owner, home)) = &retreat_destination {
                        if let Some(retreat) = report
                            .combat_report
                            .as_ref()
                            .and_then(|combat| combat.defender_retreat.as_ref())
                        {
                            fleeing_missions.push(Mission::new_with_id(
                                next_unique_mission_id(&mut rng, &mut used_mission_ids)?,
                                turn - 1,
                                *owner,
                                destination,
                                home,
                                Icon::Deploy,
                                retreat.ships.clone(),
                                BombingRaid::None,
                                false,
                                false,
                                Some(format!(
                                    "- ({turn}) Fleet withdrew from {} to {}.",
                                    destination.name, home.name
                                )),
                            ));
                        }
                    }
                    report.mission.logs.push_str(&format!(
                        "\n- ({turn}) Mission arrived in {}.",
                        destination.name
                    ));
                    if mission.deep_cover {
                        report.mission.logs.push_str(&format!(
                            "\n- ({turn}) {}",
                            if deep_cover_succeeds {
                                "Deep Cover succeeded; scanned undetected without combat."
                            } else {
                                "Deep Cover failed; normal Spy combat initiated."
                            }
                        ));
                    }

                    if report.scout_probes > 0 {
                        if let Some(attack) = report.mission.joint_attack.as_ref() {
                            if mission.objective != Icon::Destroy
                                || report.winner() != Some(mission.owner)
                            {
                                for (owner, count) in &attack.scouts {
                                    if let Some(origin) = joint_origins
                                        .get(owner)
                                        .filter(|origin| !origin.is_destroyed)
                                    {
                                        new_missions.push(objective_return_mission(
                                            next_unique_mission_id(
                                                &mut rng,
                                                &mut used_mission_ids,
                                            )?,
                                            turn,
                                            *owner,
                                            destination,
                                            origin,
                                            Army::from([(Unit::probe(), *count)]),
                                            &report.mission.logs,
                                            mission.objective,
                                        ));
                                    }
                                }
                            }
                        } else if mission.objective == Icon::Spy {
                            report.mission.logs.push_str(&format!(
                                "\n- ({turn}) Spied on planet {}.",
                                destination.name
                            ));
                            new_missions.push(objective_return_mission(
                                next_unique_mission_id(&mut rng, &mut used_mission_ids)?,
                                turn,
                                report.mission.owner,
                                destination,
                                &new_origin,
                                report.surviving_attacker.clone(),
                                &report.mission.logs,
                                Icon::Spy,
                            ));
                        } else if report.mission.objective != Icon::Destroy
                            || report.winner() != Some(mission.owner)
                        {
                            new_missions.push(objective_return_mission(
                                next_unique_mission_id(&mut rng, &mut used_mission_ids)?,
                                turn,
                                mission.owner,
                                destination,
                                &new_origin,
                                Army::from([(Unit::probe(), report.scout_probes)]),
                                &report.mission.logs,
                                mission.objective,
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
                        if let Some(joint_attack) = report.mission.joint_attack.as_ref() {
                            for (owner, army) in &joint_attack.survivors {
                                let army = joint_attack.fleet_without_scouts(*owner, army);
                                let Some(origin) = joint_origins.get(owner) else {
                                    continue;
                                };
                                if army.has_army() && !origin.is_destroyed {
                                    new_missions.push(
                                        Mission::new_with_id(
                                            next_unique_mission_id(
                                                &mut rng,
                                                &mut used_mission_ids,
                                            )?,
                                            turn,
                                            *owner,
                                            destination,
                                            origin,
                                            Icon::Deploy,
                                            army.clone(),
                                            BombingRaid::None,
                                            false,
                                            false,
                                            Some(format!(
                                                "{}\n- ({turn}) Joint combat draw; returning to {}.",
                                                report.mission.logs, origin.name
                                            )),
                                        )
                                        .with_return_objective(mission.objective),
                                    );
                                }
                            }
                        } else if retreat.has_army() {
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
                                    "{}\n- ({turn}) Combat draw; returning to {}.",
                                    report.mission.logs, new_origin.name
                                )),
                            );
                            new_missions
                                .push(return_mission.with_return_objective(mission.objective));
                        }
                    }

                    if report.winner() == Some(mission.owner) {
                        destination.army.clear_protectors();
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
                            if let Some(joint_attack) = report.mission.joint_attack.as_ref() {
                                for (owner, army) in &joint_attack.survivors {
                                    let Some(origin) = joint_origins.get(owner) else {
                                        continue;
                                    };
                                    if army.has_army() && !origin.is_destroyed {
                                        new_missions.push(objective_return_mission(
                                            next_unique_mission_id(
                                                &mut rng,
                                                &mut used_mission_ids,
                                            )?,
                                            turn,
                                            *owner,
                                            destination,
                                            origin,
                                            army.clone(),
                                            &report.mission.logs,
                                            Icon::Destroy,
                                        ));
                                    }
                                }
                            } else {
                                new_missions.push(objective_return_mission(
                                    next_unique_mission_id(&mut rng, &mut used_mission_ids)?,
                                    turn,
                                    report.mission.owner,
                                    destination,
                                    &new_origin,
                                    report.surviving_attacker.clone(),
                                    &report.mission.logs,
                                    Icon::Destroy,
                                ));
                            }
                        } else if report.planet_colonized {
                            if let Some(count) =
                                report.surviving_attacker.get_mut(&Unit::colony_ship())
                            {
                                *count = count.saturating_sub(1);
                            }
                            if let Some(attack) = report.mission.joint_attack.as_mut() {
                                if let Some(leader_army) = attack.survivors.get_mut(&mission.owner)
                                {
                                    if let Some(colony_ships) =
                                        leader_army.get_mut(&Unit::colony_ship())
                                    {
                                        *colony_ships = colony_ships.saturating_sub(1);
                                    }
                                    leader_army.retain(|_, count| *count > 0);
                                }
                                attack.survivors.retain(|_, army| army.has_army());
                            }
                            destination.colonize(mission.owner);
                            report.mission.logs.push_str(&format!(
                                "\n- ({turn}) Planet {} colonized.",
                                destination.name
                            ));
                        }

                        if !(mission.objective == Icon::Deploy
                            || (mission.objective == Icon::Colonize
                                && destination.controlled == Some(mission.owner)))
                        {
                            destination.army.retain(|unit, _| unit.is_building());
                        }
                        if mission.objective != Icon::Destroy {
                            destination.control(mission.owner);
                            if let Some(joint_attack) = report.mission.joint_attack.as_ref() {
                                for (owner, army) in &joint_attack.survivors {
                                    let stationed = joint_attack.fleet_without_scouts(*owner, army);
                                    if !stationed.has_army() {
                                        continue;
                                    }
                                    if *owner == mission.owner {
                                        destination.dock(stationed);
                                    } else {
                                        destination.protection_permissions.insert(*owner);
                                        destination.dock_protecting_fleet(*owner, stationed);
                                    }
                                }
                            } else {
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
                        }
                    } else if report.combat_report.is_some() {
                        destination.army = report.surviving_defender.clone();
                    }

                    if destination.has_independent_population() {
                        destination.army.retain(|_, count| *count > 0);
                    }
                    if destination.has_independent_population()
                        && !destination.army.iter().any(|(unit, count)| {
                            *count > 0 && !unit.is_building() && !unit.is_missile()
                        })
                    {
                        destination.independent_population = IndependentPopulation::Empty;
                    }

                    let defender_salvage = report.defender_salvage();
                    if defender_salvage != Resources::default() {
                        report.mission.logs.push_str(&format!(
                            "\n- ({turn}) Crawlers recovered {} metal, {} crystal, and {} deuterium.",
                            defender_salvage.metal,
                            defender_salvage.crystal,
                            defender_salvage.deuterium
                        ));
                    }

                    report.destination_owned = destination.owned;
                    report.destination_controlled = destination.controlled;
                    for player in &mut model.players {
                        if report.planet.controlled == Some(player.id) {
                            player.resources += defender_salvage;
                        }
                        if player.controls(destination) {
                            player.record_world_acquisition(destination.id);
                        }
                        if report.is_attacker(player.id)
                            || (!deep_cover_succeeds && report.is_defender(player.id))
                        {
                            let mut player_report = report.clone();
                            if relay_diverts_spy && report.mission.owner == player.id {
                                spoof_spy_report_as_empty(&mut player_report);
                            }
                            player.push_report(player_report);
                        }
                    }
                }

                let arrived_ids = arrived.iter().map(|mission| mission.id).collect::<HashSet<_>>();
                check_missions(model, turn)?;
                model.missions.retain(|mission| !arrived_ids.contains(&mission.id));
            }
        }
    }

    for mission in &mut model.missions {
        mission.advance(&model.map);
    }
    // Withdrawal launches one turn before this battle's snapshot. Give it that completed
    // movement step, so a pursuer launched after the battle starts a turn behind.
    for mut mission in fleeing_missions {
        let arrives = mission.turns_to_destination(&model.map) < 2;
        mission.advance(&model.map);
        let home = model.map.get_mut(mission.destination);
        if arrives && !home.is_destroyed && home.controlled == Some(mission.owner) {
            let report = resolve_combat_with_retreat_with_rng(
                turn,
                &mission,
                home,
                Default::default(),
                None,
                &mut rng,
            );
            home.dock(mission.army.clone());
            if let Some(player) = model.players.iter_mut().find(|player| player.id == mission.owner)
            {
                player.push_report(report);
            }
        } else {
            new_missions.push(mission);
        }
    }
    if new_missions.len() > MAX_ACTIVE_MISSIONS.saturating_sub(model.missions.len()) {
        return Err(GameError::MalformedState(format!(
            "mission resolution exceeds {MAX_ACTIVE_MISSIONS} active missions"
        )));
    }
    model.missions.extend(new_missions);

    // Return and withdrawal missions are staged outside `model.missions` while arrivals resolve.
    // Recheck the complete collection after the last possible destruction so none of those late
    // fleets spends a saved turn travelling toward a world that no longer exists.
    check_missions(model, turn)?;

    // An overload applies to every battle at the world during this resolution. Only after all
    // missions have resolved does it enter cooldown; that cooldown itself lasts through the next
    // complete planning and resolution turn.
    for planet in &mut model.map.planets {
        planet.shield_overload = if planet.has(&Unit::planetary_shield()) {
            planet.shield_overload.finish_turn()
        } else {
            ShieldOverloadState::Ready
        };
        for operation in &mut planet.operations.mines {
            operation.finish_turn();
        }
        if !planet.has(&Unit::space_dock()) {
            planet.operations.space_dock = SpaceDockMode::Industrial;
            planet.operations.space_dock_locked_until = 0;
            planet.operations.space_dock_selection_pending = false;
        } else if planet.operations.space_dock_selection_pending {
            // This resolution used the final draft mode. Lock the next three planning turns.
            planet.operations.space_dock_locked_until =
                model.turn.checked_add(SpaceDockMode::COMMITMENT_TURNS).ok_or_else(|| {
                    GameError::MalformedState("Space Dock commitment exceeds the turn limit".into())
                })?;
            planet.operations.space_dock_selection_pending = false;
        }
        if !planet.has(&Unit::Building(Building::Senate)) {
            planet.operations.senate = SenatePolicy::Expansion;
            planet.operations.senate_locked_until = 0;
            planet.operations.senate_selection_pending = false;
        } else if planet.operations.senate_selection_pending {
            planet.operations.senate_locked_until =
                model.turn.checked_add(SenatePolicy::COMMITMENT_TURNS).ok_or_else(|| {
                    GameError::MalformedState("Senate commitment exceeds the turn limit".into())
                })?;
            planet.operations.senate_selection_pending = false;
        }
    }

    let mut playing = Vec::new();
    for player in &mut model.players {
        player.spectator = !player.owns(model.map.get(player.home_planet));
        if !player.spectator {
            playing.push(player.id);
        }
    }
    let territory_won = model.territorial_winner().is_some();
    let solo_practice = model.rules.practice_mode && model.rules.player_count == 1;
    if !territory_won && (playing.len() > 1 || (solo_practice && playing.len() == 1)) {
        let eliminated = model
            .players
            .iter()
            .filter(|player| player.spectator)
            .map(|player| player.id)
            .collect::<HashSet<_>>();
        for planet in &mut model.map.planets {
            planet.protection_permissions.retain(|player| !eliminated.contains(player));
            planet.army.retain_protectors(|player, _| !eliminated.contains(&player));
            if planet.controlled.is_some_and(|id| eliminated.contains(&id)) {
                planet.clean();
            }
        }
        if playing.len() < 3 {
            for planet in &mut model.map.planets {
                planet.protection_permissions.clear();
            }
        }
        model.missions.retain(|mission| !eliminated.contains(&mission.owner));
        let mut dismissed = Vec::new();
        recall_unpermitted_protecting_fleets(
            model,
            turn,
            &mut rng,
            &mut used_mission_ids,
            &mut dismissed,
        )?;
        dismissed.retain(|mission| !eliminated.contains(&mission.owner));
        if dismissed.len() > MAX_ACTIVE_MISSIONS.saturating_sub(model.missions.len()) {
            return Err(GameError::MalformedState(format!(
                "protection recalls exceed {MAX_ACTIVE_MISSIONS} active missions"
            )));
        }
        model.missions.extend(dismissed);
        check_missions(model, turn)?;
    } else {
        model.status = MatchStatus::Finished;
        for player in &mut model.players {
            player.spectator = true;
        }
    }
    Ok(())
}

/// Independently rolls and resolves a fauna encounter for each eligible in-flight mission.
///
/// Launch and arrival turns are deliberately excluded. A mission must have completed at least one
/// movement step and still be more than one turn from its destination, so only complete turns spent
/// between launch and arrival can trigger an encounter. Missile Strikes and Deep Cover missions are
/// always exempt.
fn resolve_space_fauna_encounters<R: Rng + ?Sized>(
    model: &mut GameModel,
    turn: usize,
    rng: &mut R,
) {
    let chance = model.rules.space_fauna_percent;
    if chance == 0 {
        return;
    }

    let mut eligible = model
        .missions
        .iter()
        .filter(|mission| {
            mission.travel_turns > 0
                && mission.turns_to_destination(&model.map) > 1
                && mission.army.has_army()
                && mission.objective != Icon::MissileStrike
                && !mission.deep_cover
        })
        .map(|mission| mission.id)
        .collect::<Vec<_>>();
    eligible.sort_unstable();

    for mission_id in eligible {
        if rng.random_range(0..100) >= chance {
            continue;
        }
        let Some(index) = model.missions.iter().position(|mission| mission.id == mission_id) else {
            continue;
        };
        let mut original = model.missions[index].clone();

        let (formation_name, fauna_army) = encounter_formation(turn, rng);
        let mut fauna_site = model.map.get(original.destination).clone();
        fauna_site.name.clone_from(&formation_name);
        fauna_site.position = original.position;
        fauna_site.resources = Resources::default();
        fauna_site.operations = Default::default();
        fauna_site.terraformer_focus = None;
        fauna_site.command_relay_active = true;
        fauna_site.shield_overload = ShieldOverloadState::Ready;
        fauna_site.fleet_withdrawal = FleetWithdrawal::Off;
        fauna_site.is_destroyed = false;
        fauna_site.owned = None;
        // A temporary controller lets the shared resolver retain neutral survivors. It is
        // removed from the persisted report immediately after resolution.
        fauna_site.controlled = Some(0);
        fauna_site.army = fauna_army.into();
        fauna_site.protection_permissions.clear();
        fauna_site.buy.clear();
        fauna_site.surface_build_order = [None; 4];

        let spying = original.objective == Icon::Spy;
        let defenseless_colony = original.objective == Icon::Colonize
            && !original.army.iter().any(|(unit, count)| *count > 0 && unit.is_combat_ship());
        let mut battle_mission = original.clone();
        battle_mission.objective = if spying {
            Icon::Spy
        } else {
            Icon::Attack
        };
        battle_mission.protected_player = None;
        battle_mission.return_objective = None;
        battle_mission.bombing = BombingRaid::None;
        // Dedicated Spy missions withdraw their surviving Probes after the first round and keep
        // travelling. Probes attached to other objectives remain with their fleet for the battle.
        battle_mission.combat_probes = !spying;
        if defenseless_colony {
            // Probes cannot turn an otherwise unescorted Colony Ship into a viable combat fleet.
            battle_mission.army.retain(|unit, _| *unit == Unit::colony_ship());
        }
        battle_mission.deep_cover = false;
        // A coordinated assault meets fauna one travelling contingent at a time.
        battle_mission.joint_attack = None;

        let mut report = resolve_combat_with_retreat_with_rng(
            turn,
            &battle_mission,
            &fauna_site,
            EnergyGrid {
                supply: 1,
                demand: 1,
            },
            None,
            rng,
        );
        report.mission = original.clone();
        report.planet.owned = None;
        report.planet.controlled = None;
        report.destination_owned = None;
        report.destination_controlled = None;
        if let Some(combat) = &mut report.combat_report {
            for unit in combat.rounds.iter_mut().flat_map(|round| &mut round.defender) {
                unit.owner = None;
            }
        }

        let player_survived = report.surviving_attacker.has_army();
        let outcome = if !player_survived {
            "was destroyed"
        } else if spying {
            "withdrew after one combat round"
        } else if report.surviving_defender.has_army() {
            "the battle ended in a draw; both sides withdrew"
        } else {
            "defeated the creatures"
        };
        report.mission.logs.push_str(&format!(
            "\n- ({turn}) Encountered {formation_name} in deep space and {outcome}."
        ));

        if let Some(player) = model.players.iter_mut().find(|player| player.id == original.owner) {
            player.push_report(report.clone());
        }
        if player_survived {
            original.army = report.surviving_attacker;
            original.logs = report.mission.logs;
            model.missions[index] = original;
        } else {
            model.missions.remove(index);
        }
    }
}

/// Deducts a due loan only when its complete fixed bundle is available. An unaffordable loan
/// remains due and is retried after production on the next turn. The overdue record also blocks
/// new Resource Hub borrowing until the complete empire-wide due bundle can be settled.
fn settle_resource_hub_loans(model: &mut GameModel, resolved_turn: u64) {
    let due = model.resource_loans.iter().filter(|loan| loan.due_turn <= resolved_turn).fold(
        BTreeMap::<PlayerId, Resources>::new(),
        |mut due, loan| {
            *due.entry(loan.player_id).or_default() += loan.repayment();
            due
        },
    );
    let mut settled_players = HashSet::new();
    for (player_id, repayment) in due {
        if let Some(player) = model.players.iter_mut().find(|player| player.id == player_id) {
            if player.resources.contains(repayment) {
                player.resources -= repayment;
                settled_players.insert(player_id);
            }
        }
    }
    model
        .resource_loans
        .retain(|loan| loan.due_turn > resolved_turn || !settled_players.contains(&loan.player_id));
}

/// Replaces every due contingent with one lead mission while retaining per-player ownership.
fn consolidate_joint_attack_arrivals(model: &mut GameModel) {
    let due_ids = model
        .missions
        .iter()
        .filter(|mission| {
            mission.joint_attack.is_some() && mission.turns_to_destination(&model.map) < 2
        })
        .filter_map(|mission| mission.joint_attack.as_ref().map(|attack| attack.id))
        .collect::<BTreeSet<_>>();

    for attack_id in due_ids {
        let mut contingents = model
            .missions
            .extract_if(.., |mission| {
                mission.joint_attack.as_ref().is_some_and(|attack| attack.id == attack_id)
            })
            .collect::<Vec<_>>();
        let Some(leader) = contingents
            .first()
            .and_then(|mission| mission.joint_attack.as_ref())
            .map(|attack| attack.leader)
        else {
            continue;
        };
        let destination_controller = contingents
            .first()
            .and_then(|mission| model.map.try_get(mission.destination))
            .and_then(|planet| planet.controlled.or(planet.owned));
        let attacks_participant = destination_controller.is_some_and(|controller| {
            contingents.first().is_some_and(|mission| {
                mission
                    .joint_attack
                    .as_ref()
                    .is_some_and(|attack| attack.attackers.contains_key(&controller))
            })
        });
        if attacks_participant {
            let turn = usize::try_from(model.turn).unwrap_or(usize::MAX);
            for mut contingent in contingents {
                contingent.recall(&model.map, turn);
                model.missions.push(contingent);
            }
            continue;
        }
        contingents.sort_by_key(|mission| mission.owner);
        let lead_index =
            contingents.iter().position(|mission| mission.owner == leader).unwrap_or(0);
        let mut combined = contingents.remove(lead_index);
        let mut attackers = BTreeMap::new();
        let mut origins = BTreeMap::new();
        attackers.insert(combined.owner, combined.army.clone());
        origins.insert(combined.owner, combined.origin);
        for contingent in contingents {
            origins.insert(contingent.owner, contingent.origin);
            attackers.insert(contingent.owner, contingent.army.clone());
            for (unit, count) in contingent.army {
                let total = combined.army.entry(unit).or_default();
                *total = total.saturating_add(count);
            }
        }
        if let Some(attack) = &mut combined.joint_attack {
            attack.attackers = attackers;
            attack.origins = origins;
        }
        model.missions.push(combined);
    }
}

/// Docks every valid same-turn Protect arrival before any hostile mission is resolved.
///
/// This destination-first phase removes the previous shuffled-player ordering effect: a fleet
/// whose displayed arrival turn matches an attack is always present for that defense.
fn resolve_protect_arrivals<R: Rng + ?Sized>(model: &mut GameModel, turn: usize, rng: &mut R) {
    let mut groups = BTreeMap::<(PlanetId, PlayerId), Mission>::new();
    let mut arrived_ids = HashSet::new();
    for mission in model.missions.iter().filter(|mission| {
        mission.objective == Icon::Protect && mission.turns_to_destination(&model.map) < 2
    }) {
        arrived_ids.insert(mission.id);
        groups
            .entry((mission.destination, mission.owner))
            .and_modify(|grouped| grouped.merge(mission))
            .or_insert_with(|| mission.clone());
    }

    for ((destination_id, protector), mission) in groups {
        let destination = model.map.get_mut(destination_id);
        let mut report = resolve_combat_with_retreat_with_rng(
            turn,
            &mission,
            destination,
            Default::default(),
            None,
            rng,
        );
        report
            .mission
            .logs
            .push_str(&format!("\n- ({turn}) Protection fleet stationed at {}.", destination.name));
        destination.dock_protecting_fleet(protector, mission.army.clone());
        report.planet = destination.clone();
        report.surviving_defender = destination.army.clone();
        report.destination_owned = destination.owned;
        report.destination_controlled = destination.controlled;

        for player in &mut model.players {
            if player.id == protector || Some(player.id) == mission.protected_player {
                player.push_report(report.clone());
            }
        }
    }

    model.missions.retain(|mission| !arrived_ids.contains(&mission.id));
}

/// Sends stationed fleets home when their invitation or intended controller is no longer valid.
fn recall_unpermitted_protecting_fleets<R: Rng + ?Sized>(
    model: &mut GameModel,
    turn: usize,
    rng: &mut R,
    used_mission_ids: &mut HashSet<u64>,
    returning: &mut Vec<Mission>,
) -> Result<(), GameError> {
    let home_planets = model
        .players
        .iter()
        .map(|player| (player.id, player.home_planet))
        .collect::<BTreeMap<_, _>>();

    for planet_index in 0..model.map.planets.len() {
        let invalid = {
            let planet = &model.map.planets[planet_index];
            planet
                .army
                .protector_ids()
                .filter(|owner| !planet.allows_protection(*owner))
                .collect::<Vec<_>>()
        };
        for owner in invalid {
            let Some(home_planet) = home_planets.get(&owner).copied() else {
                continue;
            };
            let (origin, army) = {
                let planet = &mut model.map.planets[planet_index];
                let Some(army) = planet.army.remove_protector(owner) else {
                    continue;
                };
                (planet.clone(), army)
            };
            if origin.id == home_planet {
                model.map.get_mut(home_planet).dock(army);
                continue;
            }
            if model.map.get(home_planet).is_destroyed {
                continue;
            }
            let home = model.map.get(home_planet);
            let mission = protection_return_mission(
                next_unique_mission_id(rng, used_mission_ids)?,
                turn,
                owner,
                &origin,
                home,
                army,
                ProtectionReturnReason::AccessRevoked,
            );
            returning.push(mission);
        }
    }
    Ok(())
}

/// Reconciles every active mission with the latest ownership and destruction state of the map.
fn check_missions(model: &mut GameModel, turn: usize) -> Result<(), GameError> {
    let colony_limits = model
        .players
        .iter()
        .map(|player| colony_limit(model, player.id).map(|limit| (player.id, limit)))
        .collect::<Result<std::collections::HashMap<_, _>, _>>()?;
    let home_planets = model
        .players
        .iter()
        .map(|player| (player.id, player.home_planet))
        .collect::<std::collections::HashMap<_, _>>();
    for mission in &mut model.missions {
        check_mission(
            mission,
            &model.map,
            turn,
            colony_limits.get(&mission.owner).copied().unwrap_or(0),
            home_planets.get(&mission.owner).copied(),
        );
    }
    Ok(())
}

/// Records a scan or Relay diversion without exposing probes to the defending garrison.
fn resolve_spy_without_combat<R: Rng + ?Sized>(
    turn: usize,
    mission: &Mission,
    destination: &Planet,
    rng: &mut R,
) -> MissionReport {
    MissionReport {
        id: rng.random(),
        turn,
        mission: mission.clone(),
        planet: destination.clone(),
        scout_probes: mission.army.amount(&Unit::probe()),
        surviving_attacker: mission.army.clone(),
        surviving_defender: destination.army.clone(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: destination.owned,
        destination_controlled: destination.controlled,
        combat_report: None,
        hidden: false,
    }
}

/// Removes every observation a deceptive Relay could expose while preserving route metadata.
fn spoof_spy_report_as_empty(report: &mut MissionReport) {
    report.planet.owned = None;
    report.planet.controlled = None;
    report.planet.army.clear();
    report.planet.protection_permissions.clear();
    report.planet.buy.clear();
    report.planet.surface_build_order = [None; 4];
    report.planet.terraformer_focus = None;
    report.planet.operations = Default::default();

    report.planet.command_relay_active = true;
    report.planet.shield_overload = ShieldOverloadState::Ready;
    report.planet.fleet_withdrawal = FleetWithdrawal::Off;
    report.surviving_defender.clear();
    report.destination_owned = None;
    report.destination_controlled = None;
    report.combat_report = None;
}

/// Merges same-player missions by objective and original gameplay priority.
fn regroup_missions(missions: &[Mission]) -> Vec<Mission> {
    let mut protect: Option<Mission> = None;
    let mut deploy: Option<Mission> = None;
    let mut missile: Option<Mission> = None;
    let mut spy: Option<Mission> = None;
    // Each cover attempt must retain its own origin relay and secrecy outcome.
    let mut deep_cover = Vec::new();
    let mut rest: Option<Mission> = None;
    for mission in missions {
        if mission.objective == Icon::Spy && mission.deep_cover {
            deep_cover.push(mission.clone());
            continue;
        }
        let target = match mission.objective {
            Icon::MissileStrike => &mut missile,
            Icon::Spy => &mut spy,
            Icon::Protect => &mut protect,
            Icon::Deploy => &mut deploy,
            _ => &mut rest,
        };
        if let Some(grouped) = target {
            grouped.merge(mission);
        } else {
            *target = Some(mission.clone());
        }
    }
    [protect, deploy, missile, spy].into_iter().flatten().chain(deep_cover).chain(rest).collect()
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
    Err(GameError::MalformedState(NO_UNIQUE_MISSION_ID.to_string()))
}

/// Updates a mission whose destination ownership changed earlier in the turn.
fn check_mission(
    mission: &mut Mission,
    map: &Map,
    turn: usize,
    max_owned: usize,
    home_planet: Option<PlanetId>,
) {
    let old_objective = mission.objective;
    let destination = map.get(mission.destination);
    let protection_canceled = mission.objective == Icon::Protect
        && (mission.protected_player != destination.controlled
            || !destination.allows_protection(mission.owner));
    if protection_canceled {
        let return_destination = home_planet.unwrap_or_else(|| mission.check_origin(map));
        mission.origin = destination.id;
        mission.origin_owned = destination.owned;
        mission.origin_controlled = destination.controlled;
        mission.origin_army =
            destination.mission_origin_army(mission.owner).cloned().unwrap_or_default();
        mission.destination = return_destination;
        mission.send = turn;
        mission.travel_turns = 0;
        mission.objective = Icon::Deploy;
        mission.protected_player = None;
        mission.return_objective = Some(Icon::Protect);
        mission.bombing = BombingRaid::None;
        mission.combat_probes = false;
        mission.jump_gate = false;
        mission.logs.push_str(&format!(
            "\n- ({turn}) Protection access canceled; returning to home planet {}.",
            map.get(return_destination).name
        ));
        return;
    }
    // Colonization intent survives friendly ownership changes while the fleet travels.
    if destination.controlled == Some(mission.owner)
        && !matches!(
            mission.objective,
            Icon::Deploy | Icon::MissileStrike | Icon::Colonize | Icon::Protect
        )
    {
        mission.objective = Icon::Deploy;
        mission.protected_player = None;
    }
    if destination.controlled != Some(mission.owner) && mission.objective == Icon::Deploy {
        if destination.allows_protection(mission.owner) {
            mission.objective = Icon::Protect;
            mission.protected_player = destination.controlled;
        } else {
            mission.objective = Icon::Attack;
        }
    }
    let owned = map.planets.iter().filter(|planet| planet.owned == Some(mission.owner)).count();
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
        let return_destination = mission.check_origin(map);

        // This starts a new leg from the world that invalidated the old route. Keeping the
        // original departure world as `origin` can make a normal return encode the same planet
        // as both endpoints, which is not a valid persisted mission.
        mission.origin = destination.id;
        mission.origin_owned = destination.owned;
        mission.origin_controlled = destination.controlled;
        mission.origin_army.clone_from(destination.army.controller());
        mission.destination = return_destination;
        mission.send = turn;
        mission.travel_turns = 0;
        mission.objective = Icon::Deploy;
        mission.bombing = BombingRaid::None;
        mission.return_objective.get_or_insert(old_objective);
        mission.combat_probes = false;
        mission.jump_gate = false;
        mission.logs.push_str(&format!(
            "\n- ({turn}) Destination changed to planet {}.",
            map.get(mission.destination).name
        ));
    }
    if old_objective != mission.objective {
        mission.deep_cover = false;
        if mission.objective != Icon::Deploy && !mission.is_returning() {
            mission.return_objective = None;
        }
        mission.logs.push_str(&format!(
            "\n- ({turn}) Objective changed to {}.",
            mission.objective.to_name()
        ));
    }
}

/// Returns the configured colonization limit plus the operational home-world Senate bonus.
fn colony_limit(model: &GameModel, player_id: PlayerId) -> Result<usize, GameError> {
    let player = model.player(player_id)?;
    Ok(player.colony_limit(&model.map, model.rules.colonizable_percent))
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
