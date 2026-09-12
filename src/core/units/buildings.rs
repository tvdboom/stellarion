//! Planetary and lunar building kinds with their construction economics.

use std::iter::Iterator;

use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::resources::Resources;
use crate::core::units::{orbitals, Description, Price, Unit};

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
    /// The planetary terraforming complex.
    Terraformer,
    /// The orbital solar-satellite network.
    SolarSatellite,
    /// Small orbital craft that recover resources from nearby wreckage and asteroid fields.
    Recycler,
    /// The orbital command-relay network.
    CommandRelay,
    /// The orbital commerce hub used for bilateral resource trading.
    TradingPost,
    /// The sensor phalanx building.
    SensorPhalanx,
    /// The jump gate building.
    JumpGate,
    /// The colossal orbital railgun used for synchronized planetary strikes.
    OrbitalRailgun,
    /// The laboratory building.
    Laboratory,
    /// The orbital radar building.
    OrbitalRadar,
    /// The home-world senate building.
    Senate,
    /// Colony-only administration coordinating fleet withdrawal to the homeworld.
    ColonialAdministration,
}

/// Standing withdrawal order, measured as lost ship production points from battle start.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum FleetWithdrawal {
    /// Defend the colony without an automatic withdrawal.
    #[default]
    Off,
    /// Withdraw after losing three quarters of the fleet.
    Losses75,
    /// Withdraw after losing half of the fleet.
    Losses50,
    /// Withdraw after losing one quarter of the fleet.
    Losses25,
    /// Begin withdrawal before the first exchange of fire.
    Immediate,
}

impl FleetWithdrawal {
    /// Settings in display order, including the always-available off switch.
    pub const ALL: [Self; 5] =
        [Self::Off, Self::Losses75, Self::Losses50, Self::Losses25, Self::Immediate];

    /// Minimum completed Administration level needed to select this order.
    pub const fn minimum_level(self) -> usize {
        match self {
            Self::Off => 0,
            Self::Losses75 => 1,
            Self::Losses50 => 2,
            Self::Losses25 => 3,
            Self::Immediate => 4,
        }
    }

    /// Percentage of initial fleet production points that must be destroyed.
    pub const fn losses_percent(self) -> Option<usize> {
        match self {
            Self::Off => None,
            Self::Losses75 => Some(75),
            Self::Losses50 => Some(50),
            Self::Losses25 => Some(25),
            Self::Immediate => Some(0),
        }
    }

    /// Compact label for the colony's withdrawal selector.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::Losses75 => "75%",
            Self::Losses50 => "50%",
            Self::Losses25 => "25%",
            Self::Immediate => "Immediate",
        }
    }
}

impl Building {
    /// Highest construction level supported for upgradeable buildings.
    pub const MAX_LEVEL: usize = 5;

    /// Returns the production-equivalent tier used by shared unit calculations.
    pub fn production(&self) -> usize {
        if let Some(level) = orbitals::production_level(Unit::Building(*self)) {
            return level;
        }

        match self {
            Building::Reactor | Building::Terraformer => 2,
            Building::Shipyard | Building::Factory | Building::MissileSilo => 3,
            Building::PlanetaryShield => 4,
            Building::Senate | Building::ColonialAdministration => 5,
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
                planet into power for the empire-wide grid. Each level takes up one lunar field."
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
                "The Shipyard constructs ships and orbital structures. At higher levels, more \
                advanced ships and orbitals can be built. Higher levels also increase the fleet \
                production limit, i.e., the number of ships that can be built per turn."
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
                deuterium required for fleet travel by 10% and produces 3 energy."
            },
            Building::Terraformer => {
                "The Terraformer specializes a planet's environment for one selected resource. \
                Each completed level increases production of the focused resource by 10% and \
                reduces production of each other resource by 10%. Selecting no focus switches \
                the Terraformer off."
            },
            Building::SolarSatellite => {
                "Solar Satellites collect stellar radiation in orbit and transmit power to the \
                empire-wide grid."
            },
            Building::Recycler => {
                "Recyclers dispatch small salvage craft from their planet. Each Recycler recovers \
                a variable haul every turn from a nearby battle debris or asteroid field."
            },
            Building::CommandRelay => {
                "An active Command Relay diverts undersized enemy Spy missions before combat and \
                feeds their Probes false telemetry. It safely returns groups of up to 5 Probes \
                per completed level with a report of an empty planet. Larger groups gather \
                intelligence normally."
            },
            Building::TradingPost => {
                "A Trading Post establishes a commerce route with another player's Trading Post \
                within 3 AU. Each completed level lets its owner send up to 500 Metal, Crystal, \
                and Deuterium combined in one bilateral trade per player pair each turn. Outgoing \
                resources are reserved when both players accept; incoming resources arrive when \
                the turn resolves."
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
            Building::OrbitalRailgun => {
                "The Orbital Railgun is a colossal superweapon. Once per turn, each Railgun can \
                join a strike against an enemy world within range. Railguns always aim at the \
                same target. Every firing level adds a 5% destruction chance, modified by -2% to \
                +2% depending on the target's size. Each Planetary Shield level removes 1%, or 2% \
                while overloaded. A \
                synchronized strike costs 1,000 Deuterium and 5 Energy per firing Railgun. Each \
                level extends the firing range by 2 AU. The Orbital Railgun is always visible by \
                all players in the galaxy."
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
                "The Senate is the political heart of your empire, where delegates chart its \
                course among the stars. Each Senate level lets you own one extra planet."
            },
            Building::ColonialAdministration => {
                "When attacked. enables the option to coordinate a strategic withdrawal to your \
                homeworld. Levels 1–4 unlock retreat at decreasing percentages of fleet losses. \
                Withdrawing ships endure one final enemy round without firing back. Level 5 \
                removes that final round."
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
            Building::Terraformer => Resources::new(300, 250, 100),
            Building::SolarSatellite => Resources::new(100, 150, 0),
            Building::Recycler => Resources::new(250, 200, 100),
            Building::CommandRelay => Resources::new(300, 250, 250),
            Building::TradingPost => Resources::new(400, 300, 250),
            Building::SensorPhalanx => Resources::new(250, 200, 150),
            Building::JumpGate => Resources::new(500, 300, 500),
            Building::OrbitalRailgun => Resources::new(750, 500, 750),
            Building::Laboratory => Resources::new(200, 200, 400),
            Building::OrbitalRadar => Resources::new(400, 300, 300),
            Building::Senate => Resources::new(1000, 750, 500),
            Building::ColonialAdministration => Resources::new(200, 100, 50),
        }
    }
}
