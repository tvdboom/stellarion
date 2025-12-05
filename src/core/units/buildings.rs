use std::iter::Iterator;

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::resources::Resources;
use crate::core::units::{Description, Price};

#[derive(
    EnumIter, Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
pub enum Building {
    LunarBase,
    DemolitionNexus,
    MetalMine,
    CrystalMine,
    DeuteriumSynthesizer,
    Shipyard,
    Factory,
    MissileSilo,
    PlanetaryShield,
    Reactor,
    SensorPhalanx,
    JumpGate,
    Laboratory,
    OrbitalRadar,
}

impl Building {
    pub const MAX_LEVEL: usize = 5;
}

impl Description for Building {
    fn description(&self) -> &str {
        match self {
            Building::LunarBase => {
                "The Lunar Base increases the number of fields on the moon, allowing extra buildings \
                to be built. Every level of the Base increases the number of fields by 1."
            },
            Building::DemolitionNexus => {
                "A hardened subterranean safeguard facility designed to deny invaders the full \
                value of your moon. When the moon is conquered by an enemy, one level of a random \
                lunar building (excluding the Lunar Base) is destroyed for each level of the Nexus. \
                The Demolition Nexus don't take up lunar fields."
            },
            Building::MetalMine => {
                "The Metal Mine is the building that produces metal. The amount of metal produced \
                each turn is equal to the planet's base metal times the mine's level."
            },
            Building::CrystalMine => {
                "The Crystal Mine is the building that produces crystal. The amount of crystal \
                produced each turn is equal to the planet's base crystal times the mine's level."
            },
            Building::DeuteriumSynthesizer => {
                "The Deuterium Synthesizer is the building that produces deuterium. The amount \
                of deuterium produced each turn is equal to the planet's base deuterium times the \
                synthesizer's level."
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
                of the silo, 10 missile slots are made available (every missile takes up 1 slot)."
            },
            Building::PlanetaryShield => {
                "The Planetary Shield is a defensive structure with high shield power but no \
                damage. Enemy ships must first destroy the Planetary Shield before they can \
                attack the planet's buildings or defenses (not ships!). Each level of the \
                building increases the shield with 100. This shield does not regenerate after \
                every combat round. Interplanetary Missiles ignore the Planetary Shield."
            },
            Building::Reactor => {
                "The Reactor is a high-output energy facility that enhances the efficiency of \
                every ship launched from the planet. It optimizes fuel consumption through \
                advanced power regulation and heat-recovery systems. Each level of the Reactor \
                reduces the deuterium required for fleet travel with 10%."
            },
            Building::SensorPhalanx => {
                "The Sensor Phalanx scans the space around a planet to detect enemy attacks. \
                A Phalanx of level N scans the space at 0.8 * N AU from the planet, and it only \
                sees units with production <= N. The objective of the enemy mission is not \
                revealed. Spying missions are not detected by the Phalanx."
            },
            Building::JumpGate => {
                "The Jump Gate enables rapid travel between two owned planets with jump gates \
                (at any distance in space). Thus, having only a single gate is useless. Jumps \
                always take 1 turn and costs no fuel, independent of the fleet's composition. \
                Upgrading the Jump Gate increases the number of ships it can transport per turn."
            },
            Building::Laboratory => {
                "The Laboratory allows to convert resources of one type to another. The higher \
                the level of the laboratory, the cheaper the conversion becomes. The Laboratory \
                can only be constructed on a moon."
            },
            Building::OrbitalRadar => {
                "The Orbital Radar scans the universe for enemy fleets. A Radar of level N reveals \
                missions at N AU from the moon, and it only sees units with production <= N. It \
                works similar to the Sensor Phalanx, but has longer reach and detects any mission \
                in range (including Spy and Missile Strike), and not only those targeting the moon. \
                The Orbital radar can only be build on a moon."
            },
        }
    }
}

impl Price for Building {
    fn price(&self) -> Resources {
        match self {
            Building::LunarBase => Resources::new(200, 200, 200),
            Building::DemolitionNexus => Resources::new(300, 50, 50),
            Building::MetalMine => Resources::new(0, 200, 200),
            Building::CrystalMine => Resources::new(300, 0, 200),
            Building::DeuteriumSynthesizer => Resources::new(300, 200, 0),
            Building::Shipyard => Resources::new(400, 200, 100),
            Building::Factory => Resources::new(300, 200, 100),
            Building::MissileSilo => Resources::new(300, 300, 300),
            Building::PlanetaryShield => Resources::new(200, 100, 200),
            Building::Reactor => Resources::new(250, 150, 0),
            Building::SensorPhalanx => Resources::new(400, 300, 300),
            Building::JumpGate => Resources::new(500, 300, 500),
            Building::Laboratory => Resources::new(200, 200, 400),
            Building::OrbitalRadar => Resources::new(400, 300, 300),
        }
    }
}
