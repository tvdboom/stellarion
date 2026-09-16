use rand::SeedableRng;

use super::*;
use crate::core::map::planet::PlanetKind;
use crate::core::resources::Resources;

#[test]
fn elder_star_dragon_kills_an_unscreened_war_sun_without_rapid_fire() {
    let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    let mut encounter = Planet::new(1, "Lone Elder Star Dragon".into(), Vec2::X, false, 1.0);
    encounter.army.insert(Unit::Fauna(SpaceFauna::ElderStarDragon), 1);
    let mission = Mission::new_with_id(
        1,
        30,
        1,
        &origin,
        &encounter,
        Icon::Attack,
        Army::from([(Unit::war_sun(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );

    let report = resolve_combat_with_rng(
        30,
        &mission,
        &encounter,
        &mut rand_chacha::ChaCha8Rng::from_seed([31; 32]),
    );

    assert_eq!(report.surviving_attacker.amount(&Unit::war_sun()), 0);
    let combat = report.combat_report.unwrap();
    assert!(combat.rounds.iter().all(|round| {
        round
            .defender
            .iter()
            .filter(|unit| unit.unit == Unit::Fauna(SpaceFauna::ElderStarDragon))
            .all(|unit| unit.shots.len() <= 1)
    }));
}

#[test]
fn nullstar_behemoth_destroys_a_war_sun_in_exactly_two_single_shots() {
    let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    let mut encounter = Planet::new(1, "Lone Nullstar Behemoth".into(), Vec2::X, false, 1.0);
    let behemoth = Unit::Fauna(SpaceFauna::NullstarBehemoth);
    encounter.army.insert(behemoth, 1);
    let mission = Mission::new_with_id(
        1,
        50,
        1,
        &origin,
        &encounter,
        Icon::Attack,
        Army::from([(Unit::war_sun(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );

    let report = resolve_combat_with_rng(
        50,
        &mission,
        &encounter,
        &mut rand_chacha::ChaCha8Rng::from_seed([50; 32]),
    );
    let combat = report.combat_report.unwrap();
    let shots = combat
        .rounds
        .iter()
        .flat_map(|round| &round.defender)
        .filter(|unit| unit.unit == behemoth)
        .flat_map(|unit| &unit.shots)
        .filter(|shot| shot.unit == Some(Unit::war_sun()))
        .collect::<Vec<_>>();

    assert_eq!(combat.rounds.len(), 2);
    assert_eq!(shots.len(), 2);
    assert_eq!(shots[0].shield_damage, Unit::war_sun().shield());
    assert!(!shots[0].killed);
    assert!(shots[1].killed);
    assert!(shots.iter().all(|shot| !shot.rapid_fire));
    assert_eq!(report.surviving_attacker.amount(&Unit::war_sun()), 0);
}

#[test]
fn nullstar_behemoth_one_shots_every_conventional_spaceship() {
    for ship in Ship::iter().filter(|ship| *ship != Ship::ColonyShip && *ship != Ship::WarSun) {
        let unit = Unit::Ship(ship);
        assert!(
            SpaceFauna::NullstarBehemoth.damage() >= unit.hull() + unit.shield(),
            "{ship:?} survived the Extinction Ray stat line"
        );
    }
}

#[test]
fn bastion_statistics_apply_to_every_combat_round_and_survive_reports() {
    use crate::core::units::operations::SpaceDockMode;
    let mut destination = Planet::new(1, "Bastion".into(), Vec2::X, false, 1.0);
    destination.colonize(2);
    destination.army.clear();
    destination.army.insert(Unit::space_dock(), 1);
    destination.operations.space_dock = SpaceDockMode::Bastion;
    let origin = Planet::new(0, "Attacker".into(), Vec2::ZERO, false, 1.0);
    let mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Battleship), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([9; 32]),
    );
    let report: MissionReport =
        serde_json::from_value(serde_json::to_value(report).unwrap()).unwrap();
    assert_eq!(report.unit_hull(Unit::space_dock(), &Side::Defender), 3_000);
    assert_eq!(report.unit_shield(Unit::space_dock(), &Side::Defender), 165);
    let rounds = &report.combat_report.as_ref().unwrap().rounds;
    assert!(rounds.len() > 1);
    for round in rounds {
        let dock = round.defender.iter().find(|u| u.unit == Unit::space_dock()).unwrap();
        let incoming_hull: usize = round
            .attacker
            .iter()
            .flat_map(|u| &u.shots)
            .filter(|s| s.target_id == Some(dock.id))
            .map(|s| s.hull_damage)
            .sum();
        assert_eq!(incoming_hull, 0, "the Bastion shield absorbs a lone Battleship's volley");
        assert_eq!(dock.hull, 3_000);
        for shot in
            dock.shots.iter().filter(|shot| !shot.missed && !shot.killed && shot.unit.is_some())
        {
            assert_eq!(shot.shield_damage + shot.hull_damage, 225);
        }
    }
}

#[test]
/// Zero-damage armies produce a bounded draw instead of an infinite combat loop.
fn zero_damage_stalemate_terminates() {
    let destination = Planet {
        id: 1,
        name: "Stalemate".to_string(),
        kind: PlanetKind::Dry,
        image: 2,
        diameter: 10_000,
        temperature: (0, 10),
        position: Vec2::X,
        resources: Default::default(),
        jump_gate: 0,

        operations: Default::default(),
        terraformer_focus: Some(crate::core::resources::ResourceName::Metal),
        command_relay_active: true,
        shield_overload: Default::default(),
        fleet_withdrawal: Default::default(),
        is_destroyed: false,
        owned: Some(2),
        controlled: Some(2),
        independent_population: Default::default(),
        army: Army::from([(Unit::probe(), 1)]).into(),
        protection_permissions: Default::default(),
        buy: Vec::new(),
        surface_build_order: [None; 4],
    };
    let origin = Planet {
        id: 0,
        position: Vec2::ZERO,
        ..destination.clone()
    };
    let mut mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::probe(), 1)]),
        BombingRaid::None,
        true,
        false,
        None,
    );
    mission.position = destination.position;

    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([9; 32]),
    );
    assert_eq!(report.surviving_attacker.amount(&Unit::probe()), 1);
    assert_eq!(report.surviving_defender.amount(&Unit::probe()), 1);
    assert!(report.is_stalemate());
    assert_eq!(report.winner(), None);
}

#[test]
fn space_dock_blocks_the_death_ray_while_it_survives() {
    let mut destination = Planet::new(1, "Fortress".into(), Vec2::X, false, 1.0);
    destination.owned = Some(2);
    destination.controlled = Some(2);
    destination.army.insert(Unit::space_dock(), 1);
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.owned = Some(1);
    origin.controlled = Some(1);
    let mut mission = Mission::new_with_id(
        2,
        1,
        1,
        &origin,
        &destination,
        Icon::Destroy,
        Army::from([(Unit::war_sun(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.position = destination.position;

    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([13; 32]),
    );
    let first_round = &report.combat_report.unwrap().rounds[0];

    assert!(first_round
        .defender
        .iter()
        .any(|unit| unit.unit == Unit::space_dock() && unit.hull > 0));
    assert_eq!(first_round.destroy_probability, 0.0);
}

#[test]
fn first_war_sun_volley_uses_ten_percent_plus_planet_modifier() {
    let mut destination = Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
    let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    for (diameter, chance) in [(1_500, 0.12_f32), (7_000, 0.10), (120_000, 0.08)] {
        destination.diameter = diameter;
        let mut mission = Mission::new_with_id(
            2,
            1,
            1,
            &origin,
            &destination,
            Icon::Destroy,
            Army::from([(Unit::war_sun(), 1)]),
            BombingRaid::None,
            false,
            false,
            None,
        );
        mission.position = destination.position;
        let report = resolve_combat_with_rng(
            1,
            &mission,
            &destination,
            &mut rand_chacha::ChaCha8Rng::from_seed([13; 32]),
        );
        let actual = report.combat_report.unwrap().rounds[0].destroy_probability;
        assert!((actual - chance).abs() < 1e-6, "diameter {diameter}: {actual}");
    }
}

#[test]
fn ships_and_bombers_use_opposite_target_priorities_with_fallbacks() {
    fn combatant(id: u64, unit: Unit) -> CombatUnit {
        CombatUnit {
            id,
            owner: Some(2),
            unit,
            hull: unit.hull(),
            shield: unit.shield(),
            repairs: Vec::new(),
            shots: Vec::new(),
        }
    }

    let ship = Unit::Ship(Ship::Probe);
    let dock = Unit::space_dock();
    let defense = Unit::Defense(crate::core::units::defense::Defense::RocketLauncher);
    let mut mixed = vec![combatant(1, ship), combatant(2, dock), combatant(3, defense)];
    let mut rng = rand_chacha::ChaCha8Rng::from_seed([14; 32]);

    let ordinary_target = choose_combat_target(Unit::Ship(Ship::Cruiser), &mut mixed, &mut rng)
        .expect("ordinary ship must find a target")
        .unit;
    assert!(ordinary_target == ship || ordinary_target == dock);

    let bomber_target = choose_combat_target(Unit::Ship(Ship::Bomber), &mut mixed, &mut rng)
        .expect("bomber must find a target")
        .unit;
    assert!(bomber_target.is_defense());

    let mut defense_only = vec![combatant(4, defense)];
    assert_eq!(
        choose_combat_target(Unit::Ship(Ship::Cruiser), &mut defense_only, &mut rng)
            .expect("ordinary ship must fall back to defenses")
            .unit,
        defense
    );

    let mut ship_only = vec![combatant(5, ship)];
    assert_eq!(
        choose_combat_target(Unit::Ship(Ship::Bomber), &mut ship_only, &mut rng)
            .expect("bomber must fall back to ships")
            .unit,
        ship
    );
}

#[test]
/// An adversarial rapid-fire roll cannot keep one firing loop alive forever.
fn rapid_fire_chain_has_a_hard_limit() {
    let attacker = Unit::war_sun();
    let target = Unit::probe();
    assert!(!rapid_fire_stops(&attacker, &target, MAX_SHOTS_PER_UNIT_PER_ROUND - 1, 0.0,));
    assert!(rapid_fire_stops(&attacker, &target, MAX_SHOTS_PER_UNIT_PER_ROUND, 0.999,));
}

#[test]
/// Rapid-fire percentages are the probability of shooting again, not stopping.
fn rapid_fire_probability_controls_repeat_shots() {
    let attacker = Unit::war_sun();
    let target = Unit::probe();

    assert!(!rapid_fire_stops(&attacker, &target, 1, 0.799));
    assert!(rapid_fire_stops(&attacker, &target, 1, 0.8));
}

#[test]
/// A target without a rapid-fire entry never grants another shot.
fn missing_rapid_fire_probability_stops_shooting() {
    assert!(rapid_fire_stops(&Unit::probe(), &Unit::war_sun(), 1, 0.0));
}

#[test]
fn antiballistic_missiles_fire_one_at_a_time_and_stop_after_each_interception() {
    let mut defenders = (1..=4)
        .map(|id| CombatUnit {
            id,
            owner: None,
            unit: Unit::antiballistic_missile(),
            hull: Unit::antiballistic_missile().hull(),
            shield: Unit::antiballistic_missile().shield(),
            repairs: Vec::new(),
            shots: Vec::new(),
        })
        .collect::<Vec<_>>();
    let mut used = Vec::new();
    let mut rolls = [0.75, 0.25, 0.75, 0.25].into_iter();

    assert!(intercept_incoming_missile(&mut defenders, &mut used, 101, || {
        rolls.next().unwrap()
    }));
    assert_eq!(used, [1, 2]);
    assert_eq!(defenders.iter().map(|unit| unit.shots.len()).collect::<Vec<_>>(), [1, 1, 0, 0]);
    assert!(defenders[..2]
        .iter()
        .flat_map(|unit| &unit.shots)
        .all(|shot| shot.target_id == Some(101)));

    assert!(intercept_incoming_missile(&mut defenders, &mut used, 102, || {
        rolls.next().unwrap()
    }));
    assert_eq!(used, [1, 2, 3, 4]);
    assert_eq!(defenders.iter().map(|unit| unit.shots.len()).collect::<Vec<_>>(), [1, 1, 1, 1]);
    assert!(defenders[2..]
        .iter()
        .flat_map(|unit| &unit.shots)
        .all(|shot| shot.target_id == Some(102)));

    assert!(!intercept_incoming_missile(&mut defenders, &mut used, 103, || {
        panic!("no roll should be made after every interceptor has been used")
    }));
}

#[test]
fn combined_defense_casualties_retain_their_exact_owner() {
    let mut destination = Planet::new(1, "Protected".into(), Vec2::X, false, 1.0);
    destination.owned = Some(2);
    destination.controlled = Some(2);
    destination.army = Army::from([(Unit::probe(), 1)]).into();
    destination.protection_permissions.insert(3);
    destination.army.dock_protector(3, Army::from([(Unit::probe(), 1)]));
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.owned = Some(1);
    origin.controlled = Some(1);
    let mut mission = Mission::new_with_id(
        2,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Cruiser), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.position = destination.position;

    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([12; 32]),
    );
    let combat = report.combat_report.unwrap();
    let target_owners = combat
        .rounds
        .iter()
        .flat_map(|round| &round.defender)
        .map(|unit| (unit.id, unit.owner))
        .collect::<std::collections::HashMap<_, _>>();
    let killed_owners = combat
        .rounds
        .iter()
        .flat_map(|round| &round.attacker)
        .flat_map(|unit| &unit.shots)
        .filter(|shot| shot.killed)
        .filter_map(|shot| shot.target_id)
        .filter_map(|target| target_owners.get(&target).copied().flatten())
        .collect::<std::collections::BTreeSet<_>>();

    assert_eq!(killed_owners, std::collections::BTreeSet::from([2, 3]));
    assert!(report.surviving_defender.protector(3).is_none());
}

#[test]
fn overloaded_planetary_shield_enters_combat_with_its_level_scaled_bonus() {
    let mut destination = Planet::new(1, "Shield world".into(), Vec2::X, false, 1.0);
    destination.owned = Some(2);
    destination.controlled = Some(2);
    destination.army.insert(Unit::planetary_shield(), 5);
    destination.army.insert(Unit::Defense(crate::core::units::defense::Defense::RocketLauncher), 1);
    destination.shield_overload = crate::core::map::planet::ShieldOverloadState::Overloaded;
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.owned = Some(1);
    origin.controlled = Some(1);
    let mut mission = Mission::new_with_id(
        8,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Cruiser), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.position = destination.position;

    let report = resolve_combat_with_energy_with_rng(
        1,
        &mission,
        &destination,
        EnergyGrid {
            supply: 10,
            demand: 10,
        },
        &mut rand_chacha::ChaCha8Rng::from_seed([10; 32]),
    );
    let first = &report.combat_report.unwrap().rounds[0];
    let absorbed = first
        .attacker
        .iter()
        .flat_map(|unit| &unit.shots)
        .map(|shot| shot.planetary_shield_damage)
        .sum::<usize>();

    assert_eq!(first.planetary_shield + absorbed, 2_250);
}

#[test]
fn ship_fire_never_reaches_ground_defenses_while_the_planetary_shield_remains() {
    let mut destination = Planet::new(1, "Shield world".into(), Vec2::X, false, 1.0);
    destination.owned = Some(2);
    destination.controlled = Some(2);
    destination.army = Army::from([
        (Unit::planetary_shield(), 5),
        (Unit::crawler(), 8),
        (Unit::repair_truck(), 8),
        (Unit::Defense(crate::core::units::defense::Defense::RocketLauncher), 12),
        (Unit::Defense(crate::core::units::defense::Defense::GaussCannon), 12),
        (Unit::Defense(crate::core::units::defense::Defense::PlasmaTurret), 12),
    ])
    .into();
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.owned = Some(1);
    origin.controlled = Some(1);
    let attackers =
        Unit::ships().into_iter().filter(|unit| unit.damage() > 0).map(|unit| (unit, 6)).collect();
    let mut mission = Mission::new_with_id(
        9,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        attackers,
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.position = destination.position;

    let report = resolve_combat_with_rng(
        1,
        &mission,
        &destination,
        &mut rand_chacha::ChaCha8Rng::from_seed([11; 32]),
    );
    let combat = report.combat_report.unwrap();
    let mut shield = EnergyGrid::default().planetary_shield(5, false);
    let mut saw_shield_hit = false;
    let mut saw_defense_target = false;
    for round in combat.rounds {
        for shooter in round.attacker {
            for shot in shooter.shots {
                if shot.planetary_shield_damage > 0 {
                    saw_shield_hit = true;
                    assert!(shield > 0);
                    shield = shield.saturating_sub(shot.planetary_shield_damage);
                }
                if shooter.unit.is_ship()
                    && shot.unit.is_some_and(|unit| {
                        unit.is_defense() && !unit.is_missile() && unit != Unit::space_dock()
                    })
                {
                    saw_defense_target = true;
                    assert_eq!(shield, 0, "a ship targeted a protected ground defense");
                }
            }
        }
        assert_eq!(shield, round.planetary_shield);
    }
    assert!(saw_shield_hit && saw_defense_target, "fixture must cross the shield boundary");
}

#[test]
fn every_armed_ship_has_probe_strength_rapid_fire_against_crawlers() {
    for ship in Unit::ships().into_iter().filter(|unit| unit.damage() > 0) {
        assert_eq!(ship.rapid_fire().get(&Unit::crawler()), Some(&80), "{ship:?}");
        assert_eq!(
            ship.rapid_fire().get(&Unit::crawler()),
            ship.rapid_fire().get(&Unit::probe()),
            "{ship:?} must treat Crawlers like Probes"
        );
    }
}

#[test]
fn ships_and_missiles_match_crawler_rapid_fire_against_repair_trucks() {
    let attackers =
        Unit::ships().into_iter().chain(Unit::defenses().into_iter().filter(Unit::is_missile));
    let mut checked = 0;

    for attacker in attackers {
        let rapid_fire = attacker.rapid_fire();
        let Some(crawler_rapid_fire) = rapid_fire.get(&Unit::crawler()) else {
            continue;
        };

        checked += 1;
        assert_eq!(
            rapid_fire.get(&Unit::repair_truck()),
            Some(crawler_rapid_fire),
            "{attacker:?} must treat Repair Trucks like Crawlers"
        );
    }

    assert!(checked > 0, "fixture must include rapid fire against Crawlers");
}

fn salvage_report(surviving_crawlers: usize) -> MissionReport {
    let destination = Planet {
        id: 1,
        name: "Salvage".to_string(),
        kind: PlanetKind::Dry,
        image: 2,
        diameter: 10_000,
        temperature: (0, 10),
        position: Vec2::X,
        resources: Default::default(),
        jump_gate: 0,

        operations: Default::default(),
        terraformer_focus: Some(crate::core::resources::ResourceName::Metal),
        command_relay_active: true,
        shield_overload: Default::default(),
        fleet_withdrawal: Default::default(),
        is_destroyed: false,
        owned: Some(2),
        controlled: Some(2),
        independent_population: Default::default(),
        army: Army::from([
            (Unit::crawler(), surviving_crawlers + 10),
            (Unit::Defense(crate::core::units::defense::Defense::RocketLauncher), 10),
            (Unit::Defense(crate::core::units::defense::Defense::PlasmaTurret), 2),
            (Unit::space_dock(), 1),
            (Unit::antiballistic_missile(), 1),
        ])
        .into(),
        protection_permissions: Default::default(),
        buy: Vec::new(),
        surface_build_order: [None; 4],
    };
    let origin = Planet {
        id: 0,
        position: Vec2::ZERO,
        ..destination.clone()
    };
    let mission = Mission::new_with_id(
        7,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(crate::core::units::ships::Ship::Cruiser), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    MissionReport {
        id: 7,
        turn: 1,
        surviving_defender: Army::from([
            (Unit::crawler(), surviving_crawlers),
            (Unit::Defense(crate::core::units::defense::Defense::RocketLauncher), 4),
            (Unit::Defense(crate::core::units::defense::Defense::PlasmaTurret), 1),
        ])
        .into(),
        destination_owned: Some(2),
        destination_controlled: Some(2),
        combat_report: Some(CombatReport::default()),
        ..crate::test_support::empty_report(mission, destination)
    }
}

#[test]
fn crawler_salvage_is_component_wise_capped_and_requires_a_defender_win() {
    // Ten Crawlers, six Rocket Launchers, and one Plasma Turret were destroyed. The
    // Space Dock and missile are orbitals/ammunition and therefore not salvageable.
    assert_eq!(salvage_report(5).defender_salvage(), Resources::new(29, 6, 5));
    assert_eq!(salvage_report(50).defender_salvage(), Resources::new(290, 65, 55));
    assert_eq!(salvage_report(100).defender_salvage(), Resources::new(290, 65, 55));

    let mut draw = salvage_report(100);
    draw.surviving_attacker.insert(Unit::Ship(crate::core::units::ships::Ship::Cruiser), 1);
    assert!(draw.is_stalemate());
    assert_eq!(draw.defender_salvage(), Resources::default());

    let mut large = salvage_report(50);
    large
        .planet
        .army
        .insert(Unit::Defense(crate::core::units::defense::Defense::RocketLauncher), usize::MAX);
    assert_eq!(large.defender_salvage().metal, usize::MAX / 2);
}
