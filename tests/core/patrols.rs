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
fn patrol_interception_combines_fleets_preserves_planets_and_replays_deterministically() {
    let mut model = game();
    let point = center(&model);
    let army = Army::from([(Unit::Ship(Ship::Cruiser), 10)]);
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
        Army::from([(Unit::Ship(Ship::LightFighter), 5)]),
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
        let delayed = resolve_encounters(&mut attempt, model.turn as usize, &mut rng);
        if !attempt.players[0].reports.is_empty() {
            let mut replay = model.clone();
            let mut rng = rand_chacha::ChaCha8Rng::seed_from_u64(seed);
            assert_eq!(delayed, resolve_encounters(&mut replay, model.turn as usize, &mut rng));
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
    assert!(resolve_encounters(&mut model, 1, &mut rng).is_empty());
    assert!(model.players.iter().all(|player| player.reports.is_empty()));
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
