use std::collections::HashMap;

use itertools::Itertools;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::core::combat::stats::CombatStats;
use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::utils::NameFromEnum;

pub mod buildings;
pub mod defense;
pub mod ships;

pub trait Description {
    fn description(&self) -> &str;
}

pub trait Price {
    fn price(&self) -> Resources;
}

pub trait Combat {
    fn hull(&self) -> usize;
    fn shield(&self) -> usize;
    fn damage(&self) -> usize;
    fn rapid_fire(&self) -> HashMap<Unit, usize> {
        HashMap::new()
    }
    fn speed(&self) -> f32 {
        0.
    }
    fn fuel_consumption(&self) -> usize {
        0
    }
}

pub type Army = HashMap<Unit, usize>;

pub trait Amount {
    fn amount(&self, unit: &Unit) -> usize;
    fn has_army(&self) -> bool;
    fn total_production(&self) -> usize;
}

impl Amount for Army {
    fn amount(&self, unit: &Unit) -> usize {
        *self.get(unit).unwrap_or(&0)
    }
    fn has_army(&self) -> bool {
        self.iter().any(|(_, c)| *c > 0)
    }
    fn total_production(&self) -> usize {
        self.iter().filter_map(|(u, c)| (*c > 0).then_some(u.production())).sum()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum Unit {
    Building(Building),
    Ship(Ship),
    Defense(Defense),
}

impl Unit {
    pub fn buildings() -> Vec<Self> {
        Building::iter().map(|b| Unit::Building(b)).collect()
    }

    pub fn ships() -> Vec<Self> {
        Ship::iter().map(|b| Unit::Ship(b)).collect()
    }

    pub fn defenses() -> Vec<Self> {
        Defense::iter().map(|b| Unit::Defense(b)).collect()
    }

    pub fn all() -> Vec<Vec<Self>> {
        vec![Self::buildings(), Self::ships(), Self::defenses()]
    }

    pub fn all_valid(is_moon: bool) -> Vec<Vec<Self>> {
        if !is_moon {
            vec![
                Self::buildings()
                    .into_iter()
                    .filter(|u| {
                        !Self::lunar_buildings()
                            .iter()
                            .filter(|u| **u != Unit::Building(Building::Shipyard))
                            .contains(u)
                    })
                    .collect(),
                Self::ships(),
                Self::defenses(),
            ]
        } else {
            vec![Self::lunar_buildings(), Self::ships()]
        }
    }

    pub fn all_firing_order() -> Vec<Self> {
        Unit::ships()
            .into_iter()
            .chain(std::iter::once(Unit::space_dock()))
            .chain(Unit::defenses())
            .filter(|u| *u != Unit::crawler())
            .chain(std::iter::once(Unit::crawler()))
            .collect()
    }

    pub fn resource_buildings() -> Vec<Self> {
        vec![
            Unit::Building(Building::MetalMine),
            Unit::Building(Building::CrystalMine),
            Unit::Building(Building::DeuteriumSynthesizer),
        ]
    }

    pub fn industrial_buildings() -> Vec<Self> {
        vec![
            Unit::Building(Building::Shipyard),
            Unit::Building(Building::Factory),
            Unit::Building(Building::MissileSilo),
        ]
    }

    pub fn lunar_buildings() -> Vec<Self> {
        vec![
            Unit::Building(Building::LunarBase),
            Unit::Building(Building::DemolitionNexus),
            Unit::Building(Building::Shipyard),
            Unit::Building(Building::Laboratory),
            Unit::Building(Building::OrbitalRadar),
        ]
    }

    pub fn planetary_shield() -> Self {
        Unit::Building(Building::PlanetaryShield)
    }

    pub fn probe() -> Self {
        Unit::Ship(Ship::Probe)
    }

    pub fn colony_ship() -> Self {
        Unit::Ship(Ship::ColonyShip)
    }

    pub fn crawler() -> Self {
        Unit::Defense(Defense::Crawler)
    }

    pub fn space_dock() -> Self {
        Unit::Defense(Defense::SpaceDock)
    }

    pub fn antiballistic_missile() -> Self {
        Unit::Defense(Defense::AntiballisticMissile)
    }

    pub fn interplanetary_missile() -> Self {
        Unit::Defense(Defense::InterplanetaryMissile)
    }

    pub fn is_building(&self) -> bool {
        matches!(self, Unit::Building(_))
    }

    pub fn is_ship(&self) -> bool {
        matches!(self, Unit::Ship(_))
    }

    pub fn is_defense(&self) -> bool {
        matches!(self, Unit::Defense(_))
    }

    pub fn is_turret(&self) -> bool {
        matches!(self, Unit::Defense(d) if *d != Defense::Crawler && *d != Defense::SpaceDock && !d.is_missile())
    }

    pub fn is_missile(&self) -> bool {
        matches!(self, Unit::Defense(d) if d.is_missile())
    }

    pub fn consumes_field(&self) -> bool {
        self.is_building()
            && !matches!(
                self,
                Unit::Building(Building::LunarBase) | Unit::Building(Building::DemolitionNexus)
            )
    }

    pub fn is_economic_building(&self) -> bool {
        Self::resource_buildings().contains(self)
    }

    pub fn is_industrial_building(&self) -> bool {
        Self::industrial_buildings().contains(self)
    }

    pub fn is_combat_ship(&self) -> bool {
        match self {
            Unit::Ship(s) if !matches!(s, Ship::Probe | Ship::ColonyShip) => true,
            _ => false,
        }
    }

    pub fn production(&self) -> usize {
        match self {
            Unit::Building(_) => 1,
            Unit::Ship(s) => s.production(),
            Unit::Defense(d) => d.production(),
        }
    }

    pub fn get_stat(&self, stat: &CombatStats) -> String {
        let n = match stat {
            CombatStats::Hull => self.hull() as f32,
            CombatStats::Shield => self.shield() as f32,
            CombatStats::Damage => self.damage() as f32,
            CombatStats::Production => self.production() as f32,
            CombatStats::Speed => self.speed(),
            CombatStats::FuelConsumption => self.fuel_consumption() as f32,
            CombatStats::RapidFire => self.rapid_fire().values().sum::<usize>() as f32,
        };

        if n == 0. {
            "---".to_string()
        } else {
            n.to_string()
        }
    }

    pub fn to_name(&self) -> String {
        match self {
            Unit::Building(b) => b.to_name(),
            Unit::Ship(s) => s.to_name(),
            Unit::Defense(d) => d.to_name(),
        }
    }

    pub fn to_lowername(&self) -> String {
        match self {
            Unit::Building(b) => b.to_lowername(),
            Unit::Ship(s) => s.to_lowername(),
            Unit::Defense(d) => d.to_lowername(),
        }
    }
}

impl Description for Unit {
    fn description(&self) -> &str {
        match self {
            Unit::Building(b) => b.description(),
            Unit::Ship(s) => s.description(),
            Unit::Defense(d) => d.description(),
        }
    }
}

impl Price for Unit {
    fn price(&self) -> Resources {
        match self {
            Unit::Building(b) => b.price(),
            Unit::Ship(s) => s.price(),
            Unit::Defense(d) => d.price(),
        }
    }
}

impl Combat for Unit {
    fn hull(&self) -> usize {
        match self {
            Unit::Building(_) => 0,
            Unit::Ship(s) => s.hull(),
            Unit::Defense(d) => d.hull(),
        }
    }

    fn shield(&self) -> usize {
        match self {
            Unit::Building(_) => 0,
            Unit::Ship(s) => s.shield(),
            Unit::Defense(d) => d.shield(),
        }
    }

    fn damage(&self) -> usize {
        match self {
            Unit::Building(_) => 0,
            Unit::Ship(s) => s.damage(),
            Unit::Defense(d) => d.damage(),
        }
    }

    fn rapid_fire(&self) -> HashMap<Unit, usize> {
        match self {
            Unit::Building(_) => HashMap::new(),
            Unit::Ship(s) => s.rapid_fire(),
            Unit::Defense(d) => d.rapid_fire(),
        }
    }

    fn speed(&self) -> f32 {
        match self {
            Unit::Building(_) => 0.,
            Unit::Ship(s) => s.speed(),
            Unit::Defense(d) => d.speed(),
        }
    }

    fn fuel_consumption(&self) -> usize {
        match self {
            Unit::Building(_) => 0,
            Unit::Ship(s) => s.fuel_consumption(),
            Unit::Defense(d) => d.fuel_consumption(),
        }
    }
}
