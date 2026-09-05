use super::*;
use crate::core::combat::report::MissionReport;
use crate::core::identity::{GameCode, GameId};
use crate::core::player::{PlayerColor, PLAYER_COLOR_PALETTE};
use crate::core::simulation::{GameModel, GameRules, PersistedGame};
use crate::multiplayer::model::GameRecord;
use bevy::color::ColorToComponents;
use bevy_kira_audio::AudioSource;

/// Finds the defenses spawned by the real map setup without a window or GPU.
fn test_defenses(
    world: &mut World,
    planet: PlanetId,
) -> (Entity, Entity, Entity, Handle<ColorMaterial>) {
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
    let material = world.get::<MeshMaterial2d<ColorMaterial>>(shield).unwrap().0.clone();
    (shield, dock, gate, material)
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
fn scenery_uses_all_corners_and_keeps_most_of_the_large_sun_beyond_the_playfield() {
    assert_eq!(scenery_corner_from_seed(0), Vec2::new(-1.0, -1.0));
    assert_eq!(scenery_corner_from_seed(1), Vec2::new(1.0, -1.0));
    assert_eq!(scenery_corner_from_seed(2), Vec2::new(-1.0, 1.0));
    assert_eq!(scenery_corner_from_seed(3), Vec2::ONE);

    let map = Map {
        rect: Rect::new(-1_600.0, -900.0, 1_600.0, 900.0),
        planets: Vec::new(),
    };
    let corner = map_scenery_corner(&map);
    let sun_corner_position = map_corner(&map, corner);
    let position = solar_star_position(&map);
    let outside = (position - sun_corner_position) * corner;
    assert!(outside.x > 0.0 && outside.y > 0.0);
    let radius = SOLAR_STAR_SIZE * 0.5;
    assert!(outside.x < radius && outside.y < radius);
    assert!(outside.x > radius * 0.5 && outside.y > radius * 0.5);

    let celestial = celestial_position(&map);
    let opposite_edge_x = if corner.x > 0.0 {
        map.rect.min.x
    } else {
        map.rect.max.x
    };
    assert!((celestial.x - opposite_edge_x) * corner.x < 0.0);
    assert!((celestial.x - map.rect.center().x) * corner.x < 0.0);
    assert!((map.rect.min.y..=map.rect.max.y).contains(&celestial.y));
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
fn every_game_has_a_corner_sun_and_one_landmark_with_appropriate_camera_depth() {
    for expected_kind in CelestialKind::ALL {
        let map = (0..64)
            .map(|seed| GameModel::new([seed; 32], GameRules::default()).unwrap().map)
            .find(|map| map_scenery_selection(map) == expected_kind)
            .unwrap();
        let anchor = celestial_position(&map).extend(CELESTIAL_DEPTH);
        let sun_anchor = solar_star_position(&map).extend(SOLAR_STAR_DEPTH);
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
                if let Some(celestial) = world.get::<CelestialCmp>(child) {
                    assert_eq!(celestial.kind, CelestialKind::NeutronStar);
                    assert_eq!(parallax.drift, Vec2::ZERO);
                }
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
            assert!((color.red - 0.72).abs() < f32::EPSILON);
            assert!((color.green - 0.72).abs() < f32::EPSILON);
            assert!((color.blue - 0.72).abs() < f32::EPSILON);
            assert!((0.0..=kind.opacity()).contains(&color.alpha));
        }

        assert_eq!(nebula_follow, Some(NEBULA_PARALLAX_FOLLOW));
        let mut landmarks = world.query::<(&CelestialCmp, &GlobalTransform)>();
        let (celestial, transform) = landmarks.single(world).unwrap();
        assert_eq!(celestial.kind, expected_kind);
        assert_eq!(celestial.frames.len(), kind.frame_count());
        assert_eq!(transform.translation(), anchor);

        // The distant neutron star moves only a tenth as far on screen as map objects.
        let camera_position = Vec3::new(-2_000.0, 700.0, 1.0);
        world.get_mut::<Transform>(camera).unwrap().translation = camera_position;
        app.update();
        let world = app.world_mut();
        let pan_position = landmarks.single(world).unwrap().1.translation();
        let relative_motion =
            pan_position.truncate() - anchor.truncate() - camera_position.truncate();
        let motion_fraction = if kind == CelestialKind::NeutronStar {
            0.1
        } else {
            1.0
        };
        assert!(relative_motion.abs_diff_eq(-camera_position.truncate() * motion_fraction, 1e-3));

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
        if kind == CelestialKind::NeutronStar {
            let expected_scale = 0.5_f32.powf(0.8);
            let expected_position =
                anchor.truncate() * expected_scale + camera_position.truncate() * 0.9;
            assert!(transform.translation().truncate().abs_diff_eq(expected_position, 1e-3));
            assert!((transform.scale().x - expected_scale).abs() < 1e-5);
            assert_eq!(transform.translation().z, anchor.z);
        } else {
            assert_eq!(transform.translation(), anchor);
        }
        let mut suns = world.query_filtered::<&GlobalTransform, With<SolarStarCmp>>();
        assert_eq!(suns.single(world).unwrap().translation(), sun_anchor);
    }
}

#[test]
fn celestial_landmarks_stay_beyond_the_playfield_even_on_small_or_offset_maps() {
    let generated =
        (0..64).map(|seed| GameModel::new([seed; 32], GameRules::default()).unwrap().map);
    let unusual = [
        Rect::new(-50.0, -50.0, 50.0, 50.0),
        Rect::new(1_000.0, -700.0, 1_050.0, 900.0),
        Rect::new(-4_000.0, 2_000.0, 4_000.0, 2_040.0),
    ]
    .map(|rect| Map {
        rect,
        planets: Vec::new(),
    });
    for map in generated.chain(unusual) {
        let position = celestial_position(&map);
        let normalized = (position - map.rect.center()) / map.rect.half_size();
        assert!(normalized.x.abs() > 1.0);
        let nearest_sprite_edge = (position.x - map.rect.center().x).abs() - CELESTIAL_SIZE.x * 0.5;
        assert!(nearest_sprite_edge >= map.rect.half_size().x - 1e-3);
        assert!(normalized.y.abs() <= 1.0);
        assert!(normalized.x * map_scenery_corner(&map).x < 0.0);
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
fn defense_colors_follow_known_controllers_and_keep_their_hue_while_pulsing() {
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
            update_planet_defenses.before(bevy_tweening::AnimationSystem::AnimationUpdate),
        );
    app.world_mut().spawn((Camera2d, MainCamera));
    app.update();
    let own = test_defenses(app.world_mut(), own_home);
    let enemy = test_defenses(app.world_mut(), enemy_home);
    let unknown = test_defenses(app.world_mut(), unknown_home);

    // An unseen capture must not leak the new controller through the defense color.
    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).controlled = Some(3);
    for (index, color) in PLAYER_COLOR_PALETTE.into_iter().enumerate() {
        let enemy_color = PLAYER_COLOR_PALETTE[(index + 2) % PLAYER_COLOR_PALETTE.len()];
        {
            let mut session = app.world_mut().resource_mut::<MultiplayerSession>();
            let model = &mut session.active_game.as_mut().unwrap().persisted.state;
            model.player_mut(1).unwrap().color = Some(color);
            model.player_mut(2).unwrap().color = Some(enemy_color);
        }
        app.update();
        for ((shield, dock, gate, material), expected) in [(&own, color), (&enemy, enemy_color)] {
            assert_eq!(*app.world().get::<Visibility>(*shield).unwrap(), Visibility::Inherited);
            assert_eq!(*app.world().get::<Visibility>(*dock).unwrap(), Visibility::Inherited);
            assert_eq!(*app.world().get::<Visibility>(*gate).unwrap(), Visibility::Inherited);
            assert_eq!(app.world().get::<Sprite>(*dock).unwrap().color, expected.color());
            assert_eq!(app.world().get::<Sprite>(*gate).unwrap().color, expected.color());
            let mut alphas = Vec::new();
            // Step across loop boundaries at a frame interval that does not divide the period.
            // Check the rendered material too: an opaque material ignores the animated alpha.
            for _ in 0..354 {
                app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(17));
                app.update();
                let actual = app.world().resource::<Assets<ColorMaterial>>().get(material).unwrap();
                assert_eq!(actual.alpha_mode, bevy::sprite_render::AlphaMode2d::Blend);
                assert!(actual
                    .color
                    .with_alpha(1.)
                    .to_srgba()
                    .to_vec4()
                    .abs_diff_eq(expected.color().to_srgba().to_vec4(), 1e-6));
                alphas.push(actual.color.alpha());
            }
            for cycle in alphas.as_chunks::<177>().0 {
                assert!(cycle.iter().any(|&alpha| alpha < 0.01), "shield fades fully out");
                assert!(cycle.iter().any(|&alpha| alpha > 0.9), "shield becomes visible again");
            }
            for pair in alphas.windows(2) {
                let change = (pair[1] - pair[0]).abs();
                assert!(change < 0.02, "shield must not pop between frames: {pair:?}");
                if pair[0] < 0.001 || pair[0] > 0.949 {
                    assert!(change < 0.002, "shield must gently reverse its fade: {pair:?}");
                }
            }
        }
        for entity in [unknown.0, unknown.1, unknown.2] {
            assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
        }
    }

    // Once the local player captures it, both defenses adopt the new controller immediately.
    let alpha_before_capture =
        app.world().resource::<Assets<ColorMaterial>>().get(&enemy.3).unwrap().color.alpha();
    let elapsed_before_capture =
        app.world().get::<TweenAnim>(enemy.0).unwrap().tweenable().elapsed();
    app.world_mut().resource_mut::<Time>().advance_by(Duration::ZERO);
    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).controlled = Some(1);
    app.update();
    let own_color = app.world().get::<Sprite>(own.1).unwrap().color;
    assert_eq!(app.world().get::<Sprite>(enemy.1).unwrap().color, own_color);
    assert_eq!(app.world().get::<Sprite>(enemy.2).unwrap().color, own_color);
    assert_eq!(app.world().get::<PlanetaryShieldCmp>(enemy.0).unwrap().color, Some(own_color));
    assert_eq!(
        app.world().get::<TweenAnim>(enemy.0).unwrap().tweenable().elapsed(),
        elapsed_before_capture,
        "changing controller must not restart the fade"
    );
    let alpha_after_capture =
        app.world().resource::<Assets<ColorMaterial>>().get(&enemy.3).unwrap().color.alpha();
    assert!((alpha_after_capture - alpha_before_capture).abs() < 1e-6);

    app.world_mut().resource_mut::<Map>().get_mut(enemy_home).is_destroyed = true;
    app.update();
    for entity in [enemy.0, enemy.1, enemy.2] {
        assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
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
fn scanner_ranges_follow_owned_planet_and_controlled_moon_hover() {
    let mut model = GameModel::new([31; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    model.players[0].color = PlayerColor::new(4);
    let player = model.players[0].clone();
    let scanner_color = player.color().color();
    let home = player.home_planet;
    let enemy = model.players[1].home_planet;
    let moons = model.map.moons().iter().map(|moon| moon.id).collect::<Vec<_>>();
    let moon = moons[0];
    let enemy_moon = moons[1];
    model.map.get_mut(moon).controlled = Some(player.id);
    model.map.get_mut(enemy_moon).controlled = Some(model.players[1].id);
    for id in [home, enemy] {
        model.map.get_mut(id).army.insert(Unit::Building(Building::SensorPhalanx), 2);
    }
    for id in [moon, enemy_moon] {
        model.map.get_mut(id).army.insert(Unit::Building(Building::OrbitalRadar), 3);
    }

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
        .insert_resource(model.map)
        .insert_resource(player)
        .add_systems(Startup, draw_map)
        .add_systems(Update, update_planet_info);
    app.world_mut().spawn((Camera2d, MainCamera));

    for (hover, selected, expected) in [
        (Some(home), None, Some(home)),
        (None, Some(home), None), // Moving to the selected planet's shop hides the range.
        (Some(moon), Some(home), Some(moon)),
        (None, Some(moon), None),
        (Some(enemy), None, None),
        (Some(enemy_moon), None, None),
    ] {
        app.insert_resource(UiState {
            planet_hover: hover,
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
            assert_eq!(*visibility == Visibility::Inherited, expected == Some(id));
            if expected != Some(id) {
                continue;
            }
            visible += 1;
            assert_eq!(
                world.resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
                scanner_color
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
            let expected_radius = if id == home {
                250.0 // Two phalanx levels plus the planet radius.
            } else {
                395.0 // Three radar levels plus the moon radius.
            };
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
        if let Some(id) = expected {
            let expected_radius = if id == home {
                250.0
            } else {
                395.0
            };
            assert!(
                (outer_radius - expected_radius).abs() < 0.01,
                "scanner uses its installed level"
            );
        }
    }

    // An absent scanner and a destroyed world have no range to display.
    app.world_mut()
        .resource_mut::<Map>()
        .get_mut(home)
        .army
        .remove(&Unit::Building(Building::SensorPhalanx));
    app.world_mut().resource_mut::<Map>().get_mut(moon).is_destroyed = true;
    for id in [home, moon] {
        app.world_mut().resource_mut::<UiState>().planet_hover = Some(id);
        app.update();
        let world = app.world_mut();
        assert!(world
            .query_filtered::<&Visibility, With<ScannerCmp>>()
            .iter(world)
            .all(|visibility| *visibility == Visibility::Hidden));
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
            model.players[0].color = PlayerColor::new(4);
        }
        model.start().unwrap();
        let player = model.players[0].clone();
        let expected_color = player.color().color();
        if player_count == 1 {
            assert_eq!(expected_color, OWN_COLOR, "local games default to blue");
        }
        let home = player.home_planet;
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
