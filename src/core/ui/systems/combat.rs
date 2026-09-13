//! Borrowed combat-detail views; opening a report never copies its shot history.

use crate::core::combat::report::{RoundReport, Side};
use crate::core::combat::resolution::CombatUnit;
use crate::core::units::{Army, Unit};

pub(super) struct CombatRoundView<'a> {
    rounds: &'a [RoundReport],
    pub buildings: &'a Army,
    pub planetary_shield: usize,
    pub antiballistic_fired: usize,
    pub destroy_probability: f32,
}

impl<'a> CombatRoundView<'a> {
    pub fn new(rounds: &'a [RoundReport]) -> Option<Self> {
        let first = rounds.first()?;
        Some(Self {
            rounds,
            buildings: &rounds
                .iter()
                .find(|round| !round.buildings.is_empty())
                .unwrap_or(first)
                .buildings,
            planetary_shield: rounds
                .iter()
                .fold(0, |sum, round| sum.saturating_add(round.planetary_shield)),
            antiballistic_fired: rounds
                .iter()
                .fold(0, |sum, round| sum.saturating_add(round.antiballistic_fired)),
            destroy_probability: if rounds.len() == 1 {
                first.destroy_probability
            } else {
                1. - rounds
                    .iter()
                    .fold(1., |chance, round| chance * (1. - round.destroy_probability))
            },
        })
    }

    pub fn units(&self, side: &Side) -> impl Iterator<Item = &'a CombatUnit> + Clone {
        let attacker = *side == Side::Attacker;
        self.rounds.iter().flat_map(move |round| {
            if attacker {
                round.attacker.iter()
            } else {
                round.defender.iter()
            }
        })
    }
}

#[derive(Default)]
pub(super) struct CombatStatistics {
    pub units: usize,
    pub shield_damage: usize,
    pub hull_damage: usize,
    pub ps_damage: usize,
    pub unit_shots: usize,
    pub missile_shots: usize,
    pub building_shots: usize,
    pub shots_missed: usize,
    pub total_repaired: usize,
    pub missiles_hit: usize,
    pub bombs_hit: usize,
    pub rapid_fire: usize,
    pub enemies_killed: usize,
}

impl CombatStatistics {
    pub fn for_units<'a>(units: impl Iterator<Item = &'a CombatUnit>) -> Self {
        let mut stats = Self::default();
        for unit in units {
            stats.units += 1;
            stats.total_repaired += unit.repairs.iter().sum::<usize>();
            for shot in &unit.shots {
                stats.shield_damage += shot.shield_damage;
                stats.hull_damage += shot.hull_damage;
                stats.ps_damage += shot.planetary_shield_damage;
                stats.rapid_fire += usize::from(shot.rapid_fire);
                stats.enemies_killed += usize::from(shot.killed);
                if let Some(target) = shot.unit {
                    if !target.is_building() {
                        stats.unit_shots += 1;
                        stats.shots_missed += usize::from(shot.missed);
                    } else if target != Unit::planetary_shield() {
                        stats.building_shots += 1;
                        stats.bombs_hit += usize::from(shot.killed);
                    }
                    if target == Unit::interplanetary_missile() {
                        stats.missile_shots += 1;
                        stats.missiles_hit += usize::from(shot.killed);
                    }
                }
            }
        }
        stats
    }
}

#[cfg(test)]
#[path = "../../../../tests/core/ui_combat.rs"]
mod tests;
