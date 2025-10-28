use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::core::combat::CombatStats;
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Unit {
    Building(Building),
    Ship(Ship),
    Defense(Defense),
}

impl Unit {
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

    pub fn is_building(&self) -> bool {
        matches!(self, Unit::Building(_))
    }

    pub fn is_ship(&self) -> bool {
        matches!(self, Unit::Ship(_))
    }

    pub fn is_defense(&self) -> bool {
        matches!(self, Unit::Defense(_))
    }

    pub fn production(&self) -> usize {
        match self {
            Unit::Building(_) => 1,
            Unit::Ship(s) => s.production(),
            Unit::Defense(d) => d.production(),
        }
    }

    pub fn get(&self, stat: &CombatStats) -> String {
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
