use super::*;
use strum::IntoEnumIterator;

fn creature(fauna: SpaceFauna, slot: usize, count: usize) -> Creature {
    Creature {
        fauna,
        slot,
        count,
        survives: true,
    }
}

#[test]
fn creature_sizes_show_strength_without_dwarfing_the_mission() {
    use SpaceFauna::*;

    let size = |fauna| creature(fauna, 0, 1).size();
    assert!((48.0..60.0).contains(&size(AetherRay)), "weak fauna should be slightly smaller");
    assert!(
        (100.0..120.0).contains(&size(NullstarBehemoth)),
        "apex fauna should be larger while still fitting the encounter"
    );
    let mut fauna = SpaceFauna::iter().collect::<Vec<_>>();
    fauna.sort_by_key(|fauna| fauna.production());
    for pair in fauna.windows(2) {
        if pair[0].production() < pair[1].production() {
            assert!(size(pair[0]) < size(pair[1]));
        }
    }
    for (adult, young) in [
        (VoidManta, VoidMantaCalf),
        (CrystalLeviathan, CrystalShardling),
        (StarKraken, StarKrakenSpawn),
        (NebulaGrazer, NebulaGrazerCalf),
        (ElderStarDragon, StarDragonWyrmling),
    ] {
        assert!(size(adult) > size(young) * 1.15, "{young:?} must look smaller than its parent");
    }
}

#[test]
fn creatures_circle_the_mission_then_cross_it_with_continuous_flight() {
    for fauna in SpaceFauna::iter() {
        for slot in 0..3 {
            let creature = creature(fauna, slot, 3);
            let delay = creature.delay();
            let a = creature.position(ENTRY_END + delay + 0.01);
            let b = creature.position(ENTRY_END + delay + 0.38);
            assert!(a.dot(b) < 0.0, "{fauna:?} must fly around to the other side of the mission");
            assert!(creature.position(STRIKE_END + delay).length() < 0.01);
            for join in [ENTRY_END, ORBIT_END, STRIKE_END] {
                let t = join + delay;
                let before = (creature.position(t) - creature.position(t - 0.001)) / 0.001;
                let after = (creature.position(t + 0.001) - creature.position(t)) / 0.001;
                assert!(
                    before.distance(after) < 10.0,
                    "{fauna:?}: velocity jumps at {join}: {before:?}, {after:?}"
                );
            }
            for frame in 0..84 {
                let t = frame as f32 / 30.0;
                assert!(creature.position(t).is_finite());
                assert!(creature.position(t).length() < 200.0);
                assert!(creature.mouth(t).is_finite());
            }
        }
    }
}

#[test]
fn mesh_animation_pins_each_mouth_and_articulates_the_body() {
    for fauna in SpaceFauna::iter() {
        let mouth = anatomy(fauna).0;
        for frame in 0..84 {
            assert!(deform(fauna, mouth, frame as f32 / 30.0).distance(mouth) < 0.00001);
        }
        let moved = [Vec2::new(-0.4, 0.35), Vec2::new(0.3, -0.3), Vec2::new(0.4, 0.3)]
            .iter()
            .any(|point| deform(fauna, *point, 0.1).distance(deform(fauna, *point, 0.3)) > 0.001);
        assert!(moved, "{fauna:?} must articulate rather than rotate a rigid card");
    }
}

#[test]
fn pack_orbits_keep_two_or_three_separate_silhouettes() {
    for fauna in SpaceFauna::iter() {
        for count in [2, 3] {
            let pack = (0..count).map(|slot| creature(fauna, slot, count)).collect::<Vec<_>>();
            for frame in 14..32 {
                let t = frame as f32 / 30.0;
                for (index, a) in pack.iter().enumerate() {
                    for b in &pack[index + 1..] {
                        assert!(a.position(t).distance(b.position(t)) > a.size());
                    }
                }
            }
        }
    }
}

#[test]
fn creature_weapons_stay_attached_to_the_rotated_mouth_and_reuse_combat_colors() {
    for fauna in SpaceFauna::iter() {
        let creature = creature(fauna, 1, 3);
        let mut effect = FaunaEffect {
            report: 1,
            mission: 1,
            turn: 1,
            timer: Timer::from_seconds(FAUNA_AFTERMATH_SECONDS, TimerMode::Once),
            destroyed: false,
            victory: false,
            fleet_weapon: None,
            mission_rotation: 0.7,
            mission_size: 50.0,
            creatures: vec![creature],
            sounds: 0,
        };
        let weapon = Weapon::for_unit(Unit::Fauna(fauna));
        for time in [1.04, 1.22, 1.35] {
            let time = time + creature.delay();
            let shot = shot_sample(&effect, creature, false, time);
            assert!(shot.len > 0);
            assert!(shot.parts[0].position.distance(creature.mouth(time)) < 0.001);
            assert_eq!(shot.parts[0].color.with_alpha(1.0), weapon.color());
            if time > 1.11 + creature.delay() && weapon.beam_width().is_some() {
                let beam = shot.parts[1];
                let origin = beam.position - Vec2::from_angle(beam.angle) * beam.size.x * 0.5;
                assert!(origin.distance(creature.mouth(time)) < 0.001);
            }
        }
        assert_eq!(shot_sample(&effect, creature, true, 0.6).len, 0);
        effect.fleet_weapon = Some(Weapon::Laser);
        assert!(shot_sample(&effect, creature, true, 0.6).len > 0);
    }
}

/// Uses the actual Bevy map renderer, including deforming meshes and shared combat textures.
/// Kept out of CI because it needs a local GPU; outputs only to the ignored target directory.
#[test]
#[ignore = "renders map fauna encounter clips with a local GPU"]
#[cfg(target_os = "windows")]
fn render_fauna_map_preview() {
    use crate::core::units::ships::Ship;
    use bevy::camera::RenderTarget;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages},
        view::screenshot::{save_to_disk, Screenshot},
        RenderPlugin,
    };
    use bevy::time::TimeUpdateStrategy;
    use bevy::window::ExitCondition;
    use bevy::winit::WinitPlugin;
    use std::time::Duration;

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .init_asset::<bevy_kira_audio::AudioSource>()
    .init_resource::<WorldAssets>()
    .init_resource::<Settings>()
    .init_resource::<SuppressedMapMissions>()
    .add_message::<PlayAudioMsg>()
    .insert_resource(State::new(GameState::GameMenu))
    .insert_resource(ClearColor(Color::srgb(0.012, 0.02, 0.04)))
    .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(1.0 / 30.0)))
    .add_systems(Update, animate_fauna_aftermath);
    app.finish();
    app.cleanup();
    app.world_mut()
        .run_system_once(
            |mut assets: ResMut<WorldAssets>,
             server: Res<AssetServer>,
             mut layouts: ResMut<Assets<TextureAtlasLayout>>| {
                assets.begin_gameplay_loading(&server, &mut layouts);
            },
        )
        .unwrap();
    let load = |app: &mut App, path: &str| {
        let data = image::open(path).unwrap();
        let mut image = Image::from_dynamic(data, true, RenderAssetUsages::default());
        image.sampler = bevy::image::ImageSampler::linear();
        app.world_mut().resource_mut::<Assets<Image>>().add(image)
    };
    for fauna in SpaceFauna::iter() {
        let key = format!("map {}", fauna.to_lowername());
        let image = load(&mut app, &format!("assets/images/fauna-map/{key}.png"));
        app.world_mut().resource_mut::<WorldAssets>().images.insert(key, image);
    }
    let mission = load(&mut app, "assets/images/icons/mission.png");
    app.world_mut().resource_mut::<WorldAssets>().images.insert("mission".into(), mission);
    let atlas = app.world().resource::<WorldAssets>().texture("explosion").image;
    let pixels = image::open("assets/images/animations/explosion.png").unwrap();
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .insert(atlas.id(), Image::from_dynamic(pixels, true, RenderAssetUsages::default()))
        .unwrap();
    let mut render_image = Image::new_uninit(
        Extent3d {
            width: 1600,
            height: 1400,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    render_image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(render_image);
    let camera = app.world_mut().spawn((Camera2d, RenderTarget::Image(target.clone().into()))).id();
    let mut textures = EffectTextures::default();
    textures.initialize(&mut app.world_mut().resource_mut::<Assets<Image>>());
    app.insert_resource(PreviewTextures(textures));

    #[derive(Resource)]
    struct PreviewTextures(EffectTextures);
    #[derive(Component)]
    struct PreviewLabel;

    for groups in [false, true] {
        if let Projection::Orthographic(projection) =
            &mut *app.world_mut().get_mut::<Projection>(camera).unwrap()
        {
            projection.scale = if groups {
                0.55
            } else {
                1.0
            };
        }
        let fauna = SpaceFauna::iter().collect::<Vec<_>>();
        let scenes = if groups {
            4
        } else {
            fauna.len()
        };
        for (index, kind) in fauna.iter().enumerate().take(scenes) {
            let (pack, destroyed, label, position) = if groups {
                let (pack, destroyed, label) = match index {
                    0 => (
                        vec![SpaceFauna::VoidManta, SpaceFauna::VoidMantaCalf],
                        false,
                        "TWO MANTAS / MISSION SURVIVES",
                    ),
                    1 => (vec![SpaceFauna::AetherRay; 3], false, "THREE RAYS / MISSION SURVIVES"),
                    2 => (
                        vec![SpaceFauna::ElderStarDragon, SpaceFauna::StarDragonWyrmling],
                        true,
                        "DRAGON PAIR / MISSION LOST",
                    ),
                    _ => (
                        vec![
                            SpaceFauna::StarKraken,
                            SpaceFauna::StarKrakenSpawn,
                            SpaceFauna::StarKrakenSpawn,
                        ],
                        true,
                        "KRAKEN BROOD / MISSION LOST",
                    ),
                };
                (
                    pack,
                    destroyed,
                    label.to_string(),
                    Vec2::new(
                        (index % 2) as f32 * 390.0 - 195.0,
                        165.0 - (index / 2) as f32 * 330.0,
                    ),
                )
            } else {
                (
                    vec![*kind],
                    true,
                    kind.to_lowername().to_uppercase(),
                    Vec2::new(
                        (index % 4) as f32 * 390.0 - 585.0,
                        490.0 - (index / 4) as f32 * 330.0,
                    ),
                )
            };
            let outcome = FaunaPresentation {
                mission: index as u64,
                position,
                mission_image: "mission".into(),
                mission_rotation: 0.0,
                mission_size: 50.0,
                mission_flip_x: false,
                mission_flip_y: false,
                return_fire: Some(Unit::Ship(Ship::LightFighter)),
                creature_survivors: vec![destroyed; pack.len()],
                creatures: pack,
                destroyed,
                victory: !destroyed,
                label: if destroyed {
                    "MISSION LOST"
                } else {
                    "MISSION SURVIVES"
                },
            };
            app.world_mut()
                .run_system_once(
                    move |mut commands: Commands,
                          assets: Res<WorldAssets>,
                          mut meshes: ResMut<Assets<Mesh>>,
                          mut materials: ResMut<Assets<ColorMaterial>>,
                          textures: Res<PreviewTextures>| {
                        spawn_fauna_aftermath(
                            &mut commands,
                            index as u64,
                            1,
                            position,
                            &outcome,
                            Color::srgb(0.28, 0.68, 1.0),
                            &assets,
                            &mut meshes,
                            &mut materials,
                            &textures.0,
                        );
                        // Close-up group clips keep only the in-game outcome captions, leaving
                        // the expanding rings clear of the neighboring scene's heading.
                        if !groups {
                            commands.spawn((
                                Text2d::new(label.clone()),
                                TextFont {
                                    font_size: 18.0.into(),
                                    ..default()
                                },
                                TextColor(Color::srgb(0.72, 0.82, 0.95)),
                                Transform::from_translation(
                                    (position + Vec2::Y * 190.0).extend(EXPLOSION_Z + 1.0),
                                ),
                                PreviewLabel,
                            ));
                        }
                    },
                )
                .unwrap();
        }
        app.world_mut().resource_mut::<Settings>().turn = 1;
        app.insert_resource(State::new(GameState::GameMenu));
        for _ in 0..20 {
            app.update();
        }
        app.insert_resource(State::new(GameState::Playing));
        let directory = if groups {
            "target/fauna-map-preview/groups"
        } else {
            "target/fauna-map-preview/all"
        };
        std::fs::create_dir_all(directory).unwrap();
        let frame_count = (FAUNA_AFTERMATH_SECONDS * 30.0).ceil() as usize;
        for frame in 0..frame_count {
            // Sample [0, duration), before cleanup hands surviving missions back to the map.
            app.insert_resource(TimeUpdateStrategy::ManualDuration(if frame == 0 {
                Duration::ZERO
            } else {
                Duration::from_secs_f32(1.0 / 30.0)
            }));
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(save_to_disk(format!("{directory}/frame-{frame:03}.png")));
            app.update();
        }
        app.insert_resource(State::new(GameState::GameMenu));
        for _ in 0..10 {
            app.update();
        }
        let entities = app
            .world_mut()
            .query_filtered::<Entity, Or<(With<FaunaEffect>, With<PreviewLabel>)>>()
            .iter(app.world())
            .collect::<Vec<_>>();
        for entity in entities {
            app.world_mut().despawn(entity);
        }
        assert!(std::path::Path::new(directory)
            .join(format!("frame-{:03}.png", frame_count - 1))
            .exists());
    }
}
