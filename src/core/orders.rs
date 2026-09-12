//! Shared purchase and mission validation used by both the UI and deterministic resolution.

use thiserror::Error;

use crate::core::constants::MIN_SPY_PROBES;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::player::Player;
use crate::core::units::buildings::Building;
use crate::core::units::{orbitals, Amount, Price, Unit};

/// A player-facing reason why an order cannot currently be accepted.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrderError {
    /// The player cannot use the specified world.
    #[error("This world is not available to this player.")]
    Ownership,
    /// The unit is not constructible on this kind of world.
    #[error("This unit cannot be built on this world.")]
    Unit,
    /// One or more resource balances are insufficient.
    #[error("Not enough resources.")]
    Resources,
    /// A structure has reached its maximum level or is already queued.
    #[error("Building is at its maximum level or already queued.")]
    Building,
    /// A lunar construction requires another field.
    #[error("No lunar field is available.")]
    Fields,
    /// Required infrastructure or remaining production is insufficient.
    #[error("Required production level or capacity is unavailable.")]
    Production,
    /// Stationed and queued missiles occupy every silo slot.
    #[error("Missile silo is full.")]
    Missiles,
    /// A space dock is already stationed or queued.
    #[error("Only one Space Dock is allowed.")]
    SpaceDock,
    /// The Senate is restricted to the home world and its match-specific level cap.
    #[error("The Senate can only be built on the home planet up to this match's level limit.")]
    Senate,
    /// Colonial Administration needs a colony rather than the player's homeworld.
    #[error("Colonial Administration can only be built on a non-home planet.")]
    ColonialAdministration,
    /// The fleet does not satisfy its mission objective.
    #[error("The selected fleet does not meet the mission objective's requirements.")]
    Objective,
    /// Dedicated espionage requires a meaningful probe group.
    #[error("A Spy mission requires at least 5 Probes.")]
    SpyProbes,
    /// The fleet contains units not available at the origin.
    #[error("The selected units are not available at the origin.")]
    Fleet,
    /// A bombing raid needs bombers and a non-lunar target.
    #[error("Bombing requires Bombers and a planet destination.")]
    Bombing,
    /// Gate ownership, objective, infrastructure, or capacity is invalid.
    #[error("Jump Gate requirements or capacity are not met.")]
    JumpGate,
}

/// Returns the laboratory output, rounded down, without losing precision on large balances.
/// A missing laboratory produces nothing; levels above the cap use the level-five 1:1 rate.
pub fn conversion_output(amount: usize, laboratory_level: usize) -> usize {
    if laboratory_level == 0 {
        return 0;
    }
    let divisor = 2 + Building::MAX_LEVEL.saturating_sub(laboratory_level);
    (amount as u128 * 2 / divisor as u128) as usize
}

/// Returns the maximum legal purchase, including every already-queued unit.
pub fn purchase_limit(
    player: &Player,
    planet: &Planet,
    unit: Unit,
    senate_level_limit: usize,
) -> Result<usize, OrderError> {
    if player.spectator
        || planet.is_destroyed
        || !(player.owns(planet) || (planet.is_moon() && player.controls(planet)))
    {
        return Err(OrderError::Ownership);
    }
    if !unit.valid_on(planet.is_moon()) {
        return Err(OrderError::Unit);
    }
    let affordable = (player.resources / unit.price()).min();
    if affordable == 0 {
        return Err(OrderError::Resources);
    }
    if orbitals::production_level(unit)
        .is_some_and(|required| planet.army.amount(&Unit::Building(Building::Shipyard)) < required)
    {
        return Err(OrderError::Production);
    }
    let capacity = match unit {
        Unit::Building(building) => {
            if building == Building::ColonialAdministration && planet.id == player.home_planet {
                return Err(OrderError::ColonialAdministration);
            }
            let maximum = if building == Building::Senate {
                senate_level_limit
            } else {
                Building::MAX_LEVEL
            };
            if building == Building::Senate
                && (planet.id != player.home_planet || senate_level_limit == 0)
            {
                return Err(OrderError::Senate);
            }
            if planet.army.amount(&unit) >= maximum || planet.buy.contains(&unit) {
                return Err(OrderError::Building);
            }
            if planet.is_moon()
                && unit.consumes_field()
                && planet.fields_consumed() >= planet.max_fields()
            {
                return Err(OrderError::Fields);
            }
            1
        },
        Unit::Ship(ship) => {
            if ship.production() > planet.army.amount(&Unit::Building(Building::Shipyard)) {
                return Err(OrderError::Production);
            }
            planet.max_fleet_production().saturating_sub(planet.fleet_production())
                / ship.production()
        },
        Unit::Defense(defense) => {
            if unit == Unit::space_dock() {
                if planet.has(&unit) || planet.buy.contains(&unit) {
                    return Err(OrderError::SpaceDock);
                }
                return Ok(affordable.min(1));
            }
            let building = if defense.is_missile() {
                Building::MissileSilo
            } else {
                Building::Factory
            };
            if defense.production() > planet.army.amount(&Unit::Building(building)) {
                return Err(OrderError::Production);
            }
            let capacity =
                planet.max_battery_production().saturating_sub(planet.battery_production())
                    / defense.production();
            if defense.is_missile() {
                let remaining = planet.remaining_missile_capacity();
                if remaining == 0 {
                    return Err(OrderError::Missiles);
                }
                capacity.min(remaining)
            } else {
                capacity
            }
        },
    };
    if capacity == 0 {
        return Err(OrderError::Production);
    }
    Ok(affordable.min(capacity))
}

/// Checks the dispatched fleet and all world-dependent mission requirements.
pub fn validate_mission(
    player: &Player,
    _map: &Map,
    origin: &Planet,
    destination: &Planet,
    mission: &Mission,
) -> Result<(), OrderError> {
    if player.spectator
        || mission.owner != player.id
        || !(player.owns(origin) || player.controls(origin))
        || origin.is_destroyed
        || destination.is_destroyed
        || mission.origin != origin.id
        || mission.destination != destination.id
        || origin.id == destination.id
    {
        return Err(OrderError::Ownership);
    }
    if mission.objective == Icon::Spy && mission.army.amount(&Unit::probe()) < MIN_SPY_PROBES {
        return Err(OrderError::SpyProbes);
    }
    if (mission.objective == Icon::Protect
        && (mission.protected_player != destination.controlled
            || !destination.allows_protection(player.id)))
        || (mission.objective != Icon::Protect && mission.protected_player.is_some())
    {
        return Err(OrderError::Objective);
    }
    if !mission.objective.accepts_army(&mission.army)
        || (destination.is_moon() && mission.objective.on_planet_only())
        || !Icon::objectives(
            player.owns(destination),
            player.controls(destination),
            destination.allows_protection(player.id),
            destination.is_protected_by(player.id),
        )
        .contains(&mission.objective)
    {
        return Err(OrderError::Objective);
    }
    let available = origin.mission_origin_army(player.id).ok_or(OrderError::Ownership)?;
    if mission.army.iter().any(|(unit, count)| *count > available.amount(unit)) {
        return Err(OrderError::Fleet);
    }
    if mission.bombing != BombingRaid::None
        && (destination.is_moon()
            || !mission.army.iter().any(|(unit, count)| {
                *unit == Unit::Ship(crate::core::units::ships::Ship::Bomber) && *count > 0
            }))
    {
        return Err(OrderError::Bombing);
    }
    let jump_route_allowed = player.owns(origin)
        && (player.owns(destination)
            || (mission.objective == Icon::Protect
                && mission.protected_player == destination.controlled
                && destination.allows_protection(player.id)));
    if mission.jump_gate
        && (!matches!(mission.objective, Icon::Deploy | Icon::Protect)
            || !jump_route_allowed
            || !origin.has(&Unit::Building(Building::JumpGate))
            || !destination.has(&Unit::Building(Building::JumpGate))
            || mission.jump_cost() > origin.max_jump_capacity().saturating_sub(origin.jump_gate))
    {
        return Err(OrderError::JumpGate);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/core/orders.rs"]
mod tests;
