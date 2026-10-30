//! Persisted fleet preferences and composition-dependent tactical strengths.

use serde::{Deserialize, Serialize};

use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Combat, Unit};

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
    /// Which combat ship class draws extra fire away from the commander's other ships.
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
    /// Suitability weighted by unit count times production level, from zero to one hundred.
    /// Unarmed units, buildings, missiles, and fauna do not contribute.
    pub fn suitability(&self, army: &Army) -> u8 {
        let (fit, total) = army.iter().fold((0_u128, 0_u128), |(fit, total), (unit, count)| {
            let weight = affinity_weight(*unit).saturating_mul(*count as u128);
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

    /// Weapon damage bonus in hundredths of a percent for the combat-start fleet affinity.
    /// Only favorable matchups earn extra damage; being countered never penalizes damage.
    pub fn damage_bonus_basis_points(self, opponent: Self, affinity: u8) -> u16 {
        if self.counters(opponent) {
            15 * u16::from(affinity.min(100))
        } else {
            0
        }
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
    /// Equal target weights; bombers retain their defense-first role.
    #[default]
    Standard,
    /// Prefer fighters, destroyers, unarmed ground support, rocket launchers, and light lasers.
    LightTargets,
    /// Prefer cruisers, bombers, battleships, heavy lasers, and gauss cannons.
    MediumTargets,
    /// Prefer dreadnoughts, war suns, space docks, ion cannons, and plasma turrets.
    HeavyTargets,
}

impl TargetPriority {
    /// Runtime artwork key shared by fleet controls and combat identities.
    pub fn image_key(self) -> &'static str {
        match self {
            Self::Standard => "target standard",
            Self::LightTargets => "target small ships",
            Self::MediumTargets => "target medium ships",
            Self::HeavyTargets => "target capital ships",
        }
    }

    /// Stable presentation order for every target preference.
    pub const ALL: [Self; 4] =
        [Self::Standard, Self::LightTargets, Self::MediumTargets, Self::HeavyTargets];

    /// Short player-facing label.
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Balanced fire",
            Self::LightTargets => "Light targets",
            Self::MediumTargets => "Medium targets",
            Self::HeavyTargets => "Heavy targets",
        }
    }

    /// Explains the eligible categories affected by this preference.
    pub fn description(self) -> &'static str {
        match self {
            Self::Standard => "Random target selection.",
            Self::LightTargets => "Twice the target weight for Light Fighters, Heavy Fighters, Destroyers, Crawlers, Repair Trucks, Rocket Launchers, and Light Lasers.",
            Self::MediumTargets => "Twice the target weight for Cruisers, Bombers, Battleships, Heavy Lasers, and Gauss Cannons.",
            Self::HeavyTargets => "Twice the target weight for Dreadnoughts, War Suns, Space Docks, Ion Cannons, and Plasma Turrets.",
        }
    }

    pub(crate) fn prefers(self, unit: Unit) -> bool {
        match unit {
            Unit::Defense(
                Defense::Crawler
                | Defense::RepairTruck
                | Defense::RocketLauncher
                | Defense::LightLaser,
            ) => self == Self::LightTargets,
            Unit::Defense(Defense::HeavyLaser | Defense::GaussCannon) => {
                self == Self::MediumTargets
            },
            Unit::Defense(Defense::IonCannon | Defense::PlasmaTurret) => self == Self::HeavyTargets,
            _ => ship_target_class(unit) == Some(self),
        }
    }
}

/// Formations redistribute fire among one commander's combat ships and Space Docks.
/// Ground defenses, unarmed ships, and other commanders keep their original target weight.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ProtectionPosture {
    /// No additional exposure redistribution.
    #[default]
    Standard,
    /// Fighters and destroyers draw fire away from the other combat ship classes.
    Vanguard,
    /// Cruisers, bombers, and battleships draw fire away from the other combat ship classes.
    BattleLine,
    /// Dreadnoughts, war suns, and space docks draw fire away from the other combat ship classes.
    Bulwark,
}

impl ProtectionPosture {
    /// Runtime artwork key shared by fleet controls and combat identities.
    pub fn image_key(self) -> &'static str {
        match self {
            Self::Standard => "protection standard",
            Self::Vanguard => "protection screen capitals",
            Self::BattleLine => "protection protect bombers",
            Self::Bulwark => "protection heavy cover",
        }
    }

    /// Stable presentation order for every protection preference.
    pub const ALL: [Self; 4] = [Self::Standard, Self::Vanguard, Self::BattleLine, Self::Bulwark];

    /// Short player-facing label.
    pub fn name(self) -> &'static str {
        match self {
            Self::Standard => "Open formation",
            Self::Vanguard => "Light screen",
            Self::BattleLine => "Battle line",
            Self::Bulwark => "Heavy bulwark",
        }
    }

    /// Explains which ships draw extra fire, without resource-cost or tactic-affinity scaling.
    pub fn description(self) -> &'static str {
        match self {
            Self::Standard => "No exposure changes.",
            Self::Vanguard => {
                "Light Fighters, Heavy Fighters and Destroyers gain +25% target weight."
            },
            Self::BattleLine => "Cruisers, Bombers and Battleships gain +25% target weight.",
            Self::Bulwark => "Dreadnoughts, War Suns and Space Docks gain +25% target weight.",
        }
    }

    pub(crate) fn affects(unit: Unit) -> bool {
        ship_target_class(unit).is_some()
    }

    pub(crate) fn draws_fire(self, unit: Unit) -> bool {
        let class = match self {
            Self::Standard => return false,
            Self::Vanguard => TargetPriority::LightTargets,
            Self::BattleLine => TargetPriority::MediumTargets,
            Self::Bulwark => TargetPriority::HeavyTargets,
        };
        ship_target_class(unit) == Some(class)
    }
}

/// Ship class boundaries stay identical for outgoing priorities and incoming fire formations.
fn ship_target_class(unit: Unit) -> Option<TargetPriority> {
    match unit {
        Unit::Ship(Ship::LightFighter | Ship::HeavyFighter | Ship::Destroyer) => {
            Some(TargetPriority::LightTargets)
        },
        Unit::Ship(Ship::Cruiser | Ship::Bomber | Ship::Battleship) => {
            Some(TargetPriority::MediumTargets)
        },
        Unit::Ship(Ship::Dreadnought | Ship::WarSun) | Unit::Defense(Defense::SpaceDock) => {
            Some(TargetPriority::HeavyTargets)
        },
        _ => None,
    }
}

/// Production-level weight for tactic affinity; ineligible units contribute nothing.
pub(crate) fn affinity_weight(unit: Unit) -> u128 {
    if unit.damage() == 0 || unit.is_missile() || matches!(unit, Unit::Fauna(_) | Unit::Building(_))
    {
        return 0;
    }
    unit.production() as u128
}
