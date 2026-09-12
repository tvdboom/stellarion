use super::*;
use crate::core::map::model::SolarCorner;
use rand::SeedableRng;

fn scanned_mission() -> (Map, Player, Mission) {
    let mut rng = rand_chacha::ChaCha8Rng::from_seed([51; 32]);
    let mut origin =
        Planet::new_with_rng(0, "Enemy".into(), Vec2::X * 1_000.0, false, 1.0, &mut rng);
    origin.owned = Some(2);
    origin.controlled = Some(2);
    let mut destination = Planet::new_with_rng(1, "Home".into(), Vec2::ZERO, false, 1.0, &mut rng);
    destination.owned = Some(1);
    destination.controlled = Some(1);
    destination.army.insert(Unit::Building(Building::SensorPhalanx), 2);
    let player = Player {
        id: 1,
        home_planet: destination.id,
        ..default()
    };
    let mut mission = Mission::new_with_id(
        7,
        1,
        2,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::colony_ship(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.position = Vec2::X * Planet::SIZE;
    let map = Map {
        rect: Rect::default(),
        solar_corner: SolarCorner::BottomLeft,
        planets: vec![origin, destination],
    };
    (map, player, mission)
}

#[test]
fn phalanx_tracks_recalled_enemy_fleets_until_they_leave_range() {
    let (map, player, mut mission) = scanned_mission();
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), Some(2));

    mission.recall(&map, 2);
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), Some(2));

    let home = map.get(player.home_planet);
    mission.position = home.position + Vec2::X * (2.0 * Planet::SIZE + home.size() * 0.5);
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), Some(2));

    mission.position += Vec2::X;
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), None);
}

#[test]
fn phalanx_detects_fresh_enemy_departures_from_its_planet() {
    let (map, player, incoming) = scanned_mission();
    let departing = Mission::new_with_id(
        8,
        2,
        incoming.owner,
        map.get(incoming.destination),
        map.get(incoming.origin),
        Icon::Deploy,
        incoming.army,
        BombingRaid::None,
        false,
        false,
        None,
    );
    assert!(!departing.is_returning());
    assert_eq!(departing.is_seen_by_phalanx(&map, &player), Some(2));
}

#[test]
fn phalanx_requires_a_built_scanner_on_an_owned_route_endpoint() {
    let (mut map, player, mut mission) = scanned_mission();
    let phalanx = Unit::Building(Building::SensorPhalanx);
    mission.position = map.get(player.home_planet).position;
    map.get_mut(player.home_planet).army.remove(&phalanx);
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), None);

    map.get_mut(player.home_planet).army.insert(phalanx, 2);
    map.get_mut(player.home_planet).owned = None;
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), None);
    mission.recall(&map, 2);
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), None);

    map.get_mut(player.home_planet).owned = Some(player.id);
    let mut unrelated = map.get(mission.destination).clone();
    unrelated.id = 2;
    map.planets.push(unrelated);
    mission.origin = 2;
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), None);
}

#[test]
fn phalanx_keeps_spies_and_missiles_hidden_in_both_directions() {
    for objective in [Icon::Spy, Icon::MissileStrike] {
        let (map, player, mut mission) = scanned_mission();
        mission.objective = objective;
        assert_eq!(mission.is_seen_by_phalanx(&map, &player), None);
        mission.recall(&map, 2);
        assert_eq!(mission.is_seen_by_phalanx(&map, &player), None);
    }
}

#[test]
fn phalanx_uses_the_strongest_endpoint_scanner_that_covers_the_fleet() {
    let (mut map, player, mission) = scanned_mission();
    let origin = map.get_mut(mission.origin);
    origin.owned = Some(player.id);
    origin.controlled = Some(player.id);
    origin.army.insert(Unit::Building(Building::SensorPhalanx), 3);
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), Some(2));

    map.get_mut(mission.origin).position = Vec2::X * Planet::SIZE * 2.0;
    assert_eq!(mission.is_seen_by_phalanx(&map, &player), Some(3));
}

#[cfg(feature = "app")]
#[test]
fn phalanx_keeps_recalled_enemy_fleets_in_the_visible_mission_projection() {
    use crate::core::turns::filter_missions;

    let (map, player, mut mission) = scanned_mission();
    mission.recall(&map, 2);
    let visible = filter_missions(std::slice::from_ref(&mission), &map, &player);
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].id, mission.id);

    mission.position = map.get(mission.destination).position;
    assert!(filter_missions(&[mission], &map, &player).is_empty());
}
