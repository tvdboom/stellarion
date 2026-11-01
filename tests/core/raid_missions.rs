use super::*;
use crate::core::raids::{cargo_capacity, independent_cache};

fn raid_game() -> (GameModel, usize, usize) {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    let target = model.players[1].home_planet;
    model.map.get_mut(target).army =
        Army::from([(Unit::Defense(Defense::RocketLauncher), 2)]).into();
    (model, home, target)
}

fn arrive(model: &mut GameModel, target: PlanetId, id: u64, objective: Icon, army: Army) {
    let home = model.players[0].home_planet;
    let mut mission = Mission::new_with_id(
        id,
        model.turn as usize,
        1,
        model.map.get(home),
        model.map.get(target),
        objective,
        army,
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.position = model.map.get(target).position;
    model.missions.push(mission);
}

fn raiders() -> Army {
    Army::from([(Unit::Ship(Ship::Battleship), 10), (Unit::probe(), 5)])
}

#[test]
fn raid_moon_targets_are_rejected_but_lunar_departures_and_returns_remain_valid() {
    let (mut model, home, target) = raid_game();
    let moon = model.map.moons()[0].id;
    model.map.get_mut(home).army.extend(raiders());
    model.players[0].resources.deuterium = 1_000_000;
    let command = |origin, destination| TurnCommand::SendMission {
        mission_id: 99_001,
        origin,
        destination,
        objective: Icon::Raid,
        army: raiders(),
        bombing: BombingRaid::None,
        combat_probes: false,
        battle_plan: Default::default(),
        deep_cover: false,
        jump_gate: false,
    };
    assert!(preview_commands(&model, 1, &[command(home, target)]).is_ok());
    for controller in [None, Some(2)] {
        model.map.get_mut(moon).controlled = controller;
        let before = serde_json::to_value(&model).unwrap();
        assert!(matches!(
            preview_commands(&model, 1, &[command(home, moon)]),
            Err(GameError::InvalidCommand { .. })
        ));
        let turn = model.turn;
        assert!(matches!(
            resolve_turn(
                &mut model,
                &[
                    TurnSubmission::new(1, turn, vec![command(home, moon)]),
                    TurnSubmission::new(2, turn, vec![]),
                ]
            ),
            Err(GameError::InvalidCommand { .. })
        ));
        assert_eq!(serde_json::to_value(&model).unwrap(), before);
        arrive(&mut model, moon, 99_002, Icon::Raid, raiders());
        assert!(model.validate().is_err());
        let snapshot = serde_json::to_value(PersistedGame::new(model.clone())).unwrap();
        assert!(PersistedGame::from_json(snapshot).is_err());
        model.missions.clear();
    }
    let origin = model.map.get_mut(moon);
    origin.controlled = Some(1);
    origin.army = raiders().into();
    assert!(preview_commands(&model, 1, &[command(moon, target)]).is_ok());
    let mut returning = objective_return_mission(
        99_003,
        model.turn as usize,
        1,
        model.map.get(target),
        model.map.get(moon),
        raiders(),
        "",
        Icon::Raid,
        Default::default(),
    );
    returning.raid_cargo = Resources::new(1, 2, 3);
    model.missions.push(returning);
    assert!(model.validate().is_ok(), "a returning raider can still dock at its lunar origin");
}

#[test]
fn raid_victory_transfers_resources_preserves_colony_and_returns_all_survivors_once() {
    let (mut model, home, target) = raid_game();
    let start = model.players.iter().map(|player| player.resources).collect::<Vec<_>>();
    let production = model
        .players
        .iter()
        .map(|player| player.resource_production(&model.map))
        .collect::<Vec<_>>();
    arrive(&mut model, target, 90_001, Icon::Raid, raiders());
    let mut replay = model.clone();
    empty_turn(&mut model);
    empty_turn(&mut replay);
    assert_eq!(
        PersistedGame::new(model.clone()).to_json().unwrap(),
        PersistedGame::new(replay).to_json().unwrap()
    );
    let report = model.players[0].reports.last().unwrap();
    assert_eq!(report.winner(), Some(1));
    assert!(report.combat_report.is_some());
    assert!(!report.raid_loot.is_empty());
    assert_eq!(model.map.get(target).owned, Some(2));
    assert_eq!(model.map.get(target).controlled, Some(2));
    assert!(!model.map.get(target).army.has_fleet());
    let available = start[1] + production[1];
    assert!(available.contains(report.raid_loot));
    let mut carriers = report.surviving_attacker.clone();
    *carriers.get_mut(&Unit::probe()).unwrap() -= report.scout_probes;
    assert_eq!(report.raid_loot.total(), cargo_capacity(&carriers).min(available.total()));
    assert_eq!(
        model.players[0].resources,
        start[0] + production[0],
        "loot is still aboard the fleet"
    );
    assert_eq!(model.players[1].resources, available - report.raid_loot);
    let returning = model.missions.iter().filter(|mission| mission.owner == 1).collect::<Vec<_>>();
    assert_eq!(returning.len(), 1, "scouts must not be duplicated into a second return");
    assert_eq!(returning[0].destination, home);
    assert_eq!(returning[0].return_objective, Some(Icon::Raid));
    assert_eq!(returning[0].army, report.surviving_attacker);
    assert_eq!(returning[0].raid_cargo, report.raid_loot);
    assert_eq!(model.players[1].reports.last().unwrap().raid_loot, report.raid_loot);
    let survivors = report.surviving_attacker.clone();
    let loot = report.raid_loot;
    let resources_before_return = model.players[0].resources;
    let next_production = model.players[0].resource_production(&model.map);
    let home_fleet = model.map.get(home).army.controller().clone();
    model.missions[0].position = model.map.get(home).position;
    let saved = PersistedGame::new(model).to_json().unwrap();
    let mut model = PersistedGame::from_json(saved).unwrap().state;
    empty_turn(&mut model);
    assert!(model.missions.is_empty());
    assert_eq!(model.players[0].resources, resources_before_return + next_production + loot);
    for (unit, count) in survivors {
        assert_eq!(model.map.get(home).army.amount(&unit), home_fleet.amount(&unit) + count);
    }
}

#[test]
fn merged_returning_raids_deliver_each_cargo_once() {
    let (mut model, home, target) = raid_game();
    let start = model.players[0].resources;
    let production = model.players[0].resource_production(&model.map);
    for (id, cargo) in [(97_001, Resources::new(10, 0, 0)), (97_002, Resources::new(0, 7, 3))] {
        let mut returning = objective_return_mission(
            id,
            model.turn as usize,
            1,
            model.map.get(target),
            model.map.get(home),
            Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
            "",
            Icon::Raid,
            Default::default(),
        );
        returning.raid_cargo = cargo;
        returning.position = model.map.get(home).position;
        model.missions.push(returning);
    }
    empty_turn(&mut model);
    let delivered = Resources::new(10, 7, 3);
    assert_eq!(model.players[0].resources, start + production + delivered);
    assert!(model.missions.is_empty());
    let next_production = model.players[0].resource_production(&model.map);
    empty_turn(&mut model);
    assert_eq!(model.players[0].resources, start + production + delivered + next_production);
}

#[test]
fn returning_raid_reroutes_cargo_when_its_origin_is_lost() {
    let (mut model, home, target) = raid_game();
    let lost = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.owned.is_none())
        .unwrap()
        .id;
    let mut returning = objective_return_mission(
        98_000,
        model.turn as usize,
        1,
        model.map.get(target),
        model.map.get(lost),
        Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
        "",
        Icon::Raid,
        Default::default(),
    );
    returning.raid_cargo = Resources::new(3, 4, 3);
    check_mission(&mut returning, &model.map, model.turn as usize, 10, Some(home));
    assert_eq!(returning.objective, Icon::Deploy);
    assert_eq!(returning.destination, home);
    assert_eq!(returning.raid_cargo, Resources::new(3, 4, 3));
    returning.position = model.map.get(home).position;
    let before = model.players[0].resources;
    let production = model.players[0].resource_production(&model.map);
    model.missions.push(returning);
    empty_turn(&mut model);
    assert_eq!(model.players[0].resources, before + production + Resources::new(3, 4, 3));
}

#[test]
fn raids_keep_stealing_from_players_and_stop_after_abandonment_with_a_remaining_fleet() {
    let (mut model, _, _) = raid_game();
    let target = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.owned.is_none())
        .unwrap()
        .id;
    model.map.get_mut(target).colonize(2);
    model.players[1].resources = Resources::new(50_000, 50_000, 50_000);
    let mut amounts = Vec::new();
    for id in 1..=3 {
        arrive(&mut model, target, 91_000 + id, Icon::Raid, raiders());
        empty_turn(&mut model);
        amounts.push(model.players[0].reports.last().unwrap().raid_loot.total());
        model.missions.clear();
    }
    assert!(amounts[0] > 0);
    assert_eq!(amounts, vec![amounts[0]; 3]);
    model.map.get_mut(target).army.insert(Unit::probe(), 1);
    arrive(&mut model, target, 92_000, Icon::Raid, raiders());
    let submissions = vec![
        TurnSubmission::new(1, model.turn, vec![]),
        TurnSubmission::new(
            2,
            model.turn,
            vec![TurnCommand::AbandonPlanet {
                planet_id: target,
            }],
        ),
    ];
    resolve_turn(&mut model, &submissions).unwrap();
    let report = model.players[0].reports.last().unwrap();
    assert_eq!(report.winner(), Some(1));
    assert_eq!(report.planet.controlled, Some(2));
    assert!(report.raid_loot.is_empty());
    assert_eq!(model.map.get(target).owned, None);
    assert_eq!(model.map.get(target).controlled, Some(2));
}

#[test]
fn npc_raid_cache_diminishes_across_saves_until_permanently_exhausted() {
    let (mut model, _, _) = raid_game();
    let target = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.owned.is_none())
        .unwrap()
        .id;
    let army = Army::from([(Unit::Defense(Defense::RocketLauncher), 5)]);
    let cache = independent_cache(&army);
    let planet = model.map.get_mut(target);
    planet.independent_population = IndependentPopulation::Inhabited;
    planet.raid_cache = cache;
    planet.army = army.into();
    let mut stolen = Resources::default();
    let mut previous = usize::MAX;
    for id in 0..32 {
        arrive(&mut model, target, 93_000 + id, Icon::Raid, raiders());
        empty_turn(&mut model);
        let report = model.players[0].reports.last().unwrap();
        assert_eq!(report.winner(), Some(1));
        assert!(report.raid_loot.total() <= previous);
        previous = report.raid_loot.total();
        stolen += report.raid_loot;
        assert_eq!(model.map.get(target).owned, None);
        assert_eq!(model.map.get(target).controlled, None);
        assert_eq!(model.map.get(target).raid_cache, cache - stolen);
        model.missions.clear();
        model =
            PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap().state;
    }
    assert_eq!(stolen, cache);
    assert_eq!(previous, 0);
}

#[test]
fn raid_discovers_independents_but_empty_worlds_never_pay_loot() {
    let mut inhabited = 0;
    let mut empty = 0;
    for seed in 1..=16 {
        let mut model = GameModel::new(
            [seed; 32],
            GameRules {
                independent_populations: true,
                ..Default::default()
            },
        )
        .unwrap();
        model.start().unwrap();
        let target = model
            .map
            .planets
            .iter()
            .find(|planet| planet.independent_population == IndependentPopulation::Unrevealed)
            .unwrap()
            .id;
        arrive(&mut model, target, 94_000, Icon::Raid, raiders());
        empty_turn(&mut model);
        let report = model.players[0].reports.last().unwrap();
        if report.planet.has_independent_population() {
            inhabited += 1;
            assert!(!report.raid_loot.is_empty());
            assert_eq!(
                report.planet.raid_cache,
                independent_cache(report.planet.army.controller())
            );
        } else {
            empty += 1;
            assert!(report.raid_loot.is_empty());
            assert!(model.map.get(target).raid_cache.is_empty());
        }
    }
    assert!(inhabited > 0 && empty > 0);
}

#[test]
fn raid_defeat_yields_no_loot_and_never_captures_the_world() {
    let (mut model, _, target) = raid_game();
    model.map.get_mut(target).army = Army::from([(Unit::war_sun(), 10)]).into();
    arrive(
        &mut model,
        target,
        95_000,
        Icon::Raid,
        Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
    );
    empty_turn(&mut model);
    let report = model.players[0].reports.last().unwrap();
    assert_eq!(report.winner(), Some(2));
    assert!(report.raid_loot.is_empty());
    assert!(!report.surviving_attacker.has_army());
    assert!(model.missions.is_empty());
    assert_eq!(model.map.get(target).owned, Some(2));
    assert_eq!(model.map.get(target).controlled, Some(2));
}

#[test]
fn raid_arrivals_never_merge_with_conquest_and_launch_obeys_normal_fleet_rules() {
    let (mut model, home, target) = raid_game();
    model.map.get_mut(home).army.extend(raiders());
    let command = TurnCommand::SendMission {
        mission_id: 96_000,
        origin: home,
        destination: target,
        objective: Icon::Raid,
        army: raiders(),
        bombing: BombingRaid::None,
        combat_probes: false,
        battle_plan: Default::default(),
        deep_cover: false,
        jump_gate: false,
    };
    model.players[0].resources.deuterium = 1_000_000;
    let preview = preview_commands(&model, 1, &[command]).unwrap();
    assert_eq!(preview.missions[0].objective, Icon::Raid);
    assert!(preview.missions[0].objective.is_recallable());
    assert!(Icon::Raid.is_hostile_action());
    assert!(!Icon::Raid.accepts_army(&Army::from([(Unit::probe(), 5)])));
    arrive(&mut model, target, 96_001, Icon::Raid, raiders());
    arrive(&mut model, target, 96_002, Icon::Attack, raiders());
    let grouped = regroup_missions(&model.missions);
    assert_eq!(grouped.len(), 2);
    empty_turn(&mut model);
    assert_eq!(model.map.get(target).controlled, Some(1), "the independent Attack still conquers");
    assert!(model.players[0].reports.iter().any(|report| report.mission.objective == Icon::Raid));
    assert!(model.players[0].reports.iter().any(|report| report.mission.objective == Icon::Attack));
}
