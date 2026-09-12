use super::*;
use crate::core::combat::report::CombatReport;
use crate::core::map::icon::Icon;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::simulation::{GameModel, GameRules};
use crate::core::units::ships::Ship;
use std::collections::BTreeSet;

fn model() -> GameModel {
    let mut model = GameModel::new([7; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    model
}

#[test]
fn development_images_respect_world_limits_and_leave_status_icons_clear() {
    let model = model();
    for (kind, limit) in [(PlanetKind::Metallic, 4), (PlanetKind::Gas, 4), (PlanetKind::Gray, 3)] {
        let mut planet = model.map.planets[0].clone();
        planet.kind = kind;
        let mut world = World::new();
        let image = Handle::<Image>::default();
        let moon_shipyard = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(1), default());
        let moon_tidal_generator = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(2), default());
        let moon_orbital_radar = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(3), default());
        let planet_robotics = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(4), default());
        let gas_planet_robotics = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(5), default());
        let dedicated_images = [
            moon_shipyard.id(),
            moon_tidal_generator.id(),
            moon_orbital_radar.id(),
            planet_robotics.id(),
            gas_planet_robotics.id(),
        ];
        let art = DevelopmentArt {
            base: &image,
            base_size: Vec2::splat(512.0),
            facilities: &image,
            facilities_size: Vec2::new(768.0, 512.0),
            gas: &image,
            gas_size: Vec2::splat(512.0),
            moon_shipyard: &moon_shipyard,
            moon_tidal_generator: &moon_tidal_generator,
            moon_orbital_radar: &moon_orbital_radar,
            planet_terraformer: &planet_robotics,
            planet_administration: &planet_robotics,
            gas_planet_terraformer: &gas_planet_robotics,
            gas_planet_administration: &gas_planet_robotics,
            surface_lights: &image,
            shadow: &image,
        };
        spawn_development(
            &mut world.commands(),
            &planet,
            Development {
                settlement: 3,
                mining: 3,
                refinery: 3,
                factory: 3,
                terraformer: 3,
                administration: 0,
                shipyard: 3,
                reactor: 3,
                laboratory: 3,
                silo: 3,
                lunar_base: if planet.is_moon() {
                    3
                } else {
                    0
                },
                tidal_generator: usize::from(planet.is_moon()) * 3,
                senate: true,
                sensor: true,
                surface_build_order: if planet.is_moon() {
                    [
                        Some(Building::LunarBase),
                        Some(Building::TidalGenerator),
                        Some(Building::OrbitalRadar),
                        None,
                    ]
                } else {
                    [
                        Some(Building::MetalMine),
                        Some(Building::Shipyard),
                        Some(Building::MissileSilo),
                        Some(Building::Senate),
                    ]
                },
            },
            &art,
        );
        world.flush();
        let mut images = 0;
        for (sprite, transform, light) in
            world.query::<(&Sprite, &Transform, Option<&SurfaceLight>)>().iter(&world)
        {
            if light.is_some() {
                continue;
            }
            if sprite.rect.is_none() && !dedicated_images.contains(&sprite.image.id()) {
                continue;
            }
            images += 1;
            let right = transform.translation.x + sprite.custom_size.unwrap().x * 0.5;
            let icon_left = planet.position.x + planet.size() * 0.45 - Icon::SIZE * 0.5;
            assert!(right < icon_left, "{kind:?} development overlaps status icons");
        }
        assert_eq!(images, limit, "{kind:?} development image count");
        if kind == PlanetKind::Gas {
            assert_eq!(world.query::<&SurfaceLight>().iter(&world).count(), 0);
            assert_eq!(world.query::<&PlatformLight>().iter(&world).count(), 24);
            let mut variants = BTreeSet::new();
            for (sprite, transform) in world.query::<(&Sprite, &Transform)>().iter(&world) {
                if let Some(rect) = sprite.rect {
                    // Include the full sprite rectangle and maximum hover displacement.
                    let farthest_corner = (transform.translation.truncate() - planet.position)
                        .abs()
                        + sprite.custom_size.unwrap() * 0.5
                        + Vec2::new(0.0, planet.size() * 0.008);
                    assert!(farthest_corner.length() < planet.size() * 0.5);
                    assert!(
                        variants.insert((rect.min.x as u32, rect.min.y as u32)),
                        "duplicate gas artwork"
                    );
                }
            }
            assert_eq!(variants.len(), 4);
        }
    }
}

#[cfg(target_os = "windows")]
#[test]
fn gas_development_tiles_have_transparent_gutters() {
    let source = ::image::open(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/assets/images/planet-buildings/gas-development.png"
    ))
    .unwrap()
    .into_rgba8();
    let (width, height) = source.dimensions();
    assert_eq!((width, height), (1254, 1254));

    let cell_width = width / 2;
    let cell_height = height / 2;
    let gutter = 8;
    for row in 0..2 {
        for column in 0..2 {
            let min_x = column * cell_width;
            let min_y = row * cell_height;
            let max_x = min_x + cell_width - 1;
            let max_y = min_y + cell_height - 1;
            let mut visible_pixels = 0;
            for y in min_y..=max_y {
                for x in min_x..=max_x {
                    let alpha = source.get_pixel(x, y).0[3];
                    visible_pixels += usize::from(alpha > 0);
                    if x < min_x + gutter
                        || x > max_x - gutter
                        || y < min_y + gutter
                        || y > max_y - gutter
                    {
                        assert_eq!(
                            alpha, 0,
                            "gas development tile ({column}, {row}) reaches its crop edge at ({x}, {y})"
                        );
                    }
                }
            }
            assert!(visible_pixels > 0, "gas development tile ({column}, {row}) is empty");
        }
    }
}

#[test]
fn gas_building_categories_appear_independently_and_do_not_duplicate() {
    use Building::*;
    let mut planet = model().map.planets[0].clone();
    planet.kind = PlanetKind::Gas;
    let image = Handle::<Image>::default();
    let art = DevelopmentArt {
        base: &image,
        base_size: Vec2::splat(512.0),
        facilities: &image,
        facilities_size: Vec2::new(768.0, 512.0),
        gas: &image,
        gas_size: Vec2::splat(512.0),
        moon_shipyard: &image,
        moon_tidal_generator: &image,
        moon_orbital_radar: &image,
        planet_terraformer: &image,
        planet_administration: &image,
        gas_planet_terraformer: &image,
        gas_planet_administration: &image,
        surface_lights: &image,
        shadow: &image,
    };
    for (buildings, expected) in [
        (vec![], vec![]),
        (vec![MetalMine], vec![0]),
        (vec![CrystalMine], vec![0]),
        (vec![DeuteriumSynthesizer], vec![0]),
        (vec![Reactor], vec![0]),
        (vec![Shipyard], vec![1]),
        (vec![Factory], vec![1]),
        (vec![MissileSilo], vec![2]),
        (vec![SensorPhalanx], vec![]),
        (vec![Senate], vec![3]),
        (
            vec![
                MetalMine,
                CrystalMine,
                DeuteriumSynthesizer,
                Reactor,
                Shipyard,
                Factory,
                MissileSilo,
                Senate,
            ],
            vec![0, 1, 2, 3],
        ),
    ] {
        planet.surface_build_order = [None; 4];
        for &building in &buildings {
            planet.record_surface_building(building);
        }
        let army =
            Army::from_iter(buildings.into_iter().map(|building| (Unit::Building(building), 1)));
        let mut world = World::new();
        spawn_development(&mut world.commands(), &planet, development(&planet, Some(&army)), &art);
        world.flush();
        let mut actual = world
            .query::<&Sprite>()
            .iter(&world)
            .filter_map(|sprite| sprite.rect)
            .map(|rect| (rect.min.y / 256.0) as usize * 2 + (rect.min.x / 256.0) as usize)
            .collect::<Vec<_>>();
        actual.sort_unstable();
        assert_eq!(actual, expected);
    }
}

#[test]
fn planetary_senate_replaces_surface_sensor_artwork() {
    let mut planet = model().map.planets[0].clone();
    planet.kind = PlanetKind::Metallic;
    let image = Handle::<Image>::default();
    let art = DevelopmentArt {
        base: &image,
        base_size: Vec2::splat(512.0),
        facilities: &image,
        facilities_size: Vec2::new(768.0, 512.0),
        gas: &image,
        gas_size: Vec2::splat(512.0),
        moon_shipyard: &image,
        moon_tidal_generator: &image,
        moon_orbital_radar: &image,
        planet_terraformer: &image,
        planet_administration: &image,
        gas_planet_terraformer: &image,
        gas_planet_administration: &image,
        surface_lights: &image,
        shadow: &image,
    };
    for (building, expected) in
        [(Building::SensorPhalanx, None), (Building::Senate, Some((256, 0)))]
    {
        planet.surface_build_order = [None; 4];
        planet.record_surface_building(building);
        let army = Army::from([(Unit::Building(building), 1)]);
        let mut world = World::new();
        spawn_development(&mut world.commands(), &planet, development(&planet, Some(&army)), &art);
        world.flush();
        let actual = world
            .query::<&Sprite>()
            .iter(&world)
            .filter_map(|sprite| sprite.rect)
            .map(|rect| (rect.min.x as u32, rect.min.y as u32))
            .next();
        assert_eq!(actual, expected);
    }
}

#[test]
fn planetary_terraformer_art_uses_only_an_available_one_of_four_slots() {
    use Building::*;
    let image = Handle::<Image>::default();
    let robotics = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(4), default());
    let gas_robotics = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(5), default());
    let art = DevelopmentArt {
        base: &image,
        base_size: Vec2::splat(512.0),
        facilities: &image,
        facilities_size: Vec2::new(768.0, 512.0),
        gas: &image,
        gas_size: Vec2::splat(512.0),
        moon_shipyard: &image,
        moon_tidal_generator: &image,
        moon_orbital_radar: &image,
        planet_terraformer: &robotics,
        planet_administration: &robotics,
        gas_planet_terraformer: &gas_robotics,
        gas_planet_administration: &gas_robotics,
        surface_lights: &image,
        shadow: &image,
    };

    for kind in [PlanetKind::Metallic, PlanetKind::Gas] {
        let mut planet = model().map.planets[0].clone();
        planet.kind = kind;
        for (buildings, expected_robotics, expected_images) in [
            (vec![Terraformer], true, 1),
            (vec![MetalMine, Shipyard, MissileSilo, Terraformer], true, 4),
            (vec![MetalMine, Shipyard, MissileSilo, Senate, Terraformer], false, 4),
        ] {
            planet.surface_build_order = [None; 4];
            for &building in &buildings {
                planet.record_surface_building(building);
            }
            let army = Army::from_iter(
                buildings.into_iter().map(|building| (Unit::Building(building), 1)),
            );
            let mut world = World::new();
            spawn_development(
                &mut world.commands(),
                &planet,
                development(&planet, Some(&army)),
                &art,
            );
            world.flush();
            let structure_images = world
                .query::<(&Sprite, Option<&SurfaceLight>)>()
                .iter(&world)
                .filter_map(|(sprite, light)| {
                    light.is_none().then_some(sprite).filter(|sprite| {
                        sprite.rect.is_some()
                            || sprite.image.id() == robotics.id()
                            || sprite.image.id() == gas_robotics.id()
                    })
                })
                .collect::<Vec<_>>();

            assert_eq!(structure_images.len(), expected_images, "{kind:?}");
            let expected_robotics_image = if kind == PlanetKind::Gas {
                gas_robotics.id()
            } else {
                robotics.id()
            };
            assert_eq!(
                structure_images.iter().any(|sprite| sprite.image.id() == expected_robotics_image),
                expected_robotics,
                "{kind:?}"
            );
        }
    }
}

#[test]
fn completed_planetary_building_keeps_its_surface_position() {
    use Building::*;
    let image = Handle::<Image>::default();
    let robotics = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(4), default());
    let gas_robotics = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(5), default());
    let art = DevelopmentArt {
        base: &image,
        base_size: Vec2::splat(512.0),
        facilities: &image,
        facilities_size: Vec2::new(768.0, 512.0),
        gas: &image,
        gas_size: Vec2::splat(512.0),
        moon_shipyard: &image,
        moon_tidal_generator: &image,
        moon_orbital_radar: &image,
        planet_terraformer: &robotics,
        planet_administration: &robotics,
        gas_planet_terraformer: &gas_robotics,
        gas_planet_administration: &gas_robotics,
        surface_lights: &image,
        shadow: &image,
    };
    let robotics_position = |planet: &Planet| {
        let robotics_image = if planet.kind == PlanetKind::Gas {
            gas_robotics.id()
        } else {
            robotics.id()
        };
        let mut world = World::new();
        spawn_development(
            &mut world.commands(),
            planet,
            development(planet, Some(&planet.army)),
            &art,
        );
        world.flush();
        world
            .query::<(&Sprite, &Transform)>()
            .iter(&world)
            .find_map(|(sprite, transform)| {
                (sprite.image.id() == robotics_image).then_some(transform.translation)
            })
            .unwrap()
    };

    for kind in [PlanetKind::Metallic, PlanetKind::Gas] {
        let mut planet = model().map.planets[0].clone();
        planet.kind = kind;
        planet.army.clear();
        planet.surface_build_order = [None; 4];
        planet.buy = vec![Unit::Building(Terraformer)];
        planet.produce();
        let before = robotics_position(&planet);

        planet.buy = vec![Unit::Building(MetalMine)];
        planet.produce();
        let after = robotics_position(&planet);

        assert_eq!(before, after, "{kind:?}");
        assert_eq!(planet.surface_build_order, [Some(Terraformer), Some(MetalMine), None, None]);
    }
}

#[test]
fn moon_artwork_uses_completed_lunar_buildings_with_a_three_image_limit() {
    use Building::*;
    let mut planet = model().map.planets[0].clone();
    planet.kind = PlanetKind::Gray;
    let image = Handle::<Image>::default();
    let moon_shipyard = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(1), default());
    let moon_tidal_generator = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(2), default());
    let moon_orbital_radar = Handle::Uuid(bevy::asset::uuid::Uuid::from_u128(3), default());
    let art = DevelopmentArt {
        base: &image,
        base_size: Vec2::splat(512.0),
        facilities: &image,
        facilities_size: Vec2::new(768.0, 512.0),
        gas: &image,
        gas_size: Vec2::splat(512.0),
        moon_shipyard: &moon_shipyard,
        moon_tidal_generator: &moon_tidal_generator,
        moon_orbital_radar: &moon_orbital_radar,
        planet_terraformer: &image,
        planet_administration: &image,
        gas_planet_terraformer: &image,
        gas_planet_administration: &image,
        surface_lights: &image,
        shadow: &image,
    };
    for (buildings, mut expected_atlas, mut expected_dedicated) in [
        (vec![], vec![], vec![]),
        (vec![LunarBase], vec![(0, 256)], vec![]),
        (vec![TidalGenerator], vec![], vec![TidalGenerator]),
        (vec![OrbitalRadar], vec![], vec![OrbitalRadar]),
        (vec![Laboratory], vec![(512, 0)], vec![]),
        (vec![Shipyard], vec![], vec![Shipyard]),
        (vec![Factory, MetalMine, MissileSilo], vec![], vec![]),
        (vec![LunarBase, OrbitalRadar, Shipyard], vec![(0, 256)], vec![OrbitalRadar, Shipyard]),
        (
            vec![LunarBase, OrbitalRadar, Laboratory, Shipyard],
            vec![(0, 256), (512, 0)],
            vec![OrbitalRadar],
        ),
        (
            vec![Shipyard, Laboratory, OrbitalRadar, LunarBase],
            vec![(512, 0)],
            vec![Shipyard, OrbitalRadar],
        ),
        (
            vec![LunarBase, Shipyard, LunarBase, OrbitalRadar, Laboratory],
            vec![(0, 256)],
            vec![Shipyard, OrbitalRadar],
        ),
        (
            vec![LunarBase, Shipyard, TidalGenerator, OrbitalRadar, Laboratory],
            vec![(0, 256)],
            vec![TidalGenerator, Shipyard],
        ),
    ] {
        planet.army.clear();
        planet.surface_build_order = [None; 4];
        planet.buy = buildings.into_iter().map(Unit::Building).collect();
        planet.produce();
        let mut world = World::new();
        spawn_development(
            &mut world.commands(),
            &planet,
            development(&planet, Some(&planet.army)),
            &art,
        );
        world.flush();
        let mut actual_atlas = world
            .query::<(&Sprite, Option<&SurfaceLight>)>()
            .iter(&world)
            .filter_map(|(sprite, light)| light.is_none().then_some(sprite.rect).flatten())
            .map(|rect| (rect.min.x as u32, rect.min.y as u32))
            .collect::<Vec<_>>();
        let mut actual_dedicated = world
            .query::<&Sprite>()
            .iter(&world)
            .filter_map(|sprite| {
                if sprite.image.id() == moon_shipyard.id() {
                    Some(Shipyard)
                } else if sprite.image.id() == moon_tidal_generator.id() {
                    Some(TidalGenerator)
                } else if sprite.image.id() == moon_orbital_radar.id() {
                    Some(OrbitalRadar)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        actual_atlas.sort_unstable();
        expected_atlas.sort_unstable();
        actual_dedicated.sort_unstable();
        expected_dedicated.sort_unstable();
        assert_eq!(actual_atlas, expected_atlas);
        assert_eq!(actual_dedicated, expected_dedicated);
        assert!(actual_atlas.len() + actual_dedicated.len() <= 3);
    }
}

fn report(id: u64, turn: usize, planet: &Planet, losses: usize) -> MissionReport {
    MissionReport {
        id,
        turn,
        mission: Mission::new_with_id(
            id,
            1,
            1,
            planet,
            planet,
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::LightFighter), losses)]),
            BombingRaid::None,
            false,
            false,
            None,
        ),
        planet: planet.clone(),
        scout_probes: 0,
        surviving_attacker: Army::new(),
        surviving_defender: planet.army.clone(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: planet.owned,
        destination_controlled: planet.controlled,
        combat_report: Some(CombatReport::default()),
        hidden: false,
    }
}

#[test]
fn public_debris_deduplicates_participant_reports_and_expires_by_turn() {
    let model = model();
    let planet = &model.map.planets[0];
    let old = report(1, 5, planet, 4);
    let mut recent = report(2, 7, planet, 12);
    recent.hidden = true; // Public aftermath does not depend on report visibility.
    let sites = debris_sites([&old, &old, &recent].into_iter(), 7);
    assert_eq!(sites[&planet.id].losses, 12);
    assert_eq!(sites[&planet.id].latest_turn, 7);
    assert_eq!(debris_sites([&old, &recent].into_iter(), 8)[&planet.id].losses, 12);
    assert!(debris_sites([&old, &recent].into_iter(), 10).is_empty());
    assert!(debris_sites([&recent].into_iter(), 6).is_empty());
    assert_eq!(sites, debris_sites([&recent, &old].into_iter(), 7));
}

#[test]
fn debris_requires_actual_ship_losses_and_is_bounded() {
    let model = model();
    let mut battle = report(1, 2, &model.map.planets[0], 0);
    battle.mission.army.insert(Unit::colony_ship(), 1);
    battle.mission.army.insert(Unit::interplanetary_missile(), 20);
    battle.planet.army.insert(Unit::Building(Building::Factory), 5);
    assert_eq!(destroyed_units(&battle), 0);
    battle.mission.army.insert(Unit::probe(), 6);
    battle.surviving_attacker.insert(Unit::probe(), 4);
    battle.scout_probes = 4;
    assert_eq!(destroyed_units(&battle), 2);
    let small = debris_visuals(DebrisSize::Small);
    let medium = debris_visuals(DebrisSize::Medium);
    let large = debris_visuals(DebrisSize::Large);
    assert!(small.pieces < medium.pieces && medium.pieces < large.pieces);
    let radial_span =
        |visuals: DebrisVisuals| visuals.radial_step * visuals.pieces.saturating_sub(1) as f32;
    assert!(radial_span(small) < radial_span(medium));
    assert!(radial_span(medium) < radial_span(large));
    assert!(large.angle_step * ((large.pieces - 1) as f32) < 0.1);
    assert!(small.min_radius < medium.min_radius && medium.min_radius < large.min_radius);
    assert!(small.min_diameter < medium.min_diameter);
    assert!(medium.min_diameter < large.min_diameter);
    battle.combat_report = None;
    assert_eq!(destroyed_units(&battle), 0);
}

#[test]
fn escaping_ships_do_not_become_public_battle_wreckage() {
    let model = model();
    let mut battle = report(1, 2, &model.map.planets[0], 0);
    battle.planet.army.insert(Unit::probe(), 10);
    battle.surviving_defender.insert(Unit::probe(), 2);
    battle.combat_report.as_mut().unwrap().defender_retreat =
        Some(crate::core::combat::report::DefenderRetreat {
            after_round: Some(0),
            home_planet: 1,
            ships: Army::from([(Unit::probe(), 7)]),
        });
    assert_eq!(destroyed_units(&battle), 1);
}

#[test]
fn development_requires_known_completed_buildings_and_disappears_on_destruction() {
    let model = model();
    let mut planet = model.map.planets[0].clone();
    planet.army = Army::from([(Unit::Building(Building::MetalMine), 5)]).into();
    assert_eq!(development(&planet, None), Development::default());
    assert_eq!(development(&planet, Some(&planet.army)).settlement, 3);
    let known = Army::from([(Unit::Building(Building::MetalMine), 2)]);
    assert_eq!(development(&planet, Some(&known)).settlement, 1);
    planet.buy.push(Unit::Building(Building::Factory));
    assert_eq!(development(&planet, Some(&planet.army)).factory, 0);
    planet.army.insert(Unit::Building(Building::Shipyard), 3);
    assert_eq!(development(&planet, Some(&planet.army)).shipyard, 2);
    assert_eq!(development(&planet, Some(&planet.army)).factory, 0);
    planet.army.insert(Unit::Building(Building::Factory), 1);
    planet.army.insert(Unit::Building(Building::Terraformer), 5);
    planet.army.insert(Unit::Building(Building::Reactor), 4);
    planet.army.insert(Unit::Building(Building::Laboratory), 5);
    planet.army.insert(Unit::Building(Building::Senate), 1);
    let visible = development(&planet, Some(&planet.army));
    assert_eq!(visible.factory, 1);
    assert_eq!(visible.terraformer, 3);
    assert_eq!(visible.shipyard, 2);
    assert_eq!(visible.reactor, 2);
    assert_eq!(visible.laboratory, 3);
    assert!(visible.senate);
    planet.is_destroyed = true;
    assert_eq!(development(&planet, Some(&planet.army)), Development::default());
}

#[test]
fn clicking_uses_report_order_and_never_grants_missing_intelligence() {
    let model = model();
    let mut player = model.players[0].clone();
    let mut planet = model.map.planets[0].clone();
    planet.controlled = Some(player.id);
    planet.owned = Some(player.id);
    player.reports = vec![report(900, 2, &planet, 1), report(2, 2, &planet, 2)];
    let mut state = UiState::default();
    open_latest_battle(&player, planet.id, &mut state);
    assert_eq!(state.combat_report, Some(2)); // Random IDs are not chronology.
    assert_eq!(state.combat_report_round, 1);
    assert!(state.mission);
    assert_eq!(state.mission_tab, MissionTab::MissionReports);
    assert_eq!(state.mission_report, Some(2));
    player.id = 3;
    state = UiState::default();
    open_latest_battle(&player, planet.id, &mut state);
    assert!(state.combat_report.is_none());
    assert_eq!(state.mission_tab, MissionTab::MissionReports);
    player.reports.clear();
    state = UiState::default();
    open_latest_battle(&player, planet.id, &mut state);
    assert!(!state.mission);
    assert!(state.combat_report.is_none());
}

#[test]
fn light_clusters_relocate_only_while_dark_and_stay_on_the_surface() {
    for seed in 0_u32..24 {
        let flicker_seed = seed.wrapping_mul(17).wrapping_add(5);
        let cycle_seconds = 11.0 + noise(seed.wrapping_add(3)) * 8.0;
        let boundary = (1.0 - noise(seed)) * cycle_seconds;
        let (before, alpha_before) = light_sample(seed, flicker_seed, boundary - 0.001);
        let (after, alpha_after) = light_sample(seed, flicker_seed, boundary + 0.001);
        assert!(alpha_before < 0.001 && alpha_after < 0.001);
        assert!(!before.abs_diff_eq(after, 0.0001));
        let (stable, bright) = light_sample(seed, flicker_seed, boundary + 4.0);
        let (still, shimmer) = light_sample(seed, flicker_seed, boundary + 5.0);
        assert_eq!(stable, still);
        assert!((0.5..=1.0).contains(&bright) && (0.5..=1.0).contains(&shimmer));
        for seconds in [0.0, 13.0, 40.0, 71.0] {
            assert!(light_sample(seed, flicker_seed, seconds).0.length() <= 0.381);
        }
    }
}

#[test]
fn surface_lights_use_sparse_varied_clusters() {
    let mut planet = model().map.planets[0].clone();
    planet.kind = PlanetKind::Metallic;
    let image = Handle::<Image>::default();
    let art = DevelopmentArt {
        base: &image,
        base_size: Vec2::splat(512.0),
        facilities: &image,
        facilities_size: Vec2::new(768.0, 512.0),
        gas: &image,
        gas_size: Vec2::splat(512.0),
        moon_shipyard: &image,
        moon_tidal_generator: &image,
        moon_orbital_radar: &image,
        planet_terraformer: &image,
        planet_administration: &image,
        gas_planet_terraformer: &image,
        gas_planet_administration: &image,
        surface_lights: &image,
        shadow: &image,
    };
    let mut world = World::new();
    spawn_development(
        &mut world.commands(),
        &planet,
        Development {
            settlement: 3,
            ..default()
        },
        &art,
    );
    world.flush();

    let lights = world
        .query::<(&SurfaceLight, &Sprite, Option<&SurfaceLightBackdrop>)>()
        .iter(&world)
        .filter(|(_, _, backdrop)| backdrop.is_none())
        .map(|(light, sprite, _)| {
            (
                light.position_seed,
                light.flicker_seed,
                sprite.rect.unwrap().min.x as u32,
                sprite.custom_size.unwrap(),
            )
        })
        .collect::<Vec<_>>();
    assert!((18..=27).contains(&lights.len()));
    assert_eq!(world.query::<&SurfaceLightBackdrop>().iter(&world).count(), lights.len());
    assert_eq!(lights.iter().map(|light| light.0).collect::<BTreeSet<_>>().len(), 9);
    assert!(lights.iter().map(|light| light.1).collect::<BTreeSet<_>>().len() > 12);
    assert!(lights.iter().map(|light| light.2).collect::<BTreeSet<_>>().len() >= 3);
    assert!(
        lights
            .iter()
            .map(|light| (light.3.x.to_bits(), light.3.y.to_bits()))
            .collect::<BTreeSet<_>>()
            .len()
            > 12
    );
}

#[test]
fn light_animation_pauses_without_changing_planet_state() {
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<DetailAnimationTime>()
        .insert_resource(State::new(GameState::Playing))
        .add_systems(Update, animate_surface_lights);
    let entity = app
        .world_mut()
        .spawn((
            SurfaceLight {
                center: Vec2::new(20.0, -10.0),
                diameter: 100.0,
                position_seed: 51,
                flicker_seed: 73,
                offset: Vec2::ZERO,
                brightness: 0.0,
            },
            Transform::default(),
        ))
        .id();
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(3));
    app.update();
    let position = app.world().get::<Transform>(entity).unwrap().translation;
    let brightness = app.world().get::<SurfaceLight>(entity).unwrap().brightness;
    app.world_mut().insert_resource(State::new(GameState::GameMenu));
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(20));
    app.update();
    assert_eq!(app.world().get::<Transform>(entity).unwrap().translation, position);
    assert_eq!(app.world().get::<SurfaceLight>(entity).unwrap().brightness, brightness);
    assert_eq!(app.world().resource::<DetailAnimationTime>().0, 3.0);
}

#[test]
fn debris_motion_stays_near_its_anchor_and_pauses_with_detail_animation() {
    let planet = model().map.planets[0].clone();
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<DetailAnimationTime>()
        .insert_resource(State::new(GameState::Playing))
        .add_systems(Update, (animate_surface_lights, animate_debris).chain());
    spawn_debris(
        &mut app.world_mut().commands(),
        &planet,
        &DebrisSite {
            losses: 256,
            latest_turn: 1,
            seed: 71,
        },
        Handle::default(),
        Vec2::splat(512.0),
    );
    app.world_mut().flush();
    let entities = app
        .world_mut()
        .query_filtered::<Entity, With<Debris>>()
        .iter(app.world())
        .collect::<Vec<_>>();
    let phalanx_anchor = planet.position + Vec2::from_angle(TAU * 0.375) * planet.size() * 0.83;
    let relay_anchor = planet.position + Vec2::from_angle(TAU * 0.625) * planet.size() * 0.83;
    let phalanx_radius = Vec2::splat(planet.size() * 0.23).length() * 0.5;
    let phalanx_drift = Vec2::new(10.7, 7.6).length();
    let relay_radius = Vec2::splat(planet.size() * 0.34).length() * 0.5;
    let relay_drift = Vec2::new(1.1, 2.4).length();
    for &entity in &entities {
        let debris = app.world().get::<Debris>(entity).unwrap();
        let debris_radius =
            app.world().get::<Sprite>(entity).unwrap().custom_size.unwrap().length() * 0.5;
        let debris_drift = Vec2::splat(debris.amplitude).length();
        let offset = debris.origin - planet.position;
        assert!(offset.x < -planet.size() * 0.94, "debris must stay left of the planet");
        assert!(offset.y.abs() < planet.size() * 0.31, "debris must use the open left corridor");
        assert!(
            debris.origin.distance(phalanx_anchor)
                > debris_radius + debris_drift + phalanx_radius + phalanx_drift,
            "debris must remain clear of the Sensor Phalanx throughout both idle animations"
        );
        assert!(
            debris.origin.distance(relay_anchor)
                > debris_radius + debris_drift + relay_radius + relay_drift,
            "debris must remain clear of the Command Relay throughout both idle animations"
        );
    }
    app.update();
    let initial = *app.world().get::<Transform>(entities[0]).unwrap();
    for _ in 0..120 {
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(1));
        app.update();
        for &entity in &entities {
            let debris = app.world().get::<Debris>(entity).unwrap();
            let transform = app.world().get::<Transform>(entity).unwrap();
            assert!(
                transform.translation.truncate().distance(debris.origin) < planet.size() * 0.026
            );
            assert_eq!(transform.translation.z, PLANET_Z + DEBRIS_DEPTH);
            assert!(
                transform.rotation.angle_between(Quat::from_rotation_z(debris.rotation)) < 0.101
            );
        }
    }
    let moved = *app.world().get::<Transform>(entities[0]).unwrap();
    assert_ne!(moved.translation, initial.translation);
    assert_ne!(moved.rotation, initial.rotation);
    app.world_mut().insert_resource(State::new(GameState::GameMenu));
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(20));
    app.update();
    assert_eq!(*app.world().get::<Transform>(entities[0]).unwrap(), moved);
}

#[test]
fn zoom_and_age_disable_invisible_hit_targets() {
    let mut app = App::new();
    app.init_resource::<Time<Real>>();
    app.insert_resource(Settings {
        turn: 4,
        ..default()
    })
    .insert_resource(State::new(GameState::Playing))
    .add_systems(Update, fade_details);
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = 0.5;
    let camera = app.world_mut().spawn((MainCamera, Projection::Orthographic(projection))).id();
    let detail = app
        .world_mut()
        .spawn((
            Detail {
                planet: 1,
                opacity: 0.9,
                debris_turn: Some((2, 3)),
            },
            Sprite::default(),
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .id();
    let hit_target = app
        .world_mut()
        .spawn((
            DebrisHitTarget {
                planet: 1,
                site: DebrisSite {
                    losses: 16,
                    latest_turn: 2,
                    seed: 73,
                },
            },
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .id();
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(std::time::Duration::from_secs_f32(ZOOM_DETAIL_FADE_SECONDS));
    app.update();
    assert_eq!(*app.world().get::<Visibility>(detail).unwrap(), Visibility::Inherited);
    assert!(!app.world().get::<Pickable>(detail).unwrap().is_hoverable);
    assert_eq!(*app.world().get::<Visibility>(hit_target).unwrap(), Visibility::Inherited);
    assert!(app.world().get::<Pickable>(hit_target).unwrap().is_hoverable);
    if let Projection::Orthographic(projection) =
        &mut *app.world_mut().get_mut::<Projection>(camera).unwrap()
    {
        projection.scale = 1.4;
    }
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(std::time::Duration::from_secs_f32(ZOOM_DETAIL_FADE_SECONDS * 0.5));
    app.update();
    assert_eq!(*app.world().get::<Visibility>(detail).unwrap(), Visibility::Inherited);
    assert!(app.world().get::<Pickable>(hit_target).unwrap().is_hoverable);
    let fading_alpha = app.world().get::<Sprite>(detail).unwrap().color.alpha();
    assert!(fading_alpha > 0.0 && fading_alpha < 0.9);
    app.world_mut()
        .resource_mut::<Time<Real>>()
        .advance_by(std::time::Duration::from_secs_f32(ZOOM_DETAIL_FADE_SECONDS));
    app.update();
    assert_eq!(*app.world().get::<Visibility>(detail).unwrap(), Visibility::Hidden);
    assert!(!app.world().get::<Pickable>(detail).unwrap().is_hoverable);
    assert_eq!(*app.world().get::<Visibility>(hit_target).unwrap(), Visibility::Hidden);
    assert!(!app.world().get::<Pickable>(hit_target).unwrap().is_hoverable);
    app.world_mut().resource_mut::<Settings>().turn = 5;
    app.update();
    assert_eq!(app.world().get::<Sprite>(detail).unwrap().color.alpha(), 0.0);
}

#[test]
fn debris_art_is_not_the_hover_target() {
    let planet = model().map.planets[0].clone();
    let mut app = App::new();
    spawn_debris(
        &mut app.world_mut().commands(),
        &planet,
        &DebrisSite {
            losses: 16,
            latest_turn: 2,
            seed: 73,
        },
        Handle::default(),
        Vec2::splat(512.0),
    );
    app.world_mut().flush();

    let debris = app
        .world_mut()
        .query_filtered::<(&Sprite, &Pickable), With<Debris>>()
        .iter(app.world())
        .collect::<Vec<_>>();
    assert_eq!(debris.len(), debris_visuals(DebrisSize::Large).pieces);
    assert!(debris.iter().all(|(_, pickable)| !pickable.is_hoverable));

    let targets = app
        .world_mut()
        .query_filtered::<(&Sprite, &Pickable), With<DebrisHitTarget>>()
        .iter(app.world())
        .collect::<Vec<_>>();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].0.color.alpha(), 0.0);
    assert!(!targets[0].1.is_hoverable);
}

#[test]
fn development_fades_finish_and_can_reverse_mid_transition() {
    let mut fade = DevelopmentVisibility::default();
    let halfway = fade.update(true, 0.11);
    assert!((halfway - 0.5).abs() < 0.001);
    assert_eq!(fade.update(true, 0.22), 1.0);
    assert_eq!(fade.update(true, 1.0), 1.0);
    assert!((fade.update(false, 0.11) - 0.5).abs() < 0.001);
    assert!((fade.update(true, 0.055) - 0.75).abs() < 0.001);
    assert_eq!(fade.update(false, 0.22), 0.0);
    assert_eq!(fade.update(false, 1.0), 0.0);
}

#[test]
fn zoom_details_follow_the_debris_curve_and_settle_gradually() {
    let mut fade = ZoomDetailVisibility::default();

    assert_eq!(fade.update(1.05, 1.0), 0.0);
    let halfway_in = fade.update(0.75, ZOOM_DETAIL_FADE_SECONDS * 0.5);
    assert!((0.7..0.8).contains(&halfway_in));
    let nearly_visible = fade.update(0.75, ZOOM_DETAIL_FADE_SECONDS * 0.5);
    assert!((0.94..0.96).contains(&nearly_visible));

    let halfway_out = fade.update(1.05, ZOOM_DETAIL_FADE_SECONDS * 0.5);
    assert!((0.2..0.22).contains(&halfway_out));
    assert_eq!(fade.update(1.05, ZOOM_DETAIL_FADE_SECONDS * 1.1), 0.0);

    let mut midpoint_fade = ZoomDetailVisibility::default();
    let midpoint = midpoint_fade.update(0.9, ZOOM_DETAIL_FADE_SECONDS);
    assert!((0.47..0.49).contains(&midpoint));
}

#[test]
fn debris_fields_form_long_shallow_plumes() {
    for size in [DebrisSize::Small, DebrisSize::Medium, DebrisSize::Large] {
        let visuals = debris_visuals(size);
        let radial_span = visuals.radial_step * visuals.pieces.saturating_sub(1) as f32;
        let angular_span = visuals.angle_step * visuals.pieces.saturating_sub(1) as f32;

        assert!(radial_span > angular_span * 3.0);
        assert!(radial_span >= 0.16);
    }
}

#[test]
fn stationary_zoom_finishes_building_fades_in_both_directions() {
    let mut app = App::new();
    app.init_resource::<Time<Real>>()
        .init_resource::<Settings>()
        .insert_resource(State::new(GameState::Playing))
        .add_systems(Update, fade_details);
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = 0.85;
    let camera = app.world_mut().spawn((MainCamera, Projection::Orthographic(projection))).id();
    let building = app
        .world_mut()
        .spawn((
            Detail {
                planet: 1,
                opacity: 1.0,
                debris_turn: None,
            },
            Sprite::default(),
            Visibility::Hidden,
            Pickable::IGNORE,
        ))
        .id();
    app.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_millis(60));
    app.update();
    let alpha = app.world().get::<Sprite>(building).unwrap().color.alpha();
    assert!(alpha > 0.0 && alpha < 1.0);
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(app.world().get::<Sprite>(building).unwrap().color.alpha(), 1.0);
    if let Projection::Orthographic(projection) =
        &mut *app.world_mut().get_mut::<Projection>(camera).unwrap()
    {
        projection.scale = 0.95;
    }
    app.update();
    let alpha = app.world().get::<Sprite>(building).unwrap().color.alpha();
    assert!(alpha > 0.0 && alpha < 1.0);
    for _ in 0..4 {
        app.update();
    }
    assert_eq!(app.world().get::<Sprite>(building).unwrap().color.alpha(), 0.0);
    assert_eq!(*app.world().get::<Visibility>(building).unwrap(), Visibility::Hidden);
}

#[test]
fn projection_reuses_entities_and_removes_expired_debris() {
    let model = model();
    let mut player = model.players[0].clone();
    player.reports.push(report(40, 2, model.map.get(player.home_planet), 16));
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<WorldAssets>()
        .init_resource::<DetailCache>()
        .init_resource::<StructureShadow>()
        .init_resource::<SurfaceLightAtlas>()
        .init_resource::<MultiplayerSession>()
        .insert_resource(Missions(Vec::new()))
        .insert_resource(Settings {
            turn: 2,
            ..default()
        })
        .insert_resource(model.map)
        .insert_resource(player)
        .add_systems(Update, refresh_details);
    let image = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
    app.world_mut().resource_mut::<WorldAssets>().images.insert("wreckage".into(), image.clone());
    app.world_mut()
        .resource_mut::<WorldAssets>()
        .images
        .insert("development".into(), image.clone());
    app.world_mut().resource_mut::<WorldAssets>().images.insert("facilities".into(), image.clone());
    app.world_mut().resource_mut::<WorldAssets>().images.insert("gas-development".into(), image);
    app.update();
    let debris = |app: &mut App| {
        app.world_mut()
            .query_filtered::<Entity, With<Debris>>()
            .iter(app.world())
            .collect::<Vec<_>>()
    };
    let debris_targets = |app: &mut App| {
        app.world_mut()
            .query_filtered::<Entity, With<DebrisHitTarget>>()
            .iter(app.world())
            .collect::<Vec<_>>()
    };
    let original = debris(&mut app);
    assert_eq!(original.len(), debris_visuals(DebrisSize::Large).pieces);
    let original_target = debris_targets(&mut app);
    assert_eq!(original_target.len(), 1);
    app.update();
    assert_eq!(debris(&mut app), original);
    assert_eq!(debris_targets(&mut app), original_target);
    app.world_mut().resource_mut::<Settings>().show_cells = false;
    app.update();
    assert_eq!(debris(&mut app), original);
    app.world_mut().resource_mut::<Settings>().turn = 5;
    app.update();
    assert!(debris(&mut app).is_empty());
    assert!(debris_targets(&mut app).is_empty());
    assert_eq!(app.world().resource::<Player>().reports.len(), 1);
}

/// Opt-in GPU check; writes only to ignored build output and never connects to a backend.
#[test]
#[ignore = "opens a native render fixture; requires a GPU and generated runtime assets"]
fn render_development_and_wreckage() {
    use crate::core::basis_texture::BasisTexturePlugin;
    use bevy::app::AppExit;
    use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
    use bevy::winit::WinitPlugin;

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: format!("{}/assets-runtime", env!("CARGO_MANIFEST_DIR")),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Stellarion detail fixture".into(),
                    resolution: (1200, 820).into(),
                    ..default()
                }),
                ..default()
            })
            .set(WinitPlugin {
                run_on_any_thread: true,
            }),
    )
    .add_plugins(BasisTexturePlugin)
    .init_resource::<StructureShadow>()
    .init_resource::<SurfaceLightAtlas>()
    .init_resource::<DetailAnimationTime>()
    .insert_resource(ClearColor(Color::srgb(0.012, 0.02, 0.035)))
    .insert_resource(Settings {
        turn: 4,
        ..default()
    })
    .insert_resource(State::new(GameState::Playing))
    .insert_resource(model().map)
    .add_systems(Startup, |mut commands: Commands| {
        let mut projection = OrthographicProjection::default_2d();
        projection.scale = 0.5;
        commands.spawn((Camera2d, Projection::Orthographic(projection), MainCamera));
    })
    .add_systems(
        Update,
        (
            |mut commands: Commands,
             map: Res<Map>,
             server: Res<AssetServer>,
             images: Res<Assets<Image>>,
             mut loaded: Local<Option<Handle<Image>>>,
             mut development_loaded: Local<Option<Handle<Image>>>,
             mut facilities_loaded: Local<Option<Handle<Image>>>,
             mut gas_loaded: Local<Option<Handle<Image>>>,
             shadow: Res<StructureShadow>,
             surface_lights: Res<SurfaceLightAtlas>,
             mut spawned: Local<bool>| {
                let image = loaded
                    .get_or_insert_with(|| server.load("images/ambient/wreckage.basisu.ktx2"));
                let Some(texture) = images.get(&*image) else {
                    return;
                };
                let development_image = development_loaded.get_or_insert_with(|| {
                    server
                        .load_builder()
                        .with_settings(
                            |settings: &mut crate::core::basis_texture::BasisTextureSettings| {
                                settings.linear_filtering = true;
                            },
                        )
                        .load("images/planet-buildings/development.basisu.ktx2")
                });
                let Some(development_texture) = images.get(&*development_image) else {
                    return;
                };
                let facilities_image = facilities_loaded.get_or_insert_with(|| {
                    server
                        .load_builder()
                        .with_settings(
                            |settings: &mut crate::core::basis_texture::BasisTextureSettings| {
                                settings.linear_filtering = true;
                            },
                        )
                        .load("images/planet-buildings/facilities.basisu.ktx2")
                });
                let Some(facilities_texture) = images.get(&*facilities_image) else {
                    return;
                };
                let gas_image =
                    gas_loaded.get_or_insert_with(|| {
                        server.load_builder().with_settings(
                        |settings: &mut crate::core::basis_texture::BasisTextureSettings| {
                            settings.linear_filtering = true;
                        },
                    ).load("images/planet-buildings/gas-development.basisu.ktx2")
                    });
                let Some(gas_texture) = images.get(&*gas_image) else {
                    return;
                };
                let moon_shipyard = server.load("images/moon-buildings/shipyard.basisu.ktx2");
                let moon_tidal_generator =
                    server.load("images/moon-buildings/tidal generator.basisu.ktx2");
                let moon_orbital_radar =
                    server.load("images/moon-buildings/orbital radar.basisu.ktx2");
                let planet_robotics =
                    server.load("images/planet-buildings/terraformer.basisu.ktx2");
                let gas_planet_robotics =
                    server.load("images/planet-buildings/terraformer gas.basisu.ktx2");
                let art = DevelopmentArt {
                    base: development_image,
                    base_size: development_texture.size().as_vec2(),
                    facilities: facilities_image,
                    facilities_size: facilities_texture.size().as_vec2(),
                    gas: gas_image,
                    gas_size: gas_texture.size().as_vec2(),
                    moon_shipyard: &moon_shipyard,
                    moon_tidal_generator: &moon_tidal_generator,
                    moon_orbital_radar: &moon_orbital_radar,
                    planet_terraformer: &planet_robotics,
                    planet_administration: &planet_robotics,
                    gas_planet_terraformer: &gas_planet_robotics,
                    gas_planet_administration: &gas_planet_robotics,
                    surface_lights: &surface_lights.0,
                    shadow: &shadow.0,
                };
                if *spawned {
                    return;
                }
                *spawned = true;
                for (column, kind) in [PlanetKind::Metallic, PlanetKind::Gas, PlanetKind::Gray]
                    .into_iter()
                    .enumerate()
                {
                    for row in 0..2 {
                        let mut planet = map
                            .planets
                            .iter()
                            .find(|planet| planet.kind == kind)
                            .cloned()
                            .unwrap_or_else(|| map.planets[0].clone());
                        planet.kind = kind;
                        planet.image = kind.indices()[0];
                        planet.id = (column * 2 + row + 1) as PlanetId;
                        planet.position =
                            Vec2::new(-195.0 + column as f32 * 195.0, 90.0 - row as f32 * 180.0);
                        commands.spawn((
                            Sprite {
                                image: server
                                    .load(format!("images/planets/{}.basisu.ktx2", planet.image())),
                                custom_size: Some(Vec2::splat(planet.size())),
                                ..default()
                            },
                            Transform::from_translation(planet.position.extend(PLANET_Z)),
                        ));
                        spawn_development(
                            &mut commands,
                            &planet,
                            Development {
                                administration: 0,
                                settlement: if row == 0 {
                                    1
                                } else {
                                    3
                                },
                                mining: if planet.is_moon() {
                                    0
                                } else if row == 0 {
                                    1
                                } else {
                                    3
                                },
                                refinery: if planet.is_moon() {
                                    0
                                } else if row == 0 {
                                    1
                                } else {
                                    3
                                },
                                factory: if row == 0 {
                                    1
                                } else {
                                    3
                                },
                                terraformer: if !planet.is_moon() && row == 1 {
                                    3
                                } else {
                                    0
                                },
                                shipyard: if row == 0 {
                                    1
                                } else {
                                    3
                                },
                                reactor: if row == 0 {
                                    0
                                } else {
                                    3
                                },
                                laboratory: if planet.is_moon() && row == 1 {
                                    3
                                } else {
                                    0
                                },
                                silo: if row == 0 {
                                    0
                                } else {
                                    3
                                },
                                lunar_base: if planet.is_moon() {
                                    3
                                } else {
                                    0
                                },
                                tidal_generator: usize::from(planet.is_moon()) * 3,
                                senate: !planet.is_moon(),
                                sensor: row == 1 || kind == PlanetKind::Gas,
                                surface_build_order: if planet.is_moon() {
                                    if row == 0 {
                                        [
                                            Some(Building::Shipyard),
                                            Some(Building::OrbitalRadar),
                                            Some(Building::Laboratory),
                                            Some(Building::LunarBase),
                                        ]
                                    } else {
                                        [
                                            Some(Building::LunarBase),
                                            Some(Building::TidalGenerator),
                                            Some(Building::OrbitalRadar),
                                            Some(Building::Laboratory),
                                        ]
                                    }
                                } else {
                                    [
                                        Some(Building::MetalMine),
                                        Some(Building::Shipyard),
                                        Some(Building::MissileSilo),
                                        Some(Building::Senate),
                                    ]
                                },
                            },
                            &art,
                        );
                        spawn_debris(
                            &mut commands,
                            &planet,
                            &DebrisSite {
                                losses: if row == 0 {
                                    2
                                } else {
                                    256
                                },
                                latest_turn: if row == 0 {
                                    4
                                } else {
                                    2
                                },
                                seed: column as u32 + 71,
                            },
                            image.clone(),
                            texture.size().as_vec2(),
                        );
                    }
                }
            },
            animate_surface_lights,
            animate_floating_development,
            animate_debris,
            fade_details,
            |mut commands: Commands, time: Res<Time>, mut captured: Local<bool>| {
                if time.elapsed_secs() > 5.0 && !*captured {
                    *captured = true;
                    commands
                        .spawn(Screenshot::primary_window())
                        .observe(save_to_disk("target/map-details-preview.png"))
                        .observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                            exit.write(AppExit::Success);
                        });
                }
            },
        )
            .chain(),
    );
    app.run();
}
