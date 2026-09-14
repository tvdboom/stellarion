//! Persisted operating choices and their shared economic and combat effects.

use serde::{Deserialize, Serialize};

use crate::core::constants::SPACE_DOCK_FLEET_PRODUCTION;
use crate::core::identity::PlayerId;
use crate::core::map::model::Map;
use crate::core::map::planet::Planet;
use crate::core::resources::{ResourceName, Resources};
use crate::core::units::buildings::Building;
use crate::core::units::Unit;

/// Player-selectable operating mode for one mine or synthesizer.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MineMode {
    /// Normal output at one Energy per completed level.
    #[default]
    Normal,
    /// One boosted turn at three Energy per level, then one suspended turn before Normal resumes.
    Intensive,
    /// No production or Energy demand.
    Suspended,
}

impl MineMode {
    /// Selectable modes in display order.
    pub const ALL: [Self; 3] = [Self::Normal, Self::Intensive, Self::Suspended];

    /// Energy consumed by one level while this mode operates.
    pub const fn energy(self) -> usize {
        match self {
            Self::Normal => 1,
            Self::Intensive => 3,
            Self::Suspended => 0,
        }
    }

    /// Scales output without intermediate overflow; fractional resources round down.
    pub fn output(self, normal: usize) -> usize {
        match self {
            Self::Normal => normal,
            Self::Intensive => boost_half(normal),
            Self::Suspended => 0,
        }
    }

    /// Compact label for the choice and its tooltip.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Normal => "Normal",
            Self::Intensive => "Intensive",
            Self::Suspended => "Suspended",
        }
    }

    /// Source artwork key for the existing image-tile controls.
    pub const fn image(self) -> &'static str {
        match self {
            Self::Normal => "mine normal",
            Self::Intensive => "mine intensive",
            Self::Suspended => "mine suspended",
        }
    }
}

/// A mine's selection plus its compulsory recovery turn.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MineOperation {
    /// Current operating mode.
    pub mode: MineMode,
    /// Locks Suspended for one full recovery turn, then automatically resumes Normal.
    pub recovering: bool,
}

impl MineOperation {
    /// Advances only after production and every battle have resolved.
    pub fn finish_turn(&mut self) {
        if self.recovering {
            self.mode = MineMode::Normal;
            self.recovering = false;
        } else if self.mode == MineMode::Intensive {
            self.mode = MineMode::Suspended;
            self.recovering = true;
        }
    }
}

/// Exclusive Space Dock specialization.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpaceDockMode {
    /// Original combat statistics and five extra fleet production.
    #[default]
    Industrial,
    /// No fleet production, with 50% stronger hull, shields, and weapons.
    Bastion,
}

impl SpaceDockMode {
    /// Planning turns locked after the selection turn finishes.
    pub const COMMITMENT_TURNS: u64 = 3;

    /// Fleet production supplied by one completed Dock.
    pub const fn production(self) -> usize {
        match self {
            Self::Industrial => SPACE_DOCK_FLEET_PRODUCTION,
            Self::Bastion => 0,
        }
    }

    /// Applies specialization only to a Space Dock's base combat statistic.
    pub fn combat_stat(self, unit: Unit, base: usize) -> usize {
        if self == Self::Bastion && unit == Unit::space_dock() {
            boost_half(base)
        } else {
            base
        }
    }
}

/// Empire-wide production policy chosen by the home-world Senate.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SenatePolicy {
    /// Two additional ship production per owned planet per completed Senate level.
    #[default]
    Expansion,
    /// Five additional defense production per owned planet per completed Senate level.
    Consolidation,
}

impl SenatePolicy {
    /// Policies in display order.
    pub const ALL: [Self; 2] = [Self::Expansion, Self::Consolidation];
    /// Planning turns locked after the selection turn finishes.
    pub const COMMITMENT_TURNS: u64 = 3;

    /// Player-facing name of this policy.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Expansion => "Expansion",
            Self::Consolidation => "Consolidation",
        }
    }

    /// Artwork key for the policy's image button.
    pub const fn image(self) -> &'static str {
        match self {
            Self::Expansion => "senate expansion",
            Self::Consolidation => "senate consolidation",
        }
    }
}

/// Derived Senate support shared by purchase validation and the shop projection.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SenateSupport {
    /// The empire receiving the bonus.
    pub owner: PlayerId,
    /// Completed home-world Senate levels; queued upgrades do not contribute.
    pub level: usize,
    /// Selected empire-wide policy.
    pub policy: SenatePolicy,
}

impl SenateSupport {
    /// Production added on an eligible owned planet for the selected category.
    pub fn bonus(self, planet: &Planet, policy: SenatePolicy) -> usize {
        if self.policy == policy
            && planet.owned == Some(self.owner)
            && !planet.is_moon()
            && !planet.is_destroyed
        {
            self.level.saturating_mul(match policy {
                SenatePolicy::Expansion => 2,
                SenatePolicy::Consolidation => 5,
            })
        } else {
            0
        }
    }

    /// Total ship capacity, including local Shipyard and Space Dock production.
    pub fn fleet_capacity(self, planet: &Planet) -> usize {
        planet.max_fleet_production().saturating_add(self.bonus(planet, SenatePolicy::Expansion))
    }

    /// Total defense capacity, including the local Factory.
    pub fn defense_capacity(self, planet: &Planet) -> usize {
        planet
            .max_battery_production()
            .saturating_add(self.bonus(planet, SenatePolicy::Consolidation))
    }

    /// Prevents switching policy after spending its production on any owned planet.
    pub fn supports_queues(self, map: &Map) -> bool {
        map.planets
            .iter()
            .filter(|planet| planet.owned == Some(self.owner) && !planet.is_moon())
            .all(|planet| {
                planet.fleet_production() <= self.fleet_capacity(planet)
                    && planet.battery_production() <= self.defense_capacity(planet)
            })
    }
}

/// Operating choices belonging to the infrastructure on one world.
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildingOperations {
    /// Metal, Crystal, and Deuterium extraction, in that order.
    pub mines: [MineOperation; 3],
    /// `None` recovers the complete bulk haul; a selection recovers only that resource.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub recycler_focus: Option<ResourceName>,
    /// Current Space Dock specialization; Industrial is the initial setting.
    pub space_dock: SpaceDockMode,
    /// First planning turn on which the Space Dock may change modes again.
    pub space_dock_locked_until: u64,
    /// An editable selection that becomes committed when this turn finishes.
    pub space_dock_selection_pending: bool,
    /// Empire-wide Senate policy; only the owner's home-world Senate contributes.
    pub senate: SenatePolicy,
    /// First planning turn on which the Senate may change policy again.
    pub senate_locked_until: u64,
    /// An editable policy choice that becomes committed when this turn finishes.
    pub senate_selection_pending: bool,
}

impl BuildingOperations {
    /// Applies each extraction setting after the planet's Terraformer modifiers.
    pub fn mine_output(&self, mut normal: Resources) -> Resources {
        for resource in [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium] {
            *normal.get_mut(&resource) = self.mine(resource).mode.output(normal.get(&resource));
        }
        normal
    }

    /// Accesses the selected resource's extraction settings.
    pub fn mine(&self, resource: ResourceName) -> &MineOperation {
        &self.mines[resource_index(resource)]
    }

    /// Mutably accesses the selected resource's extraction settings.
    pub fn mine_mut(&mut self, resource: ResourceName) -> &mut MineOperation {
        &mut self.mines[resource_index(resource)]
    }

    /// Applies selective recovery to an already rolled bulk haul.
    pub fn recycler_output(&self, bulk: Resources) -> Resources {
        match self.recycler_focus {
            None => bulk,
            Some(resource) => {
                let mut selected = Resources::default();
                *selected.get_mut(&resource) = boost_half(bulk.get(&resource));
                selected
            },
        }
    }
}

/// Returns the extraction building responsible for a resource.
pub const fn mine_building(resource: ResourceName) -> Building {
    match resource {
        ResourceName::Metal => Building::MetalMine,
        ResourceName::Crystal => Building::CrystalMine,
        ResourceName::Deuterium => Building::DeuteriumSynthesizer,
    }
}

const fn resource_index(resource: ResourceName) -> usize {
    match resource {
        ResourceName::Metal => 0,
        ResourceName::Crystal => 1,
        ResourceName::Deuterium => 2,
    }
}

fn boost_half(value: usize) -> usize {
    value.saturating_add(value / 2)
}
