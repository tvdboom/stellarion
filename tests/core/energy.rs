use super::*;

use crate::core::map::planet::{ShieldOverloadState, SolarBand};
use crate::core::simulation::{GameModel, GameRules};
use crate::core::units::Description;
use strum::IntoEnumIterator;

#[test]
fn every_building_has_one_authoritative_per_level_energy_value() {
    let one_demand = [
        Building::MetalMine,
        Building::CrystalMine,
        Building::DeuteriumSynthesizer,
        Building::Terraformer,
        Building::SensorPhalanx,
        Building::PlanetaryShield,
        Building::JumpGate,
        Building::Laboratory,
        Building::OrbitalRadar,
    ];

    for building in Building::iter() {
        let expected = if building == Building::TidalGenerator {
            EnergyGrid {
                supply: 5,
                demand: 0,
            }
        } else if matches!(building, Building::Reactor | Building::SolarSatellite) {
            EnergyGrid {
                supply: 3,
                demand: 0,
            }
        } else if building == Building::Senate {
            EnergyGrid {
                supply: 0,
                demand: 2,
            }
        } else if building == Building::OrbitalRailgun {
            EnergyGrid {
                supply: 0,
                demand: 4,
            }
        } else if one_demand.contains(&building) {
            EnergyGrid {
                supply: 0,
                demand: 1,
            }
        } else {
            EnergyGrid::default()
        };

        assert_eq!(
            EnergyGrid::for_building(building, Some(SolarBand::Inner)),
            expected,
            "{building:?}"
        );
    }
}

#[test]
fn space_dock_consumes_two_energy() {
    assert_eq!(
        EnergyGrid::for_unit(Unit::space_dock(), None),
        EnergyGrid {
            supply: 0,
            demand: 2,
        }
    );
}

#[test]
fn senate_consumes_two_energy() {
    assert_eq!(
        EnergyGrid::for_building(Building::Senate, None),
        EnergyGrid {
            supply: 0,
            demand: 2,
        }
    );
}

#[test]
fn building_descriptions_do_not_duplicate_energy_supply_or_demand() {
    for building in Building::iter() {
        let description = building.description().to_ascii_lowercase();
        assert!(
            !(description.contains("energy")
                && (description.contains("supplies") || description.contains("consumes"))),
            "{building:?} still includes its energy value in the description"
        );
    }
}

#[test]
fn starting_home_grid_is_neutral() {
    let game = GameModel::new([41; 32], GameRules::default()).unwrap();
    for player in &game.players {
        assert_eq!(
            player.energy_grid(&game.map),
            EnergyGrid {
                supply: 3,
                demand: 3
            }
        );
    }
}

#[test]
fn fully_developed_planet_balance_follows_its_solar_band() {
    let mut game = GameModel::new([47; 32], GameRules::default()).unwrap();
    let player = game.players[0].id;
    for world in &mut game.map.planets {
        world.owned = None;
        world.controlled = None;
        world.army.clear();
    }
    for (band, expected_supply) in
        [(SolarBand::Inner, 30), (SolarBand::Temperate, 25), (SolarBand::Outer, 20)]
    {
        let planet_id = game
            .map
            .planets()
            .into_iter()
            .find(|planet| game.map.solar_band(planet.id) == Some(band))
            .unwrap()
            .id;
        let planet = game.map.get_mut(planet_id);
        planet.colonize(player);
        for building in Building::iter().filter(|building| {
            Unit::Building(*building).valid_on(false) && *building != Building::Senate
        }) {
            planet.army.insert(Unit::Building(building), Building::MAX_LEVEL);
        }
        planet.army.insert(Unit::Building(Building::Senate), 1);
        planet.army.insert(Unit::space_dock(), 1);

        assert_eq!(
            EnergyGrid::for_world(&game.map, game.map.get(planet_id)),
            EnergyGrid {
                supply: expected_supply,
                demand: 59,
            },
            "{band:?}",
        );
    }
}

#[test]
fn satellite_output_follows_the_planets_solar_band() {
    let mut game = GameModel::new([42; 32], GameRules::default()).unwrap();
    let player = game.players[0].id;
    for planet in &mut game.map.planets {
        planet.owned = None;
        planet.controlled = None;
        planet.army.clear();
    }

    for expected_band in [SolarBand::Inner, SolarBand::Temperate, SolarBand::Outer] {
        let planet_id = game
            .map
            .planets()
            .into_iter()
            .find(|planet| game.map.solar_band(planet.id) == Some(expected_band))
            .unwrap()
            .id;
        let planet = game.map.get_mut(planet_id);
        planet.owned = Some(player);
        planet.controlled = Some(player);
        planet.army.insert(Unit::Building(Building::SolarSatellite), 1);
    }

    assert_eq!(
        EnergyGrid::for_player(player, &game.map),
        EnergyGrid {
            supply: 6,
            demand: 0
        }
    );
}

#[test]
fn tidal_generators_supply_five_energy_per_level() {
    let mut game = GameModel::new([43; 32], GameRules::default()).unwrap();
    let player = game.players[0].id;
    for planet in &mut game.map.planets {
        planet.owned = None;
        planet.controlled = None;
        planet.army.clear();
    }
    let moon_id = game.map.moons()[0].id;
    let moon = game.map.get_mut(moon_id);
    moon.colonize(player);
    moon.army.insert(Unit::Building(Building::TidalGenerator), 3);

    assert_eq!(
        EnergyGrid::for_player(player, &game.map),
        EnergyGrid {
            supply: 15,
            demand: 0
        }
    );
}

#[test]
fn lunar_laboratory_and_orbital_radar_consume_one_energy_per_level() {
    let mut game = GameModel::new([48; 32], GameRules::default()).unwrap();
    let player = game.players[0].id;
    let moon_id = game.map.moons()[0].id;
    let moon = game.map.get_mut(moon_id);
    moon.colonize(player);
    moon.army.clear();
    moon.army.insert(Unit::Building(Building::Laboratory), 3);
    moon.army.insert(Unit::Building(Building::OrbitalRadar), 4);

    assert_eq!(
        EnergyGrid::for_world(&game.map, game.map.get(moon_id)),
        EnergyGrid {
            supply: 0,
            demand: 7,
        }
    );
}

#[test]
fn terraformer_consumes_one_energy_per_level() {
    let mut game = GameModel::new([44; 32], GameRules::default()).unwrap();
    let player = game.players[0].id;
    for planet in &mut game.map.planets {
        planet.owned = None;
        planet.controlled = None;
        planet.army.clear();
    }
    let planet = game.map.planets.iter_mut().find(|planet| !planet.is_moon()).unwrap();
    planet.colonize(player);
    planet.army.clear();
    planet.army.insert(Unit::Building(Building::Terraformer), 4);

    assert_eq!(
        EnergyGrid::for_player(player, &game.map),
        EnergyGrid {
            supply: 0,
            demand: 4
        }
    );
}

#[test]
fn individual_world_grids_sum_to_the_player_total() {
    let game = GameModel::new([45; 32], GameRules::default()).unwrap();
    for player in &game.players {
        let worlds = game
            .map
            .planets
            .iter()
            .filter(|planet| {
                planet.controlled.or(planet.owned) == Some(player.id) && !planet.is_destroyed
            })
            .map(|planet| EnergyGrid::for_world(&game.map, planet))
            .fold(EnergyGrid::default(), |total, world| EnergyGrid {
                supply: total.supply + world.supply,
                demand: total.demand + world.demand,
            });
        assert_eq!(worlds, player.energy_grid(&game.map));
    }
}

#[test]
fn each_missing_energy_costs_ten_percent_and_resources_and_shields_share_efficiency() {
    let resources = Resources::new(usize::MAX, usize::MAX / 2, 101);
    assert_eq!(EnergyGrid::default().scale_resources(resources), resources);
    let three_energy_short = EnergyGrid {
        supply: 3,
        demand: 6,
    };
    assert_eq!(three_energy_short.efficiency_percent(), 70);
    assert_eq!(
        three_energy_short.scale_resources(Resources::new(100, 80, 60)),
        Resources::new(70, 56, 42),
    );
    assert_eq!(three_energy_short.planetary_shield(1, false), 210);
    assert_eq!(three_energy_short.planetary_shield(1, true), 231);

    let blackout = EnergyGrid {
        supply: 0,
        demand: 10,
    };
    assert_eq!(blackout.efficiency_percent(), 30);
    assert_eq!(blackout.scale_resources(Resources::new(100, 80, 60)), Resources::new(30, 24, 18),);
    assert_eq!(blackout.planetary_shield(1, false), 90);
    assert_eq!(EnergyGrid::default().planetary_shield(5, true), 2_250);
}

#[test]
fn orbital_railgun_consumes_four_energy_per_level() {
    assert_eq!(
        EnergyGrid::for_building(Building::OrbitalRailgun, None),
        EnergyGrid {
            supply: 0,
            demand: 4,
        }
    );
}

#[test]
fn shield_overload_adds_three_flat_demand_to_its_world() {
    let mut game = GameModel::new([50; 32], GameRules::default()).unwrap();
    let home = game.players[0].home_planet;
    let before = EnergyGrid::for_world(&game.map, game.map.get(home));
    let planet = game.map.get_mut(home);
    planet.army.insert(Unit::planetary_shield(), 2);
    planet.shield_overload = ShieldOverloadState::Overloaded;
    let after = EnergyGrid::for_world(&game.map, game.map.get(home));

    assert_eq!(after.supply, before.supply);
    assert_eq!(after.demand, before.demand + 2 + 3);
}

#[test]
fn next_turn_grid_includes_all_queued_infrastructure() {
    let mut game = GameModel::new([49; 32], GameRules::default()).unwrap();
    let player = game.players[0].id;
    let home = game.players[0].home_planet;
    game.map
        .get_mut(home)
        .buy
        .extend([Unit::Building(Building::MetalMine), Unit::Building(Building::Reactor)]);

    assert_eq!(
        EnergyGrid::for_player_next_turn(player, &game.map),
        EnergyGrid {
            supply: 6,
            demand: 4,
        }
    );
}
