use super::*;
use crate::core::combat::report::MissionReport;
use crate::core::identity::{GameCode, GameId};
use crate::core::player::{PlayerColor, PLAYER_COLOR_PALETTE};
use crate::core::simulation::{GameModel, GameRules, PersistedGame};
use crate::multiplayer::model::GameRecord;
use bevy::color::ColorToComponents;
use bevy_kira_audio::AudioSource;

#[test]
fn destruction_animation_hides_the_planet_swap_under_the_blast() {
    let mut planet = Planet::new(0, "Cindra".into(), Vec2::ZERO, false, 1.0);
    let original_image = planet.image();
    planet.destroy();

    assert_eq!(map_planet_image(&planet, true), original_image);
    assert_eq!(map_planet_image(&planet, false), "planet0");

    planet.image = 0;
    assert_eq!(map_planet_image(&planet, true), "planet0");
}

#[test]
fn railgun_strike_icon_appears_for_planets_and_moons_before_a_shot_is_committed() {
    let mut model = GameModel::new([62; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = model.players[0].clone();
    let origin = player.home_planet;
    let target = model.players[1].home_planet;
    let moon = model.map.moons()[0].id;
    model.map.get_mut(origin).army.insert(Unit::Building(Building::OrbitalRailgun), 1);
    let origin_position = model.map.get(origin).position;
    model.map.get_mut(target).position = origin_position + Vec2::X * Planet::SIZE;
    let moon_world = model.map.get_mut(moon);
    moon_world.owned = None;
    moon_world.controlled = None;
    moon_world.position = origin_position + Vec2::Y * Planet::SIZE;
    let mut pending = PendingTurnCommands {
        turn: model.turn,
        ..default()
    };

    assert!(!railgun_action_available(&model.map, &player, Some(&pending), target, false));
    assert!(railgun_action_available(&model.map, &player, Some(&pending), target, true));
    assert!(railgun_action_available(&model.map, &player, Some(&pending), moon, true));
    assert!(pending.push(TurnCommand::FireOrbitalRailguns {
        target: moon,
    }));
    assert!(!railgun_action_available(&model.map, &player, Some(&pending), target, true));
}

/// Finds the defenses spawned by the real map setup without a window or GPU.
fn test_defenses(
    world: &mut World,
    planet: PlanetId,
) -> (Entity, Entity, Entity, Entity, Entity, Entity) {
    let planet = world
        .query::<(Entity, &PlanetCmp)>()
        .iter(world)
        .find(|(_, cmp)| cmp.id == planet)
        .unwrap()
        .0;
    let children = world.get::<Children>(planet).unwrap();
    let shield =
        children.iter().find(|&child| world.get::<PlanetaryShieldCmp>(child).is_some()).unwrap();
    let dock = children.iter().find(|&child| world.get::<SpaceDockCmp>(child).is_some()).unwrap();
    let gate = children.iter().find(|&child| world.get::<JumpGateCmp>(child).is_some()).unwrap();
    let satellite =
        children.iter().find(|&child| world.get::<SolarSatelliteCmp>(child).is_some()).unwrap();
    let relay =
        children.iter().find(|&child| world.get::<CommandRelayCmp>(child).is_some()).unwrap();
    let phalanx =
        children.iter().find(|&child| world.get::<SensorPhalanxCmp>(child).is_some()).unwrap();
    (shield, dock, gate, satellite, relay, phalanx)
}

fn test_satellites(world: &mut World, planet: PlanetId) -> Vec<Entity> {
    let planet = world
        .query::<(Entity, &PlanetCmp)>()
        .iter(world)
        .find(|(_, cmp)| cmp.id == planet)
        .unwrap()
        .0;
    let mut satellites = world
        .get::<Children>(planet)
        .unwrap()
        .iter()
        .filter(|&child| world.get::<SolarSatelliteCmp>(child).is_some())
        .collect::<Vec<_>>();
    satellites.sort_by_key(|entity| world.get::<SolarSatelliteCmp>(*entity).unwrap().level);
    satellites
}

fn test_railgun(world: &mut World, planet: PlanetId) -> Entity {
    let planet = world
        .query::<(Entity, &PlanetCmp)>()
        .iter(world)
        .find(|(_, cmp)| cmp.id == planet)
        .unwrap()
        .0;
    world
        .get::<Children>(planet)
        .unwrap()
        .iter()
        .find(|&child| world.get::<OrbitalRailgunCmp>(child).is_some())
        .unwrap()
}

fn test_phalanxes(world: &mut World, planet: PlanetId) -> Vec<Entity> {
    let planet = world
        .query::<(Entity, &PlanetCmp)>()
        .iter(world)
        .find(|(_, cmp)| cmp.id == planet)
        .unwrap()
        .0;
    let mut drones = world
        .get::<Children>(planet)
        .unwrap()
        .iter()
        .filter(|&child| world.get::<SensorPhalanxCmp>(child).is_some())
        .collect::<Vec<_>>();
    drones.sort_by_key(|entity| world.get::<SensorPhalanxCmp>(*entity).unwrap().index);
    drones
}

#[test]
fn ambience_wraps_stars_around_the_camera_without_moving_worlds() {
    let mut app = App::new();
    app.init_resource::<Time>().add_systems(Update, animate_map_ambience);
    app.world_mut().spawn((MainCamera, Transform::from_xyz(4_000.0, 0.0, 1.0)));
    let layer = app
        .world_mut()
        .spawn((ParallaxCmp::new(0.0, 1.0, 0.0, Vec2::ZERO), Transform::default()))
        .id();
    let star = app
        .world_mut()
        .spawn((
            AmbientStarCmp {
                anchor: Vec2::new(-2_000.0, 0.0),
                phase: -PI * 0.5,
                speed: 0.0,
                base_alpha: 0.6,
                minimum_alpha: 0.0,
                pulse_power: 3.4,
            },
            Sprite::default(),
            Transform::from_xyz(-2_000.0, 0.0, 0.0),
        ))
        .id();
    app.world_mut().entity_mut(layer).add_child(star);
    let planet_position = Vec3::new(123.0, 456.0, PLANET_Z);
    let planet = app
        .world_mut()
        .spawn((
            PlanetCmp::new(0),
            PlanetAmbienceCmp {
                phase: 0.0,
                minimum_brightness: 0.9,
            },
            Sprite::default(),
            Transform::from_translation(planet_position),
        ))
        .id();

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
    app.update();

    let star_x = app.world().get::<Transform>(star).unwrap().translation.x;
    assert!((star_x - 4_000.0).abs() <= AMBIENT_STAR_FIELD_SIZE.x * 0.5);
    assert_eq!(app.world().get::<Sprite>(star).unwrap().color.alpha(), 0.0);
    assert_eq!(app.world().get::<Transform>(planet).unwrap().translation, planet_position);
    let brightness = app.world().get::<Sprite>(planet).unwrap().color.to_srgba().red;
    assert!(brightness > 0.9 && brightness < 1.0);
}

#[test]
fn comet_streaks_fade_at_both_ends_and_keep_occasional_timing_bounded() {
    assert_eq!(comet_visibility(0.0), 0.0);
    assert_eq!(comet_visibility(1.0), 0.0);
    assert!(comet_visibility(0.2) > 0.99);
    assert!(comet_visibility(0.5) > 0.99);

    for sequence in 0..1_000 {
        let delay = next_comet_delay(sequence);
        assert!((1.4..=27.0).contains(&delay));
    }
}

#[test]
fn pulsars_flare_briefly_and_relocate_while_dark() {
    assert_eq!(pulsar_visibility(0.0), 0.0);
    assert!(pulsar_visibility(0.12) > 0.99);
    assert_eq!(pulsar_visibility(0.5), 0.0);
    assert_eq!(pulsar_visibility(1.0), 0.0);

    let first = pulsar_anchor(0x1234_5678, 0);
    let second = pulsar_anchor(0x1234_5678, 1);
    assert_ne!(first, second);
    assert!(first.x.abs() <= AMBIENT_PULSAR_FIELD_SIZE.x * 0.5);
    assert!(first.y.abs() <= AMBIENT_PULSAR_FIELD_SIZE.y * 0.5);
}

#[test]
fn scenery_keeps_the_sun_at_the_arc_origin_and_the_landmark_inside_the_opposite_edge() {
    let map = Map {
        rect: Rect::new(-1_600.0, -900.0, 1_600.0, 900.0),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: Vec::new(),
    };
    let corner = map_scenery_corner(&map);
    let sun_corner_position = map_corner(&map, corner);
    let position = solar_star_position(&map);
    let outside = (position - sun_corner_position) * corner;
    assert!(outside.x > 0.0 && outside.y > 0.0);
    let radius = SOLAR_STAR_SIZE * 0.5;
    assert!(outside.x < radius && outside.y < radius);
    assert!(outside.x < radius * 0.25 && outside.y < radius * 0.25);

    let celestial = celestial_position(&map);
    let opposite_edge_x = if corner.x > 0.0 {
        map.rect.min.x
    } else {
        map.rect.max.x
    };
    assert!((celestial.x - opposite_edge_x).abs() >= CELESTIAL_SIZE.x * 0.5);
    assert!((celestial.x - opposite_edge_x).abs() <= CELESTIAL_SIZE.x * 0.5 + CELESTIAL_MAP_MARGIN);
    assert!((celestial.x - map.rect.center().x) * corner.x < 0.0);
    assert!(map.rect.contains(celestial));
    const { assert!(CELESTIAL_SIZE.x < SOLAR_STAR_SIZE * 0.5) };

    for sample in 0..=200 {
        let elapsed = sample as f32 * SOLAR_STAR_FRAME_SECONDS / 20.0;
        let alphas = (0..SOLAR_STAR_FRAME_COUNT)
            .map(|frame| solar_star_frame_alpha(frame, elapsed))
            .collect::<Vec<_>>();
        assert!((alphas.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert!(alphas.iter().filter(|&&alpha| alpha > 0.0).count() <= 2);
    }

    for kind in CelestialKind::ALL {
        for sample in 0..=200 {
            let elapsed = sample as f32 * kind.frame_seconds() * kind.frame_count() as f32 / 200.0;
            let states =
                [celestial_frame_state(kind, 0, elapsed), celestial_frame_state(kind, 1, elapsed)];
            let alphas = states.map(|(_, alpha)| alpha);
            assert!((alphas.iter().sum::<f32>() - kind.opacity()).abs() < 1e-5);
            assert!(states.iter().all(|(frame, _)| *frame < kind.frame_count()));
            assert_eq!(states[1].0, (states[0].0 + 1) % kind.frame_count());
        }
        assert_ne!(
            celestial_frame_state(kind, 0, 0.0).0,
            celestial_frame_state(kind, 0, kind.frame_seconds()).0
        );
        let duration = kind.frame_count() as f32 * kind.frame_seconds();
        assert_eq!(celestial_frame_state(kind, 0, duration), celestial_frame_state(kind, 0, 0.0));
    }
}

#[test]
fn scenery_varies_between_maps_and_survives_save_roundtrips_and_world_changes() {
    let mut selections = Vec::new();
    for seed in 0..64 {
        let mut map = GameModel::new([seed; 32], GameRules::default()).unwrap().map;
        let selection = map_scenery_selection(&map);
        let corner = map_scenery_corner(&map);
        if !selections.contains(&selection) {
            selections.push(selection);
        }
        let saved = serde_json::to_string(&map).unwrap();
        let restored: Map = serde_json::from_str(&saved).unwrap();
        assert_eq!(selection, map_scenery_selection(&restored));
        assert_eq!(corner, map_scenery_corner(&restored));
        for planet in &mut map.planets {
            planet.is_destroyed = true;
            planet.owned = None;
            planet.controlled = None;
            planet.name = "Changed after a turn".into();
            planet.army.clear();
        }
        assert_eq!(selection, map_scenery_selection(&map));
        assert_eq!(corner, map_scenery_corner(&map));
    }
    for kind in CelestialKind::ALL {
        assert!(selections.contains(&kind));
    }
    assert_eq!(selections.len(), 3);
}

#[test]
fn asteroid_belt_forms_one_complete_circle_with_photographic_cutouts_and_visible_tumbling() {
    let map = GameModel::new([29; 32], GameRules::default()).unwrap().map;
    let layout = asteroid_belt_layout(&map).unwrap();
    let base_count = asteroid_belt_asteroid_count(layout.radius);
    let expected_count = asteroid_belt_placements(&map, layout).len();
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<Assets<Image>>()
        .insert_resource(map)
        .add_systems(
            Startup,
            |mut commands: Commands, map: Res<Map>, mut images: ResMut<Assets<Image>>| {
                let images = [
                    images.add(Image::default()),
                    images.add(Image::default()),
                    images.add(Image::default()),
                    images.add(Image::default()),
                ];
                spawn_asteroid_belt(&mut commands, &map, &images);
            },
        )
        .add_systems(Update, animate_asteroid_belts);
    app.update();

    let before = {
        let world = app.world_mut();
        let belts =
            world.query_filtered::<Entity, With<AsteroidBeltCmp>>().iter(world).collect::<Vec<_>>();
        assert_eq!(belts.len(), 1, "a map must spawn exactly one asteroid belt");
        assert!(world.get::<Children>(belts[0]).is_none());
        let asteroid_entities =
            world.query_filtered::<Entity, With<AsteroidCmp>>().iter(world).collect::<Vec<_>>();
        assert_eq!(asteroid_entities.len(), expected_count);
        assert!(asteroid_entities.iter().all(|entity| world.get::<ChildOf>(*entity).is_none()));
        let planets = world
            .resource::<Map>()
            .planets()
            .into_iter()
            .map(|planet| (planet.position, planet.size() * 0.5))
            .collect::<Vec<_>>();
        let mut asteroids =
            world.query_filtered::<(Entity, &Transform, &Pickable, &Sprite), With<AsteroidCmp>>();
        let mut image_ids = Vec::new();
        let mut phases = Vec::new();
        let asteroids = asteroids
            .iter(world)
            .map(|(entity, transform, pickable, sprite)| {
                assert_eq!(*pickable, Pickable::IGNORE);
                assert!(transform.translation.z < PLANET_Z);
                assert!(transform.translation.z > VORONOI_Z);
                assert_eq!(transform.translation.z, ASTEROID_BELT_DEPTH);
                assert!((0.0..=38.0).contains(&sprite.custom_size.unwrap().x));
                assert_eq!(sprite.color, Color::srgba(0.78, 0.76, 0.72, 0.82));
                for (planet_position, planet_radius) in &planets {
                    let asteroid_radius = sprite.custom_size.unwrap().x * 0.5;
                    assert!(
                        transform.translation.truncate().distance(*planet_position)
                            > planet_radius + asteroid_radius + ASTEROID_PLANET_CLEARANCE - 2.0
                    );
                }
                assert!(world.get::<Children>(entity).is_none());
                if !image_ids.contains(&sprite.image.id()) {
                    image_ids.push(sprite.image.id());
                }
                phases.push(world.get::<AsteroidCmp>(entity).unwrap().phase.rem_euclid(TAU));
                (entity, *transform)
            })
            .collect::<Vec<_>>();
        assert_eq!(image_ids.len(), 4);
        phases.sort_by(f32::total_cmp);
        let largest_gap = phases
            .windows(2)
            .map(|pair| pair[1] - pair[0])
            .chain(std::iter::once(phases[0] + TAU - phases[phases.len() - 1]))
            .fold(0.0_f32, f32::max);
        let base_spacing = TAU / base_count as f32;
        let maximum_planet_gap = 2.0
            * ((Planet::SIZE * 0.5
                + layout.maximum_asteroid_diameter * 0.5
                + ASTEROID_PLANET_CLEARANCE
                + ASTEROID_MAXIMUM_WOBBLE)
                / (layout.radius - layout.radial_half_width))
                .asin()
            + base_spacing * 2.25;
        assert!(
            largest_gap < maximum_planet_gap,
            "belt gap {largest_gap} exceeds one planet's clearance arc {maximum_planet_gap}"
        );
        asteroids
    };
    assert_eq!(before.len(), expected_count);

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
    app.update();
    assert!(before.iter().any(|(entity, transform)| {
        app.world().get::<Transform>(*entity).unwrap().translation.distance(transform.translation)
            > 0.5
    }));
    assert!(before.iter().any(|(entity, transform)| {
        app.world().get::<Transform>(*entity).unwrap().rotation.angle_between(transform.rotation)
            > 0.08
    }));
}

#[test]
fn multiplayer_projection_repairs_an_empty_asteroid_field_without_duplicates() {
    let map = GameModel::new([29; 32], GameRules::default()).unwrap().map;
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<AudioSource>()
        .init_resource::<WorldAssets>()
        .insert_resource(map)
        .add_systems(Update, ensure_asteroid_belt);

    app.update();
    app.update();
    let initial_count = {
        let world = app.world_mut();
        assert_eq!(world.query_filtered::<Entity, With<AsteroidBeltCmp>>().iter(world).count(), 1);
        world.query_filtered::<Entity, With<AsteroidCmp>>().iter(world).count()
    };
    assert!(initial_count >= ASTEROID_BELT_MINIMUM_COUNT);

    let asteroids = {
        let world = app.world_mut();
        world.query_filtered::<Entity, With<AsteroidCmp>>().iter(world).collect::<Vec<_>>()
    };
    for entity in asteroids {
        app.world_mut().despawn(entity);
    }
    app.update();

    let world = app.world_mut();
    assert_eq!(world.query_filtered::<Entity, With<AsteroidBeltCmp>>().iter(world).count(), 1);
    assert_eq!(
        world.query_filtered::<Entity, With<AsteroidCmp>>().iter(world).count(),
        initial_count
    );
}

#[test]
fn every_supported_map_gets_exactly_one_seed_varied_complete_asteroid_field() {
    let rules = [
        GameRules {
            planets_per_player: 5,
            moons_percent: 0,
            player_count: 1,
            practice_mode: true,
            ..default()
        },
        GameRules::default(),
        GameRules {
            planets_per_player: 20,
            moons_percent: 100,
            player_count: 4,
            ..default()
        },
    ];
    let mut radii = Vec::new();
    for rules in rules {
        for seed in 0..32_u8 {
            let map = GameModel::new([seed; 32], rules.clone()).unwrap().map;
            let layout = asteroid_belt_layout(&map).unwrap();
            assert!((0..=1).contains(&layout.between_bands));
            assert!(layout.radius.is_finite() && layout.radius > 0.0);
            assert!((ASTEROID_BELT_MINIMUM_COUNT..=ASTEROID_BELT_MAXIMUM_COUNT)
                .contains(&asteroid_belt_asteroid_count(layout.radius)));
            let gap = solar_band_gaps(&map)
                .into_iter()
                .find(|gap| gap.between_bands == layout.between_bands)
                .unwrap();
            assert!(layout.radius - layout.radial_half_width > gap.inner_center);
            assert!(layout.radius + layout.radial_half_width < gap.outer_center);
            let star = map.solar_star_position();
            let planets = map.planets();
            let placements = asteroid_belt_placements(&map, layout);
            let base_count = asteroid_belt_asteroid_count(layout.radius);
            assert!((base_count..=base_count + ASTEROID_BELT_MINIMUM_VISIBLE_COUNT)
                .contains(&placements.len()));
            for placement in &placements {
                let position = star + Vec2::from_angle(placement.phase) * placement.radius;
                for planet in &planets {
                    assert!(
                        position.distance(planet.position)
                            > planet.size() * 0.5
                                + placement.diameter * 0.5
                                + ASTEROID_PLANET_CLEARANCE
                                - 2.0
                    );
                }
            }
            radii.push(layout.radius.to_bits());
        }
    }
    radii.sort_unstable();
    radii.dedup();
    assert!(radii.len() > 16);
}

#[test]
fn every_game_map_has_one_visible_belt_between_adjacent_solar_areas() {
    let mut minimum_visible = usize::MAX;
    let mut worst_case = None;
    for player_count in 1..=4 {
        for planets_per_player in [5, 10, 20] {
            for moons_percent in [0, 30, 100] {
                let rules = GameRules {
                    planets_per_player,
                    moons_percent,
                    player_count,
                    practice_mode: player_count == 1,
                    ..default()
                };
                for sample in 0..256_u64 {
                    let seed = crate::core::random::DeterministicRngState::from_u64(
                        sample
                            + player_count as u64 * 17
                            + planets_per_player as u64 * 101
                            + moons_percent as u64 * 10_003,
                    )
                    .seed;
                    let map = GameModel::new(seed, rules.clone()).unwrap().map;
                    let layout = asteroid_belt_layout(&map).unwrap();
                    let gap = solar_band_gaps(&map)
                        .into_iter()
                        .find(|gap| gap.between_bands == layout.between_bands)
                        .unwrap();
                    assert!((0..=1).contains(&layout.between_bands));
                    assert!(layout.radius - layout.radial_half_width > gap.inner_center);
                    assert!(layout.radius + layout.radial_half_width < gap.outer_center);
                    let placements = asteroid_belt_placements(&map, layout);
                    let visible = visible_asteroid_count(&map, &placements);
                    if visible < minimum_visible {
                        minimum_visible = visible;
                        worst_case = Some((
                            player_count,
                            planets_per_player,
                            moons_percent,
                            sample,
                            layout.radius,
                            layout.between_bands,
                        ));
                    }
                }
            }
        }
    }
    assert!(
        minimum_visible >= ASTEROID_BELT_MINIMUM_VISIBLE_COUNT,
        "game has no recognizable asteroid belt inside the playable map: \
         minimum_visible={minimum_visible}, worst_case={worst_case:?}"
    );
}

#[test]
fn every_game_has_a_corner_sun_and_one_landmark_with_appropriate_camera_depth() {
    for expected_kind in CelestialKind::ALL {
        let map = (0..64)
            .map(|seed| GameModel::new([seed; 32], GameRules::default()).unwrap().map)
            .find(|map| map_scenery_selection(map) == expected_kind)
            .unwrap();
        let anchor = celestial_position(&map).extend(CELESTIAL_DEPTH);
        let sun_anchor = solar_star_position(&map).extend(SOLAR_STAR_DEPTH);
        assert!(anchor.z > VORONOI_Z && anchor.z < PLANET_Z);
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default(), TransformPlugin))
            .init_asset::<Image>()
            .init_asset::<Font>()
            .init_asset::<TextureAtlasLayout>()
            .init_asset::<AudioSource>()
            .init_resource::<WorldAssets>()
            .insert_resource(map)
            .add_systems(
                Startup,
                |mut commands: Commands, assets: Res<WorldAssets>, map: Res<Map>| {
                    spawn_background_landmarks(&mut commands, &assets, &map);
                },
            )
            .add_systems(Update, (animate_space_scenery, crate::core::camera::update_parallax));
        let camera = app
            .world_mut()
            .spawn((
                MainCamera,
                Transform::from_xyz(0.0, 0.0, 1.0),
                Projection::Orthographic(OrthographicProjection {
                    scale: 1.0,
                    ..OrthographicProjection::default_2d()
                }),
            ))
            .id();
        app.update();

        let world = app.world_mut();
        assert_eq!(world.query::<&CelestialCmp>().iter(world).count(), 1);
        assert_eq!(world.query::<&SolarStarCmp>().iter(world).count(), 1);
        let kind = map_scenery_selection(world.resource::<Map>());
        let mut layers = world.query::<(&ParallaxCmp, &Children)>();
        let mut nebula_follow = None;
        for (parallax, children) in layers.iter(world) {
            for child in children.iter() {
                if world.get::<NebulaCmp>(child).is_some() {
                    nebula_follow = Some(parallax.camera_follow);
                }
                assert!(world.get::<CelestialCmp>(child).is_none());
            }
        }

        let mut celestial_frames = world.query::<(&CelestialFrameCmp, &Sprite)>();
        let frames = celestial_frames.iter(world).collect::<Vec<_>>();
        assert_eq!(frames.len(), 2);
        assert!(
            (frames.iter().map(|(_, sprite)| sprite.color.to_srgba().alpha).sum::<f32>()
                - kind.opacity())
            .abs()
                < 1e-5
        );
        for (frame, sprite) in frames {
            assert!(frame.slot < 2);
            assert_eq!(sprite.custom_size, Some(CELESTIAL_SIZE * kind.size_scale()));
            let color = sprite.color.to_srgba();
            assert!((color.red - CELESTIAL_TINT).abs() < f32::EPSILON);
            assert!((color.green - CELESTIAL_TINT).abs() < f32::EPSILON);
            assert!((color.blue - CELESTIAL_TINT).abs() < f32::EPSILON);
            assert!((0.0..=kind.opacity()).contains(&color.alpha));
        }

        assert_eq!(nebula_follow, Some(NEBULA_PARALLAX_FOLLOW));
        let mut landmarks = world.query::<(&CelestialCmp, &GlobalTransform)>();
        let (celestial, transform) = landmarks.single(world).unwrap();
        assert_eq!(celestial.kind, expected_kind);
        assert_eq!(celestial.frames.len(), kind.frame_count());
        assert_eq!(transform.translation(), anchor);

        // Every landmark has a stable map position, so scanning to its edge always reveals it.
        let camera_position = Vec3::new(-2_000.0, 700.0, 1.0);
        world.get_mut::<Transform>(camera).unwrap().translation = camera_position;
        app.update();
        let world = app.world_mut();
        let pan_position = landmarks.single(world).unwrap().1.translation();
        let relative_motion =
            pan_position.truncate() - anchor.truncate() - camera_position.truncate();
        assert!(relative_motion.abs_diff_eq(-camera_position.truncate(), 1e-3));

        // Zoom and elapsed time must preserve the depth response without adding drift.
        if let Projection::Orthographic(projection) =
            &mut *world.get_mut::<Projection>(camera).unwrap()
        {
            projection.scale = 0.5;
        }
        world
            .resource_mut::<Time<bevy::time::Virtual>>()
            .advance_by(std::time::Duration::from_secs(36_000));
        app.update();
        let world = app.world_mut();
        let transform = landmarks.single(world).unwrap().1;
        assert_eq!(transform.translation(), anchor);
        let mut suns = world.query_filtered::<&GlobalTransform, With<SolarStarCmp>>();
        assert_eq!(suns.single(world).unwrap().translation(), sun_anchor);
    }
}

#[test]
fn celestial_landmarks_stay_visible_inside_normal_small_and_offset_maps() {
    let generated =
        (0..64).map(|seed| GameModel::new([seed; 32], GameRules::default()).unwrap().map);
    let unusual = [
        Rect::new(-50.0, -50.0, 50.0, 50.0),
        Rect::new(1_000.0, -700.0, 1_050.0, 900.0),
        Rect::new(-4_000.0, 2_000.0, 4_000.0, 2_040.0),
    ]
    .map(|rect| Map {
        rect,
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: Vec::new(),
    });
    for map in generated.chain(unusual) {
        let position = celestial_position(&map);
        let normalized = (position - map.rect.center()) / map.rect.half_size();
        assert!(normalized.x.abs() <= 1.0);
        assert!(normalized.y.abs() <= 1.0);
        assert!(normalized.x * map_scenery_corner(&map).x < 0.0);
        if map.rect.width() >= CELESTIAL_SIZE.x + CELESTIAL_MAP_MARGIN * 2.0 {
            assert!(
                position.x - CELESTIAL_SIZE.x * 0.5 >= map.rect.min.x + CELESTIAL_MAP_MARGIN - 1e-3
            );
            assert!(
                position.x + CELESTIAL_SIZE.x * 0.5 <= map.rect.max.x - CELESTIAL_MAP_MARGIN + 1e-3
            );
        }
        if map.rect.height() >= CELESTIAL_SIZE.y + CELESTIAL_MAP_MARGIN * 2.0 {
            assert!(position.y - CELESTIAL_SIZE.y * 0.5 >= map.rect.min.y - 1e-3);
            assert!(position.y + CELESTIAL_SIZE.y * 0.5 <= map.rect.max.y + 1e-3);
        }
    }
}

#[test]
fn ambience_uses_three_star_depths_and_cross_shaped_glints() {
    let mut app = App::new();
    app.add_plugins(TransformPlugin).add_systems(Startup, |mut commands: Commands| {
        spawn_ambient_stars(&mut commands);
    });
    app.update();

    let world = app.world_mut();
    let mut layers = world.query::<(&ParallaxCmp, &Children)>();
    let mut star_layers = layers
        .iter(world)
        .filter(|(_, children)| {
            children.iter().any(|child| world.get::<AmbientStarCmp>(child).is_some())
        })
        .map(|(parallax, children)| {
            let star_count = children
                .iter()
                .filter(|child| world.get::<AmbientStarCmp>(*child).is_some())
                .count();
            (parallax.camera_follow, parallax.drift, star_count)
        })
        .collect::<Vec<_>>();
    star_layers.sort_by(|left, right| right.0.total_cmp(&left.0));
    assert_eq!(star_layers.len(), 3);
    assert_eq!(star_layers.iter().map(|layer| layer.2).collect::<Vec<_>>(), [525, 350, 275]);
    assert!(star_layers.windows(2).all(|layers| layers[0].0 > layers[1].0));
    assert!(star_layers.iter().any(|(_, drift, _)| drift.x < 0.0));
    assert!(star_layers.iter().any(|(_, drift, _)| drift.x > 0.0));

    let mut pulsars = world.query::<(&AmbientPulsarCmp, &Children)>();
    assert_eq!(pulsars.iter(world).count(), 18);
    assert!(pulsars.iter(world).all(|(_, children)| {
        children.len() == 2
            && children.iter().all(|child| world.get::<AmbientPulsarRayCmp>(child).is_some())
    }));
}

#[test]
fn comet_system_spawns_thin_half_screen_streaks_and_cleans_them_up() {
    let mut app = App::new();
    app.init_resource::<Time>()
        .insert_resource(AmbientCometSpawner {
            remaining: 0.0,
            sequence: 0,
        })
        .add_systems(Update, update_ambient_comets);
    app.world_mut().spawn((Camera2d, MainCamera));

    app.update();
    let world = app.world_mut();
    let mut comets =
        world.query_filtered::<(Entity, &Children, &AmbientCometCmp), With<AmbientCometCmp>>();
    let (comet, children, comet_data) = comets.single(world).unwrap();
    assert_eq!(children.len(), 2);
    let travel_distance = comet_data.velocity.length() * comet_data.lifetime;
    assert!((800.0 * 0.44..=800.0 * 0.56).contains(&travel_distance));
    assert!(comet_data.peak_alpha <= 0.54);
    let children = children.iter().collect::<Vec<_>>();
    assert!(children.iter().all(|child| world.get::<AmbientCometPartCmp>(*child).is_some()));
    assert!(children.iter().all(|child| {
        world
            .get::<Sprite>(*child)
            .and_then(|sprite| sprite.custom_size)
            .is_some_and(|size| size.x <= 200.0 && size.y <= 1.28)
    }));

    world.resource_mut::<Time>().advance_by(Duration::from_secs(5));
    app.update();
    assert!(app.world().get_entity(comet).is_err());
    assert!(children.iter().all(|child| app.world().get_entity(*child).is_err()));
}

#[test]
fn infrastructure_uses_known_controllers_but_strategic_orbitals_are_public() {
    let mut model = GameModel::new(
        [17; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let own_home = model.players[0].home_planet;
    let enemy_home = model.players[1].home_planet;
    let unknown_home = model.players[2].home_planet;
    let army = Army::from([
        (Unit::planetary_shield(), 1),
        (Unit::space_dock(), 1),
        (Unit::Building(Building::OrbitalRailgun), 1),
        (Unit::Building(Building::SolarSatellite), Building::MAX_LEVEL),
        (Unit::Building(Building::CommandRelay), 1),
        (Unit::Building(Building::SensorPhalanx), 1),
        (Unit::Building(Building::JumpGate), 1),
    ]);
    for home in [own_home, enemy_home, unknown_home] {
        model.map.get_mut(home).army = army.clone();
    }
    let mut player = model.players[0].clone();
    player.reports.push(MissionReport {
        id: 1,
        turn: 1,
        mission: Mission {
            id: 1,
            owner: player.id,
            origin: own_home,
            destination: enemy_home,
            objective: Icon::Spy,
            ..default()
        },
        planet: model.map.get(enemy_home).clone(),
        scout_probes: 1_000_000,
        surviving_attacker: Army::new(),
        surviving_defender: army,
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: Some(2),
        destination_controlled: Some(2),
        combat_report: None,
        hidden: false,
    });
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        submitted_players: Vec::new(),
        id: GameId::new("defense-color-test"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 1_700_000_000,
        max_players: 3,
        status: model.status,
        persisted: PersistedGame::new(model.clone()),
        members: vec![],
    });
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<Time>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<WorldAssets>()
        .init_resource::<Missions>()
        .insert_resource(model.map)
        .insert_resource(player)
        .insert_resource(session)
        .add_plugins(bevy_tweening::TweeningPlugin)
        .add_systems(Startup, draw_map)
        .add_systems(
            Update,
            (
                update_planet_defenses.before(bevy_tweening::AnimationSystem::AnimationUpdate),
                animate_orbital_railguns.after(update_planet_defenses),
                animate_phalanx_drones,
            ),
        );
    let normal_shield_image = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
    {
        let mut assets = app.world_mut().resource_mut::<WorldAssets>();
        assets.images.insert("planetary shield marker".into(), normal_shield_image.clone());
    }
    app.world_mut().spawn((Camera2d, MainCamera));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(220));
    app.update();
    let own = test_defenses(app.world_mut(), own_home);
    let enemy = test_defenses(app.world_mut(), enemy_home);
    let unknown = test_defenses(app.world_mut(), unknown_home);
    let own_railgun = test_railgun(app.world_mut(), own_home);
    let enemy_railgun = test_railgun(app.world_mut(), enemy_home);
    let unknown_railgun = test_railgun(app.world_mut(), unknown_home);
    let own_satellites = test_satellites(app.world_mut(), own_home);
    let enemy_satellites = test_satellites(app.world_mut(), enemy_home);
    let unknown_satellites = test_satellites(app.world_mut(), unknown_home);
    let own_phalanxes = test_phalanxes(app.world_mut(), own_home);
    let enemy_phalanxes = test_phalanxes(app.world_mut(), enemy_home);
    let unknown_phalanxes = test_phalanxes(app.world_mut(), unknown_home);
    for drones in [&own_phalanxes, &enemy_phalanxes, &unknown_phalanxes] {
        assert_eq!(drones.len(), PHALANX_DRONE_COUNT);
        assert_eq!(
            drones
                .iter()
                .map(|entity| app.world().get::<SensorPhalanxCmp>(*entity).unwrap().index)
                .collect::<Vec<_>>(),
            (0..PHALANX_DRONE_COUNT).collect::<Vec<_>>()
        );
    }
    for satellites in [&own_satellites, &enemy_satellites, &unknown_satellites] {
        assert_eq!(satellites.len(), Building::MAX_LEVEL);
        assert_eq!(
            satellites
                .iter()
                .map(|entity| app.world().get::<SolarSatelliteCmp>(*entity).unwrap().level)
                .collect::<Vec<_>>(),
            (1..=Building::MAX_LEVEL).collect::<Vec<_>>()
        );
    }
    let shield_depth = app.world().get::<Transform>(own.0).unwrap().translation.z;
    let dock_depth = app.world().get::<Transform>(own.1).unwrap().translation.z;
    let railgun_depth = app.world().get::<Transform>(own_railgun).unwrap().translation.z;
    let gate_depth = app.world().get::<Transform>(own.2).unwrap().translation.z;
    let relay_depth = app.world().get::<Transform>(own.4).unwrap().translation.z;
    let phalanx_depths = own_phalanxes
        .iter()
        .map(|entity| app.world().get::<Transform>(*entity).unwrap().translation.z)
        .collect::<Vec<_>>();
    let satellite_depths = own_satellites
        .iter()
        .map(|entity| app.world().get::<Transform>(*entity).unwrap().translation.z)
        .collect::<Vec<_>>();
    assert!(
        MISSION_Z - 0.1 > PLANET_Z + dock_depth,
        "missions and their exhaust render above the orbital stack"
    );
    assert!(dock_depth > gate_depth);
    assert!(dock_depth > railgun_depth && railgun_depth > phalanx_depths[0]);
    assert!(dock_depth > relay_depth);
    assert!(phalanx_depths.iter().all(|&depth| dock_depth > depth));
    assert!(satellite_depths.iter().all(|&depth| gate_depth > depth && relay_depth > depth));
    assert!(phalanx_depths
        .iter()
        .all(|&phalanx| satellite_depths.iter().all(|&satellite| phalanx > satellite)));
    assert!(satellite_depths.iter().all(|&depth| depth > shield_depth));
    for level in 0..=Building::MAX_LEVEL {
        app.world_mut()
            .resource_mut::<Map>()
            .get_mut(own_home)
            .army
            .insert(Unit::Building(Building::SolarSatellite), level);
        app.update();
        let visible = own_satellites
            .iter()
            .filter(|entity| {
                *app.world().get::<Visibility>(**entity).unwrap() == Visibility::Inherited
            })
            .count();
        assert_eq!(visible, level, "one satellite should appear for each completed level");
    }

    // The dock and satellite network keep independent, seamless orbits.
    for (entity, radius, size, period) in [
        (own.1, Planet::SIZE * 0.75, Planet::SIZE * 0.4, Duration::from_secs(12)),
        (own.3, Planet::SIZE * 0.68, Planet::SIZE * 0.2, Duration::from_secs(14)),
    ] {
        let transform = app.world().get::<Transform>(entity).unwrap();
        let sprite = app.world().get::<Sprite>(entity).unwrap();
        let cycle = app.world().get::<TweenAnim>(entity).unwrap().tweenable().cycle_duration();
        assert!((transform.translation.truncate().length() - radius).abs() < 0.01);
        assert_eq!(sprite.custom_size, Some(Vec2::splat(size)));
        assert_eq!(cycle, period);
        assert_eq!(*app.world().get::<Pickable>(entity).unwrap(), Pickable::IGNORE);
    }
    for satellites in [&own_satellites, &enemy_satellites] {
        for entity in satellites {
            let transform = app.world().get::<Transform>(*entity).unwrap();
            assert!((transform.translation.truncate().length() - Planet::SIZE * 0.68).abs() < 0.01);
            assert_eq!(transform.rotation, Quat::IDENTITY);
            assert_eq!(
                app.world().get::<Sprite>(*entity).unwrap().custom_size,
                Some(Vec2::splat(Planet::SIZE * 0.2))
            );
            assert_eq!(*app.world().get::<Visibility>(*entity).unwrap(), Visibility::Inherited);
        }
    }
    assert_eq!(
        app.world().get::<Sprite>(own.0).unwrap().custom_size,
        Some(Vec2::splat(Planet::SIZE * 1.3))
    );
    assert_eq!(
        app.world().get::<TweenAnim>(own.0).unwrap().tweenable().cycle_duration(),
        Duration::from_secs(3)
    );
    assert_eq!(app.world().get::<TweenAnim>(own.0).unwrap().speed, 1.0);
    assert_eq!(app.world().get::<Sprite>(own.0).unwrap().image, normal_shield_image);
    assert_eq!(app.world().get::<Transform>(own.0).unwrap().rotation, Quat::IDENTITY);
    let alpha_before_overload = app.world().get::<Sprite>(own.0).unwrap().color.alpha();
    app.world_mut().resource_mut::<Map>().get_mut(own_home).shield_overload =
        crate::core::map::planet::ShieldOverloadState::Overloaded;
    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).shield_overload =
        crate::core::map::planet::ShieldOverloadState::Overloaded;
    app.world_mut().resource_mut::<Time>().advance_by(Duration::ZERO);
    app.update();
    assert!(app.world().get::<PlanetaryShieldCmp>(own.0).unwrap().overloaded);
    assert!(alpha_before_overload < PLANETARY_SHIELD_MAX_ALPHA);
    assert_eq!(app.world().get::<Sprite>(own.0).unwrap().color.alpha(), PLANETARY_SHIELD_MAX_ALPHA);
    assert_eq!(app.world().get::<TweenAnim>(own.0).unwrap().speed, 0.0);
    assert_eq!(app.world().get::<Transform>(own.0).unwrap().rotation, Quat::IDENTITY);

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(325));
    app.update();
    let shield = app.world().get::<PlanetaryShieldCmp>(own.0).unwrap();
    assert!((shield.spin_factor - 0.5).abs() < 0.001);
    assert_eq!(
        app.world().get::<Sprite>(own.0).unwrap().color.alpha(),
        PLANETARY_SHIELD_MAX_ALPHA,
        "an overloaded shield must remain steadily visible"
    );
    let rotation_while_accelerating = app.world().get::<Transform>(own.0).unwrap().rotation;
    assert_ne!(rotation_while_accelerating, Quat::IDENTITY);

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(325));
    app.update();
    let shield = app.world().get::<PlanetaryShieldCmp>(own.0).unwrap();
    assert_eq!(shield.spin_factor, 1.0);
    assert_ne!(app.world().get::<Transform>(own.0).unwrap().rotation, rotation_while_accelerating);
    assert_eq!(
        app.world().get::<TweenAnim>(own.0).unwrap().tweenable().cycle_duration(),
        Duration::from_secs(3)
    );
    assert_eq!(app.world().get::<TweenAnim>(own.0).unwrap().speed, 0.0);
    assert_eq!(app.world().get::<Sprite>(own.0).unwrap().color.alpha(), PLANETARY_SHIELD_MAX_ALPHA);
    assert_eq!(app.world().get::<Sprite>(own.0).unwrap().image, normal_shield_image);
    assert!(
        !app.world().get::<PlanetaryShieldCmp>(enemy.0).unwrap().overloaded,
        "enemy overload intent must not leak through the map"
    );
    assert_eq!(app.world().get::<Sprite>(enemy.0).unwrap().image, normal_shield_image);
    assert_eq!(app.world().get::<Transform>(enemy.0).unwrap().rotation, Quat::IDENTITY);

    let rotation_before_stop = app.world().get::<Transform>(own.0).unwrap().rotation;
    app.world_mut().resource_mut::<Map>().get_mut(own_home).shield_overload =
        crate::core::map::planet::ShieldOverloadState::Ready;
    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).shield_overload =
        crate::core::map::planet::ShieldOverloadState::Ready;
    app.world_mut().resource_mut::<Time>().advance_by(Duration::ZERO);
    app.update();
    assert!(!app.world().get::<PlanetaryShieldCmp>(own.0).unwrap().overloaded);
    assert_eq!(app.world().get::<PlanetaryShieldCmp>(own.0).unwrap().spin_factor, 1.0);
    assert_eq!(app.world().get::<TweenAnim>(own.0).unwrap().speed, 1.0);
    assert_eq!(app.world().get::<Sprite>(own.0).unwrap().color.alpha(), PLANETARY_SHIELD_MAX_ALPHA);

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(325));
    app.update();
    let shield = app.world().get::<PlanetaryShieldCmp>(own.0).unwrap();
    assert!((shield.spin_factor - 0.5).abs() < 0.001);
    assert!(app.world().get::<Sprite>(own.0).unwrap().color.alpha() < PLANETARY_SHIELD_MAX_ALPHA);
    let rotation_after_slowing = app.world().get::<Transform>(own.0).unwrap().rotation;
    let slowing_rotation = rotation_before_stop.angle_between(rotation_after_slowing);
    assert!(slowing_rotation > 0.0);
    assert!(
        slowing_rotation < TAU * 0.325 / PLANETARY_SHIELD_OVERLOAD_ROTATION_SECONDS,
        "the ring decelerates instead of stopping abruptly"
    );

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(325));
    app.update();
    let stopped_rotation = app.world().get::<Transform>(own.0).unwrap().rotation;
    assert_eq!(app.world().get::<PlanetaryShieldCmp>(own.0).unwrap().spin_factor, 0.0);
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
    app.update();
    assert_eq!(app.world().get::<Transform>(own.0).unwrap().rotation, stopped_rotation);
    assert_eq!(
        app.world().get::<TweenAnim>(own.0).unwrap().tweenable().cycle_duration(),
        Duration::from_secs(3)
    );
    assert_eq!(app.world().get::<TweenAnim>(own.0).unwrap().speed, 1.0);
    assert_eq!(app.world().get::<Sprite>(own.0).unwrap().image, normal_shield_image);
    let gate_anchor = app.world().get::<Transform>(own.2).unwrap().translation;
    let gate_rotation = app.world().get::<Transform>(own.2).unwrap().rotation;
    assert!((gate_anchor.truncate().length() - Planet::SIZE * 0.9).abs() < 0.01);
    assert_eq!(
        app.world().get::<Sprite>(own.2).unwrap().custom_size,
        Some(Vec2::splat(Planet::SIZE * 0.36))
    );
    assert_eq!(
        app.world().get::<TweenAnim>(own.2).unwrap().tweenable().cycle_duration(),
        Duration::from_secs(17)
    );
    assert_eq!(
        *app.world().get::<Pickable>(own.2).unwrap(),
        Pickable::IGNORE,
        "a lone owned gate must not receive hover, cursor, or click events"
    );
    let relay = app.world().get::<RangeMarkerMotionCmp>(own.4).unwrap();
    assert!((relay.anchor.length() - Planet::SIZE * 0.83).abs() < 0.01);
    assert!(
        app.world().get::<Transform>(own.4).unwrap().translation.truncate().distance(relay.anchor)
            <= 2.7
    );
    assert_eq!(
        app.world().get::<Sprite>(own.4).unwrap().custom_size,
        Some(Vec2::splat(Planet::SIZE * 0.34))
    );
    assert!(app.world().get::<TweenAnim>(own.4).is_none());
    assert_eq!(*app.world().get::<Pickable>(own.4).unwrap(), Pickable::IGNORE);

    for entity in &own_phalanxes {
        let drone = app.world().get::<SensorPhalanxCmp>(*entity).unwrap();
        assert!((drone.anchor.length() - Planet::SIZE * 0.83).abs() < 0.01);
        assert_eq!(
            app.world().get::<Sprite>(*entity).unwrap().custom_size,
            Some(Vec2::splat(Planet::SIZE * 0.23))
        );
        assert!(app.world().get::<TweenAnim>(*entity).is_none());
        assert!(app.world().get::<RangeMarkerMotionCmp>(*entity).is_none());
        assert_eq!(*app.world().get::<Pickable>(*entity).unwrap(), Pickable::default());
        assert!(
            app.world().get::<Transform>(*entity).unwrap().translation.distance(gate_anchor)
                > Planet::SIZE * (0.36 + 0.23) * 0.5,
            "the Phalanx formation must stay clear of the Jump Gate"
        );
    }
    let phalanx_transform = *app.world().get::<Transform>(own.5).unwrap();
    let railgun_transform = *app.world().get::<Transform>(own_railgun).unwrap();
    assert!(app.world().get::<Children>(own_railgun).is_none());
    let satellite_position = app.world().get::<Transform>(own.3).unwrap().translation;
    let satellite_rotation = app.world().get::<Transform>(own.3).unwrap().rotation;
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(250));
    app.update();
    assert!(
        app.world()
            .get::<Transform>(own.5)
            .unwrap()
            .translation
            .distance(phalanx_transform.translation)
            > 0.01,
        "the Phalanx drones should move around one another"
    );
    let moved_railgun = app.world().get::<Transform>(own_railgun).unwrap();
    assert!(
        moved_railgun.translation.distance(railgun_transform.translation) > 0.01,
        "the Railgun should drift while holding station"
    );
    assert_eq!(moved_railgun.scale, Vec3::ONE, "the Railgun should not pulse in size");
    let satellite_transform = app.world().get::<Transform>(own.3).unwrap();
    assert!(satellite_transform.translation.distance(satellite_position) > 0.01);
    assert_eq!(
        satellite_transform.rotation, satellite_rotation,
        "satellites should orbit without spinning around themselves"
    );

    // An unseen control change remains private for ordinary infrastructure, while public
    // strategic orbitals continue to identify the planet's owner.
    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).controlled = Some(3);
    for (index, color) in PLAYER_COLOR_PALETTE.into_iter().enumerate() {
        let enemy_color = PLAYER_COLOR_PALETTE[(index + 2) % PLAYER_COLOR_PALETTE.len()];
        {
            let mut session = app.world_mut().resource_mut::<MultiplayerSession>();
            let model = &mut session.active_game.as_mut().unwrap().persisted.state;
            model.player_mut(1).unwrap().color = color;
            model.player_mut(2).unwrap().color = enemy_color;
        }
        app.update();
        for ((shield, _, gate, satellite, relay, phalanx), expected) in
            [(&own, color), (&enemy, enemy_color)]
        {
            assert_eq!(*app.world().get::<Visibility>(*shield).unwrap(), Visibility::Inherited);
            for orbital in [gate, satellite, relay, phalanx] {
                assert_eq!(
                    *app.world().get::<Visibility>(*orbital).unwrap(),
                    Visibility::Inherited
                );
                assert_eq!(app.world().get::<Sprite>(*orbital).unwrap().color, expected.color());
            }
            let mut alphas = Vec::new();
            // Step across loop boundaries at a frame interval that does not divide the period.
            // Check the rendered sprite too: hue and alpha are presentation behavior.
            for _ in 0..354 {
                app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(17));
                app.update();
                let actual = app.world().get::<Sprite>(*shield).unwrap().color;
                assert!(actual
                    .with_alpha(1.)
                    .to_srgba()
                    .to_vec4()
                    .abs_diff_eq(expected.color().to_srgba().to_vec4(), 1e-6));
                alphas.push(actual.alpha());
            }
            for cycle in alphas.as_chunks::<177>().0 {
                assert!(cycle.iter().any(|&alpha| alpha < 0.01), "shield fades fully out");
                assert!(cycle.iter().any(|&alpha| alpha > 0.8), "shield becomes visible again");
            }
            for pair in alphas.windows(2) {
                let change = (pair[1] - pair[0]).abs();
                assert!(change < 0.02, "shield must not pop between frames: {pair:?}");
                if pair[0] < 0.001 || pair[0] > 0.849 {
                    assert!(change < 0.002, "shield must gently reverse its fade: {pair:?}");
                }
            }
        }
        let public_owner_color =
            app.world().resource::<MultiplayerSession>().player_color(2).color();
        for (dock, expected) in [(&own.1, color.color()), (&enemy.1, public_owner_color)] {
            assert_eq!(*app.world().get::<Visibility>(*dock).unwrap(), Visibility::Inherited);
            assert_eq!(app.world().get::<Sprite>(*dock).unwrap().color, expected);
        }
        for (railgun, expected) in
            [(&own_railgun, color.color()), (&enemy_railgun, public_owner_color)]
        {
            assert_eq!(*app.world().get::<Visibility>(*railgun).unwrap(), Visibility::Inherited);
            assert_eq!(app.world().get::<Sprite>(*railgun).unwrap().color, expected);
            assert_eq!(*app.world().get::<Pickable>(*railgun).unwrap(), Pickable::default());
        }
        for (satellites, expected) in [(&own_satellites, color), (&enemy_satellites, enemy_color)] {
            for satellite in satellites {
                assert_eq!(
                    *app.world().get::<Visibility>(*satellite).unwrap(),
                    Visibility::Inherited
                );
                assert_eq!(app.world().get::<Sprite>(*satellite).unwrap().color, expected.color());
            }
        }
        for (drones, expected) in [(&own_phalanxes, color), (&enemy_phalanxes, enemy_color)] {
            for drone in drones {
                assert_eq!(*app.world().get::<Visibility>(*drone).unwrap(), Visibility::Inherited);
                assert_eq!(app.world().get::<Sprite>(*drone).unwrap().color, expected.color());
            }
        }
        assert_eq!(
            *app.world().get::<Visibility>(unknown.1).unwrap(),
            Visibility::Inherited,
            "a Space Dock remains visible on a planet with no scan intelligence"
        );
        let unknown_owner_color =
            app.world().resource::<MultiplayerSession>().player_color(3).color();
        assert_eq!(app.world().get::<Sprite>(unknown.1).unwrap().color, unknown_owner_color);
        assert_eq!(
            *app.world().get::<Visibility>(unknown_railgun).unwrap(),
            Visibility::Inherited,
            "an Orbital Railgun remains visible on a planet with no scan intelligence"
        );
        assert_eq!(app.world().get::<Sprite>(unknown_railgun).unwrap().color, unknown_owner_color);
        for entity in [unknown.0, unknown.2, unknown.3, unknown.4, unknown.5] {
            assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
        }
        for satellite in &unknown_satellites {
            assert_eq!(*app.world().get::<Visibility>(*satellite).unwrap(), Visibility::Hidden);
        }
        for drone in &unknown_phalanxes {
            assert_eq!(*app.world().get::<Visibility>(*drone).unwrap(), Visibility::Hidden);
        }
    }

    // Private infrastructure follows control, but public strategic markers retain the owner.
    let alpha_before_capture = app.world().get::<Sprite>(enemy.0).unwrap().color.alpha();
    let elapsed_before_capture =
        app.world().get::<TweenAnim>(enemy.0).unwrap().tweenable().elapsed();
    app.world_mut().resource_mut::<Time>().advance_by(Duration::ZERO);
    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).controlled = Some(1);
    app.update();
    let own_color = app.world().get::<Sprite>(own.1).unwrap().color;
    let enemy_owner_color = app.world().resource::<MultiplayerSession>().player_color(2).color();
    assert_eq!(app.world().get::<Sprite>(enemy.1).unwrap().color, enemy_owner_color);
    assert_eq!(app.world().get::<Sprite>(enemy_railgun).unwrap().color, enemy_owner_color);
    assert_eq!(app.world().get::<Sprite>(enemy.2).unwrap().color, own_color);
    assert_eq!(app.world().get::<Sprite>(enemy.3).unwrap().color, own_color);
    assert_eq!(app.world().get::<Sprite>(enemy.4).unwrap().color, own_color);
    assert_eq!(app.world().get::<Sprite>(enemy.5).unwrap().color, own_color);
    for satellite in &enemy_satellites {
        assert_eq!(app.world().get::<Sprite>(*satellite).unwrap().color, own_color);
    }
    for drone in &enemy_phalanxes {
        assert_eq!(app.world().get::<Sprite>(*drone).unwrap().color, own_color);
    }
    assert_eq!(app.world().get::<PlanetaryShieldCmp>(enemy.0).unwrap().color, Some(own_color));
    assert_eq!(
        app.world().get::<TweenAnim>(enemy.0).unwrap().tweenable().elapsed(),
        elapsed_before_capture,
        "changing controller must not restart the fade"
    );
    let alpha_after_capture = app.world().get::<Sprite>(enemy.0).unwrap().color.alpha();
    assert!((alpha_after_capture - alpha_before_capture).abs() < 1e-6);
    assert_eq!(
        app.world().get::<Transform>(own.2).unwrap().translation,
        gate_anchor,
        "the Jump Gate spins without orbiting its planet"
    );
    assert!(
        app.world().get::<Transform>(own.2).unwrap().rotation.angle_between(gate_rotation) > 0.01,
        "the anchored Jump Gate still spins around itself"
    );

    let camera =
        app.world_mut().query_filtered::<Entity, With<MainCamera>>().single(app.world()).unwrap();
    let Projection::Orthographic(projection) =
        &mut *app.world_mut().get_mut::<Projection>(camera).unwrap()
    else {
        panic!("map camera should use an orthographic projection");
    };
    projection.scale = crate::core::constants::MAX_ZOOM;
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(220));
    app.update();
    for world in [&own, &enemy] {
        assert_eq!(*app.world().get::<Visibility>(world.1).unwrap(), Visibility::Inherited);
        assert_eq!(*app.world().get::<Visibility>(world.2).unwrap(), Visibility::Inherited);
        assert_eq!(*app.world().get::<Visibility>(world.3).unwrap(), Visibility::Hidden);
        assert_eq!(*app.world().get::<Visibility>(world.4).unwrap(), Visibility::Hidden);
        assert_eq!(*app.world().get::<Visibility>(world.5).unwrap(), Visibility::Inherited);
    }
    for railgun in [own_railgun, enemy_railgun] {
        assert_eq!(*app.world().get::<Visibility>(railgun).unwrap(), Visibility::Inherited);
    }
    for satellite in own_satellites.iter().chain(&enemy_satellites) {
        assert_eq!(*app.world().get::<Visibility>(*satellite).unwrap(), Visibility::Hidden);
    }

    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).is_destroyed = true;
    app.update();
    for entity in [enemy.0, enemy.1, enemy.2, enemy.3, enemy.4, enemy.5] {
        assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
    }
    assert_eq!(*app.world().get::<Visibility>(enemy_railgun).unwrap(), Visibility::Hidden);
    assert_eq!(*app.world().get::<Pickable>(enemy_railgun).unwrap(), Pickable::IGNORE);
    for satellite in &enemy_satellites {
        assert_eq!(*app.world().get::<Visibility>(*satellite).unwrap(), Visibility::Hidden);
    }
}

#[test]
fn territory_cells_blend_between_players_and_fade_away_without_popping() {
    let first = Color::srgb(0.2, 0.4, 0.8);
    let second = Color::srgb(0.9, 0.3, 0.15);
    let mut visibility = Visibility::Hidden;
    let mut material = ColorMaterial::default();
    let mut transition = TerritoryTransitionCmp::default();

    update_territory_visual(
        &mut visibility,
        &mut material,
        &mut transition,
        Some(first),
        true,
        0.58,
        true,
        true,
        0.0,
    );
    assert_eq!(visibility, Visibility::Inherited);
    assert_eq!(material.color, first.with_alpha(0.58));

    update_territory_visual(
        &mut visibility,
        &mut material,
        &mut transition,
        Some(second),
        true,
        0.58,
        true,
        true,
        TERRITORY_TRANSITION_SECONDS * 0.5,
    );
    assert_ne!(material.color, first.with_alpha(0.58));
    assert_ne!(material.color, second.with_alpha(0.58));
    assert_eq!(visibility, Visibility::Inherited);

    update_territory_visual(
        &mut visibility,
        &mut material,
        &mut transition,
        Some(second),
        true,
        0.58,
        true,
        true,
        TERRITORY_TRANSITION_SECONDS,
    );
    assert_eq!(material.color, second.with_alpha(0.58));

    update_territory_visual(
        &mut visibility,
        &mut material,
        &mut transition,
        None,
        false,
        0.58,
        true,
        true,
        TERRITORY_TRANSITION_SECONDS,
    );
    assert_eq!(material.color.alpha(), 0.0);
    assert_eq!(visibility, Visibility::Hidden);
}

#[test]
fn planet_selection_stops_camera_focus_and_preserves_the_origin_for_other_owners() {
    let map = Map::new(2, 0);
    let mut planet = map.planets[0].clone();
    let player = Player::new(1, planet.id);
    let mut state = UiState {
        mission: true,
        combat_report: Some(3),
        ..default()
    };
    let previous_origin = map.planets[1].id;
    state.mission_info.origin = previous_origin;

    for owner in [None, Some(2)] {
        planet.owned = owner;
        state.to_selected = true;
        state.focus_planet = Some(previous_origin);
        select_planet(&planet, &mut state, &player);
        assert_eq!(state.planet_selected, Some(planet.id));
        assert!(!state.to_selected);
        assert_eq!(state.focus_planet, None);
        assert!(!state.mission);
        assert_eq!(state.combat_report, None);
        assert_eq!(state.mission_info.origin, previous_origin);
    }
    for (owned, controlled) in [(Some(player.id), None), (None, Some(player.id))] {
        planet.owned = owned;
        planet.controlled = controlled;
        state.planet_selected = None;
        state.mission_info.origin = previous_origin;
        state.to_selected = true;
        select_planet(&planet, &mut state, &player);
        assert_eq!(state.planet_selected, Some(planet.id));
        assert!(!state.to_selected);
        assert_eq!(state.mission_info.origin, planet.id);
    }
}

#[test]
fn range_marker_idle_motion_meets_seamlessly_at_its_cycle_boundary() {
    let anchor = Vec2::new(-60.0, -55.0);
    for phase in [0.0, 0.4, PI, TAU - 0.1] {
        let (start_position, start_tilt) = range_marker_pose(anchor, 0.0, phase);
        let (end_position, end_tilt) = range_marker_pose(anchor, RANGE_MARKER_CYCLE_SECONDS, phase);
        assert!(start_position.abs_diff_eq(end_position, 1e-4));
        assert!((start_tilt - end_tilt).abs() < 1e-5);
    }
}

#[test]
fn range_marker_idle_motion_continues_while_visible() {
    let anchor = Vec2::new(-60.0, -55.0);
    let mut app = App::new();
    app.init_resource::<Time>().add_systems(Update, animate_range_markers);
    let marker = app
        .world_mut()
        .spawn((
            Transform::from_translation(anchor.extend(0.7)),
            Visibility::Inherited,
            RangeMarkerMotionCmp {
                anchor,
                elapsed: 0.0,
                phase: 0.4,
            },
        ))
        .id();

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
    app.update();
    let moving = *app.world().get::<Transform>(marker).unwrap();
    assert!(moving.translation.truncate().distance(anchor) > 0.1);
    assert!(moving.rotation.angle_between(Quat::IDENTITY) > 0.001);

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(3));
    app.update();
    let advanced = app.world().get::<Transform>(marker).unwrap();
    assert!(advanced.translation.distance(moving.translation) > 0.01);
}

#[test]
fn phalanx_drone_pseudo_orbits_are_distinct_and_loop_seamlessly() {
    let anchor = Vec2::new(-58.0, 58.0);
    let drones = (0..PHALANX_DRONE_COUNT)
        .map(|index| SensorPhalanxCmp {
            index,
            anchor,
            phase: 0.37,
        })
        .collect::<Vec<_>>();
    let starts = drones.iter().map(|drone| phalanx_drone_pose(drone, 0.0)).collect::<Vec<_>>();
    let ends = drones
        .iter()
        .map(|drone| phalanx_drone_pose(drone, PHALANX_DRONE_CYCLE_SECONDS))
        .collect::<Vec<_>>();

    for (start, end) in starts.iter().zip(ends) {
        assert!(start.translation.abs_diff_eq(end.translation, 1e-4));
        assert!(start.scale.abs_diff_eq(end.scale, 1e-4));
        assert!(start.rotation.angle_between(end.rotation) < 1e-3);
    }
    for first in 0..starts.len() {
        for second in first + 1..starts.len() {
            assert!(starts[first].translation.distance(starts[second].translation) > 1.0);
        }
    }
    let later = phalanx_drone_pose(&drones[0], 0.73);
    assert!(later.translation.distance(starts[0].translation) > 1.0);
    assert_ne!(later.translation.x - anchor.x, later.translation.y - anchor.y);
}

#[test]
fn jump_gate_hover_links_only_the_players_other_owned_gates() {
    let mut model = GameModel::new(
        [71; 32],
        GameRules {
            player_count: 2,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let player = model.players[0].clone();
    let home = player.home_planet;
    let enemy = model.players[1].home_planet;
    let other = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.id != home && planet.id != enemy)
        .unwrap()
        .id;
    for id in [home, other, enemy] {
        model.map.get_mut(id).army.insert(Unit::Building(Building::JumpGate), 1);
    }
    model.map.get_mut(other).owned = Some(player.id);
    model.map.get_mut(other).controlled = Some(player.id);

    let gate_offset = Vec2::new(70.0, -30.0);
    let expected = jump_gate_link_particles(
        model.map.get(home).position + gate_offset,
        model.map.get(other).position + gate_offset,
        PlayerColor::for_player(player.id).color(),
        0.0,
        visual_noise(home as u32 ^ (other as u32).rotate_left(13)) * TAU,
    )
    .len();
    assert!(expected > 0);

    let mut app = App::new();
    app.init_resource::<Time>()
        .insert_resource(model.map)
        .insert_resource(player)
        .insert_resource(MultiplayerSession::default())
        .insert_resource(UiState {
            jump_gate_hover: Some(home),
            ..default()
        })
        .add_systems(Update, update_jump_gate_links);
    for planet in [home, other, enemy] {
        app.world_mut().spawn((
            Transform::from_translation(gate_offset.extend(0.72)),
            JumpGateCmp {
                planet,
            },
        ));
    }

    app.update();
    let visible_links = app.world_mut().query::<&JumpGateLinkCmp>().iter(app.world()).count();
    assert_eq!(visible_links, expected, "the enemy gate must not receive a preview link");

    app.world_mut().resource_mut::<UiState>().jump_gate_hover = Some(enemy);
    app.update();
    assert_eq!(app.world_mut().query::<&JumpGateLinkCmp>().iter(app.world()).count(), 0);
}

#[test]
fn jump_gate_shortcut_needs_two_owned_gates_and_opens_an_enabled_deploy_draft() {
    let mut model = GameModel::new(
        [72; 32],
        GameRules {
            player_count: 2,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let player = model.players[0].clone();
    let home = player.home_planet;
    let enemy = model.players[1].home_planet;
    let other = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.id != home && planet.id != enemy)
        .unwrap()
        .id;
    model.map.get_mut(home).army.insert(Unit::Building(Building::JumpGate), Building::MAX_LEVEL);

    let settings = Settings {
        turn: 9,
        ..default()
    };
    let mut state = UiState::default();
    assert!(!jump_gate_network_available(&model.map, &player));
    assert!(!open_jump_gate_mission(&mut state, &settings, &model.map, &player, home));
    assert!(!state.mission, "upgrading one gate must not make it clickable");

    let other_planet = model.map.get_mut(other);
    other_planet.owned = Some(player.id);
    other_planet.controlled = Some(player.id);
    other_planet.army.insert(Unit::Building(Building::JumpGate), 1);

    assert!(jump_gate_network_available(&model.map, &player));
    assert!(open_jump_gate_mission(&mut state, &settings, &model.map, &player, home));
    assert!(state.mission);
    assert_eq!(state.mission_tab, MissionTab::NewMission);
    assert_eq!(state.mission_info.origin, home);
    assert_eq!(state.mission_info.destination, other);
    assert_eq!(state.mission_info.objective, Icon::Deploy);
    assert!(state.mission_info.jump_gate);
    assert!(state.jump_gate_history);
    assert!(!state.mission_info.army.has_army());
}

#[test]
fn scanner_ranges_follow_infrastructure_marker_hover_and_controlled_moon_hover() {
    let mut model = GameModel::new([31; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    model.players[0].color = PlayerColor::new(4).unwrap();
    let player = model.players[0].clone();
    let scanner_color = player.color().color();
    let enemy_scanner_color = model.players[1].color().color();
    let home = player.home_planet;
    let enemy = model.players[1].home_planet;
    let moons = model.map.moons().iter().map(|moon| moon.id).collect::<Vec<_>>();
    let moon = moons[0];
    let enemy_moon = moons[1];
    model.map.get_mut(moon).controlled = Some(player.id);
    model.map.get_mut(enemy_moon).controlled = Some(model.players[1].id);
    for id in [home, enemy] {
        model.map.get_mut(id).army.insert(Unit::Building(Building::SensorPhalanx), 2);
        model.map.get_mut(id).army.insert(Unit::Building(Building::OrbitalRailgun), 2);
    }
    for id in [moon, enemy_moon] {
        model.map.get_mut(id).army.insert(Unit::Building(Building::OrbitalRadar), 3);
    }
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        submitted_players: Vec::new(),
        id: GameId::new("railgun-range-color-test"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 1_700_000_000,
        max_players: 2,
        status: model.status,
        persisted: PersistedGame::new(model.clone()),
        members: vec![],
    });
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<WorldAssets>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<Time>()
        .init_resource::<UiState>()
        .init_resource::<Settings>()
        .init_resource::<Missions>()
        .insert_resource(session)
        .insert_resource(model.map)
        .insert_resource(player)
        .add_systems(Startup, draw_map)
        .add_systems(Update, update_planet_info);
    app.world_mut().spawn((Camera2d, MainCamera));

    for (hover, preview, selected, expected) in [
        (Some(home), None, None, None),
        (None, Some(MapRangePreview::SensorPhalanx(home)), None, Some((home, 250.0))),
        (None, Some(MapRangePreview::OrbitalRailgun(home)), None, Some((home, 400.0))),
        (Some(moon), None, Some(home), Some((moon, 395.0))),
        (None, None, Some(moon), None),
        (None, Some(MapRangePreview::SensorPhalanx(enemy)), None, None),
        (None, Some(MapRangePreview::OrbitalRailgun(enemy)), None, Some((enemy, 400.0))),
        (Some(enemy_moon), None, None, None),
    ] {
        app.insert_resource(UiState {
            planet_hover: hover,
            range_preview: preview,
            planet_selected: selected,
            ..default()
        });
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(100));
        app.update();
        let world = app.world_mut();
        let mut scanners = world.query_filtered::<
            (&ChildOf, &Visibility, &Mesh2d, &MeshMaterial2d<ColorMaterial>),
            With<ScannerCmp>,
        >();
        let mut visible = 0;
        let mut outer_radius = 0.0_f32;
        for (parent, visibility, mesh, material) in scanners.iter(world) {
            let id = world.get::<PlanetCmp>(parent.parent()).unwrap().id;
            assert_eq!(
                *visibility == Visibility::Inherited,
                expected.is_some_and(|(expected_id, _)| expected_id == id)
            );
            if expected.is_none_or(|(expected_id, _)| expected_id != id) {
                continue;
            }
            visible += 1;
            let expected_color = if preview == Some(MapRangePreview::OrbitalRailgun(enemy)) {
                enemy_scanner_color
            } else {
                scanner_color
            };
            assert_eq!(
                world.resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
                expected_color
            );
            let positions = world
                .resource::<Assets<Mesh>>()
                .get(&mesh.0)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
                .as_float3()
                .unwrap();
            let radius =
                positions.iter().map(|p| Vec2::new(p[0], p[1]).length()).reduce(f32::max).unwrap();
            let expected_radius = expected.unwrap().1;
            assert!(radius <= expected_radius + 0.01, "decorative arcs stay inside the range");
            outer_radius = outer_radius.max(radius);
        }
        assert_eq!(
            visible,
            if expected.is_some() {
                3
            } else {
                0
            }
        );
        if let Some((_, expected_radius)) = expected {
            assert!(
                (outer_radius - expected_radius).abs() < 0.01,
                "scanner uses its installed level"
            );
        }
    }

    // Absent infrastructure and a destroyed world have no range to display.
    app.world_mut()
        .resource_mut::<Map>()
        .get_mut(home)
        .army
        .remove(&Unit::Building(Building::SensorPhalanx));
    app.world_mut()
        .resource_mut::<Map>()
        .get_mut(home)
        .army
        .remove(&Unit::Building(Building::OrbitalRailgun));
    app.world_mut().resource_mut::<Map>().get_mut(moon).is_destroyed = true;
    for (hover, preview) in [
        (None, Some(MapRangePreview::SensorPhalanx(home))),
        (None, Some(MapRangePreview::OrbitalRailgun(home))),
        (Some(moon), None),
    ] {
        {
            let mut state = app.world_mut().resource_mut::<UiState>();
            state.planet_hover = hover;
            state.range_preview = preview;
        }
        app.update();
        let world = app.world_mut();
        let mut scanners = world.query_filtered::<(&ChildOf, &Visibility), With<ScannerCmp>>();
        let visible = scanners
            .iter(world)
            .filter(|(_, visibility)| **visibility == Visibility::Inherited)
            .map(|(parent, _)| world.get::<PlanetCmp>(parent.parent()).unwrap().id)
            .collect::<Vec<_>>();
        assert!(visible.is_empty(), "hover={hover:?}, preview={preview:?}: {visible:?}");
    }
}

#[test]
fn home_crown_tracks_the_measured_name_width() {
    let mut app = App::new();
    app.add_systems(Update, position_home_crown);
    let name = app.world_mut().spawn((PlanetNameCmp, bevy::text::TextLayoutInfo::default())).id();
    let crown = app.world_mut().spawn((HomeCrownCmp, Transform::default(), ChildOf(name))).id();
    for width in [40.0, 180.0, 75.0] {
        app.world_mut().get_mut::<bevy::text::TextLayoutInfo>(name).unwrap().size =
            Vec2::new(width, 18.0);
        app.update();
        let transform = app.world().get::<Transform>(crown).unwrap();
        let crown_right = transform.translation.x + TITLE_TEXT_SIZE * 0.5;
        assert!(crown_right < -width * 0.5);
        assert_eq!(transform.translation.y, 0.0);
    }
}

#[test]
fn home_map_label_follows_the_same_hover_and_info_rules_as_other_planets() {
    let model = GameModel::new([31; 32], GameRules::default()).unwrap();
    for player in &model.players {
        let mut app = App::new();
        app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<Font>()
            .init_asset::<TextureAtlasLayout>()
            .init_asset::<bevy_kira_audio::AudioSource>()
            .init_resource::<WorldAssets>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<ColorMaterial>>()
            .init_resource::<Time>()
            .init_resource::<UiState>()
            .init_resource::<MultiplayerSession>()
            .insert_resource(Settings {
                show_info: false,
                ..default()
            })
            .init_resource::<Missions>()
            .insert_resource(model.map.clone())
            .insert_resource(player.clone())
            .add_systems(Startup, draw_map)
            .add_systems(Update, update_planet_info);
        app.world_mut().spawn((Camera2d, MainCamera));
        let other = model.map.planets.iter().find(|p| p.id != player.home_planet).unwrap().id;
        for (hover, show_info) in [
            (None, false),
            (Some(player.home_planet), false),
            (Some(other), false),
            (None, false),
            (None, true),
            (None, false),
        ] {
            app.world_mut().resource_mut::<UiState>().planet_hover = hover;
            app.world_mut().resource_mut::<Settings>().show_info = show_info;
            app.update();
            let world = app.world_mut();
            let mut names = world.query_filtered::<(&ChildOf, &Visibility), With<PlanetNameCmp>>();
            for (parent, visibility) in names.iter(world) {
                let id = world.get::<PlanetCmp>(parent.parent()).unwrap().id;
                assert_eq!(*visibility == Visibility::Inherited, hover == Some(id) || show_info);
            }
        }
        let world = app.world_mut();
        assert!(world.query::<&Text2d>().iter(world).all(|text| text.0 != "HOME"));
        let mut crowns = world
            .query_filtered::<(&ChildOf, &MeshMaterial2d<ColorMaterial>), With<HomeCrownCmp>>();
        let markers = crowns.iter(world).collect::<Vec<_>>();
        assert_eq!(markers.len(), 1);
        let (parent, material) = markers[0];
        assert_eq!(
            world.resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
            HOME_PLANET_COLOR
        );
        let planet_entity = world.get::<ChildOf>(parent.parent()).unwrap().parent();
        assert_eq!(world.get::<PlanetCmp>(planet_entity).unwrap().id, player.home_planet);
    }
}

#[test]
fn empty_orbitals_shortcut_only_appears_while_inspecting_an_owned_planet() {
    let mut model = GameModel::new([41; 32], GameRules::default()).unwrap();
    let player = model.players[0].clone();
    let home = player.home_planet;
    model.map.get_mut(home).army.retain(|unit, _| !unit.is_orbital());

    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<AudioSource>()
        .init_resource::<WorldAssets>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<Time>()
        .init_resource::<UiState>()
        .init_resource::<Settings>()
        .init_resource::<Missions>()
        .init_resource::<MultiplayerSession>()
        .insert_resource(model.map)
        .insert_resource(player)
        .add_systems(Startup, draw_map)
        .add_systems(Update, update_planet_info);
    app.world_mut().spawn((Camera2d, MainCamera));
    app.update();

    let planet_entity = app
        .world_mut()
        .query::<(Entity, &PlanetCmp)>()
        .iter(app.world())
        .find(|(_, planet)| planet.id == home)
        .unwrap()
        .0;
    let orbital_icon = app
        .world()
        .get::<Children>(planet_entity)
        .unwrap()
        .iter()
        .find(|&child| app.world().get::<Icon>(child) == Some(&Icon::Orbitals))
        .unwrap();

    assert_eq!(app.world().get::<Visibility>(orbital_icon), Some(&Visibility::Hidden));

    app.world_mut().resource_mut::<UiState>().planet_hover = Some(home);
    app.update();
    assert_eq!(app.world().get::<Visibility>(orbital_icon), Some(&Visibility::Inherited));

    app.world_mut().resource_mut::<UiState>().planet_hover = None;
    app.world_mut()
        .resource_mut::<Map>()
        .get_mut(home)
        .army
        .insert(Unit::Building(crate::core::units::buildings::Building::SolarSatellite), 1);
    app.update();
    assert_eq!(app.world().get::<Visibility>(orbital_icon), Some(&Visibility::Inherited));
}

#[test]
fn modal_game_menus_hide_planet_details_even_with_selection_and_show_info() {
    for menu in [GameState::GameMenu, GameState::Settings, GameState::EndGame] {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_state::<GameState>()
            .insert_resource(UiState {
                planet_hover: Some(1),
                planet_selected: Some(1),
                ..default()
            })
            .insert_resource(Settings {
                show_info: true,
                ..default()
            })
            .add_systems(OnEnter(menu), hide_planet_details);

        let planet = app.world_mut().spawn((PlanetCmp::new(1), Visibility::Inherited)).id();
        let name = app.world_mut().spawn((PlanetNameCmp, Visibility::Inherited)).id();
        let resources = app.world_mut().spawn((PlanetResourcesCmp, Visibility::Inherited)).id();
        let icon = app.world_mut().spawn((Icon::Fleet, Visibility::Inherited)).id();
        let scanner = app.world_mut().spawn((ScannerCmp::default(), Visibility::Inherited)).id();
        let defense = app.world_mut().spawn((SpaceDockCmp, Visibility::Inherited)).id();
        let details = [name, resources, icon, scanner];
        app.world_mut().entity_mut(planet).add_children(&details).add_child(defense);

        app.update();
        for entity in details {
            assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Inherited);
        }

        app.world_mut().resource_mut::<NextState<GameState>>().set(menu);
        app.update();

        assert_eq!(*app.world().resource::<State<GameState>>().get(), menu);
        for entity in details {
            assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
        }
        for entity in [planet, defense] {
            assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Inherited);
        }
        assert_eq!(app.world().resource::<UiState>().planet_selected, Some(1));
        assert!(app.world().resource::<Settings>().show_info);
    }
}

#[test]
fn modal_game_menus_hide_turn_controls() {
    let mut app = App::new();
    app.insert_resource(State::new(GameState::Playing))
        .insert_resource(Player::new(1, 0))
        .insert_resource(UiState {
            end_turn: true,
            ..default()
        })
        .insert_resource(crate::multiplayer::client::PendingTurnCommands {
            submission: crate::multiplayer::client::SubmissionState::Accepted,
            ..default()
        })
        .add_systems(Update, update_end_turn);
    let button = app
        .world_mut()
        .spawn((Visibility::Hidden, Text::new("End turn"), EndTurnButtonCmp, MainButtonLabelCmp))
        .id();
    let waiting = app.world_mut().spawn((Visibility::Hidden, EndTurnLabelCmp)).id();
    let spectator = app.world_mut().spawn((Visibility::Hidden, SpectatorLabelCmp)).id();

    app.update();
    assert_eq!(*app.world().get::<Visibility>(button).unwrap(), Visibility::Inherited);
    assert_eq!(*app.world().get::<Visibility>(waiting).unwrap(), Visibility::Inherited);
    assert_eq!(app.world().get::<Text>(button).unwrap().0, "Continue turn");
    assert_eq!(*app.world().get::<Visibility>(spectator).unwrap(), Visibility::Hidden);

    for state in [GameState::GameMenu, GameState::Settings, GameState::EndGame] {
        app.insert_resource(State::new(state));
        app.update();
        for entity in [button, waiting, spectator] {
            assert_eq!(
                *app.world().get::<Visibility>(entity).unwrap(),
                Visibility::Hidden,
                "{state:?} leaves a turn control visible"
            );
        }
    }
}

#[test]
fn ownership_cells_render_above_background_in_local_and_multiplayer_games() {
    for player_count in [1, 2] {
        let mut model = GameModel::new(
            [7; 32],
            GameRules {
                player_count,
                practice_mode: player_count == 1,
                ..default()
            },
        )
        .unwrap();
        if player_count == 2 {
            model.players[0].color = PlayerColor::new(4).unwrap();
        }
        model.start().unwrap();
        let player = model.players[0].clone();
        let expected_color = player.color().color();
        if player_count == 1 {
            assert_eq!(expected_color, OWN_COLOR, "local games default to blue");
        }
        let home = player.home_planet;
        let public_enemy = (player_count == 2).then(|| model.players[1].home_planet);
        let public_enemy_color = public_enemy.map(|_| model.players[1].color().color());
        let planet_count = model.map.planets.len();
        let mut session = MultiplayerSession::default();
        session.local_practice = player_count == 1;
        session.active_game = Some(GameRecord {
            submitted_players: Vec::new(),
            id: GameId::new("voronoi-test"),
            code: GameCode::new("ABCDEF"),
            revision: 0,
            saved_at: 1_700_000_000,
            max_players: player_count,
            status: model.status,
            persisted: PersistedGame::new(model.clone()),
            members: vec![],
        });
        let mut app = App::new();
        app.add_plugins(TransformPlugin)
            .init_resource::<Time>()
            .init_resource::<Assets<Mesh>>()
            .init_resource::<Assets<ColorMaterial>>()
            .init_resource::<Settings>()
            .insert_resource(model.map)
            .insert_resource(Missions(model.missions))
            .insert_resource(player)
            .insert_resource(session)
            .add_systems(
                Startup,
                |mut commands: Commands,
                 map: Res<Map>,
                 mut meshes: ResMut<Assets<Mesh>>,
                 mut materials: ResMut<Assets<ColorMaterial>>| {
                    spawn_voronoi_cells(&mut commands, &map, &mut meshes, &mut materials);
                },
            )
            .add_systems(Update, update_voronoi);
        app.update();

        let world = app.world_mut();
        let mut cells = world.query::<(
            Entity,
            &VoronoiCmp,
            &Visibility,
            &GlobalTransform,
            &Mesh2d,
            &MeshMaterial2d<ColorMaterial>,
        )>();
        assert_eq!(cells.iter(world).count(), planet_count);
        let mut home_entity = None;
        let mut public_enemy_entity = None;
        for (entity, cell, visibility, transform, mesh, material) in cells.iter(world) {
            assert!(transform.translation().z > BACKGROUND_Z);
            assert!(transform.translation().z < PLANET_Z);
            let positions = world
                .resource::<Assets<Mesh>>()
                .get(&mesh.0)
                .unwrap()
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .unwrap()
                .as_float3()
                .unwrap();
            assert!(positions.iter().all(|position| position[2] == 0.0));
            if cell.0 == home {
                home_entity = Some(entity);
                assert_eq!(*visibility, Visibility::Inherited);
                assert_eq!(
                    world.resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
                    expected_color.with_alpha(0.01)
                );
            } else {
                if Some(cell.0) == public_enemy {
                    public_enemy_entity = Some(entity);
                }
                assert_eq!(*visibility, Visibility::Hidden, "unknown territory stays hidden");
            }
        }
        let home_entity = home_entity.expect("home world has an ownership cell");
        let mut edges = world.query::<(
            &VoronoiEdgeCmp,
            &Visibility,
            &GlobalTransform,
            &MeshMaterial2d<ColorMaterial>,
        )>();
        let mut home_edges = 0;
        for (edge, visibility, transform, material) in edges.iter(world) {
            assert!(transform.translation().z > VORONOI_Z);
            assert!(transform.translation().z < PLANET_Z);
            if edge.planet == home {
                home_edges += 1;
                assert_eq!(*visibility, Visibility::Inherited);
                assert_eq!(
                    world.resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
                    expected_color.with_alpha(0.58)
                );
            }
        }
        assert!(home_edges >= 3);

        if let Some(enemy) = public_enemy {
            let enemy_entity = public_enemy_entity.unwrap();
            let enemy_material =
                app.world().get::<MeshMaterial2d<ColorMaterial>>(enemy_entity).unwrap().0.clone();
            app.world_mut().resource_mut::<Map>().get_mut(enemy).army.insert(Unit::space_dock(), 1);
            app.update();
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(TERRITORY_TRANSITION_SECONDS));
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(enemy_entity).unwrap(),
                Visibility::Inherited
            );
            assert_eq!(
                app.world().resource::<Assets<ColorMaterial>>().get(&enemy_material).unwrap().color,
                public_enemy_color.unwrap().with_alpha(0.01),
                "a public Space Dock reveals the owner's Voronoi cell"
            );

            app.world_mut().resource_mut::<Map>().get_mut(enemy).army.remove(&Unit::space_dock());
            app.update();
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(TERRITORY_TRANSITION_SECONDS));
            app.update();
            assert_eq!(
                *app.world().get::<Visibility>(enemy_entity).unwrap(),
                Visibility::Hidden,
                "the cell disappears again when its only public ownership source is gone"
            );
            app.world_mut().resource_mut::<Time>().advance_by(Duration::ZERO);
        }

        app.world_mut().resource_mut::<Settings>().show_cells = false;
        app.update();
        let world = app.world_mut();
        assert!(world
            .query_filtered::<&Visibility, With<MapCmp>>()
            .iter(world)
            .all(|visibility| *visibility == Visibility::Hidden));
        app.world_mut().resource_mut::<Settings>().show_cells = true;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(home_entity).unwrap(), Visibility::Inherited);

        let home_material =
            app.world().get::<MeshMaterial2d<ColorMaterial>>(home_entity).unwrap().0.clone();
        app.world_mut().resource_mut::<Map>().get_mut(home).controlled = None;
        app.update();
        assert_eq!(*app.world().get::<Visibility>(home_entity).unwrap(), Visibility::Inherited);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(TERRITORY_TRANSITION_SECONDS * 0.5));
        app.update();
        let fading_alpha = app
            .world()
            .resource::<Assets<ColorMaterial>>()
            .get(&home_material)
            .unwrap()
            .color
            .alpha();
        assert!(fading_alpha > 0.0 && fading_alpha < 0.01);
        assert_eq!(*app.world().get::<Visibility>(home_entity).unwrap(), Visibility::Inherited);

        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(TERRITORY_TRANSITION_SECONDS));
        app.update();
        assert_eq!(*app.world().get::<Visibility>(home_entity).unwrap(), Visibility::Hidden);
        assert_eq!(
            app.world()
                .resource::<Assets<ColorMaterial>>()
                .get(&home_material)
                .unwrap()
                .color
                .alpha(),
            0.0
        );
    }
}
