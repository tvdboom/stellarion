use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;

use super::*;
use crate::core::map::planet::PlanetKind;

#[test]
fn empty_planet_colonization_starts_with_a_balanced_resource_grid() {
    use crate::core::energy::EnergyGrid;
    use crate::core::units::{buildings::Building, Army, Unit};

    let mut planet = Planet::new(1, "Colony".into(), Vec2::ZERO, false, 1.0);
    planet.colonize(7);

    assert_eq!(
        planet.army,
        Army::from([
            (Unit::Building(Building::MetalMine), 1),
            (Unit::Building(Building::CrystalMine), 1),
            (Unit::Building(Building::DeuteriumSynthesizer), 1),
            (Unit::Building(Building::Reactor), 1),
        ])
    );
    let grid = planet
        .army
        .iter()
        .fold(EnergyGrid::default(), |grid, (unit, count)| grid.with_unit(*unit, None, *count));
    assert_eq!(grid.supply, 3);
    assert_eq!(grid.demand, 3);
    assert_eq!(grid.balance(), 0);
}

#[test]
fn lunar_first_completions_survive_saves_and_upgrades() {
    use crate::core::units::{buildings::Building, Unit};
    let mut moon = Planet::new(1, "Moon".into(), Vec2::ZERO, true, 1.0);
    moon.buy = vec![Unit::Building(Building::LunarBase), Unit::Building(Building::Shipyard)];
    assert_eq!(moon.surface_build_order, [None; 4]);
    moon.produce();
    moon.buy = vec![Unit::Building(Building::LunarBase), Unit::Building(Building::OrbitalRadar)];
    moon.produce();
    let encoded = serde_json::to_string(&moon).unwrap();
    let mut restored: Planet = serde_json::from_str(&encoded).unwrap();
    restored.buy = vec![Unit::Building(Building::Laboratory)];
    restored.produce();
    assert_eq!(
        restored.surface_build_order,
        [
            Some(Building::LunarBase),
            Some(Building::Shipyard),
            Some(Building::OrbitalRadar),
            Some(Building::Laboratory)
        ]
    );
    restored.destroy();
    assert_eq!(restored.surface_build_order, [None; 4]);
}

#[test]
fn planetary_surface_slots_follow_first_completion_and_never_repack() {
    use crate::core::units::{buildings::Building, Unit};
    use Building::*;

    let mut planet = Planet::new(1, "Planet".into(), Vec2::ZERO, false, 1.0);
    planet.buy = vec![Unit::Building(Robotics)];
    planet.produce();
    planet.buy = vec![
        Unit::Building(MetalMine),
        Unit::Building(CrystalMine),
        Unit::Building(Reactor),
        Unit::Building(Factory),
        Unit::Building(Shipyard),
        Unit::Building(MissileSilo),
        Unit::Building(Senate),
    ];
    planet.produce();
    assert_eq!(
        planet.surface_build_order,
        [Some(Robotics), Some(MetalMine), Some(Shipyard), Some(MissileSilo)]
    );

    planet.army.remove(&Unit::Building(Robotics));
    let encoded = serde_json::to_string(&planet).unwrap();
    let mut restored: Planet = serde_json::from_str(&encoded).unwrap();
    assert_eq!(restored.surface_build_order, planet.surface_build_order);

    let previous_field_name = encoded.replace("surface_build_order", "lunar_build_order");
    let previous_save: Planet = serde_json::from_str(&previous_field_name).unwrap();
    assert_eq!(previous_save.surface_build_order, planet.surface_build_order);

    restored.buy = vec![Unit::Building(Robotics)];
    restored.produce();
    assert_eq!(restored.surface_build_order, planet.surface_build_order);
}

#[test]
fn destroyed_worlds_use_the_destroyed_planet_and_moon_artwork() {
    for (moon, expected_image) in [(false, "planet0"), (true, "moon0")] {
        let mut planet = Planet::new(1, "Masduk".to_string(), Vec2::ZERO, moon, 1.0);
        assert_ne!(planet.image(), expected_image);

        planet.destroy();

        assert_eq!(planet.image(), expected_image);
        let saved = serde_json::to_string(&planet).unwrap();
        let restored: Planet = serde_json::from_str(&saved).unwrap();
        assert_eq!(restored.image(), expected_image);
    }
}

/// Covers practice, typical multiplayer, and the densest supported planet/moon settings.
#[test]
fn supported_maps_keep_worlds_inside_bounds_and_clear_of_each_other() {
    for seed in 0..32 {
        for (n_planets, p_moons) in
            [(5, 0), (20, 30), (30, 100), (60, 0), (60, 100), (80, 0), (80, 100)]
        {
            let mut rng = ChaCha8Rng::seed_from_u64(seed);
            let map = Map::new_with_rng(n_planets, p_moons, &mut rng);
            assert_eq!(map.planets().len(), n_planets);
            assert_eq!(map.moons().len(), n_planets * p_moons / 100);
            let safe_bounds = Rect::from_corners(map.rect.min * 0.9, map.rect.max * 0.9);
            for (index, planet) in map.planets.iter().enumerate() {
                assert_eq!(planet.id, index);
                assert!(safe_bounds.contains(planet.position));
                for other in &map.planets[index + 1..] {
                    let separation = match (planet.is_moon(), other.is_moon()) {
                        (false, false) => 250.0,
                        (true, true) => 125.0,
                        _ => 150.0,
                    };
                    let distance = planet.position.distance(other.position);
                    assert!(distance >= separation, "seed {seed}: {distance} < {separation}");
                    assert!(distance > (planet.size() + other.size()) * 0.5);
                }
            }
        }
    }
}

/// Shuffling lattice points changes IDs but leaves repeated rows and equal neighbor gaps.
#[test]
fn scattered_maps_have_varied_coordinates_and_neighbor_distances() {
    for (n_planets, p_moons) in [(20, 30), (80, 100)] {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let map = Map::new_with_rng(n_planets, p_moons, &mut rng);
        let positions = map.planets.iter().map(|planet| planet.position).collect::<Vec<_>>();
        for coordinate in [|p: &Vec2| p.x, |p: &Vec2| p.y] {
            let unique = positions.iter().map(coordinate).map(f32::to_bits).unique().count();
            assert!(unique > positions.len() * 9 / 10);
        }
        let nearest = positions
            .iter()
            .enumerate()
            .map(|(index, position)| {
                positions
                    .iter()
                    .enumerate()
                    .filter(|(other, _)| *other != index)
                    .map(|(_, other)| position.distance(*other))
                    .fold(f32::INFINITY, f32::min)
            })
            .collect::<Vec<_>>();
        let smallest = nearest.iter().copied().fold(f32::INFINITY, f32::min);
        let largest = nearest.iter().copied().fold(0.0, f32::max);
        assert!(largest - smallest > Planet::SIZE);
    }
}

/// Multiplayer creation must use only the supplied random stream for the complete map.
#[test]
fn map_generation_is_reproducible_and_seeded() {
    let make_map = |seed| {
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let map = Map::new_with_rng(20, 30, &mut rng);
        serde_json::to_value(map).unwrap()
    };
    assert_eq!(make_map(42), make_map(42));
    assert_ne!(make_map(42), make_map(43));
}

#[test]
fn solar_bands_have_stable_quarters_and_matching_planet_kinds() {
    let mut rng = ChaCha8Rng::seed_from_u64(91);
    let map = Map::new_with_rng(20, 30, &mut rng);
    let mut counts = [0; 3];

    for world in &map.planets {
        let band = map.solar_band(world.id);
        if world.is_moon() {
            assert_eq!(band, None);
            continue;
        }
        match band.unwrap() {
            SolarBand::Inner => {
                counts[0] += 1;
                assert!(matches!(world.kind, PlanetKind::Dry | PlanetKind::Metallic));
            },
            SolarBand::Temperate => counts[1] += 1,
            SolarBand::Outer => {
                counts[2] += 1;
                assert!(matches!(
                    world.kind,
                    PlanetKind::Gas | PlanetKind::Ice | PlanetKind::Water
                ));
            },
        }
    }

    assert_eq!(counts, [5, 10, 5]);
}

#[test]
fn lunar_temperatures_follow_their_distance_from_the_star() {
    let mut rng = ChaCha8Rng::seed_from_u64(91);
    let map = Map::new_with_rng(80, 100, &mut rng);
    let positions = map.planets.iter().map(|world| world.position).collect::<Vec<_>>();
    let moons = map.planets.iter().map(Planet::is_moon).collect::<Vec<_>>();
    let bands = solar_bands(map.rect, &positions, &moons);
    let mut seen = [false; 3];

    for moon in map.moons() {
        match bands[moon.id].unwrap() {
            SolarBand::Inner => {
                seen[0] = true;
                assert!((-100..0).contains(&moon.temperature.0));
                assert!((100..=240).contains(&moon.temperature.1));
                assert_eq!(moon.temperature_emoji(), "🔥");
            },
            SolarBand::Temperate => {
                seen[1] = true;
                assert!((-180..-100).contains(&moon.temperature.0));
                assert!((40..=130).contains(&moon.temperature.1));
                assert_eq!(moon.temperature_emoji(), "☀");
            },
            SolarBand::Outer => {
                seen[2] = true;
                assert!((-260..-180).contains(&moon.temperature.0));
                assert!((-120..=-20).contains(&moon.temperature.1));
                assert_eq!(moon.temperature_emoji(), "❄");
            },
        }
    }

    assert_eq!(seen, [true; 3]);
}

/// Moons can occupy space that would be rejected for a pair of full-sized planets.
#[test]
fn moons_can_be_closer_to_planets_than_other_planets() {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let map = Map::new_with_rng(80, 100, &mut rng);
    assert!(map.moons().iter().any(|moon| {
        map.planets().iter().any(|planet| moon.position.distance(planet.position) < 250.0)
    }));
}

/// Crowded input must finish with more space, never with smaller gaps or overlapping worlds.
#[test]
fn crowded_custom_maps_expand_without_relaxing_clearance() {
    let mut rng = ChaCha8Rng::seed_from_u64(42);
    let initial_rect = Rect::new(-50.0, -50.0, 50.0, 50.0);
    let mut rect = initial_rect;
    let moons = (0..100).map(|index| index % 2 == 0).collect::<Vec<_>>();
    let positions = generate_positions(&mut rect, &moons, &mut rng);
    assert_eq!(positions.len(), 100);
    assert!(rect.width() > initial_rect.width());
    assert!(rect.height() > initial_rect.height());
    assert!(positions.iter().all(|position| rect.contains(*position)));
    for (index, &position) in positions.iter().enumerate() {
        for (other, &other_position) in positions.iter().enumerate().skip(index + 1) {
            let separation = match (moons[index], moons[other]) {
                (false, false) => 250.0,
                (true, true) => 125.0,
                _ => 150.0,
            };
            assert!(position.distance(other_position) >= separation);
        }
    }
    assert!(generate_positions(&mut rect, &[], &mut rng).is_empty());
}
