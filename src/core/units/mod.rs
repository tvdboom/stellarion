use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::utils::NameFromEnum;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub mod buildings;
pub mod defense;
pub mod missions;
pub mod ships;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
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

    pub fn level(&self) -> usize {
        match self {
            Unit::Building(_) => 0,
            Unit::Ship(s) => s.level(),
            Unit::Defense(d) => d.level(),
        }
    }

    pub fn description(&self) -> &str {
        match self {
            Unit::Building(b) => b.description(),
            Unit::Ship(s) => s.description(),
            Unit::Defense(d) => d.description(),
        }
    }

    pub fn price(&self) -> Resources {
        match self {
            Unit::Building(b) => b.price(),
            Unit::Ship(s) => s.price(),
            Unit::Defense(d) => d.price(),
        }
    }
}

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
