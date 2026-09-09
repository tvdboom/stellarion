//! Deterministic Recycler targets, debris lifetime, and per-turn resource output.

use std::collections::{BTreeMap, BTreeSet};

use bevy::math::Vec2;
use rand::{Rng, RngExt};

use crate::core::combat::report::MissionReport;
use crate::core::identity::PlayerId;
use crate::core::map::asteroids::recycler_asteroid_targets;
use crate::core::map::model::Map;
#[cfg(test)]
use crate::core::map::planet::Planet;
use crate::core::map::planet::PlanetId;
use crate::core::player::Player;
use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Unit};

/// Minimum per-level haul while harvesting the permanent asteroid field.
pub const RECYCLER_ASTEROID_OUTPUT_MIN: Resources = Resources::new(15, 7, 3);
/// Maximum per-level haul while harvesting the permanent asteroid field.
pub const RECYCLER_ASTEROID_OUTPUT_MAX: Resources = Resources::new(25, 13, 7);
/// Minimum per-level haul while salvaging temporary battle debris.
pub const RECYCLER_DEBRIS_OUTPUT_MIN: Resources = Resources::new(40, 20, 8);
/// Maximum per-level haul while salvaging temporary battle debris.
pub const RECYCLER_DEBRIS_OUTPUT_MAX: Resources = Resources::new(60, 30, 12);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Coarse wreckage class controlling how many turns a battle site remains usable.
pub enum DebrisSize {
    /// One to three destroyed ships; visible and recyclable for one turn.
    Small,
    /// Four to fifteen destroyed ships; visible and recyclable for two turns.
    Medium,
    /// Sixteen or more destroyed ships; visible and recyclable for three turns.
    Large,
}

impl DebrisSize {
    /// Classifies a non-empty site by the number of destroyed ships it represents.
    pub fn from_losses(losses: usize) -> Option<Self> {
        match losses {
            0 => None,
            1..=3 => Some(Self::Small),
            4..=15 => Some(Self::Medium),
            _ => Some(Self::Large),
        }
    }

    /// Number of turn intervals for which this debris can be harvested.
    pub const fn lifetime_turns(self) -> usize {
        match self {
            Self::Small => 1,
            Self::Medium => 2,
            Self::Large => 3,
        }
    }
}

/// A deduplicated public battle-debris site.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DebrisSite {
    pub(crate) losses: usize,
    pub(crate) latest_turn: usize,
    pub(crate) seed: u32,
}

impl DebrisSite {
    /// Returns the aggregate display class of all wreckage still present at this world.
    pub(crate) fn size(&self) -> Option<DebrisSize> {
        DebrisSize::from_losses(self.losses)
    }
}

/// Counts only ships actually destroyed in combat, excluding retreats and consumed Colony Ships.
pub(crate) fn destroyed_ships(report: &MissionReport) -> usize {
    if report.combat_report.is_none() {
        return 0;
    }
    [
        (&report.mission.army, &report.surviving_attacker),
        (&report.planet.army, &report.surviving_defender),
    ]
    .into_iter()
    .enumerate()
    .flat_map(|(side, (before, after))| {
        before.iter().filter(|(unit, _)| unit.is_ship() && **unit != Unit::colony_ship()).map(
            move |(unit, count)| {
                count.saturating_sub(after.amount(unit)).saturating_sub(if side == 1 {
                    report.escaped_defenders(unit)
                } else {
                    0
                })
            },
        )
    })
    .fold(0usize, usize::saturating_add)
}

/// Builds public debris from canonical participant reports, deduplicating their persisted copies.
pub(crate) fn debris_sites<'a>(
    reports: impl Iterator<Item = &'a MissionReport>,
    turn: usize,
) -> BTreeMap<PlanetId, DebrisSite> {
    let mut seen = BTreeSet::new();
    let mut sites = BTreeMap::<PlanetId, DebrisSite>::new();
    for report in reports {
        if !seen.insert(report.id) {
            continue;
        }
        let losses = destroyed_ships(report);
        let Some(size) = DebrisSize::from_losses(losses) else {
            continue;
        };
        if turn.checked_sub(report.turn).is_none_or(|age| age >= size.lifetime_turns()) {
            continue;
        }
        let site = sites.entry(report.mission.destination).or_default();
        site.losses = site.losses.saturating_add(losses);
        site.latest_turn = site.latest_turn.max(report.turn);
        site.seed ^= report.id as u32;
    }
    sites
}

#[derive(Clone, Copy, Debug, PartialEq)]
/// Source selected by a working Recycler for one turn interval.
pub(crate) enum RecyclerSource {
    /// Temporary battle wreckage around the Recycler's own planet.
    Debris {
        /// World-space center of the wreckage corridor.
        target: Vec2,
        /// Current aggregate wreckage class.
        size: DebrisSize,
    },
    /// The map's permanent asteroid field.
    AsteroidField {
        /// World-space center of the nearest reachable asteroid.
        target: Vec2,
    },
}

impl RecyclerSource {
    /// Returns the inclusive per-level yield range associated with this source.
    pub(crate) const fn output_range(self) -> (Resources, Resources) {
        match self {
            Self::Debris {
                ..
            } => (RECYCLER_DEBRIS_OUTPUT_MIN, RECYCLER_DEBRIS_OUTPUT_MAX),
            Self::AsteroidField {
                ..
            } => (RECYCLER_ASTEROID_OUTPUT_MIN, RECYCLER_ASTEROID_OUTPUT_MAX),
        }
    }

    /// Rolls one level's haul from the persisted turn stream, then scales it by building level.
    fn roll_output<R: Rng + ?Sized>(self, level: usize, rng: &mut R) -> Resources {
        let (minimum, maximum) = self.output_range();
        Resources::new(
            rng.random_range(minimum.metal..=maximum.metal),
            rng.random_range(minimum.crystal..=maximum.crystal),
            rng.random_range(minimum.deuterium..=maximum.deuterium),
        ) * level
    }
}

/// Selects one source for every world in one pass through the shared asteroid field.
pub(crate) fn recycler_sources(
    map: &Map,
    debris: &BTreeMap<PlanetId, DebrisSite>,
) -> BTreeMap<PlanetId, RecyclerSource> {
    let asteroid_targets = recycler_asteroid_targets(map);
    map.planets()
        .into_iter()
        .filter_map(|planet| {
            let source = if let Some(site) = debris.get(&planet.id) {
                Some(RecyclerSource::Debris {
                    target: planet.position - Vec2::X * planet.size() * 1.06,
                    size: site.size()?,
                })
            } else {
                asteroid_targets.get(&planet.id).copied().map(|target| {
                    RecyclerSource::AsteroidField {
                        target,
                    }
                })
            }?;
            Some((planet.id, source))
        })
        .collect()
}

/// Rolls one empire's raw Recycler output from the persisted stream for this turn.
pub(crate) fn recycler_production<R: Rng + ?Sized>(
    map: &Map,
    players: &[Player],
    player_id: PlayerId,
    turn: usize,
    rng: &mut R,
) -> Resources {
    let recyclers = map
        .planets
        .iter()
        .filter(|planet| planet.owned == Some(player_id))
        .filter_map(|planet| {
            let level =
                planet.army.amount(&Unit::Building(Building::Recycler)).min(Building::MAX_LEVEL);
            (level > 0).then_some((planet.id, level))
        })
        .collect::<Vec<_>>();
    if recyclers.is_empty() {
        return Resources::default();
    }
    let debris = debris_sites(players.iter().flat_map(|player| player.reports.iter()), turn);
    let sources = recycler_sources(map, &debris);
    recyclers
        .iter()
        .filter_map(|(planet_id, level)| {
            sources.get(planet_id).copied().map(|source| source.roll_output(*level, rng))
        })
        .sum()
}

#[cfg(test)]
#[path = "../../tests/core/recycling.rs"]
mod tests;
