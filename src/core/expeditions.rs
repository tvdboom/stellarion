//! Deterministic deep-space expeditions, distance-based discoveries, and physical cargo returns.

use std::collections::HashSet;
use std::ops::RangeInclusive;

use bevy::prelude::Vec2;
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

pub use crate::core::constants::COLONY_SHIP_CARGO;
pub use crate::core::raids::cargo_capacity;

use crate::core::combat::report::MissionReport;
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::missions::{BombingRaid, Mission, MissionId};
use crate::core::patrols::{can_return_to, return_port};
use crate::core::raids::surviving_cargo;
use crate::core::resources::Resources;
use crate::core::simulation::GameModel;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Price, Unit};

const RECOVERABLE_SHIPS: [Ship; 8] = [
    Ship::LightFighter,
    Ship::HeavyFighter,
    Ship::Destroyer,
    Ship::Cruiser,
    Ship::Bomber,
    Ship::Battleship,
    Ship::Dreadnought,
    Ship::WarSun,
];
const SALVAGE_PROBABILITY: f64 = 0.99;

struct ShipRecovery {
    ship: Ship,
    count: usize,
    weight: usize,
}

/// Explicit lifecycle prevents a recalled fleet from receiving a discovery.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ExpeditionPhase {
    /// Travelling to the selected exploration site.
    Outbound,
    /// Carrying the expedition's outcome to a safe port.
    Returning,
}

/// Exploration site and cargo; rewards are credited only on docking.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expedition {
    /// Fixed coordinate selected on the map.
    pub center: Vec2,
    /// Outbound exploration or physical return travel.
    pub phase: ExpeditionPhase,
    /// Recovered resource cargo, lost with the expedition if it is destroyed.
    pub resources: Resources,
    /// Recovered vessels carried home separately from resource cargo.
    pub ships: Army,
}

/// Returns the distance from controlled territory and an outlook score centered at 50.
#[cfg(feature = "app")]
pub(crate) fn outlook(
    map: &Map,
    mission: &Mission,
    arrival_turn: usize,
    fuel: usize,
) -> (Option<f64>, u8) {
    let Some(expedition) = &mission.expedition else {
        return (None, 0);
    };
    if !valid_expedition_target(map, mission.origin, expedition.center) {
        return (None, 0);
    }

    let territory_distance = distance_to_controlled_world(map, mission.owner, expedition.center);
    let colonies = mission.army.amount(&Unit::colony_ship());
    if colonies == 0 {
        return (territory_distance, 0);
    }
    let distance = territory_distance.unwrap_or_default();
    let capacity = cargo_capacity(&mission.army);
    let resource_chance = resource_discovery_probability(distance);
    let ship_chance = ship_discovery_probability(distance, mission.army.total_production());
    let mean = |range: RangeInclusive<usize>| (*range.start() as f64 + *range.end() as f64) / 2.0;
    let ships = ship_recoveries(distance, arrival_turn, &mission.army);
    let total_weight = ships.iter().map(|recovery| recovery.weight).sum::<usize>();
    let mean_ship_value = ships
        .iter()
        .map(|recovery| {
            recovery.weight as f64 * recovery.count as f64 * recovery.ship.price().total() as f64
        })
        .sum::<f64>()
        / total_weight as f64;

    let expected_value = resource_chance * mean(resource_haul(capacity, distance))
        + ship_chance * mean_ship_value
        + (1.0 - resource_chance)
            * (1.0 - ship_chance)
            * SALVAGE_PROBABILITY
            * mean(salvage_haul(capacity, distance));
    let score = (100.0 * expected_value / (expected_value + fuel as f64)).round() as u8;
    (territory_distance, score)
}

/// Planets and moons use the current controller, including after captures and abandonment.
fn distance_to_controlled_world(map: &Map, owner: PlayerId, center: Vec2) -> Option<f64> {
    let center = center.as_dvec2();
    map.planets
        .iter()
        .filter(|world| world.controlled == Some(owner))
        .map(|world| world.position.as_dvec2().distance_squared(center))
        .min_by(f64::total_cmp)
        .map(|squared| squared.sqrt() / f64::from(Planet::SIZE))
}

fn resource_discovery_probability(distance: f64) -> f64 {
    (0.95 * distance / (distance + 15.0)) * (2.0 / 3.0)
}

fn resource_haul(capacity: usize, distance: f64) -> RangeInclusive<usize> {
    let distance_factor = 0.25 + 0.75 * distance / (distance + 15.0);
    let maximum = ((capacity as f64 * distance_factor) as usize).min(capacity);
    maximum / 2..=maximum
}

fn salvage_haul(capacity: usize, distance: f64) -> RangeInclusive<usize> {
    // Sites near controlled territory still yield a small amount of salvage.
    let maximum = ((capacity as f64 * 0.1 * distance / (distance + 15.0)) as usize).max(1);
    (maximum / 2).max(1)..=maximum
}

fn ship_discovery_probability(distance: f64, production: usize) -> f64 {
    let remoteness = distance / (distance + 8.0);
    let fleet = production as f64 / (production as f64 + 40.0);
    (remoteness * (0.80 + 0.15 * fleet)).min(0.95)
}

fn ship_recoveries(distance: f64, turn: usize, army: &Army) -> [ShipRecovery; 8] {
    let production = army.total_production() as f64;
    let remoteness = distance / (distance + 8.0);
    let age = turn.saturating_sub(1) as f64;
    let maturity = age / (age + 30.0);
    let fleet = production / (production + 40.0);
    let quality = remoteness * maturity * fleet;

    // Production committed to this mission, rather than empire income, determines
    // recovery capability. Diminishing returns improve the mix of ships; a separate
    // resource-value budget prevents a lone Colony Ship from finding capital ships.
    // Turn 1 has no heavy-ship weight, and later turns gradually improve that weight.
    let budget = ((100.0 + 35.0 * production) * (0.5 + 1.5 * remoteness) * (0.5 + 0.5 * maturity))
        .clamp(50.0, 20_000.0) as usize;
    let count_limit = army.amount(&Unit::colony_ship()).min(1_000) * 10;
    RECOVERABLE_SHIPS.map(|ship| {
        let count = (budget / ship.price().total()).min(count_limit);
        let count = if ship == Ship::WarSun {
            count.min(1)
        } else {
            count
        };
        let weight = match ship {
            Ship::LightFighter => 400.0,
            Ship::HeavyFighter => 300.0,
            Ship::Destroyer => 200.0,
            Ship::Cruiser => 150.0 + 200.0 * quality,
            Ship::Bomber => 100.0 + 100.0 * quality,
            Ship::Battleship => 600.0 * quality,
            Ship::Dreadnought => 700.0 * quality,
            Ship::WarSun => 500.0 * quality * quality,
            Ship::Probe | Ship::ColonyShip => 0.0,
        };
        ShipRecovery {
            ship,
            count,
            weight: if count == 0 {
                0
            } else {
                weight.round() as usize
            },
        }
    })
}

/// Allows every finite point within the map that requires travel from the launch world.
pub fn valid_expedition_target(map: &Map, origin: PlanetId, center: Vec2) -> bool {
    center.is_finite()
        && map.rect.contains(center)
        && map.try_get(origin).is_some_and(|origin| origin.position != center)
}

/// Chance of finding any reward when a lone Colony Ship reaches the site.
/// Escort production slightly increases ship discovery odds for larger fleets.
pub fn success_probability(map: &Map, owner: PlayerId, center: Vec2) -> f64 {
    if !center.is_finite() || !map.rect.contains(center) {
        return 0.0;
    }
    let distance = distance_to_controlled_world(map, owner, center).unwrap_or_default();
    let resource_chance = resource_discovery_probability(distance);
    let ship_chance = ship_discovery_probability(distance, Unit::colony_ship().production());
    1.0 - (1.0 - resource_chance) * (1.0 - ship_chance) * (1.0 - SALVAGE_PROBABILITY)
}

impl Mission {
    /// Creates a space expedition. Launch validation separately requires at least one Colony Ship.
    pub fn new_expedition(
        id: MissionId,
        turn: usize,
        owner: PlayerId,
        origin: &Planet,
        center: Vec2,
        army: Army,
    ) -> Self {
        let mut mission = Self::new_with_id(
            id,
            turn,
            owner,
            origin,
            origin,
            Icon::Expedition,
            army,
            BombingRaid::None,
            true,
            false,
            None,
        );
        mission.expedition = Some(Expedition {
            center,
            phase: ExpeditionPhase::Outbound,
            resources: Resources::default(),
            ships: Army::new(),
        });
        let launch = center - origin.position;
        // Start short expeditions partway along their route so they still have a flight and fuel cost.
        mission.position = origin.position
            + launch.normalize_or_zero() * (Planet::SIZE * 0.7).min(launch.length() * 0.5);
        if mission.position == center {
            mission.position = origin.position;
        }
        mission.logs = format!(
            "- ({turn}) Expedition sent to ({:.1}, {:.1}) AU.",
            center.x / Planet::SIZE,
            center.y / Planet::SIZE,
        );
        mission
    }

    /// Validates live and historical snapshots, including the expedition's original launch point.
    pub(crate) fn valid_expedition(&self, map: &Map) -> bool {
        let Some(expedition) = &self.expedition else {
            return self.objective != Icon::Expedition
                && self.return_objective != Some(Icon::Expedition);
        };
        valid_expedition_target(map, self.origin, expedition.center)
            && self.patrol.is_none()
            && self.intercept.is_none()
            && !self.jump_gate
            && !self.deep_cover
            && self.joint_attack.is_none()
            && self.protected_player.is_none()
            && self.bombing == BombingRaid::None
            && self.raid_cargo.is_empty()
            && self.army.has_army()
            && self.army.keys().all(Unit::is_ship)
            && expedition.resources.total() <= cargo_capacity(&self.army)
            && expedition.ships.iter().all(|(unit, count)| {
                matches!(unit, Unit::Ship(ship) if RECOVERABLE_SHIPS.contains(ship))
                    && (1..=10_000).contains(count)
                    && (*unit != Unit::war_sun() || *count == 1)
            })
            && match expedition.phase {
                ExpeditionPhase::Outbound => {
                    self.objective == Icon::Expedition
                        && self.return_objective.is_none()
                        && self.origin == self.destination
                        && expedition.resources.is_empty()
                        && expedition.ships.is_empty()
                        && self.army.amount(&Unit::colony_ship()) > 0
                },
                ExpeditionPhase::Returning => {
                    self.objective == Icon::Deploy
                        && self.return_objective == Some(Icon::Expedition)
                },
            }
    }

    /// Recalls and lost ports use the same free, physical return. No friendly port is conquered.
    pub(crate) fn return_expedition(&mut self, map: &Map, turn: usize) {
        if let Some(port) = return_port(map, self.owner, self.destination, self.position) {
            self.destination = port;
        }
        if let Some(expedition) = &mut self.expedition {
            expedition.phase = ExpeditionPhase::Returning;
        }
        self.objective = Icon::Deploy;
        self.return_objective = Some(Icon::Expedition);
        self.send = turn;
        self.travel_turns = 0;
        self.logs.push_str(&format!(
            "\n- ({turn}) Expedition returning to {}.",
            map.get(self.destination).name,
        ));
    }
}

fn discover<R: Rng + ?Sized>(mission: &mut Mission, map: &Map, turn: usize, rng: &mut R) {
    let Some(expedition) = &mut mission.expedition else {
        return;
    };
    let colonies = mission.army.amount(&Unit::colony_ship());
    if colonies == 0 {
        return;
    }

    // A stranded fleet with no controlled worlds can still find a little salvage.
    let distance =
        distance_to_controlled_world(map, mission.owner, expedition.center).unwrap_or_default();
    let capacity = cargo_capacity(&mission.army);
    if rng.random_bool(ship_discovery_probability(distance, mission.army.total_production())) {
        let recoveries = ship_recoveries(distance, turn, &mission.army);
        // A surviving Colony Ship always has at least one affordable Light Fighter option.
        let mut roll =
            rng.random_range(0..recoveries.iter().map(|recovery| recovery.weight).sum::<usize>());
        for recovery in recoveries {
            if roll < recovery.weight {
                expedition.ships.insert(Unit::Ship(recovery.ship), recovery.count);
                break;
            }
            roll -= recovery.weight;
        }
    }
    // Roll resources independently so ship discoveries can also bring home cargo.
    // Keep the existing full resource-haul chance and near-certain fallback salvage.
    if rng.random_bool(resource_discovery_probability(distance)) {
        expedition.resources = roll_resource_haul(resource_haul(capacity, distance), rng);
    } else if expedition.ships.is_empty() && rng.random_bool(SALVAGE_PROBABILITY) {
        expedition.resources = roll_resource_haul(salvage_haul(capacity, distance), rng);
    } else if expedition.ships.is_empty() {
        mission
            .logs
            .push_str(&format!("\n- ({turn}) No recoverable discoveries. Fleet returning."));
        return;
    }
    mission.logs.push_str(&format!(
        "\n- ({turn}) Discovery: {} metal, {} crystal, {} deuterium and {} recovered ships. Cargo is credited after docking.",
        expedition.resources.metal, expedition.resources.crystal, expedition.resources.deuterium,
        expedition.ships.values().copied().sum::<usize>(),
    ));
}

fn roll_resource_haul<R: Rng + ?Sized>(range: RangeInclusive<usize>, rng: &mut R) -> Resources {
    let total = rng.random_range(range);
    let metal = rng.random_range(0..=total);
    let crystal = rng.random_range(0..=total - metal);
    Resources::new(metal, crystal, total - metal - crystal)
}

/// Resolves arrival once in stable ID order, then prevents a second movement step this turn.
pub(crate) fn resolve_arrivals<R: Rng + ?Sized>(
    model: &mut GameModel,
    turn: usize,
    blocked: &mut HashSet<MissionId>,
    rng: &mut R,
) {
    for mission in &mut model.missions {
        if let Some(expedition) = &mut mission.expedition {
            expedition.resources = surviving_cargo(expedition.resources, &mission.army, rng);
            if mission.objective == Icon::Expedition
                && mission.army.amount(&Unit::colony_ship()) == 0
            {
                mission.return_expedition(&model.map, turn);
                blocked.insert(mission.id);
            }
        }
    }
    let mut ids = model
        .missions
        .iter()
        .filter(|mission| {
            mission.expedition.is_some()
                && !blocked.contains(&mission.id)
                && mission.turns_to_destination(&model.map) < 2
        })
        .map(|mission| mission.id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    for id in ids {
        let Some(index) = model.missions.iter().position(|mission| mission.id == id) else {
            continue;
        };
        let mut mission = model.missions[index].clone();
        if mission.objective == Icon::Expedition {
            mission.advance(&model.map);
            discover(&mut mission, &model.map, turn, rng);
            mission.return_expedition(&model.map, turn);
            model.missions[index] = mission;
            blocked.insert(id);
        } else if can_return_to(model.map.get(mission.destination), mission.owner) {
            mission.advance(&model.map);
            mission
                .logs
                .push_str(&format!("\n- ({turn}) Expedition docked; recovered cargo delivered."));
            let planet = model.map.get_mut(mission.destination);
            let mut returning = mission.army.clone();
            if let Some(expedition) = &mission.expedition {
                for (unit, count) in &expedition.ships {
                    let entry = returning.entry(*unit).or_default();
                    *entry = entry.saturating_add(*count);
                }
            }
            let report = MissionReport {
                id: rng.random::<u64>().max(1),
                turn,
                mission: mission.clone(),
                planet: planet.clone(),
                scout_probes: 0,
                surviving_attacker: returning.clone(),
                surviving_defender: planet.army.clone(),
                planet_colonized: false,
                planet_destroyed: false,
                raid_loot: Resources::default(),
                destination_owned: planet.owned,
                destination_controlled: planet.controlled,
                combat_report: None,
                hidden: false,
            };
            planet.dock_with_battle_plan(returning, mission.battle_plan);
            if let Some(player) = model.players.iter_mut().find(|player| player.id == mission.owner)
            {
                if let Some(expedition) = &mission.expedition {
                    player.resources += expedition.resources;
                }
                player.push_report(report);
            }
            model.missions.remove(index);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/core/expeditions.rs"]
mod tests;
