//! Persisted fleet preferences and composition-dependent tactical strengths.

use serde::{Deserialize, Serialize};

use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Combat, Price, Unit};

/// Orders shared by the units under one commander's control.
#[derive(Clone, Copy, Debug, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BattlePlan {
    /// Selects the highest-affinity tactic for this fleet until the commander overrides it.
    pub automatic_tactic: bool,
    /// Composition-dependent approach used against the opposing commander's tactic.
    pub tactic: CombatTactic,
    /// Preferred enemy category within the shooter's eligible target pool.
    pub target_priority: TargetPriority,
    /// Which mobile ships take additional exposure to protect their companions.
    pub protection: ProtectionPosture,
}

impl PartialEq for BattlePlan {
    fn eq(&self, other: &Self) -> bool {
        // The cached tactic is presentation data in automatic mode. A recalculation must
        // not invalidate an ally's accepted orders or create a command merely by hovering.
        self.automatic_tactic == other.automatic_tactic
            && (self.automatic_tactic || self.tactic == other.tactic)
            && self.target_priority == other.target_priority
            && self.protection == other.protection
    }
}

impl Serialize for BattlePlan {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut value = serializer.serialize_struct("BattlePlan", 4)?;
        value.serialize_field("automatic_tactic", &self.automatic_tactic)?;
        // Automatic intent has one canonical transport representation, so a local
        // affinity recalculation cannot invalidate an accepted allied contribution.
        value.serialize_field(
            "tactic",
            &if self.automatic_tactic {
                CombatTactic::default()
            } else {
                self.tactic
            },
        )?;
        value.serialize_field("target_priority", &self.target_priority)?;
        value.serialize_field("protection", &self.protection)?;
        value.end()
    }
}

impl Default for BattlePlan {
    fn default() -> Self {
        Self {
            automatic_tactic: true,
            tactic: CombatTactic::default(),
            target_priority: TargetPriority::default(),
            protection: ProtectionPosture::default(),
        }
    }
}

impl BattlePlan {
    /// Freezes the selected tactic for this battle while retaining targeting and protection.
    pub fn frozen(self, army: &Army) -> Self {
        Self {
            automatic_tactic: false,
            ..self.resolved(army)
        }
    }
    /// Resolves automatic selection from the actual force, preserving all manual orders.
    /// Ties prefer Breakthrough, then the stable tactic order. Empty forces use Breakthrough.
    pub fn resolved(mut self, army: &Army) -> Self {
        if self.automatic_tactic {
            let mut best = Self {
                automatic_tactic: false,
                tactic: CombatTactic::default(),
                ..self
            };
            let mut score = best.suitability(army);
            for tactic in CombatTactic::ALL {
                let candidate = Self {
                    tactic,
                    ..best
                };
                let fit = candidate.suitability(army);
                if fit > score {
                    best = candidate;
                    score = fit;
                }
            }
            self.tactic = best.tactic;
        }
        self
    }
    /// Resource-cost-weighted suitability, from zero to one hundred percent.
    /// Unarmed units, buildings, missiles, and fauna do not contribute.
    pub fn suitability(&self, army: &Army) -> u8 {
        let (fit, total) = army.iter().fold((0_u128, 0_u128), |(fit, total), (unit, count)| {
            let weight = combat_value(*unit).saturating_mul(*count as u128);
            (
                fit.saturating_add(weight.saturating_mul(self.tactic.affinity(*unit) as u128)),
                total.saturating_add(weight),
            )
        });
        fit.checked_div(total).unwrap_or(0).min(100) as u8
    }
}

/// Six equally available tactics, each countering two and countered by two others.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[repr(usize)]
pub enum CombatTactic {
    /// Numerous light combat ships overwhelm heavier deployments.
    Swarm,
    /// Mobile medium ships disrupt slow or stationary formations.
    Skirmish,
    /// A concentrated cruiser and battleship assault.
    #[default]
    Breakthrough,
    /// Heavy siege vessels concentrate sustained fire.
    Siege,
    /// Destroyers, battleships, and light batteries intercept mobile attackers.
    Interception,
    /// Heavy batteries and durable capital ships anchor the engagement.
    Bastion,
}

impl CombatTactic {
    /// Runtime artwork key shared by fleet controls and combat identities.
    pub fn image_key(self) -> &'static str {
        match self {
            Self::Swarm => "tactic swarm",
            Self::Skirmish => "tactic skirmish",
            Self::Breakthrough => "tactic breakthrough",
            Self::Siege => "tactic siege",
            Self::Interception => "tactic interception",
            Self::Bastion => "tactic bastion",
        }
    }

    /// Stable presentation order for every tactic.
    pub const ALL: [Self; 6] = [
        Self::Swarm,
        Self::Skirmish,
        Self::Breakthrough,
        Self::Siege,
        Self::Interception,
        Self::Bastion,
    ];

    /// Short player-facing label.
    pub fn name(self) -> &'static str {
        match self {
            Self::Swarm => "Swarm",
            Self::Skirmish => "Skirmish",
            Self::Breakthrough => "Breakthrough",
            Self::Siege => "Siege",
            Self::Interception => "Interception",
            Self::Bastion => "Bastion",
        }
    }

    /// Explains composition and counter choices without implying a guaranteed bonus.
    pub fn description(self) -> &'static str {
        match self {
            Self::Swarm => "Light and Heavy Fighters. Counters Breakthrough and Siege; countered by Interception and Bastion.",
            Self::Skirmish => "Destroyers and Cruisers. Counters Siege and Bastion; countered by Breakthrough and Interception.",
            Self::Breakthrough => "Cruisers and Battleships. Counters Skirmish and Interception; countered by Swarm and Siege.",
            Self::Siege => "Bombers, Dreadnoughts, and War Suns. Counters Breakthrough and Bastion; countered by Swarm and Skirmish.",
            Self::Interception => "Destroyers, Battleships, and light batteries. Counters Swarm and Skirmish; countered by Breakthrough and Bastion.",
            Self::Bastion => "Dreadnoughts, heavy batteries, and Space Docks. Counters Swarm and Interception; countered by Skirmish and Siege.",
        }
    }

    /// Whether this tactic earns a composition-scaled weapon bonus against the other.
    pub fn counters(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::Swarm, Self::Breakthrough | Self::Siege)
                | (Self::Skirmish, Self::Siege | Self::Bastion)
                | (Self::Breakthrough, Self::Skirmish | Self::Interception)
                | (Self::Siege, Self::Breakthrough | Self::Bastion)
                | (Self::Interception, Self::Swarm | Self::Skirmish)
                | (Self::Bastion, Self::Swarm | Self::Interception)
        )
    }

    /// Suitability of one unit, as a percentage. Columns follow `ALL` order.
    /// Partial fits let mixed fleets adapt without making every choice equally effective.
    pub fn affinity(self, unit: Unit) -> u8 {
        let fits = match unit {
            Unit::Ship(Ship::LightFighter) => [100, 50, 25, 0, 50, 25],
            Unit::Ship(Ship::HeavyFighter) => [100, 50, 50, 25, 50, 25],
            Unit::Ship(Ship::Destroyer) => [50, 100, 50, 25, 100, 25],
            Unit::Ship(Ship::Cruiser) => [25, 100, 100, 50, 75, 50],
            Unit::Ship(Ship::Bomber) => [0, 25, 50, 100, 25, 50],
            Unit::Ship(Ship::Battleship) => [25, 50, 100, 75, 100, 75],
            Unit::Ship(Ship::Dreadnought) => [0, 25, 75, 100, 50, 100],
            Unit::Ship(Ship::WarSun) => [0, 0, 75, 100, 50, 75],
            Unit::Defense(Defense::RocketLauncher | Defense::LightLaser) => {
                [50, 0, 25, 25, 100, 50]
            },
            Unit::Defense(Defense::HeavyLaser) => [25, 0, 50, 50, 75, 100],
            Unit::Defense(Defense::GaussCannon | Defense::IonCannon | Defense::PlasmaTurret) => {
                [0, 0, 50, 75, 50, 100]
            },
            Unit::Defense(Defense::SpaceDock) => [0, 0, 50, 75, 75, 100],
            Unit::Ship(Ship::Probe | Ship::ColonyShip)
            | Unit::Defense(
                Defense::Crawler
                | Defense::RepairTruck
                | Defense::AntiballisticMissile
                | Defense::InterplanetaryMissile,
            )
            | Unit::Building(_)
            | Unit::Fauna(_) => [0; 6],
        };
        fits[self as usize]
    }
}

/// Preferred target category; preferences bias probability rather than force a target.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum TargetPriority {
    /// Existing ship-first and bomber-defense-first behavior.
    #[default]
    Standard,
    /// Prefer armed fighters.
    SmallShips,
    /// Prefer destroyers, cruisers, and bombers.
    MediumShips,
    /// Prefer battleships, dreadnoughts, war suns, and orbital space docks.
    CapitalShips,
    /// Prefer ground defenses while retaining other possible targets.
    Defenses,
}

impl TargetPriority {
    /// Runtime artwork key shared by fleet controls and combat identities.
    pub fn image_key(self) -> &'static str {
        match self {
            Self::Standard => "target standard",
            Self::SmallShips => "target small ships",
            Self::MediumShips => "target medium ships",
            Self::CapitalShips => "target capital ships",
            Self::Defenses => "target defenses",
        }
    }

    /// Stable presentation order for every target preference.
    pub const ALL: [Self; 5] =
        [Self::Standard, Self::SmallShips, Self::MediumShips, Self::CapitalShips, Self::Defenses];

    /// Short player-facing label.
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Balanced fire",
            Self::SmallShips => "Small ships",
            Self::MediumShips => "Medium ships",
            Self::CapitalShips => "Capital ships",
            Self::Defenses => "Defenses",
        }
    }

    /// Explains the eligible categories affected by this preference.
    pub fn description(self) -> &'static str {
        match self {
            Self::Standard => "Normal target selection. Ships engage ships and Space Docks first; Bombers engage defenses first.",
            Self::SmallShips => "Twice the target weight for Light and Heavy Fighters in the ship pool. Bombers retain their defense role.",
            Self::MediumShips => "Twice the target weight for Destroyers, Cruisers, and Bombers in the ship pool. Bombers retain their defense role.",
            Self::CapitalShips => "Twice the target weight for Battleships, Dreadnoughts, War Suns, and Space Docks. Bombers retain their defense role.",
            Self::Defenses => "Ground defenses receive twice the target weight. Other ships may engage defenses before enemy ships are cleared; planetary shields still protect them.",
        }
    }

    pub(crate) fn prefers(self, unit: Unit) -> bool {
        match self {
            Self::Standard => false,
            Self::SmallShips => matches!(unit, Unit::Ship(Ship::LightFighter | Ship::HeavyFighter)),
            Self::MediumShips => {
                matches!(unit, Unit::Ship(Ship::Destroyer | Ship::Cruiser | Ship::Bomber))
            },
            Self::CapitalShips => is_mobile_capital(unit) || unit == Unit::space_dock(),
            Self::Defenses => unit.is_defense() && !unit.is_missile() && unit != Unit::space_dock(),
        }
    }
}

/// Exposure redistribution among mobile ships belonging to the same commander.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProtectionPosture {
    /// No additional exposure redistribution.
    #[default]
    Standard,
    /// Fighters and destroyers screen mobile capitals.
    ScreenCapitals,
    /// Mobile capitals cover smaller armed ships.
    HeavyCover,
    /// Fighters, destroyers, and cruisers escort bombers.
    ProtectBombers,
}

impl ProtectionPosture {
    /// Runtime artwork key shared by fleet controls and combat identities.
    pub fn image_key(self) -> &'static str {
        match self {
            Self::Standard => "protection standard",
            Self::ScreenCapitals => "protection screen capitals",
            Self::HeavyCover => "protection heavy cover",
            Self::ProtectBombers => "protection protect bombers",
        }
    }

    /// Stable presentation order for every protection preference.
    pub const ALL: [Self; 4] =
        [Self::Standard, Self::ScreenCapitals, Self::HeavyCover, Self::ProtectBombers];

    /// Short player-facing label.
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Open formation",
            Self::ScreenCapitals => "Screen capitals",
            Self::HeavyCover => "Heavy cover",
            Self::ProtectBombers => "Protect bombers",
        }
    }

    /// Explains the ships accepting risk and the bounded benefit they can provide.
    pub fn description(self) -> &'static str {
        match self {
            Self::Standard => "No exposure changes.",
            Self::ScreenCapitals => "Fighters and Destroyers take extra fire to reduce mobile capital exposure by up to 25%, scaled by surviving screen strength.",
            Self::HeavyCover => "Battleships, Dreadnoughts, and War Suns take extra fire to reduce smaller armed ship exposure by up to 25%, scaled by surviving cover strength.",
            Self::ProtectBombers => "Fighters, Destroyers, and Cruisers take extra fire to reduce Bomber exposure by up to 25%, scaled by surviving escort strength.",
        }
    }

    pub(crate) fn protects(self, unit: Unit) -> bool {
        match self {
            Self::Standard => false,
            Self::ScreenCapitals => is_mobile_capital(unit),
            Self::HeavyCover => matches!(
                unit,
                Unit::Ship(
                    Ship::LightFighter
                        | Ship::HeavyFighter
                        | Ship::Destroyer
                        | Ship::Cruiser
                        | Ship::Bomber
                )
            ),
            Self::ProtectBombers => unit == Unit::Ship(Ship::Bomber),
        }
    }

    pub(crate) fn covers(self, unit: Unit) -> bool {
        match self {
            Self::Standard => false,
            Self::ScreenCapitals => matches!(
                unit,
                Unit::Ship(Ship::LightFighter | Ship::HeavyFighter | Ship::Destroyer)
            ),
            Self::HeavyCover => is_mobile_capital(unit),
            Self::ProtectBombers => matches!(
                unit,
                Unit::Ship(
                    Ship::LightFighter | Ship::HeavyFighter | Ship::Destroyer | Ship::Cruiser
                )
            ),
        }
    }
}

fn is_mobile_capital(unit: Unit) -> bool {
    matches!(unit, Unit::Ship(Ship::Battleship | Ship::Dreadnought | Ship::WarSun))
}

/// Resource value used consistently for composition fit and available covering strength.
pub(crate) fn combat_value(unit: Unit) -> u128 {
    if unit.damage() == 0 || unit.is_missile() || matches!(unit, Unit::Fauna(_) | Unit::Building(_))
    {
        return 0;
    }
    let price = unit.price();
    price.metal as u128 + price.crystal as u128 + price.deuterium as u128
}
