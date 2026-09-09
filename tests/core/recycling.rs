use super::*;
use crate::core::combat::report::{CombatReport, MissionReport};
use crate::core::map::icon::Icon;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::random::DeterministicRngState;
use crate::core::simulation::{GameModel, GameRules};
use crate::core::units::ships::Ship;
use crate::core::units::Army;

fn report(id: u64, turn: usize, planet: &Planet, losses: usize) -> MissionReport {
    MissionReport {
        id,
        turn,
        mission: Mission::new_with_id(
            id,
            turn.saturating_sub(1),
            1,
            planet,
            planet,
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::LightFighter), losses)]),
            BombingRaid::None,
            false,
            false,
            None,
        ),
        planet: planet.clone(),
        scout_probes: 0,
        surviving_attacker: Army::new(),
        surviving_defender: planet.army.clone(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: planet.owned,
        destination_controlled: planet.controlled,
        combat_report: Some(CombatReport::default()),
        hidden: false,
    }
}

#[test]
fn debris_size_controls_one_two_and_three_turn_lifetimes() {
    let model = GameModel::new([18; 32], GameRules::default()).unwrap();
    let planet = &model.map.planets[0];
    for (id, losses, size, lifetime) in
        [(1, 3, DebrisSize::Small, 1), (2, 4, DebrisSize::Medium, 2), (3, 16, DebrisSize::Large, 3)]
    {
        let battle = report(id, 8, planet, losses);
        assert_eq!(DebrisSize::from_losses(losses), Some(size));
        for age in 0..lifetime {
            assert!(debris_sites([&battle].into_iter(), 8 + age).contains_key(&planet.id));
        }
        assert!(!debris_sites([&battle].into_iter(), 8 + lifetime).contains_key(&planet.id));
    }
}

#[test]
fn recycler_prefers_debris_and_scales_output_by_level() {
    let mut model = GameModel::new([29; 32], GameRules::default()).unwrap();
    let asteroid_targets = recycler_asteroid_targets(&model.map);
    let target = model
        .map
        .planets()
        .into_iter()
        .find(|planet| asteroid_targets.contains_key(&planet.id))
        .map(|planet| planet.id)
        .expect("generated map should place at least one planet near its asteroid field");
    let planet = model.map.get_mut(target);
    planet.owned = Some(1);
    planet.controlled = Some(1);
    planet.army.insert(Unit::Building(Building::Recycler), 3);

    let mut asteroid_rng_state = DeterministicRngState::from_u64(91);
    let asteroid_outputs = (0..8)
        .map(|_| {
            let mut rng = asteroid_rng_state.next_rng();
            recycler_production(&model.map, &model.players, 1, 7, &mut rng)
        })
        .collect::<Vec<_>>();
    assert!(
        asteroid_outputs.iter().skip(1).any(|output| *output != asteroid_outputs[0]),
        "the persisted turn streams should vary the Recycler haul"
    );
    for output in asteroid_outputs {
        assert!(output >= RECYCLER_ASTEROID_OUTPUT_MIN * 3usize);
        assert!(output <= RECYCLER_ASTEROID_OUTPUT_MAX * 3usize);
    }

    let debris = report(77, 7, model.map.get(target), 4);
    model.players[0].push_report(debris);
    let mut first_rng = DeterministicRngState::from_u64(123).next_rng();
    let mut replay_rng = DeterministicRngState::from_u64(123).next_rng();
    let output = recycler_production(&model.map, &model.players, 1, 7, &mut first_rng);
    assert_eq!(
        output,
        recycler_production(&model.map, &model.players, 1, 7, &mut replay_rng),
        "the same persisted stream must reproduce the same haul"
    );
    assert!(output >= RECYCLER_DEBRIS_OUTPUT_MIN * 3usize);
    assert!(output <= RECYCLER_DEBRIS_OUTPUT_MAX * 3usize);
    assert!(RECYCLER_DEBRIS_OUTPUT_MIN > RECYCLER_ASTEROID_OUTPUT_MAX);
}
