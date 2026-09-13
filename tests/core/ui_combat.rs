use super::*;
use crate::core::combat::resolution::ShotReport;
use crate::core::units::buildings::Building;

fn combatant(id: u64, unit: Unit, shots: Vec<ShotReport>) -> CombatUnit {
    CombatUnit {
        id,
        owner: None,
        unit,
        hull: 10,
        shield: 2,
        repairs: vec![3, 5],
        shots,
    }
}

#[test]
fn total_view_borrows_rounds_in_order_and_preserves_aggregate_metadata() {
    let rounds = [
        RoundReport {
            attacker: vec![combatant(1, Unit::probe(), vec![])],
            planetary_shield: usize::MAX,
            antiballistic_fired: 2,
            destroy_probability: 0.25,
            ..Default::default()
        },
        RoundReport {
            attacker: vec![combatant(1, Unit::probe(), vec![])],
            defender: vec![combatant(2, Unit::space_dock(), vec![])],
            buildings: Army::from([(Unit::planetary_shield(), 3)]),
            planetary_shield: 1,
            antiballistic_fired: 3,
            destroy_probability: 0.5,
        },
    ];
    assert!(CombatRoundView::new(&[]).is_none());
    let view = CombatRoundView::new(&rounds).unwrap();
    let units: Vec<_> = view.units(&Side::Attacker).collect();
    assert!(std::ptr::eq(units[0], &rounds[0].attacker[0]));
    assert!(std::ptr::eq(units[1], &rounds[1].attacker[0]));
    assert!(std::ptr::eq(view.buildings, &rounds[1].buildings));
    assert_eq!(view.units(&Side::Defender).count(), 1);
    assert_eq!(view.planetary_shield, usize::MAX);
    assert_eq!(view.antiballistic_fired, 5);
    assert_eq!(view.destroy_probability, 0.625);
    let single = CombatRoundView::new(&rounds[1..]).unwrap();
    assert_eq!(single.destroy_probability, rounds[1].destroy_probability);
    assert_eq!(single.antiballistic_fired, 3);
}

#[test]
fn statistics_preserve_shot_categories_repairs_and_hover_filtering() {
    let targets = [
        None,
        Some(Unit::probe()),
        Some(Unit::interplanetary_missile()),
        Some(Unit::planetary_shield()),
        Some(Unit::Building(Building::MetalMine)),
    ];
    let units = [
        combatant(
            1,
            Unit::war_sun(),
            targets
                .into_iter()
                .map(|unit| ShotReport {
                    unit,
                    shield_damage: 2,
                    hull_damage: 3,
                    planetary_shield_damage: 4,
                    missed: true,
                    killed: true,
                    rapid_fire: true,
                    ..Default::default()
                })
                .collect(),
        ),
        combatant(2, Unit::probe(), vec![]),
    ];
    let stats = CombatStatistics::for_units(units.iter());
    assert_eq!((stats.units, stats.total_repaired), (2, 16));
    assert_eq!((stats.shield_damage, stats.hull_damage, stats.ps_damage), (10, 15, 20));
    assert_eq!((stats.unit_shots, stats.shots_missed), (2, 2));
    assert_eq!((stats.missile_shots, stats.missiles_hit), (1, 1));
    assert_eq!((stats.building_shots, stats.bombs_hit), (1, 1));
    assert_eq!((stats.rapid_fire, stats.enemies_killed), (5, 5));
    let hovered =
        CombatStatistics::for_units(units.iter().filter(|unit| unit.unit == Unit::probe()));
    assert_eq!((hovered.units, hovered.unit_shots, hovered.total_repaired), (1, 0, 8));
    assert_eq!(CombatStatistics::for_units(std::iter::empty()).units, 0);
}

#[test]
#[ignore = "microbenchmark; compares owned and borrowed report preparation"]
fn benchmark_combat_report_preparation() {
    use std::hint::black_box;
    use std::time::Instant;

    let round = RoundReport {
        attacker: (0..1_024)
            .map(|id| {
                combatant(
                    id,
                    Unit::war_sun(),
                    vec![
                        ShotReport {
                            unit: Some(Unit::probe()),
                            hull_damage: 10,
                            ..Default::default()
                        };
                        8
                    ],
                )
            })
            .collect(),
        ..Default::default()
    };
    let rounds = vec![round; 8];
    let started = Instant::now();
    for _ in 0..100 {
        let owned = rounds.iter().flat_map(|round| round.attacker.clone()).collect::<Vec<_>>();
        black_box(CombatStatistics::for_units(black_box(&owned).iter()));
    }
    let copied = started.elapsed();
    let started = Instant::now();
    for _ in 0..100 {
        let view = CombatRoundView::new(black_box(&rounds)).unwrap();
        black_box(CombatStatistics::for_units(view.units(&Side::Attacker)));
    }
    let borrowed = started.elapsed();
    eprintln!("100 preparations, 8 rounds × 1,024 combatants × 8 shots: copied={copied:?}, borrowed={borrowed:?}");
}
