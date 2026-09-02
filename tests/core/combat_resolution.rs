use rand::SeedableRng;

use super::*;
use crate::core::map::planet::PlanetKind;

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
        is_destroyed: false,
        owned: Some(2),
        controlled: Some(2),
        army: Army::from([(Unit::probe(), 1)]),
        buy: Vec::new(),
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
    assert!(!rapid_fire_stops(&attacker, &target, MAX_SHOTS_PER_UNIT_PER_ROUND - 1, 0.999,));
    assert!(rapid_fire_stops(&attacker, &target, MAX_SHOTS_PER_UNIT_PER_ROUND, 0.999,));
}
