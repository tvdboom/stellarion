//! Regression tests for asynchronous arrivals, effect limits and playback clocks.

use super::*;
use crate::core::combat::report::Side;
use crate::core::combat::resolution::ShotReport;
use crate::core::combat::systems::FireState;

fn app() -> App {
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<Settings>()
        .init_resource::<Assets<Image>>()
        .add_message::<SpawnShotMsg>()
        .add_message::<PlayAudioMsg>()
        .add_systems(Update, run_combat_animations);
    app
}

fn unit(app: &mut App, unit: Unit, side: Side, pos: Vec3, hull: usize, shield: usize) -> Entity {
    app.world_mut()
        .spawn((
            CombatUnitCmp {
                unit,
                side,
                hull,
                max_hull: hull,
                shield,
                max_shield: shield,
                fire: FireState::Fired,
            },
            Sprite {
                custom_size: Some(Vec2::splat(100.)),
                ..default()
            },
            Transform::from_translation(pos),
            CombatCmp,
        ))
        .id()
}

fn step(app: &mut App, seconds: f32) {
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(seconds));
    app.update();
}

fn fire(
    app: &mut App,
    source: Entity,
    shooter: Unit,
    target: Unit,
    shot: ShotReport,
    repair: bool,
) {
    app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
        shot: ShotReport {
            unit: Some(target),
            ..shot
        },
        repair,
        side: Side::Defender,
        source: Some((source, shooter, Vec3::new(0., 200., 11.))),
    });
}

#[test]
fn large_salvos_are_bounded_without_losing_recorded_damage() {
    let mut app = app();
    let shooter = Unit::Ship(Ship::Battleship);
    let target = Unit::Ship(Ship::Cruiser);
    let source = unit(&mut app, shooter, Side::Attacker, Vec3::Y * 200., 100, 0);
    let defender = unit(&mut app, target, Side::Defender, Vec3::ZERO, 20_000, 20_000);
    for _ in 0..10_000 {
        fire(
            &mut app,
            source,
            shooter,
            target,
            ShotReport {
                hull_damage: 1,
                shield_damage: 1,
                ..default()
            },
            false,
        );
    }
    step(&mut app, 0.);
    assert_eq!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count(), 3);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 20_000);
    // Crossing the entire flight in one frame must still apply each total once.
    step(&mut app, 4.);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 10_000);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().shield, 10_000);
    step(&mut app, 4.);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 10_000);
    // Stop the damaged ship's continuing ambient sparks before checking cleanup.
    app.world_mut().get_mut::<CombatUnitCmp>(defender).unwrap().hull = 20_000;
    step(&mut app, 4.);
    assert_eq!(app.world_mut().query::<&Particle>().iter(app.world()).count(), 0);
}

#[test]
fn all_weapon_families_use_the_original_shared_hit_sound() {
    let kinds = [
        Unit::Ship(Ship::LightFighter),
        Unit::Ship(Ship::HeavyFighter),
        Unit::Ship(Ship::Destroyer),
        Unit::Ship(Ship::Cruiser),
        Unit::Ship(Ship::Bomber),
        Unit::Ship(Ship::Battleship),
        Unit::Ship(Ship::Dreadnought),
        Unit::Ship(Ship::WarSun),
        Unit::Defense(Defense::LightLaser),
        Unit::Defense(Defense::HeavyLaser),
        Unit::Defense(Defense::PlasmaTurret),
        Unit::Defense(Defense::IonCannon),
        Unit::Defense(Defense::SpaceDock),
    ];
    for kind in kinds {
        let mut app = app();
        let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
        let target =
            unit(&mut app, Unit::Ship(Ship::Cruiser), Side::Defender, Vec3::ZERO, 100, 100);
        for _ in 0..30 {
            fire(
                &mut app,
                source,
                kind,
                Unit::Ship(Ship::Cruiser),
                ShotReport {
                    shield_damage: 1,
                    ..default()
                },
                false,
            );
        }
        step(&mut app, 0.);
        step(&mut app, 2.);
        let names = app
            .world_mut()
            .resource_mut::<Messages<PlayAudioMsg>>()
            .drain()
            .map(|m| m.name)
            .collect::<Vec<_>>();
        assert_eq!(names, ["short explosion"], "fast-forward must not layer every hit");
        assert_eq!(app.world().get::<CombatUnitCmp>(target).unwrap().shield, 70);
        assert_eq!(app.world().get::<CombatUnitCmp>(target).unwrap().hull, 100);
    }
}

#[test]
fn plasma_and_ion_are_beams_and_massive_weapons_have_wider_profiles() {
    let plasma = Weapon::for_unit(Unit::Defense(Defense::PlasmaTurret));
    let ion = Weapon::for_unit(Unit::Defense(Defense::IonCannon));
    let green = plasma.color().to_srgba();
    let blue = ion.color().to_srgba();
    assert!(green.green > green.blue * 2. && green.green > green.red * 2.);
    assert!(blue.blue > blue.green * 2. && blue.blue > blue.red * 2.);
    for heavy in [Weapon::Solar, Weapon::Siege] {
        assert!(heavy.beam_width().unwrap() > plasma.beam_width().unwrap());
        assert!(heavy.charge() > plasma.charge());
    }
}

#[test]
fn every_weapon_ends_at_its_leading_tip_without_overshooting_the_target() {
    let kinds = [
        Unit::Ship(Ship::LightFighter),
        Unit::Ship(Ship::HeavyFighter),
        Unit::Ship(Ship::Destroyer),
        Unit::Ship(Ship::Cruiser),
        Unit::Ship(Ship::Bomber),
        Unit::Ship(Ship::Battleship),
        Unit::Ship(Ship::Dreadnought),
        Unit::Ship(Ship::WarSun),
        Unit::Defense(Defense::LightLaser),
        Unit::Defense(Defense::HeavyLaser),
        Unit::Defense(Defense::GaussCannon),
        Unit::Defense(Defense::PlasmaTurret),
        Unit::Defense(Defense::IonCannon),
        Unit::Defense(Defense::SpaceDock),
    ];
    for kind in kinds {
        for missed in [false, true] {
            let mut app = app();
            let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
            unit(&mut app, kind, Side::Defender, Vec3::ZERO, 100, 0);
            fire(
                &mut app,
                source,
                kind,
                kind,
                ShotReport {
                    missed,
                    ..default()
                },
                false,
            );
            step(&mut app, 0.);
            let weapon = Weapon::for_unit(kind);
            let mut elapsed = 0.;
            for progress in [0.2, 0.55, 0.99] {
                let next = 0.08 + weapon.charge() + weapon.flight() * progress;
                step(&mut app, next - elapsed);
                elapsed = next;
                let (impact, transform) = app
                    .world_mut()
                    .query::<(&PendingImpact, &Transform)>()
                    .single(app.world())
                    .unwrap();
                let tip =
                    transform.translation + transform.rotation * Vec3::X * transform.scale.x * 0.5;
                let expected = impact.position(progress);
                assert!(tip.truncate().distance(expected.truncate()) < 0.001, "{kind:?}");
                assert!(tip.y >= 0., "{kind:?} must stop at the enemy row, including misses");
            }
        }
    }
}

#[test]
fn pause_freezes_projectiles_particles_and_damage_then_speed_resumes_them() {
    let mut app = app();
    let kind = Unit::Ship(Ship::Bomber);
    let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
    let defender = unit(&mut app, kind, Side::Defender, Vec3::ZERO, 100, 0);
    fire(
        &mut app,
        source,
        kind,
        kind,
        ShotReport {
            hull_damage: 20,
            ..default()
        },
        false,
    );
    step(&mut app, 0.);
    step(&mut app, 0.2);
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    let before = app.world_mut().query::<&PendingImpact>().single(app.world()).unwrap().elapsed;
    step(&mut app, 10.);
    assert_eq!(
        app.world_mut().query::<&PendingImpact>().single(app.world()).unwrap().elapsed,
        before
    );
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 100);
    app.world_mut().resource_mut::<Settings>().combat_paused = false;
    app.world_mut().resource_mut::<Settings>().combat_speed = 8.;
    step(&mut app, 0.1);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 80);
}

#[test]
fn misses_show_feedback_without_moving_cards_and_repair_drones_deliver_once() {
    let mut app = app();
    let kind = Unit::Ship(Ship::LightFighter);
    let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
    let defender = unit(&mut app, kind, Side::Defender, Vec3::ZERO, 100, 0);
    fire(
        &mut app,
        source,
        kind,
        kind,
        ShotReport {
            missed: true,
            hull_damage: 99,
            ..default()
        },
        false,
    );
    step(&mut app, 0.);
    step(&mut app, 0.27);
    assert_eq!(app.world().get::<Transform>(defender).unwrap().translation, Vec3::ZERO);
    step(&mut app, 0.3);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 100);
    assert_eq!(app.world().get::<Transform>(defender).unwrap().translation, Vec3::ZERO);
    assert!(app.world_mut().query::<&Text2d>().iter(app.world()).any(|label| label.0 == "MISS"));
    app.world_mut().get_mut::<CombatUnitCmp>(defender).unwrap().hull = 30;
    for _ in 0..10 {
        fire(
            &mut app,
            source,
            Unit::crawler(),
            kind,
            ShotReport {
                hull_damage: 5,
                ..default()
            },
            true,
        );
    }
    step(&mut app, 0.);
    step(&mut app, 2.);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 80);
    step(&mut app, 2.);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 80);
    assert!(app.world().get::<Transform>(defender).unwrap().translation.length() < 0.001);
}

#[test]
fn destroyed_target_during_flight_is_safe_and_wrecks_finish_at_low_frame_rates() {
    let mut app = app();
    let kind = Unit::Ship(Ship::WarSun);
    let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
    let target = unit(&mut app, kind, Side::Defender, Vec3::ZERO, 100, 0);
    fire(
        &mut app,
        source,
        kind,
        kind,
        ShotReport {
            hull_damage: 100,
            ..default()
        },
        false,
    );
    step(&mut app, 0.);
    app.world_mut().despawn(target);
    app.world_mut().entity_mut(source).insert(Wreck::new(Vec3::Y * 200., 100., kind));
    step(&mut app, 4.);
    assert!(app.world().get_entity(source).is_err());
    assert_eq!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count(), 0);
    step(&mut app, 4.);
    assert_eq!(app.world_mut().query::<&Particle>().iter(app.world()).count(), 0);
}

#[test]
fn planet_destruction_effect_requires_a_recorded_planet_kill() {
    for destroys in [false, true] {
        let mut app = app();
        app.world_mut().spawn((
            Cinematic::new(Vec3::Y * 200., Vec3::ZERO, Vec2::new(900., 600.), 100., destroys),
            CombatCmp,
        ));
        let mut cursor = app.world().resource::<Messages<PlayAudioMsg>>().get_cursor();
        step(&mut app, 4.);
        // Only the recorded destruction requests a large explosion sound.
        assert_eq!(
            cursor.read(app.world().resource::<Messages<PlayAudioMsg>>()).count() > 0,
            destroys
        );
        step(&mut app, DEATH_RAY_DURATION + 0.1);
        assert_eq!(app.world_mut().query::<&Particle>().iter(app.world()).count(), 0);
    }
}

#[test]
fn exiting_during_camera_jolt_restores_exact_position() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = app();
    let origin = Vec3::new(21., -63., 1000.);
    let camera = app
        .world_mut()
        .spawn((
            MainCamera,
            Transform::from_translation(origin),
            Projection::Orthographic(OrthographicProjection::default_2d()),
        ))
        .id();
    let mut ray = Cinematic::new(Vec3::Y * 200., origin, Vec2::splat(900.), 100., true);
    assert_eq!(ray.target.z, ray.origin.z, "camera depth must not stretch the beam");
    ray.elapsed = 3.8;
    app.world_mut().spawn(ray);
    app.world_mut().run_system_once(shake_combat_camera).unwrap();
    assert_ne!(app.world().get::<Transform>(camera).unwrap().translation, origin);
    app.world_mut().run_system_once(restore_combat_camera).unwrap();
    assert!(app.world().get::<Transform>(camera).unwrap().translation.abs_diff_eq(origin, 0.0001));
    assert!(app.world().get::<CombatCameraMotion>(camera).is_none());
}

/// Explicit GPU visual review, kept out of headless CI. Writes to ignored build output.
#[test]
#[ignore = "renders combat review frames with a local GPU"]
#[cfg(target_os = "windows")]
fn render_combat_effects_preview() {
    use bevy::camera::RenderTarget;
    use bevy::render::{
        render_resource::TextureUsages,
        view::screenshot::{save_to_disk, Screenshot},
        RenderPlugin,
    };
    use bevy::time::TimeUpdateStrategy;
    use bevy::window::ExitCondition;
    use bevy::winit::WinitPlugin;

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
    .init_resource::<Settings>()
    .add_message::<SpawnShotMsg>()
    .add_message::<PlayAudioMsg>()
    .insert_resource(ClearColor(Color::srgb(0.015, 0.025, 0.055)))
    .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(
        1. / 60.,
    )))
    .add_systems(Update, run_combat_animations);
    app.finish();
    app.cleanup();
    let mut render_image = Image::new_uninit(
        Extent3d {
            width: 1280,
            height: 800,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    render_image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(render_image);
    app.world_mut().spawn((Camera2d, RenderTarget::Image(target.clone().into())));
    let load = |app: &mut App, path: &str| {
        let data = image::open(path).unwrap();
        let mut image = Image::from_dynamic(data, true, RenderAssetUsages::default());
        image.sampler = bevy::image::ImageSampler::linear();
        app.world_mut().resource_mut::<Assets<Image>>().add(image)
    };
    app.init_asset::<bevy_kira_audio::AudioSource>().init_resource::<WorldAssets>();
    use bevy::ecs::system::RunSystemOnce;
    app.world_mut()
        .run_system_once(
            |mut art: ResMut<WorldAssets>,
             server: Res<AssetServer>,
             mut layouts: ResMut<Assets<TextureAtlasLayout>>| {
                art.begin_gameplay_loading(&server, &mut layouts);
            },
        )
        .unwrap();
    let atlas = app.world().resource::<WorldAssets>().texture("explosion").image;
    let pixels = image::open("assets/images/animations/explosion.png").unwrap();
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .insert(atlas.id(), Image::from_dynamic(pixels, true, RenderAssetUsages::default()))
        .unwrap();
    let destroyed = load(&mut app, "assets/images/planets/destroyed bg.png");
    app.world_mut().resource_mut::<WorldAssets>().images.insert("destroyed bg".into(), destroyed);
    let backdrop = load(&mut app, "assets/images/planets/blue large.png");
    app.world_mut().spawn((
        Sprite {
            image: backdrop,
            custom_size: Some(Vec2::new(1280., 800.)),
            ..default()
        },
        Transform::from_xyz(0., 0., 10.),
        BackgroundImageCmp,
    ));
    let kinds = [
        (Unit::Ship(Ship::Battleship), "ships/battleship", "BATTLESHIP"),
        (Unit::Ship(Ship::HeavyFighter), "ships/heavy fighter", "HEAVY FTR"),
        (Unit::Defense(Defense::LightLaser), "defense/light laser", "LIGHT LASER"),
        (Unit::Defense(Defense::HeavyLaser), "defense/heavy laser", "HEAVY LASER"),
        (Unit::Defense(Defense::PlasmaTurret), "defense/plasma turret", "PLASMA"),
        (Unit::Defense(Defense::IonCannon), "defense/ion cannon", "ION"),
        (Unit::Ship(Ship::WarSun), "ships/war sun", "WAR SUN"),
        (Unit::Defense(Defense::SpaceDock), "defense/space dock", "SPACE DOCK"),
        (Unit::Defense(Defense::GaussCannon), "defense/gauss cannon", "GAUSS"),
    ];
    let mut sources = Vec::new();
    let mut defenders = Vec::new();
    for (index, (kind, path, label)) in kinds.into_iter().enumerate() {
        let x = (index as f32 - 4.) * 140.;
        let art = load(&mut app, &format!("assets/images/{path}.png"));
        let source = unit(&mut app, kind, Side::Attacker, Vec3::new(x, 240., 11.), 1000, 500);
        app.world_mut().get_mut::<Sprite>(source).unwrap().image = art.clone();
        let defender = unit(&mut app, kind, Side::Defender, Vec3::new(x, -190., 11.), 1000, 500);
        app.world_mut().get_mut::<Sprite>(defender).unwrap().image = art;
        sources.push((source, kind, Vec3::new(x, 240., 11.)));
        defenders.push(defender);
        app.world_mut().spawn((
            Text2d::new(label),
            TextFont {
                font_size: 18.0.into(),
                ..default()
            },
            TextColor(Color::srgb(0.7, 0.8, 0.9)),
            Transform::from_xyz(x, 320., 13.),
            CombatCmp,
        ));
    }
    // Warm render pipelines before sampling the effects clock.
    for _ in 0..20 {
        app.update();
    }
    for source in &sources {
        for i in 0..6 {
            app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
                source: Some(*source),
                side: Side::Defender,
                repair: false,
                shot: ShotReport {
                    unit: Some(source.1),
                    hull_damage: 30,
                    shield_damage: 85,
                    missed: i == 5,
                    ..default()
                },
            });
        }
    }
    std::fs::create_dir_all("target/combat-preview").unwrap();
    for frame in 0..190 {
        if frame == 90 {
            app.world_mut().entity_mut(defenders[4]).insert(Wreck::new(
                Vec3::new(0., -190., 11.),
                100.,
                Unit::Defense(Defense::PlasmaTurret),
            ));
            let source = sources[1];
            app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
                source: Some(source),
                side: Side::Defender,
                repair: true,
                shot: ShotReport {
                    unit: Some(Unit::Ship(Ship::HeavyFighter)),
                    hull_damage: 50,
                    ..default()
                },
            });
        }
        if [20, 42, 60, 82, 110, 175].contains(&frame) {
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(save_to_disk(format!("target/combat-preview/frame-{frame}.png")));
        }
        app.update();
    }
    let clear_scene = |app: &mut App| {
        let entities = app
            .world_mut()
            .query_filtered::<Entity, With<CombatCmp>>()
            .iter(app.world())
            .collect::<Vec<_>>();
        for entity in entities {
            if app.world().get_entity(entity).is_ok() {
                app.world_mut().despawn(entity);
            }
        }
    };
    clear_scene(&mut app);
    let bomber_kind = Unit::Ship(Ship::Bomber);
    let bomber_pos = Vec3::new(-200., 240., 11.);
    let bomber = unit(&mut app, bomber_kind, Side::Attacker, bomber_pos, 1000, 0);
    let art = load(&mut app, "assets/images/ships/bomber.png");
    app.world_mut().get_mut::<Sprite>(bomber).unwrap().image = art;
    for (index, building) in Unit::resource_buildings().into_iter().enumerate() {
        let position = Vec3::new((index as f32 - 1.) * 260., -160., 11.);
        let target_unit = unit(&mut app, building, Side::Defender, position, 5, 0);
        let art =
            load(&mut app, &format!("assets/images/buildings/{}.png", building.to_lowername()));
        app.world_mut().get_mut::<Sprite>(target_unit).unwrap().image = art;
        for _ in 0..if index == 0 {
            2
        } else {
            1
        } {
            app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
                source: Some((bomber, bomber_kind, bomber_pos)),
                side: Side::Defender,
                repair: false,
                shot: ShotReport {
                    unit: Some(building),
                    killed: index != 1,
                    missed: index == 1,
                    ..default()
                },
            });
        }
    }
    for frame in 0..150 {
        if [30, 50, 75, 100].contains(&frame) {
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(save_to_disk(format!("target/combat-preview/bombing-{frame}.png")));
        }
        app.update();
    }
    clear_scene(&mut app);
    let sun = unit(&mut app, Unit::war_sun(), Side::Attacker, Vec3::new(300., 240., 11.), 1000, 0);
    let art = load(&mut app, "assets/images/ships/war sun.png");
    app.world_mut().get_mut::<Sprite>(sun).unwrap().image = art;
    app.world_mut().spawn((
        Cinematic::new(Vec3::new(300., 240., 11.), Vec3::ZERO, Vec2::new(1280., 800.), 100., true),
        CombatCmp,
    ));
    for frame in 0..390 {
        if [40, 95, 150, 190, 238, 330].contains(&frame) {
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(save_to_disk(format!("target/combat-preview/death-ray-{frame}.png")));
        }
        app.update();
    }
    for file in ["frame-20.png", "frame-82.png", "death-ray-95.png", "death-ray-238.png"] {
        assert!(std::path::Path::new("target/combat-preview").join(file).exists());
    }
}
