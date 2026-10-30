use super::*;
use crate::core::simulation::{GameModel, GameRules};

fn model() -> GameModel {
    let mut model = GameModel::new([7; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    model
}

#[test]
fn atmospheric_overlays_exclude_airless_and_destroyed_worlds_and_never_intercept_input() {
    let mut map = model().map;
    let kinds = [
        PlanetKind::Water,
        PlanetKind::Gas,
        PlanetKind::Ice,
        PlanetKind::Dry,
        PlanetKind::Metallic,
        PlanetKind::Gray,
    ];
    for (index, planet) in map.planets.iter_mut().enumerate() {
        planet.kind = kinds[index % kinds.len()];
        planet.is_destroyed = index % 7 == 0;
    }
    let mut world = World::new();
    world.init_resource::<Assets<Image>>();
    let textures = WeatherTextures::from_world(&mut world);
    spawn_weather(&mut world.commands(), &map, &textures);
    world.flush();
    for (effect, pickable, transform) in
        world.query::<(&PlanetWeather, &Pickable, &Transform)>().iter(&world)
    {
        let planet = map.get(effect.planet);
        assert!(!planet.is_destroyed);
        assert!(matches!(
            planet.kind,
            PlanetKind::Water | PlanetKind::Gas | PlanetKind::Ice | PlanetKind::Dry
        ));
        assert_eq!(*pickable, Pickable::IGNORE);
        assert!(transform.translation.z > PLANET_Z && transform.translation.z < PLANET_Z + 0.05);
    }
    for planet in &map.planets {
        let count = world
            .query::<&PlanetWeather>()
            .iter(&world)
            .filter(|effect| effect.planet == planet.id)
            .count();
        let expected = if planet.is_destroyed {
            0
        } else {
            match planet.kind {
                PlanetKind::Ice => 5,
                PlanetKind::Gas => 4,
                PlanetKind::Water | PlanetKind::Dry => 3,
                _ => 0,
            }
        };
        assert_eq!(count, expected);
    }
}

#[test]
fn clouds_and_weather_remain_inside_the_globe_and_loop_without_a_seam() {
    for kind in [TextureKind::Clouds, TextureKind::Bands, TextureKind::Aurora] {
        for phase in [0.0, 0.4, 1.8, 4.1] {
            for point in [Vec2::ONE, Vec2::new(-0.75, 0.75), Vec2::new(0.0, 1.0)] {
                assert_eq!(pixel(kind, point, phase)[3], 0);
            }
        }
        for x in -8..=8 {
            for y in -8..=8 {
                let point = Vec2::new(x as f32, y as f32) / 10.0;
                let start = pixel(kind, point, 0.0)[3];
                let end = pixel(kind, point, TAU)[3];
                assert!(start.abs_diff(end) <= 1);
            }
        }
    }
    // Negative coordinates cross smoothly; signed fractional parts would introduce seams.
    assert!(
        (noise(Vec3::new(-1.001, 2.3, -0.7)) - noise(Vec3::new(-0.999, 2.3, -0.7))).abs() < 0.01
    );
}

#[test]
fn animation_pauses_hides_during_combat_and_removes_destroyed_weather_without_touching_state() {
    let mut map = model().map;
    map.planets[0].kind = PlanetKind::Ice;
    let before = serde_json::to_value(&map).unwrap();
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<Assets<Image>>()
        .init_resource::<WeatherTextures>()
        .init_resource::<WeatherTime>()
        .insert_resource(map)
        .insert_resource(State::new(GameState::Playing))
        .add_systems(Update, animate_weather);
    app.world_mut().spawn((
        MainCamera,
        Projection::Orthographic(OrthographicProjection {
            scale: 0.5,
            ..OrthographicProjection::default_2d()
        }),
    ));
    app.world_mut().resource_scope(|world, textures: Mut<WeatherTextures>| {
        let map = world.resource::<Map>().clone();
        spawn_weather(&mut world.commands(), &map, &textures);
    });
    app.world_mut().flush();
    let image_count = app.world().resource::<Assets<Image>>().len();
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(8));
    app.update();
    let states = |app: &mut App| {
        app.world_mut()
            .query::<(&Sprite, &Transform)>()
            .iter(app.world())
            .map(|(sprite, transform)| (sprite.rect, sprite.color, transform.translation))
            .collect::<Vec<_>>()
    };
    let playing = states(&mut app);
    assert_eq!(app.world().resource::<WeatherTime>().0, 8.0);
    app.world_mut().insert_resource(State::new(GameState::GameMenu));
    app.update();
    assert_eq!(states(&mut app), playing);
    assert_eq!(serde_json::to_value(app.world().resource::<Map>()).unwrap(), before);
    assert_eq!(app.world().resource::<Assets<Image>>().len(), image_count);
    app.world_mut().insert_resource(State::new(GameState::Combat));
    app.update();
    assert!(app
        .world_mut()
        .query::<&Visibility>()
        .iter(app.world())
        .all(|visibility| *visibility == Visibility::Hidden));
    app.world_mut().insert_resource(State::new(GameState::Playing));
    app.world_mut().resource_mut::<Map>().planets[0].is_destroyed = true;
    app.update();
    assert!(app
        .world_mut()
        .query::<(&PlanetWeather, &Visibility)>()
        .iter(app.world())
        .filter(|(weather, _)| weather.planet == 0)
        .all(|(_, visibility)| *visibility == Visibility::Hidden));
    for mut projection in app
        .world_mut()
        .query_filtered::<&mut Projection, With<MainCamera>>()
        .iter_mut(app.world_mut())
    {
        if let Projection::Orthographic(projection) = &mut *projection {
            projection.scale = 4.0;
        }
    }
    app.update();
    assert!(app
        .world_mut()
        .query::<(&WeatherLayer, &Visibility)>()
        .iter(app.world())
        .filter(|(layer, _)| !matches!(layer, WeatherLayer::Rim))
        .all(|(_, visibility)| *visibility == Visibility::Hidden));
}

/// Opt-in GPU comparison; stays offline and writes only to ignored build output.
#[test]
#[ignore = "opens a native render fixture; requires a GPU and generated runtime assets"]
fn render_planet_atmospheres_and_weather() {
    use crate::core::basis_texture::BasisTexturePlugin;
    use bevy::app::AppExit;
    use bevy::render::view::screenshot::{save_to_disk, Screenshot, ScreenshotCaptured};
    use bevy::winit::WinitPlugin;

    let mut map = model().map;
    map.planets.truncate(4);
    for (index, (planet, kind)) in map
        .planets
        .iter_mut()
        .zip([PlanetKind::Water, PlanetKind::Gas, PlanetKind::Ice, PlanetKind::Dry])
        .enumerate()
    {
        planet.kind = kind;
        planet.image = kind.indices()[0];
        planet.position = Vec2::new(-225.0 + index as f32 * 150.0, -70.0);
    }
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: format!("{}/assets-runtime", env!("CARGO_MANIFEST_DIR")),
                ..default()
            })
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: "Stellarion atmosphere fixture".into(),
                    resolution: (1300, 720).into(),
                    ..default()
                }),
                ..default()
            })
            .set(WinitPlugin {
                run_on_any_thread: true,
            }),
    )
    .add_plugins(BasisTexturePlugin)
    .init_resource::<WeatherTextures>()
    .init_resource::<WeatherTime>()
    .insert_resource(State::new(GameState::Playing))
    .insert_resource(ClearColor(Color::srgb(0.012, 0.02, 0.035)))
    .insert_resource(map)
    .add_systems(
        Startup,
        |mut commands: Commands,
         map: Res<Map>,
         textures: Res<WeatherTextures>,
         server: Res<AssetServer>| {
            commands.spawn((
                Camera2d,
                MainCamera,
                Projection::Orthographic(OrthographicProjection {
                    scale: 0.5,
                    ..OrthographicProjection::default_2d()
                }),
            ));
            for planet in &map.planets {
                for y in [-70.0, 85.0] {
                    commands.spawn((
                        Sprite {
                            image: server
                                .load(format!("images/planets/{}.basisu.ktx2", planet.image())),
                            custom_size: Some(Vec2::splat(planet.size())),
                            ..default()
                        },
                        Transform::from_xyz(planet.position.x, y, PLANET_Z),
                    ));
                }
            }
            for (text, y) in [("Original", 150.0), ("Atmosphere, clouds and weather", -145.0)] {
                commands.spawn((
                    Text2d::new(text),
                    TextFont {
                        font_size: FontSize::Px(14.0),
                        ..default()
                    },
                    Transform::from_xyz(0.0, y, 5.0),
                ));
            }
            spawn_weather(&mut commands, &map, &textures);
        },
    )
    .add_systems(
        Update,
        (animate_weather, |mut commands: Commands, time: Res<Time>, mut captured: Local<bool>| {
            if time.elapsed_secs() > 7.0 && !*captured {
                *captured = true;
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk("target/map-weather-preview.png"))
                    .observe(|_: On<ScreenshotCaptured>, mut exit: MessageWriter<AppExit>| {
                        exit.write(AppExit::Success);
                    });
            }
        })
            .chain(),
    );
    app.run();
}
