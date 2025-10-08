use crate::core::units::Description;
use bevy::prelude::Component;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(Component, EnumIter, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum BuildingName {
    Mine,
    Shipyard,
    Factory,
    Silo,
    PlanetShield,
    SensorPhalanx,
    JumpGate,
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

impl Description for Building {
    fn description(&self) -> &str {
        match self.name {
            BuildingName::Mine => {
                "Mines are the backbone of any colony, providing the essential resources \
                needed for expansion and survival. Upgrading mines increases their efficiency, \
                allowing for greater resource extraction and supporting larger populations."
            },
            BuildingName::Shipyard => {
                "The shipyard is where all spacefaring vessels are constructed. A higher-level \
                shipyard allows for the construction of more advanced ships, enabling players to \
                build a formidable fleet to defend their colony or launch offensives against enemies."
            },
            BuildingName::Factory => {
                "Factories are essential for producing the components and materials needed for \
                building ships and defenses. Upgrading factories increases their production speed \
                and capacity, ensuring a steady supply of resources for your colony's growth."
            }
            BuildingName::Silo => {
                "Silos are used to store deuterium, a crucial resource for powering ships and \
                advanced technologies. Upgrading silos increases their storage capacity, allowing \
                players to stockpile more deuterium for future use."
            },
            BuildingName::PlanetShield => {
                "The planet shield provides a defensive barrier against incoming attacks. \
                Upgrading the planet shield increases its strength and durability, making it \
                harder for enemies to penetrate and cause damage to your colony."
            },
            BuildingName::SensorPhalanx => {
                "The sensor phalanx allows for the detection of enemy fleets and activities \
                in the vicinity of your colony. Upgrading the sensor phalanx increases its range \
                and sensitivity, providing better intelligence on potential threats."
            },
            BuildingName::JumpGate => {
                "The jump gate enables rapid travel between distant locations in space. \
                Upgrading the jump gate increases its stability and the frequency of jumps, \
                allowing for quicker deployment of fleets across the galaxy."
            },
        }
    }
}
