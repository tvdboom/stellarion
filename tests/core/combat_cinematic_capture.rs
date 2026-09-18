//! Opt-in offscreen GPU review using the actual cinematic painter and playback controls.

use super::*;
use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::identity::{GameCode, GameId, UserId};
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::random::DeterministicRngState;
use crate::core::simulation::{GameModel, GameRules, PersistedGame};
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Unit};
use crate::multiplayer::client::MultiplayerSession;
use crate::multiplayer::model::{GameMembership, GameRecord};

fn preview_session() -> MultiplayerSession {
    let model = GameModel::new(
        [45; 32],
        GameRules {
            player_count: 2,
            ..default()
        },
    )
    .unwrap();
    let game_id = GameId::new("cinematic-preview");
    let members = (1..=2)
        .map(|player_id| GameMembership {
            game_id: game_id.clone(),
            player_id,
            user_id: UserId::new(format!("preview-{player_id}")),
            display_name: format!("Practice P{player_id}"),
            is_creator: player_id == 1,
            identity_version: 0,
            connected: true,
        })
        .collect();
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: game_id,
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: 2,
        status: model.status,
        persisted: PersistedGame::new(model),
        members,
        submitted_players: vec![],
    });
    session
}

fn battle(war_sun: bool) -> crate::core::combat::report::MissionReport {
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
    if war_sun {
        // A death ray can only fire after the orbital fleet and Space Dock are gone.
        // Use a surface-only defense so this fixture always records an actual discharge.
        planet.army = Army::from([
            (Unit::Defense(Defense::RocketLauncher), 6),
            (Unit::Defense(Defense::HeavyLaser), 4),
            (Unit::repair_truck(), 2),
            (Unit::planetary_shield(), 1),
        ])
        .into();
    }
    let mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &planet,
        if war_sun {
            Icon::Destroy
        } else {
            Icon::Attack
        },
        if war_sun {
            Army::from([(Unit::war_sun(), 2)])
        } else {
            Army::from([
                (Unit::Ship(Ship::LightFighter), 12),
                (Unit::Ship(Ship::HeavyFighter), 6),
                (Unit::Ship(Ship::Destroyer), 3),
                (Unit::Ship(Ship::Cruiser), 3),
                (Unit::Ship(Ship::Bomber), 2),
                (Unit::Ship(Ship::Battleship), 2),
                (Unit::Ship(Ship::Dreadnought), 1),
            ])
        },
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
    use crate::core::audio::{draw_audio_controls, ChangeAudioMsg, VolumeFeedbackMsg};
    use crate::core::basis_texture::{BasisTexturePlugin, BasisTextureSettings};
    use crate::core::states::AppState;
    use bevy::asset::RenderAssetUsages;
    use bevy::camera::RenderTarget;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    use bevy::render::RenderPlugin;
    use bevy::window::ExitCondition;
    use bevy::winit::WinitPlugin;
    use bevy_egui::input::EguiInputEvent;
    use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, EguiTextureHandle, PrimaryEguiContext};

    #[derive(Resource)]
    struct CaptureImages(Vec<(String, Handle<Image>)>);

    let report = battle(false);
    let playback = CinematicPlayback::new(&report);
    let mut samples = vec![("entrance".to_string(), 1.2), ("battle".to_string(), 3.5)];
    if let Some(shot) = playback
        .timeline
        .shots
        .iter()
        .find(|shot| shot.outcome.shield_damage > 0 || shot.outcome.planetary_shield_damage > 0)
    {
        samples.push(("shield".into(), shot.impact_at + 0.05));
        samples.push(("shield-flow".into(), shot.impact_at + 0.70));
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
    .insert_resource(preview_session())
    .insert_resource(playback)
    .insert_resource(UiState {
        in_combat: Some(report_id),
        combat_view: CombatView::Cinematic,
        ..default()
    })
    // No advance system is installed: fixed sample times stay still while the HUD reflects
    // normal playback rather than covering every captured scene with the pause banner.
    .init_resource::<Settings>()
    .insert_resource(State::new(AppState::Game))
    .insert_resource(State::new(GameState::Combat))
    .init_resource::<ImageIds>()
    .init_resource::<NextState<GameState>>()
    .add_message::<ChangeAudioMsg>()
    .add_message::<VolumeFeedbackMsg>()
    .add_systems(
        EguiPrimaryContextPass,
        (crate::core::ui::systems::set_ui_style, draw_cinematic, draw_audio_controls).chain(),
    );
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
        "icons",
        "ui",
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
            if (directory == "icons" && name != "planetary shield marker")
                || (directory == "ui" && name != "long button")
            {
                continue;
            }
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
                // Render settled HUD states without depending on wall-clock fade-in timing.
                let context = contexts.ctx_mut().unwrap();
                let mut style = (*context.global_style()).clone();
                style.animation_time = 0.0;
                context.set_global_style(style);
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
    // Exercise the same HUD systems and pointer/scroll input path used in a live replay.
    app.world_mut().resource_mut::<CinematicPlayback>().elapsed = 3.5;
    let [gear, sound] = app
        .world_mut()
        .run_system_once(|mut contexts: EguiContexts| {
            let context = contexts.ctx_mut().unwrap();
            let rect = context.content_rect();
            let scale = crate::core::ui::systems::viewport_ui_scale(rect.size());
            let spacing = context.global_style().spacing.item_spacing.x;
            let sound = rect.right_top() + egui::vec2(-36.0, 36.0) * scale;
            [sound - egui::vec2((32.0 + spacing) * scale, 0.0), sound]
        })
        .unwrap();
    for (name, position) in [("settings", gear), ("volume", sound)] {
        app.world_mut().write_message(EguiInputEvent {
            context: camera,
            event: egui::Event::PointerMoved(position),
        });
        if name == "volume" {
            app.world_mut().write_message(EguiInputEvent {
                context: camera,
                event: egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Line,
                    delta: egui::vec2(0.0, -1.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            });
        }
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
    app.world_mut().write_message(EguiInputEvent {
        context: camera,
        event: egui::Event::PointerGone,
    });
    app.world_mut()
        .run_system_once(|mut contexts: EguiContexts| {
            contexts.ctx_mut().unwrap().data_mut(|data| {
                data.remove_temp::<f64>(egui::Id::new("audio volume scroll"));
            });
        })
        .unwrap();
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

    // The War Sun's planetary discharge has its own charge/focus phases. Render a
    // second real resolver report so the shared masks are checked at cinematic scale.
    let report = battle(true);
    let mut playback = CinematicPlayback::new(&report);
    assert!(!playback.timeline.planet_attacks.is_empty());
    for (name, handle) in &app.world().resource::<CaptureImages>().0 {
        let texture = app.world().resource::<Assets<Image>>().get(handle).unwrap();
        playback.set_sprite_size(name, texture.width(), texture.height());
    }
    let ray = &playback.timeline.planet_attacks[0];
    let ray_samples = [
        ("war-sun-charge", ray.start_at + (ray.end_at - ray.start_at) * 0.38),
        ("war-sun-beam", ray.start_at + (ray.end_at - ray.start_at) * 0.68),
    ];
    app.world_mut().resource_mut::<UiState>().in_combat = Some(report.id);
    app.world_mut().resource_mut::<Player>().reports = vec![report];
    app.insert_resource(playback);
    app.world_mut().entity_mut(camera).insert(RenderTarget::Image(target.clone().into()));
    for (name, elapsed) in ray_samples {
        app.world_mut().resource_mut::<CinematicPlayback>().elapsed = elapsed;
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
}
