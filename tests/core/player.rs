use super::*;
use crate::core::units::ships::Ship;
use crate::test_support::empty_report;

#[test]
fn intelligence_keeps_peak_buildings_but_latest_fleets_and_orbitals() {
    let planet = Planet::new(1, "Former colony".into(), Vec2::ZERO, false, 1.0);
    let mine = Unit::Building(Building::MetalMine);
    let satellite = Unit::Building(Building::SolarSatellite);
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut player = Player::new(1, 0);
    let departure = Mission {
        owner: player.id,
        origin: planet.id,
        origin_controlled: Some(player.id),
        send: 3,
        origin_army: Army::from([(mine, 5), (satellite, 4), (fighter, 10)]),
        army: Army::from([(fighter, 3)]),
        ..default()
    };
    player.reports.push(empty_report(departure.clone(), planet.clone()));
    let pending = Mission {
        send: 4,
        origin_army: Army::from([(mine, 2), (satellite, 1), (fighter, 2)]),
        ..departure.clone()
    };
    let info = player.last_info(&planet, &[pending]).unwrap();
    assert_eq!((info.turn, info.controlled), (4, None));
    assert_eq!(info.army.amount(&mine), 5);
    assert_eq!(info.army.amount(&satellite), 1);
    assert_eq!(info.army.amount(&fighter), 0); // Departures subtract with saturation.

    let enemy = Mission {
        owner: 2,
        objective: Icon::Attack,
        ..departure
    };
    let info = player.last_info(&planet, std::slice::from_ref(&enemy)).unwrap();
    assert_eq!((info.turn, info.controlled), (3, Some(2))); // Last observation wins ties.
    assert_eq!(info.army, Army::from([(mine, 5)]));
    player.reports.push(empty_report(enemy, planet.clone()));
    let info = player.last_info(&planet, &[]).unwrap();
    assert_eq!((info.turn, info.controlled), (3, Some(2)));
    assert_eq!(info.army, Army::from([(mine, 5)]));
}

#[test]
fn intelligence_ignores_hidden_departures_returns_missiles_and_destroyed_worlds() {
    let mut planet = Planet::new(1, "Target".into(), Vec2::ZERO, false, 1.0);
    let mut player = Player::new(1, 0);
    let missions = [
        Mission {
            origin: planet.id,
            owner: 2,
            objective: Icon::Spy,
            ..default()
        },
        Mission {
            origin: planet.id,
            owner: 1,
            origin_controlled: Some(2),
            ..default()
        },
        Mission {
            origin: 3,
            destination: planet.id,
            objective: Icon::MissileStrike,
            ..default()
        },
    ];
    player.reports = missions.iter().map(|m| empty_report(m.clone(), planet.clone())).collect();
    assert!(player.last_info(&planet, &missions).is_none());
    let visible = Mission {
        origin: planet.id,
        owner: 2,
        objective: Icon::Attack,
        ..default()
    };
    assert!(player.last_info(&planet, std::slice::from_ref(&visible)).is_some());
    planet.is_destroyed = true;
    assert!(player.last_info(&planet, &[visible]).is_none());
}

#[test]
fn conquered_world_intelligence_excludes_returning_probes_and_preserves_buildings() {
    let mut planet = Planet::new(1, "Target".into(), Vec2::ZERO, false, 1.0);
    planet.controlled = Some(2);
    let mut player = Player::new(1, 0);
    let fighter = Unit::Ship(Ship::LightFighter);
    player.reports.push(MissionReport {
        turn: 5,
        destination_controlled: Some(player.id),
        scout_probes: 3,
        surviving_attacker: Army::from([(Unit::probe(), 5), (fighter, 4)]),
        surviving_defender: [(Unit::planetary_shield(), 2)].into_iter().collect(),
        ..empty_report(
            Mission {
                origin: 0,
                destination: planet.id,
                owner: player.id,
                objective: Icon::Attack,
                ..default()
            },
            planet.clone(),
        )
    });
    let info = player.last_info(&planet, &[]).unwrap();
    assert_eq!((info.turn, info.controlled), (5, Some(player.id)));
    assert_eq!(info.army.amount(&Unit::probe()), 2);
    assert_eq!(info.army.amount(&fighter), 4);
    assert_eq!(info.army.amount(&Unit::planetary_shield()), 2);
}
