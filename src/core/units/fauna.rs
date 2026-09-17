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
    /// A slow-charging white-violet beam that annihilates all but the heaviest ships.
    ExtinctionRay,
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
    /// Colossal solitary hunter that collapses prey with a focused null-energy beam.
    NullstarBehemoth,
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
            Self::NullstarBehemoth => 7,
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
            Self::NullstarBehemoth => FaunaAttack::ExtinctionRay,
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
            Self::NullstarBehemoth => {
                "A moon-sized solitary hunter sheathed in obsidian plates. Its maw compresses null energy into an Extinction Ray that destroys any conventional ship in one hit; even a War Sun can endure only the first strike."
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
            Self::NullstarBehemoth => 6_000,
        }
    }

    fn shield(&self) -> usize {
        0
    }

    fn damage(&self) -> usize {
        match self {
            Self::AetherRay => 25,
            Self::IonWisp => 40,
            Self::VoidManta => 70,
            Self::VoidMantaCalf => 35,
            Self::CrystalLeviathan => 110,
            Self::CrystalShardling => 50,
            Self::Gravemaw => 160,
            Self::StarKraken => 220,
            Self::StarKrakenSpawn => 65,
            Self::NebulaGrazer => 100,
            Self::NebulaGrazerCalf => 45,
            Self::RiftSerpent => 260,
            Self::SolarRoc => 350,
            Self::ElderStarDragon => 500,
            Self::StarDragonWyrmling => 180,
            Self::NullstarBehemoth => 800,
        }
    }

    fn rapid_fire(&self) -> HashMap<Unit, usize> {
        HashMap::new()
    }
}

/// Relative formation cost based on the larger of a creature's hull and damage compared with a
/// Light Fighter. Apex creatures consume most of the capped budget so late encounters can include
/// their related fauna without multiplying the apex itself.
pub(crate) const fn encounter_threat(creature: SpaceFauna) -> usize {
    use SpaceFauna::*;

    match creature {
        AetherRay => 2,
        IonWisp | VoidMantaCalf => 3,
        CrystalShardling | StarKrakenSpawn => 4,
        NebulaGrazerCalf => 5,
        VoidManta => 6,
        CrystalLeviathan => 10,
        NebulaGrazer => 10,
        StarDragonWyrmling => 10,
        Gravemaw => 14,
        StarKraken => 18,
        RiftSerpent => 22,
        SolarRoc | ElderStarDragon => 30,
        NullstarBehemoth => 50,
    }
}

/// Returns the encounter threat budget for a turn.
///
/// The opening four turns deliberately stay gentle. Strength rises quickly through turn ten,
/// reaches its former late-game level at turn fifty, then continues climbing gradually to a higher
/// permanent cap at turn one hundred. Turn zero is treated as turn one so setup and first-turn
/// previews use the same opening formation strength.
pub(crate) const fn encounter_threat_budget(turn: usize) -> usize {
    let turn = if turn == 0 {
        1
    } else {
        turn
    };
    let uncapped = if turn <= 4 {
        turn.saturating_add(1)
    } else if turn <= 10 {
        5_usize.saturating_add((turn - 4).saturating_mul(3))
    } else if turn <= 50 {
        23_usize.saturating_add((turn - 10).saturating_mul(27) / 40)
    } else {
        50_usize.saturating_add((turn - 50).saturating_mul(20) / 50)
    };
    if uncapped > 70 {
        70
    } else {
        uncapped
    }
}

fn encounter_name(reinforcement_pattern: &[SpaceFauna], army: &Army) -> String {
    use SpaceFauna::*;

    let total = army.values().copied().sum::<usize>();
    let primary = reinforcement_pattern
        .iter()
        .copied()
        .find(|creature| army.contains_key(&Unit::Fauna(*creature)))
        .or_else(|| {
            army.keys().find_map(|unit| match unit {
                Unit::Fauna(creature) => Some(*creature),
                _ => None,
            })
        });
    let Some(primary) = primary else {
        return "Deep-Space Encounter".to_string();
    };
    let primary_name = Unit::Fauna(primary).to_name();
    if total == 1 {
        return format!("Lone {primary_name}");
    }

    let amount = |creature| army.get(&Unit::Fauna(creature)).copied().unwrap_or(0);
    let related = |adult, young| amount(adult).saturating_add(amount(young));
    match primary {
        AetherRay => {
            if total <= 4 {
                "Aether Ray Shoal".to_string()
            } else {
                "Aether Ray Swarm".to_string()
            }
        },
        IonWisp => {
            if total <= 4 {
                "Ion Wisp Drift".to_string()
            } else {
                "Ion Storm".to_string()
            }
        },
        VoidManta | VoidMantaCalf => "Void Manta Nursery".to_string(),
        CrystalLeviathan | CrystalShardling => "Crystal Leviathan Cluster".to_string(),
        StarKraken | StarKrakenSpawn => "Star Kraken Brood".to_string(),
        NebulaGrazer | NebulaGrazerCalf => {
            let grazers = related(NebulaGrazer, NebulaGrazerCalf);
            if grazers == 1 {
                "Nebula Grazer Migration".to_string()
            } else if grazers <= 3 {
                "Nebula Grazer Family".to_string()
            } else if grazers <= 6 {
                "Nebula Grazer Herd".to_string()
            } else {
                "Nebula Grazer Migration".to_string()
            }
        },
        ElderStarDragon => "Star Dragon Brood".to_string(),
        StarDragonWyrmling => "Rogue Star Dragon Wyrmling".to_string(),
        Gravemaw => "Gravemaw Hunting Pack".to_string(),
        RiftSerpent => "Rift Serpent Procession".to_string(),
        SolarRoc => "Solar Roc Aerie".to_string(),
        NullstarBehemoth => "Lone Nullstar Behemoth".to_string(),
    }
}

/// Builds a turn-scaled, deterministic creature group and its report title.
pub(crate) fn encounter_formation<R: Rng + ?Sized>(turn: usize, rng: &mut R) -> (String, Army) {
    use SpaceFauna::*;

    let target = encounter_threat_budget(turn);
    let candidates: &[&[SpaceFauna]] = match turn {
        0..=4 => &[&[AetherRay], &[IonWisp, AetherRay]],
        5..=6 => &[&[AetherRay], &[IonWisp, AetherRay], &[VoidManta, VoidMantaCalf, AetherRay]],
        7..=11 => &[
            &[AetherRay],
            &[IonWisp, AetherRay],
            &[VoidManta, VoidMantaCalf, AetherRay],
            &[NebulaGrazer, NebulaGrazerCalf, AetherRay],
            &[CrystalLeviathan, CrystalShardling, AetherRay],
            &[StarKraken, StarKrakenSpawn, AetherRay],
        ],
        12..=20 => &[
            &[CrystalLeviathan, CrystalShardling, AetherRay],
            &[Gravemaw, AetherRay],
            &[StarKraken, StarKrakenSpawn, AetherRay],
            &[RiftSerpent, CrystalShardling, AetherRay],
            &[NebulaGrazer, NebulaGrazerCalf, AetherRay],
            &[StarDragonWyrmling, AetherRay],
        ],
        21..=49 => &[
            &[SolarRoc, IonWisp, AetherRay],
            &[ElderStarDragon, StarDragonWyrmling, StarDragonWyrmling, AetherRay],
            &[StarKraken, StarKrakenSpawn, AetherRay],
            &[RiftSerpent, CrystalLeviathan, AetherRay],
            &[Gravemaw, VoidManta, VoidMantaCalf, AetherRay],
            &[NebulaGrazer, NebulaGrazerCalf, AetherRay],
        ],
        _ => &[
            &[NullstarBehemoth],
            &[SolarRoc, IonWisp, AetherRay],
            &[ElderStarDragon, StarDragonWyrmling, StarDragonWyrmling, AetherRay],
            &[StarKraken, StarKrakenSpawn, AetherRay],
            &[RiftSerpent, CrystalLeviathan, AetherRay],
            &[Gravemaw, VoidManta, VoidMantaCalf, AetherRay],
            &[NebulaGrazer, NebulaGrazerCalf, AetherRay],
        ],
    };

    let eligible = candidates
        .iter()
        .copied()
        .filter(|pattern| encounter_threat(pattern[0]) <= target)
        .collect::<Vec<_>>();
    let selected = rng.random_range(0..eligible.len());
    let reinforcement_pattern = eligible[selected];
    if reinforcement_pattern == [NullstarBehemoth] {
        return (
            "Lone Nullstar Behemoth".to_string(),
            Army::from([(Unit::Fauna(NullstarBehemoth), 1)]),
        );
    }
    let mut threat = 0_usize;
    let mut army = Army::new();
    while threat < target {
        let threat_before_reinforcements = threat;
        for creature in reinforcement_pattern {
            let creature_threat = encounter_threat(*creature);
            let remaining = target - threat;
            if creature_threat > remaining || remaining.saturating_sub(creature_threat) == 1 {
                continue;
            }
            let unit = Unit::Fauna(*creature);
            army.entry(unit).and_modify(|count| *count = count.saturating_add(1)).or_insert(1);
            threat = threat.saturating_add(creature_threat);
            if threat >= target {
                break;
            }
        }
        if threat == threat_before_reinforcements {
            debug_assert_eq!(target - threat, 3);
            army.entry(Unit::Fauna(IonWisp))
                .and_modify(|count| *count = count.saturating_add(1))
                .or_insert(1);
            threat = threat.saturating_add(encounter_threat(IonWisp));
        }
    }

    (encounter_name(reinforcement_pattern, &army), army)
}
