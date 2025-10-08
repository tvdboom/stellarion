use crate::core::resources::Resources;
use crate::core::units::{Description, Price};
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Component, EnumIter, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BuildingName {
    Mine,
    Shipyard,
    Factory,
    MissileSilo,
    PlanetaryShield,
    SensorPhalanx,
    JumpGate,
}

impl Description for BuildingName {
    fn description(&self) -> &str {
        match self {
            BuildingName::Mine => {
                "The mine is the building that produces resources. The amount of resources \
                mined each turn is equal to the planet's base resources times the mine's level."
            },
            BuildingName::Shipyard => {
                "The shipyard is responsible for the construction of all ships. A higher level \
                allows the construction of more advanced ships."
            },
            BuildingName::Factory => {
                "The factory is responsible for the construction of planet defenses. The higher \
                the level, the more advanced defenses can be built."
            }
            BuildingName::MissileSilo => {
                "A missile silo is a building that launches and stores missiles. A level 2 silo \
                is required to be able to build interplanetary missiles For each level of the silo, \
                10 missile slots are made available."
            },
            BuildingName::PlanetaryShield => {
                "The planetary shield is a defensive structure with high shield power to use as \
                fodder. They act as a single unit of fodder. Higher levels increase the shield \
                power."
            },
            BuildingName::SensorPhalanx => {
                "The sensor phalanx scans the space around a planet to detect incoming attacks. \
                The higher the level of the phalanx, the more accurate the scanned information."
            },
            BuildingName::JumpGate => {
                "The jump gate enables rapid travel between two controlled planets with jump \
                gates (at any distance in space). Thus, having only a single gate is useless. \
                Jumps always take 1 turn, independent of the fleet's composition. Upgrading the \
                jump gate increases the number of ships it can transport per turn."
            },
        }
    }
}

impl Price for BuildingName {
    fn price(&self) -> Resources {
        match self {
            BuildingName::Mine => Resources::new(500, 100, 0),
            BuildingName::Shipyard => Resources::new(400, 200, 100),
            BuildingName::Factory => Resources::new(300, 200, 100),
            BuildingName::MissileSilo => Resources::new(300, 300, 300),
            BuildingName::PlanetaryShield => Resources::new(200, 100, 200),
            BuildingName::SensorPhalanx => Resources::new(400, 300, 300),
            BuildingName::JumpGate => Resources::new(500, 300, 500),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Building {
    pub name: BuildingName,
    pub level: usize,
}

impl Building {
    pub const MAX_LEVEL: usize = 5;

    pub fn new(name: BuildingName) -> Self {
        Self {
            name,
            level: 1,
        }
    }
}
