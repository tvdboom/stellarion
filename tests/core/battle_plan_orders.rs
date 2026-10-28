use super::*;
use crate::core::combat::plan::{CombatTactic, ProtectionPosture, TargetPriority};

fn siege_plan() -> BattlePlan {
    BattlePlan {
        automatic_tactic: false,
        tactic: CombatTactic::Siege,
        target_priority: TargetPriority::Defenses,
        protection: ProtectionPosture::ProtectBombers,
    }
}

#[test]
fn merged_source_fleets_are_recorded_on_each_ship_and_survive_draw_returns() {
    use crate::core::combat::resolution::resolve_combat_with_rng;
    let mut origin =
        crate::core::map::planet::Planet::new(0, "Home".into(), Vec2::ZERO, false, 1.0);
    origin.controlled = Some(1);
    let mut target = crate::core::map::planet::Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
    target.controlled = Some(2);
    target.army.insert(Unit::Defense(crate::core::units::defense::Defense::RocketLauncher), 1);
    target.army.insert(Unit::planetary_shield(), 100);
    let mut first = Mission::new_with_id(
        41,
        1,
        1,
        &origin,
        &target,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Destroyer), 2)]),
        BombingRaid::None,
        true,
        false,
        None,
    );
    first.battle_plan = BattlePlan {
        automatic_tactic: false,
        tactic: CombatTactic::Swarm,
        ..Default::default()
    };
    let mut second = first.clone();
    second.id = 42;
    second.army.insert(Unit::Ship(Ship::Destroyer), 3);
    second.battle_plan = siege_plan();
    first.merge(&second);
    assert_eq!(first.battle_fleets.len(), 2);
    assert_eq!(first.battle_plan_for(Some(1)).tactic, CombatTactic::Siege);
    let report = resolve_combat_with_rng(1, &first, &target, &mut StdRng::seed_from_u64(19));
    assert!(report.is_stalemate());
    let round = report.combat_report.as_ref().unwrap().rounds.first().unwrap();
    for ship in &round.attacker {
        let expected = if ship.fleet_id == 41 {
            CombatTactic::Swarm
        } else {
            assert_eq!(ship.fleet_id, 42);
            CombatTactic::Siege
        };
        assert_eq!(ship.battle_plan.tactic, expected);
    }
    let returning = report.retained_battle_fleets(&report.surviving_attacker);
    assert_eq!(returning.len(), 2);
    assert_eq!(returning[0].army.amount(&Unit::Ship(Ship::Destroyer)), 2);
    assert_eq!(returning[1].army.amount(&Unit::Ship(Ship::Destroyer)), 3);
    assert_eq!(returning[0].battle_plan.tactic, CombatTactic::Swarm);
    assert_eq!(returning[1].battle_plan.tactic, CombatTactic::Siege);
}

#[test]
fn source_fleet_identity_and_composition_are_validated_and_explicit_orders_reform_them() {
    let mut model = started_model(2);
    let fighter = Unit::Ship(Ship::LightFighter);
    let source = crate::core::missions::BattleFleet {
        id: 901,
        army: Army::from([(fighter, 2)]),
        battle_plan: siege_plan(),
    };
    let mut mission = Mission::new_with_id(
        900,
        1,
        1,
        model.map.get(model.players[0].home_planet),
        model.map.get(model.players[1].home_planet),
        Icon::Attack,
        source.army.clone(),
        BombingRaid::None,
        false,
        false,
        None,
    )
    .with_battle_fleets(vec![source.clone()]);
    model.missions.push(mission.clone());
    model.validate().unwrap();
    model.missions[0].battle_fleets[0].army.insert(fighter, 3);
    assert!(model.validate().is_err());
    model.missions[0] = mission.clone();
    let mut duplicate = mission.clone();
    duplicate.id = source.id;
    duplicate.battle_fleets.clear();
    model.missions.push(duplicate);
    assert!(model.validate().is_err(), "active and source identities cannot collide");
    assert!(mission.set_battle_plan_for(1, BattlePlan::default()));
    assert!(mission.battle_fleets.is_empty());
    assert_eq!(mission.battle_plan_for(Some(1)).tactic, CombatTactic::Swarm);
}

#[test]
fn active_mission_plans_are_owned_persisted_commands() {
    let mut model = started_model(3);
    let original = Mission {
        id: 9901,
        owner: 1,
        origin: model.players[0].home_planet,
        destination: model.players[2].home_planet,
        objective: Icon::Attack,
        army: Army::from([(Unit::Ship(Ship::LightFighter), 4)]),
        ..Default::default()
    };
    model.missions.push(original.clone());
    let command = TurnCommand::SetMissionBattlePlan {
        mission_id: original.id,
        battle_plan: siege_plan(),
    };
    let serialized = serde_json::to_value(&command).unwrap();
    assert_eq!(serialized["kind"], "set_mission_battle_plan");
    let restored: TurnCommand = serde_json::from_value(serialized).unwrap();
    let projected = preview_commands(&model, 1, &[restored]).unwrap();
    assert_eq!(projected.missions[0].battle_plan, siege_plan());
    assert_eq!(projected.missions[0].army, original.army);
    assert_eq!(projected.missions[0].origin, original.origin);
    assert_eq!(model.missions[0].battle_plan, BattlePlan::default());
    assert!(preview_commands(&model, 2, &[command]).is_err());
    assert!(apply_mission_battle_plan(&mut model, 1, 9999, siege_plan()).is_err());
    model.players[0].spectator = true;
    assert!(apply_mission_battle_plan(&mut model, 1, original.id, siege_plan()).is_err());
}

#[test]
fn active_allied_mission_edits_only_change_the_acting_contingent() {
    let mut model = started_model(3);
    let army = Army::from([(Unit::Ship(Ship::LightFighter), 4)]);
    model.missions.push(Mission {
        id: 9902,
        owner: 1,
        objective: Icon::Attack,
        army: army.clone(),
        joint_attack: Some(JointAttackMission {
            leader: 1,
            attackers: BTreeMap::from([(1, army.clone()), (2, army)]),
            combat_orders: BTreeMap::from([(1, Default::default()), (2, Default::default())]),
            ..Default::default()
        }),
        ..Default::default()
    });
    apply_mission_battle_plan(&mut model, 2, 9902, siege_plan()).unwrap();
    assert_eq!(model.missions[0].battle_plan_for(Some(2)), siege_plan());
    assert_eq!(model.missions[0].battle_plan_for(Some(1)), BattlePlan::default());
    assert_eq!(model.missions[0].battle_plan, BattlePlan::default());
    assert!(apply_mission_battle_plan(&mut model, 3, 9902, siege_plan()).is_err());
    model.missions[0].objective = Icon::MissileStrike;
    assert!(apply_mission_battle_plan(&mut model, 1, 9902, siege_plan()).is_err());
}

#[test]
fn standing_orders_belong_to_the_commander_not_the_host() {
    let mut model = started_model(3);
    let home = model.players[0].home_planet;
    let host_plan = siege_plan();
    let guest_plan = BattlePlan {
        automatic_tactic: false,
        tactic: CombatTactic::Swarm,
        ..Default::default()
    };
    apply_battle_plan(&mut model, 1, home, host_plan).unwrap();
    assert!(apply_battle_plan(&mut model, 2, home, guest_plan).is_err());
    set_protection_permission_immediately(&mut model, 1, home, 2, true).unwrap();
    model
        .map
        .get_mut(home)
        .dock_protecting_fleet(2, Army::from([(Unit::Ship(Ship::LightFighter), 3)]));
    apply_battle_plan(&mut model, 2, home, guest_plan).unwrap();
    assert_eq!(model.map.get(home).battle_plan_for(Some(1)), host_plan);
    assert_eq!(model.map.get(home).battle_plan_for(Some(2)), guest_plan);
    assert!(apply_battle_plan(&mut model, 3, home, guest_plan).is_err());
    model.validate().unwrap();
    let restored: GameModel =
        serde_json::from_value(serde_json::to_value(&model).unwrap()).unwrap();
    assert_eq!(restored.map.get(home).battle_plan_for(Some(2)), guest_plan);
    model.map.get_mut(home).army.remove_protector(2);
    assert!(model.validate().is_err(), "orphan protector orders must be rejected");
    model.map.get_mut(home).prune_battle_plans();
    model.validate().unwrap();
}

#[test]
fn eliminated_hosts_return_foreign_survivors_with_their_own_orders() {
    let mut model = started_model(3);
    let target = model.players[0].home_planet;
    set_protection_permission_immediately(&mut model, 1, target, 2, true).unwrap();
    model
        .map
        .get_mut(target)
        .dock_protecting_fleet(2, Army::from([(Unit::Ship(Ship::LightFighter), 2)]));
    apply_battle_plan(&mut model, 2, target, siege_plan()).unwrap();
    model.map.get_mut(target).clean();
    let mut rng = StdRng::seed_from_u64(99);
    let mut returning = Vec::new();
    recall_unpermitted_protecting_fleets(
        &mut model,
        2,
        &mut rng,
        &mut HashSet::new(),
        &mut returning,
    )
    .unwrap();
    assert_eq!(returning.len(), 1);
    assert_eq!(returning[0].owner, 2);
    assert_eq!(returning[0].battle_plan, siege_plan());
    assert!(model.map.get(target).protector_battle_plans.is_empty());
}

#[test]
fn dispatch_freezes_selected_orders_and_draft_copy_preserves_them() {
    let mut model = started_model(2);
    let origin = model.players[0].home_planet;
    let destination = model.players[1].home_planet;
    let fighter = Unit::Ship(Ship::LightFighter);
    model.map.get_mut(origin).army.insert(fighter, 3);
    model.players[0].resources = Resources::new(100_000, 100_000, 100_000);
    apply_battle_plan(&mut model, 1, origin, siege_plan()).unwrap();
    let chosen = BattlePlan {
        automatic_tactic: false,
        tactic: CombatTactic::Swarm,
        ..Default::default()
    };
    apply_mission(
        &mut model,
        1,
        7_901,
        origin,
        destination,
        Icon::Attack,
        &Army::from([(fighter, 2)]),
        BombingRaid::None,
        false,
        chosen,
        false,
        false,
    )
    .unwrap();
    apply_battle_plan(&mut model, 1, origin, BattlePlan::default()).unwrap();
    assert_eq!(model.missions[0].battle_plan, chosen);
    let copied = Mission::from_mission(
        1,
        1,
        model.map.get(origin),
        model.map.get(destination),
        &model.missions[0],
    );
    assert_eq!(copied.battle_plan, chosen);
    model.validate().unwrap();
}

#[test]
fn canceling_a_protector_launch_restores_standing_orders_not_mission_overrides() {
    let mut model = started_model(3);
    let origin = model.players[0].home_planet;
    let destination = model.players[2].home_planet;
    let fighter = Unit::Ship(Ship::LightFighter);
    set_protection_permission_immediately(&mut model, 1, origin, 2, true).unwrap();
    model.map.get_mut(origin).dock_protecting_fleet(2, Army::from([(fighter, 2)]));
    apply_battle_plan(&mut model, 2, origin, siege_plan()).unwrap();
    model.players[1].resources = Resources::new(100_000, 100_000, 100_000);
    let before = serde_json::to_value(&model).unwrap();
    let projected = preview_commands(
        &model,
        2,
        &[
            TurnCommand::SendMission {
                mission_id: 7_902,
                origin,
                destination,
                objective: Icon::Attack,
                army: Army::from([(fighter, 2)]),
                bombing: BombingRaid::None,
                combat_probes: false,
                battle_plan: BattlePlan::default(),
                deep_cover: false,
                jump_gate: false,
            },
            TurnCommand::RecallMission {
                mission_id: 7_902,
            },
        ],
    )
    .unwrap();
    assert_eq!(serde_json::to_value(projected).unwrap(), before);
}

#[test]
fn merged_arrivals_preserve_independent_plans_and_joint_contingents_stay_separate() {
    let mut first = Mission {
        id: 1,
        owner: 1,
        objective: Icon::Attack,
        army: Army::from([(Unit::Ship(Ship::LightFighter), 3)]),
        ..Default::default()
    };
    let mut second = first.clone();
    second.id = 2;
    second.battle_plan = siege_plan();
    let groups = regroup_missions(&[first.clone(), second.clone()]);
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].battle_fleets.len(), 2);
    assert_eq!(groups[0].battle_fleets[1].battle_plan, siege_plan());
    second.battle_plan = first.battle_plan;
    assert_eq!(regroup_missions(&[first.clone(), second.clone()]).len(), 1);
    first.joint_attack = Some(JointAttackMission::default());
    assert_eq!(regroup_missions(&[first, second]).len(), 2);
}

#[test]
fn captured_world_adopts_attacking_plan_and_followup_force_docks_safely() {
    let mut model = started_model(2);
    let origin = model.players[0].home_planet;
    let target = model.map.planets.iter().find(|p| !p.is_moon() && p.owned.is_none()).unwrap().id;
    let first_plan = siege_plan();
    let second_plan = BattlePlan {
        automatic_tactic: false,
        tactic: CombatTactic::Swarm,
        ..Default::default()
    };
    model.map.get_mut(target).army.clear();
    let destination = model.map.get(target).clone();
    let origin_world = model.map.get(origin).clone();
    for (id, plan) in [(8_001, first_plan), (8_002, second_plan)] {
        let mut mission = Mission::new_with_id(
            id,
            1,
            1,
            &origin_world,
            &destination,
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
            BombingRaid::None,
            false,
            false,
            None,
        )
        .with_battle_plan(plan);
        mission.position = destination.position;
        model.missions.push(mission);
    }
    let turn = model.turn;
    resolve_turn(
        &mut model,
        &[TurnSubmission::new(1, turn, vec![]), TurnSubmission::new(2, turn, vec![])],
    )
    .unwrap();
    assert_eq!(model.map.get(target).controlled, Some(1));
    assert_eq!(model.map.get(target).army.amount(&Unit::Ship(Ship::LightFighter)), 2);
    assert_eq!(model.map.get(target).battle_plan, first_plan);
    assert!(model.players[0].reports.iter().all(|report| report.combat_report.is_none()));
    model.validate().unwrap();
}

#[test]
fn friendly_reinforcements_preserve_host_and_protector_orders() {
    let mut model = started_model(3);
    let target = model.players[0].home_planet;
    let origin = model.map.planets.iter().find(|p| !p.is_moon() && p.owned.is_none()).unwrap().id;
    model.map.get_mut(origin).colonize(1);
    set_protection_permission_immediately(&mut model, 1, target, 2, true).unwrap();
    let fighter = Unit::Ship(Ship::LightFighter);
    model.map.get_mut(target).dock_protecting_fleet(2, Army::from([(fighter, 2)]));
    let guest_plan = BattlePlan {
        automatic_tactic: false,
        tactic: CombatTactic::Swarm,
        ..Default::default()
    };
    apply_battle_plan(&mut model, 1, target, siege_plan()).unwrap();
    apply_battle_plan(&mut model, 2, target, guest_plan).unwrap();
    let destination = model.map.get(target).clone();
    let mut mission = Mission::new_with_id(
        8_003,
        1,
        1,
        model.map.get(origin),
        &destination,
        Icon::Deploy,
        Army::from([(fighter, 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    )
    .with_battle_plan(BattlePlan::default());
    mission.position = destination.position;
    model.missions.push(mission);
    let turn = model.turn;
    let submissions =
        (1..=3).map(|owner| TurnSubmission::new(owner, turn, vec![])).collect::<Vec<_>>();
    resolve_turn(&mut model, &submissions).unwrap();
    assert_eq!(model.map.get(target).battle_plan, siege_plan());
    assert_eq!(model.map.get(target).battle_plan_for(Some(2)), guest_plan);
    assert_eq!(model.map.get(target).army.protector(2).unwrap().amount(&fighter), 2);
    model.validate().unwrap();
}

#[test]
fn allied_missions_preserve_each_accepted_plan() {
    let mut model = started_model(3);
    let target = model.players[2].home_planet;
    let mut contributions = Vec::new();
    for (owner, plan) in [
        (1, siege_plan()),
        (
            2,
            BattlePlan {
                automatic_tactic: false,
                tactic: CombatTactic::Interception,
                ..Default::default()
            },
        ),
    ] {
        let origin = model.player(owner).unwrap().home_planet;
        model.map.get_mut(origin).army.insert(Unit::Ship(Ship::LightFighter), 4);
        model.player_mut(owner).unwrap().resources = Resources::new(100_000, 100_000, 100_000);
        contributions.push(JointAttackContribution {
            player_id: owner,
            origin,
            army: Army::from([(Unit::Ship(Ship::LightFighter), 2)]),
            bombing: BombingRaid::None,
            combat_probes: false,
            battle_plan: plan,
        });
    }
    apply_joint_mission(
        &mut model,
        1,
        1_901,
        1_902,
        target,
        Icon::Attack,
        BombingRaid::None,
        false,
        &contributions,
    )
    .unwrap();
    for mission in &model.missions {
        let own = contributions.iter().find(|c| c.player_id == mission.owner).unwrap();
        assert_eq!(mission.battle_plan, own.battle_plan);
        for contribution in &contributions {
            assert_eq!(
                mission.battle_plan_for(Some(contribution.player_id)),
                contribution.battle_plan
            );
        }
    }
    model.validate().unwrap();
}

#[test]
fn allied_arrival_after_friendly_capture_returns_all_contingents_with_orders() {
    let mut model = started_model(3);
    let target = model.map.planets.iter().find(|p| !p.is_moon() && p.owned.is_none()).unwrap().id;
    model.map.get_mut(target).army.clear();
    let fighter = Unit::Ship(Ship::LightFighter);
    let guest_plan = BattlePlan {
        automatic_tactic: false,
        tactic: CombatTactic::Swarm,
        ..Default::default()
    };
    let mut contributions = Vec::new();
    for (owner, battle_plan) in [(1, siege_plan()), (2, guest_plan)] {
        let origin = model.player(owner).unwrap().home_planet;
        model.map.get_mut(origin).army.insert(fighter, 4);
        model.player_mut(owner).unwrap().resources = Resources::new(100_000, 100_000, 100_000);
        contributions.push(JointAttackContribution {
            player_id: owner,
            origin,
            army: Army::from([(fighter, 2)]),
            bombing: BombingRaid::None,
            combat_probes: false,
            battle_plan,
        });
    }
    apply_joint_mission(
        &mut model,
        1,
        9_101,
        9_102,
        target,
        Icon::Attack,
        BombingRaid::None,
        false,
        &contributions,
    )
    .unwrap();
    let destination = model.map.get(target).clone();
    for mission in &mut model.missions {
        mission.position = destination.position;
        mission.joint_attack.as_mut().unwrap().arrival_turn = 2;
    }
    let mut first = Mission::new_with_id(
        9_100,
        1,
        1,
        model.map.get(contributions[0].origin),
        &destination,
        Icon::Attack,
        Army::from([(fighter, 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    first.position = destination.position;
    model.missions.push(first);
    let turn = model.turn;
    let submissions =
        (1..=3).map(|owner| TurnSubmission::new(owner, turn, vec![])).collect::<Vec<_>>();
    resolve_turn(&mut model, &submissions).unwrap();
    assert_eq!(model.map.get(target).controlled, Some(1));
    assert_eq!(model.map.get(target).army.amount(&fighter), 1);
    for contribution in contributions {
        let returned =
            model.missions.iter().find(|mission| mission.owner == contribution.player_id).unwrap();
        assert_eq!(returned.army, contribution.army);
        assert_eq!(returned.battle_plan, contribution.battle_plan);
        assert!(returned.joint_attack.is_none());
        assert!(returned.is_returning());
    }
    model.validate().unwrap();
}
