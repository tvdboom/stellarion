use super::*;

fn cover_game(
    origin_level: usize,
    destination_level: usize,
    probes: usize,
) -> (GameModel, Mission) {
    let mut model = started_model(3);
    let owner = model.players[0].id;
    let origin = model.players[0].home_planet;
    let target = model.players[1].home_planet;
    let relay = Unit::Building(Building::CommandRelay);
    model.players[0].resources.deuterium = 100_000;
    model.map.get_mut(origin).army.insert(Unit::probe(), probes);
    if origin_level > 0 {
        model.map.get_mut(origin).army.insert(relay, origin_level);
    }
    let destination = model.map.get_mut(target);
    destination.army = Army::from([(Unit::Defense(Defense::RocketLauncher), 100)]).into();
    if destination_level > 0 {
        destination.army.insert(relay, destination_level);
    }
    let mission = Mission::new_with_id(
        8_001,
        model.turn as usize,
        owner,
        model.map.get(origin),
        model.map.get(target),
        Icon::Spy,
        Army::from([(Unit::probe(), probes)]),
        BombingRaid::None,
        false,
        false,
        None,
    )
    .with_deep_cover(true);
    (model, mission)
}

fn launch(mission: &Mission) -> TurnCommand {
    TurnCommand::SendMission {
        mission_id: mission.id,
        origin: mission.origin,
        destination: mission.destination,
        objective: mission.objective,
        army: mission.army.clone(),
        bombing: mission.bombing.clone(),
        combat_probes: mission.combat_probes,
        deep_cover: mission.deep_cover,
        jump_gate: mission.jump_gate,
    }
}

#[test]
fn higher_relay_scans_without_losses_deception_or_defender_and_protector_reports() {
    for (origin_level, destination_level, probes) in [(1, 0, 5), (2, 1, 5), (5, 4, 25)] {
        for deception_active in [false, true] {
            let (mut model, mut mission) = cover_game(origin_level, destination_level, probes);
            let owner = mission.owner;
            let target = mission.destination;
            let protector = model.players[2].id;
            model.map.get_mut(mission.origin).command_relay_active = deception_active;
            let destination = model.map.get_mut(target);
            destination.command_relay_active = deception_active;
            destination.protection_permissions.insert(protector);
            destination
                .dock_protecting_fleet(protector, Army::from([(Unit::Ship(Ship::Battleship), 10)]));
            let defenses = destination.army.clone();
            mission.position = destination.position;
            model.missions.push(mission);
            empty_turn(&mut model);

            let report = model.player(owner).unwrap().reports.last().unwrap();
            assert!(report.mission.deep_cover);
            assert!(!report.hidden);
            assert!(report.combat_report.is_none());
            assert_eq!(report.scout_probes, probes);
            assert_eq!(report.surviving_attacker.amount(&Unit::probe()), probes);
            assert_eq!(report.planet.army, defenses, "the scan must not receive fake telemetry");
            assert_eq!(model.map.get(target).army, defenses);
            assert!(model.players[1].reports.is_empty());
            assert!(model.players[2].reports.is_empty());
            let returning = model.missions.iter().find(|mission| mission.owner == owner).unwrap();
            assert_eq!(returning.return_objective, Some(Icon::Spy));
            assert_eq!(returning.army.amount(&Unit::probe()), probes);
            assert!(!returning.deep_cover);
            assert!(returning.is_seen_by_phalanx(&model.map, &model.players[1]).is_none());
            let snapshot = PersistedGame::new(model).to_json().unwrap();
            assert!(PersistedGame::from_json(snapshot).is_ok());
        }
    }
}

#[test]
fn equal_or_lower_relay_resolves_the_same_combat_as_normal_spying() {
    // Thirty probes exceed every relay's diversion threshold, so normal spying fights.
    for (origin_level, destination_level) in [(1, 1), (1, 2), (5, 5)] {
        let (mut covered, mut mission) = cover_game(origin_level, destination_level, 30);
        mission.position = covered.map.get(mission.destination).position;
        covered.missions.push(mission);
        let mut normal = covered.clone();
        normal.missions[0].deep_cover = false;
        empty_turn(&mut covered);
        empty_turn(&mut normal);
        let report = covered.players[0].reports.last().unwrap();
        let ordinary = normal.players[0].reports.last().unwrap();
        let combat = report.combat_report.as_ref().expect("failed cover must enter combat");
        assert_eq!(combat.rounds.len(), 1);
        assert_eq!(report.scout_probes, ordinary.scout_probes);
        assert_eq!(report.surviving_attacker, ordinary.surviving_attacker);
        assert_eq!(
            serde_json::to_value(combat).unwrap(),
            serde_json::to_value(ordinary.combat_report.as_ref().unwrap()).unwrap()
        );
        assert!(!covered.players[1].reports.is_empty());
    }
}

#[test]
fn failed_cover_enters_combat_even_for_a_group_small_enough_to_divert() {
    let (mut model, mut mission) = cover_game(1, 1, 5);
    mission.position = model.map.get(mission.destination).position;
    model.missions.push(mission);
    empty_turn(&mut model);
    let report = model.players[0].reports.last().unwrap();
    assert!(!report.planet.army.is_empty());
    assert_eq!(report.combat_report.as_ref().unwrap().rounds.len(), 1);
    assert!(!model.players[1].reports.is_empty());
}

#[test]
fn cover_uses_completed_relay_levels_at_arrival() {
    for (launch_level, arrival_level, destination_level, succeeds) in
        [(3, 1, 2, false), (1, 3, 2, true), (1, 0, 0, false)]
    {
        let (mut model, mut mission) = cover_game(launch_level, destination_level, 30);
        let relay = Unit::Building(Building::CommandRelay);
        model.map.get_mut(mission.origin).army.remove(&relay);
        if arrival_level > 0 {
            model.map.get_mut(mission.origin).army.insert(relay, arrival_level);
        }
        mission.position = model.map.get(mission.destination).position;
        model.missions.push(mission);
        empty_turn(&mut model);
        assert_eq!(model.players[1].reports.is_empty(), succeeds);
        assert_eq!(model.players[0].reports.last().unwrap().combat_report.is_none(), succeeds);
    }
}

#[test]
fn deep_cover_launch_requires_a_built_relay_spy_objective_and_extra_fuel() {
    let (mut model, mission) = cover_game(0, 0, 5);
    let command = launch(&mission);
    assert!(preview_commands(&model, mission.owner, std::slice::from_ref(&command)).is_err());
    model.map.get_mut(mission.origin).buy.push(Unit::Building(Building::CommandRelay));
    assert!(preview_commands(&model, mission.owner, std::slice::from_ref(&command)).is_err());
    model.map.get_mut(mission.origin).buy.clear();
    model.map.get_mut(mission.origin).army.insert(Unit::Building(Building::CommandRelay), 1);
    let mut wrong_objective = mission.clone();
    wrong_objective.objective = Icon::Attack;
    assert_eq!(
        validate_mission(
            model.player(mission.owner).unwrap(),
            &model.map,
            model.map.get(mission.origin),
            model.map.get(mission.destination),
            &wrong_objective,
        ),
        Err(crate::core::orders::OrderError::DeepCover)
    );
    model.players[0].resources.deuterium =
        mission.clone().with_deep_cover(false).fuel_consumption(&model.map);
    assert!(preview_commands(&model, mission.owner, std::slice::from_ref(&command)).is_err());
    model.players[0].resources.deuterium = mission.fuel_consumption(&model.map);
    let launched = preview_commands(&model, mission.owner, &[command]).unwrap();
    assert_eq!(launched.players[0].resources.deuterium, 0);
    assert!(launched.missions[0].deep_cover);
}

#[test]
fn surcharge_is_paid_for_any_attempt_undiscounted_and_refunded_on_launch_cancellation() {
    for destination_level in [0, 3, 5] {
        let (mut model, mission) = cover_game(3, destination_level, 30);
        model.map.get_mut(mission.origin).army.insert(Unit::Building(Building::Reactor), 5);
        let ordinary = mission.clone().with_deep_cover(false);
        assert_eq!(
            mission.fuel_consumption(&model.map),
            ordinary.fuel_consumption(&model.map) + 300
        );
        let commands = vec![
            launch(&mission),
            TurnCommand::RecallMission {
                mission_id: mission.id,
            },
        ];
        let canceled = preview_commands(&model, mission.owner, &commands).unwrap();
        assert_eq!(canceled.players[0].resources, model.players[0].resources);
        assert!(canceled.missions.is_empty());
        assert_eq!(canceled.map.get(mission.origin).army, model.map.get(mission.origin).army);

        let submission = TurnSubmission::new(mission.owner, model.turn, vec![launch(&mission)]);
        let json = serde_json::to_value(&submission).unwrap();
        assert_eq!(json["commands"][0]["deep_cover"], true);
        let decoded: TurnSubmission = serde_json::from_value(json).unwrap();
        let launched = preview_commands(&model, mission.owner, &decoded.commands).unwrap();
        let snapshot = PersistedGame::new(launched).to_json().unwrap();
        let restored = PersistedGame::from_json(snapshot).unwrap();
        assert!(restored.state.missions[0].deep_cover);
    }
}

#[test]
fn simultaneous_cover_attempts_keep_their_origins_and_do_not_merge_with_normal_spies() {
    let (mut model, mut strong) = cover_game(3, 2, 30);
    let weak_origin = model.map.planets.iter().find(|planet| planet.owned.is_none()).unwrap().id;
    model.map.get_mut(weak_origin).colonize(strong.owner);
    model.map.get_mut(weak_origin).army.insert(Unit::Building(Building::CommandRelay), 1);
    strong.position = model.map.get(strong.destination).position;
    let mut weak = strong.clone();
    weak.id += 1;
    weak.origin = weak_origin;
    let mut normal = strong.clone();
    normal.id += 2;
    normal.deep_cover = false;
    model.missions.extend([strong, weak, normal]);
    empty_turn(&mut model);
    assert_eq!(model.players[0].reports.len(), 3);
    assert_eq!(model.players[1].reports.len(), 2);
    let successful = model.players[0]
        .reports
        .iter()
        .filter(|report| report.combat_report.is_none())
        .collect::<Vec<_>>();
    assert_eq!(successful.len(), 1);
    assert_eq!(successful[0].mission.id, 8_001);
    assert_eq!(successful[0].scout_probes, 30);
}

#[test]
fn deep_cover_fuel_saturates_and_non_spy_snapshots_reject_the_option() {
    let (mut model, mut mission) = cover_game(1, 0, 5);
    mission.army.insert(Unit::probe(), usize::MAX);
    assert_eq!(mission.deep_cover_cost(), usize::MAX);
    assert_eq!(mission.fuel_consumption(&model.map), usize::MAX);
    mission.army.insert(Unit::probe(), 5);
    mission.objective = Icon::Attack;
    model.missions.push(mission);
    assert!(model.validate().is_err());
}
