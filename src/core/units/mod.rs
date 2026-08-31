//! Unified unit model, stable army storage, and unit-category helpers.

use std::collections::{BTreeMap, HashMap};

use itertools::Itertools;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use strum::IntoEnumIterator;

use crate::core::combat::stats::CombatStats;
use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::utils::NameFromEnum;

pub mod buildings;
pub mod defense;
pub mod ships;

/// Provides user-facing explanatory text for unit kinds.
pub trait Description {
    /// Returns the user-facing description of this gameplay value.
    fn description(&self) -> &str;
}

/// Provides construction cost data for unit kinds.
pub trait Price {
    /// Returns the resource cost of producing this unit.
    fn price(&self) -> Resources;
}

/// Provides combat statistics for unit kinds.
pub trait Combat {
    /// Returns this unit type's base hull strength.
    fn hull(&self) -> usize;
    /// Returns this unit type's base shield strength.
    fn shield(&self) -> usize;
    /// Returns this unit type's base weapon damage.
    fn damage(&self) -> usize;
    /// Returns rapid-fire probabilities keyed by target unit.
    fn rapid_fire(&self) -> HashMap<Unit, usize> {
        HashMap::new()
    }
    /// Returns the movement or animation speed represented by this value.
    fn speed(&self) -> f32 {
        0.
    }
    /// Returns the deuterium consumed for the supplied movement distance.
    fn fuel_consumption(&self) -> usize {
        0
    }
}

/// Deterministically ordered collection of unit counts.
pub type Army = BTreeMap<Unit, usize>;

/// Queries army counts and aggregate production values.
pub trait Amount {
    /// Returns the stored count for the requested unit, or zero when absent.
    fn amount(&self, unit: &Unit) -> usize;
    /// Returns whether this value has army.
    fn has_army(&self) -> bool;
    /// Returns the summed production value of every unit in the army.
    fn total_production(&self) -> usize;
}

impl Amount for Army {
    /// Returns the stored count for the requested unit, or zero when absent.
    fn amount(&self, unit: &Unit) -> usize {
        *self.get(unit).unwrap_or(&0)
    }
    /// Returns whether this value has army.
    fn has_army(&self) -> bool {
        self.iter().any(|(_, c)| *c > 0)
    }
    /// Returns the summed production value of every unit in the army.
    fn total_production(&self) -> usize {
        self.iter()
            .filter_map(|(unit, count)| {
                (*count > 0).then_some(unit.production().saturating_mul(*count))
            })
            .fold(0, usize::saturating_add)
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
/// Stable tagged union of every constructible building, ship, and defense.
pub enum Unit {
    /// The building value.
    Building(Building),
    /// The ship value.
    Ship(Ship),
    /// The defense value.
    Defense(Defense),
}

impl Serialize for Unit {
    /// Serializes units as stable enum-path strings so they are valid JSON map keys.
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&format!("{self:?}"))
    }
}

impl<'de> Deserialize<'de> for Unit {
    /// Parses the stable enum-path representation used by persisted armies.
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = String::deserialize(deserializer)?;
        Self::all()
            .into_iter()
            .flatten()
            .find(|unit| format!("{unit:?}") == encoded)
            .ok_or_else(|| D::Error::custom(format!("unknown unit identifier: {encoded}")))
    }
}

impl Unit {
    /// Returns every building unit kind.
    pub fn buildings() -> Vec<Self> {
        Building::iter().map(Unit::Building).collect()
    }

    /// Returns every ship unit kind.
    pub fn ships() -> Vec<Self> {
        Ship::iter().map(Unit::Ship).collect()
    }

    /// Returns every defense unit kind.
    pub fn defenses() -> Vec<Self> {
        Defense::iter().map(Unit::Defense).collect()
    }

    /// Returns every value in this unit category.
    pub fn all() -> Vec<Vec<Self>> {
        vec![Self::buildings(), Self::ships(), Self::defenses()]
    }

    /// Returns units valid for the supplied planet and ownership context.
    pub fn all_valid(is_moon: bool) -> Vec<Vec<Self>> {
        if !is_moon {
            vec![
                Self::buildings()
                    .into_iter()
                    .filter(|u| {
                        !Self::lunar_buildings()
                            .iter()
                            .filter(|u| **u != Unit::Building(Building::Shipyard))
                            .contains(u)
                    })
                    .collect(),
                Self::ships(),
                Self::defenses(),
            ]
        } else {
            vec![Self::lunar_buildings(), Self::ships()]
        }
    }

    /// Returns all combat units in deterministic firing order.
    pub fn all_firing_order() -> Vec<Self> {
        Unit::ships()
            .into_iter()
            .chain(std::iter::once(Unit::space_dock()))
            .chain(
                Unit::defenses()
                    .into_iter()
                    .filter(|u| *u != Unit::crawler() && *u != Unit::space_dock()),
            )
            .collect()
    }

    /// Computes resource buildings for the current owned worlds.
    pub fn resource_buildings() -> Vec<Self> {
        vec![
            Unit::Building(Building::MetalMine),
            Unit::Building(Building::CrystalMine),
            Unit::Building(Building::DeuteriumSynthesizer),
        ]
    }

    /// Returns buildings that increase unit production capacity.
    pub fn industrial_buildings() -> Vec<Self> {
        vec![
            Unit::Building(Building::Shipyard),
            Unit::Building(Building::Factory),
            Unit::Building(Building::MissileSilo),
        ]
    }

    /// Returns structures that may be constructed on controlled moons.
    pub fn lunar_buildings() -> Vec<Self> {
        vec![
            Unit::Building(Building::LunarBase),
            Unit::Building(Building::DemolitionNexus),
            Unit::Building(Building::Shipyard),
            Unit::Building(Building::Laboratory),
            Unit::Building(Building::OrbitalRadar),
        ]
    }

    /// Returns the canonical planetary-shield unit key.
    pub fn planetary_shield() -> Self {
        Unit::Building(Building::PlanetaryShield)
    }

    /// Returns the canonical espionage-probe unit key.
    pub fn probe() -> Self {
        Unit::Ship(Ship::Probe)
    }

    /// Returns the canonical colony-ship unit key.
    pub fn colony_ship() -> Self {
        Unit::Ship(Ship::ColonyShip)
    }
    /// Returns the canonical War Sun unit key.
    pub fn war_sun() -> Self {
        Unit::Ship(Ship::WarSun)
    }

    /// Returns the canonical repair-crawler unit key.
    pub fn crawler() -> Self {
        Unit::Defense(Defense::Crawler)
    }

    /// Returns the canonical space-dock unit key.
    pub fn space_dock() -> Self {
        Unit::Defense(Defense::SpaceDock)
    }

    /// Returns the canonical antiballistic-missile unit key.
    pub fn antiballistic_missile() -> Self {
        Unit::Defense(Defense::AntiballisticMissile)
    }

    /// Returns the canonical interplanetary-missile unit key.
    pub fn interplanetary_missile() -> Self {
        Unit::Defense(Defense::InterplanetaryMissile)
    }

    /// Returns whether this value building.
    pub fn is_building(&self) -> bool {
        matches!(self, Unit::Building(_))
    }

    /// Returns whether this value ship.
    pub fn is_ship(&self) -> bool {
        matches!(self, Unit::Ship(_))
    }

    /// Returns whether this value defense.
    pub fn is_defense(&self) -> bool {
        matches!(self, Unit::Defense(_))
    }

    /// Returns whether this value turret.
    pub fn is_turret(&self) -> bool {
        matches!(self, Unit::Defense(d) if *d != Defense::Crawler && *d != Defense::SpaceDock && !d.is_missile())
    }

    /// Returns whether this value missile.
    pub fn is_missile(&self) -> bool {
        matches!(self, Unit::Defense(d) if d.is_missile())
    }

    /// Returns whether constructing this unit consumes a planetary or lunar field.
    pub fn consumes_field(&self) -> bool {
        self.is_building()
            && !matches!(
                self,
                Unit::Building(Building::LunarBase) | Unit::Building(Building::DemolitionNexus)
            )
    }

    /// Returns whether this value economic building.
    pub fn is_economic_building(&self) -> bool {
        Self::resource_buildings().contains(self)
    }

    /// Returns whether this value industrial building.
    pub fn is_industrial_building(&self) -> bool {
        Self::industrial_buildings().contains(self)
    }

    /// Returns whether this value combat ship.
    pub fn is_combat_ship(&self) -> bool {
        matches!(self, Unit::Ship(s) if !matches!(s, Ship::Probe | Ship::ColonyShip))
    }

    /// Returns the production-time/value score shared by economy and combat ordering.
    pub fn production(&self) -> usize {
        match self {
            Unit::Building(_) => 1,
            Unit::Ship(s) => s.production(),
            Unit::Defense(d) => d.production(),
        }
    }

    /// Returns the selected combat statistic as a displayable numeric value.
    pub fn get_stat(&self, stat: &CombatStats) -> String {
        let n = match stat {
            CombatStats::Hull => self.hull() as f32,
            CombatStats::Shield => self.shield() as f32,
            CombatStats::Damage => self.damage() as f32,
            CombatStats::Production => self.production() as f32,
            CombatStats::Speed => self.speed(),
            CombatStats::FuelConsumption => self.fuel_consumption() as f32,
            CombatStats::RapidFire => self.rapid_fire().values().sum::<usize>() as f32,
        };

        if n == 0. {
            "---".to_string()
        } else {
            n.to_string()
        }
    }

    /// Returns the human-readable display name.
    pub fn to_name(&self) -> String {
        match self {
            Unit::Building(b) => b.to_name(),
            Unit::Ship(s) => s.to_name(),
            Unit::Defense(d) => d.to_name(),
        }
    }

    /// Returns the lowercase asset-key form of the name.
    pub fn to_lowername(&self) -> String {
        match self {
            Unit::Building(b) => b.to_lowername(),
            Unit::Ship(s) => s.to_lowername(),
            Unit::Defense(d) => d.to_lowername(),
        }
    }
}

impl Description for Unit {
    /// Returns the user-facing description of this gameplay value.
    fn description(&self) -> &str {
        match self {
            Unit::Building(b) => b.description(),
            Unit::Ship(s) => s.description(),
            Unit::Defense(d) => d.description(),
        }
    }
}

impl Price for Unit {
    /// Returns the resource cost of producing this unit.
    fn price(&self) -> Resources {
        match self {
            Unit::Building(b) => b.price(),
            Unit::Ship(s) => s.price(),
            Unit::Defense(d) => d.price(),
        }
    }
}

impl Combat for Unit {
    /// Returns this unit type's base hull strength.
    fn hull(&self) -> usize {
        match self {
            Unit::Building(_) => 0,
            Unit::Ship(s) => s.hull(),
            Unit::Defense(d) => d.hull(),
        }
    }

    /// Returns this unit type's base shield strength.
    fn shield(&self) -> usize {
        match self {
            Unit::Building(_) => 0,
            Unit::Ship(s) => s.shield(),
            Unit::Defense(d) => d.shield(),
        }
    }

    /// Returns this unit type's base weapon damage.
    fn damage(&self) -> usize {
        match self {
            Unit::Building(_) => 0,
            Unit::Ship(s) => s.damage(),
            Unit::Defense(d) => d.damage(),
        }
    }

    /// Returns rapid-fire probabilities keyed by target unit.
    fn rapid_fire(&self) -> HashMap<Unit, usize> {
        match self {
            Unit::Building(_) => HashMap::new(),
            Unit::Ship(s) => s.rapid_fire(),
            Unit::Defense(d) => d.rapid_fire(),
        }
    }

    /// Returns the movement or animation speed represented by this value.
    fn speed(&self) -> f32 {
        match self {
            Unit::Building(_) => 0.,
            Unit::Ship(s) => s.speed(),
            Unit::Defense(d) => d.speed(),
        }
    }

    /// Returns the deuterium consumed for the supplied movement distance.
    fn fuel_consumption(&self) -> usize {
        match self {
            Unit::Building(_) => 0,
            Unit::Ship(s) => s.fuel_consumption(),
            Unit::Defense(d) => d.fuel_consumption(),
        }
    }
}
