//! Shared purchase and mission validation used by both the UI and deterministic resolution.

use thiserror::Error;

use crate::core::constants::MIN_SPY_PROBES;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::patrols::{valid_patrol_target, PatrolPhase};
use crate::core::player::Player;
use crate::core::sabotage::known_targets;
use crate::core::units::buildings::Building;
use crate::core::units::operations::SenateSupport;
use crate::core::units::ships::Ship;
use crate::core::units::{orbitals, Amount, Price, Unit};

/// A player-facing reason why an order cannot currently be accepted.
#[derive(Clone, Copy, Debug, Error, Eq, PartialEq)]
pub enum OrderError {
    /// A mission cannot depart from and arrive at the same world.
    #[error("A mission needs different origin and destination worlds.")]
    SameWorld,
    /// The player cannot use the specified world.
    #[error("This world is not available to this player.")]
    Ownership,
    /// The unit is not constructible on this kind of world.
    #[error("This unit cannot be built on this world.")]
    Unit,
    /// One or more resource balances are insufficient.
    #[error("Not enough resources.")]
    Resources,
    /// A structure has reached its maximum permitted level.
    #[error("Building is already at maximum level.")]
    BuildingAtMaximumLevel,
    /// A structure can only be upgraded once per turn.
    #[error("Building is already queued.")]
    BuildingAlreadyQueued,
    /// A lunar construction requires another field.
    #[error("No lunar field is available.")]
    Fields,
    /// An orbital or ship needs a higher completed Shipyard level.
    #[error("Requires Shipyard level {0}.")]
    ShipyardLevel(usize),
    /// A ground defense needs a higher completed Factory level.
    #[error("Requires Factory level {0}.")]
    FactoryLevel(usize),
    /// A missile needs a higher completed Missile Silo level.
    #[error("Requires Missile Silo level {0}.")]
    MissileSiloLevel(usize),
    /// Queued ships have consumed the available fleet production.
    #[error("Not enough fleet production.")]
    FleetProduction,
    /// Queued defenses have consumed the available defense production.
    #[error("Not enough defense production.")]
    DefenseProduction,
    /// Stationed and queued missiles occupy every silo slot.
    #[error("Not enough Missile Silo capacity.")]
    MissileCapacity,
    /// A space dock is already stationed.
    #[error("Space Dock is already built.")]
    SpaceDockAlreadyBuilt,
    /// A space dock is already queued.
    #[error("Space Dock is already queued.")]
    SpaceDockAlreadyQueued,
    /// The Senate is restricted to the home world and its match-specific level cap.
    #[error("The Senate can only be built on the home planet up to this match's level limit.")]
    Senate,
    /// Colonial Administration needs a colony rather than the player's homeworld.
    #[error("Colonial Administration can only be built on a non-home planet.")]
    ColonialAdministration,
    /// The dispatched fleet is missing a required unit or includes a disallowed unit.
    #[error("{0}")]
    MissionFleet(&'static str),
    /// The selected objective requires a different destination or route.
    #[error("{0}")]
    MissionDestination(&'static str),
    /// Dedicated espionage requires a meaningful probe group.
    #[error("Spy and Sabotage missions require at least 5 Probes.")]
    SpyProbes,
    /// Sabotage requires an orbital structure disclosed to the attacking player.
    #[error("Select a known Sensor Phalanx, Orbital Railgun or Jump Gate on an enemy planet.")]
    SabotageTarget,
    /// Deep Cover needs a dedicated probe mission and a completed relay at its origin.
    #[error("Deep Cover requires a Spy or Sabotage mission and a Command Relay at the origin.")]
    DeepCover,
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
    senate: SenateSupport,
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
            if planet.army.amount(&unit) >= maximum {
                return Err(OrderError::BuildingAtMaximumLevel);
            }
            if planet.buy.contains(&unit) {
                return Err(OrderError::BuildingAlreadyQueued);
            }
            if planet.is_moon()
                && unit.consumes_field()
                && planet.fields_consumed() >= planet.max_fields()
            {
                return Err(OrderError::Fields);
            }
            if let Some(required) = orbitals::production_level(unit) {
                if planet.army.amount(&Unit::Building(Building::Shipyard)) < required {
                    return Err(OrderError::ShipyardLevel(required));
                }
            }
            1
        },
        Unit::Ship(ship) => {
            let required = ship.production();
            if required > planet.army.amount(&Unit::Building(Building::Shipyard)) {
                return Err(OrderError::ShipyardLevel(required));
            }
            let capacity =
                senate.fleet_capacity(planet).saturating_sub(planet.fleet_production()) / required;
            if capacity == 0 {
                return Err(OrderError::FleetProduction);
            }
            capacity
        },
        Unit::Defense(defense) => {
            if unit == Unit::space_dock() {
                if planet.has(&unit) {
                    return Err(OrderError::SpaceDockAlreadyBuilt);
                }
                if planet.buy.contains(&unit) {
                    return Err(OrderError::SpaceDockAlreadyQueued);
                }
                let required = orbitals::production_level(unit).unwrap_or(defense.production());
                if planet.army.amount(&Unit::Building(Building::Shipyard)) < required {
                    return Err(OrderError::ShipyardLevel(required));
                }
                1
            } else {
                let required = defense.production();
                let infrastructure = if defense.is_missile() {
                    Building::MissileSilo
                } else {
                    Building::Factory
                };
                if required > planet.army.amount(&Unit::Building(infrastructure)) {
                    return Err(if defense.is_missile() {
                        OrderError::MissileSiloLevel(required)
                    } else {
                        OrderError::FactoryLevel(required)
                    });
                }
                let capacity =
                    senate.defense_capacity(planet).saturating_sub(planet.battery_production())
                        / required;
                if capacity == 0 {
                    return Err(OrderError::DefenseProduction);
                }
                if defense.is_missile() {
                    let remaining = planet.remaining_missile_capacity();
                    if remaining == 0 {
                        return Err(OrderError::MissileCapacity);
                    }
                    capacity.min(remaining)
                } else {
                    capacity
                }
            }
        },
        Unit::Fauna(_) => return Err(OrderError::Unit),
    };
    let affordable = (player.resources / unit.price()).min();
    if affordable == 0 {
        return Err(OrderError::Resources);
    }
    Ok(affordable.min(capacity))
}

/// Checks the dispatched fleet and all world-dependent mission requirements.
pub fn validate_mission(
    player: &Player,
    map: &Map,
    origin: &Planet,
    destination: &Planet,
    mission: &Mission,
) -> Result<(), OrderError> {
    validate_mission_with_patrols(player, map, origin, destination, mission, &[])
}

/// Checks mission requirements, allowing probes to reinforce an owned operating patrol.
pub fn validate_mission_with_patrols(
    player: &Player,
    map: &Map,
    origin: &Planet,
    destination: &Planet,
    mission: &Mission,
    missions: &[Mission],
) -> Result<(), OrderError> {
    let objective = mission.objective;
    if matches!(objective, Icon::Patrol | Icon::Intercept | Icon::Expedition) {
        let available = origin.mission_origin_army(player.id).ok_or(OrderError::Ownership)?;
        if player.spectator
            || mission.owner != player.id
            || !origin.can_launch_mission(player.id)
            || mission.origin != origin.id
            || mission.destination != origin.id
        {
            return Err(OrderError::Ownership);
        }
        if let Some(reason) = objective
            .fleet_unavailable_reason(&mission.army)
            .filter(|_| objective != Icon::Patrol || !mission.accepts_patrol_army(missions))
        {
            return Err(OrderError::MissionFleet(reason));
        }
        match objective {
            Icon::Expedition if !mission.valid_expedition(map) => {
                return Err(OrderError::MissionDestination(
                    "Choose a point inside the map away from the launch world.",
                ));
            },
            Icon::Intercept if !mission.valid_intercept(map) => {
                return Err(OrderError::MissionDestination(
                    "Choose a visible enemy fleet you can reach before it arrives.",
                ));
            },
            Icon::Patrol
                if !mission.valid_patrol(map)
                    || !mission.patrol.as_ref().is_some_and(|patrol| {
                        patrol.phase == PatrolPhase::Outbound
                            && valid_patrol_target(
                                map,
                                origin.id,
                                patrol.center,
                                patrol.remaining_turns,
                            )
                    }) =>
            {
                return Err(OrderError::MissionDestination(
                    "Choose a patrol zone in empty space and set a valid patrol duration.",
                ));
            },
            _ => {},
        }
        if mission.army.iter().any(|(unit, count)| *count > available.amount(unit)) {
            return Err(OrderError::Fleet);
        }
        return Ok(());
    }
    if origin.id == destination.id {
        return Err(OrderError::SameWorld);
    }
    if !mission.valid_sabotage()
        || (mission.objective == Icon::Sabotage
            && !mission.sabotage.is_some_and(|sabotage| {
                !sabotage.succeeded
                    && known_targets(player, destination, missions).contains(&sabotage.target)
            }))
    {
        return Err(OrderError::SabotageTarget);
    }
    if player.spectator
        || mission.owner != player.id
        || !origin.can_launch_mission(player.id)
        || destination.is_destroyed
        || mission.origin != origin.id
        || mission.destination != destination.id
    {
        return Err(OrderError::Ownership);
    }
    if objective.is_probe_mission() && mission.army.amount(&Unit::probe()) < MIN_SPY_PROBES {
        return Err(OrderError::SpyProbes);
    }
    if mission.deep_cover
        && (!mission.objective.is_probe_mission()
            || !origin.has(&Unit::Building(Building::CommandRelay)))
    {
        return Err(OrderError::DeepCover);
    }
    let protection_allowed = destination.allows_protection(player.id);
    if objective == Icon::Protect
        && (mission.protected_player != destination.controlled || !protection_allowed)
    {
        return Err(OrderError::MissionDestination(
            "Protect requires an invitation from the destination's current controller.",
        ));
    }
    if objective != Icon::Protect && mission.protected_player.is_some() {
        return Err(OrderError::MissionDestination(
            "Select Protect to send a fleet under a protection invitation.",
        ));
    }
    if let Some(reason) = objective.fleet_unavailable_reason(&mission.army) {
        return Err(OrderError::MissionFleet(reason));
    }
    if destination.is_moon() && objective.on_planet_only() {
        return Err(OrderError::MissionDestination(
            "This objective requires a planet as its destination.",
        ));
    }
    let owns_destination = player.owns(destination);
    let controls_destination = player.controls(destination);
    let already_protecting = destination.blocks_hostile_action_by(player.id);
    if !Icon::objectives(
        owns_destination,
        controls_destination,
        protection_allowed,
        already_protecting,
    )
    .contains(&objective)
    {
        return Err(OrderError::MissionDestination(match objective {
            Icon::Deploy => "Deploy requires a destination you own or control.",
            _ if owns_destination => "Choose Deploy for a world you own.",
            _ if controls_destination => "Choose Colonize or Deploy for a world you control.",
            _ if already_protecting => {
                "Choose Protect for a world where you have a protection fleet."
            },
            _ => "Choose a planet or moon mission objective for this destination.",
        }));
    }
    let available = origin.mission_origin_army(player.id).ok_or(OrderError::Ownership)?;
    if mission.army.iter().any(|(unit, count)| *count > available.amount(unit)) {
        return Err(OrderError::Fleet);
    }
    if mission.bombing != BombingRaid::None
        && (destination.is_moon()
            || !mission
                .army
                .iter()
                .any(|(unit, count)| *unit == Unit::Ship(Ship::Bomber) && *count > 0))
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
            || origin.operational_level(Building::JumpGate) == 0
            || destination.operational_level(Building::JumpGate) == 0
            || mission.jump_cost() > origin.max_jump_capacity().saturating_sub(origin.jump_gate))
    {
        return Err(OrderError::JumpGate);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../../tests/core/orders.rs"]
mod tests;
