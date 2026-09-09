//! Derived per-turn energy supply, demand, and deterministic brownout scaling.

use crate::core::constants::{
    ORBITAL_RAILGUN_ENERGY_PER_LEVEL, PS_OVERLOAD_BONUS_PERCENT_PER_LEVEL, PS_OVERLOAD_ENERGY_COST,
    PS_SHIELD_PER_LEVEL, REACTOR_ENERGY_PER_LEVEL, TIDAL_GENERATOR_ENERGY_PER_LEVEL,
};
use crate::core::identity::PlayerId;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, SolarBand};
use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Unit};

/// Minimum resource output retained during a complete grid collapse.
const MIN_RESOURCE_EFFICIENCY_PERCENT: usize = 30;
/// Percentage points lost for each unit of unmet energy demand.
const EFFICIENCY_LOSS_PER_MISSING_ENERGY: usize = 10;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
/// One player's derived energy state for the current turn.
pub struct EnergyGrid {
    /// Total non-stored energy generated this turn.
    pub supply: usize,
    /// Total infrastructure demand this turn.
    pub demand: usize,
}

impl EnergyGrid {
    /// Returns the supply and demand added by one level of a building.
    pub fn for_building(building: Building, solar_band: Option<SolarBand>) -> Self {
        match building {
            // Construction, storage, transport, and administration are situational services rather
            // than continuous loads.
            Building::LunarBase
            | Building::Shipyard
            | Building::Factory
            | Building::MissileSilo
            | Building::Recycler
            | Building::CommandRelay
            | Building::ColonialAdministration => Self::default(),
            Building::TidalGenerator => Self {
                supply: TIDAL_GENERATOR_ENERGY_PER_LEVEL,
                demand: 0,
            },
            Building::Reactor => Self {
                supply: REACTOR_ENERGY_PER_LEVEL,
                demand: 0,
            },
            Building::SolarSatellite => Self {
                supply: solar_band.map_or(0, SolarBand::satellite_energy),
                demand: 0,
            },
            Building::Senate => Self {
                supply: 0,
                demand: 2,
            },
            Building::MetalMine
            | Building::CrystalMine
            | Building::DeuteriumSynthesizer
            | Building::Terraformer
            | Building::SensorPhalanx
            | Building::PlanetaryShield
            | Building::JumpGate
            | Building::Laboratory
            | Building::OrbitalRadar => Self {
                supply: 0,
                demand: 1,
            },
            Building::OrbitalRailgun => Self {
                supply: 0,
                demand: ORBITAL_RAILGUN_ENERGY_PER_LEVEL,
            },
        }
    }

    /// Returns the supply or demand added by one level or copy of a unit.
    pub fn for_unit(unit: Unit, solar_band: Option<SolarBand>) -> Self {
        match unit {
            Unit::Building(building) => Self::for_building(building, solar_band),
            unit if unit == Unit::space_dock() => Self {
                supply: 0,
                demand: 2,
            },
            _ => Self::default(),
        }
    }

    /// Calculates the grid from infrastructure on worlds controlled by this player.
    pub fn for_player(player_id: PlayerId, map: &Map) -> Self {
        let mut grid = Self::default();
        for planet in map.planets.iter().filter(|planet| {
            planet.controlled.or(planet.owned) == Some(player_id) && !planet.is_destroyed
        }) {
            let world = Self::for_world(map, planet);
            grid.supply = grid.supply.saturating_add(world.supply);
            grid.demand = grid.demand.saturating_add(world.demand);
        }
        grid
    }

    /// Calculates next turn's grid, including infrastructure already queued for construction.
    pub fn for_player_next_turn(player_id: PlayerId, map: &Map) -> Self {
        let mut grid = Self::for_player(player_id, map);
        for planet in map.planets.iter().filter(|planet| {
            planet.controlled.or(planet.owned) == Some(player_id) && !planet.is_destroyed
        }) {
            let solar_band = map.solar_band(planet.id);
            for unit in &planet.buy {
                grid = grid.with_unit(*unit, solar_band, 1);
            }
        }
        grid
    }

    /// Calculates the independent energy production and demand of one planet or moon.
    pub fn for_world(map: &Map, planet: &Planet) -> Self {
        if planet.is_destroyed {
            return Self::default();
        }

        let solar_band = map.solar_band(planet.id);
        let grid = planet.army.iter().fold(Self::default(), |mut grid, (unit, levels)| {
            let per_level = Self::for_unit(*unit, solar_band);
            grid.supply = grid.supply.saturating_add(per_level.supply.saturating_mul(*levels));
            grid.demand = grid.demand.saturating_add(per_level.demand.saturating_mul(*levels));
            grid
        });
        if planet.shield_overload.is_overloaded()
            && planet.army.amount(&Unit::planetary_shield()) > 0
        {
            Self {
                demand: grid.demand.saturating_add(PS_OVERLOAD_ENERGY_COST),
                ..grid
            }
        } else {
            grid
        }
    }

    /// Returns signed surplus or shortage. Surplus is never stored.
    pub fn balance(self) -> i128 {
        self.supply as i128 - self.demand as i128
    }

    /// Returns operating efficiency after losing ten percentage points per missing Energy.
    pub fn efficiency_percent(self) -> usize {
        let shortage = self.demand.saturating_sub(self.supply);
        100usize
            .saturating_sub(shortage.saturating_mul(EFFICIENCY_LOSS_PER_MISSING_ENERGY))
            .max(MIN_RESOURCE_EFFICIENCY_PERCENT)
    }

    /// Returns this grid with the supply and demand from additional units included.
    pub fn with_unit(self, unit: Unit, solar_band: Option<SolarBand>, count: usize) -> Self {
        let added = Self::for_unit(unit, solar_band);
        Self {
            supply: self.supply.saturating_add(added.supply.saturating_mul(count)),
            demand: self.demand.saturating_add(added.demand.saturating_mul(count)),
        }
    }

    /// Returns this grid with a one-turn action cost added to demand.
    pub fn with_action_demand(self, demand: usize) -> Self {
        Self {
            demand: self.demand.saturating_add(demand),
            ..self
        }
    }

    /// Applies the grid ratio to normal resource income with a recovery floor.
    pub fn scale_resources(self, resources: Resources) -> Resources {
        resources.scaled_percent(self.efficiency_percent())
    }

    /// Returns Planetary Shield strength scaled by the same efficiency as resource output.
    pub fn planetary_shield(self, levels: usize, overloaded: bool) -> usize {
        let base = levels.saturating_mul(PS_SHIELD_PER_LEVEL);
        let powered =
            ((base as u128).saturating_mul(self.efficiency_percent() as u128) / 100) as usize;
        if overloaded {
            let percent =
                100usize.saturating_add(levels.saturating_mul(PS_OVERLOAD_BONUS_PERCENT_PER_LEVEL));
            ((powered as u128).saturating_mul(percent as u128) / 100).min(usize::MAX as u128)
                as usize
        } else {
            powered
        }
    }
}

#[cfg(test)]
#[path = "../../tests/core/energy.rs"]
mod tests;
