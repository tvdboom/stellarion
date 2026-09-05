use super::*;
use crate::core::units::ships::Ship;

fn journey(old_turns: f32, army: Army) -> (Map, Mission) {
    let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    let rating = army.keys().map(Combat::speed).fold(f32::INFINITY, f32::min);
    let destination = Planet::new(
        1,
        "Destination".into(),
        Vec2::X * Planet::SIZE * (rating * old_turns + 1.4),
        false,
        1.0,
    );
    let mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        army,
        BombingRaid::None,
        false,
        false,
        None,
    );
    (
        Map {
            rect: Rect::default(),
            planets: vec![origin, destination],
        },
        mission,
    )
}

#[test]
fn accelerated_eta_matches_movement_and_survives_reload() {
    for unit in [
        Unit::Ship(Ship::LightFighter),
        Unit::Ship(Ship::ColonyShip),
        Unit::interplanetary_missile(),
    ] {
        for (old, expected) in [(1.0, 1), (2.0, 2), (5.0, 3), (8.0, 4), (14.0, 6), (20.0, 7)] {
            let (map, mut mission) = journey(old, Army::from([(unit, 1)]));
            assert_eq!(mission.duration(&map), expected, "{unit:?}, old={old}");
            for elapsed in 0..expected {
                assert_eq!(
                    mission.turns_to_destination(&map),
                    expected - elapsed,
                    "{unit:?}, old={old}, elapsed={elapsed}"
                );
                mission.advance(&map);
                mission = serde_json::from_slice(&serde_json::to_vec(&mission).unwrap()).unwrap();
            }
            assert_eq!(mission.turns_to_destination(&map), 0);
        }
    }
}

#[test]
fn acceleration_does_not_make_extra_short_routes_one_turn() {
    let army = Army::from([(Unit::Ship(Ship::LightFighter), 1)]);
    for (old, expected) in [(0.8, 1), (1.001, 2), (2.666, 2), (2.668, 3), (5.001, 4)] {
        let (map, mission) = journey(old, army.clone());
        assert_eq!(mission.duration(&map), expected);
    }
}

#[test]
fn mixed_fleets_jump_gates_and_new_legs_keep_their_rules() {
    let army = Army::from([(Unit::probe(), 10), (Unit::Ship(Ship::ColonyShip), 1)]);
    let (map, mut mission) = journey(14.0, army);
    assert_eq!(mission.speed(), Unit::Ship(Ship::ColonyShip).speed());
    let initial_fuel = mission.fuel_consumption(&map);
    mission.advance(&map);
    mission.advance(&map);
    assert_eq!(mission.travel_turns, 2);
    let returning = Mission::new_with_id(
        2,
        3,
        1,
        &map.planets[1],
        &map.planets[0],
        Icon::Deploy,
        mission.army.clone(),
        BombingRaid::None,
        false,
        false,
        None,
    );
    assert_eq!(returning.travel_turns, 0);
    assert_eq!(returning.duration(&map), 6);
    assert_eq!(returning.fuel_consumption(&map), initial_fuel);
    mission.jump_gate = true;
    assert_eq!(mission.duration(&map), 1);
    assert_eq!(mission.fuel_consumption(&map), 0);
    mission.advance(&map);
    assert_eq!(mission.position, map.planets[1].position);
}

#[test]
fn next_turn_movement_tracks_acceleration_and_actual_arrival() {
    let army = Army::from([(Unit::probe(), 10), (Unit::Ship(Ship::ColonyShip), 1)]);
    let (map, mut mission) = journey(14.0, army);
    let first_movement = mission.next_turn_movement(&map);
    assert!((first_movement - mission.speed()).abs() < 1e-5);

    while mission.duration(&map) > 0 {
        let movement = mission.next_turn_movement(&map);
        let before = mission.position;
        if mission.travel_turns == 1 {
            assert!((movement - first_movement * 5.0 / 3.0).abs() < 1e-5);
        }
        if mission.duration(&map) == 1 {
            let remaining = before.distance(map.get(mission.destination).position) / Planet::SIZE;
            assert!((movement - remaining).abs() < 1e-5);
        }
        mission.advance(&map);
        assert!((movement - before.distance(mission.position) / Planet::SIZE).abs() < 1e-5);
    }
    assert_eq!(mission.next_turn_movement(&map), 0.0);
}

#[test]
fn next_turn_movement_handles_jump_gates_and_stationary_fleets() {
    let (map, mut mission) = journey(20.0, Army::from([(Unit::probe(), 1)]));
    mission.army.clear();
    assert_eq!(mission.next_turn_movement(&map), 0.0);

    mission.army.insert(Unit::probe(), 1);
    mission.jump_gate = true;
    let before = mission.position;
    let movement = mission.next_turn_movement(&map);
    assert!(movement.is_finite());
    mission.advance(&map);
    assert_eq!(mission.position, map.get(mission.destination).position);
    assert!((movement - before.distance(mission.position) / Planet::SIZE).abs() < 1e-5);
    assert_eq!(mission.next_turn_movement(&map), 0.0);
}
