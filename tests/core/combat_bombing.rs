//! Raid limits, eligibility, deterministic targeting and report persistence.

use super::*;
use crate::core::random::DeterministicRngState;
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;

fn buildings(level: usize) -> Army {
    Unit::resource_buildings()
        .into_iter()
        .chain(Unit::industrial_buildings())
        .map(|unit| (unit, level))
        .collect()
}

fn battle(bombers: usize, defenders: Army, raid: BombingRaid, seed: u64) -> MissionReport {
    let mut rng = DeterministicRngState::from_u64(seed).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    origin.colonize(1);
    let mut destination = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
    destination.colonize(2);
    destination.army = defenders;
    let mission = Mission::new_with_id(
        10,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Bomber), bombers)]),
        raid,
        false,
        false,
        None,
    );
    resolve_combat_with_rng(1, &mission, &destination, &mut rng)
}

fn bombing_shots(round: &RoundReport) -> impl Iterator<Item = &ShotReport> {
    round.attacker.iter().flat_map(|cu| &cu.shots).filter(|shot| shot.is_bombing())
}

#[test]
fn bombing_occurs_once_in_long_battles_and_only_survivors_participate() {
    for raid in [BombingRaid::Economic, BombingRaid::Industrial] {
        for shield in [0, 1, 3, 5] {
            let mut delayed_raids = 0;
            let mut later_rounds = 0;
            for seed in 0..64 {
                let mut defenders = buildings(5);
                defenders.insert(Unit::Defense(Defense::GaussCannon), 12);
                defenders.insert(Unit::planetary_shield(), shield);
                let report = battle(12, defenders, raid.clone(), seed);
                let combat = report.combat_report.as_ref().unwrap();
                let first_unshielded = combat.rounds.iter().position(|r| r.planetary_shield == 0);
                let mut losses = Army::new();
                for (index, round) in combat.rounds.iter().enumerate() {
                    let shots = bombing_shots(round).collect::<Vec<_>>();
                    if Some(index) != first_unshielded {
                        assert!(shots.is_empty(), "raid repeated or preceded shield collapse");
                    } else {
                        delayed_raids += usize::from(index > 0 && !shots.is_empty());
                        later_rounds += usize::from(index + 1 < combat.rounds.len());
                        let survivors = round.attacker.iter().filter(|cu| cu.hull > 0).count();
                        if shots.iter().filter(|shot| shot.killed).count() < 9 {
                            assert_eq!(shots.len(), survivors);
                        } else {
                            assert!(shots.len() <= survivors);
                        }
                    }
                    for cu in &round.attacker {
                        let attempts = cu.shots.iter().filter(|shot| shot.is_bombing()).count();
                        assert!(attempts <= 1);
                        if cu.hull == 0 {
                            assert_eq!(attempts, 0, "dead Bomber took part in the raid");
                        }
                    }
                    for shot in shots {
                        assert_ne!(shot.missed, shot.killed);
                        assert!(!shot.rapid_fire);
                        if shot.killed {
                            *losses.entry(shot.unit.unwrap()).or_default() += 1;
                        }
                    }
                    for (unit, level) in buildings(5) {
                        assert_eq!(round.buildings.amount(&unit), level - losses.amount(&unit));
                    }
                }
                for (unit, count) in &losses {
                    assert!(*count <= 3);
                    assert_eq!(report.surviving_defender.amount(unit), 5 - count);
                }
            }
            assert!(later_rounds > 0, "must exercise rounds after the raid");
            if shield == 5 {
                assert!(delayed_raids > 0, "must exercise delayed shield collapse");
            }
        }
    }
}

#[test]
fn bombing_caps_each_building_at_three_and_each_category_at_nine() {
    for raid in [BombingRaid::Economic, BombingRaid::Industrial] {
        for seed in 0..32 {
            let report = battle(1000, buildings(5), raid.clone(), seed);
            let combat = report.combat_report.as_ref().unwrap();
            assert_eq!(combat.rounds.len(), 1);
            let shots = bombing_shots(&combat.rounds[0]).collect::<Vec<_>>();
            assert_eq!(shots.iter().filter(|s| s.killed).count(), 9);
            let mut losses = Army::new();
            for shot in shots {
                let unit = shot.unit.unwrap();
                assert!(losses.amount(&unit) < 3, "targeted a capped building");
                if shot.killed {
                    *losses.entry(unit).or_default() += 1;
                }
            }
            for (unit, before) in buildings(5) {
                let eligible = match raid {
                    BombingRaid::Economic => unit.is_economic_building(),
                    BombingRaid::Industrial => unit.is_industrial_building(),
                    BombingRaid::None => unreachable!(),
                };
                assert_eq!(
                    report.surviving_defender.amount(&unit),
                    before
                        - if eligible {
                            3
                        } else {
                            0
                        }
                );
            }
        }
    }
}

#[test]
fn bombing_spreads_randomly_and_skips_depleted_or_missing_buildings() {
    let mut first_targets = std::collections::BTreeSet::new();
    let mut mixed_sequences = 0;
    for seed in 0..64 {
        let report = battle(10, buildings(5), BombingRaid::Economic, seed);
        let shots = bombing_shots(&report.combat_report.as_ref().unwrap().rounds[0])
            .map(|shot| shot.unit.unwrap())
            .collect::<Vec<_>>();
        first_targets.insert(shots[0]);
        mixed_sequences += usize::from(shots.windows(2).any(|pair| pair[0] != pair[1]));
    }
    assert_eq!(first_targets.len(), 3);
    assert!(mixed_sequences > 48, "raid targeting should not concentrate on one building");

    let mine = Unit::Building(Building::MetalMine);
    let crystal = Unit::Building(Building::CrystalMine);
    let report = battle(1000, Army::from([(mine, 1), (crystal, 5)]), BombingRaid::Economic, 13);
    let shots = bombing_shots(&report.combat_report.as_ref().unwrap().rounds[0]);
    let mut remaining = Army::from([(mine, 1), (crystal, 5)]);
    for shot in shots {
        let unit = shot.unit.unwrap();
        assert!(remaining.amount(&unit) > 0);
        if shot.killed {
            *remaining.get_mut(&unit).unwrap() -= 1;
        }
    }
    assert_eq!(report.surviving_defender.amount(&mine), 0);
    assert_eq!(report.surviving_defender.amount(&crystal), 2);
}

#[test]
fn bombing_requires_exposed_targets_surviving_bombers_and_an_enabled_raid() {
    let mut shielded = buildings(5);
    shielded.insert(Unit::planetary_shield(), 5);
    let mut lethal = buildings(5);
    lethal.insert(Unit::war_sun(), 100);
    for (defenders, raid) in [
        (buildings(5), BombingRaid::None),
        (shielded, BombingRaid::Economic),
        (lethal, BombingRaid::Economic),
        (Army::new(), BombingRaid::Economic),
        (Unit::industrial_buildings().into_iter().map(|u| (u, 5)).collect(), BombingRaid::Economic),
    ] {
        let report = battle(1, defenders.clone(), raid, 42);
        for (unit, count) in defenders.iter().filter(|(unit, _)| unit.is_building()) {
            assert_eq!(report.surviving_defender.amount(unit), *count);
        }
        if let Some(combat) = report.combat_report {
            assert!(combat.rounds.iter().all(|round| bombing_shots(round).next().is_none()));
        }
    }
}

#[test]
fn bombing_reports_preserve_misses_and_replay_deterministically() {
    let mut misses = 0;
    let mut hits = 0;
    for seed in 0..64 {
        let report = battle(1, buildings(5), BombingRaid::Industrial, seed);
        let shot = bombing_shots(&report.combat_report.as_ref().unwrap().rounds[0]).next().unwrap();
        misses += usize::from(shot.missed);
        hits += usize::from(shot.killed);
        let encoded = serde_json::to_string(&report).unwrap();
        let decoded: MissionReport = serde_json::from_str(&encoded).unwrap();
        assert_eq!(serde_json::to_value(decoded).unwrap(), serde_json::to_value(&report).unwrap());
        assert_eq!(
            encoded,
            serde_json::to_string(&battle(1, buildings(5), BombingRaid::Industrial, seed)).unwrap()
        );
    }
    assert!(misses > hits && hits > 0);
}
