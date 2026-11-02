use super::*;
use crate::core::map::icon::Icon;
use crate::core::missions::Mission;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use bevy::math::Vec2;
use proptest::prelude::*;
use rand::{rngs::StdRng, SeedableRng};

proptest! {
    #[test]
    fn raid_loot_fills_capacity_without_exceeding_any_balance(
        metal in any::<usize>(), crystal in any::<usize>(), deuterium in any::<usize>(),
        capacity in any::<usize>(), seed in any::<u64>(),
    ) {
        let available = Resources::new(metal, crystal, deuterium);
        let mut rng = StdRng::seed_from_u64(seed);
        let loot = random_loot(available, capacity, &mut rng);
        prop_assert!(available.contains(loot));
        prop_assert_eq!(loot.total(), capacity.min(available.total()));
    }
}

#[test]
fn raid_cargo_uses_ship_production_and_saturates() {
    for unit in Unit::ships() {
        assert_eq!(
            ship_cargo_capacity(unit),
            [0, 0, 10, 30, 70, 150][unit.production()],
            "{unit:?}"
        );
    }
    assert_eq!(ship_cargo_capacity(Unit::Defense(Defense::RocketLauncher)), 0);
    assert_eq!(
        cargo_capacity(&Army::from([
            (Unit::Ship(Ship::LightFighter), 3),
            (Unit::Ship(Ship::WarSun), 2),
            (Unit::colony_ship(), 1),
            (Unit::probe(), 1),
            (Unit::Defense(Defense::RocketLauncher), 500),
        ])),
        310
    );
    assert_eq!(
        cargo_capacity(&Army::from([
            (Unit::Ship(Ship::LightFighter), usize::MAX),
            (Unit::probe(), usize::MAX)
        ])),
        0
    );
    assert_eq!(cargo_capacity(&Army::from([(Unit::war_sun(), usize::MAX)])), usize::MAX);
}

#[test]
fn raid_composition_varies_but_is_repeatable_and_handles_missing_resources() {
    let available = Resources::new(100, 200, 300);
    let outcomes = (0..32)
        .map(|seed| {
            let first = random_loot(available, 100, &mut StdRng::seed_from_u64(seed));
            let second = random_loot(available, 100, &mut StdRng::seed_from_u64(seed));
            assert_eq!(first, second);
            first
        })
        .collect::<Vec<_>>();
    assert!(outcomes.windows(2).any(|pair| pair[0] != pair[1]));
    assert_eq!(
        random_loot(Resources::new(0, 7, 0), 100, &mut StdRng::seed_from_u64(1)),
        Resources::new(0, 7, 0)
    );
}

#[test]
fn raid_plunder_excludes_scouts_and_withholds_credit_until_delivery() {
    let mut planet = Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
    planet.colonize(2);
    let mission = Mission {
        owner: 1,
        objective: Icon::Raid,
        ..Default::default()
    };
    let mut report = crate::test_support::empty_report(mission, planet.clone());
    report.surviving_attacker = Army::from([(Unit::Ship(Ship::Destroyer), 1), (Unit::probe(), 10)]);
    report.scout_probes = 10;
    let mut players = vec![Player::new(1, 0), Player::new(2, 1)];
    players[0].resources = Resources::new(usize::MAX - 3, usize::MAX, usize::MAX);
    players[1].resources = Resources::new(100, 100, 100);
    let loot = plunder(&report, &mut planet, &mut players, &mut StdRng::seed_from_u64(1));
    assert_eq!(loot.total(), 10);
    assert_eq!(players[0].resources.metal, usize::MAX - 3);
    assert_eq!(players[1].resources, Resources::new(100, 100, 100) - loot);
    players[0].resources = Resources::default();
    let loot = plunder(&report, &mut planet, &mut players, &mut StdRng::seed_from_u64(1));
    assert_eq!(loot.total(), 10, "departed scouts cannot carry extra loot");
    assert_eq!(players[0].resources, Resources::default());
}

#[test]
fn raid_cargo_is_lost_when_surviving_carriers_cannot_hold_it() {
    let cargo = Resources::new(50, 30, 20);
    let fleet = Army::from([(Unit::Ship(Ship::Destroyer), 3)]);
    let retained = surviving_cargo(cargo, &fleet, &mut StdRng::seed_from_u64(3));
    assert_eq!(retained.total(), 30);
    assert!(cargo.contains(retained));
    assert_eq!(
        surviving_cargo(cargo, &Army::new(), &mut StdRng::seed_from_u64(3)),
        Resources::default()
    );
    assert_eq!(surviving_cargo(retained, &fleet, &mut StdRng::seed_from_u64(3)), retained);
    let light_fleet = Army::from([(Unit::Ship(Ship::LightFighter), 100)]);
    assert!(surviving_cargo(cargo, &light_fleet, &mut StdRng::seed_from_u64(3)).is_empty());
}
