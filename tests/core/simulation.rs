use super::*;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;

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
                if unit.is_building() {
                    Building::MAX_LEVEL
                } else {
                    model.map.get(home).army.amount(&unit) + 3
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
        let expected_resources = preview.players[0].resources
            + preview.players[0].resource_production(&preview.map.planets);
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
/// Keeps normal multiplayer snapshots backward-compatible and rejects mixed practice rules.
fn practice_rules_are_explicit_and_json_compatible() {
    let json = serde_json::to_value(GameRules::default()).unwrap();
    assert!(json.get("practice_mode").is_none());
    let loaded: GameRules = serde_json::from_value(json).unwrap();
    assert!(!loaded.practice_mode);
    assert!(matches!(
        GameModel::new(
            [2; 32],
            GameRules {
                practice_mode: true,
                ..GameRules::default()
            }
        ),
        Err(GameError::InvalidPlayerCount(2))
    ));
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
        target.army = Army::from([(fighter, 3)]);

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
    model.map.get_mut(destination.id).army = Army::new();

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
/// Rejects unsupported envelopes and broken cross-references without partially loading them.
fn rejects_malformed_persisted_state() {
    let persisted = PersistedGame::new(started_model(2));
    let mut wrong_schema = persisted.to_json().unwrap();
    wrong_schema["schema_version"] = serde_json::json!(999);
    assert!(matches!(
        PersistedGame::from_json(wrong_schema),
        Err(GameError::UnsupportedSchema(999))
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
    let mut invalid_report = PersistedGame::new(with_report).to_json().unwrap();
    invalid_report["state"]["players"][0]["reports"][0]["mission"]["destination"] =
        serde_json::json!(u64::MAX);
    assert!(matches!(PersistedGame::from_json(invalid_report), Err(GameError::MalformedState(_))));
}

#[test]
/// Snapshots created before lobby colors receive distinct deterministic slot colors.
fn legacy_players_without_colors_remain_compatible() {
    let mut json = PersistedGame::new(started_model(4)).to_json().unwrap();
    for player in json["state"]["players"].as_array_mut().unwrap() {
        player.as_object_mut().unwrap().remove("color");
    }

    let loaded = PersistedGame::from_json(json).unwrap();
    for player in &loaded.state.players {
        assert_eq!(player.color(), crate::core::player::PlayerColor::for_player(player.id));
    }
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
