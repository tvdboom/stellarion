use super::*;
use crate::core::combat::report::{CombatReport, DefenderRetreat, RetreatingFleet};
use crate::core::combat::resolution::{resolve_combat_with_rng, CombatUnit};
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::random::DeterministicRngState;
use crate::core::units::{
    buildings::Building, defense::Defense, ships::Ship, Amount, Army, Combat,
};
use bevy::prelude::Vec2;

fn record(id: u64, owner: PlayerId, unit: Unit) -> CombatUnit {
    CombatUnit {
        id,
        owner: Some(owner),
        unit,
        hull: unit.hull(),
        shield: unit.shield(),
        repairs: Vec::new(),
        shots: Vec::new(),
    }
}

fn report(rounds: Vec<RoundReport>) -> MissionReport {
    let mut planet = Planet::new(1, "Target".into(), Vec2::ZERO, false, 1.0);
    planet.colonize(2);
    // Synthetic reports specify their entire visible garrison explicitly. Colonization's
    // starter mines belong in the real-resolver fixtures, not these combatant-only scenarios.
    planet.army = Army::new().into();
    let mut report = crate::test_support::empty_report(Mission::default(), planet);
    report.combat_report = Some(CombatReport {
        rounds,
        defender_retreat: None,
    });
    report
}

fn actor_index(movie: &CinematicTimeline, defender: bool, id: u64) -> usize {
    movie
        .actors
        .iter()
        .position(|actor| actor.id == Some(id) && (actor.side == Side::Defender) == defender)
        .unwrap()
}

#[test]
fn mutual_kills_wait_for_recorded_launches_and_keep_post_kill_misses() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut attacker = record(12, 1, fighter);
    let mut defender = record(12, 2, fighter);
    for combatant in [&mut attacker, &mut defender] {
        combatant.hull = 0;
        combatant.shield = 0;
        combatant.shots = vec![
            ShotReport {
                target_id: Some(12),
                unit: Some(fighter),
                hull_damage: fighter.hull(),
                shield_damage: fighter.shield(),
                killed: true,
                ..Default::default()
            },
            ShotReport {
                target_id: Some(12),
                unit: Some(fighter),
                missed: true,
                ..Default::default()
            },
        ];
    }
    let movie = CinematicTimeline::new(&report(vec![RoundReport {
        attacker: vec![attacker],
        defender: vec![defender],
        ..Default::default()
    }]));
    assert_eq!(movie.actors.len(), 2);
    assert_eq!(movie.shots.len(), 4);
    for (index, actor) in movie.actors.iter().enumerate() {
        let death = actor.death_at.unwrap();
        assert!(movie
            .shots
            .iter()
            .filter(|shot| shot.source == index)
            .all(|shot| shot.launch_at < death));
        let kill = movie
            .shots
            .iter()
            .find(|shot| shot.target == Some(index) && shot.outcome.killed)
            .unwrap();
        let miss = movie
            .shots
            .iter()
            .find(|shot| shot.target == Some(index) && shot.outcome.missed)
            .unwrap();
        assert!(kill.impact_at < miss.impact_at);
        assert_eq!(actor.state_at(movie.duration).hull, 0);
        assert_eq!(actor.state_at(0.0).hull, fighter.hull());
    }
}

#[test]
fn shared_shield_collapses_before_ground_damage_and_repairs_accumulate() {
    let turret = Unit::Defense(Defense::HeavyLaser);
    let mut attacking = record(40, 1, Unit::Ship(Ship::Bomber));
    attacking.shots = vec![
        ShotReport {
            unit: Some(Unit::planetary_shield()),
            planetary_shield_damage: 100,
            ..Default::default()
        },
        ShotReport {
            target_id: Some(90),
            unit: Some(turret),
            shield_damage: turret.shield(),
            hull_damage: 100,
            ..Default::default()
        },
    ];
    let fighter = Unit::Ship(Ship::LightFighter);
    attacking.shots.push(ShotReport {
        target_id: Some(93),
        unit: Some(fighter),
        shield_damage: fighter.shield(),
        hull_damage: fighter.hull(),
        killed: true,
        ..Default::default()
    });
    attacking.shots.extend((0..15).map(|_| ShotReport {
        target_id: Some(93),
        unit: Some(fighter),
        missed: true,
        ..Default::default()
    }));
    let mut casualty = record(93, 2, fighter);
    casualty.hull = 0;
    casualty.shield = 0;
    let mut defender = record(90, 2, turret);
    defender.hull = turret.hull() - 100 + 60;
    defender.shield = 0;
    defender.repairs = vec![40, 20];
    let battle = report(vec![RoundReport {
        attacker: vec![attacking],
        defender: vec![
            defender,
            record(91, 2, Unit::repair_truck()),
            record(92, 2, Unit::repair_truck()),
            casualty,
        ],
        ..Default::default()
    }]);
    let movie = CinematicTimeline::new(&battle);
    let shield = movie.shots.iter().find(|shot| shot.outcome.planetary_shield_damage > 0).unwrap();
    let damage = movie.shots.iter().find(|shot| shot.outcome.hull_damage > 0).unwrap();
    assert!(shield.impact_at < damage.impact_at);
    assert_eq!(movie.initial_planetary_shield, 100);
    assert_eq!(movie.planetary_shield_at(shield.impact_at - 0.01), 100);
    assert_eq!(movie.planetary_shield_at(shield.impact_at), 0);
    let actor = &movie.actors[actor_index(&movie, true, 90)];
    assert_eq!(actor.state_at(damage.impact_at).hull, turret.hull() - 100);
    assert_eq!(movie.repairs.len(), 2);
    let last_impact = movie.shots.iter().map(|shot| shot.impact_at).fold(0.0_f32, f32::max);
    assert!(movie.repairs[0].start_at < last_impact);
    assert!(movie.repairs.iter().all(|repair| repair.start_at > damage.impact_at));
    assert_ne!(movie.repairs[0].source, movie.repairs[1].source);
    assert_eq!(movie.repairs.iter().map(|repair| repair.amount).sum::<usize>(), 60);
    assert_eq!(actor.state_at(movie.repairs[0].end_at).hull, turret.hull() - 60);
    assert_eq!(actor.state_at(movie.repairs[1].end_at).hull, turret.hull() - 40);
    assert_eq!(actor.state_at(movie.duration).hull, turret.hull() - 40);
}

#[test]
fn reordered_survivors_keep_hull_and_regenerate_only_shields() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut first = record(5, 1, fighter);
    first.hull = fighter.hull() - 10;
    first.shield = 0;
    let second = record(9, 1, fighter);
    let mut next = first.clone();
    next.shield = fighter.shield();
    let movie = CinematicTimeline::new(&report(vec![
        RoundReport {
            attacker: vec![first.clone(), second.clone()],
            ..Default::default()
        },
        RoundReport {
            attacker: vec![second, next],
            ..Default::default()
        },
    ]));
    let actor = &movie.actors[actor_index(&movie, false, 5)];
    assert_eq!(movie.actors.len(), 2);
    assert_eq!(
        actor.state_at(movie.duration),
        CinematicActorState {
            hull: first.hull,
            shield: fighter.shield()
        }
    );
    assert!(actor.states.windows(2).all(|pair| pair[0].0 <= pair[1].0));
    assert!(actor.death_at.is_none());
}

#[test]
fn retreat_moves_only_commanders_survivors_and_includes_colony_support() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut battle = report(vec![RoundReport {
        attacker: vec![record(1, 1, fighter)],
        defender: vec![record(2, 2, fighter), record(3, 3, fighter)],
        ..Default::default()
    }]);
    battle.combat_report.as_mut().unwrap().defender_retreat = Some(DefenderRetreat {
        after_round: Some(0),
        home_planet: 2,
        ships: Army::from([(fighter, 1), (Unit::colony_ship(), 1)]),
        fleets: Default::default(),
    });
    let movie = CinematicTimeline::new(&battle);
    let defender = &movie.actors[actor_index(&movie, true, 2)];
    let protector = &movie.actors[actor_index(&movie, true, 3)];
    let colony = movie.actors.iter().find(|actor| actor.unit == Unit::colony_ship()).unwrap();
    assert_eq!(defender.retreat_at, colony.retreat_at);
    assert!(defender.retreat_at.is_some());
    assert!(protector.retreat_at.is_none());
    assert!(movie.actors.iter().all(|actor| actor.death_at.is_none()));
}

#[test]
fn collective_retreat_moves_controller_and_protector_survivors() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut battle = report(vec![RoundReport {
        attacker: vec![record(1, 1, fighter)],
        defender: vec![record(2, 2, fighter), record(3, 3, fighter)],
        ..Default::default()
    }]);
    battle.combat_report.as_mut().unwrap().defender_retreat = Some(DefenderRetreat {
        after_round: Some(0),
        home_planet: 20,
        ships: Army::from([(fighter, 2)]),
        fleets: std::collections::BTreeMap::from([
            (
                2,
                RetreatingFleet {
                    home_planet: 20,
                    ships: Army::from([(fighter, 1)]),
                },
            ),
            (
                3,
                RetreatingFleet {
                    home_planet: 30,
                    ships: Army::from([(fighter, 1)]),
                },
            ),
        ]),
    });

    let movie = CinematicTimeline::new(&battle);

    let controller = &movie.actors[actor_index(&movie, true, 2)];
    let protector = &movie.actors[actor_index(&movie, true, 3)];
    assert_eq!(controller.retreat_at, protector.retreat_at);
    assert!(controller.retreat_at.is_some());
}

#[test]
fn immediate_retreat_has_individual_living_ships_without_invented_shots() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut battle = report(vec![RoundReport::default()]);
    battle.combat_report.as_mut().unwrap().defender_retreat = Some(DefenderRetreat {
        after_round: None,
        home_planet: 2,
        ships: Army::from([(fighter, 3), (Unit::colony_ship(), 1)]),
        fleets: Default::default(),
    });
    let movie = CinematicTimeline::new(&battle);
    assert_eq!(movie.actors.len(), 4);
    assert!(movie.shots.is_empty());
    assert!(movie.actors.iter().all(|actor| actor
        .retreat_at
        .is_some_and(|at| at < movie.entrance_duration)
        && actor.death_at.is_none()
        && actor.state_at(movie.duration).hull > 0));
}

#[test]
fn planet_destruction_follows_only_recorded_success_and_removes_orbital_scenery() {
    let round = RoundReport {
        attacker: vec![record(1, 1, Unit::war_sun())],
        defender: vec![record(2, 2, Unit::Defense(Defense::HeavyLaser))],
        destroy_probability: 0.4,
        ..Default::default()
    };
    let mut battle = report(vec![round.clone(), round]);
    battle.planet.army.insert(Unit::Building(Building::SolarSatellite), 2);
    battle.planet_destroyed = true;
    let movie = CinematicTimeline::new(&battle);
    assert_eq!(movie.planet_attacks.len(), 2);
    assert!(!movie.planet_attacks[0].destroyed);
    assert!(movie.planet_attacks[1].destroyed);
    let destroyed_at = movie.planet_attacks[1].end_at;
    assert_eq!(movie.actors.iter().filter(|actor| actor.side == Side::Defender).count(), 3);
    for actor in movie.actors.iter().filter(|actor| actor.side == Side::Defender) {
        assert_eq!(actor.death_at, Some(destroyed_at));
        assert!(actor.state_at(destroyed_at - 0.01).hull > 0);
        assert_eq!(actor.state_at(destroyed_at).hull, 0);
    }
    battle.planet_destroyed = false;
    let unsuccessful = CinematicTimeline::new(&battle);
    assert!(unsuccessful.planet_attacks.iter().all(|attack| !attack.destroyed));
    assert!(unsuccessful.actors.iter().all(|actor| actor.death_at.is_none()));
}

#[test]
fn missiles_explode_only_for_recorded_intercepts_and_used_launchers_depart() {
    let missile = Unit::interplanetary_missile();
    let mut interceptor = record(2, 2, Unit::antiballistic_missile());
    interceptor.shots = vec![ShotReport {
        target_id: Some(1),
        unit: Some(missile),
        killed: true,
        ..Default::default()
    }];
    let movie = CinematicTimeline::new(&report(vec![RoundReport {
        attacker: vec![record(1, 1, missile), record(3, 1, missile)],
        defender: vec![interceptor],
        antiballistic_fired: 1,
        ..Default::default()
    }]));
    let incoming = &movie.actors[actor_index(&movie, false, 1)];
    let interceptor = &movie.actors[actor_index(&movie, true, 2)];
    assert_eq!(incoming.death_at, Some(movie.shots[0].impact_at));
    assert_eq!(incoming.state_at(movie.duration).hull, 0);
    assert_eq!(interceptor.retreat_at, Some(movie.shots[0].impact_at));
    assert!(interceptor.death_at.is_none());
    let without_target = &movie.actors[actor_index(&movie, false, 3)];
    assert!(without_target.death_at.is_none());
    assert!(without_target.retreat_at.is_some());
    assert_eq!(movie.shots.len(), 1);
}

#[test]
fn resolved_battle_replays_every_shot_and_is_seekable_without_new_randomness() {
    let mut rng = DeterministicRngState::from_u64(731).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
    origin.colonize(1);
    let mut planet = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1.0, &mut rng);
    planet.colonize(2);
    planet.army = Army::from([
        (Unit::Defense(Defense::HeavyLaser), 8),
        (Unit::repair_truck(), 3),
        (Unit::space_dock(), 1),
        (Unit::planetary_shield(), 1),
    ])
    .into();
    let mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &planet,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Cruiser), 4), (Unit::Ship(Ship::LightFighter), 10)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let battle = resolve_combat_with_rng(1, &mission, &planet, &mut rng);
    let movie = CinematicTimeline::new(&battle);
    let repeated = CinematicTimeline::new(&battle);
    let combat = battle.combat_report.as_ref().unwrap();
    let expected = combat
        .rounds
        .iter()
        .flat_map(|round| round.attacker.iter().chain(&round.defender))
        .map(|unit| unit.shots.len())
        .sum::<usize>();
    assert_eq!(movie.shots.len(), expected);
    assert_eq!(movie.duration, repeated.duration);
    for (round_index, snapshot) in combat.rounds.iter().enumerate() {
        let mut prefix = battle.clone();
        prefix.combat_report.as_mut().unwrap().rounds.truncate(round_index + 1);
        let prefix = CinematicTimeline::new(&prefix);
        // Sample before the closing hold and boundary checkpoint,
        // after all actual impacts/heals, so a corrective snapshot cannot hide damage mistakes.
        let boundary = prefix.duration - CLOSING_HOLD - 0.135;
        for (defender, army) in [(false, &snapshot.attacker), (true, &snapshot.defender)] {
            for record in army {
                let actor = &movie.actors[actor_index(&movie, defender, record.id)];
                assert_eq!(
                    actor.state_at(boundary),
                    CinematicActorState {
                        hull: record.hull,
                        shield: record.shield
                    },
                    "Actor {} at the end of recorded round {round_index}",
                    record.id,
                );
            }
        }
    }
    assert_eq!(
        movie
            .shots
            .iter()
            .map(|shot| (shot.source, shot.target, shot.launch_at, shot.impact_at))
            .collect::<Vec<_>>(),
        repeated
            .shots
            .iter()
            .map(|shot| (shot.source, shot.target, shot.launch_at, shot.impact_at))
            .collect::<Vec<_>>()
    );
    for (index, actor) in movie.actors.iter().enumerate() {
        assert!(actor.states.windows(2).all(|pair| pair[0].0 <= pair[1].0));
        let end = actor.state_at(movie.duration);
        assert_eq!(end, actor.state_at(movie.duration));
        assert_eq!(actor.state_at(0.0).hull, actor.max_hull);
        if let Some(death) = actor.death_at {
            assert_eq!(end.hull, 0);
            assert!(movie
                .shots
                .iter()
                .filter(|shot| shot.source == index)
                .all(|shot| shot.launch_at < death));
        } else {
            let record = combat
                .rounds
                .last()
                .unwrap()
                .units(&actor.side)
                .iter()
                .find(|record| Some(record.id) == actor.id)
                .unwrap();
            assert_eq!(
                end,
                CinematicActorState {
                    hull: record.hull,
                    shield: record.shield
                }
            );
        }
    }
}

#[test]
fn stalemate_flies_the_attacker_away_and_keeps_the_defender() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut battle = report(vec![RoundReport {
        attacker: vec![record(1, 1, fighter)],
        defender: vec![record(2, 2, fighter)],
        ..Default::default()
    }]);
    battle.mission.objective = Icon::Attack;
    battle.surviving_attacker = Army::from([(fighter, 1)]);
    battle.surviving_defender = Army::from([(fighter, 1)]).into();
    assert!(battle.is_stalemate());
    let movie = CinematicTimeline::new(&battle);
    assert!(movie.actors.iter().all(|actor| actor.death_at.is_none()));
    assert!(movie
        .actors
        .iter()
        .find(|actor| actor.side == Side::Attacker)
        .unwrap()
        .retreat_at
        .is_some());
    assert!(movie
        .actors
        .iter()
        .find(|actor| actor.side == Side::Defender)
        .unwrap()
        .retreat_at
        .is_none());
    assert!(movie.planet_attacks.is_empty());
}

#[test]
fn fauna_stalemate_flies_both_surviving_sides_away() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let fauna = Unit::Fauna(crate::core::units::fauna::SpaceFauna::VoidManta);
    let mut battle = report(vec![RoundReport {
        attacker: vec![record(1, 1, fighter)],
        defender: vec![record(2, 0, fauna)],
        ..Default::default()
    }]);
    battle.mission.objective = Icon::Attack;
    battle.planet.army.insert(fauna, 1);
    battle.surviving_attacker = Army::from([(fighter, 1)]);
    battle.surviving_defender = Army::from([(fauna, 1)]).into();
    assert!(battle.is_stalemate());

    let movie = CinematicTimeline::new(&battle);
    assert!(movie
        .actors
        .iter()
        .filter(|actor| actor.death_at.is_none())
        .all(|actor| actor.retreat_at.is_some()));
}

#[test]
fn ground_bombs_target_one_building_per_type_and_only_remove_recorded_levels() {
    let mine = Unit::Building(Building::MetalMine);
    let factory = Unit::Building(Building::Factory);
    let targets = [(mine, true), (mine, false), (mine, true), (factory, true)];
    let attackers = targets
        .iter()
        .enumerate()
        .map(|(index, (unit, hit))| {
            let mut bomber = record(index as u64, 1, Unit::Ship(Ship::Bomber));
            bomber.shots.push(ShotReport {
                unit: Some(*unit),
                killed: *hit,
                missed: !hit,
                ..Default::default()
            });
            bomber
        })
        .collect();
    let mut battle = report(vec![RoundReport {
        attacker: attackers,
        ..Default::default()
    }]);
    for unit in Unit::resource_buildings().into_iter().chain(Unit::industrial_buildings()) {
        battle.planet.army.insert(
            unit,
            if unit == factory {
                1
            } else {
                3
            },
        );
    }
    let movie = CinematicTimeline::new(&battle);
    assert_eq!(movie.actors.iter().filter(|actor| actor.initial_levels.is_some()).count(), 6);
    assert_eq!(movie.level_losses.len(), 3, "A miss must not create a level-loss caption");
    for shot in &movie.shots {
        let target =
            &movie.actors[shot.target.expect("A recorded building has an exact visible target")];
        assert_eq!(Some(target.unit), shot.outcome.unit);
    }
    let mine = movie.actors.iter().find(|actor| actor.unit == mine).unwrap();
    assert_eq!(mine.levels_at(0.0), Some(3));
    assert_eq!(mine.levels_at(movie.duration), Some(1));
    assert!(mine.death_at.is_none(), "Losing one level does not destroy the whole building");
    for loss in &movie.level_losses {
        let target = &movie.actors[loss.target];
        assert_eq!(target.levels_at(loss.impact_at - 0.001), Some(loss.remaining_levels + 1));
        assert_eq!(target.levels_at(loss.impact_at), Some(loss.remaining_levels));
        assert_eq!(target.death_at, (loss.remaining_levels == 0).then_some(loss.impact_at));
    }
    let factory = movie.actors.iter().find(|actor| actor.unit == factory).unwrap();
    assert_eq!(factory.levels_at(movie.duration), Some(0));
    assert_eq!(factory.state_at(movie.duration).hull, 0);
    assert_eq!(mine.levels_at(0.0), Some(3), "Seeking backwards restores the original levels");
}

#[test]
fn bombing_overlaps_unrelated_fire_but_waits_for_own_volley_and_shield_breach() {
    let mine = Unit::Building(Building::MetalMine);
    let mut bomber = record(1, 1, Unit::Ship(Ship::Bomber));
    bomber.shots = vec![
        ShotReport {
            unit: Some(Unit::planetary_shield()),
            planetary_shield_damage: 100,
            ..Default::default()
        },
        ShotReport {
            unit: Some(mine),
            killed: true,
            ..Default::default()
        },
    ];
    let mut cruiser = record(3, 1, Unit::Ship(Ship::Cruiser));
    cruiser.shots = (0..24)
        .map(|_| ShotReport {
            missed: true,
            ..Default::default()
        })
        .collect();
    let mut battle = report(vec![RoundReport {
        attacker: vec![bomber, cruiser],
        ..Default::default()
    }]);
    battle.planet.army.insert(mine, 2);
    let movie = CinematicTimeline::new(&battle);
    let bomb = movie.shots.iter().find(|shot| shot.outcome.is_bombing()).unwrap();
    let breach = movie.shots.iter().find(|shot| shot.outcome.planetary_shield_damage > 0).unwrap();
    assert!(bomb.launch_at > breach.impact_at);
    assert_eq!(movie.planetary_shield_at(bomb.launch_at), 0);
    assert!(movie
        .shots
        .iter()
        .filter(|shot| shot.source == bomb.source && !shot.outcome.is_bombing())
        .all(|shot| shot.launch_at < bomb.launch_at));
    assert!(
        movie
            .shots
            .iter()
            .any(|shot| !shot.outcome.is_bombing() && shot.impact_at > bomb.launch_at),
        "Bombs should fly while unrelated salvos are still in progress"
    );
}

#[test]
fn resolver_bombing_levels_and_misses_match_the_saved_surviving_garrison() {
    for (seed, raid) in [(731, BombingRaid::Economic), (913, BombingRaid::Industrial)] {
        let mut rng = DeterministicRngState::from_u64(seed).next_rng();
        let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
        origin.colonize(1);
        let mut planet = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1.0, &mut rng);
        planet.colonize(2);
        let buildings: Vec<_> =
            Unit::resource_buildings().into_iter().chain(Unit::industrial_buildings()).collect();
        let mut defenders: Army = buildings.iter().copied().zip([1, 3, 5, 1, 3, 5]).collect();
        defenders.insert(Unit::Defense(Defense::RocketLauncher), 4);
        defenders.insert(Unit::planetary_shield(), 1);
        planet.army = defenders.into();
        let mission = Mission::new_with_id(
            1,
            1,
            1,
            &origin,
            &planet,
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::Bomber), 36)]),
            raid,
            false,
            false,
            None,
        );
        let battle = resolve_combat_with_rng(1, &mission, &planet, &mut rng);
        let movie = CinematicTimeline::new(&battle);
        let hits = movie
            .shots
            .iter()
            .filter(|shot| shot.outcome.is_bombing() && shot.outcome.killed && !shot.outcome.missed)
            .count();
        assert!(hits > 0, "Seeded resolver fixture must exercise actual level loss");
        assert_eq!(hits, movie.level_losses.len());
        for unit in buildings {
            let actor = movie.actors.iter().find(|actor| actor.unit == unit).unwrap();
            assert_eq!(actor.levels_at(0.0), Some(battle.planet.army.amount(&unit)));
            assert_eq!(
                actor.levels_at(movie.duration),
                Some(battle.surviving_defender.amount(&unit))
            );
            assert_eq!(actor.death_at.is_some(), battle.surviving_defender.amount(&unit) == 0);
            assert!(actor
                .levels
                .windows(2)
                .all(|pair| pair[0].0 < pair[1].0 && pair[0].1 == pair[1].1 + 1));
        }
        let barrier = movie
            .shots
            .iter()
            .filter(|shot| shot.outcome.planetary_shield_damage > 0)
            .map(|shot| shot.impact_at)
            .fold(0.0_f32, f32::max);
        assert!(movie
            .shots
            .iter()
            .filter(|shot| shot.outcome.is_bombing())
            .all(|shot| shot.launch_at > barrier));
    }
}

#[test]
fn unbreached_shield_and_failed_bombs_preserve_surface_buildings() {
    let mine = Unit::Building(Building::MetalMine);
    for missed_bomb in [false, true] {
        let mut bomber = record(1, 1, Unit::Ship(Ship::Bomber));
        bomber.shots.push(if missed_bomb {
            ShotReport {
                unit: Some(mine),
                missed: true,
                ..Default::default()
            }
        } else {
            ShotReport {
                unit: Some(Unit::planetary_shield()),
                planetary_shield_damage: 20,
                ..Default::default()
            }
        });
        let mut battle = report(vec![RoundReport {
            attacker: vec![bomber],
            planetary_shield: if missed_bomb {
                0
            } else {
                80
            },
            ..Default::default()
        }]);
        battle.planet.army.insert(mine, 3);
        let movie = CinematicTimeline::new(&battle);
        let mine = movie.actors.iter().find(|actor| actor.unit == mine).unwrap();
        assert_eq!(mine.levels_at(movie.duration), Some(3));
        assert!(mine.death_at.is_none());
        assert!(movie.level_losses.is_empty());
    }
}

#[test]
fn planet_strikes_overlap_recorded_covering_fire_without_changing_the_outcome() {
    let mut sun = record(1, 1, Unit::war_sun());
    sun.shots.push(ShotReport {
        target_id: Some(20),
        unit: Some(Unit::Defense(Defense::RocketLauncher)),
        missed: true,
        ..Default::default()
    });
    let mut attacker = vec![sun];
    let mut defender = Vec::new();
    for id in 2..8 {
        let mut escort = record(id, 1, Unit::Ship(Ship::LightFighter));
        let mut turret = record(id + 18, 2, Unit::Defense(Defense::RocketLauncher));
        for _ in 0..10 {
            escort.shots.push(ShotReport {
                target_id: Some(turret.id),
                unit: Some(turret.unit),
                missed: true,
                ..Default::default()
            });
            turret.shots.push(ShotReport {
                target_id: Some(escort.id),
                unit: Some(escort.unit),
                missed: true,
                ..Default::default()
            });
        }
        attacker.push(escort);
        defender.push(turret);
    }
    let round = RoundReport {
        attacker,
        defender,
        destroy_probability: 0.5,
        ..Default::default()
    };
    let mut sparse = round.clone();
    for unit in sparse.attacker.iter_mut().chain(&mut sparse.defender) {
        unit.shots.truncate(1);
    }
    for (round, expected_shots) in [(round, 242), (sparse, 26)] {
        for destroyed in [false, true] {
            let mut battle = report(vec![round.clone(), round.clone()]);
            battle.planet_destroyed = destroyed;
            let movie = CinematicTimeline::new(&battle);
            assert_eq!(
                movie.shots.len(),
                expected_shots,
                "Do not invent fire to fill the planet strike"
            );
            for attack in &movie.planet_attacks {
                for side in [Side::Attacker, Side::Defender] {
                    assert!(
                        movie.shots.iter().any(|shot| {
                            movie.actors[shot.source].side == side
                                && shot.launch_at > attack.discharge_at
                                && shot.impact_at < attack.end_at
                        }),
                        "Both sides should still fire while the planet beam is active"
                    );
                }
                assert!(
                    movie
                        .shots
                        .iter()
                        .filter(|shot| attack.sources.contains(&shot.source))
                        .all(|shot| shot.launch_at < attack.start_at
                            || shot.launch_at > attack.end_at)
                );
            }
            let collapse = movie.planet_attacks.last().unwrap().end_at;
            for actor in &movie.actors {
                if destroyed && actor.side == Side::Defender {
                    assert_eq!(actor.death_at, Some(collapse));
                } else {
                    assert!(actor.death_at.is_none());
                    assert_eq!(actor.state_at(movie.duration).hull, actor.max_hull);
                }
            }
        }
    }
}

#[test]
fn surviving_war_suns_combine_once_per_attempt_and_preserve_failed_outcomes() {
    let mut dead = record(3, 1, Unit::war_sun());
    dead.hull = 0;
    let first = RoundReport {
        attacker: vec![record(1, 1, Unit::war_sun()), record(2, 3, Unit::war_sun()), dead],
        destroy_probability: 0.5,
        ..Default::default()
    };
    let mut second = first.clone();
    second.attacker.retain(|unit| unit.hull > 0);
    let mut battle = report(vec![first, second]);
    battle.planet_destroyed = true;
    battle.planet.army.insert(Unit::Building(Building::Factory), 5);
    let movie = CinematicTimeline::new(&battle);
    assert_eq!(movie.planet_attacks.len(), 2, "Several War Suns share one recorded attempt");
    for attack in &movie.planet_attacks {
        let ids: Vec<_> =
            attack.sources.iter().map(|source| movie.actors[*source].id.unwrap()).collect();
        assert_eq!(ids, [1, 2]);
        assert!(attack.start_at < attack.discharge_at && attack.discharge_at < attack.end_at);
        assert!(attack.sources.iter().all(|source| movie.actors[*source].death_at.is_none()));
    }
    assert!(!movie.planet_attacks[0].destroyed);
    let final_attack = &movie.planet_attacks[1];
    assert!(final_attack.destroyed);
    assert!(movie.planet_attacks[0].end_at < final_attack.start_at);
    let factory =
        movie.actors.iter().find(|actor| actor.unit == Unit::Building(Building::Factory)).unwrap();
    assert_eq!(factory.levels_at(final_attack.end_at - 0.01), Some(5));
    assert_eq!(factory.levels_at(final_attack.end_at), Some(0));
    assert!(
        movie.level_losses.is_empty(),
        "Planet destruction is not a fictitious bombing level loss"
    );
    assert!(
        movie.duration >= final_attack.end_at + 3.8,
        "The actual breakup must finish before the result"
    );
    battle.planet_destroyed = false;
    let failed = CinematicTimeline::new(&battle);
    assert!(failed.planet_attacks.iter().all(|attack| !attack.destroyed));
    let factory =
        failed.actors.iter().find(|actor| actor.unit == Unit::Building(Building::Factory)).unwrap();
    assert_eq!(factory.levels_at(failed.duration), Some(5));
    assert!(factory.death_at.is_none());
}
