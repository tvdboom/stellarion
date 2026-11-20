use super::*;

use crate::core::combat::plan::BattlePlan;
use crate::core::constants::SOLAR_STAR_SIZE;
use crate::core::patrols::{resolve_encounters, PatrolPhase};
use crate::core::random::DeterministicRngState;
use crate::core::simulation::{
    preview_commands, resolve_turn, GameRules, PersistedGame, TurnCommand, TurnSubmission,
};
use crate::core::units::buildings::Building;

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

fn site(model: &GameModel) -> Vec2 {
    let point = model.map.rect.center();
    assert!(valid_expedition_target(&model.map, model.players[0].home_planet, point));
    point
}

fn launch(model: &mut GameModel) -> TurnCommand {
    let home = model.players[0].home_planet;
    let army = Army::from([(Unit::colony_ship(), 2), (Unit::Ship(Ship::Cruiser), 3)]);
    model.map.get_mut(home).army.extend(army.clone());
    TurnCommand::SendExpedition {
        mission_id: 700,
        origin: home,
        center: site(model),
        army,
        battle_plan: BattlePlan::default(),
    }
}

fn empty_turn(model: &mut GameModel) {
    let turn = model.turn;
    resolve_turn(
        model,
        &[TurnSubmission::new(1, turn, vec![]), TurnSubmission::new(2, turn, vec![])],
    )
    .unwrap();
}

#[test]
fn launch_requires_colony_ship_valid_coordinates_and_available_owned_units() {
    let mut model = game();
    let command = launch(&mut model);
    assert!(preview_commands(&model, 1, std::slice::from_ref(&command)).is_ok());
    let TurnCommand::SendExpedition {
        army,
        center,
        origin,
        ..
    } = command.clone()
    else {
        unreachable!()
    };
    let make = |origin, center, army| TurnCommand::SendExpedition {
        mission_id: 700,
        origin,
        center,
        army,
        battle_plan: BattlePlan::default(),
    };
    for invalid in [
        make(origin, center, Army::from([(Unit::Ship(Ship::Cruiser), 1)])),
        make(origin, center, Army::from([(Unit::colony_ship(), 3)])),
        make(origin, Vec2::NAN, army.clone()),
        make(origin, model.map.rect.max + Vec2::ONE, army.clone()),
        make(origin, model.map.get(origin).position, army.clone()),
        make(model.players[1].home_planet, center, army.clone()),
    ] {
        assert!(preview_commands(&model, 1, &[invalid]).is_err());
    }
    assert!(army.amount(&Unit::colony_ship()) > 0);
    assert!(success_probability(&model.map, 1, center) > 0.0);
}

#[test]
fn expeditions_can_target_worlds_and_either_side_of_the_suns_edge() {
    let mut model = game();
    let TurnCommand::SendExpedition {
        mission_id,
        origin,
        army,
        battle_plan,
        ..
    } = launch(&mut model)
    else {
        unreachable!()
    };
    let map = &model.map;
    let sun = map.solar_star_position();
    let direction = -map.solar_corner().x * Vec2::X;
    let radius = SOLAR_STAR_SIZE * 0.5;

    for center in [
        sun + direction * (radius - 20.0),
        sun + direction * (radius + 20.0),
        map.get(model.players[1].home_planet).position,
        map.rect.center(),
        map.get(origin).position + Vec2::X,
    ] {
        assert!(valid_expedition_target(map, origin, center));
        let command = TurnCommand::SendExpedition {
            mission_id,
            origin,
            center,
            army: army.clone(),
            battle_plan,
        };
        let launch = preview_commands(&model, 1, &[command]).unwrap();
        assert!(launch.missions[0].fuel_consumption(&launch.map) > 0);
        assert!(launch.missions[0].turns_to_destination(&launch.map) > 0);
    }
}

#[test]
fn a_saved_expedition_inside_the_sun_can_be_recalled() {
    let mut model = game();
    let command = launch(&mut model);
    let turn = model.turn;
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, turn, vec![command]), TurnSubmission::new(2, turn, vec![])],
    )
    .unwrap();
    let sun = model.map.solar_star_position();
    let direction = -model.map.solar_corner().x * Vec2::X;
    let mission = &mut model.missions[0];
    mission.expedition.as_mut().unwrap().center = sun + direction * (SOLAR_STAR_SIZE * 0.5 - 20.0);
    let mission_id = mission.id;

    let saved = PersistedGame::new(model).to_json().unwrap();
    let restored = PersistedGame::from_json(saved).unwrap().state;
    let recall = TurnCommand::RecallMission {
        mission_id,
    };
    let recalled = preview_commands(&restored, 1, &[recall]).unwrap();
    assert!(recalled.missions[0].is_returning());
}

#[test]
fn discovery_odds_increase_with_distance_from_controlled_worlds() {
    let mut model = game();
    let origin = model.players[0].home_planet;
    model.map.get_mut(origin).position = Vec2::ZERO;
    let nearby = success_probability(&model.map, 1, Vec2::X * Planet::SIZE);
    let far = success_probability(&model.map, 1, Vec2::X * Planet::SIZE * 10.0);
    assert!(nearby >= 0.99 && far > nearby && far < 1.0);
    assert_eq!(nearby, success_probability(&model.map, 1, Vec2::Y * Planet::SIZE));
    assert_eq!(success_probability(&model.map, 1, Vec2::NAN), 0.0);
    assert_eq!(success_probability(&model.map, 1, model.map.rect.max + Vec2::ONE), 0.0);
    assert!(resource_discovery_probability(1.0) < 0.1);
    assert!(resource_discovery_probability(30.0) > 0.4);
}

#[test]
fn discoveries_use_control_at_arrival_including_moons_and_excluding_occupied_worlds() {
    let mut model = game();
    let home = model.players[0].home_planet;
    let occupied = model.players[1].home_planet;
    let moon = model.map.planets.iter().find(|world| world.is_moon()).unwrap().id;
    let center = Vec2::ZERO;
    model.map.get_mut(home).position = Vec2::X * Planet::SIZE * 10.0;
    model.map.get_mut(occupied).position = Vec2::Y;
    model.map.get_mut(occupied).owned = Some(1);
    model.map.get_mut(moon).position = center;
    model.map.get_mut(moon).controlled = Some(1);
    assert_eq!(distance_to_controlled_world(&model.map, 1, center), Some(0.0));

    let mut mission = Mission::new_expedition(
        900,
        1,
        1,
        model.map.get(home),
        center,
        Army::from([(Unit::colony_ship(), 1)]),
    );
    mission.position = center;
    model.missions.push(mission);

    // The second fleet reaches the same site after losing control of the moon.
    let mut lost_moon = model.clone();
    lost_moon.map.get_mut(moon).controlled = Some(2);
    assert_eq!(distance_to_controlled_world(&lost_moon.map, 1, center), Some(10.0));
    let seed = (0..64)
        .map(DeterministicRngState::from_u64)
        .find(|seed| seed.clone().next_rng().random_bool(ship_discovery_probability(10.0, 2)))
        .unwrap();
    resolve_arrivals(&mut model, 2, &mut HashSet::new(), &mut seed.clone().next_rng());
    resolve_arrivals(&mut lost_moon, 2, &mut HashSet::new(), &mut seed.clone().next_rng());

    let close = model.missions[0].expedition.as_ref().unwrap();
    let far = lost_moon.missions[0].expedition.as_ref().unwrap();
    assert_eq!(close.phase, ExpeditionPhase::Returning);
    assert_eq!(far.phase, ExpeditionPhase::Returning);
    assert!(close.resources.total() <= 1 && close.ships.is_empty());
    assert!(far.resources.total() > 1 || !far.ships.is_empty());
}

#[cfg(feature = "app")]
#[test]
fn expedition_outlook_compares_expected_rewards_and_fuel_around_break_even() {
    let model = game();
    let origin = model.map.get(model.players[0].home_planet);
    let direction = (model.map.rect.center() - origin.position).normalize();
    let mut mission = Mission::new_expedition(
        900,
        1,
        1,
        origin,
        origin.position + direction * Planet::SIZE,
        Army::from([(Unit::colony_ship(), 1)]),
    );

    // At 1 AU, a lone Colony Ship averages about seven resources of recovery value.
    assert_eq!(outlook(&model.map, &mission, 2, 7).1, 50);
    assert!(outlook(&model.map, &mission, 2, 6).1 > 50);
    assert!(outlook(&model.map, &mission, 2, 8).1 < 50);
    assert_eq!(outlook(&model.map, &mission, 2, 0).1, 100);
    assert_eq!(outlook(&model.map, &mission, 2, usize::MAX).1, 0);
    mission.army.clear();
    assert_eq!(outlook(&model.map, &mission, 2, 0).1, 0);
}

#[test]
fn expedition_replays_out_and_back_and_credits_cargo_only_after_docking() {
    let mut model = game();
    let command = launch(&mut model);
    let turn = model.turn;
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, turn, vec![command]), TurnSubmission::new(2, turn, vec![])],
    )
    .unwrap();
    let mut explored = false;
    let home = model.players[0].home_planet;
    let original_colonies = model.map.get(home).army.amount(&Unit::colony_ship());
    for _ in 0..100 {
        let mut restored =
            PersistedGame::from_json(PersistedGame::new(model.clone()).to_json().unwrap())
                .unwrap()
                .state;
        let before = model.players[0].resources;
        let income = model.players[0].resource_production(&model.map);
        let cargo = model
            .missions
            .first()
            .and_then(|mission| mission.expedition.as_ref())
            .map(|expedition| expedition.resources)
            .unwrap_or_default();
        empty_turn(&mut model);
        empty_turn(&mut restored);
        assert_eq!(serde_json::to_value(&model).unwrap(), serde_json::to_value(restored).unwrap());
        if model.missions.is_empty() {
            assert_eq!(model.players[0].resources, before + income + cargo);
            break;
        }
        assert_eq!(model.players[0].resources, before + income);
        let mission = &model.missions[0];
        assert_eq!(mission.image(&model.players[0]), "mission colonize");
        explored |= mission.is_returning();
    }
    assert!(explored && model.missions.is_empty());
    assert_eq!(model.map.get(home).army.amount(&Unit::colony_ship()), original_colonies + 2);
    assert!(model.players[0]
        .reports
        .iter()
        .any(|report| report.mission.return_objective == Some(Icon::Expedition)));
}

#[test]
fn discovery_is_reproducible_and_empty_returns_are_rare_for_any_fleet_size() {
    let model = game();
    let home = model.players[0].home_planet;
    let template = Mission::new_expedition(
        900,
        model.turn as usize,
        1,
        model.map.get(home),
        site(&model),
        Army::from([(Unit::colony_ship(), 1)]),
    );
    let mut outcomes = [[0; 2]; 2];
    let mut larger_ship_finds = 0;
    for seed in 0..5_000 {
        let mut mission = template.clone();
        let mut replay = template.clone();
        let roll = |mission: &mut Mission| {
            discover(mission, &model.map, 2, &mut DeterministicRngState::from_u64(seed).next_rng());
        };
        roll(&mut mission);
        if seed < 100 {
            roll(&mut replay);
            assert_eq!(
                serde_json::to_value(&mission).unwrap(),
                serde_json::to_value(replay).unwrap()
            );
        }
        replay = template.clone();
        replay.army.insert(Unit::colony_ship(), 30);
        replay.army.insert(Unit::Ship(Ship::Cruiser), 70);
        roll(&mut replay);
        let cargo = mission.expedition.unwrap();
        let larger_cargo = replay.expedition.unwrap();
        larger_ship_finds += usize::from(!larger_cargo.ships.is_empty());
        outcomes[usize::from(!cargo.resources.is_empty())][usize::from(!cargo.ships.is_empty())] +=
            1;
    }
    assert!(outcomes.into_iter().flatten().all(|count| count > 0));
    assert!(outcomes[0][0] < 75, "unexpected empty returns: {outcomes:?}");
    assert!(larger_ship_finds > outcomes[0][1] + outcomes[1][1]);
}

fn sampled_ship_finds(distance: f64, turn: usize, army: Army) -> [usize; 9] {
    let mut model = game();
    let home = model.players[0].home_planet;
    model.map.get_mut(home).position = Vec2::ZERO;
    let template = Mission::new_expedition(
        900,
        1,
        1,
        model.map.get(home),
        Vec2::X * Planet::SIZE * distance as f32,
        army,
    );
    let mut finds = [0; 9];
    for seed in 0..10_000 {
        let mut mission = template.clone();
        discover(
            &mut mission,
            &model.map,
            turn,
            &mut DeterministicRngState::from_u64(seed).next_rng(),
        );
        let cargo = mission.expedition.unwrap();
        assert!(cargo.ships.len() <= 1);
        let index = cargo.ships.keys().next().map_or(0, |unit| {
            RECOVERABLE_SHIPS.iter().position(|ship| Unit::Ship(*ship) == *unit).unwrap() + 1
        });
        assert!(cargo.ships.amount(&Unit::war_sun()) <= 1);
        finds[index] += 1;
    }
    finds
}

#[test]
fn distant_expeditions_find_ships_regularly_and_capitals_scale_with_turn_and_fleet() {
    let small = Army::from([(Unit::colony_ship(), 1)]);
    let strong = Army::from([(Unit::colony_ship(), 4), (Unit::Ship(Ship::Cruiser), 30)]);
    let nearby = sampled_ship_finds(1.0, 80, strong.clone());
    let early = sampled_ship_finds(60.0, 2, strong.clone());
    let unescorted = sampled_ship_finds(60.0, 80, small);
    let remote = sampled_ship_finds(60.0, 80, strong);
    let lighter = sampled_ship_finds(
        60.0,
        80,
        Army::from([(Unit::colony_ship(), 4), (Unit::Ship(Ship::LightFighter), 30)]),
    );
    assert!((7_500..8_500).contains(&(10_000 - remote[0])), "{remote:?}");
    assert!(remote[0] < nearby[0] && remote[0] < unescorted[0]);
    assert!(remote[6..].iter().sum::<usize>() > nearby[6..].iter().sum::<usize>());
    assert!(remote[6..].iter().sum::<usize>() > 2_000, "{remote:?}");
    assert!((300..1_000).contains(&remote[8]), "War Sun finds: {remote:?}");
    assert!(remote[7] > early[7] * 3 && remote[8] > early[8] * 3);
    assert!(remote[7] > lighter[7] && remote[8] > lighter[8]);
    assert_eq!(unescorted[6..].iter().sum::<usize>(), 0);
}

#[test]
fn ship_recovery_limits_prevent_early_capitals_and_bound_extreme_fleets() {
    let strong = Army::from([(Unit::colony_ship(), 4), (Unit::Ship(Ship::Cruiser), 30)]);
    let early = ship_recoveries(60.0, 1, &strong);
    assert!(early
        .iter()
        .filter(|recovery| recovery.ship.production() >= 4)
        .all(|recovery| recovery.weight == 0));
    for army in [
        Army::from([(Unit::colony_ship(), 1)]),
        Army::from([(Unit::colony_ship(), usize::MAX), (Unit::war_sun(), usize::MAX)]),
    ] {
        for distance in [0.0, 1.0, 30.0, f64::MAX] {
            let chance = ship_discovery_probability(distance, army.total_production());
            assert!(chance.is_finite() && (0.0..=0.95).contains(&chance));
            for recovery in ship_recoveries(distance, usize::MAX, &army) {
                assert!(recovery.count <= 10_000);
                assert!(recovery.count * recovery.ship.price().total() <= 20_000);
                if recovery.ship == Ship::WarSun {
                    assert!(recovery.count <= 1);
                }
            }
        }
    }
}

#[test]
fn recovered_capitals_round_trip_in_snapshots_and_are_delivered_on_docking() {
    let mut model = game();
    let home = model.players[0].home_planet;
    let mut mission = Mission::new_expedition(
        900,
        1,
        1,
        model.map.get(home),
        site(&model),
        Army::from([(Unit::colony_ship(), 1)]),
    );
    mission.return_expedition(&model.map, 1);
    mission.position = model.map.get(home).position;
    let capitals = Army::from([(Unit::war_sun(), 1), (Unit::Ship(Ship::Dreadnought), 2)]);
    mission.expedition.as_mut().unwrap().ships = capitals.clone();
    assert!(mission.valid_expedition(&model.map));
    mission.expedition.as_mut().unwrap().ships.insert(Unit::war_sun(), 2);
    assert!(!mission.valid_expedition(&model.map));
    mission.expedition.as_mut().unwrap().ships = capitals.clone();
    let before = model.map.get(home).army.clone();
    model.missions.push(mission);
    let mut restored =
        PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap().state;
    let mut rng = restored.rng.clone().next_rng();
    resolve_arrivals(&mut restored, 2, &mut HashSet::new(), &mut rng);
    assert!(restored.missions.is_empty());
    for (unit, count) in capitals {
        assert_eq!(restored.map.get(home).army.amount(&unit), before.amount(&unit) + count);
    }
}

#[test]
fn recall_earns_nothing_and_lost_return_port_redirects_without_hostile_orders() {
    let mut model = game();
    let command = launch(&mut model);
    model = preview_commands(&model, 1, &[command]).unwrap();
    let home = model.players[0].home_planet;
    let port = model.map.planets.iter().find(|planet| planet.owned.is_none()).unwrap().id;
    model.map.get_mut(port).owned = Some(1);
    model.map.get_mut(port).controlled = Some(1);
    let mission = &mut model.missions[0];
    mission.recall(&model.map, model.turn as usize);
    assert!(mission.expedition.as_ref().unwrap().resources.is_empty());
    assert!(mission.expedition.as_ref().unwrap().ships.is_empty());
    model.map.get_mut(home).controlled = Some(2);
    model.map.get_mut(home).owned = Some(2);
    model.missions[0].return_expedition(&model.map, model.turn as usize);
    assert_eq!(model.missions[0].destination, port);
    assert_eq!(model.missions[0].objective, Icon::Deploy);
    assert!(model.missions[0].valid_expedition(&model.map));
}

#[test]
fn fuel_uses_normal_distance_and_reactor_discount_and_is_deducted_once() {
    let mut model = game();
    let command = launch(&mut model);
    let home = model.players[0].home_planet;
    let projected = preview_commands(&model, 1, std::slice::from_ref(&command)).unwrap();
    let mission = &projected.missions[0];
    let fuel = mission.fuel_consumption(&model.map);
    assert!(fuel > 0);
    assert_eq!(
        model.players[0].resources.deuterium - projected.players[0].resources.deuterium,
        fuel
    );
    let reactor = Unit::Building(Building::Reactor);
    let next_level = model.map.get(home).army.amount(&reactor) + 1;
    model.map.get_mut(home).army.insert(reactor, next_level);
    let discounted = preview_commands(&model, 1, std::slice::from_ref(&command)).unwrap();
    assert!(discounted.missions[0].fuel_consumption(&model.map) < fuel);
    model.players[0].resources.deuterium = 0;
    assert!(preview_commands(&model, 1, &[command]).is_err());
}

#[test]
fn colony_holds_are_large_and_discoveries_never_exceed_dispatched_capacity() {
    assert_eq!(cargo_capacity(&Army::from([(Unit::colony_ship(), 1)])), 200);
    assert_eq!(cargo_capacity(&Army::from([(Unit::colony_ship(), 9)])), 1_800);
    assert_eq!(
        cargo_capacity(&Army::from([
            (Unit::colony_ship(), 9),
            (Unit::Ship(Ship::HeavyFighter), 9)
        ])),
        1_800
    );
    for ship in Unit::ships() {
        if ship != Unit::colony_ship() {
            assert!(cargo_capacity(&Army::from([(ship, 1)])) < COLONY_SHIP_CARGO);
        }
    }
    assert_eq!(cargo_capacity(&Army::from([(Unit::colony_ship(), usize::MAX)])), usize::MAX);
    let model = game();
    let home = model.players[0].home_planet;
    for seed in 0..100 {
        let mut mission = Mission::new_expedition(
            900,
            model.turn as usize,
            1,
            model.map.get(home),
            site(&model),
            Army::from([(Unit::colony_ship(), 1)]),
        );
        discover(
            &mut mission,
            &model.map,
            2,
            &mut DeterministicRngState::from_u64(seed).next_rng(),
        );
        assert!(mission.expedition.as_ref().unwrap().resources.total() <= 200);
    }
}

#[test]
fn casualties_lose_excess_cargo_and_recovered_ships_do_not_enlarge_the_hold() {
    let mut model = game();
    let home = model.players[0].home_planet;
    let mut mission = Mission::new_expedition(
        900,
        model.turn as usize,
        1,
        model.map.get(home),
        site(&model),
        Army::from([(Unit::colony_ship(), 2), (Unit::Ship(Ship::HeavyFighter), 3)]),
    );
    mission.return_expedition(&model.map, model.turn as usize);
    let cargo = mission.expedition.as_mut().unwrap();
    cargo.resources = Resources::new(100, 100, 100);
    cargo.ships.insert(Unit::Ship(Ship::Dreadnought), 10);
    mission.army.insert(Unit::colony_ship(), 1);
    mission.position = site(&model);
    model.missions.push(mission);
    let mut rng = model.rng.clone().next_rng();
    resolve_arrivals(&mut model, 2, &mut HashSet::from([900]), &mut rng);
    assert_eq!(model.missions[0].expedition.as_ref().unwrap().resources.total(), 200);
    assert_eq!(model.missions[0].expedition.as_ref().unwrap().ships.values().sum::<usize>(), 10);
    assert!(model.missions[0].valid_expedition(&model.map));
}

#[test]
fn unescorted_expeditions_can_be_destroyed_by_space_fauna() {
    let mut model = game();
    model.rules.space_fauna_percent = 30;
    let home = model.players[0].home_planet;
    let mut mission = Mission::new_expedition(
        900,
        model.turn as usize,
        1,
        model.map.get(home),
        site(&model),
        Army::from([(Unit::colony_ship(), 1)]),
    );
    mission.travel_turns = 1;
    assert!(mission.duration(&model.map) > 1);
    model.missions.push(mission);
    let mut encountered = false;
    for seed in 0..30 {
        let mut attempt = model.clone();
        attempt.rng = DeterministicRngState::from_u64(seed);
        empty_turn(&mut attempt);
        if attempt.players[0].reports.iter().any(|report| {
            report.is_space_fauna_encounter()
                && report.mission.objective == Icon::Expedition
                && !report.surviving_attacker.has_army()
        }) {
            assert!(attempt.missions.is_empty());
            encountered = true;
            break;
        }
    }
    assert!(encountered);
}

#[test]
fn hostile_patrols_can_destroy_expeditions_outbound_and_returning() {
    for returning in [false, true] {
        let mut model = game();
        let point = site(&model);
        let home = model.players[0].home_planet;
        let mut expedition = Mission::new_expedition(
            900,
            1,
            1,
            model.map.get(home),
            point,
            Army::from([(Unit::colony_ship(), 1)]),
        );
        if returning {
            expedition.return_expedition(&model.map, 1);
        }
        expedition.position =
            point + (model.map.get(home).position - point).normalize_or_zero() * Planet::SIZE;
        let enemy_home = model.players[1].home_planet;
        let mut patrol = Mission::new_patrol(
            901,
            1,
            2,
            model.map.get(enemy_home),
            point,
            Army::from([(Unit::Ship(Ship::Cruiser), 100)]),
            None,
        );
        patrol.position = point;
        patrol.patrol.as_mut().unwrap().phase = PatrolPhase::OnStation;
        model.missions = vec![expedition, patrol];
        let mut detected = false;
        for seed in 0..10 {
            let mut attempt = model.clone();
            resolve_encounters(
                &mut attempt,
                2,
                &mut DeterministicRngState::from_u64(seed).next_rng(),
                &mut HashSet::new(),
            );
            if attempt.missions.iter().all(|mission| mission.id != 900) {
                assert!(attempt.players[0]
                    .reports
                    .iter()
                    .any(|report| report.mission.objective == Icon::Patrol));
                detected = true;
                break;
            }
        }
        assert!(detected, "patrol never engaged expedition, returning={returning}");
    }
}
