use super::*;

use crate::core::combat::plan::BattlePlan;
use crate::core::constants::DEEP_COVER_DEUTERIUM_PER_PROBE;
use crate::core::identity::PlayerId;
use crate::core::map::planet::{PlanetId, PlanetKind};
use crate::core::missions::BombingRaid;
use crate::core::orders::validate_mission;
use crate::core::resources::Resources;
use crate::core::simulation::{
    preview_commands, resolve_turn, GameModel, GameRules, PersistedGame, TurnCommand,
    TurnSubmission,
};
use crate::core::units::ships::Ship;
use crate::core::units::Army;
use crate::test_support::empty_report;

fn game() -> (GameModel, PlanetId, PlanetId) {
    let mut model = GameModel::new(
        [73; 32],
        GameRules {
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
    let origin = model.players[0].home_planet;
    let target = model.players[1].home_planet;
    model.map.get_mut(origin).army.insert(Unit::probe(), 30);
    model.map.get_mut(target).army.clear();
    for target_type in SabotageTarget::ALL {
        model.map.get_mut(target).army.insert(Unit::Building(target_type.building()), 1);
    }
    (model, origin, target)
}

fn disclose(model: &mut GameModel, owner: PlayerId, origin: PlanetId, target: PlanetId) {
    let mission = Mission::new_with_id(
        800,
        model.turn as usize,
        owner,
        model.map.get(origin),
        model.map.get(target),
        Icon::Spy,
        Army::from([(Unit::probe(), 25)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let mut report = empty_report(mission, model.map.get(target).clone());
    report.id = 801;
    report.turn = model.turn as usize;
    report.scout_probes = 25;
    report.surviving_defender = model.map.get(target).army.clone();
    report.destination_owned = Some(2);
    report.destination_controlled = Some(2);
    model.player_mut(owner).unwrap().push_report(report);
}

fn command(origin: PlanetId, destination: PlanetId, target: SabotageTarget) -> TurnCommand {
    TurnCommand::SendSabotage {
        mission_id: 900,
        origin,
        destination,
        target,
        army: Army::from([(Unit::probe(), 5)]),
        deep_cover: false,
        battle_plan: BattlePlan::default(),
    }
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
fn only_observed_structures_or_public_railguns_can_be_selected() {
    let (mut model, origin, target) = game();
    assert_eq!(
        known_targets(&model.players[0], model.map.get(target), &[]),
        vec![SabotageTarget::OrbitalRailgun]
    );
    assert!(preview_commands(&model, 1, &[command(origin, target, SabotageTarget::SensorPhalanx)])
        .is_err());
    disclose(&mut model, 1, origin, target);
    assert_eq!(known_targets(&model.players[0], model.map.get(target), &[]), SabotageTarget::ALL);
    assert!(preview_commands(&model, 1, &[command(origin, target, SabotageTarget::SensorPhalanx)])
        .is_ok());
    let moon = model
        .map
        .planets
        .iter_mut()
        .find(|world| world.id != origin && world.id != target)
        .unwrap();
    moon.kind = PlanetKind::Gray;
    moon.army.insert(Unit::Building(Building::OrbitalRailgun), 1);
    assert!(known_targets(&model.players[0], moon, &[]).is_empty());
    // A friendly world is never a valid sabotage target, even with complete knowledge.
    assert!(known_targets(&model.players[0], model.map.get(origin), &[]).is_empty());
}

#[test]
fn probe_only_minimum_ownership_target_and_fuel_are_validated() {
    let (mut model, origin, target) = game();
    disclose(&mut model, 1, origin, target);
    let base = command(origin, target, SabotageTarget::JumpGate);
    for army in [
        Army::from([(Unit::probe(), 4)]),
        Army::from([(Unit::probe(), 31)]),
        Army::from([(Unit::probe(), 5), (Unit::colony_ship(), 1)]),
    ] {
        let mut invalid = base.clone();
        if let TurnCommand::SendSabotage {
            army: selected,
            ..
        } = &mut invalid
        {
            *selected = army;
        }
        assert!(preview_commands(&model, 1, &[invalid]).is_err());
    }
    let projected = preview_commands(&model, 1, std::slice::from_ref(&base)).unwrap();
    let mission = &projected.missions[0];
    assert_eq!(
        model.players[0].resources.deuterium - projected.players[0].resources.deuterium,
        mission.fuel_consumption(&model.map)
    );
    assert_eq!(mission.image(&model.players[0]), "mission spy");
    model.players[0].resources.deuterium = 0;
    assert!(preview_commands(&model, 1, &[base]).is_err());
}

#[test]
fn success_disables_only_selected_structure_for_one_planning_turn_and_replays() {
    for selected in SabotageTarget::ALL {
        let (mut model, origin, target) = game();
        disclose(&mut model, 1, origin, target);
        model = preview_commands(&model, 1, &[command(origin, target, selected)]).unwrap();
        model.missions[0].position = model.map.get(target).position;
        empty_turn(&mut model);
        for target_type in SabotageTarget::ALL {
            assert_eq!(
                model.map.get(target).operational_level(target_type.building()),
                usize::from(target_type != selected)
            );
            assert_eq!(
                model.map.get(target).army.amount(&Unit::Building(target_type.building())),
                1
            );
        }
        assert!(model.players[0].reports.last().unwrap().mission.sabotage.unwrap().succeeded);
        assert!(known_targets(&model.players[0], model.map.get(target), &[]).contains(&selected));
        assert!(model
            .missions
            .iter()
            .any(|mission| mission.return_objective == Some(Icon::Sabotage)));
        let mut restored =
            PersistedGame::from_json(PersistedGame::new(model.clone()).to_json().unwrap())
                .unwrap()
                .state;
        let preview = preview_commands(&model, 2, &[]).unwrap();
        assert_eq!(preview.map.get(target).sabotage, model.map.get(target).sabotage);
        empty_turn(&mut model);
        empty_turn(&mut restored);
        assert_eq!(serde_json::to_value(&model).unwrap(), serde_json::to_value(restored).unwrap());
        assert_eq!(model.map.get(target).operational_level(selected.building()), 1);
    }
}

#[test]
fn stale_intelligence_and_destroyed_probes_do_not_disable_structures() {
    for orbital_removed in [true, false] {
        let (mut model, origin, target) = game();
        disclose(&mut model, 1, origin, target);
        if orbital_removed {
            model.map.get_mut(target).army.remove(&Unit::Building(Building::SensorPhalanx));
        } else {
            model.map.get_mut(target).army.insert(Unit::Ship(Ship::Cruiser), 100);
        }
        model =
            preview_commands(&model, 1, &[command(origin, target, SabotageTarget::SensorPhalanx)])
                .unwrap();
        model.missions[0].position = model.map.get(target).position;
        empty_turn(&mut model);
        assert_eq!(model.map.get(target).sabotage, SabotageState::default());
        assert!(!model.players[0].reports.last().unwrap().mission.sabotage.unwrap().succeeded);
    }
}

#[test]
fn active_relay_cannot_divert_sabotage_of_a_known_orbital() {
    let (mut model, origin, target) = game();
    let planet = model.map.get_mut(target);
    planet.army.insert(Unit::Building(Building::CommandRelay), 1);
    assert!(planet.command_relay_diverts(5));
    disclose(&mut model, 1, origin, target);

    let selected = SabotageTarget::SensorPhalanx;
    model = preview_commands(&model, 1, &[command(origin, target, selected)]).unwrap();
    model.missions[0].position = model.map.get(target).position;
    empty_turn(&mut model);

    assert!(model.map.get(target).sabotage.sensor_phalanx);
    let report = model.players[0].reports.last().unwrap();
    assert!(report.mission.sabotage.unwrap().succeeded);
    assert_eq!(report.planet.army.amount(&Unit::Building(selected.building())), 1);
}

#[test]
fn different_targets_and_spy_arrivals_never_merge_into_one_sabotage_order() {
    let (mut model, origin, target) = game();
    disclose(&mut model, 1, origin, target);
    let first = command(origin, target, SabotageTarget::SensorPhalanx);
    let mut second = command(origin, target, SabotageTarget::JumpGate);
    if let TurnCommand::SendSabotage {
        mission_id,
        ..
    } = &mut second
    {
        *mission_id = 901;
    }
    model = preview_commands(&model, 1, &[first, second]).unwrap();
    for mission in &mut model.missions {
        mission.position = model.map.get(target).position;
    }
    empty_turn(&mut model);
    assert!(
        model.map.get(target).sabotage.sensor_phalanx && model.map.get(target).sabotage.jump_gate
    );
    assert!(!model.map.get(target).sabotage.orbital_railgun);
}

#[test]
fn disabled_structures_cannot_detect_fire_or_jump_from_either_endpoint() {
    let (mut model, home, target) = game();
    model.map.get_mut(home).position = bevy::math::Vec2::X * 150.0;
    model.map.get_mut(target).position = bevy::math::Vec2::ZERO;
    let mut incoming = Mission::new_with_id(
        990,
        1,
        1,
        model.map.get(home),
        model.map.get(target),
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    incoming.position = model.map.get(target).position;
    assert!(incoming.is_seen_by_phalanx(&model.map, &model.players[1]).is_some());
    model.map.get_mut(target).sabotage.sensor_phalanx = true;
    assert!(incoming.is_seen_by_phalanx(&model.map, &model.players[1]).is_none());

    let fire = TurnCommand::FireOrbitalRailguns {
        target: home,
    };
    assert!(preview_commands(&model, 2, std::slice::from_ref(&fire)).is_ok());
    model.map.get_mut(target).sabotage.orbital_railgun = true;
    assert!(preview_commands(&model, 2, &[fire]).is_err());

    let port = model
        .map
        .planets
        .iter()
        .find(|planet| planet.owned.is_none() && !planet.is_moon())
        .unwrap()
        .id;
    model.map.get_mut(port).owned = Some(2);
    model.map.get_mut(port).controlled = Some(2);
    model.map.get_mut(port).army.insert(Unit::Building(Building::JumpGate), 1);
    model.map.get_mut(target).army.insert(Unit::probe(), 5);
    let jump = Mission::new_with_id(
        991,
        1,
        2,
        model.map.get(target),
        model.map.get(port),
        Icon::Deploy,
        Army::from([(Unit::probe(), 5)]),
        BombingRaid::None,
        false,
        true,
        None,
    );
    assert!(validate_mission(
        &model.players[1],
        &model.map,
        model.map.get(target),
        model.map.get(port),
        &jump
    )
    .is_ok());
    model.map.get_mut(target).sabotage.jump_gate = true;
    assert!(validate_mission(
        &model.players[1],
        &model.map,
        model.map.get(target),
        model.map.get(port),
        &jump
    )
    .is_err());
    model.map.get_mut(target).sabotage.jump_gate = false;
    model.map.get_mut(port).sabotage.jump_gate = true;
    assert!(validate_mission(
        &model.players[1],
        &model.map,
        model.map.get(target),
        model.map.get(port),
        &jump
    )
    .is_err());
}

#[test]
fn deep_cover_pays_normal_probe_surcharge_and_sabotages_without_combat_when_relay_is_higher() {
    let (mut model, origin, target) = game();
    model.map.get_mut(origin).army.insert(Unit::Building(Building::CommandRelay), 1);
    disclose(&mut model, 1, origin, target);
    let base = command(origin, target, SabotageTarget::JumpGate);
    let ordinary = preview_commands(&model, 1, std::slice::from_ref(&base)).unwrap();
    let mut covert = base;
    if let TurnCommand::SendSabotage {
        deep_cover,
        ..
    } = &mut covert
    {
        *deep_cover = true;
    }
    let mut covered = preview_commands(&model, 1, &[covert]).unwrap();
    assert_eq!(covered.missions[0].deep_cover_cost(), 5 * DEEP_COVER_DEUTERIUM_PER_PROBE);
    assert_eq!(
        ordinary.players[0].resources.deuterium - covered.players[0].resources.deuterium,
        covered.missions[0].deep_cover_cost()
    );
    covered.missions[0].position = covered.map.get(target).position;
    empty_turn(&mut covered);
    assert!(covered.map.get(target).sabotage.jump_gate);
    assert!(covered.players[0].reports.last().unwrap().combat_report.is_none());
    assert!(covered.players[1].reports.is_empty());
}
