use super::*;
use crate::core::constants::{MIN_SPY_PROBES, ORBITAL_RAILGUN_FIRE_ENERGY_COST};
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::Combat;
use bevy::math::Vec2;

#[test]
fn colonial_withdrawal_orders_enforce_ownership_levels_and_home_restriction() {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    let colony = model
        .map
        .planets
        .iter()
        .find(|world| !world.is_moon() && world.owned.is_none())
        .unwrap()
        .id;
    model.map.get_mut(colony).colonize(1);
    for level in 1..=5 {
        model
            .map
            .get_mut(colony)
            .army
            .insert(Unit::Building(Building::ColonialAdministration), level);
        for withdrawal in FleetWithdrawal::ALL {
            let command = TurnCommand::SetFleetWithdrawal {
                planet_id: colony,
                withdrawal,
            };
            let projected = preview_commands(&model, 1, &[command]);
            assert_eq!(projected.is_ok(), level >= withdrawal.minimum_level());
            assert!(apply_fleet_withdrawal(&mut model, 2, colony, withdrawal).is_err());
        }
    }
    assert!(apply_fleet_withdrawal(&mut model, 1, home, FleetWithdrawal::Off).is_err());
    model.players[0].resources = Resources::new(50_000, 50_000, 50_000);
    assert_eq!(
        purchase_limit(
            &model.players[0],
            model.map.get(home),
            Unit::Building(Building::ColonialAdministration),
            3
        ),
        Err(crate::core::orders::OrderError::ColonialAdministration)
    );
    model.map.get_mut(colony).army.remove(&Unit::Building(Building::ColonialAdministration));
    assert_eq!(
        purchase_limit(
            &model.players[0],
            model.map.get(colony),
            Unit::Building(Building::ColonialAdministration),
            3
        ),
        Ok(1)
    );
    let moon = model.map.moons()[0].id;
    model.map.get_mut(moon).controlled = Some(1);
    assert!(purchase_limit(
        &model.players[0],
        model.map.get(moon),
        Unit::Building(Building::ColonialAdministration),
        3
    )
    .is_err());
}

fn withdrawing_colony_model(distance_au: f32) -> (GameModel, usize, usize, Army) {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    let enemy_home = model.players[1].home_planet;
    let colony = model
        .map
        .planets
        .iter()
        .find(|world| !world.is_moon() && world.owned.is_none())
        .unwrap()
        .id;
    let home_position = model.map.get(home).position;
    let ships = Army::from([(Unit::Ship(Ship::LightFighter), 20), (Unit::colony_ship(), 1)]);
    let planet = model.map.get_mut(colony);
    planet.colonize(1);
    planet.position = home_position + Vec2::X * Planet::SIZE * distance_au;
    planet.army = ships.clone().into();
    planet.army.insert(Unit::Building(Building::ColonialAdministration), 5);
    planet.fleet_withdrawal = FleetWithdrawal::Immediate;
    let mut attack = Mission::new_with_id(
        7,
        model.turn as usize,
        2,
        model.map.get(enemy_home),
        model.map.get(colony),
        Icon::Attack,
        Army::from([(Unit::war_sun(), 2)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    attack.position = model.map.get(colony).position;
    model.missions.push(attack);
    (model, home, colony, ships)
}

#[test]
fn colonial_withdrawal_creates_one_normal_deploy_and_survives_save_reload() {
    let (mut model, home, colony, ships) = withdrawing_colony_model(7.);
    let mut repeated = model.clone();
    empty_turn(&mut model);
    empty_turn(&mut repeated);
    assert_eq!(serde_json::to_value(&model).unwrap(), serde_json::to_value(&repeated).unwrap());
    assert_eq!(model.missions.len(), 1);
    let flight = &model.missions[0];
    assert_eq!(flight.owner, 1);
    assert_eq!(flight.origin, colony);
    assert_eq!(flight.destination, home);
    assert_eq!(flight.objective, Icon::Deploy);
    assert_eq!(flight.return_objective, None);
    assert!(!flight.jump_gate);
    assert_eq!(flight.travel_turns, 1);
    assert_eq!(flight.send as u64, model.turn - 1);
    assert_ne!(flight.position, model.map.get(colony).position);
    assert_eq!(flight.army, ships);
    let fresh_departure = Mission::new_with_id(
        100,
        model.turn as usize,
        1,
        model.map.get(colony),
        model.map.get(home),
        Icon::Deploy,
        ships.clone(),
        BombingRaid::None,
        false,
        false,
        None,
    );
    assert_eq!(fresh_departure.turns_to_destination(&model.map), 3);
    assert_eq!(flight.turns_to_destination(&model.map), 2);
    assert_eq!(flight.is_seen_by_phalanx(&model.map, &model.players[1]), None);
    assert_eq!(flight.is_seen_by_radar(&model.map, &model.players[1]), None);
    let mut scanned_map = model.map.clone();
    let moon = scanned_map.moons()[0].id;
    scanned_map.get_mut(moon).controlled = Some(2);
    scanned_map.get_mut(moon).position = flight.position;
    scanned_map.get_mut(moon).army.insert(Unit::Building(Building::OrbitalRadar), 5);
    assert_eq!(flight.is_seen_by_radar(&scanned_map, &model.players[1]), Some(5));
    assert_eq!(model.map.get(colony).army.amount(&Unit::Ship(Ship::LightFighter)), 0);
    assert_eq!(model.map.get(colony).controlled, Some(2));
    assert_eq!(model.map.get(colony).fleet_withdrawal, FleetWithdrawal::Off);
    let report = model.players[0].reports.last().unwrap();
    assert_eq!(report.escaped_defenders(&Unit::Ship(Ship::LightFighter)), 20);
    let saved = PersistedGame::new(model).to_json().unwrap();
    let mut model = PersistedGame::from_json(saved).unwrap().state;
    for _ in 0..10 {
        if model.missions.is_empty() {
            break;
        }
        empty_turn(&mut model);
    }
    assert!(model.missions.is_empty());
    assert_eq!(model.map.get(home).army.amount(&Unit::Ship(Ship::LightFighter)), 20);
    assert_eq!(model.map.get(home).army.amount(&Unit::colony_ship()), 1);
}

#[test]
fn lone_colony_ship_withdraws_as_a_mission_without_combat_playback() {
    let (mut model, home, colony, _) = withdrawing_colony_model(7.);
    model.map.get_mut(colony).army = Army::from([
        (Unit::colony_ship(), 1),
        (Unit::Building(Building::ColonialAdministration), 5),
    ])
    .into();

    empty_turn(&mut model);

    assert_eq!(model.missions.len(), 1);
    let flight = &model.missions[0];
    assert_eq!((flight.owner, flight.origin, flight.destination), (1, colony, home));
    assert_eq!(flight.objective, Icon::Deploy);
    assert_eq!(flight.army, Army::from([(Unit::colony_ship(), 1)]));
    let report =
        model.players.iter().find(|player| player.id == 1).unwrap().reports.last().unwrap();
    assert!(!report.has_combat_playback());
}

#[test]
fn one_turn_colonial_withdrawal_docks_at_home_in_the_battle_turn() {
    let (mut model, home, colony, ships) = withdrawing_colony_model(2.);
    empty_turn(&mut model);
    assert!(model.missions.is_empty());
    assert_eq!(
        model.map.get(home).army.amount(&Unit::Ship(Ship::LightFighter)),
        ships.amount(&Unit::Ship(Ship::LightFighter))
    );
    assert_eq!(model.map.get(home).army.amount(&Unit::colony_ship()), 1);
    assert_eq!(model.map.get(colony).army.amount(&Unit::Ship(Ship::LightFighter)), 0);
    assert!(model.players[0]
        .reports
        .iter()
        .any(|report| report.escaped_defenders(&Unit::Ship(Ship::LightFighter)) == 20));
    model.validate().unwrap();
}

#[test]
fn world_acquisition_order_survives_reinforcement_colonization_and_resume() {
    let mut model = started_model(2);
    let home = model.players[0].home_planet;
    let first = model.map.moons()[0].id;
    let second = model
        .map
        .planets
        .iter()
        .rev()
        .find(|planet| !planet.is_moon() && planet.controlled.is_none())
        .unwrap()
        .id;
    for (index, (destination, objective)) in
        [(first, Icon::Attack), (second, Icon::Colonize), (first, Icon::Deploy)]
            .into_iter()
            .enumerate()
    {
        let target = model.map.get_mut(destination);
        if index < 2 {
            target.army.clear();
        }
        let army = Army::from([(Unit::Ship(Ship::LightFighter), 1), (Unit::colony_ship(), 1)]);
        let mut mission = Mission::new_with_id(
            index as u64 + 1,
            model.turn as usize,
            1,
            model.map.get(home),
            model.map.get(destination),
            objective,
            army,
            BombingRaid::None,
            false,
            false,
            None,
        );
        mission.position = model.map.get(destination).position;
        model.missions.push(mission);
        empty_turn(&mut model);
    }
    assert_eq!(model.map.get(first).controlled, Some(1));
    assert_eq!(model.map.get(second).owned, Some(1));
    assert_eq!(model.players[0].world_acquisition_order, vec![home, first, second]);
    // Moving between the owned and controlled groups preserves first acquisition.
    model.map.get_mut(second).abandon();
    model.map.get_mut(second).army.insert(Unit::colony_ship(), 1);
    apply_colonize(&mut model, 1, second).unwrap();
    model.players[0].reports.clear();
    let loaded = PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap();
    assert_eq!(loaded.state.players[0].world_acquisition_order, vec![home, first, second]);
}

#[test]
fn invalid_world_acquisition_histories_are_rejected() {
    let model = started_model(2);
    let home = model.players[0].home_planet;
    for history in [vec![], vec![home, home], vec![home, usize::MAX]] {
        let mut invalid = model.clone();
        invalid.players[0].world_acquisition_order = history;
        assert!(invalid.validate().is_err());
    }
}

/// Creates and starts a deterministic model for unit tests.
fn started_model(player_count: u8) -> GameModel {
    let mut model = GameModel::new(
        [player_count; 32],
        GameRules {
            player_count,
            ..GameRules::default()
        },
    )
    .unwrap();
    model.start().unwrap();
    model
}

#[test]
fn protection_permission_is_planet_specific_and_controller_only() {
    let mut model = started_model(3);
    let protected = model.players[0].home_planet;

    apply_protection_permission(&mut model, 1, protected, 2, true).unwrap();

    assert!(model.map.get(protected).allows_protection(2));
    assert!(!model.map.get(protected).allows_protection(3));
    assert!(apply_protection_permission(&mut model, 2, protected, 3, true).is_err());
    assert!(apply_protection_permission(&mut model, 1, protected, 1, true).is_err());
    let loaded = PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap();
    assert!(loaded.state.map.get(protected).allows_protection(2));
}

#[test]
fn only_the_protector_can_dispatch_their_stationed_fleet() {
    let mut model = started_model(3);
    let protected = model.players[0].home_planet;
    let target = model.players[2].home_planet;
    let fighter = Unit::Ship(Ship::LightFighter);
    model.players[1].resources = Resources::new(1_000_000, 1_000_000, 1_000_000);
    model.map.get_mut(protected).protection_permissions.insert(2);
    model.map.get_mut(protected).army.dock_protector(2, Army::from([(fighter, 5)]));
    let host_army = model.map.get(protected).army.controller().clone();
    let attack = TurnCommand::SendMission {
        mission_id: 700,
        origin: protected,
        destination: target,
        objective: Icon::Attack,
        army: Army::from([(fighter, 3)]),
        bombing: BombingRaid::None,
        combat_probes: false,
        jump_gate: false,
    };

    assert!(preview_commands(&model, 1, std::slice::from_ref(&attack)).is_err());
    let preview = preview_commands(&model, 2, &[attack]).unwrap();

    assert_eq!(preview.map.get(protected).army.controller(), &host_army);
    assert_eq!(preview.map.get(protected).army.protector(2).unwrap().amount(&fighter), 2);
    assert_eq!(preview.missions.len(), 1);
    assert_eq!(
        (preview.missions[0].owner, preview.missions[0].origin, preview.missions[0].objective),
        (2, protected, Icon::Attack)
    );
}

#[test]
fn canceled_in_flight_protection_returns_to_the_protectors_homeworld() {
    let mut model = started_model(3);
    let protected = model.players[0].home_planet;
    let protector_home = model.players[1].home_planet;
    model.map.get_mut(protected).protection_permissions.insert(2);
    let mut mission = Mission::new_with_id(
        701,
        model.turn as usize,
        2,
        model.map.get(protector_home),
        model.map.get(protected),
        Icon::Protect,
        Army::from([(Unit::Ship(Ship::Cruiser), 2)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.position = model.map.get(protected).position;
    model.missions.push(mission);
    let submissions = vec![
        TurnSubmission::new(
            1,
            model.turn,
            vec![TurnCommand::SetProtectionPermission {
                planet_id: protected,
                protector: 2,
                allowed: false,
            }],
        ),
        TurnSubmission::new(2, model.turn, vec![]),
        TurnSubmission::new(3, model.turn, vec![]),
    ];

    resolve_turn(&mut model, &submissions).unwrap();

    let returning = model.missions.iter().find(|mission| mission.id == 701).unwrap();
    assert_eq!(returning.destination, protector_home);
    assert_eq!(returning.objective, Icon::Deploy);
    assert_eq!(returning.return_objective, Some(Icon::Protect));
    assert_eq!(returning.protected_player, None);
    assert!(returning.logs.contains("returning to home planet"));
}

#[test]
fn same_turn_revocation_accepts_then_recalls_a_planned_protect_launch() {
    let mut model = started_model(3);
    let protected = model.players[0].home_planet;
    let protector_home = model.players[1].home_planet;
    let cruiser = Unit::Ship(Ship::Cruiser);
    model.map.get_mut(protected).protection_permissions.insert(2);
    model.map.get_mut(protector_home).army.insert(cruiser, 2);
    model.players[1].resources = Resources::new(1_000_000, 1_000_000, 1_000_000);
    let submissions = vec![
        TurnSubmission::new(
            1,
            model.turn,
            vec![TurnCommand::SetProtectionPermission {
                planet_id: protected,
                protector: 2,
                allowed: false,
            }],
        ),
        TurnSubmission::new(
            2,
            model.turn,
            vec![TurnCommand::SendMission {
                mission_id: 704,
                origin: protector_home,
                destination: protected,
                objective: Icon::Protect,
                army: Army::from([(cruiser, 2)]),
                bombing: BombingRaid::None,
                combat_probes: false,
                jump_gate: false,
            }],
        ),
        TurnSubmission::new(3, model.turn, vec![]),
    ];

    resolve_turn(&mut model, &submissions).unwrap();

    assert!(model.missions.iter().all(|mission| mission.id != 704));
    assert_eq!(model.map.get(protector_home).army.amount(&cruiser), 2);
    assert!(model.map.get(protected).army.protector(2).is_none());
    let report = model.players[1].reports.iter().find(|report| report.mission.id == 704).unwrap();
    assert_eq!(report.mission.destination, protector_home);
    assert_eq!(report.mission.objective, Icon::Deploy);
    assert_eq!(report.mission.return_objective, Some(Icon::Protect));
}

#[test]
fn revoking_stationed_protection_launches_one_homeward_return() {
    let mut model = started_model(3);
    let protected = model.players[0].home_planet;
    let protector_home = model.players[1].home_planet;
    let fleet = Army::from([(Unit::Ship(Ship::LightFighter), 4), (Unit::Ship(Ship::Cruiser), 1)]);
    model.map.get_mut(protected).protection_permissions.insert(2);
    model.map.get_mut(protected).army.dock_protector(2, fleet.clone());
    let submissions = vec![
        TurnSubmission::new(
            1,
            model.turn,
            vec![TurnCommand::SetProtectionPermission {
                planet_id: protected,
                protector: 2,
                allowed: false,
            }],
        ),
        TurnSubmission::new(2, model.turn, vec![]),
        TurnSubmission::new(3, model.turn, vec![]),
    ];

    resolve_turn(&mut model, &submissions).unwrap();

    assert!(model.map.get(protected).army.protector(2).is_none());
    let returning = model.missions.iter().find(|mission| mission.owner == 2).unwrap();
    assert_eq!((returning.origin, returning.destination), (protected, protector_home));
    assert_eq!(returning.objective, Icon::Deploy);
    assert_eq!(returning.return_objective, Some(Icon::Protect));
    assert_eq!(returning.army, fleet);
}

#[test]
fn same_turn_protection_arrives_before_an_attack() {
    let mut model = started_model(3);
    let protected = model.players[0].home_planet;
    let protector_home = model.players[1].home_planet;
    let attacker_home = model.players[2].home_planet;
    let support = Army::from([(Unit::Ship(Ship::Cruiser), 3)]);
    model.map.get_mut(protected).protection_permissions.insert(2);
    let mut protection = Mission::new_with_id(
        702,
        model.turn as usize,
        2,
        model.map.get(protector_home),
        model.map.get(protected),
        Icon::Protect,
        support.clone(),
        BombingRaid::None,
        false,
        false,
        None,
    );
    protection.position = model.map.get(protected).position;
    let mut attack = Mission::new_with_id(
        703,
        model.turn as usize,
        3,
        model.map.get(attacker_home),
        model.map.get(protected),
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    attack.position = model.map.get(protected).position;
    model.missions.extend([protection, attack]);

    empty_turn(&mut model);

    let report = model.players[2].reports.iter().find(|report| report.mission.id == 703).unwrap();
    assert_eq!(report.planet.army.protector(2), Some(&support));
    assert_eq!(report.defender_players(), vec![1, 2]);
    let defenders = &report.combat_report.as_ref().unwrap().rounds[0].defender;
    assert!(defenders.iter().any(|unit| unit.owner == Some(1)));
    assert!(defenders.iter().any(|unit| unit.owner == Some(2)));
    assert_eq!(model.map.get(protected).army, report.surviving_defender);
    assert!(report.surviving_defender.protector(2).is_some());
}

#[test]
fn planet_persists_one_owner_aware_garrison_instead_of_parallel_armies() {
    let mut model = started_model(3);
    let protected = model.players[0].home_planet;
    model
        .map
        .get_mut(protected)
        .army
        .dock_protector(2, Army::from([(Unit::Ship(Ship::Cruiser), 2)]));

    let json = serde_json::to_value(model.map.get(protected)).unwrap();
    assert!(json.get("protecting_fleets").is_none());
    assert!(json["army"].get("controller").is_some());
    assert!(json["army"]["protectors"].get("2").is_some());

    let restored: Planet = serde_json::from_value(json).unwrap();
    assert_eq!(restored.army, model.map.get(protected).army);
}

#[test]
fn every_home_planet_starts_with_five_rocket_launchers() {
    for player_count in 1..=4 {
        let mut model = GameModel::new(
            [player_count; 32],
            GameRules {
                player_count,
                practice_mode: player_count == 1,
                ..GameRules::default()
            },
        )
        .unwrap();
        model.start().unwrap();
        for player in &model.players {
            assert_eq!(
                model.map.solar_band(player.home_planet),
                Some(crate::core::map::planet::SolarBand::Temperate)
            );
            assert_eq!(
                model
                    .map
                    .get(player.home_planet)
                    .army
                    .amount(&Unit::Defense(Defense::RocketLauncher)),
                5
            );
        }
    }
}

#[test]
/// Proves identical state and submissions produce byte-identical next state.
fn resolution_is_deterministic() {
    let mut left = started_model(2);
    let mut right = left.clone();
    let submissions = left
        .players
        .iter()
        .map(|player| TurnSubmission::new(player.id, left.turn, Vec::new()))
        .collect::<Vec<_>>();

    assert_eq!(resolve_turn(&mut left, &submissions), resolve_turn(&mut right, &submissions));
    assert_eq!(
        serde_json::to_vec(&PersistedGame::new(left)).unwrap(),
        serde_json::to_vec(&PersistedGame::new(right)).unwrap()
    );
}

#[test]
/// Validates every supported multiplayer player count.
fn supports_two_through_four_players() {
    for count in 2..=4 {
        let model = started_model(count);
        assert_eq!(model.players.len(), usize::from(count));
        assert_eq!(
            model.players.iter().map(|player| player.id).collect::<HashSet<_>>().len(),
            usize::from(count)
        );
    }
}

#[test]
/// Advances a one-player practice match without declaring an automatic victory.
fn local_practice_resolves_immediately_and_stays_active() {
    let mut model = GameModel::new(
        [1; 32],
        GameRules {
            player_count: 1,
            practice_mode: true,
            ..GameRules::default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let result = resolve_turn(&mut model, &[TurnSubmission::new(1, 1, Vec::new())]).unwrap();
    assert_eq!(result.turn, 2);
    assert!(!result.finished);
    assert_eq!(model.status, MatchStatus::Active);
    assert!(!model.players[0].spectator);
}

#[test]
fn testing_boost_covers_controlled_worlds_and_uses_each_worlds_roster() {
    for owned_worlds_only in [false, true] {
        let mut model = GameModel::new(
            [1; 32],
            GameRules {
                player_count: 1,
                practice_mode: true,
                ..GameRules::default()
            },
        )
        .unwrap();
        model.start().unwrap();
        let home = model.players[0].home_planet;
        let moon = model.map.moons()[0].id;
        model.map.get_mut(moon).controlled = Some(1);
        // Fleet-controlled planets are valid targets even before they are colonized.
        let occupied = model.map.planets().into_iter().find(|p| p.id != home).unwrap().id;
        model.map.get_mut(occupied).controlled = Some(1);
        let commands = vec![
            TurnCommand::PracticeBoost {
                owned_worlds_only,
            },
            TurnCommand::BuyUnits {
                planet_id: home,
                unit: Unit::war_sun(),
                count: 1,
            },
        ];
        let preview = preview_commands(&model, 1, &commands).unwrap();
        assert_eq!(preview.map.get(moon).owned, None, "moons are controlled, never colonized");
        assert_eq!(
            preview.map.get(occupied).army.amount(&Unit::war_sun()),
            3,
            "controlled planets must be included in the Shift shortcut"
        );
        for unit in Unit::all().into_iter().flatten() {
            let expected = if unit.valid_on(true) {
                if unit.is_building() {
                    Building::MAX_LEVEL
                } else {
                    3
                }
            } else {
                0
            };
            assert_eq!(
                preview.map.get(moon).army.amount(&unit),
                expected,
                "unexpected testing amount for {unit:?} on a moon"
            );
        }
        for unit in Unit::all().into_iter().flatten() {
            let expected = if unit.valid_on(false) {
                match unit {
                    Unit::Building(Building::ColonialAdministration) => 0,
                    Unit::Building(Building::Senate) | Unit::Defense(Defense::SpaceDock) => 1,
                    Unit::Building(_) => Building::MAX_LEVEL,
                    Unit::Ship(_) | Unit::Defense(_) => model.map.get(home).army.amount(&unit) + 3,
                }
            } else {
                0
            };
            assert_eq!(
                preview.map.get(home).army.amount(&unit),
                expected,
                "unexpected testing amount for {unit:?} on a planet"
            );
        }
        assert_eq!(
            preview
                .map
                .planets
                .iter()
                .map(|planet| planet.army.amount(&Unit::Building(Building::Senate)))
                .sum::<usize>(),
            1,
            "the testing shortcut must create only the home-world Senate"
        );
        for planet in &preview.map.planets {
            let targeted =
                !owned_worlds_only || planet.owned == Some(1) || planet.controlled == Some(1);
            assert_eq!(
                planet.army.amount(&Unit::space_dock()),
                usize::from(targeted && !planet.is_moon()),
                "the testing shortcut must create at most one Space Dock per planet"
            );
        }
        let mut preview_rng_state = preview.rng.clone();
        let mut preview_rng = preview_rng_state.next_rng();
        let expected_resources = preview.players[0].resources
            + preview.players[0].energy_grid(&preview.map).scale_resources(
                preview.players[0].raw_resource_production(&preview.map)
                    + recycler_production(
                        &preview.map,
                        &preview.players,
                        1,
                        preview.turn as usize,
                        &mut preview_rng,
                    ),
            );
        resolve_turn(&mut model, &[TurnSubmission::new(1, 1, commands)]).unwrap();
        assert_eq!(model.turn, 2);
        assert_eq!(model.status, MatchStatus::Active);
        assert_eq!(model.players[0].resources, expected_resources);
        for planet in &model.map.planets {
            assert_eq!(
                planet.army.amount(&Unit::war_sun()),
                if planet.id == home {
                    4
                } else if !owned_worlds_only || planet.id == moon || planet.id == occupied {
                    3
                } else {
                    0
                }
            );
        }
        for unit in Unit::all().into_iter().flatten().filter(|unit| !unit.valid_on(true)) {
            assert_eq!(
                model.map.get(moon).army.amount(&unit),
                0,
                "{unit:?} must remain absent from a moon after resolution"
            );
        }
    }
}

#[test]
fn multiplayer_replays_testing_boost_for_every_peer() {
    let mut model = started_model(2);
    let player_id = model.players[0].id;
    let home = model.players[0].home_planet;
    let resources = model.players[0].resources;
    let commands = vec![TurnCommand::PracticeBoost {
        owned_worlds_only: true,
    }];
    let preview = preview_commands(&model, player_id, &commands).unwrap();
    assert_eq!(preview.players[0].resources, resources + 1_000usize);
    assert_eq!(preview.map.get(home).army.amount(&Unit::war_sun()), 3);

    resolve_turn(
        &mut model,
        &[TurnSubmission::new(player_id, 1, commands), TurnSubmission::new(2, 1, Vec::new())],
    )
    .unwrap();
    assert_eq!(model.turn, 2);
    assert_eq!(model.map.get(home).army.amount(&Unit::war_sun()), 3);
}

#[test]
fn abandoning_a_planet_keeps_control_only_while_a_fleet_remains() {
    let model = started_model(2);
    let player_id = model.players[0].id;
    let planet_id = model
        .map
        .planets()
        .into_iter()
        .find(|planet| model.players.iter().all(|player| player.home_planet != planet.id))
        .unwrap()
        .id;
    let fighter = Unit::Ship(Ship::LightFighter);

    for stationed_fighters in [0, 1] {
        let mut state = model.clone();
        let planet = state.map.get_mut(planet_id);
        planet.army.clear();
        planet.colonize(player_id);
        // Owned planets are controlled by definition. Exercise the sparse presentation form that
        // triggered the same-turn Voronoi fade, where that redundant value is not populated.
        planet.controlled = None;
        planet.army.insert(fighter, stationed_fighters);

        let preview = preview_commands(
            &state,
            player_id,
            &[TurnCommand::AbandonPlanet {
                planet_id,
            }],
        )
        .unwrap();
        let abandoned = preview.map.get(planet_id);

        assert_eq!(abandoned.owned, None);
        assert_eq!(
            abandoned.controlled,
            (stationed_fighters > 0).then_some(player_id),
            "control must follow stationed units immediately in the turn preview"
        );
    }
}

#[test]
fn direct_colonization_adds_balanced_starter_infrastructure() {
    let mut model = started_model(2);
    let player_id = model.players[0].id;
    let planet_id =
        model.map.planets().into_iter().find(|planet| planet.controlled.is_none()).unwrap().id;
    let planet = model.map.get_mut(planet_id);
    planet.controlled = Some(player_id);
    planet.army.clear();
    planet.army.insert(Unit::colony_ship(), 1);

    apply_colonize(&mut model, player_id, planet_id).unwrap();

    let planet = model.map.get(planet_id);
    for building in [
        Building::MetalMine,
        Building::CrystalMine,
        Building::DeuteriumSynthesizer,
        Building::Reactor,
    ] {
        assert_eq!(planet.army.amount(&Unit::Building(building)), 1, "{building:?}");
    }
    assert_eq!(planet.army.amount(&Unit::colony_ship()), 0);
    assert_eq!(crate::core::energy::EnergyGrid::for_world(&model.map, planet).balance(), 0);
}

#[test]
/// Persists practice mode explicitly and accepts every locally controlled player count.
fn practice_rules_are_explicit() {
    let json = serde_json::to_value(GameRules::default()).unwrap();
    assert_eq!(json.get("practice_mode"), Some(&serde_json::json!(false)));
    let loaded: GameRules = serde_json::from_value(json).unwrap();
    assert!(!loaded.practice_mode);
    for player_count in 1..=4 {
        assert!(GameModel::new(
            [player_count; 32],
            GameRules {
                player_count,
                practice_mode: true,
                ..GameRules::default()
            }
        )
        .is_ok());
    }
    for player_count in [0, 5, u8::MAX] {
        assert!(matches!(
            GameModel::new(
                [player_count; 32],
                GameRules {
                    player_count,
                    practice_mode: true,
                    ..GameRules::default()
                }
            ),
            Err(GameError::InvalidPlayerCount(value)) if value == player_count
        ));
    }
}

#[test]
/// Rejects player counts outside the public contract.
fn rejects_player_count_boundaries() {
    for count in [0, 1, 5, u8::MAX] {
        let result = GameModel::new(
            [count; 32],
            GameRules {
                player_count: count,
                ..GameRules::default()
            },
        );
        assert!(matches!(result, Err(GameError::InvalidPlayerCount(value)) if value == count));
    }
}

#[test]
/// Rejects missing, duplicate, and stale simultaneous submissions.
fn validates_submission_set() {
    let model = started_model(2);
    let first = TurnSubmission::new(1, model.turn, Vec::new());
    assert!(matches!(
        resolve_turn(&mut model.clone(), std::slice::from_ref(&first)),
        Err(GameError::MissingSubmission(2))
    ));
    assert!(matches!(
        resolve_turn(&mut model.clone(), &[first.clone(), first]),
        Err(GameError::DuplicateSubmission(1))
    ));
    let stale = model
        .players
        .iter()
        .map(|player| TurnSubmission::new(player.id, 0, Vec::new()))
        .collect::<Vec<_>>();
    assert!(matches!(resolve_turn(&mut model.clone(), &stale), Err(GameError::StaleTurn { .. })));

    let oversized = TurnSubmission::new(
        1,
        model.turn,
        vec![
            TurnCommand::AbandonPlanet {
                planet_id: 0
            };
            MAX_COMMANDS_PER_SUBMISSION + 1
        ],
    );
    let second = TurnSubmission::new(2, model.turn, Vec::new());
    assert!(matches!(
        resolve_turn(&mut model.clone(), &[oversized, second]),
        Err(GameError::InvalidCommand {
            player_id: 1,
            ..
        })
    ));
}

#[test]
/// Mission commands produced by the UI may include unselected zero-count ship entries.
fn mission_commands_ignore_zero_count_units() {
    let mut model = started_model(2);
    let origin = model.players[0].home_planet;
    let destination = model.players[1].home_planet;
    let heavy_fighter = Unit::Ship(Ship::HeavyFighter);
    model.map.get_mut(origin).army.insert(heavy_fighter, 2);
    let selected = Army::from([(heavy_fighter, 2), (Unit::Ship(Ship::LightFighter), 0)]);

    apply_mission(
        &mut model,
        1,
        7,
        origin,
        destination,
        Icon::Attack,
        &selected,
        BombingRaid::None,
        false,
        false,
    )
    .unwrap();

    assert_eq!(model.missions[0].army, Army::from([(heavy_fighter, 2)]));
    assert_eq!(model.map.get(origin).army.amount(&heavy_fighter), 0);
}

#[test]
fn every_recallable_mission_type_can_be_recalled_from_its_current_position_for_free() {
    let model = started_model(2);
    let player_id = model.players[0].id;
    let original_origin = model.players[0].home_planet;
    let outbound_destination = model.players[1].home_planet;
    let current_position = model
        .map
        .get(original_origin)
        .position
        .lerp(model.map.get(outbound_destination).position, 0.6);

    for (index, (objective, army)) in [
        (Icon::Deploy, Army::from([(Unit::Ship(Ship::LightFighter), 1)])),
        (Icon::Colonize, Army::from([(Unit::colony_ship(), 1)])),
        (Icon::Attack, Army::from([(Unit::Ship(Ship::Bomber), 1)])),
        (Icon::Spy, Army::from([(Unit::probe(), MIN_SPY_PROBES)])),
        (Icon::Destroy, Army::from([(Unit::war_sun(), 1)])),
    ]
    .into_iter()
    .enumerate()
    {
        let mut state = model.clone();
        let mut mission = Mission::new_with_id(
            100 + index as u64,
            state.turn as usize,
            player_id,
            state.map.get(original_origin),
            state.map.get(outbound_destination),
            objective,
            army,
            if objective == Icon::Attack {
                BombingRaid::Economic
            } else {
                BombingRaid::None
            },
            objective == Icon::Spy,
            false,
            None,
        );
        mission.position = current_position;
        mission.travel_turns = 1;
        state.missions.push(mission);
        let resources = state.player(player_id).unwrap().resources;

        let preview = preview_commands(
            &state,
            player_id,
            &[TurnCommand::RecallMission {
                mission_id: 100 + index as u64,
            }],
        )
        .unwrap();
        preview.validate().unwrap();
        let recalled = &preview.missions[0];

        assert_eq!(preview.player(player_id).unwrap().resources, resources);
        assert_eq!(recalled.position, current_position);
        assert_eq!(recalled.origin, outbound_destination);
        assert_eq!(recalled.destination, original_origin);
        assert_eq!(recalled.travel_turns, 0);
        assert_eq!(recalled.objective, Icon::Deploy);
        assert_eq!(recalled.return_objective, Some(objective));
        assert_eq!(recalled.bombing, BombingRaid::None);
        assert!(!recalled.combat_probes);
        assert!(!recalled.jump_gate);
        assert!(recalled.is_returning());
        assert!(recalled.logs.contains("Mission recalled to planet"));
    }
}

#[test]
fn missile_strikes_cannot_be_recalled_once_launched() {
    let mut model = started_model(2);
    let player_id = model.players[0].id;
    let origin = model.players[0].home_planet;
    let destination = model.players[1].home_planet;
    model.missions.push(Mission::new_with_id(
        70,
        model.turn as usize,
        player_id,
        model.map.get(origin),
        model.map.get(destination),
        Icon::MissileStrike,
        Army::from([(Unit::interplanetary_missile(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    ));

    assert!(matches!(
        preview_commands(
            &model,
            player_id,
            &[TurnCommand::RecallMission { mission_id: 70 }]
        ),
        Err(GameError::InvalidCommand { reason, .. })
            if reason == "missile strikes cannot be recalled once launched"
    ));
}

#[test]
fn recall_commands_reject_foreign_missing_and_already_returning_missions() {
    let mut model = started_model(2);
    let player_id = model.players[0].id;
    let origin = model.players[0].home_planet;
    let destination = model.players[1].home_planet;
    model.missions.push(Mission::new_with_id(
        71,
        model.turn as usize,
        player_id,
        model.map.get(origin),
        model.map.get(destination),
        Icon::Attack,
        Army::from([(Unit::probe(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    ));

    assert!(matches!(
        preview_commands(
            &model,
            model.players[1].id,
            &[TurnCommand::RecallMission {
                mission_id: 71
            }]
        ),
        Err(GameError::InvalidCommand { .. })
    ));
    assert!(matches!(
        preview_commands(
            &model,
            player_id,
            &[TurnCommand::RecallMission {
                mission_id: u64::MAX
            }]
        ),
        Err(GameError::InvalidCommand { .. })
    ));

    let recalled = preview_commands(
        &model,
        player_id,
        &[TurnCommand::RecallMission {
            mission_id: 71,
        }],
    )
    .unwrap();
    assert!(matches!(
        preview_commands(
            &recalled,
            player_id,
            &[TurnCommand::RecallMission {
                mission_id: 71
            }]
        ),
        Err(GameError::InvalidCommand { .. })
    ));
}

#[test]
fn departing_planets_and_moons_release_control_only_when_vacant() {
    let base = started_model(2);
    let player_id = base.players[0].id;
    let destination = base.players[0].home_planet;
    let planet = base
        .map
        .planets()
        .into_iter()
        .find(|planet| base.players.iter().all(|player| player.home_planet != planet.id))
        .unwrap()
        .id;
    let moon = base.map.moons()[0].id;
    let fighter = Unit::Ship(Ship::LightFighter);

    for (world_id, building) in
        [(planet, Unit::Building(Building::MetalMine)), (moon, Unit::Building(Building::LunarBase))]
    {
        for (building_count, stationed_fighters, expected_control) in
            [(0, 1, None), (1, 1, Some(player_id)), (0, 2, Some(player_id))]
        {
            let mut model = base.clone();
            let world = model.map.get_mut(world_id);
            world.owned = None;
            world.controlled = Some(player_id);
            world.army.clear();
            world.army.insert(building, building_count);
            world.army.insert(fighter, stationed_fighters);

            apply_mission(
                &mut model,
                player_id,
                100 + world_id as u64,
                world_id,
                destination,
                Icon::Deploy,
                &Army::from([(fighter, 1)]),
                BombingRaid::None,
                false,
                false,
            )
            .unwrap();

            assert_eq!(
                model.map.get(world_id).controlled,
                expected_control,
                "control must match the buildings and ships left on world {world_id}"
            );
        }
    }
}

#[test]
fn colonize_missions_preserve_intent_through_friendly_ownership_changes() {
    for (owned_midway, owned_on_arrival) in [(false, false), (true, false), (true, true)] {
        let mut model = started_model(2);
        let player = model.players[0].clone();
        let origin = model.map.get(player.home_planet).clone();
        let destination = model
            .map
            .planets()
            .into_iter()
            .find(|planet| planet.controlled.is_none())
            .unwrap()
            .clone();
        let mut mission = Mission::new_with_id(
            71,
            model.turn as usize,
            player.id,
            &origin,
            &destination,
            Icon::Colonize,
            Army::from([(Unit::colony_ship(), 1)]),
            BombingRaid::None,
            false,
            false,
            None,
        );
        mission.position =
            destination.position + bevy::math::Vec2::X * Planet::SIZE * mission.speed() * 5.0;
        model.missions.push(mission);
        let fighter = Unit::Ship(Ship::LightFighter);
        let target = model.map.get_mut(destination.id);
        target.controlled = Some(player.id);
        target.owned = owned_midway.then_some(player.id);
        target.army = Army::from([(fighter, 3)]).into();

        let submissions = model
            .players
            .iter()
            .map(|candidate| TurnSubmission::new(candidate.id, model.turn, Vec::new()))
            .collect::<Vec<_>>();
        resolve_turn(&mut model, &submissions).unwrap();
        assert_eq!(model.missions[0].objective, Icon::Colonize);

        model.map.get_mut(destination.id).owned = owned_on_arrival.then_some(player.id);
        model.missions[0].position = destination.position;
        let submissions = model
            .players
            .iter()
            .map(|candidate| TurnSubmission::new(candidate.id, model.turn, Vec::new()))
            .collect::<Vec<_>>();
        resolve_turn(&mut model, &submissions).unwrap();

        let target = model.map.get(destination.id);
        assert_eq!(target.owned, Some(player.id));
        assert_eq!(target.controlled, Some(player.id));
        assert_eq!(target.army.amount(&fighter), 3);
        assert_eq!(target.army.amount(&Unit::colony_ship()), usize::from(owned_on_arrival));
        for building in [
            Building::MetalMine,
            Building::CrystalMine,
            Building::DeuteriumSynthesizer,
            Building::Reactor,
        ] {
            assert_eq!(
                target.army.amount(&Unit::Building(building)),
                usize::from(!owned_on_arrival),
                "{building:?}"
            );
        }
        assert!(model.missions.is_empty());
        let report = model.players[0].reports.last().unwrap();
        assert_eq!(report.mission.objective, Icon::Colonize);
        assert_eq!(report.planet_colonized, !owned_on_arrival);
    }
}

#[test]
fn resolved_spy_and_destroy_missions_preserve_their_images_on_the_return_trip() {
    let mut model = started_model(2);
    let player = model.players[0].clone();
    let origin = model.map.get(player.home_planet).clone();
    let destination = model
        .map
        .planets
        .iter()
        .find(|planet| model.players.iter().all(|candidate| candidate.home_planet != planet.id))
        .unwrap()
        .clone();
    model.map.get_mut(destination.id).army = Army::new().into();

    let mut spy = Mission::new_with_id(
        51,
        model.turn as usize,
        player.id,
        &origin,
        &destination,
        Icon::Spy,
        Army::from([(Unit::probe(), 2)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    spy.position = destination.position;
    let mut destroy = Mission::new_with_id(
        52,
        model.turn as usize,
        player.id,
        &origin,
        &destination,
        Icon::Destroy,
        Army::from([(Unit::war_sun(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    destroy.position = destination.position;
    model.missions.extend([spy, destroy]);

    let submissions = model
        .players
        .iter()
        .map(|candidate| TurnSubmission::new(candidate.id, model.turn, Vec::new()))
        .collect::<Vec<_>>();
    resolve_turn(&mut model, &submissions).unwrap();

    for (objective, image) in [(Icon::Spy, "mission spy"), (Icon::Destroy, "mission destroy")] {
        let returning = model
            .missions
            .iter()
            .find(|mission| mission.return_objective == Some(objective))
            .expect("the resolved mission should still be travelling home");
        assert_eq!(returning.objective, Icon::Deploy);
        assert_eq!(returning.image(&player), image);
    }
}

#[test]
fn mission_redirected_from_destroyed_destination_has_distinct_route_endpoints() {
    let mut model = started_model(2);
    let player = model.players[0].clone();
    let origin = model.map.get(player.home_planet).clone();
    let destination = model
        .map
        .planets
        .iter()
        .find(|planet| {
            !planet.is_moon()
                && model.players.iter().all(|candidate| candidate.home_planet != planet.id)
        })
        .unwrap()
        .clone();
    let mut mission = Mission::new_with_id(
        53,
        model.turn as usize,
        player.id,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.advance(&model.map);
    let position_before_redirect = mission.position;
    let redirect_turn = model.turn as usize + 1;

    model.map.get_mut(destination.id).destroy();
    check_mission(
        &mut mission,
        &model.map,
        redirect_turn,
        usize::MAX,
        Some(model.players[0].home_planet),
    );

    assert_eq!(mission.origin, destination.id);
    assert_eq!(mission.destination, origin.id);
    assert_ne!(mission.origin, mission.destination);
    assert_eq!(mission.position, position_before_redirect);
    assert_eq!(mission.send, redirect_turn);
    assert_eq!(mission.travel_turns, 0);
    assert_eq!(mission.objective, Icon::Deploy);
    assert!(!mission.jump_gate);
    model.turn += 1;
    model.missions.push(mission);
    model.validate().unwrap();
}

#[test]
fn end_of_turn_recheck_redirects_every_mission_from_a_destroyed_destination() {
    let mut model = started_model(2);
    let player = model.players[0].clone();
    let origin = model.map.get(player.home_planet).clone();
    let destination = model
        .map
        .planets
        .iter()
        .find(|planet| {
            !planet.is_moon()
                && model.players.iter().all(|candidate| candidate.home_planet != planet.id)
        })
        .unwrap()
        .clone();
    model.missions.extend((60..63).map(|id| {
        let mut mission = Mission::new_with_id(
            id,
            model.turn as usize,
            player.id,
            &origin,
            &destination,
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
            BombingRaid::None,
            false,
            false,
            None,
        );
        mission.advance(&model.map);
        mission
    }));
    let positions = model.missions.iter().map(|mission| mission.position).collect::<Vec<_>>();
    let redirect_turn = model.turn as usize + 1;
    model.turn += 1;
    model.map.get_mut(destination.id).destroy();

    check_missions(&mut model, redirect_turn).unwrap();

    assert_eq!(model.missions.len(), positions.len());
    for (mission, position) in model.missions.iter().zip(positions) {
        assert_eq!(mission.origin, destination.id);
        assert_eq!(mission.destination, origin.id);
        assert_eq!(mission.position, position);
        assert!(!model.map.get(mission.destination).is_destroyed);
    }
    model.validate().unwrap();
}

#[test]
fn planet_configuration_commands_are_deterministic_and_require_completed_infrastructure() {
    let mut model = started_model(2);
    let player_id = model.players[0].id;
    let home = model.players[0].home_planet;
    let enemy_home = model.players[1].home_planet;
    let planet = model.map.get_mut(home);
    planet.army.insert(Unit::Building(Building::Terraformer), 1);
    planet.army.insert(Unit::Building(Building::CommandRelay), 1);

    let preview = preview_commands(
        &model,
        player_id,
        &[
            TurnCommand::SetTerraformerFocus {
                planet_id: home,
                resource: Some(ResourceName::Deuterium),
            },
            TurnCommand::SetCommandRelay {
                planet_id: home,
                active: false,
            },
        ],
    )
    .unwrap();
    assert_eq!(preview.map.get(home).terraformer_focus, Some(ResourceName::Deuterium));
    assert!(!preview.map.get(home).command_relay_active);

    let switched_off = preview_commands(
        &preview,
        player_id,
        &[TurnCommand::SetTerraformerFocus {
            planet_id: home,
            resource: None,
        }],
    )
    .unwrap();
    assert_eq!(switched_off.map.get(home).terraformer_focus, None);

    for command in [
        TurnCommand::SetTerraformerFocus {
            planet_id: enemy_home,
            resource: Some(ResourceName::Crystal),
        },
        TurnCommand::SetCommandRelay {
            planet_id: enemy_home,
            active: false,
        },
    ] {
        assert!(matches!(
            preview_commands(&model, player_id, &[command]),
            Err(GameError::InvalidCommand { .. })
        ));
    }
}

#[test]
fn planetary_shield_overload_applies_once_then_cools_down_for_one_turn() {
    let mut model = started_model(2);
    let player_id = model.players[0].id;
    let home = model.players[0].home_planet;
    let enemy_home = model.players[1].home_planet;
    model.map.get_mut(home).army.insert(Unit::planetary_shield(), 2);
    let base_demand = model.players[0].energy_grid(&model.map).demand;
    let overload = TurnCommand::SetPlanetaryShieldOverload {
        planet_id: home,
        active: true,
    };

    let preview = preview_commands(&model, player_id, std::slice::from_ref(&overload)).unwrap();
    assert_eq!(preview.map.get(home).shield_overload, ShieldOverloadState::Overloaded);
    assert_eq!(preview.players[0].energy_grid(&preview.map).demand, base_demand + 3);
    let cancelled = preview_commands(
        &preview,
        player_id,
        &[TurnCommand::SetPlanetaryShieldOverload {
            planet_id: home,
            active: false,
        }],
    )
    .unwrap();
    assert_eq!(cancelled.map.get(home).shield_overload, ShieldOverloadState::Ready);

    assert!(preview_commands(
        &model,
        player_id,
        &[TurnCommand::SetPlanetaryShieldOverload {
            planet_id: enemy_home,
            active: true,
        }],
    )
    .is_err());

    let submissions = model
        .players
        .iter()
        .map(|player| {
            TurnSubmission::new(
                player.id,
                model.turn,
                if player.id == player_id {
                    vec![overload.clone()]
                } else {
                    Vec::new()
                },
            )
        })
        .collect::<Vec<_>>();
    resolve_turn(&mut model, &submissions).unwrap();
    assert_eq!(model.map.get(home).shield_overload, ShieldOverloadState::Cooldown);
    assert!(preview_commands(&model, player_id, std::slice::from_ref(&overload)).is_err());

    empty_turn(&mut model);
    assert_eq!(model.map.get(home).shield_overload, ShieldOverloadState::Ready);
    assert!(preview_commands(&model, player_id, &[overload]).is_ok());
}

#[test]
fn command_relay_spoof_threshold_is_five_probes_per_level_and_can_be_disabled() {
    let mut planet = Planet::new(0, "Relay".into(), Vec2::ZERO, false, 1.0);
    for level in 1..=Building::MAX_LEVEL {
        planet.army.insert(Unit::Building(Building::CommandRelay), level);
        assert!(!planet.command_relay_diverts(0));
        assert!(planet.command_relay_diverts(level * 5));
        assert!(!planet.command_relay_diverts(level * 5 + 1));
    }
    planet.command_relay_active = false;
    assert!(!planet.command_relay_diverts(5));
}

#[test]
fn command_relay_diverts_undersized_spies_before_combat_and_returns_every_probe() {
    for (probes, relay_level, active, diverted) in
        [(5, 1, true, true), (10, 2, true, true), (11, 2, true, false), (5, 2, false, false)]
    {
        let mut model = started_model(2);
        let attacker = model.players[0].id;
        let defender = model.players[1].id;
        let origin = model.players[0].home_planet;
        let destination = model.players[1].home_planet;
        let target = model.map.get_mut(destination);
        target.army = Army::from([
            (Unit::Building(Building::CommandRelay), relay_level),
            (Unit::Defense(Defense::RocketLauncher), 5),
        ])
        .into();
        target.command_relay_active = active;
        let defending_army = target.army.clone();

        let mut mission = Mission::new_with_id(
            9_000 + probes as u64,
            model.turn as usize,
            attacker,
            model.map.get(origin),
            model.map.get(destination),
            Icon::Spy,
            Army::from([(Unit::probe(), probes)]),
            BombingRaid::None,
            false,
            false,
            None,
        );
        mission.position = model.map.get(destination).position;
        model.missions.push(mission);
        empty_turn(&mut model);

        let attacker_report = model
            .player(attacker)
            .unwrap()
            .reports
            .last()
            .expect("attacker should receive the Spy report");
        let defender_report = model
            .player(defender)
            .unwrap()
            .reports
            .last()
            .expect("defender should retain the true report");
        assert_eq!(attacker_report.planet.owned.is_none(), diverted, "probes={probes}");
        assert_eq!(attacker_report.planet.army.is_empty(), diverted, "probes={probes}");
        assert_eq!(attacker_report.surviving_defender.is_empty(), diverted, "probes={probes}");
        assert_eq!(defender_report.planet.owned, Some(defender));
        assert_eq!(
            defender_report.planet.army.amount(&Unit::Building(Building::CommandRelay)),
            relay_level
        );
        if diverted {
            assert_eq!(attacker_report.scout_probes, probes);
            assert_eq!(attacker_report.surviving_attacker.amount(&Unit::probe()), probes);
            assert!(attacker_report.combat_report.is_none());
            assert_eq!(defender_report.scout_probes, probes);
            assert_eq!(defender_report.surviving_attacker.amount(&Unit::probe()), probes);
            assert!(defender_report.combat_report.is_none());
            assert_eq!(model.map.get(destination).army, defending_army);
            assert!(model.missions.iter().any(|mission| {
                mission.owner == attacker
                    && mission.return_objective == Some(Icon::Spy)
                    && mission.army.amount(&Unit::probe()) == probes
            }));
        }
    }
}

#[test]
/// Rejects incomplete snapshots and broken cross-references without partially loading them.
fn rejects_malformed_persisted_state() {
    let persisted = PersistedGame::new(started_model(2));
    let mut unknown_field = persisted.to_json().unwrap();
    unknown_field["removed_field"] = serde_json::json!(true);
    assert!(matches!(PersistedGame::from_json(unknown_field), Err(GameError::MalformedState(_))));

    let mut missing_rules_field = persisted.to_json().unwrap();
    missing_rules_field["state"]["rules"].as_object_mut().unwrap().remove("practice_mode");
    assert!(matches!(
        PersistedGame::from_json(missing_rules_field),
        Err(GameError::MalformedState(_))
    ));

    let mut missing_home = persisted.to_json().unwrap();
    missing_home["state"]["players"][0]["home_planet"] = serde_json::json!(u64::MAX);
    assert!(matches!(PersistedGame::from_json(missing_home), Err(GameError::MalformedState(_))));

    let mut duplicate_player = persisted.to_json().unwrap();
    duplicate_player["state"]["players"][1]["id"] = serde_json::json!(1);
    assert!(matches!(
        PersistedGame::from_json(duplicate_player),
        Err(GameError::MalformedState(_))
    ));

    let mut duplicate_color = persisted.to_json().unwrap();
    duplicate_color["state"]["players"][1]["color"] =
        duplicate_color["state"]["players"][0]["color"].clone();
    assert!(matches!(PersistedGame::from_json(duplicate_color), Err(GameError::MalformedState(_))));

    let mut with_report = started_model(2);
    let origin = with_report.map.get(with_report.players[0].home_planet).clone();
    let destination = with_report.map.get(with_report.players[1].home_planet).clone();
    let mission = Mission::new_with_id(
        7,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::new(),
        BombingRaid::None,
        false,
        false,
        None,
    );
    with_report.players[0].push_report(crate::core::combat::report::MissionReport {
        id: 7,
        turn: 1,
        mission,
        planet: destination.clone(),
        scout_probes: 0,
        surviving_attacker: Army::new(),
        surviving_defender: destination.army.clone(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: destination.owned,
        destination_controlled: destination.controlled,
        combat_report: None,
        hidden: false,
    });
    let mut invalid_report = PersistedGame::new(with_report.clone()).to_json().unwrap();
    invalid_report["state"]["players"][0]["reports"][0]["mission"]["destination"] =
        serde_json::json!(u64::MAX);
    assert!(matches!(PersistedGame::from_json(invalid_report), Err(GameError::MalformedState(_))));

    let mut incomplete_report = PersistedGame::new(with_report).to_json().unwrap();
    incomplete_report["state"]["players"][0]["reports"][0]["mission"]
        .as_object_mut()
        .unwrap()
        .remove("return_objective");
    assert!(matches!(
        PersistedGame::from_json(incomplete_report),
        Err(GameError::MalformedState(_))
    ));
}

#[test]
fn rejects_nonfinite_world_geometry_and_invalid_map_bounds() {
    let model = started_model(2);
    for position in [Vec2::new(f32::NAN, 0.0), Vec2::new(0.0, f32::INFINITY)] {
        let mut invalid = model.clone();
        invalid.map.planets[0].position = position;
        assert!(matches!(invalid.validate(), Err(GameError::MalformedState(_))));
    }
    for (min, max) in [
        (Vec2::ZERO, Vec2::ZERO),
        (Vec2::ONE, Vec2::ZERO),
        (Vec2::ZERO, Vec2::splat(f32::INFINITY)),
        (Vec2::splat(-f32::MAX), Vec2::splat(f32::MAX)),
    ] {
        let mut invalid = model.clone();
        invalid.map.rect.min = min;
        invalid.map.rect.max = max;
        assert!(matches!(invalid.validate(), Err(GameError::MalformedState(_))));
    }
    let mut wire = PersistedGame::new(model).to_json().unwrap();
    wire["state"]["map"]["planets"][0]["position"] = serde_json::json!([1e100, 0.0]);
    assert!(PersistedGame::from_json(wire).is_err());
}

#[test]
fn missing_submission_errors_are_in_stable_player_order() {
    let mut model = started_model(4);
    for _ in 0..16 {
        assert_eq!(resolve_turn(&mut model, &[]), Err(GameError::MissingSubmission(1)));
    }
}

#[test]
fn exhausted_turn_counter_fails_without_changing_state() {
    let mut model = started_model(2);
    model.turn = u64::MAX;
    let before = serde_json::to_vec(&model).unwrap();
    let submissions = vec![
        TurnSubmission::new(1, model.turn, vec![]),
        TurnSubmission::new(2, model.turn, vec![]),
    ];
    assert!(matches!(resolve_turn(&mut model, &submissions), Err(GameError::MalformedState(_))));
    assert_eq!(serde_json::to_vec(&model).unwrap(), before);
}

#[test]
fn laboratory_preview_uses_exact_large_balance_output() {
    let mut model = started_model(2);
    let moon = model.map.moons()[0].id;
    model.map.get_mut(moon).controlled = Some(1);
    model.map.get_mut(moon).army.insert(Unit::Building(Building::Laboratory), 5);
    let amount = 16_777_219;
    model.players[0].resources = Resources::new(amount, 0, 0);
    let preview = preview_commands(
        &model,
        1,
        &[TurnCommand::ConvertResources {
            planet_id: moon,
            from: ResourceName::Metal,
            to: ResourceName::Crystal,
            amount,
        }],
    )
    .unwrap();
    assert_eq!(preview.players[0].resources, Resources::new(0, amount, 0));
    assert_eq!(model.players[0].resources, Resources::new(amount, 0, 0));
}

#[test]
/// Turn submissions require the complete current wire shape.
fn rejects_incomplete_or_extended_turn_submissions() {
    let submission = TurnSubmission::new(1, 1, Vec::new());
    let mut missing = serde_json::to_value(&submission).unwrap();
    missing.as_object_mut().unwrap().remove("generation");
    assert!(serde_json::from_value::<TurnSubmission>(missing).is_err());

    let mut extended = serde_json::to_value(submission).unwrap();
    extended["removed_field"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TurnSubmission>(extended).is_err());
}

#[test]
/// Player colors are mandatory in every persisted snapshot.
fn rejects_players_without_colors() {
    let mut json = PersistedGame::new(started_model(4)).to_json().unwrap();
    for player in json["state"]["players"].as_array_mut().unwrap() {
        player.as_object_mut().unwrap().remove("color");
    }

    assert!(matches!(PersistedGame::from_json(json), Err(GameError::MalformedState(_))));
}

#[test]
/// Losing the final opposing home world completes the match with one stable winner.
fn resolution_completes_game() {
    let mut model = started_model(2);
    let defeated_home = model.players[1].home_planet;
    let planet = model.map.get_mut(defeated_home);
    planet.owned = Some(1);
    planet.controlled = Some(1);
    model.players[1].spectator = true;
    let submissions = model
        .players
        .iter()
        .filter(|player| !player.spectator)
        .map(|player| TurnSubmission::new(player.id, model.turn, Vec::new()))
        .collect::<Vec<_>>();

    let result = resolve_turn(&mut model, &submissions).unwrap();
    assert!(result.finished);
    assert_eq!(result.winner, Some(1));
    assert_eq!(model.status, MatchStatus::Finished);
}

fn give_territory(model: &mut GameModel, owner: PlayerId, count: usize) {
    let homes = model.players.iter().map(|player| player.home_planet).collect::<HashSet<_>>();
    let home = model.player(owner).unwrap().home_planet;
    model.map.get_mut(home).controlled = Some(owner);
    let ids = model
        .map
        .planets
        .iter()
        .filter(|planet| !planet.is_moon() && !planet.is_destroyed && !homes.contains(&planet.id))
        .map(|planet| planet.id)
        .take(count - 1)
        .collect::<Vec<_>>();
    assert_eq!(ids.len(), count - 1);
    for id in ids {
        model.map.get_mut(id).controlled = Some(owner);
    }
}

fn empty_turn(model: &mut GameModel) -> TurnResult {
    let submissions = model
        .players
        .iter()
        .filter(|player| !player.spectator)
        .map(|player| TurnSubmission::new(player.id, model.turn, vec![]))
        .collect::<Vec<_>>();
    resolve_turn(model, &submissions).unwrap()
}

#[test]
fn territory_target_uses_surviving_planets_and_starting_players_and_rounds_up() {
    for (players, expected, expected_after_destruction) in [(2, 15, 8), (3, 20, 10), (4, 25, 13)] {
        let mut model = started_model(players);
        assert_eq!(model.planets_to_win(), expected);
        model.players[0].spectator = true;
        assert_eq!(model.planets_to_win(), expected);

        let homes = model.players.iter().map(|player| player.home_planet).collect::<HashSet<_>>();
        let destroyed = model
            .map
            .planets
            .iter()
            .filter(|planet| !planet.is_moon() && !homes.contains(&planet.id))
            .map(|planet| planet.id)
            .take(usize::from(players) * 5)
            .collect::<Vec<_>>();
        assert_eq!(destroyed.len(), usize::from(players) * 5);
        for id in destroyed {
            model.map.get_mut(id).destroy();
        }
        assert_eq!(model.planets_to_win(), expected_after_destruction);
    }
    let mut model = started_model(4);
    model.map.planets =
        model.map.planets.into_iter().filter(|planet| !planet.is_moon()).take(30).collect();
    assert_eq!(model.planets_to_win(), 19);
}

#[test]
fn destroying_half_the_planets_halves_the_territorial_threshold() {
    let mut model = started_model(2);
    let homes = model.players.iter().map(|player| player.home_planet).collect::<HashSet<_>>();
    let destroyed = model
        .map
        .planets
        .iter()
        .filter(|planet| !planet.is_moon() && !homes.contains(&planet.id))
        .map(|planet| planet.id)
        .take(10)
        .collect::<Vec<_>>();
    assert_eq!(destroyed.len(), 10);
    for id in destroyed {
        model.map.get_mut(id).destroy();
    }

    assert_eq!(model.planets_to_win(), 8);
    give_territory(&mut model, 2, 7);
    assert_eq!(model.territorial_winner(), None);
    give_territory(&mut model, 2, 8);
    assert_eq!(model.territorial_winner(), Some(2));
}

#[test]
fn senate_levels_scale_with_galaxy_size_and_the_ownership_setting() {
    let mut model = GameModel::new(
        [93; 32],
        GameRules {
            planets_per_player: 20,
            player_count: 4,
            ..GameRules::default()
        },
    )
    .unwrap();
    let player_id = model.players[0].id;
    let home = model.players[0].home_planet;
    let total = model.map.planets().len();
    assert_eq!(total, 80);
    model.map.get_mut(home).army.insert(Unit::Building(Building::Senate), 3);

    for (percent, senate_levels, expected_limit) in [(25, 3, 23), (35, 2, 30), (50, 1, 41)] {
        model.rules.colonizable_percent = percent;
        assert_eq!(Player::senate_level_limit(&model.map, percent), senate_levels);
        assert_eq!(colony_limit(&model, player_id).unwrap(), expected_limit);
    }
}

#[test]
fn smaller_galaxies_reduce_the_senate_level_cap() {
    for (players, planets_per_player, expected) in [(2, 5, 1), (2, 20, 2), (4, 20, 3)] {
        let model = GameModel::new(
            [players; 32],
            GameRules {
                planets_per_player,
                player_count: players,
                ..GameRules::default()
            },
        )
        .unwrap();
        assert_eq!(Player::senate_level_limit(&model.map, 25), expected);
    }
}

#[test]
fn only_offered_colonization_percentages_are_valid() {
    for percent in [25, 35, 50] {
        assert!(GameRules {
            colonizable_percent: percent,
            ..GameRules::default()
        }
        .validate()
        .is_ok());
    }
    for percent in [1, 34, 100] {
        assert!(GameRules {
            colonizable_percent: percent,
            ..GameRules::default()
        }
        .validate()
        .is_err());
    }
}

#[test]
fn territory_wins_on_threshold_without_eliminating_opponents_and_survives_save() {
    for players in 2..=4 {
        let mut model = started_model(players);
        let owner = u64::from(players); // Winner must not default to the first surviving player.
        let target = model.planets_to_win();
        give_territory(&mut model, owner, target - 1);
        assert!(!empty_turn(&mut model).finished);
        give_territory(&mut model, owner, target);
        let result = empty_turn(&mut model);
        assert_eq!(result.winner, Some(owner));
        assert!(result.finished);
        assert!(model.players.iter().all(|player| player.spectator));
        assert!(model.players.iter().all(|player| player.owns(model.map.get(player.home_planet))));
        let loaded =
            PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap()).unwrap();
        assert_eq!(loaded.state.winner(), Some(owner));
    }
}

#[test]
fn moons_destroyed_worlds_and_eliminated_empires_cannot_supply_victory() {
    let mut model = started_model(2);
    let target = model.planets_to_win();
    give_territory(&mut model, 2, target - 1);
    for moon in model.map.planets.iter_mut().filter(|planet| planet.is_moon()) {
        moon.controlled = Some(2);
    }
    assert_eq!(model.territorial_winner(), None);
    give_territory(&mut model, 2, target);
    let id = model
        .map
        .planets
        .iter()
        .find(|p| p.controlled == Some(2) && p.owned.is_none() && !p.is_moon())
        .unwrap()
        .id;
    model.map.get_mut(id).is_destroyed = true;
    assert_eq!(model.territorial_winner(), None);
    model.map.get_mut(id).is_destroyed = false;
    let home = model.players[1].home_planet;
    model.map.get_mut(home).owned = None;
    model.players[1].spectator = true;
    assert_eq!(empty_turn(&mut model).winner, Some(1));
}

#[test]
fn accelerated_arrival_resolves_on_the_displayed_turn() {
    let mut model = started_model(2);
    let origin = model.players[0].home_planet;
    let destination =
        model.map.planets.iter().find(|p| !p.is_moon() && p.controlled.is_none()).unwrap().id;
    model.map.get_mut(origin).position = Vec2::ZERO;
    let unit = Unit::Ship(Ship::LightFighter);
    model.map.get_mut(destination).position = Vec2::X * Planet::SIZE * (unit.speed() * 8.0 + 1.4);
    model.missions.push(Mission::new_with_id(
        1,
        1,
        1,
        model.map.get(origin),
        model.map.get(destination),
        Icon::Attack,
        Army::from([(unit, 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    ));
    for remaining in (1..=4).rev() {
        assert_eq!(model.missions[0].duration(&model.map), remaining);
        empty_turn(&mut model);
        if remaining > 1 {
            assert_eq!(model.map.get(destination).controlled, None);
            model = PersistedGame::from_json(PersistedGame::new(model).to_json().unwrap())
                .unwrap()
                .state;
        }
    }
    assert!(model.missions.is_empty());
    assert_eq!(model.map.get(destination).controlled, Some(1));
}

#[test]
fn territorial_victory_waits_for_all_arriving_attacks() {
    for take_home in [false, true] {
        let mut model = started_model(2);
        let target = model.planets_to_win();
        give_territory(&mut model, 2, target);
        assert_eq!(model.territorial_winner(), Some(2));
        let destination = if take_home {
            model.players[1].home_planet
        } else {
            model
                .map
                .planets
                .iter()
                .find(|p| !p.is_moon() && p.controlled == Some(2) && p.owned.is_none())
                .unwrap()
                .id
        };
        model.map.get_mut(destination).army.clear();
        let origin = model.players[0].home_planet;
        let mut mission = Mission::new_with_id(
            1,
            1,
            1,
            model.map.get(origin),
            model.map.get(destination),
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
            BombingRaid::None,
            false,
            false,
            None,
        );
        mission.position = model.map.get(destination).position;
        model.missions.push(mission);
        let result = empty_turn(&mut model);
        assert_eq!(result.winner, take_home.then_some(1));
        assert_eq!(result.finished, take_home);
    }
}

#[test]
fn orbital_railgun_range_deuterium_cost_and_once_per_turn_limit_are_enforced() {
    let mut model = started_model(2);
    let origin = model.players[0].home_planet;
    let target = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.owned.is_none())
        .unwrap()
        .id;
    model.players[0].resources = Resources::new(1_000, 1_000, 1_000);
    model.map.get_mut(origin).army.insert(Unit::Building(Building::OrbitalRailgun), 1);
    let origin_position = model.map.get(origin).position;
    model.map.get_mut(target).position = origin_position + Vec2::X * Planet::SIZE * 3.0;
    let fire = TurnCommand::FireOrbitalRailguns {
        target,
    };
    assert!(preview_commands(&model, 1, std::slice::from_ref(&fire)).is_err());

    model.map.get_mut(origin).army.insert(Unit::Building(Building::OrbitalRailgun), 2);
    model.players[0].resources.deuterium = 999;
    assert!(preview_commands(&model, 1, std::slice::from_ref(&fire)).is_err());
    model.players[0].resources.deuterium = 1_000;
    assert!(
        model.players[0].energy_grid(&model.map).balance()
            < ORBITAL_RAILGUN_FIRE_ENERGY_COST as i128,
        "the test must exercise firing through an Energy shortage"
    );
    let preview = preview_commands(&model, 1, std::slice::from_ref(&fire)).unwrap();
    assert_eq!(orbital_railgun_fire_cost(1), Resources::new(0, 0, 1_000));
    assert_eq!(ORBITAL_RAILGUN_FIRE_ENERGY_COST, 5);
    assert_eq!(preview.players[0].resources, Resources::new(1_000, 1_000, 0));
    assert!(preview_commands(&model, 1, &[fire.clone(), fire]).is_err());
    assert!(preview_commands(
        &model,
        2,
        &[TurnCommand::FireOrbitalRailguns {
            target,
        }]
    )
    .is_err());

    let firing_income = model.players[0]
        .energy_grid(&model.map)
        .with_action_demand(orbital_railgun_fire_energy_cost(1))
        .scale_resources(model.players[0].raw_resource_production(&model.map));
    let submissions = vec![
        TurnSubmission::new(
            1,
            model.turn,
            vec![TurnCommand::FireOrbitalRailguns {
                target,
            }],
        ),
        TurnSubmission::new(2, model.turn, Vec::new()),
    ];
    resolve_turn(&mut model, &submissions).unwrap();
    assert_eq!(model.players[0].resources, Resources::new(1_000, 1_000, 0) + firing_income);
}

#[test]
fn orbital_railguns_can_target_moons() {
    let mut model = started_model(2);
    let origin = model.players[0].home_planet;
    let target = model.map.moons()[0].id;
    let origin_position = model.map.get(origin).position;
    let moon = model.map.get_mut(target);
    moon.owned = None;
    moon.controlled = None;
    moon.position = origin_position + Vec2::X * Planet::SIZE;
    model.map.get_mut(origin).army.insert(Unit::Building(Building::OrbitalRailgun), 1);
    model.map.get_mut(origin).army.insert(Unit::Building(Building::Reactor), Building::MAX_LEVEL);
    model
        .map
        .get_mut(origin)
        .army
        .insert(Unit::Building(Building::TidalGenerator), Building::MAX_LEVEL);
    model.players[0].resources = Resources::new(1_000, 1_000, 1_000);

    let preview = preview_commands(
        &model,
        1,
        &[TurnCommand::FireOrbitalRailguns {
            target,
        }],
    )
    .unwrap();

    assert_eq!(preview.orbital_strikes[0].target, target);

    let submissions = vec![
        TurnSubmission::new(
            1,
            model.turn,
            vec![TurnCommand::FireOrbitalRailguns {
                target,
            }],
        ),
        TurnSubmission::new(2, model.turn, Vec::new()),
    ];
    resolve_turn(&mut model, &submissions).unwrap();
    assert_eq!(model.orbital_strikes[0].target, target);
    let saved = PersistedGame::new(model).to_json().unwrap();
    assert!(PersistedGame::from_json(saved).is_ok());
}

#[test]
fn orbital_railguns_combine_by_target_and_persist_the_public_outcome() {
    let mut model = started_model(2);
    let worlds = model
        .map
        .planets
        .iter()
        .filter(|planet| !planet.is_moon() && planet.owned.is_none())
        .map(|planet| planet.id)
        .take(2)
        .collect::<Vec<_>>();
    let first = model.players[0].home_planet;
    let second = worlds[0];
    let target = worlds[1];
    model.map.get_mut(second).colonize(1);
    model.players[0].record_world_acquisition(second);
    let target_position = model.map.get(target).position;
    for (index, origin) in [first, second].into_iter().enumerate() {
        let planet = model.map.get_mut(origin);
        planet.position = target_position + Vec2::X * Planet::SIZE * (index as f32 + 1.0);
        planet.army.insert(Unit::Building(Building::OrbitalRailgun), 1);
    }
    model.players[0].resources = Resources::new(5_000, 5_000, 5_000);
    model.map.get_mut(first).army.insert(Unit::Building(Building::Reactor), Building::MAX_LEVEL);
    model
        .map
        .get_mut(first)
        .army
        .insert(Unit::Building(Building::TidalGenerator), Building::MAX_LEVEL);
    let preview = preview_commands(
        &model,
        1,
        &[TurnCommand::FireOrbitalRailguns {
            target,
        }],
    )
    .unwrap();
    assert_eq!(preview.orbital_strikes[0].origins, vec![first.min(second), first.max(second)]);
    assert_eq!(orbital_railgun_fire_cost(2), Resources::new(0, 0, 2_000));
    assert_eq!(orbital_railgun_fire_energy_cost(2), 10);
    assert_eq!(preview.players[0].resources, Resources::new(5_000, 5_000, 3_000));
    let firing_income = model.players[0]
        .energy_grid(&model.map)
        .with_action_demand(orbital_railgun_fire_energy_cost(2))
        .scale_resources(model.players[0].raw_resource_production(&model.map));
    let submissions = vec![
        TurnSubmission::new(
            1,
            model.turn,
            vec![TurnCommand::FireOrbitalRailguns {
                target,
            }],
        ),
        TurnSubmission::new(2, model.turn, Vec::new()),
    ];

    resolve_turn(&mut model, &submissions).unwrap();
    assert_eq!(model.players[0].resources, Resources::new(5_000, 5_000, 3_000) + firing_income);
    assert_eq!(model.orbital_strikes.len(), 1);
    let strike = &model.orbital_strikes[0];
    assert_eq!(strike.turn, model.turn);
    assert_eq!(strike.origins, vec![first.min(second), first.max(second)]);
    assert_eq!(strike.target, target);
    assert_eq!(
        strike.chance_basis_points,
        orbital_railgun_destruction_basis_points(&model.map, &[first, second], target)
    );
    assert_eq!(model.map.get(target).is_destroyed, strike.destroyed);
    let saved = PersistedGame::new(model).to_json().unwrap();
    assert!(PersistedGame::from_json(saved).is_ok());
}

#[test]
fn orbital_railgun_chance_combines_firing_levels_with_the_war_sun_size_curve() {
    let mut model = started_model(2);
    let first = model.players[0].home_planet;
    let second = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.owned.is_none())
        .unwrap()
        .id;
    let target = model.players[1].home_planet;
    model.map.get_mut(first).army.insert(Unit::Building(Building::OrbitalRailgun), 5);
    model.map.get_mut(second).army.insert(Unit::Building(Building::OrbitalRailgun), 2);

    for (diameter, size_bonus) in [(1_500, 800), (10_000, 300), (120_000, 0)] {
        model.map.get_mut(target).diameter = diameter;
        assert_eq!(
            orbital_railgun_destruction_basis_points(&model.map, &[first], target),
            2_500 + size_bonus
        );
        assert_eq!(
            orbital_railgun_destruction_basis_points(&model.map, &[first, second], target),
            3_500 + size_bonus
        );
    }
}

#[test]
fn planetary_shield_levels_reduce_railgun_chance_twice_as_much_when_overloaded() {
    let mut model = started_model(2);
    let origin = model.players[0].home_planet;
    let target = model.players[1].home_planet;
    model.map.get_mut(origin).army.insert(Unit::Building(Building::OrbitalRailgun), 1);
    let target_planet = model.map.get_mut(target);
    target_planet.diameter = 10_000;
    target_planet.army.insert(Unit::Building(Building::PlanetaryShield), 2);

    assert_eq!(orbital_railgun_destruction_basis_points(&model.map, &[origin], target), 600);
    model.map.get_mut(target).shield_overload = ShieldOverloadState::Overloaded;
    assert_eq!(orbital_railgun_destruction_basis_points(&model.map, &[origin], target), 400);

    model
        .map
        .get_mut(target)
        .army
        .insert(Unit::Building(Building::PlanetaryShield), Building::MAX_LEVEL);
    assert_eq!(orbital_railgun_destruction_basis_points(&model.map, &[origin], target), 0);
}

proptest::proptest! {
    #![proptest_config(proptest::test_runner::Config::with_cases(16))]

    #[test]
    /// Round-trips arbitrary compact seeds without changing valid state.
    fn serialized_state_round_trips(seed in proptest::prelude::any::<u64>()) {
        let mut bytes = [0_u8; 32];
        bytes[..8].copy_from_slice(&seed.to_le_bytes());
        let model = GameModel::new(bytes, GameRules::default()).unwrap();
        let persisted = PersistedGame::new(model);
        let json = persisted.to_json().unwrap();
        let loaded = PersistedGame::from_json(json).unwrap();
        proptest::prop_assert_eq!(
            serde_json::to_vec(&persisted).unwrap(),
            serde_json::to_vec(&loaded).unwrap()
        );
    }

    #[test]
    /// Arbitrary valid games resolve deterministically and retain ownership/unit invariants.
    fn arbitrary_empty_turns_preserve_invariants(
        seed in proptest::array::uniform32(proptest::prelude::any::<u8>()),
        player_count in 2_u8..=4,
    ) {
        let rules = GameRules {
            player_count,
            ..GameRules::default()
        };
        let mut left = GameModel::new(seed, rules).unwrap();
        left.start().unwrap();
        let mut right = left.clone();
        let submissions = left
            .players
            .iter()
            .map(|player| TurnSubmission::new(player.id, left.turn, Vec::new()))
            .collect::<Vec<_>>();

        let left_result = resolve_turn(&mut left, &submissions).unwrap();
        let right_result = resolve_turn(&mut right, &submissions).unwrap();
        proptest::prop_assert_eq!(left_result, right_result);
        proptest::prop_assert_eq!(
            serde_json::to_vec(&left).unwrap(),
            serde_json::to_vec(&right).unwrap()
        );
        left.validate().unwrap();

        let player_ids = left.players.iter().map(|player| player.id).collect::<HashSet<_>>();
        proptest::prop_assert_eq!(player_ids.len(), usize::from(player_count));
        for planet in &left.map.planets {
            for owner in [planet.owned, planet.controlled].into_iter().flatten() {
                proptest::prop_assert!(player_ids.contains(&owner));
            }
            for count in planet.army.values() {
                proptest::prop_assert!(
                    serde_json::to_value(count).unwrap().as_u64().is_some()
                );
            }
        }
    }
}
