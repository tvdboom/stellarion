use crate::core::resources::Resources;
use crate::core::units::{Description, Price};
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::iter::Iterator;
use strum_macros::EnumIter;

pub type Complex = HashMap<Building, usize>;

#[derive(Component, EnumIter, Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub enum Building {
    Mine,
    Shipyard,
    Factory,
    MissileSilo,
    PlanetaryShield,
    SensorPhalanx,
    JumpGate,
}

impl Building {
    pub const MAX_LEVEL: usize = 5;
}

impl Description for Building {
    fn description(&self) -> &str {
        match self {
            Building::Mine => {
                "The mine is the building that produces resources. The amount of resources \
                mined each turn is equal to the planet's base resources times the mine's level."
            },
            Building::Shipyard => {
                "The shipyard is responsible for the construction of all ships. At higher levels, \
                more advanced ships can be build. Higher levels also increase the production \
                limit, i.e., the number of ships that can be build per turn."
            },
            Building::Factory => {
                "The factory is responsible for the construction of planet defenses. At higher \
                levels, more advanced defenses can be build. Higher levels also increase the \
                production limit, i.e., the number of defenses that can be build per turn."
            },
            Building::MissileSilo => {
                "A missile silo is a building that launches and stores missiles. For each level \
                of the silo, 10 missile slots are made available (every missile take up 1 slot)."
            },
            Building::PlanetaryShield => {
                "The planetary shield is a defensive structure with high shield power but no \
                damage. Enemy ships must first destroy the planetary shield before they can \
                attack the defenses. Each level of the building increases the shield with 50. \
                Interplanetary missiles ignore the planetary shield."
            },
            Building::SensorPhalanx => {
                "The sensor phalanx scans the space around a planet to detect incoming attacks. \
                The higher the level of the phalanx, the more accurate the scanned information."
            },
            Building::JumpGate => {
                "The jump gate enables rapid travel between two controlled planets with jump \
                gates (at any distance in space). Thus, having only a single gate is useless. \
                Jumps always take 1 turn, independent of the fleet's composition. Upgrading the \
                jump gate increases the number of ships it can transport per turn."
            },
        }
    }
}

impl Price for Building {
    fn price(&self) -> Resources {
        match self {
            Building::Mine => Resources::new(500, 100, 0),
            Building::Shipyard => Resources::new(400, 200, 100),
            Building::Factory => Resources::new(300, 200, 100),
            Building::MissileSilo => Resources::new(300, 300, 300),
            Building::PlanetaryShield => Resources::new(200, 100, 200),
            Building::SensorPhalanx => Resources::new(400, 300, 300),
            Building::JumpGate => Resources::new(500, 300, 500),
        }
    }
}
