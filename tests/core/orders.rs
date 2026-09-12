use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::map::icon::Icon;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::orders::{conversion_output, purchase_limit, validate_mission, OrderError};
use crate::core::random::DeterministicRngState;
use crate::core::simulation::{resolve_turn, GameModel, GameRules, TurnCommand, TurnSubmission};
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Price, Unit};

#[test]
fn construction_prices_use_multiples_of_ten() {
    for unit in Unit::all().into_iter().flatten() {
        let price = unit.price();
        for amount in [price.metal, price.crystal, price.deuterium] {
            assert_eq!(amount % 10, 0, "{unit:?} has a non-round resource cost");
        }
    }
}

#[test]
fn senate_has_a_capstone_building_price() {
    assert_eq!(
        Unit::Building(Building::Senate).price(),
        crate::core::resources::Resources::new(1_000, 750, 500)
    );
}

#[test]
fn orbital_railgun_is_a_five_level_premium_over_the_jump_gate() {
    let jump_gate = Unit::Building(Building::JumpGate).price();
    let railgun = Unit::Building(Building::OrbitalRailgun).price();

    assert_eq!(railgun, crate::core::resources::Resources::new(750, 500, 750));
    assert!(jump_gate < railgun);
    assert_eq!(
        railgun * Building::MAX_LEVEL,
        crate::core::resources::Resources::new(3_750, 2_500, 3_750)
    );
}

#[test]
fn five_colonial_administration_levels_cost_less_than_one_space_dock() {
    let administration = Unit::Building(Building::ColonialAdministration).price();
    let full_investment = administration * Building::MAX_LEVEL;
    let space_dock = Unit::space_dock().price();

    assert_eq!(administration, crate::core::resources::Resources::new(200, 100, 50));
    assert_eq!(full_investment, crate::core::resources::Resources::new(1_000, 500, 250));
    assert!(full_investment < space_dock);
}

#[test]
fn senate_purchase_respects_the_match_level_limit() {
    let mut game = game();
    let home = game.players[0].home_planet;
    game.players[0].resources = crate::core::resources::Resources::new(10_000, 10_000, 10_000);
    let player = &game.players[0];
    let planet = game.map.get_mut(home);
    let senate = Unit::Building(Building::Senate);

    planet.army.insert(senate, 1);
    assert_eq!(purchase_limit(player, planet, senate, 2), Ok(1));
    assert_eq!(purchase_limit(player, planet, senate, 1), Err(OrderError::Building));
    planet.army.insert(senate, 2);
    assert_eq!(purchase_limit(player, planet, senate, 2), Err(OrderError::Building));
}

#[test]
fn tidal_generators_consume_lunar_fields() {
    let mut game = game();
    let player_id = game.players[0].id;
    let moon_id = game.map.moons()[0].id;
    let moon = game.map.get_mut(moon_id);
    moon.controlled = Some(player_id);
    moon.army.clear();

    assert_eq!(
        purchase_limit(
            &game.players[0],
            game.map.get(moon_id),
            Unit::Building(Building::TidalGenerator),
            Building::MAX_LEVEL,
        ),
        Err(OrderError::Fields)
    );

    let moon = game.map.get_mut(moon_id);
    moon.army.insert(Unit::Building(Building::LunarBase), 1);
    assert_eq!(
        purchase_limit(
            &game.players[0],
            game.map.get(moon_id),
            Unit::Building(Building::TidalGenerator),
            Building::MAX_LEVEL,
        ),
        Ok(1)
    );

    game.map.get_mut(moon_id).army.insert(Unit::Building(Building::TidalGenerator), 1);
    assert_eq!(game.map.get(moon_id).fields_consumed(), 1);
    assert_eq!(
        purchase_limit(
            &game.players[0],
            game.map.get(moon_id),
            Unit::Building(Building::Laboratory),
            Building::MAX_LEVEL,
        ),
        Err(OrderError::Fields)
    );
}

#[test]
fn planet_buildings_use_progressive_spy_intelligence_tiers() {
    for (building, expected) in [
        (Building::MetalMine, 1),
        (Building::CrystalMine, 1),
        (Building::DeuteriumSynthesizer, 1),
        (Building::Reactor, 2),
        (Building::Terraformer, 2),
        (Building::Shipyard, 3),
        (Building::Factory, 3),
        (Building::MissileSilo, 3),
        (Building::PlanetaryShield, 4),
        (Building::Senate, 5),
    ] {
        assert_eq!(Unit::Building(building).production(), expected, "{building:?}");
    }
}

#[test]
fn missiles_reach_targets_between_three_and_four_au_in_one_turn_without_fuel() {
    let mut game = game();
    let origin_id = game.players[0].home_planet;
    let destination_id = game.players[1].home_planet;
    game.map.get_mut(origin_id).position = bevy::math::Vec2::ZERO;
    // Arrival uses the distance to the planet's edge (center distance minus 0.7 AU).
    game.map.get_mut(destination_id).position =
        bevy::math::Vec2::X * crate::core::map::planet::Planet::SIZE * 4.5;
    let mut mission = Mission::new_with_id(
        1,
        1,
        1,
        game.map.get(origin_id),
        game.map.get(destination_id),
        Icon::MissileStrike,
        Army::from([(Unit::interplanetary_missile(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    assert!(mission.distance(&game.map) > 3.0);
    assert!(mission.distance(&game.map) < 4.0);
    assert_eq!(mission.duration(&game.map), 1);
    assert_eq!(mission.fuel_consumption(&game.map), 0);
    mission.advance(&game.map);
    assert_eq!(mission.distance(&game.map), 0.0);
}

fn game() -> GameModel {
    let mut game = GameModel::new([7; 32], GameRules::default()).unwrap();
    game.start().unwrap();
    game
}

#[test]
fn laboratory_conversion_is_exact_and_bounded() {
    for (level, expected) in [(0, 0), (1, 33), (2, 40), (3, 50), (4, 66), (5, 100)] {
        assert_eq!(conversion_output(100, level), expected);
    }
    // f32 rounds this odd amount up, even at the lossless level-five exchange rate.
    let amount = 16_777_219;
    assert_eq!(conversion_output(amount, 5), amount);
    assert_eq!(conversion_output(amount, 3), amount / 2);
    assert_eq!(conversion_output(usize::MAX, 5), usize::MAX);
    assert_eq!(conversion_output(usize::MAX, usize::MAX), usize::MAX);
    assert_eq!(conversion_output(usize::MAX, 1), usize::MAX / 3);
}

fn submit(game: &mut GameModel, commands: Vec<TurnCommand>) -> Result<(), String> {
    let turn = game.turn;
    resolve_turn(
        game,
        &[TurnSubmission::new(1, turn, commands), TurnSubmission::new(2, turn, vec![])],
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}

#[test]
fn purchase_limit_includes_queued_missiles_of_both_types() {
    let mut game = game();
    let home = game.players[0].home_planet;
    let planet = game.map.get_mut(home);
    planet.army.insert(Unit::Building(Building::MissileSilo), 1);
    planet.army.insert(Unit::Building(Building::Factory), 5);
    planet.army.insert(Unit::antiballistic_missile(), 7);
    planet.buy.push(Unit::interplanetary_missile());
    assert_eq!(
        purchase_limit(
            &game.players[0],
            planet,
            Unit::antiballistic_missile(),
            Building::MAX_LEVEL,
        )
        .unwrap(),
        2
    );
    planet.buy.extend([Unit::antiballistic_missile(); 2]);
    assert!(purchase_limit(
        &game.players[0],
        planet,
        Unit::interplanetary_missile(),
        Building::MAX_LEVEL,
    )
    .is_err());
}

#[test]
fn stationed_space_dock_adds_five_ship_production_slots() {
    let mut game = game();
    let home = game.players[0].home_planet;
    game.players[0].resources = crate::core::resources::Resources::new(10_000, 10_000, 10_000);
    let planet = game.map.get_mut(home);
    planet.army.insert(Unit::Building(Building::Shipyard), 1);

    assert_eq!(planet.max_fleet_production(), 5);
    planet.buy.push(Unit::space_dock());
    assert_eq!(planet.max_fleet_production(), 5, "queued docks are not operational");
    planet.buy.clear();
    planet.army.insert(Unit::space_dock(), 1);
    assert_eq!(planet.max_fleet_production(), 10);

    planet.buy.extend([Unit::Ship(Ship::LightFighter); 5]);
    assert_eq!(
        purchase_limit(
            &game.players[0],
            planet,
            Unit::Ship(Ship::LightFighter),
            Building::MAX_LEVEL,
        )
        .unwrap(),
        5
    );
}

#[test]
fn terraformer_specializes_resources_without_changing_unit_capacity() {
    let mut game = game();
    let home = game.players[0].home_planet;
    game.players[0].resources = crate::core::resources::Resources::new(10_000, 10_000, 10_000);
    let player = &game.players[0];
    let planet = game.map.get_mut(home);
    planet.army.insert(Unit::Building(Building::Shipyard), 1);
    planet.army.insert(Unit::Building(Building::Factory), 1);
    planet.army.insert(Unit::Building(Building::MetalMine), 1);
    planet.army.insert(Unit::Building(Building::CrystalMine), 1);
    planet.army.insert(Unit::Building(Building::DeuteriumSynthesizer), 1);
    planet.army.insert(Unit::Building(Building::Terraformer), 3);
    planet.resources = crate::core::resources::Resources::new(1_000, 1_000, 1_000);
    planet.terraformer_focus = Some(crate::core::resources::ResourceName::Crystal);

    assert_eq!(
        planet.resource_production(),
        crate::core::resources::Resources::new(700, 1_300, 700)
    );
    assert_eq!(planet.max_fleet_production(), 5);
    assert_eq!(planet.max_battery_production(), 5);
    assert_eq!(
        purchase_limit(player, planet, Unit::Ship(Ship::Cruiser), Building::MAX_LEVEL),
        Err(crate::core::orders::OrderError::Production),
        "Terraforming must not replace Shipyard unlock levels"
    );

    planet.army.remove(&Unit::Building(Building::Terraformer));
    planet.buy.push(Unit::Building(Building::Terraformer));
    assert_eq!(
        planet.resource_production(),
        crate::core::resources::Resources::new(1_000, 1_000, 1_000),
        "queued Terraformers are not operational"
    );

    planet.army.insert(Unit::Building(Building::Terraformer), 3);
    planet.buy.clear();
    planet.terraformer_focus = None;
    assert_eq!(
        planet.resource_production(),
        crate::core::resources::Resources::new(1_000, 1_000, 1_000),
        "a Terraformer with no focus is switched off"
    );
}

#[test]
fn planet_building_roster_includes_both_home_and_colony_capstones() {
    let planet_buildings =
        Unit::buildings().into_iter().filter(|unit| unit.valid_on(false)).collect::<Vec<_>>();
    assert_eq!(
        planet_buildings,
        [
            Building::MetalMine,
            Building::CrystalMine,
            Building::DeuteriumSynthesizer,
            Building::Reactor,
            Building::Terraformer,
            Building::Shipyard,
            Building::Factory,
            Building::MissileSilo,
            Building::PlanetaryShield,
            Building::Senate,
            Building::ColonialAdministration,
        ]
        .into_iter()
        .map(Unit::Building)
        .collect::<Vec<_>>()
    );
}

#[test]
fn orbitals_require_their_production_level_in_completed_shipyards() {
    let mut game = game();
    let home = game.players[0].home_planet;
    game.players[0].resources = crate::core::resources::Resources::new(10_000, 10_000, 10_000);
    let player = &game.players[0];
    let planet = game.map.get_mut(home);
    planet.army.remove(&Unit::Building(Building::Shipyard));
    planet.army.remove(&Unit::Building(Building::Factory));

    assert_eq!(
        Unit::orbitals(),
        vec![
            Unit::Building(Building::SolarSatellite),
            Unit::Building(Building::Recycler),
            Unit::Building(Building::CommandRelay),
            Unit::Building(Building::SensorPhalanx),
            Unit::Building(Building::JumpGate),
            Unit::Building(Building::OrbitalRailgun),
            Unit::space_dock(),
        ]
    );
    for orbital in Unit::orbitals() {
        let required = orbital.production();
        planet.army.insert(Unit::Building(Building::Shipyard), required - 1);
        assert_eq!(
            purchase_limit(player, planet, orbital, Building::MAX_LEVEL),
            Err(OrderError::Production),
            "{orbital:?} should require Shipyard level {required}"
        );
        planet.army.insert(Unit::Building(Building::Shipyard), required);
        assert_eq!(purchase_limit(player, planet, orbital, Building::MAX_LEVEL), Ok(1));
    }
}

#[test]
fn command_relay_no_longer_changes_jump_gate_capacity() {
    let mut game = game();
    let home = game.players[0].home_planet;
    let planet = game.map.get_mut(home);
    let relay = Unit::Building(Building::CommandRelay);
    let gate = Unit::Building(Building::JumpGate);

    planet.army.insert(relay, 3);
    assert_eq!(planet.max_jump_capacity(), 0, "a relay cannot replace a Jump Gate");

    planet.army.insert(gate, 2);
    assert_eq!(planet.max_jump_capacity(), 10);

    planet.buy.push(relay);
    assert_eq!(planet.max_jump_capacity(), 10, "relays do not increase gate capacity");
}

#[test]
fn protect_can_jump_to_the_receivers_gate_but_not_back_from_it() {
    let mut game = game();
    let protector = game.players[0].id;
    let receiver = game.players[1].id;
    let home = game.players[0].home_planet;
    let receiving_planet = game.players[1].home_planet;
    let gate = Unit::Building(Building::JumpGate);
    let ship = Unit::Ship(Ship::LightFighter);

    game.map.get_mut(home).army.insert(gate, 1);
    game.map.get_mut(home).army.insert(ship, 1);
    game.map.get_mut(receiving_planet).army.insert(gate, 1);
    game.map.get_mut(receiving_planet).protection_permissions.insert(protector);

    let protect = Mission::new_with_id(
        71,
        game.turn as usize,
        protector,
        game.map.get(home),
        game.map.get(receiving_planet),
        Icon::Protect,
        Army::from([(ship, 1)]),
        BombingRaid::None,
        false,
        true,
        None,
    );
    assert_eq!(protect.protected_player, Some(receiver));
    assert_eq!(
        validate_mission(
            &game.players[0],
            &game.map,
            game.map.get(home),
            game.map.get(receiving_planet),
            &protect,
        ),
        Ok(())
    );

    let returning = Mission::new_with_id(
        72,
        game.turn as usize,
        protector,
        game.map.get(receiving_planet),
        game.map.get(home),
        Icon::Deploy,
        Army::from([(ship, 1)]),
        BombingRaid::None,
        false,
        true,
        None,
    );
    assert_eq!(
        validate_mission(
            &game.players[0],
            &game.map,
            game.map.get(receiving_planet),
            game.map.get(home),
            &returning,
        ),
        Err(OrderError::Ownership)
    );
}

#[test]
fn spy_missions_require_five_probes_but_can_reach_any_world() {
    let mut game = game();
    let origin_id = game.players[0].home_planet;
    let destination_id = game.players[1].home_planet;
    let planet_size = crate::core::map::planet::Planet::SIZE;

    game.map.get_mut(origin_id).position = bevy::math::Vec2::ZERO;
    for planet in &mut game.map.planets {
        if planet.id != origin_id {
            planet.position = bevy::math::Vec2::X * planet_size;
        }
    }
    game.map.get_mut(destination_id).position = bevy::math::Vec2::X * planet_size * 12.0;
    game.map.get_mut(origin_id).army.insert(Unit::probe(), 5);

    let spy = |game: &GameModel, count| {
        Mission::new_with_id(
            77,
            1,
            1,
            game.map.get(origin_id),
            game.map.get(destination_id),
            Icon::Spy,
            Army::from([(Unit::probe(), count)]),
            BombingRaid::None,
            false,
            false,
            None,
        )
    };

    assert_eq!(
        validate_mission(
            &game.players[0],
            &game.map,
            game.map.get(origin_id),
            game.map.get(destination_id),
            &spy(&game, 4),
        ),
        Err(OrderError::SpyProbes)
    );
    assert_eq!(
        validate_mission(
            &game.players[0],
            &game.map,
            game.map.get(origin_id),
            game.map.get(destination_id),
            &spy(&game, 5),
        ),
        Ok(())
    );

    game.map.get_mut(origin_id).buy.push(Unit::Building(Building::CommandRelay));
    game.map.get_mut(origin_id).buy.clear();
    game.map
        .get_mut(origin_id)
        .army
        .insert(Unit::Building(Building::CommandRelay), Building::MAX_LEVEL);
    assert_eq!(
        validate_mission(
            &game.players[0],
            &game.map,
            game.map.get(origin_id),
            game.map.get(destination_id),
            &spy(&game, 5),
        ),
        Ok(())
    );
}

#[test]
fn crawler_is_the_first_and_cheapest_defense() {
    let defenses = Unit::defenses();
    assert_eq!(defenses[0], Unit::crawler());
    assert_eq!(defenses[1], Unit::repair_truck());
    assert_eq!(defenses[0].damage(), 0);
    assert!(defenses.iter().skip(1).all(|unit| Unit::crawler().price() < unit.price()));
}

#[test]
fn mixed_missile_overflow_rejects_atomically() {
    let mut game = game();
    let home = game.players[0].home_planet;
    let planet = game.map.get_mut(home);
    planet.army.insert(Unit::Building(Building::MissileSilo), 2);
    planet.army.insert(Unit::Building(Building::Factory), 5);
    planet.army.insert(Unit::antiballistic_missile(), 19);
    let before = serde_json::to_value(&game).unwrap();
    assert!(submit(
        &mut game,
        vec![
            TurnCommand::BuyUnits {
                planet_id: home,
                unit: Unit::antiballistic_missile(),
                count: 1
            },
            TurnCommand::BuyUnits {
                planet_id: home,
                unit: Unit::interplanetary_missile(),
                count: 1
            },
        ]
    )
    .is_err());
    assert_eq!(serde_json::to_value(&game).unwrap(), before);
}

#[test]
fn colonization_requires_a_colony_ship_in_the_dispatched_fleet() {
    let mut game = game();
    let home = game.players[0].home_planet;
    let target = game.map.planets.iter().find(|p| p.owned.is_none() && !p.is_moon()).unwrap().id;
    game.map.get_mut(home).army.insert(Unit::colony_ship(), 1);
    game.map.get_mut(home).army.insert(Unit::Ship(Ship::LightFighter), 1);
    assert!(submit(
        &mut game,
        vec![TurnCommand::SendMission {
            mission_id: 99,
            origin: home,
            destination: target,
            objective: Icon::Colonize,
            army: Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
            bombing: BombingRaid::None,
            combat_probes: false,
            jump_gate: false,
        }]
    )
    .is_err());
    assert_eq!(game.map.get(target).owned, None);
    assert_eq!(game.map.get(home).army.amount(&Unit::colony_ship()), 1);
}

#[test]
fn every_rapid_fire_shot_starts_with_full_damage() {
    let game = game();
    let origin = game.map.get(game.players[0].home_planet);
    let mut destination = game.map.get(game.players[1].home_planet).clone();
    destination.army = Army::from([(Unit::Ship(Ship::HeavyFighter), 20)]).into();
    let attacker = Unit::Ship(Ship::Battleship);
    let mission = Mission::new_with_id(
        1,
        1,
        1,
        origin,
        &destination,
        Icon::Attack,
        Army::from([(attacker, 1)]),
        BombingRaid::None,
        true,
        false,
        None,
    );
    let mut checked = 0;
    for seed in 0..100 {
        let report = resolve_combat_with_rng(
            2,
            &mission,
            &destination,
            &mut DeterministicRngState::from_u64(seed).next_rng(),
        );
        if let Some(combat) = report.combat_report {
            for shot in
                combat.rounds[0].attacker[0].shots.iter().skip(1).filter(|s| !s.missed && !s.killed)
            {
                assert_eq!(shot.hull_damage + shot.shield_damage, attacker.damage());
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "fixture must exercise surviving targets on rapid-fire followups");
}

#[test]
fn bombing_damage_is_persisted_when_attackers_win() {
    let mut damaged = 0;
    for seed in 0..30 {
        let mut game = game();
        game.rng = DeterministicRngState::from_u64(seed);
        let home = game.players[0].home_planet;
        let target =
            game.map.planets.iter().find(|p| p.owned.is_none() && !p.is_moon()).unwrap().id;
        let mine = Unit::Building(Building::MetalMine);
        game.map.get_mut(target).army = Army::from([(mine, 5)]).into();
        let mut mission = Mission::new_with_id(
            123,
            1,
            1,
            game.map.get(home),
            game.map.get(target),
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::Bomber), 1)]),
            BombingRaid::Economic,
            false,
            false,
            None,
        );
        mission.position = game.map.get(target).position;
        game.missions.push(mission);
        submit(&mut game, vec![]).unwrap();
        let report = game.players[0].reports.last().unwrap();
        assert_eq!(
            game.map.get(target).army.amount(&mine),
            report.surviving_defender.amount(&mine)
        );
        damaged += usize::from(report.surviving_defender.amount(&mine) < 5);
    }
    assert!(damaged > 0);
}
