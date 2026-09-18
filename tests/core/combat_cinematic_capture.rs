//! Opt-in offscreen GPU review using the actual cinematic painter and playback controls.

use super::*;
use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::random::DeterministicRngState;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Unit};

fn battle() -> crate::core::combat::report::MissionReport {
    let mut rng = DeterministicRngState::from_u64(4567).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
    origin.colonize(1);
    let mut planet = Planet::new_with_rng(1, "Asterion".into(), Vec2::X, false, 1.0, &mut rng);
    planet.colonize(2);
    planet.army = Army::from([
        (Unit::Defense(Defense::RocketLauncher), 6),
        (Unit::Defense(Defense::LightLaser), 5),
        (Unit::Defense(Defense::HeavyLaser), 4),
        (Unit::Defense(Defense::GaussCannon), 3),
        (Unit::Defense(Defense::PlasmaTurret), 2),
        (Unit::Defense(Defense::IonCannon), 3),
        (Unit::repair_truck(), 3),
        (Unit::crawler(), 2),
        (Unit::planetary_shield(), 1),
        (Unit::space_dock(), 1),
        (Unit::Ship(Ship::LightFighter), 8),
        (Unit::Ship(Ship::Destroyer), 3),
        (Unit::Ship(Ship::Battleship), 1),
    ])
    .into();
    let mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &planet,
        Icon::Attack,
        Army::from([
            (Unit::Ship(Ship::LightFighter), 12),
            (Unit::Ship(Ship::HeavyFighter), 6),
            (Unit::Ship(Ship::Destroyer), 3),
            (Unit::Ship(Ship::Cruiser), 3),
            (Unit::Ship(Ship::Bomber), 2),
            (Unit::Ship(Ship::Battleship), 2),
            (Unit::Ship(Ship::Dreadnought), 1),
        ]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    resolve_combat_with_rng(1, &mission, &planet, &mut rng)
}

#[test]
#[ignore = "offscreen GPU review; writes PNGs under target/cinematic-preview"]
fn render_cinematic_preview() {
    use crate::core::basis_texture::{BasisTexturePlugin, BasisTextureSettings};
    use bevy::asset::RenderAssetUsages;
    use bevy::camera::RenderTarget;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    use bevy::render::RenderPlugin;
    use bevy::window::ExitCondition;
    use bevy::winit::WinitPlugin;
    use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, EguiTextureHandle, PrimaryEguiContext};

    #[derive(Resource)]
    struct CaptureImages(Vec<(String, Handle<Image>)>);

    let report = battle();
    let playback = CinematicPlayback::new(&report);
    let mut samples = vec![("entrance".to_string(), 1.2), ("battle".to_string(), 3.5)];
    if let Some(shot) = playback
        .timeline
        .shots
        .iter()
        .find(|shot| shot.outcome.shield_damage > 0 || shot.outcome.planetary_shield_damage > 0)
    {
        samples.push(("shield".into(), shot.impact_at + 0.05));
    }
    if let Some(repair) = playback.timeline.repairs.first() {
        samples.push(("repair".into(), (repair.start_at + repair.end_at) * 0.5));
    }
    if let Some(death) =
        playback.timeline.actors.iter().filter_map(|actor| actor.death_at).min_by(f32::total_cmp)
    {
        samples.push(("explosion".into(), death + 0.2));
    }
    samples.push(("result".into(), playback.timeline.duration));
    let mut player = Player::new(1, 0);
    let report_id = report.id;
    player.reports.push(report);
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(AssetPlugin {
                file_path: format!("{}/assets-runtime", env!("CARGO_MANIFEST_DIR")),
                meta_check: bevy::asset::AssetMetaCheck::Never,
                ..default()
            })
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
    .add_plugins((EguiPlugin::default(), BasisTexturePlugin))
    .insert_resource(player)
    .insert_resource(playback)
    .insert_resource(UiState {
        in_combat: Some(report_id),
        combat_view: CombatView::Cinematic,
        ..default()
    })
    .insert_resource(Settings {
        combat_paused: true,
        ..default()
    })
    .init_resource::<ImageIds>()
    .init_resource::<NextState<GameState>>()
    .add_systems(EguiPrimaryContextPass, draw_cinematic);
    app.finish();
    app.cleanup();
    let mut handles = Vec::new();
    for directory in [
        "ships",
        "defense",
        "orbitals",
        "planets",
        "bg",
        "ambient",
        "fauna-map",
        "animations",
        "cinematic",
    ] {
        let path = format!("assets/images/{directory}");
        let Ok(entries) = std::fs::read_dir(path) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "png") {
                continue;
            }
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let handle = app
                .world()
                .resource::<AssetServer>()
                .load_builder()
                .with_settings(|settings: &mut BasisTextureSettings| {
                    settings.premultiply_alpha = true;
                    settings.linear_filtering = true;
                })
                .load(format!("images/{directory}/{name}.basisu.ktx2"));
            handles.push((name, handle));
        }
    }
    app.insert_resource(CaptureImages(handles));
    let make_target = |app: &mut App, width, height| {
        let mut image = Image::new_uninit(
            Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            TextureDimension::D2,
            TextureFormat::Rgba8UnormSrgb,
            RenderAssetUsages::default(),
        );
        image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
        app.world_mut().resource_mut::<Assets<Image>>().add(image)
    };
    let target = make_target(&mut app, 1440, 900);
    let camera = app
        .world_mut()
        .spawn((Camera2d, PrimaryEguiContext, RenderTarget::Image(target.clone().into())))
        .id();
    for _ in 0..20 {
        app.update();
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(45);
    loop {
        let server = app.world().resource::<AssetServer>();
        let missing: Vec<_> = app
            .world()
            .resource::<CaptureImages>()
            .0
            .iter()
            .filter(|(_, handle)| !server.is_loaded_with_dependencies(handle.id()))
            .map(|(name, _)| name.clone())
            .collect();
        if missing.is_empty() {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "Run just assets before GPU review; missing {missing:?}"
        );
        app.update();
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
    app.world_mut()
        .run_system_once(
            |mut contexts: EguiContexts,
             mut images: ResMut<ImageIds>,
             handles: Res<CaptureImages>,
             textures: Res<Assets<Image>>,
             mut playback: ResMut<CinematicPlayback>| {
                for (name, handle) in &handles.0 {
                    images.0.insert(
                        name.clone(),
                        contexts.add_image(EguiTextureHandle::Strong(handle.clone())),
                    );
                    let texture = textures.get(handle).unwrap();
                    playback.set_sprite_size(name, texture.width(), texture.height());
                }
            },
        )
        .unwrap();
    std::fs::create_dir_all("target/cinematic-preview").unwrap();
    for (name, time) in samples {
        app.world_mut().resource_mut::<CinematicPlayback>().elapsed = time;
        for _ in 0..8 {
            app.update();
        }
        app.world_mut()
            .spawn(Screenshot::image(target.clone()))
            .observe(save_to_disk(format!("target/cinematic-preview/{name}.png")));
        for _ in 0..8 {
            app.update();
        }
    }
    let small = make_target(&mut app, 640, 480);
    app.world_mut().entity_mut(camera).insert(RenderTarget::Image(small.clone().into()));
    app.world_mut().resource_mut::<CinematicPlayback>().elapsed = 3.5;
    for _ in 0..8 {
        app.update();
    }
    app.world_mut()
        .spawn(Screenshot::image(small))
        .observe(save_to_disk("target/cinematic-preview/small.png"));
    for _ in 0..12 {
        app.update();
    }
}
