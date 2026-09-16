//! Unconstructible space-fauna combatants and deterministic encounter formations.

use std::collections::HashMap;

use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::units::{Army, Combat, Description, Unit};

/// Visual and audio family used when a creature attacks during combat playback.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaunaAttack {
    /// Expanding pressure rings and a resonant call.
    SonicPulse,
    /// Branching blue-white electrical discharge.
    Lightning,
    /// Corrosive green biological plasma.
    BioPlasma,
    /// Dense gravity wave that distorts the battlefield.
    GravityPulse,
    /// Focused violet energy from crystalline or extradimensional tissue.
    VoidLance,
    /// A broad stream of stellar fire.
    StellarFire,
}

#[derive(
    EnumIter, Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
)]
/// Hostile vacuum organisms that can interrupt an in-flight mission.
pub enum SpaceFauna {
    /// Small translucent ray that hunts in shoals using pressure waves.
    AetherRay,
    /// Electrically charged jellyfish-like drifter.
    IonWisp,
    /// Armored ray with corrosive glands.
    VoidManta,
    /// Immature void manta with incomplete armor and weaker corrosive glands.
    VoidMantaCalf,
    /// Jagged mineral organism that focuses coherent energy.
    CrystalLeviathan,
    /// Young crystal leviathan whose mineral plates have not yet interlocked.
    CrystalShardling,
    /// Asteroid-mimicking ambush predator with a gravity well for a maw.
    Gravemaw,
    /// Massive cephalopod that spits biological plasma.
    StarKraken,
    /// Four-limbed immature star kraken that remains close to its parent.
    StarKrakenSpawn,
    /// Herd-forming stellar grazer that releases defensive spore pulses.
    NebulaGrazer,
    /// Smaller grazer with folded membranes and an immature filter lattice.
    NebulaGrazerCalf,
    /// Long extradimensional predator that lashes targets with void energy.
    RiftSerpent,
    /// Radiation-sailed organism powered by an internal stellar furnace.
    SolarRoc,
    /// Solitary apex predator capable of breathing stellar fire.
    ElderStarDragon,
    /// Dangerous juvenile of the elder star dragon's vacuum-adapted lineage.
    StarDragonWyrmling,
}

impl SpaceFauna {
    /// Relative combat-value tier used in report strength bars.
    pub const fn production(self) -> usize {
        match self {
            Self::AetherRay
            | Self::IonWisp
            | Self::VoidMantaCalf
            | Self::CrystalShardling
            | Self::StarKrakenSpawn
            | Self::NebulaGrazerCalf => 1,
            Self::VoidManta | Self::NebulaGrazer => 2,
            Self::CrystalLeviathan | Self::Gravemaw | Self::StarDragonWyrmling => 3,
            Self::StarKraken | Self::RiftSerpent => 4,
            Self::SolarRoc => 5,
            Self::ElderStarDragon => 6,
        }
    }

    /// Creature-specific presentation family; it never affects deterministic damage.
    pub const fn attack(self) -> FaunaAttack {
        match self {
            Self::AetherRay | Self::NebulaGrazer | Self::NebulaGrazerCalf => {
                FaunaAttack::SonicPulse
            },
            Self::IonWisp => FaunaAttack::Lightning,
            Self::VoidManta | Self::VoidMantaCalf | Self::StarKraken | Self::StarKrakenSpawn => {
                FaunaAttack::BioPlasma
            },
            Self::Gravemaw => FaunaAttack::GravityPulse,
            Self::CrystalLeviathan | Self::CrystalShardling | Self::RiftSerpent => {
                FaunaAttack::VoidLance
            },
            Self::SolarRoc | Self::ElderStarDragon | Self::StarDragonWyrmling => {
                FaunaAttack::StellarFire
            },
        }
    }
}

impl Description for SpaceFauna {
    fn description(&self) -> &str {
        match self {
            Self::AetherRay => {
                "A swift shoaling predator whose resonant membranes launch pressure waves through vacuum. It has no shield, but its elastic body absorbs surprising punishment."
            },
            Self::IonWisp => {
                "A charged drifter that stores stellar wind in a luminous core and discharges it through branching tendrils. It has no shield and relies on erratic movement."
            },
            Self::VoidManta => {
                "An armored ray that sprays corrosive biological plasma from glands beneath its wings. Its shieldless chitin is considerably tougher than a fighter hull."
            },
            Self::VoidMantaCalf => {
                "An immature manta that shelters beneath an adult's wings. Its incomplete shieldless armor is lighter, but its corrosive discharge is already lethal."
            },
            Self::CrystalLeviathan => {
                "A mineral lifeform whose fractured core focuses a violet cutting beam. Crystalline plates provide enormous hull strength but cannot regenerate like a shield."
            },
            Self::CrystalShardling => {
                "A juvenile mineral organism that orbits a mature leviathan. Its fractured plates hold no shield, while its exposed core emits a short void lance."
            },
            Self::Gravemaw => {
                "An asteroid mimic that drags prey toward its lamprey maw with pulsing gravity nodules. It is slow, shieldless, and exceptionally hard to kill."
            },
            Self::StarKraken => {
                "A territorial cephalopod whose acidic plasma can tear through capital ships. Six armored limbs form a deep reserve of shieldless hull."
            },
            Self::StarKrakenSpawn => {
                "A four-limbed spawn that attacks whatever its parent marks as prey. It lacks a shield and mature reach, but hunts in a tightly coordinated brood."
            },
            Self::NebulaGrazer => {
                "A normally placid herd animal that answers threats with resonant spore bursts. Its layered hide supplies high hull strength without any energy shield."
            },
            Self::NebulaGrazerCalf => {
                "A young grazer defended by the herd. Folded filter membranes give it less hull than an adult, though its shieldless body still releases defensive pulses."
            },
            Self::RiftSerpent => {
                "A segmented predator partly anchored beyond normal space. It lashes fleets with void energy and carries no shield, only a vast armored body."
            },
            Self::SolarRoc => {
                "A radiation-sailed apex hunter powered by a stellar furnace. Its incandescent discharge is devastating, while a plated thorax forms a massive unshielded hull."
            },
            Self::ElderStarDragon => {
                "An ancient solitary leviathan and the deadliest known space fauna. It vents stellar fire, never retreats, and protects an immense shieldless body with black plates and vast solar sails."
            },
            Self::StarDragonWyrmling => {
                "A juvenile vacuum leviathan whose solar sails have not fully opened. It follows an elder into battle and vents focused stellar heat from an unshielded armored body."
            },
        }
    }
}

impl Combat for SpaceFauna {
    fn hull(&self) -> usize {
        match self {
            Self::AetherRay => 90,
            Self::IonWisp => 130,
            Self::VoidManta => 260,
            Self::VoidMantaCalf => 140,
            Self::CrystalLeviathan => 460,
            Self::CrystalShardling => 180,
            Self::Gravemaw => 700,
            Self::StarKraken => 900,
            Self::StarKrakenSpawn => 190,
            Self::NebulaGrazer => 520,
            Self::NebulaGrazerCalf => 210,
            Self::RiftSerpent => 1_100,
            Self::SolarRoc => 1_550,
            Self::ElderStarDragon => 3_200,
            Self::StarDragonWyrmling => 620,
        }
    }

    fn shield(&self) -> usize {
        0
    }

    fn damage(&self) -> usize {
        match self {
            Self::AetherRay => 12,
            Self::IonWisp => 22,
            Self::VoidManta => 38,
            Self::VoidMantaCalf => 18,
            Self::CrystalLeviathan => 58,
            Self::CrystalShardling => 24,
            Self::Gravemaw => 72,
            Self::StarKraken => 98,
            Self::StarKrakenSpawn => 30,
            Self::NebulaGrazer => 48,
            Self::NebulaGrazerCalf => 20,
            Self::RiftSerpent => 118,
            Self::SolarRoc => 155,
            Self::ElderStarDragon => 235,
            Self::StarDragonWyrmling => 78,
        }
    }

    fn rapid_fire(&self) -> HashMap<Unit, usize> {
        use crate::core::units::ships::Ship;

        match self {
            Self::AetherRay
            | Self::IonWisp
            | Self::VoidMantaCalf
            | Self::CrystalShardling
            | Self::StarKrakenSpawn
            | Self::NebulaGrazer
            | Self::NebulaGrazerCalf => HashMap::new(),
            Self::VoidManta | Self::CrystalLeviathan | Self::Gravemaw => {
                HashMap::from([(Unit::Ship(Ship::Probe), 70), (Unit::Ship(Ship::LightFighter), 35)])
            },
            Self::StarKraken | Self::RiftSerpent => HashMap::from([
                (Unit::Ship(Ship::Probe), 80),
                (Unit::Ship(Ship::LightFighter), 55),
                (Unit::Ship(Ship::HeavyFighter), 35),
            ]),
            Self::SolarRoc | Self::ElderStarDragon | Self::StarDragonWyrmling => HashMap::from([
                (Unit::Ship(Ship::Probe), 80),
                (Unit::Ship(Ship::LightFighter), 70),
                (Unit::Ship(Ship::HeavyFighter), 55),
                (Unit::Ship(Ship::Destroyer), 35),
            ]),
        }
    }
}

/// Builds a turn-scaled, deterministic creature group and its report title.
pub(crate) fn encounter_formation<R: Rng + ?Sized>(turn: usize, rng: &mut R) -> (String, Army) {
    use SpaceFauna::*;

    let mut army = Army::new();
    let mut add = |kind, count| {
        army.insert(Unit::Fauna(kind), count);
    };

    let name = match turn {
        0..=5 => match rng.random_range(0..4) {
            0 => {
                add(AetherRay, rng.random_range(1..=3));
                "Aether Ray Shoal"
            },
            1 => {
                add(IonWisp, rng.random_range(1..=2));
                "Ion Wisp Drift"
            },
            2 => {
                add(VoidManta, 1);
                "Lone Void Manta"
            },
            _ => {
                add(VoidManta, 1);
                add(VoidMantaCalf, 2);
                "Void Manta Nursery"
            },
        },
        6..=12 => match rng.random_range(0..6) {
            0 => {
                add(AetherRay, rng.random_range(3..=6));
                "Aether Ray Shoal"
            },
            1 => {
                add(IonWisp, rng.random_range(2..=4));
                "Ion Storm"
            },
            2 => {
                add(VoidManta, 1);
                add(VoidMantaCalf, rng.random_range(2..=3));
                "Void Manta Nursery"
            },
            3 => {
                add(NebulaGrazer, 1);
                add(NebulaGrazerCalf, rng.random_range(2..=3));
                "Nebula Grazer Family"
            },
            4 => {
                add(CrystalLeviathan, 1);
                "Crystal Leviathan"
            },
            _ => {
                add(CrystalLeviathan, 1);
                add(CrystalShardling, 3);
                "Crystal Leviathan Cluster"
            },
        },
        13..=20 => match rng.random_range(0..6) {
            0 => {
                add(CrystalLeviathan, 1);
                add(CrystalShardling, rng.random_range(3..=4));
                "Crystal Leviathan Cluster"
            },
            1 => {
                add(Gravemaw, 1);
                add(AetherRay, rng.random_range(2..=4));
                "Gravemaw Hunting Pack"
            },
            2 => {
                add(StarKraken, 1);
                add(StarKrakenSpawn, rng.random_range(3..=4));
                "Star Kraken Brood"
            },
            3 => {
                add(RiftSerpent, 1);
                "Rift Serpent"
            },
            4 => {
                add(NebulaGrazer, 1);
                add(NebulaGrazerCalf, rng.random_range(3..=4));
                "Nebula Grazer Migration"
            },
            _ => {
                add(StarDragonWyrmling, 1);
                "Rogue Star Dragon Wyrmling"
            },
        },
        _ => match rng.random_range(0..6) {
            0 => {
                add(SolarRoc, 1);
                add(IonWisp, 2);
                "Solar Roc Aerie"
            },
            1 => {
                add(ElderStarDragon, 1);
                add(StarDragonWyrmling, 3);
                "Star Dragon Brood"
            },
            2 => {
                add(StarKraken, 1);
                add(StarKrakenSpawn, 4);
                "Star Kraken Brood"
            },
            3 => {
                add(RiftSerpent, 1);
                add(CrystalLeviathan, 2);
                "Rift Serpent Procession"
            },
            4 => {
                add(VoidManta, 1);
                add(VoidMantaCalf, 3);
                add(Gravemaw, 1);
                "Void Manta Hunting Nursery"
            },
            _ => {
                add(SolarRoc, 1);
                add(NebulaGrazer, 1);
                add(NebulaGrazerCalf, 3);
                "Solar Roc Migration"
            },
        },
    };

    (name.to_string(), army)
}
