use rand::SeedableRng;

use super::*;
use crate::core::map::planet::PlanetKind;
use crate::core::resources::Resources;

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
        terraformer_focus: Some(crate::core::resources::ResourceName::Metal),
        command_relay_active: true,
        fleet_withdrawal: Default::default(),
        is_destroyed: false,
        owned: Some(2),
        controlled: Some(2),
        army: Army::from([(Unit::probe(), 1)]),
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
        terraformer_focus: Some(crate::core::resources::ResourceName::Metal),
        command_relay_active: true,
        fleet_withdrawal: Default::default(),
        is_destroyed: false,
        owned: Some(2),
        controlled: Some(2),
        army: Army::from([
            (Unit::crawler(), surviving_crawlers + 10),
            (Unit::Defense(crate::core::units::defense::Defense::RocketLauncher), 10),
            (Unit::Defense(crate::core::units::defense::Defense::PlasmaTurret), 2),
            (Unit::space_dock(), 1),
            (Unit::antiballistic_missile(), 1),
        ]),
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
        mission,
        planet: destination,
        scout_probes: 0,
        surviving_attacker: Army::new(),
        surviving_defender: Army::from([
            (Unit::crawler(), surviving_crawlers),
            (Unit::Defense(crate::core::units::defense::Defense::RocketLauncher), 4),
            (Unit::Defense(crate::core::units::defense::Defense::PlasmaTurret), 1),
        ]),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: Some(2),
        destination_controlled: Some(2),
        combat_report: Some(CombatReport::default()),
        hidden: false,
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
