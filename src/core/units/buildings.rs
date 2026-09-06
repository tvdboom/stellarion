//! Planetary and lunar building kinds with their construction economics.

use std::iter::Iterator;

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::resources::Resources;
use crate::core::units::{Description, Price};

#[derive(
    EnumIter, Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
/// Constructible economic, industrial, military, and lunar structures.
pub enum Building {
    /// The lunar base building.
    LunarBase,
    /// The tidal generator building.
    TidalGenerator,
    /// The metal mine building.
    MetalMine,
    /// The crystal mine building.
    CrystalMine,
    /// The deuterium synthesizer building.
    DeuteriumSynthesizer,
    /// The shipyard building.
    Shipyard,
    /// The factory building.
    Factory,
    /// The missile silo building.
    MissileSilo,
    /// The planetary shield building.
    PlanetaryShield,
    /// The reactor building.
    Reactor,
    /// The robotics building.
    Robotics,
    /// The orbital solar-satellite network.
    SolarSatellite,
    /// The orbital command-relay network.
    CommandRelay,
    /// The sensor phalanx building.
    SensorPhalanx,
    /// The jump gate building.
    JumpGate,
    /// The laboratory building.
    Laboratory,
    /// The orbital radar building.
    OrbitalRadar,
    /// The home-world senate building.
    Senate,
}

impl Building {
    /// Highest construction level supported for upgradeable buildings.
    pub const MAX_LEVEL: usize = 5;

    /// Returns the production-equivalent tier used when Probes reveal this building.
    pub fn production(&self) -> usize {
        match self {
            Building::MetalMine | Building::CrystalMine | Building::DeuteriumSynthesizer => 1,
            Building::Shipyard | Building::Factory | Building::MissileSilo => 2,
            Building::PlanetaryShield | Building::Reactor => 3,
            Building::Robotics => 4,
            Building::Senate => 5,
            _ => 1,
        }
    }
}

impl Description for Building {
    /// Returns the user-facing description of this gameplay value.
    fn description(&self) -> &str {
        match self {
            Building::LunarBase => {
                "The Lunar Base increases the number of fields on the moon, allowing extra buildings \
                to be built. Every level of the Base increases the number of fields by 1."
            },
            Building::TidalGenerator => {
                "The Tidal Generator converts gravitational stress between a moon and its parent \
                planet into power for the empire-wide grid. The Tidal Generator doesn't take up \
                lunar fields."
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
                attack the planet's buildings or defenses. Each level of the building increases \
                the shield with 300. This shield does not regenerate after every combat round. \
                Interplanetary Missiles ignore the Planetary Shield."
            },
            Building::Reactor => {
                "The Reactor is a high-output energy facility that enhances the efficiency of \
                every ship launched from the planet. It optimizes fuel consumption through \
                advanced power regulation and heat-recovery systems. Each level reduces the \
                deuterium required for fleet travel by 10%."
            },
            Building::Robotics => {
                "Robotics automates repetitive work in the Shipyard and Factory. Each completed \
                level adds 2 production capacity to both facilities on this planet, but does not \
                unlock units above their own building level."
            },
            Building::SolarSatellite => {
                "Solar Satellites collect stellar radiation in orbit and transmit power to the \
                empire-wide grid. Solar Satellites can only be constructed around planets."
            },
            Building::CommandRelay => {
                "The Command Relay extends the range of Spy missions launched from its planet. \
                Without a Relay, Probes can reach one sixth of the galaxy from that origin. Each \
                completed level adds another sixth, and level 5 can reach every world."
            },
            Building::SensorPhalanx => {
                "The Sensor Phalanx scans the space around a planet to detect enemy attacks. \
                A Phalanx of level N scans the space at 1.0 * N AU from the planet, and it only \
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
                missions at 1.2 * N AU from the moon, and it only sees units with production <= N. It \
                works similar to the Sensor Phalanx, but has longer reach and detects any mission \
                in range (including Spy and Missile Strike), and not only those targeting the moon. \
                The Orbital radar can only be build on a moon."
            },
            Building::Senate => {
                "The Senate coordinates an empire's colonial administration. Only one Senate may \
                be constructed on the home planet. Larger, more restrictive galaxies permit up to \
                three levels."
            },
        }
    }
}

impl Price for Building {
    /// Returns the resource cost of producing this unit.
    fn price(&self) -> Resources {
        match self {
            Building::LunarBase => Resources::new(200, 200, 200),
            Building::TidalGenerator => Resources::new(200, 50, 50),
            Building::MetalMine => Resources::new(0, 200, 200),
            Building::CrystalMine => Resources::new(300, 0, 200),
            Building::DeuteriumSynthesizer => Resources::new(300, 200, 0),
            Building::Shipyard => Resources::new(400, 200, 100),
            Building::Factory => Resources::new(300, 200, 100),
            Building::MissileSilo => Resources::new(200, 200, 200),
            Building::PlanetaryShield => Resources::new(200, 100, 200),
            Building::Reactor => Resources::new(200, 100, 0),
            Building::Robotics => Resources::new(300, 250, 100),
            Building::SolarSatellite => Resources::new(100, 150, 0),
            Building::CommandRelay => Resources::new(300, 250, 250),
            Building::SensorPhalanx => Resources::new(250, 200, 150),
            Building::JumpGate => Resources::new(500, 300, 500),
            Building::Laboratory => Resources::new(200, 200, 400),
            Building::OrbitalRadar => Resources::new(400, 300, 300),
            Building::Senate => Resources::new(1000, 750, 500),
        }
    }
}
