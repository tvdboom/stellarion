//! Planetary orbital roster and Shipyard unlock levels.

use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::Unit;

/// Planet-only orbital structures in shop display order.
pub const ALL: [Unit; 8] = [
    Unit::Building(Building::SolarSatellite),
    Unit::Building(Building::Recycler),
    Unit::Building(Building::CommandRelay),
    Unit::Building(Building::TradingPost),
    Unit::Building(Building::SensorPhalanx),
    Unit::Building(Building::JumpGate),
    Unit::Building(Building::OrbitalRailgun),
    Unit::Defense(Defense::SpaceDock),
];

/// Returns whether this unit belongs to the planet-only orbital roster.
pub const fn is_orbital(unit: Unit) -> bool {
    production_level(unit).is_some()
}

/// Returns the completed Shipyard level required to construct this orbital.
pub const fn production_level(unit: Unit) -> Option<usize> {
    match unit {
        Unit::Building(Building::SolarSatellite | Building::Recycler) => Some(1),
        Unit::Building(Building::CommandRelay) => Some(2),
        Unit::Building(Building::TradingPost | Building::SensorPhalanx) => Some(3),
        Unit::Building(Building::JumpGate) => Some(4),
        Unit::Building(Building::OrbitalRailgun) | Unit::Defense(Defense::SpaceDock) => Some(5),
        Unit::Building(_) | Unit::Ship(_) | Unit::Defense(_) => None,
    }
}

/// Returns the Spy intelligence level needed to reveal this orbital.
///
/// Strategic superstructures remain public even though they require a level-five Shipyard.
pub const fn intelligence_level(unit: Unit) -> Option<usize> {
    match unit {
        Unit::Building(Building::OrbitalRailgun) | Unit::Defense(Defense::SpaceDock) => None,
        _ => production_level(unit),
    }
}
