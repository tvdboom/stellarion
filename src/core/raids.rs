//! Deterministic raid capacity, finite independent caches, and resource selection.

use rand::seq::SliceRandom;
use rand::{Rng, RngExt};

use crate::core::combat::report::MissionReport;
use crate::core::map::planet::Planet;
use crate::core::player::Player;
use crate::core::resources::{ResourceName, Resources};
use crate::core::units::{Army, Price, Unit};

/// Per-ship raid cargo: production levels 1–5 carry 0, 10, 30, 70, and 150 resources.
/// Non-ships cannot carry cargo. The exponential curve is `10 * (2^(level - 1) - 1)`.
pub fn ship_cargo_capacity(unit: Unit) -> usize {
    if !unit.is_ship() {
        return 0;
    }
    2_usize
        .saturating_pow(unit.production().saturating_sub(1) as u32)
        .saturating_sub(1)
        .saturating_mul(10)
}

/// Combined carrying capacity of the surviving ships, saturating for very large fleets.
pub fn cargo_capacity(army: &Army) -> usize {
    army.iter().fold(0, |capacity, (unit, count)| {
        capacity.saturating_add(ship_cargo_capacity(*unit).saturating_mul(*count))
    })
}

/// A neutral world's original combat units fund a small cache that never regenerates.
pub(crate) fn independent_cache(army: &Army) -> Resources {
    army.iter()
        .filter(|(unit, _)| !unit.is_building() && !unit.is_missile())
        .map(|(unit, count)| unit.price() * *count)
        .sum::<Resources>()
        .scaled_percent(10)
}

/// Seizes loot only on victory. The return mission carries it until docking.
pub(crate) fn plunder<R: Rng + ?Sized>(
    report: &MissionReport,
    destination: &mut Planet,
    players: &mut [Player],
    rng: &mut R,
) -> Resources {
    if report.winner() != Some(report.mission.owner) {
        return Resources::default();
    }
    let Some(attacker) = players.iter().find(|player| player.id == report.mission.owner) else {
        return Resources::default();
    };
    let mut carriers = report.surviving_attacker.clone();
    if let Some(probes) = carriers.get_mut(&Unit::probe()) {
        *probes = probes.saturating_sub(report.scout_probes);
    }
    let mut capacity = cargo_capacity(&carriers);
    // Abandonment removes the colony's claim on the shared stockpile, even if
    // the former owner's fleet still controls this world.
    let defender = destination.owned.filter(|owner| *owner != attacker.id);
    let stockpile = if let Some(owner) = defender {
        players
            .iter()
            .find(|player| player.id == owner)
            .map_or(Resources::default(), |player| player.resources)
    } else if destination.controlled.is_none() && destination.owned.is_none() {
        let total = destination.raid_cache.total();
        capacity = capacity.min(total / 2 + total % 2);
        destination.raid_cache
    } else {
        Resources::default()
    };
    let loot = random_loot(stockpile, capacity, rng);
    if let Some(owner) = defender {
        if let Some(player) = players.iter_mut().find(|player| player.id == owner) {
            player.resources -= loot;
        }
    } else {
        destination.raid_cache -= loot;
    }
    loot
}

/// Redistributes cargo among survivors, losing any excess when carrying ships are destroyed.
pub(crate) fn surviving_cargo<R: Rng + ?Sized>(
    cargo: Resources,
    army: &Army,
    rng: &mut R,
) -> Resources {
    let capacity = cargo_capacity(army);
    if cargo.total() <= capacity {
        cargo
    } else {
        random_loot(cargo, capacity, rng)
    }
}

/// Randomly divides a full cargo across available resources in constant time.
/// The lower bound reserves enough for later kinds, even with very large balances.
fn random_loot<R: Rng + ?Sized>(available: Resources, capacity: usize, rng: &mut R) -> Resources {
    let mut kinds = [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium];
    kinds.shuffle(rng);
    let mut remaining = capacity.min(available.total());
    let mut loot = Resources::default();
    for (index, kind) in kinds.iter().enumerate() {
        let later = kinds[index + 1..]
            .iter()
            .fold(0_usize, |total, kind| total.saturating_add(available.get(kind)));
        let minimum = remaining.saturating_sub(later);
        let maximum = remaining.min(available.get(kind));
        let amount = if minimum == maximum {
            minimum
        } else {
            rng.random_range(minimum..=maximum)
        };
        *loot.get_mut(kind) = amount;
        remaining -= amount;
    }
    loot
}

#[cfg(test)]
#[path = "../../tests/core/raids.rs"]
mod tests;
