use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::map::icon::Icon;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::orders::purchase_limit;
use crate::core::random::DeterministicRngState;
use crate::core::simulation::{resolve_turn, GameModel, GameRules, TurnCommand, TurnSubmission};
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Unit};

fn game() -> GameModel {
    let mut game = GameModel::new([7; 32], GameRules::default()).unwrap();
    game.start().unwrap();
    game
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
    assert_eq!(purchase_limit(&game.players[0], planet, Unit::antiballistic_missile()).unwrap(), 2);
    planet.buy.extend([Unit::antiballistic_missile(); 2]);
    assert!(purchase_limit(&game.players[0], planet, Unit::interplanetary_missile()).is_err());
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
    destination.army = Army::from([(Unit::Ship(Ship::HeavyFighter), 20)]);
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
        game.map.get_mut(target).army = Army::from([(mine, 5)]);
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
