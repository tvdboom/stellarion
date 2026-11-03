use super::*;
use crate::core::combat::report::Side;
use crate::core::map::planet::PlanetId;
use crate::core::resources::Resources;
use crate::core::simulation::{
    preview_commands, resolve_turn, GameRules, PersistedGame, TurnCommand, TurnSubmission,
};
use crate::core::units::{buildings::Building, ships::Ship};
use rand::SeedableRng;

fn game(defender: PlayerId) -> (GameModel, PlanetId) {
    game_with_players(defender, 2)
}

fn game_with_players(defender: PlayerId, player_count: u8) -> (GameModel, PlanetId) {
    let mut model = GameModel::new(
        [73; 32],
        GameRules {
            player_count,
            space_fauna_percent: 0,
            independent_populations: false,
            ..Default::default()
        },
    )
    .unwrap();
    model.start().unwrap();
    model.map.rect =
        bevy::prelude::Rect::from_corners(Vec2::splat(-10_000.0), Vec2::splat(10_000.0));
    for player in &mut model.players {
        player.resources = Resources::new(100_000, 100_000, 100_000);
    }
    let home = model.players.iter().find(|player| player.id == defender).unwrap().home_planet;
    let enemy = model.players.iter().find(|player| player.id != defender).unwrap();
    let enemy_id = enemy.id;
    let enemy_home = enemy.home_planet;
    model.map.get_mut(home).position = Vec2::ZERO;
    model.map.get_mut(enemy_home).position = Vec2::X * Planet::SIZE * 30.0;
    model.map.get_mut(home).army.insert(Unit::Ship(Ship::LightFighter), 500);
    let moon = model.map.moons()[0].id;
    model.map.get_mut(moon).controlled = Some(defender);
    model.map.get_mut(moon).position = Vec2::ZERO;
    model.map.get_mut(moon).army.insert(Unit::Building(Building::OrbitalRadar), 30);
    let target = Mission::new_with_id(
        900,
        model.turn as usize,
        enemy_id,
        model.map.get(enemy_home),
        model.map.get(home),
        Icon::Spy,
        Army::from([(Unit::probe(), 5)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    model.missions.push(target);
    model.validate().unwrap();
    (model, home)
}

fn command(home: PlanetId, fighters: usize) -> TurnCommand {
    TurnCommand::SendIntercept {
        mission_id: 700,
        origin: home,
        target: 900,
        army: Army::from([(Unit::Ship(Ship::LightFighter), fighters)]),
        battle_plan: Default::default(),
    }
}

fn next(model: &mut GameModel, orders: Vec<(PlayerId, Vec<TurnCommand>)>) {
    let submissions = model
        .players
        .iter()
        .filter(|player| !player.spectator)
        .map(|player| {
            TurnSubmission::new(
                player.id,
                model.turn,
                orders
                    .iter()
                    .find(|(id, _)| *id == player.id)
                    .map_or_else(Vec::new, |(_, commands)| commands.clone()),
            )
        })
        .collect::<Vec<_>>();
    resolve_turn(model, &submissions).unwrap();
}

#[test]
fn spies_are_destroyed_before_arrival_without_planet_intel_and_the_interceptor_returns() {
    let (mut model, home) = game(1);
    assert!(target_available(&model.missions[0], &model.map, &model.players[0]));
    let projected = preview_commands(&model, 1, &[command(home, 100)]).unwrap();
    let interceptor = projected.missions.iter().find(|mission| mission.id == 700).unwrap();
    let scheduled = interceptor.intercept.as_ref().unwrap().encounter_turn;
    assert!(scheduled < model.turn as usize + model.missions[0].duration(&model.map));
    let mut replay =
        PersistedGame::from_json(PersistedGame::new(projected).to_json().unwrap()).unwrap().state;
    next(&mut model, vec![(1, vec![command(home, 100)])]);
    next(&mut replay, vec![]);
    while model.turn < scheduled as u64 {
        next(&mut model, vec![]);
        next(&mut replay, vec![]);
    }
    assert_eq!(serde_json::to_value(&model).unwrap(), serde_json::to_value(&replay).unwrap());
    assert!(!model.missions.iter().any(|mission| mission.id == 900));
    let report = model.players[1]
        .reports
        .iter()
        .find(|report| report.mission.objective == Icon::Intercept)
        .unwrap();
    assert!(report.is_space_encounter());
    assert_eq!(report.scout_probes, 0);
    assert!(report.mission.conceals_origin());
    assert!(!report.surviving_defender.has_army());
    assert!(!report.can_see(&Side::Attacker, 2));
    assert!(report.can_see(&Side::Defender, 2));
    assert!(report.can_see(&Side::Attacker, 1));
    assert!(report.can_see(&Side::Defender, 1));
    assert!(model.players[1].last_info(model.map.get(home), &model.missions).is_none());
    let returning = model.missions.iter().find(|mission| mission.id == 700).unwrap();
    assert_eq!(returning.intercept.as_ref().unwrap().phase, InterceptPhase::Returning);
    assert!(returning.position.distance(model.map.get(home).position) > Planet::SIZE);
    for _ in 0..20 {
        if !model.missions.iter().any(|mission| mission.id == 700) {
            break;
        }
        next(&mut model, vec![]);
    }
    assert!(!model.missions.iter().any(|mission| mission.id == 700));
    model.validate().unwrap();
}

#[test]
fn interceptions_without_survivors_reveal_no_attackers_even_after_mutual_destruction() {
    let mut site =
        crate::core::map::planet::Planet::new(0, "Deep space".into(), Vec2::ZERO, false, 1.0);
    site.controlled = Some(2);
    let mut report = crate::test_support::empty_report(
        Mission {
            owner: 1,
            ..Default::default()
        },
        site,
    );
    report.surviving_defender.insert(Unit::probe(), 0);
    report.combat_report = Some(Default::default());
    for objective in [Icon::Patrol, Icon::Intercept] {
        report.mission.objective = objective;
        assert!(!report.can_see(&Side::Attacker, 2));
        assert!(report.can_see(&Side::Defender, 2));
        assert!(report.can_see(&Side::Attacker, 1));
    }
    report.mission.objective = Icon::Attack;
    assert!(report.can_see(&Side::Attacker, 2));
}

#[test]
fn targeting_requires_visibility_and_permits_final_approach_but_not_landed_or_special_travel() {
    let (mut model, home) = game(1);
    let target = model.missions[0].clone();
    for special in 0..3 {
        model.missions[0] = target.clone();
        match special {
            0 => model.missions[0].deep_cover = true,
            1 => model.missions[0].jump_gate = true,
            _ => model.missions[0].objective = Icon::MissileStrike,
        }
        assert!(!target_available(&model.missions[0], &model.map, &model.players[0]));
    }
    model.missions[0] = target;
    model.missions[0].position = Vec2::X * Planet::SIZE;
    assert!(preview_commands(&model, 1, &[command(home, 5)]).is_ok());
    model.missions[0].position = Vec2::X * Planet::SIZE * 0.5;
    assert!(preview_commands(&model, 1, &[command(home, 5)]).is_err());
    model.missions[0].position = Vec2::X * Planet::SIZE * 30.0;
    for planet in &mut model.map.planets {
        planet.army.remove(&Unit::Building(Building::OrbitalRadar));
    }
    assert!(preview_commands(&model, 1, &[command(home, 5)]).is_err());
    assert!(preview_commands(&model, 2, &[command(home, 5)]).is_err());
}

#[test]
fn recall_makes_the_ambush_miss_regardless_of_player_slot_order() {
    for defender in [1, 2] {
        let (mut model, home) = game(defender);
        let enemy = if defender == 1 {
            2
        } else {
            1
        };
        next(
            &mut model,
            vec![
                (defender, vec![command(home, 100)]),
                (
                    enemy,
                    vec![TurnCommand::RecallMission {
                        mission_id: 900,
                    }],
                ),
            ],
        );
        for _ in 0..8 {
            next(&mut model, vec![]);
        }
        assert!(model.players.iter().all(|player| player.reports.iter().all(|report| report
            .mission
            .objective
            != Icon::Intercept
            || report.is_missed_interception())));
        assert!(!model.missions.iter().any(|mission| mission.id == 700));
    }
}

#[test]
fn cancelled_launch_refunds_ships_and_fuel_and_invalid_persisted_state_is_rejected() {
    let (model, home) = game(1);
    let preview = preview_commands(
        &model,
        1,
        &[
            command(home, 100),
            TurnCommand::RecallMission {
                mission_id: 700,
            },
        ],
    )
    .unwrap();
    assert_eq!(preview.players[0].resources, model.players[0].resources);
    assert_eq!(preview.map.get(home).army, model.map.get(home).army);
    let valid = preview_commands(&model, 1, &[command(home, 100)]).unwrap();
    for fraction in [0.0, -0.1, 1.01, f32::NAN, f32::INFINITY] {
        let mut malformed = valid.clone();
        malformed
            .missions
            .iter_mut()
            .find(|mission| mission.id == 700)
            .unwrap()
            .intercept
            .as_mut()
            .unwrap()
            .encounter_fraction = fraction;
        assert!(malformed.validate().is_err());
    }
    let mut preview = valid;
    preview
        .missions
        .iter_mut()
        .find(|mission| mission.id == 700)
        .unwrap()
        .intercept
        .as_mut()
        .unwrap()
        .point
        .x = f32::NAN;
    assert!(preview.validate().is_err());
}

#[test]
fn surviving_targets_keep_their_full_movement_instead_of_being_pinned() {
    let (mut model, home) = game(1);
    model.missions[0].objective = Icon::Attack;
    model.missions[0].army = Army::from([(Unit::Ship(Ship::LightFighter), 1000)]);
    model.map.get_mut(home).army.insert(Unit::Building(Building::SensorPhalanx), 30);
    let projected = preview_commands(&model, 1, &[command(home, 1)]).unwrap();
    let due = projected
        .missions
        .iter()
        .find(|mission| mission.id == 700)
        .unwrap()
        .intercept
        .as_ref()
        .unwrap()
        .encounter_turn;
    next(&mut model, vec![(1, vec![command(home, 1)])]);
    while (model.turn as usize) < due - 1 {
        next(&mut model, vec![]);
    }
    let mut expected = model.missions.iter().find(|mission| mission.id == 900).unwrap().clone();
    expected.advance(&model.map);
    next(&mut model, vec![]);
    let actual = model.missions.iter().find(|mission| mission.id == 900).unwrap();
    assert_eq!(actual.position, expected.position);
    assert_eq!(actual.travel_turns, expected.travel_turns);
}

#[test]
fn patrol_and_targeted_interceptions_share_one_battle_per_turn() {
    let (mut model, home) = game(1);
    let mut interceptor = Mission::plan_intercept(
        700,
        model.turn as usize,
        1,
        model.map.get(home),
        &model.missions[0],
        Army::from([(Unit::Ship(Ship::LightFighter), 100)]),
        &model.map,
    )
    .unwrap();
    let encounter = interceptor.intercept.as_ref().unwrap().clone();
    interceptor.position = encounter.point;
    model.missions[0].position = encounter.point;
    model.missions.push(interceptor);
    let mut rng = rand_chacha::ChaCha8Rng::from_seed([1; 32]);
    resolve_interceptions(
        &mut model,
        encounter.encounter_turn,
        &mut rng,
        &mut HashSet::from([900]),
        &mut HashSet::from([900]),
    );
    assert!(model.players[0].reports.iter().all(|report| report.is_missed_interception()));
    assert!(model.missions.iter().any(|mission| mission.id == 900));
}

/// An already accelerating fleet can cover much more ground than a fresh launch next turn.
fn two_turn_approach() -> (GameModel, PlanetId) {
    let (mut model, home) = game(1);
    let enemy_home = model.missions[0].origin;
    model.map.get_mut(enemy_home).position = Vec2::X * Planet::SIZE * 40.3;
    model.missions[0].position = Vec2::X * Planet::SIZE * 39.6;
    for _ in 0..4 {
        model.missions[0].advance(&model.map);
    }
    model.turn += 4;
    assert_eq!(model.missions[0].duration(&model.map), 2);
    model.validate().unwrap();
    (model, home)
}

#[test]
fn two_turn_approach_can_be_intercepted_between_endpoints_from_a_planet_or_moon() {
    for launch_from_moon in [false, true] {
        let (mut model, home) = two_turn_approach();
        let origin = model
            .map
            .planets
            .iter()
            .find(|planet| {
                planet.id != home
                    && planet.id != model.missions[0].origin
                    && planet.is_moon() == launch_from_moon
            })
            .unwrap()
            .id;
        let point = model.missions[0].next_turn_position(&model.map);
        let current = model.missions[0].position;
        let port = model.map.get_mut(origin);
        port.owned = (!launch_from_moon).then_some(1);
        port.controlled = Some(1);
        port.position = current + Vec2::Y * Planet::SIZE;
        port.army.insert(Unit::Ship(Ship::LightFighter), 100);
        model.validate().unwrap();

        // Being 1 AU from the current contact is insufficient: it moves 9.9 AU next turn.
        assert!((current.distance(point) / Planet::SIZE - 9.9).abs() < 0.001);
        assert!(preview_commands(&model, 1, &[command(origin, 100)]).is_err());
        // The destination can now meet it during the following turn's final approach.
        assert!(preview_commands(&model, 1, &[command(home, 100)]).is_ok());
        // A world beside the middle of the segment can meet it this turn, even though
        // the old endpoint-only planner rejected this geometry.
        model.map.get_mut(origin).position = current.lerp(point, 0.5) + Vec2::Y * Planet::SIZE;
        let preview = preview_commands(&model, 1, &[command(origin, 100)]).unwrap();
        let interception = preview
            .missions
            .iter()
            .find(|mission| mission.id == 700)
            .unwrap()
            .intercept
            .as_ref()
            .unwrap();
        assert_eq!(interception.encounter_turn, model.turn as usize + 1);
        assert!(interception.encounter_fraction > 0.0 && interception.encounter_fraction < 1.0);
        assert!(interception.point.distance(point) > Planet::SIZE);
        assert!(
            interception.point.distance(current.lerp(point, interception.encounter_fraction))
                < CONTACT_TOLERANCE
        );

        next(&mut model, vec![(1, vec![command(origin, 100)])]);
        assert!(!model.missions.iter().any(|mission| mission.id == 900));
        assert!(model.players[0].reports.iter().any(|report| {
            report.mission.objective == Icon::Intercept && !report.is_missed_interception()
        }));
        model.validate().unwrap();
    }
}

#[test]
fn a_slower_selected_ship_can_close_a_final_approach_interception_window() {
    let (mut model, home) = two_turn_approach();
    model.missions[0].position = Vec2::X * Planet::SIZE * 3.0;
    assert_eq!(model.missions[0].duration(&model.map), 1);
    let mut origin = model.map.get(home).clone();
    origin.position = model.missions[0].position + Vec2::Y * Planet::SIZE;
    let fighters = Army::from([(Unit::Ship(Ship::LightFighter), 100)]);
    let plan = |army| {
        Mission::plan_intercept(
            700,
            model.turn as usize,
            1,
            &origin,
            &model.missions[0],
            army,
            &model.map,
        )
    };
    assert!(plan(fighters.clone()).is_some());
    let mut mixed = fighters;
    mixed.insert(Unit::Ship(Ship::WarSun), 1);
    assert!(plan(mixed).is_none());
}

#[test]
fn final_approach_survivors_still_attack_deploy_or_protect_on_the_scheduled_turn() {
    for objective in [Icon::Attack, Icon::Deploy, Icon::Protect] {
        let (mut model, home) = game_with_players(
            1,
            if objective == Icon::Protect {
                3
            } else {
                2
            },
        );
        let destination = if objective == Icon::Deploy {
            let colony = model
                .map
                .planets
                .iter()
                .find(|planet| {
                    !planet.is_moon() && planet.id != home && planet.id != model.missions[0].origin
                })
                .unwrap()
                .id;
            let colony = model.map.get_mut(colony);
            colony.owned = Some(2);
            colony.controlled = Some(2);
            colony.position = Vec2::ZERO;
            colony.id
        } else {
            home
        };
        if objective == Icon::Protect {
            model.map.get_mut(home).protection_permissions.insert(2);
        }
        let target = &mut model.missions[0];
        target.objective = objective;
        target.destination = destination;
        target.protected_player = (objective == Icon::Protect).then_some(1);
        target.army = Army::from([(Unit::Ship(Ship::LightFighter), 1000)]);
        target.position = Vec2::X * Planet::SIZE * 3.0;
        assert_eq!(target.duration(&model.map), 1);
        model.validate().unwrap();
        let draft = preview_commands(&model, 1, &[command(home, 1)]).unwrap();
        let contact = draft.missions.iter().find(|mission| mission.id == 700).unwrap();
        let contact = contact.intercept.as_ref().unwrap();
        assert_eq!(contact.encounter_turn, model.turn as usize + 1);
        assert!(contact.point.distance(model.map.get(destination).position) > Planet::SIZE * 0.7);

        next(&mut model, vec![(1, vec![command(home, 1)])]);
        assert!(!model.missions.iter().any(|mission| mission.id == 900), "{objective:?}");
        assert!(
            model.players[1].reports.iter().any(|report| {
                report.turn == model.turn as usize
                    && report.mission.objective == Icon::Intercept
                    && report.surviving_defender.has_army()
                    && !report.is_missed_interception()
            }),
            "{objective:?} never fought in space"
        );
        assert!(
            model.players[1].reports.iter().any(|report| {
                report.turn == model.turn as usize
                    && report.mission.objective == objective
                    && report.mission.destination == destination
                    && !report.is_space_encounter()
            }),
            "{objective:?} failed to arrive on schedule"
        );
        if objective == Icon::Protect {
            assert!(model.map.get(home).army.protector(2).is_some_and(Amount::has_army));
        }
        model.validate().unwrap();
    }
}

#[test]
fn final_approach_recalls_miss_and_changed_timing_never_retargets_the_ambush() {
    for recall in [false, true] {
        let (mut model, home) = game(1);
        model.missions[0].position = Vec2::X * Planet::SIZE * 3.0;
        let mut draft = preview_commands(&model, 1, &[command(home, 100)]).unwrap();
        let scheduled = draft.missions.iter().find(|mission| mission.id == 700).unwrap().clone();
        if recall {
            draft
                .missions
                .iter_mut()
                .find(|mission| mission.id == 900)
                .unwrap()
                .recall(&draft.map, model.turn as usize);
        } else {
            // Same leg and direction, but the fleet is delayed relative to the committed time.
            draft.missions.iter_mut().find(|mission| mission.id == 900).unwrap().position.x +=
                Planet::SIZE;
        }
        next(&mut draft, vec![]);
        let report = draft.players[0]
            .reports
            .iter()
            .find(|report| report.mission.id == 700 && report.mission.objective == Icon::Intercept)
            .unwrap();
        assert!(report.is_missed_interception());
        assert_eq!(
            report.mission.intercept.as_ref().unwrap().point,
            scheduled.intercept.as_ref().unwrap().point
        );
        draft.validate().unwrap();
    }
}

#[test]
fn intercepted_allied_contingents_arrive_together_on_the_original_turn() {
    use crate::core::missions::{FleetCombatOrders, JointAttackMission};
    use std::collections::BTreeMap;

    for remaining in [1, 2] {
        let (mut model, home) = game_with_players(1, 3);
        let home_two = model.players[1].home_planet;
        let home_three = model.players[2].home_planet;
        let expected_turn = model.turn as usize + remaining;
        let army = Army::from([(Unit::Ship(Ship::LightFighter), 1000)]);
        let escort = Army::from([(Unit::Ship(Ship::LightFighter), 1)]);
        let attack = JointAttackMission {
            id: 50,
            leader: 2,
            arrival_turn: expected_turn,
            attackers: BTreeMap::from([(2, army.clone()), (3, escort.clone())]),
            origins: BTreeMap::from([(2, home_two), (3, home_three)]),
            combat_orders: BTreeMap::from([
                (2, FleetCombatOrders::default()),
                (3, FleetCombatOrders::default()),
            ]),
            ..Default::default()
        };
        let target = &mut model.missions[0];
        target.objective = Icon::Attack;
        target.army = army;
        target.position = Vec2::X * Planet::SIZE * 3.0;
        target.joint_attack = Some(attack.clone());
        let mut contributor = Mission::new_with_id(
            901,
            model.turn as usize,
            3,
            model.map.get(home_three),
            model.map.get(home),
            Icon::Attack,
            escort,
            BombingRaid::None,
            true,
            false,
            None,
        );
        contributor.position = Vec2::X * Planet::SIZE * 3.0;
        contributor.joint_attack = Some(attack);
        model.missions.push(contributor);
        model.validate().unwrap();

        next(&mut model, vec![(1, vec![command(home, 1)])]);
        assert!(model.players[1].reports.iter().any(|report| {
            report.mission.objective == Icon::Intercept && !report.is_missed_interception()
        }));
        if remaining == 2 {
            assert!(model.players[1]
                .reports
                .iter()
                .all(|report| report.mission.joint_attack.is_none()));
            next(&mut model, vec![]);
        }
        assert!(model.players[1].reports.iter().any(|report| {
            report.turn == expected_turn
                && report.mission.joint_attack.as_ref().is_some_and(|attack| attack.id == 50)
        }));
        model.validate().unwrap();
    }
}

#[test]
fn earlier_contacts_spend_the_battle_budget_before_later_contacts_regardless_of_id() {
    let (mut model, home) = game(1);
    model.missions[0].position = Vec2::X * Planet::SIZE * 3.0;
    model.missions[0].objective = Icon::Attack;
    model.missions[0].army = Army::from([(Unit::Ship(Ship::LightFighter), 1000)]);
    let mut early = Mission::plan_intercept(
        800,
        model.turn as usize,
        1,
        model.map.get(home),
        &model.missions[0],
        Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
        &model.map,
    )
    .unwrap();
    let mut late = early.clone();
    late.id = 700;
    let late_contact = late.intercept.as_mut().unwrap();
    late_contact.encounter_fraction += 0.1;
    late_contact.point = model.missions[0]
        .position
        .lerp(model.missions[0].next_turn_position(&model.map), late_contact.encounter_fraction);
    late.position = late_contact.point;
    early.position = early.intercept.as_ref().unwrap().point;
    model.missions.extend([late, early]);
    resolve_interceptions(
        &mut model,
        2,
        &mut rand_chacha::ChaCha8Rng::from_seed([1; 32]),
        &mut HashSet::new(),
        &mut HashSet::new(),
    );
    let reports = &model.players[0].reports;
    assert!(reports
        .iter()
        .any(|report| report.mission.id == 800 && !report.is_missed_interception()));
    assert!(reports
        .iter()
        .any(|report| report.mission.id == 700 && report.is_missed_interception()));
}

#[test]
fn contact_geometry_handles_crossings_equal_speeds_tangency_and_deadlines() {
    let au = Planet::SIZE;
    // Both endpoints are out of range, but the target crosses the reachable area between them.
    let fraction = reachable_fraction(
        Vec2::new(-4.0, 1.0) * au,
        Vec2::new(4.0, 1.0) * au,
        Vec2::ZERO,
        2.4,
        0,
        1.0,
    )
    .unwrap();
    assert!(fraction > 0.0 && fraction < 0.5);
    // Equal approach and pursuit speeds make the squared quadratic's leading term zero.
    assert!(reachable_fraction(Vec2::X * 3.0 * au, Vec2::X * au, Vec2::ZERO, 2.0, 0, 0.5).is_none());
    assert!(reachable_fraction(Vec2::X * 3.0 * au, Vec2::X * au, Vec2::ZERO, 2.0, 0, 0.6).is_some());
    assert!(reachable_fraction(Vec2::X * 3.0 * au, Vec2::X * 0.6 * au, Vec2::ZERO, 2.4, 0, 0.4)
        .is_none());
    assert!(reachable_fraction(Vec2::X * 3.0 * au, Vec2::X * 0.6 * au, Vec2::ZERO, 2.4, 0, 0.5)
        .is_some());
    let tangent = reachable_fraction(
        Vec2::new(0.0, 0.875) * au,
        Vec2::new(5.0, 0.875) * au,
        Vec2::ZERO,
        3.0,
        0,
        1.0,
    )
    .unwrap();
    assert!((tangent - 0.13125).abs() < 1e-6);
    assert!(reachable_fraction(
        Vec2::new(0.0, 0.9) * au,
        Vec2::new(5.0, 0.9) * au,
        Vec2::ZERO,
        3.0,
        0,
        1.0
    )
    .is_none());
}
