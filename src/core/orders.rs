//! Shared purchase and mission validation used by both the UI and deterministic resolution.

use thiserror::Error;

use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::player::Player;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Price, Unit};

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
    /// The fleet does not satisfy its mission objective.
    #[error("The selected fleet does not meet the mission objective's requirements.")]
    Objective,
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

/// Returns the maximum legal purchase, including every already-queued unit.
pub fn purchase_limit(player: &Player, planet: &Planet, unit: Unit) -> Result<usize, OrderError> {
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
    let capacity = match unit {
        Unit::Building(_) => {
            if planet.army.amount(&unit) >= Building::MAX_LEVEL || planet.buy.contains(&unit) {
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
            if unit == Unit::space_dock() {
                if planet.has(&unit) || planet.buy.contains(&unit) {
                    return Err(OrderError::SpaceDock);
                }
                capacity.min(1)
            } else if defense.is_missile() {
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
    origin: &Planet,
    destination: &Planet,
    mission: &Mission,
) -> Result<(), OrderError> {
    if player.spectator
        || mission.owner != player.id
        || !player.controls(origin)
        || origin.is_destroyed
        || destination.is_destroyed
        || mission.origin != origin.id
        || mission.destination != destination.id
        || origin.id == destination.id
    {
        return Err(OrderError::Ownership);
    }
    if !mission.objective.accepts_army(&mission.army)
        || (destination.is_moon() && mission.objective.on_planet_only())
        || !Icon::objectives(player.owns(destination), player.controls(destination))
            .contains(&mission.objective)
    {
        return Err(OrderError::Objective);
    }
    if mission.army.iter().any(|(unit, count)| *count > origin.army.amount(unit)) {
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
    if mission.jump_gate
        && (mission.objective != Icon::Deploy
            || !player.owns(origin)
            || !player.owns(destination)
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
