use std::iter::Iterator;

use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::resources::Resources;
use crate::core::units::{Description, Price};

#[derive(Component, EnumIter, Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
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
                "The Mine is the building that produces resources. The amount of resources \
                mined each turn is equal to the planet's base resources times the Mine's level."
            },
            Building::Shipyard => {
                "The Shipyard is responsible for the construction of all ships. At higher levels, \
                more advanced ships can be build. Higher levels also increase the production \
                limit, i.e., the number of ships that can be build per turn."
            },
            Building::Factory => {
                "The Factory is responsible for the construction of planet defenses. At higher \
                levels, more advanced defenses can be build. Higher levels also increase the \
                production limit, i.e., the number of defenses that can be build per turn."
            },
            Building::MissileSilo => {
                "A Missile Silo is a building that launches and stores missiles. For each level \
                of the silo, 10 missile slots are made available (every missile take up 1 slot)."
            },
            Building::PlanetaryShield => {
                "The Planetary Shield is a defensive structure with high shield power but no \
                damage. Enemy ships must first destroy the Planetary Shield before they can \
                attack the planet's defenses (not ships!). Each level of the building increases \
                the shield with 50. This shield does not regenerate after every combat round. \
                Interplanetary Missiles ignore the Planetary Shield."
            },
            Building::SensorPhalanx => {
                "The Sensor Phalanx scans the space around a planet to detect enemy attacks. \
                A Phalanx of level N scans the space at 0.6 * N AU from the planet, and it only \
                sees units with production <= N. Spying missions are not detected by the Phalanx."
            },
            Building::JumpGate => {
                "The Jump Gate enables rapid travel between two owned planets with jump gates \
                (at any distance in space). Thus, having only a single gate is useless. Jumps \
                always take 1 turn and costs no fuel, independent of the fleet's composition. \
                Upgrading the Jump Gate increases the number of ships it can transport per turn."
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
