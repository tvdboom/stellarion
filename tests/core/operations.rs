use super::*;
use crate::core::units::operations::{BuildingOperations, MineMode, SpaceDockMode};

#[test]
fn extraction_modes_scale_each_resource_and_queued_levels_independently() {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    let planet = model.map.get_mut(home);
    planet.resources = Resources::new(101, 81, 61);
    planet.army.clear();
    for resource in [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium] {
        planet.army.insert(Unit::Building(mine_building(resource)), 2);
    }
    planet.operations.mine_mut(ResourceName::Metal).mode = MineMode::Intensive;
    planet.operations.mine_mut(ResourceName::Crystal).mode = MineMode::Suspended;
    assert_eq!(planet.resource_production(), Resources::new(303, 0, 122));
    assert_eq!(model.players[0].energy_grid(&model.map).demand, 8);
    model
        .map
        .get_mut(home)
        .buy
        .extend([Unit::Building(Building::MetalMine), Unit::Building(Building::CrystalMine)]);
    assert_eq!(EnergyGrid::for_player_next_turn(1, &model.map).demand, 11);
    let planet = model.map.get_mut(home);
    planet.army.insert(Unit::Building(Building::Terraformer), 1);
    planet.terraformer_focus = Some(ResourceName::Metal);
    // Terraformer's normal 222 Metal is boosted once to 333.
    assert_eq!(planet.resource_production(), Resources::new(333, 0, 109));
    assert_eq!(MineMode::Intensive.output(usize::MAX), usize::MAX);
}

#[test]
fn intensive_extraction_pays_now_and_forces_exactly_one_complete_recovery_turn() {
    for resource in [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium] {
        let mut model = started_model(2);
        let home = model.players[0].home_planet;
        model.map.get_mut(home).army.insert(Unit::Building(Building::Reactor), 5);
        let before = model.players[0].resources;
        let normal = model.map.get(home).resource_production();
        let command = TurnCommand::SetMineMode {
            planet_id: home,
            resource,
            mode: MineMode::Intensive,
        };
        resolve_turn(
            &mut model,
            &[TurnSubmission::new(1, 1, vec![command.clone()]), TurnSubmission::new(2, 1, vec![])],
        )
        .unwrap();
        assert_eq!(
            model.players[0].resources.get(&resource) - before.get(&resource),
            normal.get(&resource) + normal.get(&resource) / 2
        );
        assert_eq!(model.map.get(home).operations.mine(resource).mode, MineMode::Suspended);
        assert!(model.map.get(home).operations.mine(resource).recovering);
        assert!(preview_commands(&model, 1, std::slice::from_ref(&command)).is_err());
        assert!(apply_mine_mode(&mut model, 1, home, resource, MineMode::Normal).is_err());
        // A save/reload cannot discard the compulsory recovery.
        model =
            PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap().state;
        let before = model.players[0].resources.get(&resource);
        empty_turn(&mut model);
        assert_eq!(model.players[0].resources.get(&resource), before);
        assert!(!model.map.get(home).operations.mine(resource).recovering);
        assert_eq!(model.map.get(home).operations.mine(resource).mode, MineMode::Normal);
        assert!(preview_commands(&model, 1, &[command]).is_ok());
        empty_turn(&mut model);
        assert_eq!(model.players[0].resources.get(&resource) - before, normal.get(&resource));
        assert_eq!(model.map.get(home).operations.mine(resource).mode, MineMode::Normal);
        assert!(!model.map.get(home).operations.mine(resource).recovering);
    }
}

#[test]
fn manual_extraction_suspension_persists_across_turns_and_reload_until_changed() {
    for resource in [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium] {
        let mut model = started_model(2);
        let home = model.players[0].home_planet;
        model.map.get_mut(home).army.insert(Unit::Building(Building::Reactor), 5);
        let normal = model.map.get(home).resource_production().get(&resource);
        let before = model.players[0].resources.get(&resource);
        apply_mine_mode(&mut model, 1, home, resource, MineMode::Suspended).unwrap();
        model =
            PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap().state;
        for _ in 0..3 {
            empty_turn(&mut model);
            assert_eq!(model.players[0].resources.get(&resource), before);
            assert_eq!(model.map.get(home).operations.mine(resource).mode, MineMode::Suspended);
            assert!(!model.map.get(home).operations.mine(resource).recovering);
        }
        apply_mine_mode(&mut model, 1, home, resource, MineMode::Normal).unwrap();
        empty_turn(&mut model);
        assert_eq!(model.players[0].resources.get(&resource) - before, normal);
    }
}

#[test]
fn operating_commands_reject_foreign_missing_and_only_queued_structures() {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    assert!(apply_mine_mode(&mut model, 2, home, ResourceName::Metal, MineMode::Intensive).is_err());
    model.map.get_mut(home).army.remove(&Unit::Building(Building::MetalMine));
    model.map.get_mut(home).buy.push(Unit::Building(Building::MetalMine));
    assert!(apply_mine_mode(&mut model, 1, home, ResourceName::Metal, MineMode::Normal).is_err());
    assert!(apply_recycler_focus(&mut model, 1, home, Some(ResourceName::Metal)).is_err());
    assert!(apply_space_dock_mode(&mut model, 1, home, SpaceDockMode::Bastion).is_err());
    model.map.get_mut(home).army.insert(Unit::Building(Building::Recycler), 1);
    assert!(apply_recycler_focus(&mut model, 2, home, None).is_err());
    let invalid = model.map.get_mut(home).operations.mine_mut(ResourceName::Metal);
    invalid.mode = MineMode::Normal;
    invalid.recovering = true;
    assert!(model.validate().is_err());
}

#[test]
fn dock_final_selection_commits_only_at_turn_end_for_the_next_three_turns() {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    let planet = model.map.get_mut(home);
    planet.army.insert(Unit::space_dock(), 1);
    let industrial_production = planet.max_fleet_production();
    assert_eq!(planet.operations.space_dock, SpaceDockMode::Industrial);
    let select = |mode| TurnCommand::SetSpaceDockMode {
        planet_id: home,
        mode,
    };
    model = preview_commands(
        &model,
        1,
        &[
            select(SpaceDockMode::Bastion),
            select(SpaceDockMode::Industrial),
            select(SpaceDockMode::Bastion),
        ],
    )
    .unwrap();
    assert_eq!(model.map.get(home).max_fleet_production(), industrial_production - 5);
    assert_eq!(model.map.get(home).unit_hull(Unit::space_dock()), 3_000);
    assert_eq!(model.map.get(home).unit_shield(Unit::space_dock()), 165);
    assert_eq!(model.map.get(home).unit_damage(Unit::space_dock()), 225);
    assert_eq!(model.map.get(home).operations.space_dock_locked_until, 0);
    assert!(model.map.get(home).operations.space_dock_selection_pending);
    let expected_unlock = model.turn + 4;
    // Saving an unfinished selection preserves its editability and eventual commitment.
    model = PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap().state;
    apply_space_dock_mode(&mut model, 1, home, SpaceDockMode::Industrial).unwrap();
    apply_space_dock_mode(&mut model, 1, home, SpaceDockMode::Bastion).unwrap();
    empty_turn(&mut model);
    assert_eq!(model.map.get(home).operations.space_dock_locked_until, expected_unlock);
    assert!(!model.map.get(home).operations.space_dock_selection_pending);
    model = PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap().state;
    while model.turn < expected_unlock {
        assert!(apply_space_dock_mode(&mut model, 1, home, SpaceDockMode::Industrial).is_err());
        // Reselecting the committed mode is harmless and must not extend the lock.
        apply_space_dock_mode(&mut model, 1, home, SpaceDockMode::Bastion).unwrap();
        empty_turn(&mut model);
        assert_eq!(model.map.get(home).operations.space_dock_locked_until, expected_unlock);
    }
    apply_space_dock_mode(&mut model, 1, home, SpaceDockMode::Industrial).unwrap();
    assert_eq!(model.map.get(home).max_fleet_production(), industrial_production);
    apply_space_dock_mode(&mut model, 1, home, SpaceDockMode::Bastion).unwrap();
    apply_space_dock_mode(&mut model, 1, home, SpaceDockMode::Industrial).unwrap();
    empty_turn(&mut model);
    assert_eq!(model.map.get(home).operations.space_dock, SpaceDockMode::Industrial);
    assert_eq!(model.map.get(home).operations.space_dock_locked_until, model.turn + 3);
    assert!(apply_space_dock_mode(&mut model, 1, home, SpaceDockMode::Bastion).is_err());
}

#[test]
fn bastion_cannot_keep_industrial_production_already_spent_on_queued_ships() {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    model.players[0].resources = Resources::new(1_000_000, 1_000_000, 1_000_000);
    model.map.get_mut(home).army.insert(Unit::space_dock(), 1);
    let capacity = model.map.get(home).max_fleet_production();
    let build = TurnCommand::BuyUnits {
        planet_id: home,
        unit: Unit::Ship(Ship::LightFighter),
        count: capacity,
    };
    let bastion = TurnCommand::SetSpaceDockMode {
        planet_id: home,
        mode: SpaceDockMode::Bastion,
    };
    assert!(preview_commands(&model, 1, &[build.clone(), bastion.clone()]).is_err());
    assert!(preview_commands(&model, 1, &[bastion, build]).is_err());
    assert_eq!(model.map.get(home).operations.space_dock, SpaceDockMode::Industrial);
}

#[test]
fn selective_recovery_preserves_only_the_boosted_resource_and_saturates() {
    let bulk = Resources::new(51, 21, 9);
    let mut operations = BuildingOperations::default();
    assert_eq!(operations.recycler_output(bulk), bulk);
    for (resource, expected) in [
        (ResourceName::Metal, Resources::new(76, 0, 0)),
        (ResourceName::Crystal, Resources::new(0, 31, 0)),
        (ResourceName::Deuterium, Resources::new(0, 0, 13)),
    ] {
        operations.recycler_focus = Some(resource);
        assert_eq!(operations.recycler_output(bulk), expected);
    }
    operations.recycler_focus = Some(ResourceName::Metal);
    assert_eq!(
        operations.recycler_output(Resources::new(usize::MAX, 1, 1)),
        Resources::new(usize::MAX, 0, 0)
    );
}

#[test]
fn abandoning_a_colony_cannot_refund_jump_energy_or_skip_extraction_recovery() {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    let colony = model.map.planets.iter().find(|p| !p.is_moon() && p.owned.is_none()).unwrap().id;
    model.map.get_mut(colony).colonize(1);
    for id in [home, colony] {
        model.map.get_mut(id).army.insert(Unit::Building(Building::JumpGate), 1);
    }
    let fighter = Unit::Ship(Ship::LightFighter);
    model.map.get_mut(colony).army.insert(fighter, 1);
    model.map.get_mut(colony).operations.mine_mut(ResourceName::Metal).mode = MineMode::Intensive;
    model.map.get_mut(colony).operations.mine_mut(ResourceName::Metal).finish_turn();
    let projected = preview_commands(
        &model,
        1,
        &[
            TurnCommand::SendMission {
                mission_id: 51,
                origin: colony,
                destination: home,
                objective: Icon::Deploy,
                army: Army::from([(fighter, 1)]),
                bombing: BombingRaid::None,
                combat_probes: false,
                deep_cover: false,
                jump_gate: true,
            },
            TurnCommand::AbandonPlanet {
                planet_id: colony,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        crate::core::energy::jump_gate_energy_demand(
            &projected.missions,
            1,
            projected.turn as usize
        ),
        1
    );
    assert!(projected.map.get(colony).operations.mine(ResourceName::Metal).recovering);
}

#[test]
fn jumps_round_energy_per_mission_refund_cancellation_and_charge_only_launch_turn() {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    let colony = model.map.planets.iter().find(|p| !p.is_moon() && p.owned.is_none()).unwrap().id;
    model.map.get_mut(colony).colonize(1);
    for id in [home, colony] {
        model.map.get_mut(id).army.insert(Unit::Building(Building::JumpGate), 5);
    }
    let fighter = Unit::Ship(Ship::LightFighter);
    model.map.get_mut(home).army.insert(fighter, 20);
    let idle = model.players[0].energy_grid(&model.map);
    let send = |mission_id, count| TurnCommand::SendMission {
        mission_id,
        origin: home,
        destination: colony,
        objective: Icon::Deploy,
        army: Army::from([(fighter, count)]),
        bombing: BombingRaid::None,
        combat_probes: false,
        deep_cover: false,
        jump_gate: true,
    };
    for (count, expected) in [(1, 1), (5, 1), (6, 2), (10, 2), (11, 3)] {
        let preview = preview_commands(&model, 1, &[send(1, count)]).unwrap();
        assert_eq!(
            crate::core::energy::jump_gate_energy_demand(
                &preview.missions,
                1,
                preview.turn as usize
            ),
            expected
        );
        assert_eq!(
            preview.players[0].energy_grid(&preview.map).with_action_demand(expected).demand,
            idle.demand + expected
        );
    }
    let preview = preview_commands(&model, 1, &[send(1, 1), send(2, 1)]).unwrap();
    assert_eq!(
        crate::core::energy::jump_gate_energy_demand(&preview.missions, 1, preview.turn as usize),
        2
    );
    let cancelled = preview_commands(
        &preview,
        1,
        &[TurnCommand::RecallMission {
            mission_id: 1,
        }],
    )
    .unwrap();
    assert_eq!(
        crate::core::energy::jump_gate_energy_demand(
            &cancelled.missions,
            1,
            cancelled.turn as usize
        ),
        1
    );
    let uncharged = model.clone();
    let expected_raw = model.players[0].raw_resource_production(&model.map);
    let before = model.players[0].resources;
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, 1, vec![send(1, 6)]), TurnSubmission::new(2, 1, vec![])],
    )
    .unwrap();
    assert_eq!(
        model.players[0].resources - before,
        idle.with_action_demand(2).scale_resources(expected_raw)
    );
    assert_eq!(
        crate::core::energy::jump_gate_energy_demand(&model.missions, 1, model.turn as usize),
        0
    );
    assert_eq!(
        model.players[0].energy_grid(&model.map),
        uncharged.players[0].energy_grid(&uncharged.map)
    );
    assert_eq!(EnergyGrid::for_building(Building::JumpGate, None), EnergyGrid::default());
}
