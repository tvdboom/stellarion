use super::*;
use crate::core::map::planet::Planet;
use crate::core::random::DeterministicRngState;
use crate::core::units::buildings::{Building, FleetWithdrawal};
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;

fn battle(
    level: usize,
    withdrawal: FleetWithdrawal,
    attacker: Army,
    defender: Army,
    home: Option<usize>,
) -> MissionReport {
    let mut rng = DeterministicRngState::from_u64(23).next_rng();
    let mut origin = Planet::new_with_rng(0, "Attacker".into(), Vec2::ZERO, false, 1., &mut rng);
    origin.colonize(1);
    let mut colony = Planet::new_with_rng(1, "Colony".into(), Vec2::X, false, 1., &mut rng);
    colony.colonize(2);
    colony.army = defender;
    colony.army.insert(Unit::Building(Building::ColonialAdministration), level);
    colony.fleet_withdrawal = withdrawal;
    let mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &colony,
        Icon::Attack,
        attacker,
        BombingRaid::None,
        false,
        false,
        None,
    );
    resolve_combat_with_retreat_with_rng(
        1,
        &mission,
        &colony,
        EnergyGrid {
            supply: 1,
            demand: 1,
        },
        home,
        &mut rng,
    )
}

#[test]
fn level_four_immediate_retreat_takes_one_volley_without_ship_return_fire() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let dreadnought = Unit::Ship(Ship::Dreadnought);
    let turret = Unit::Defense(Defense::GaussCannon);
    let report = battle(
        4,
        FleetWithdrawal::Immediate,
        Army::from([(fighter, 1)]),
        Army::from([(dreadnought, 2), (Unit::colony_ship(), 1), (turret, 3)]),
        Some(2),
    );
    let combat = report.combat_report.as_ref().unwrap();
    let retreat = combat.defender_retreat.as_ref().unwrap();
    assert_eq!(retreat.after_round, Some(0));
    assert_eq!(retreat.ships.amount(&dreadnought), 2);
    assert_eq!(retreat.ships.amount(&Unit::colony_ship()), 1);
    assert!(retreat.ships.keys().all(Unit::is_ship));
    assert!(combat.rounds[0].attacker.iter().any(|unit| !unit.shots.is_empty()));
    assert!(combat.rounds[0]
        .defender
        .iter()
        .filter(|unit| unit.unit.is_ship())
        .all(|unit| unit.shots.is_empty()));
    assert!(combat.rounds[0]
        .defender
        .iter()
        .any(|unit| unit.unit == turret && !unit.shots.is_empty()));
    assert_eq!(report.surviving_defender.amount(&dreadnought), 0);
    assert_eq!(report.surviving_defender.amount(&Unit::colony_ship()), 0);
    assert_eq!(report.surviving_defender.amount(&turret), 3);
}

#[test]
fn level_five_immediate_retreat_leaves_before_any_shot() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let report = battle(
        5,
        FleetWithdrawal::Immediate,
        Army::from([(Unit::war_sun(), 20)]),
        Army::from([(fighter, 2), (Unit::colony_ship(), 1), (Unit::probe(), 3)]),
        Some(2),
    );
    let combat = report.combat_report.as_ref().unwrap();
    let retreat = combat.defender_retreat.as_ref().unwrap();
    assert_eq!(retreat.after_round, None);
    assert_eq!(retreat.ships.amount(&fighter), 2);
    assert_eq!(retreat.ships.amount(&Unit::colony_ship()), 1);
    assert_eq!(retreat.ships.amount(&Unit::probe()), 3);
    assert!(combat
        .rounds
        .iter()
        .flat_map(|round| round.attacker.iter().chain(&round.defender))
        .all(|unit| unit.shots.is_empty()));
    assert_eq!(report.winner(), Some(1));
    assert!(!report.is_stalemate());
}

#[test]
fn threshold_uses_destroyed_production_points_and_one_additional_round() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let cruiser = Unit::Ship(Ship::Cruiser);
    for (level, withdrawal) in [
        (1, FleetWithdrawal::Losses75),
        (2, FleetWithdrawal::Losses50),
        (3, FleetWithdrawal::Losses25),
    ] {
        let report = battle(
            level,
            withdrawal,
            Army::from([(cruiser, 20)]),
            Army::from([(fighter, 90), (Unit::Ship(Ship::HeavyFighter), 20)]),
            Some(2),
        );
        let combat = report.combat_report.as_ref().unwrap();
        let retreat = combat.defender_retreat.as_ref().expect("fleet should escape this battle");
        let departure = retreat.after_round.unwrap();
        assert!(departure >= 1);
        let initial = report
            .planet
            .army
            .iter()
            .filter(|(unit, _)| unit.is_ship())
            .map(|(unit, count)| unit.production() * count)
            .sum::<usize>();
        for (index, round) in combat.rounds[..departure].iter().enumerate() {
            let remaining = round
                .defender
                .iter()
                .filter(|unit| unit.unit.is_ship() && unit.hull > 0)
                .map(|unit| unit.unit.production())
                .sum::<usize>();
            assert_eq!(
                (initial - remaining) * 100 >= initial * withdrawal.losses_percent().unwrap(),
                index == departure - 1
            );
        }
        assert!(combat.rounds[departure]
            .defender
            .iter()
            .filter(|unit| unit.unit.is_ship())
            .all(|unit| unit.shots.is_empty()));
        assert!(combat.rounds[departure].attacker.iter().any(|unit| !unit.shots.is_empty()));
    }
}

#[test]
fn level_five_threshold_has_no_additional_volley() {
    let attacker = Army::from([(Unit::Ship(Ship::Cruiser), 12)]);
    let defender =
        Army::from([(Unit::Ship(Ship::LightFighter), 90), (Unit::Ship(Ship::HeavyFighter), 20)]);
    let covered = battle(3, FleetWithdrawal::Losses25, attacker.clone(), defender.clone(), Some(2));
    let clean = battle(5, FleetWithdrawal::Losses25, attacker, defender, Some(2));
    let covered = covered.combat_report.unwrap().defender_retreat.unwrap();
    let clean = clean.combat_report.unwrap().defender_retreat.unwrap();
    assert_eq!(covered.after_round.unwrap(), clean.after_round.unwrap() + 1);
    assert!(clean.ships.total_production() >= covered.ships.total_production());
}

#[test]
fn missing_home_disabled_orders_and_locked_thresholds_never_evacuate() {
    for (level, setting, home) in [
        (5, FleetWithdrawal::Off, Some(2)),
        (5, FleetWithdrawal::Immediate, None),
        (5, FleetWithdrawal::Immediate, Some(1)),
        (3, FleetWithdrawal::Immediate, Some(2)),
    ] {
        let report = battle(
            level,
            setting,
            Army::from([(Unit::war_sun(), 5)]),
            Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
            home,
        );
        assert!(report
            .combat_report
            .as_ref()
            .is_none_or(|combat| combat.defender_retreat.is_none()));
    }
}

#[test]
fn final_volley_can_destroy_every_withdrawing_ship() {
    let report = battle(
        4,
        FleetWithdrawal::Immediate,
        Army::from([(Unit::war_sun(), 50)]),
        Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
        Some(2),
    );
    assert!(report.combat_report.unwrap().defender_retreat.is_none());
    assert_eq!(report.surviving_defender.amount(&Unit::Ship(Ship::LightFighter)), 0);
}

#[test]
fn withdrawal_from_an_unarmed_colonization_still_records_a_playable_departure() {
    let base = battle(
        5,
        FleetWithdrawal::Immediate,
        Army::from([(Unit::colony_ship(), 1)]),
        Army::from([(Unit::probe(), 2)]),
        Some(2),
    );
    let mut mission = base.mission.clone();
    mission.objective = Icon::Colonize;
    let mut rng = DeterministicRngState::from_u64(19).next_rng();
    let report = resolve_combat_with_retreat_with_rng(
        1,
        &mission,
        &base.planet,
        EnergyGrid::default(),
        Some(2),
        &mut rng,
    );
    assert!(report.planet_colonized);
    let combat = report.combat_report.unwrap();
    assert_eq!(combat.rounds.len(), 1);
    assert_eq!(combat.defender_retreat.unwrap().ships.amount(&Unit::probe()), 2);
}
