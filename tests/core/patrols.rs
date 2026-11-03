use super::*;
use crate::core::simulation::{
    preview_commands, resolve_turn, GameRules, PersistedGame, TurnSubmission,
};
use crate::core::units::ships::Ship;
use rand::SeedableRng;

fn game() -> GameModel {
    let mut model = GameModel::new(
        [73; 32],
        GameRules {
            player_count: 2,
            space_fauna_percent: 0,
            independent_populations: false,
            ..GameRules::default()
        },
    )
    .unwrap();
    model.start().unwrap();
    for player in &mut model.players {
        player.resources = Resources::new(100_000, 100_000, 100_000);
    }
    model
}

fn center(model: &GameModel) -> Vec2 {
    let rect = model.map.rect;
    (1..20)
        .flat_map(|x| {
            (1..20)
                .map(move |y| rect.min + rect.size() * Vec2::new(x as f32 / 20.0, y as f32 / 20.0))
        })
        .find(|point| valid_patrol_target(&model.map, model.players[0].home_planet, *point, None))
        .unwrap()
}

fn stationed(
    model: &GameModel,
    id: u64,
    owner: PlayerId,
    point: Vec2,
    army: Army,
    turns: Option<usize>,
) -> Mission {
    let mut mission = Mission::new_patrol(
        id,
        model.turn as usize,
        owner,
        model.map.get(model.players[owner as usize - 1].home_planet),
        point,
        army,
        turns,
    );
    mission.position = point;
    mission.travel_turns = 1;
    mission.patrol.as_mut().unwrap().phase = PatrolPhase::OnStation;
    mission
}

fn empty_turn(model: &mut GameModel) {
    let submissions = model
        .players
        .iter()
        .filter(|player| !player.spectator)
        .map(|player| TurnSubmission::new(player.id, model.turn, vec![]))
        .collect::<Vec<_>>();
    resolve_turn(model, &submissions).unwrap();
}

#[test]
fn reinforcements_travel_merge_and_replay_with_the_existing_patrol_orders_and_upkeep() {
    use crate::core::combat::plan::{BattlePlan, CombatTactic};

    let mut model = game();
    let home = model.players[0].home_planet;
    let port = model.map.planets.iter().find(|planet| planet.owned.is_none()).unwrap().id;
    model.map.get_mut(port).owned = Some(1);
    model.map.get_mut(port).controlled = Some(1);
    let point = center(&model);
    let cruiser = Unit::Ship(Ship::Cruiser);
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut target = stationed(&model, 900, 1, point, Army::from([(fighter, 4)]), Some(100));
    target.origin = port;
    target.destination = port;
    target.battle_plan = BattlePlan {
        automatic_tactic: false,
        tactic: CombatTactic::Swarm,
        ..Default::default()
    };
    target.patrol.as_mut().unwrap().friendly_players.insert(2);
    let orders = target.battle_plan;
    model.missions.push(target);
    let army = Army::from([(cruiser, 3)]);
    model.map.get_mut(home).army.extend(army.clone());
    let command = TurnCommand::SendPatrol {
        mission_id: 700,
        origin: home,
        center: point,
        army,
        turns: None,
    };
    let projected = preview_commands(&model, 1, std::slice::from_ref(&command)).unwrap();
    assert_eq!(projected.missions.len(), 2, "dispatch must not merge before arrival");
    let turn = model.turn;
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, turn, vec![command]), TurnSubmission::new(2, turn, vec![])],
    )
    .unwrap();
    let mut merged = false;
    for _ in 0..50 {
        let mut restored =
            PersistedGame::from_json(PersistedGame::new(model.clone()).to_json().unwrap())
                .unwrap()
                .state;
        let balance = model.players[0].resources.deuterium;
        let production = model.players[0].resource_production(&model.map).deuterium;
        let upkeep = model
            .missions
            .iter()
            .filter(|mission| mission.is_patrolling())
            .map(Mission::patrol_upkeep)
            .sum::<usize>();
        empty_turn(&mut model);
        empty_turn(&mut restored);
        assert_eq!(serde_json::to_value(&model).unwrap(), serde_json::to_value(restored).unwrap());
        assert_eq!(model.players[0].resources.deuterium, balance - upkeep + production);
        if model.missions.len() == 1 {
            let patrol = &model.missions[0];
            assert_eq!(patrol.id, 900);
            assert_eq!(patrol.origin, port);
            assert_eq!(patrol.destination, port);
            assert_eq!(patrol.army, Army::from([(fighter, 4), (cruiser, 3)]));
            assert_eq!(patrol.battle_plan, orders);
            assert_eq!(patrol.patrol.as_ref().unwrap().friendly_players, BTreeSet::from([2]));
            assert_eq!(
                patrol.patrol.as_ref().unwrap().remaining_turns,
                Some(100 - model.turn as usize + 1)
            );
            assert!(patrol.logs.contains("Reinforcements from mission 700"));
            merged = true;
            break;
        }
    }
    assert!(merged, "reinforcements never joined the operating patrol");
    let balance = model.players[0].resources.deuterium;
    let production = model.players[0].resource_production(&model.map).deuterium;
    let upkeep = model.missions[0].patrol_upkeep();
    empty_turn(&mut model);
    assert_eq!(model.players[0].resources.deuterium, balance - upkeep + production);
    let patrol = &mut model.missions[0];
    patrol.return_patrol(&model.map, model.turn as usize);
    assert_eq!(patrol.destination, port);
    assert_eq!(patrol.army.amount(&cruiser), 3);
}

#[test]
fn probe_reinforcements_travel_merge_and_replay_with_an_operating_patrol() {
    let mut model = game();
    let home = model.players[0].home_planet;
    let point = center(&model);
    let fighter = Unit::Ship(Ship::LightFighter);
    model.missions.push(stationed(&model, 900, 1, point, Army::from([(fighter, 4)]), None));
    model.map.get_mut(home).army.insert(Unit::probe(), 3);
    let command = TurnCommand::SendPatrol {
        mission_id: 700,
        origin: home,
        center: point,
        army: Army::from([(Unit::probe(), 3)]),
        turns: None,
    };
    let projected = preview_commands(&model, 1, std::slice::from_ref(&command)).unwrap();
    assert_eq!(projected.missions.len(), 2);
    assert_eq!(projected.map.get(home).army.amount(&Unit::probe()), 0);
    assert_eq!(projected.missions[1].patrol.as_ref().unwrap().phase, PatrolPhase::Outbound);
    let canceled = preview_commands(
        &model,
        1,
        &[
            command.clone(),
            TurnCommand::RecallMission {
                mission_id: 700,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        serde_json::to_value(canceled).unwrap(),
        serde_json::to_value(preview_commands(&model, 1, &[]).unwrap()).unwrap()
    );
    let turn = model.turn;
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, turn, vec![command]), TurnSubmission::new(2, turn, vec![])],
    )
    .unwrap();
    for _ in 0..50 {
        let mut restored =
            PersistedGame::from_json(PersistedGame::new(model.clone()).to_json().unwrap())
                .unwrap()
                .state;
        empty_turn(&mut model);
        empty_turn(&mut restored);
        assert_eq!(serde_json::to_value(&model).unwrap(), serde_json::to_value(restored).unwrap());
        if model.missions.len() == 1 {
            break;
        }
    }
    assert_eq!(model.missions.len(), 1);
    let patrol = &model.missions[0];
    assert_eq!(patrol.id, 900);
    assert!(patrol.is_patrolling());
    assert_eq!(patrol.army, Army::from([(fighter, 4), (Unit::probe(), 3)]));
    let upkeep = patrol.patrol_upkeep();
    let balance = model.players[0].resources.deuterium;
    let production = model.players[0].resource_production(&model.map).deuterium;
    empty_turn(&mut model);
    assert_eq!(model.players[0].resources.deuterium, balance - upkeep + production);
}

#[test]
fn probes_can_only_launch_to_reinforce_an_owned_operating_patrol_at_the_same_center() {
    for scenario in 0..6 {
        let mut model = game();
        let home = model.players[0].home_planet;
        let point = center(&model);
        model.map.get_mut(home).army.insert(Unit::probe(), 3);
        let mut target =
            stationed(&model, 900, 1, point, Army::from([(Unit::Ship(Ship::Cruiser), 2)]), None);
        match scenario {
            0 => {}, // No patrol at the destination.
            1 => target.owner = 2,
            2 => target.patrol.as_mut().unwrap().phase = PatrolPhase::Outbound,
            3 => target.return_patrol(&model.map, model.turn as usize),
            4 => {
                target.position += Vec2::X;
                target.patrol.as_mut().unwrap().center += Vec2::X;
            },
            5 => {}, // The only eligible reinforcement target.
            _ => unreachable!(),
        }
        if scenario != 0 {
            model.missions.push(target);
        }
        let before = serde_json::to_value(&model).unwrap();
        let result = preview_commands(
            &model,
            1,
            &[TurnCommand::SendPatrol {
                mission_id: 700,
                origin: home,
                center: point,
                army: Army::from([(Unit::probe(), 3)]),
                turns: None,
            }],
        );
        assert_eq!(result.is_ok(), scenario == 5, "target scenario {scenario}");
        assert_eq!(serde_json::to_value(&model).unwrap(), before);
    }
}

#[test]
fn orphaned_probe_reinforcements_return_instead_of_starting_a_patrol() {
    for recalled in [false, true] {
        let mut model = game();
        let home = model.players[0].home_planet;
        let point = center(&model);
        let probes = Army::from([(Unit::probe(), 3)]);
        let initial_probes = model.map.get(home).army.amount(&Unit::probe());
        if recalled {
            let mut target = stationed(
                &model,
                900,
                1,
                point,
                Army::from([(Unit::Ship(Ship::Cruiser), 2)]),
                None,
            );
            target.return_patrol(&model.map, model.turn as usize);
            model.missions.push(target);
        }
        let mut incoming = stationed(&model, 700, 1, point, probes.clone(), None);
        incoming.patrol.as_mut().unwrap().phase = PatrolPhase::Outbound;
        model.missions.push(incoming);
        empty_turn(&mut model);
        let incoming = model.missions.iter().find(|mission| mission.id == 700).unwrap();
        assert!(incoming.is_returning());
        assert!(!incoming.is_patrolling());
        assert!(incoming.valid_patrol(&model.map));
        assert_eq!(incoming.army, probes);
        for _ in 0..50 {
            if model.missions.iter().all(|mission| mission.id != 700) {
                break;
            }
            empty_turn(&mut model);
        }
        assert!(model.missions.iter().all(|mission| mission.id != 700));
        assert_eq!(model.map.get(home).army.amount(&Unit::probe()), initial_probes + 3);
    }
}

#[test]
fn simultaneous_probe_arrivals_need_combat_ships_to_remain_on_station() {
    let model = game();
    let point = center(&model);
    for escorted in [false, true] {
        let mut missions = vec![
            stationed(&model, 10, 1, point, Army::from([(Unit::probe(), 2)]), None),
            stationed(&model, 20, 1, point, Army::from([(Unit::probe(), 3)]), None),
        ];
        let mut arrivals = BTreeSet::from([10, 20]);
        if escorted {
            missions.push(stationed(
                &model,
                30,
                1,
                point,
                Army::from([(Unit::Ship(Ship::Cruiser), 2)]),
                None,
            ));
            arrivals.insert(30);
        }
        let mut reverse = missions.iter().rev().cloned().collect();
        merge_arrivals(&mut missions, &arrivals, &model.map, 2);
        merge_arrivals(&mut reverse, &arrivals, &model.map, 2);
        missions.sort_by_key(|mission| mission.id);
        reverse.sort_by_key(|mission| mission.id);
        assert_eq!(
            serde_json::to_value(&missions).unwrap(),
            serde_json::to_value(reverse).unwrap()
        );
        assert!(missions.iter().all(|mission| mission.valid_patrol(&model.map)));
        if escorted {
            assert_eq!(missions.len(), 1);
            assert_eq!(missions[0].id, 30);
            assert_eq!(missions[0].army.amount(&Unit::probe()), 5);
            assert!(missions[0].is_patrolling());
        } else {
            assert_eq!(missions.len(), 2);
            assert!(missions.iter().all(Mission::is_returning));
        }
    }
}

#[test]
fn simultaneous_patrol_arrivals_merge_by_id_independently_of_storage_order() {
    let model = game();
    let point = center(&model);
    let cruiser = Unit::Ship(Ship::Cruiser);
    let fleets =
        [30, 10, 20].map(|id| stationed(&model, id, 1, point, Army::from([(cruiser, 2)]), None));
    let arrivals = BTreeSet::from([10, 20, 30]);
    let mut forward = fleets.to_vec();
    let mut reverse = fleets.into_iter().rev().collect();
    merge_arrivals(&mut forward, &arrivals, &model.map, 2);
    merge_arrivals(&mut reverse, &arrivals, &model.map, 2);
    assert_eq!(forward.len(), 1);
    assert_eq!(forward[0].id, 10);
    assert_eq!(forward[0].army.amount(&cruiser), 6);
    assert_eq!(serde_json::to_value(forward).unwrap(), serde_json::to_value(reverse).unwrap());
}

#[test]
fn arrivals_only_merge_into_owned_operating_patrols_at_the_exact_same_center() {
    let model = game();
    let point = center(&model);
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 2)]);
    let incoming = stationed(&model, 700, 1, point, army.clone(), None);
    let enemy = stationed(&model, 900, 2, point, army.clone(), None);
    let nearby = stationed(&model, 901, 1, point + Vec2::X, army.clone(), None);
    let mut outbound = stationed(&model, 902, 1, point, army.clone(), None);
    outbound.patrol.as_mut().unwrap().phase = PatrolPhase::Outbound;
    let mut returning = stationed(&model, 903, 1, point, army, None);
    returning.return_patrol(&model.map, 1);
    let mut missions = vec![incoming, enemy, nearby, outbound, returning];
    let before = serde_json::to_value(&missions).unwrap();
    merge_arrivals(&mut missions, &BTreeSet::from([700]), &model.map, 2);
    assert_eq!(serde_json::to_value(missions).unwrap(), before);
}

#[test]
fn reinforcements_start_their_own_patrol_when_the_target_is_gone_or_recalled() {
    for recalled in [false, true] {
        let mut model = game();
        let point = center(&model);
        let army = Army::from([(Unit::Ship(Ship::Cruiser), 2)]);
        if recalled {
            let mut target = stationed(&model, 900, 1, point, army.clone(), None);
            target.return_patrol(&model.map, model.turn as usize);
            model.missions.push(target);
        }
        let mut incoming = stationed(&model, 700, 1, point, army.clone(), None);
        incoming.patrol.as_mut().unwrap().phase = PatrolPhase::Outbound;
        model.missions.push(incoming);
        empty_turn(&mut model);
        let patrol = model.missions.iter().find(|mission| mission.id == 700).unwrap();
        assert!(patrol.is_patrolling());
        assert_eq!(patrol.position, point);
        assert_eq!(patrol.army, army);
        assert!(patrol.valid_patrol(&model.map));
    }
}

#[test]
fn exposure_checks_whole_routes_stationary_fleets_and_tangents() {
    let radius = PATROL_RADIUS;
    let (exposure, point) =
        route_exposure(Vec2::new(-radius * 2.0, 0.0), Vec2::new(radius * 2.0, 0.0), Vec2::ZERO);
    assert!((exposure - 0.5).abs() < 1e-10);
    assert_eq!(point, Vec2::ZERO);
    assert_eq!(route_exposure(Vec2::ZERO, Vec2::ZERO, Vec2::ZERO).0, 1.0);
    assert_eq!(
        route_exposure(Vec2::splat(radius * 2.0), Vec2::splat(radius * 2.0), Vec2::ZERO).0,
        0.0
    );
    assert_eq!(
        route_exposure(
            Vec2::new(-radius * 2.0, radius),
            Vec2::new(radius * 2.0, radius),
            Vec2::ZERO
        )
        .0,
        0.0
    );
}

#[test]
fn patrol_overlap_uses_shared_area_and_excludes_tangency() {
    let radius = PATROL_RADIUS;
    assert_eq!(patrol_overlap_exposure(Vec2::ZERO, Vec2::ZERO), (1.0, Vec2::ZERO));
    let (exposure, point) = patrol_overlap_exposure(Vec2::X * radius, Vec2::ZERO);
    assert!((exposure - 0.3910022189557706).abs() < 1e-10);
    assert_eq!(point, Vec2::X * radius * 0.5);
    let (partial, point) = patrol_overlap_exposure(Vec2::X * radius * 1.5, Vec2::ZERO);
    assert!((partial - 0.1442936128143875).abs() < 1e-10);
    assert_eq!(point, Vec2::X * radius * 0.75);
    assert_eq!(patrol_overlap_exposure(Vec2::ZERO, Vec2::X * radius * 1.5), (partial, point));
    assert!(patrol_overlap_exposure(Vec2::X * (radius * 2.0 - 0.01), Vec2::ZERO).0 > 0.0);
    for separation in [radius * 2.0, radius * 3.0] {
        assert_eq!(patrol_overlap_exposure(Vec2::X * separation, Vec2::ZERO).0, 0.0);
    }
}

#[test]
fn detection_depends_on_exposure_and_combined_production_with_diminishing_returns() {
    assert_eq!(detection_probability(0.0, 1_000), 0.0);
    assert_eq!(detection_probability(1.0, 0), 0.0);
    assert_eq!(detection_probability(1.0, 20), 0.5);
    assert_eq!(detection_probability(0.5, 40), 0.5);
    assert_eq!(detection_probability(1.0, 60), 0.75);
    assert!(detection_probability(1.0, u128::MAX) <= 1.0);
}

#[test]
fn launch_arrival_operating_fuel_and_auto_return_survive_serialized_turns() {
    let mut model = game();
    let home = model.players[0].home_planet;
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 3)]);
    model.map.get_mut(home).army.extend(army.clone());
    let point = center(&model);
    let command = TurnCommand::SendPatrol {
        mission_id: 700,
        origin: home,
        center: point,
        army: army.clone(),
        turns: None,
    };
    let projected = preview_commands(&model, 1, std::slice::from_ref(&command)).unwrap();
    assert_eq!(projected.missions[0].objective, Icon::Patrol);
    assert!(projected.missions[0].valid_patrol(&model.map));
    assert!(projected.players[0].resources.deuterium < model.players[0].resources.deuterium);
    let turn = model.turn;
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, turn, vec![command]), TurnSubmission::new(2, turn, vec![])],
    )
    .unwrap();
    for _ in 0..50 {
        let mission = model.missions.iter().find(|mission| mission.id == 700).unwrap();
        if mission.is_patrolling() {
            break;
        }
        assert_eq!(mission.patrol.as_ref().unwrap().remaining_turns, None);
        empty_turn(&mut model);
    }
    assert!(model.missions[0].is_patrolling());
    assert_eq!(model.missions[0].position, point);
    for _ in 0..6 {
        let previous = model.players[0].resources.deuterium;
        let production = model.players[0].resource_production(&model.map).deuterium;
        let upkeep = model.missions[0].patrol_upkeep();
        let mut restored =
            PersistedGame::from_json(PersistedGame::new(model.clone()).to_json().unwrap())
                .unwrap()
                .state;
        empty_turn(&mut model);
        empty_turn(&mut restored);
        assert_eq!(serde_json::to_value(&model).unwrap(), serde_json::to_value(restored).unwrap());
        assert_eq!(model.players[0].resources.deuterium, previous - upkeep + production);
        assert_eq!(model.missions[0].patrol.as_ref().unwrap().remaining_turns, None);
        assert_eq!(model.missions[0].position, point);
    }
    model.players[0].resources.deuterium = model.missions[0].patrol_upkeep() - 1;
    empty_turn(&mut model);
    for _ in 0..50 {
        if model.missions.is_empty() {
            break;
        }
        assert!(model.missions[0].is_returning());
        empty_turn(&mut model);
    }
    assert!(model.missions.is_empty());
    assert_eq!(model.map.get(home).army.amount(&Unit::Ship(Ship::Cruiser)), 3);
}

#[test]
fn upkeep_is_reserved_before_spending_recall_releases_it_and_shortage_returns() {
    let mut model = game();
    let point = center(&model);
    model.missions.push(stationed(
        &model,
        700,
        1,
        point,
        Army::from([(Unit::Ship(Ship::Cruiser), 3)]),
        Some(3),
    ));
    let balance = model.players[0].resources.deuterium;
    let upkeep = model.missions[0].patrol_upkeep();
    let preview = preview_commands(&model, 1, &[]).unwrap();
    assert_eq!(preview.players[0].resources.deuterium, balance - upkeep);
    assert_eq!(preview.missions[0].patrol.as_ref().unwrap().remaining_turns, Some(3));
    let recalled = preview_commands(
        &model,
        1,
        &[TurnCommand::RecallMission {
            mission_id: 700,
        }],
    )
    .unwrap();
    assert_eq!(recalled.players[0].resources.deuterium, balance);
    assert!(recalled.missions[0].is_returning());
    assert_eq!(recalled.missions[0].position, point);
    model.players[0].resources.deuterium = upkeep - 1;
    let preview = preview_commands(&model, 1, &[]).unwrap();
    assert!(preview.missions[0].is_returning());
    assert_eq!(preview.players[0].resources.deuterium, upkeep - 1);
}

#[test]
fn same_turn_launch_cancellation_refunds_fuel_and_ships_and_invalid_orders_are_atomic() {
    let mut model = game();
    let home = model.players[0].home_planet;
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 3)]);
    model.map.get_mut(home).army.extend(army.clone());
    let point = center(&model);
    let launch = |center, turns, army| TurnCommand::SendPatrol {
        mission_id: 700,
        origin: home,
        center,
        army,
        turns,
    };
    let before = serde_json::to_value(&model).unwrap();
    let canceled = preview_commands(
        &model,
        1,
        &[
            launch(point, None, army.clone()),
            TurnCommand::RecallMission {
                mission_id: 700,
            },
        ],
    )
    .unwrap();
    assert_eq!(serde_json::to_value(canceled).unwrap(), before);
    for invalid in [
        launch(Vec2::NAN, None, army.clone()),
        launch(model.map.rect.max + Vec2::ONE, None, army.clone()),
        launch(model.map.get(home).position, None, army.clone()),
        launch(point, Some(0), army.clone()),
        launch(point, Some(MAX_PATROL_TURNS + 1), army.clone()),
        launch(point, None, Army::from([(Unit::probe(), 3)])),
        launch(point, None, Army::from([(Unit::Ship(Ship::Cruiser), 4)])),
    ] {
        assert!(preview_commands(&model, 1, &[invalid]).is_err());
        assert_eq!(serde_json::to_value(&model).unwrap(), before);
    }
}

#[test]
fn patrol_casualties_ignore_empty_ship_classes_and_preserve_proportional_survivors() {
    let mut model = game();
    let point = center(&model);
    let cruiser = Unit::Ship(Ship::Cruiser);
    let fighter = Unit::Ship(Ship::LightFighter);
    model.missions.extend([
        stationed(&model, 701, 1, point, Army::from([(cruiser, 2), (fighter, 0)]), None),
        stationed(&model, 700, 1, point, Army::from([(cruiser, 1), (fighter, 0)]), None),
        stationed(&model, 800, 2, point, Army::from([(cruiser, 4)]), None),
    ]);
    model.missions[0].patrol.as_mut().unwrap().friendly_players.insert(2);
    let mut model =
        PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap().state;

    install_survivors(&mut model, &[700, 701], &Army::from([(cruiser, 2), (fighter, 0)]));

    assert_eq!(model.missions[0].army, Army::from([(cruiser, 1)]));
    assert_eq!(model.missions[1].army, Army::from([(cruiser, 1)]));
    assert_eq!(model.missions[0].patrol.as_ref().unwrap().friendly_players, [2].into());
    assert!(model.missions[1].patrol.as_ref().unwrap().friendly_players.is_empty());
    assert_eq!(model.missions[2].army, Army::from([(cruiser, 4)]));

    install_survivors(&mut model, &[700, 701], &Army::new());
    assert_eq!(model.missions.len(), 1);
    assert_eq!(model.missions[0].id, 800);
    model.validate().unwrap();
}

#[test]
fn patrol_interception_combines_fleets_preserves_planets_and_replays_deterministically() {
    use crate::core::combat::report::Side;

    let mut model = game();
    let point = center(&model);
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 10), (Unit::colony_ship(), 0)]);
    model.missions.push(stationed(&model, 700, 1, point, army.clone(), None));
    model.missions.push(stationed(&model, 701, 1, point, army, None));
    let home = model.players[1].home_planet;
    let mut target = Mission::new_with_id(
        800,
        model.turn as usize,
        2,
        model.map.get(home),
        model.map.get(model.players[0].home_planet),
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::LightFighter), 5), (Unit::probe(), 0)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    target.position = point;
    model.missions.push(target);
    model.validate().unwrap();
    let planets = serde_json::to_value(&model.map).unwrap();
    let mut resolved = None;
    for seed in 0..64 {
        let mut attempt = model.clone();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        let delayed =
            resolve_encounters(&mut attempt, model.turn as usize, &mut rng, &mut HashSet::new());
        if !attempt.players[0].reports.is_empty() {
            let mut replay = model.clone();
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
            assert_eq!(
                delayed,
                resolve_encounters(&mut replay, model.turn as usize, &mut rng, &mut HashSet::new())
            );
            assert_eq!(
                serde_json::to_value(&attempt).unwrap(),
                serde_json::to_value(&replay).unwrap()
            );
            resolved = Some(attempt);
            break;
        }
    }
    let resolved = resolved.unwrap();
    assert_eq!(serde_json::to_value(&resolved.map).unwrap(), planets);
    for player in &resolved.players {
        assert_eq!(player.reports.len(), 1);
        let report = &player.reports[0];
        assert!(report.is_space_encounter());
        assert_eq!(report.mission.army.amount(&Unit::Ship(Ship::Cruiser)), 20);
        assert_eq!(report.planet.controlled, Some(2));
        assert_eq!(report.destination_controlled, Some(2));
        assert!(!report.planet_colonized && !report.planet_destroyed);
        assert!(!report.surviving_defender.has_army());
        assert!(!report.can_see(&Side::Attacker, 2));
        assert!(report.can_see(&Side::Defender, 2));
        assert!(report.can_see(&Side::Attacker, 1));
        assert!(report.can_see(&Side::Defender, 1));
    }
    assert!(crate::core::recycling::debris_sites(
        resolved.players.iter().flat_map(|player| &player.reports),
        1
    )
    .is_empty());
    resolved.validate().unwrap();
}

#[test]
fn self_owned_fleets_and_special_travel_cannot_be_intercepted() {
    let mut model = game();
    let point = center(&model);
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 10)]);
    model.missions.push(stationed(&model, 700, 1, point, army.clone(), None));
    model.missions.push(stationed(&model, 701, 1, point, army, None));
    for (id, objective, deep_cover, jump_gate) in [
        (800, Icon::MissileStrike, false, false),
        (801, Icon::Spy, true, false),
        (802, Icon::Deploy, false, true),
    ] {
        let mut mission = Mission::new_with_id(
            id,
            1,
            2,
            model.map.get(model.players[1].home_planet),
            model.map.get(model.players[0].home_planet),
            objective,
            Army::from([(Unit::probe(), 5)]),
            BombingRaid::None,
            false,
            jump_gate,
            None,
        )
        .with_deep_cover(deep_cover);
        mission.position = point;
        model.missions.push(mission);
    }
    let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(0);
    assert!(resolve_encounters(&mut model, 1, &mut rng, &mut HashSet::new()).is_empty());
    assert!(model.players.iter().all(|player| player.reports.is_empty()));
}

fn friendly_detection_game() -> GameModel {
    let mut model = game();
    let point = center(&model);
    let mut patrol =
        stationed(&model, 700, 1, point, Army::from([(Unit::Ship(Ship::Cruiser), 10)]), None);
    patrol.patrol.as_mut().unwrap().friendly_players.insert(2);
    let mut target = Mission::new_with_id(
        800,
        model.turn as usize,
        2,
        model.map.get(model.players[1].home_planet),
        model.map.get(model.players[0].home_planet),
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::LightFighter), 5)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    target.position = point;
    model.missions.extend([patrol, target]);
    model
}

#[test]
fn patrol_friendship_orders_validate_authority_and_survive_saved_drafts() {
    let model = friendly_detection_game();
    let command = TurnCommand::SetPatrolFriendly {
        mission_id: 700,
        player_id: 2,
        friendly: false,
    };
    let command: TurnCommand =
        serde_json::from_str(&serde_json::to_string(&command).unwrap()).unwrap();
    let projected = preview_commands(&model, 1, &[command.clone(), command]).unwrap();
    assert!(projected.missions[0].patrol.as_ref().unwrap().friendly_players.is_empty());
    let command = TurnCommand::SetPatrolFriendly {
        mission_id: 700,
        player_id: 2,
        friendly: true,
    };
    let projected = preview_commands(&projected, 1, &[command.clone(), command]).unwrap();
    let restored =
        PersistedGame::from_json(PersistedGame::new(projected).to_json().unwrap()).unwrap().state;
    assert_eq!(restored.missions[0].patrol.as_ref().unwrap().friendly_players, [2].into());
    for (actor, mission_id, player_id) in
        [(2, 700, 1), (1, 800, 2), (1, 999, 2), (1, 700, 1), (1, 700, 99)]
    {
        assert!(preview_commands(
            &model,
            actor,
            &[TurnCommand::SetPatrolFriendly {
                mission_id,
                player_id,
                friendly: true
            }]
        )
        .is_err());
    }
    let mut returning = model.clone();
    returning.missions[0].return_patrol(&model.map, model.turn as usize);
    assert!(preview_commands(
        &returning,
        1,
        &[TurnCommand::SetPatrolFriendly {
            mission_id: 700,
            player_id: 2,
            friendly: false
        }]
    )
    .is_err());
    let mut malformed = serde_json::to_value(PersistedGame::new(model)).unwrap();
    for invalid in [serde_json::json!([1]), serde_json::json!([99]), serde_json::Value::Null] {
        malformed["state"]["missions"][0]["patrol"]["friendly_players"] = invalid;
        assert!(PersistedGame::from_json(malformed.clone()).is_err());
    }
}

#[test]
fn friendly_contact_reports_fleet_and_location_without_combat_or_movement_loss() {
    use crate::core::combat::report::Side;
    let model = friendly_detection_game();
    let mut contacts = 0;
    for seed in 0..16 {
        let mut observed = model.clone();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        assert!(resolve_encounters(
            &mut observed,
            model.turn as usize,
            &mut rng,
            &mut HashSet::new()
        )
        .is_empty());
        assert_eq!(
            serde_json::to_value(&observed.map).unwrap(),
            serde_json::to_value(&model.map).unwrap()
        );
        assert!(
            observed.players[1].reports.is_empty(),
            "observation alone must not notify the detected player"
        );
        for (before, after) in model.missions.iter().zip(&observed.missions) {
            assert_eq!(before.army, after.army);
            assert_eq!(before.position, after.position);
            assert_eq!(before.travel_turns, after.travel_turns);
        }
        if let Some(report) = observed.players[0].reports.last() {
            contacts += 1;
            assert!(report.is_patrol_contact());
            assert!(!report.is_stalemate());
            assert_eq!(report.winner(), None);
            assert_eq!(report.status(&observed.players[0]), "contact");
            assert!(report.can_see(&Side::Defender, 1));
            let (_, detection_point) = route_exposure(
                model.missions[1].position,
                model.missions[1].next_turn_position(&model.map),
                model.missions[0].position,
            );
            assert_eq!(report.planet.position, detection_point);
            assert_eq!(report.planet.army.controller(), &model.missions[1].army);
            assert!(!report.has_combat_playback());
            observed.validate().unwrap();
            let restored =
                PersistedGame::from_json(PersistedGame::new(observed.clone()).to_json().unwrap())
                    .unwrap();
            assert!(restored.state.players[0].reports[0].is_patrol_contact());
            let mut replay = model.clone();
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
            resolve_encounters(&mut replay, model.turn as usize, &mut rng, &mut HashSet::new());
            assert_eq!(
                serde_json::to_value(observed).unwrap(),
                serde_json::to_value(replay).unwrap()
            );
        }
    }
    assert!(contacts > 0);
}

#[test]
fn overlapping_friendly_patrols_report_contacts_during_resolved_turns() {
    let mut model = game();
    let first = center(&model);
    let second = first + Vec2::X * PATROL_RADIUS * 1.5;
    assert!(valid_patrol_target(&model.map, model.players[1].home_planet, second, None));
    assert_eq!(route_exposure(first, first, second).0, 0.0);
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 10)]);
    for (id, owner, point, friendly) in [(700, 1, first, 2), (800, 2, second, 1)] {
        let mut patrol = stationed(&model, id, owner, point, army.clone(), None);
        patrol.patrol.as_mut().unwrap().friendly_players.insert(friendly);
        model.missions.push(patrol);
    }
    model.validate().unwrap();
    let mut replay = model.clone();
    for _ in 0..32 {
        empty_turn(&mut model);
        empty_turn(&mut replay);
    }
    for player in &model.players {
        assert!(
            !player.reports.is_empty(),
            "overlapping patrols must be able to detect each other"
        );
        for report in &player.reports {
            assert!(report.is_patrol_contact());
            assert_eq!(report.mission.owner, player.id);
            assert_eq!(report.destination_controlled, Some(3 - player.id));
            assert_eq!(report.planet.position, (first + second) * 0.5);
            assert_eq!(report.surviving_attacker, army);
            assert_eq!(report.surviving_defender.controller(), &army);
        }
    }
    assert_eq!(model.missions.len(), 2);
    for (patrol, point) in model.missions.iter().zip([first, second]) {
        assert!(patrol.is_patrolling());
        assert_eq!(patrol.position, point);
        assert_eq!(patrol.army, army);
        assert!(patrol.logs.contains("No combat."));
    }
    model.validate().unwrap();
    let saved = PersistedGame::new(model).to_json().unwrap();
    assert_eq!(saved, PersistedGame::new(replay).to_json().unwrap());
    let restored = PersistedGame::from_json(saved).unwrap();
    assert!(restored.state.players.iter().all(|player| !player.reports.is_empty()));
}

#[test]
fn friendship_is_directional_and_mutual_friendlies_only_observe_each_other() {
    let mut model = friendly_detection_game();
    model.missions[1] = stationed(
        &model,
        800,
        2,
        model.missions[0].position + Vec2::X * PATROL_RADIUS * 1.5,
        Army::from([(Unit::Ship(Ship::Cruiser), 10)]),
        None,
    );
    let mut battles = 0;
    for seed in 0..32 {
        let mut attempt = model.clone();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        resolve_encounters(&mut attempt, model.turn as usize, &mut rng, &mut HashSet::new());
        for report in
            attempt.players[0].reports.iter().filter(|report| report.combat_report.is_some())
        {
            battles += 1;
            assert_eq!(report.mission.owner, 2, "the hostile patrol must initiate");
        }
    }
    assert!(battles > 0);
    model.missions[1].patrol.as_mut().unwrap().friendly_players.insert(1);
    let original = model.missions.iter().map(|mission| mission.army.clone()).collect::<Vec<_>>();
    for seed in 0..8 {
        let mut attempt = model.clone();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        assert!(resolve_encounters(
            &mut attempt,
            model.turn as usize,
            &mut rng,
            &mut HashSet::new()
        )
        .is_empty());
        assert_eq!(
            attempt.missions.iter().map(|mission| mission.army.clone()).collect::<Vec<_>>(),
            original
        );
        assert!(attempt
            .players
            .iter()
            .flat_map(|player| &player.reports)
            .all(MissionReport::is_patrol_contact));
        attempt.validate().unwrap();
    }
}

#[test]
fn colocated_patrols_keep_distinct_friendship_orders_when_initiating_combat() {
    let mut model = friendly_detection_game();
    let mut hostile = model.missions[0].clone();
    hostile.id = 701;
    hostile.patrol.as_mut().unwrap().friendly_players.clear();
    model.missions.push(hostile);
    let friendly_army = model.missions[0].army.clone();
    let mut battles = 0;
    for seed in 0..8 {
        let mut attempt = model.clone();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        resolve_encounters(&mut attempt, model.turn as usize, &mut rng, &mut HashSet::new());
        if let Some(report) = attempt.players[0].reports.last() {
            battles += 1;
            assert_eq!(report.mission.id, 701);
            assert_eq!(report.mission.army.amount(&Unit::Ship(Ship::Cruiser)), 10);
            assert!(report.combat_report.is_some());
        }
        assert_eq!(
            attempt.missions.iter().find(|mission| mission.id == 700).unwrap().army,
            friendly_army
        );
    }
    assert!(battles > 0);
}

#[test]
fn a_stronger_friendly_zone_does_not_suppress_an_overlapping_hostile_patrol() {
    let mut model = friendly_detection_game();
    let mut hostile = model.missions[0].clone();
    hostile.id = 701;
    hostile.position += Vec2::X;
    hostile.patrol.as_mut().unwrap().center = hostile.position;
    hostile.patrol.as_mut().unwrap().friendly_players.clear();
    hostile.army.insert(Unit::Ship(Ship::Cruiser), 5);
    model.missions.push(hostile);
    let mut battles = 0;
    for seed in 0..8 {
        let mut attempt = model.clone();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        resolve_encounters(&mut attempt, model.turn as usize, &mut rng, &mut HashSet::new());
        if let Some(report) =
            attempt.players[0].reports.iter().find(|report| report.combat_report.is_some())
        {
            battles += 1;
            assert_eq!(report.mission.id, 701);
            assert_eq!(report.mission.army.amount(&Unit::Ship(Ship::Cruiser)), 5);
        }
    }
    assert!(battles > 0);
}

#[test]
fn a_friendly_patrol_remains_attackable_after_its_hostile_neighbor_fought() {
    let mut model = friendly_detection_game();
    let mut hostile = model.missions[0].clone();
    hostile.id = 699;
    hostile.patrol.as_mut().unwrap().friendly_players.clear();
    model.missions.push(hostile);
    model.missions.push(stationed(
        &model,
        801,
        2,
        model.missions[0].position,
        Army::from([(Unit::Ship(Ship::Cruiser), 10)]),
        None,
    ));
    let mut both_engaged = false;
    for seed in 0..32 {
        let mut attempt = model.clone();
        let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
        resolve_encounters(&mut attempt, model.turn as usize, &mut rng, &mut HashSet::new());
        let battles = attempt.players[0]
            .reports
            .iter()
            .filter(|report| report.combat_report.is_some())
            .collect::<Vec<_>>();
        if battles.first().is_some_and(|report| {
            report.mission.id == 699
                && report.planet.army.amount(&Unit::Ship(Ship::LightFighter)) == 5
        }) && battles.iter().any(|report| report.mission.id == 801)
        {
            both_engaged = true;
            break;
        }
    }
    assert!(
        both_engaged,
        "an idle friendly patrol must remain attackable after its hostile neighbor fought"
    );
}

#[test]
fn lost_return_port_redirects_to_a_controlled_world_without_starting_an_attack() {
    let mut model = game();
    let point = center(&model);
    let mut mission =
        stationed(&model, 700, 1, point, Army::from([(Unit::Ship(Ship::Cruiser), 3)]), None);
    let alternative = model.map.planets.iter().find(|planet| planet.owned.is_none()).unwrap().id;
    model.map.get_mut(alternative).controlled = Some(1);
    model.map.get_mut(mission.destination).controlled = Some(2);
    mission.return_patrol(&model.map, 2);
    assert_eq!(mission.destination, alternative);
    assert_eq!(mission.objective, Icon::Deploy);
    assert_eq!(mission.position, point);
    assert_eq!(mission.return_objective, Some(Icon::Patrol));
    assert!(mission.valid_patrol(&model.map));
}

#[test]
fn returning_patrols_dock_alongside_ordinary_deployments_without_merging_space_metadata() {
    let mut model = game();
    let home = model.players[0].home_planet;
    let other = model.players[1].home_planet;
    let point = center(&model);
    let before = model.map.get(home).army.amount(&Unit::Ship(Ship::Cruiser));
    let mut patrol =
        stationed(&model, 700, 1, point, Army::from([(Unit::Ship(Ship::Cruiser), 3)]), Some(0));
    patrol.return_patrol(&model.map, model.turn as usize);
    patrol.position = model.map.get(home).position;
    let mut deploy = Mission::new_with_id(
        800,
        model.turn as usize,
        1,
        model.map.get(other),
        model.map.get(home),
        Icon::Deploy,
        Army::from([(Unit::Ship(Ship::LightFighter), 2)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    deploy.position = model.map.get(home).position;
    model.missions.extend([patrol, deploy]);
    empty_turn(&mut model);
    assert!(model.missions.is_empty());
    assert_eq!(model.map.get(home).army.amount(&Unit::Ship(Ship::Cruiser)), before + 3);
    assert_eq!(model.players[0].reports.len(), 2);
    assert_eq!(
        model.players[0].reports.iter().filter(|report| report.mission.patrol.is_some()).count(),
        1
    );
    model.validate().unwrap();
}

#[test]
fn full_operating_turns_roll_the_existing_fauna_hazard_without_travel_distance() {
    let mut model = game();
    model.rules.space_fauna_percent = 30;
    let point = center(&model);
    model.missions.push(stationed(&model, 700, 1, point, Army::from([(Unit::war_sun(), 2)]), None));
    assert_eq!(model.missions[0].duration(&model.map), 0);
    let mut encountered = false;
    for seed in 0..64 {
        let mut attempt = model.clone();
        attempt.rng = crate::core::random::DeterministicRngState::from_u64(seed);
        empty_turn(&mut attempt);
        if attempt.players[0].reports.iter().any(|report| report.is_space_fauna_encounter()) {
            let snapshot = PersistedGame::new(attempt).to_json().unwrap();
            PersistedGame::from_json(snapshot).unwrap();
            encountered = true;
            break;
        }
    }
    assert!(encountered, "an operating patrol must remain eligible for fauna encounters");
}

#[test]
fn an_owned_empty_launch_world_can_receive_its_patrol_without_stranding_or_conquest() {
    let mut model = game();
    let home = model.players[0].home_planet;
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 3)]);
    model.map.get_mut(home).army = army.clone().into();
    let point = center(&model);
    let mut model = preview_commands(
        &model,
        1,
        &[TurnCommand::SendPatrol {
            mission_id: 700,
            origin: home,
            center: point,
            army,
            turns: None,
        }],
    )
    .unwrap();
    assert_eq!(model.map.get(home).controlled, None);
    assert_eq!(model.map.get(home).owned, Some(1));
    assert_eq!(return_port(&model.map, 1, home, point), Some(home));
    model.missions[0].position = point;
    model.missions[0].travel_turns = 1;
    model.missions[0].patrol.as_mut().unwrap().phase = PatrolPhase::OnStation;
    model.validate().unwrap();
    let turn = model.turn;
    resolve_turn(
        &mut model,
        &[
            TurnSubmission::new(
                1,
                turn,
                vec![TurnCommand::RecallMission {
                    mission_id: 700,
                }],
            ),
            TurnSubmission::new(2, turn, vec![]),
        ],
    )
    .unwrap();
    for _ in 0..50 {
        if model.missions.is_empty() {
            break;
        }
        empty_turn(&mut model);
    }
    assert!(model.missions.is_empty());
    assert_eq!(model.map.get(home).army.amount(&Unit::Ship(Ship::Cruiser)), 3);
    assert_eq!(model.map.get(home).owned, Some(1));
    assert_eq!(model.map.get(home).controlled, Some(1));
}
