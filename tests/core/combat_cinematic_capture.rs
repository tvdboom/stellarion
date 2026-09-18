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
use crate::core::units::{Amount, Army, Unit};
use crate::multiplayer::client::MultiplayerSession;
use crate::multiplayer::model::{GameMembership, GameRecord};
use crate::utils::NameFromEnum;
use strum::IntoEnumIterator;

fn preview_session() -> MultiplayerSession {
    let model = GameModel::new(
        [45; 32],
        GameRules {
            player_count: 4,
            ..default()
        },
    )
    .unwrap();
    let game_id = GameId::new("cinematic-preview");
    let members = (1..=4)
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
        max_players: 4,
        status: model.status,
        persisted: PersistedGame::new(model),
        members,
        submitted_players: vec![],
    });
    session
}

fn battle(war_sun: bool) -> crate::core::combat::report::MissionReport {
    if !war_sun {
        return battle_with_seed(false, 4567);
    }
    (0..64)
        .map(|seed| battle_with_seed(true, seed))
        .find(|report| report.planet_destroyed)
        .expect("capture requires a successful recorded destruction")
}

fn battle_with_seed(war_sun: bool, seed: u64) -> crate::core::combat::report::MissionReport {
    let mut rng = DeterministicRngState::from_u64(seed).next_rng();
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

fn bombing_battle(raid: BombingRaid) -> crate::core::combat::report::MissionReport {
    let buildings = match raid {
        BombingRaid::Economic => Unit::resource_buildings(),
        BombingRaid::Industrial => Unit::industrial_buildings(),
        BombingRaid::None => unreachable!("capture requires a bombing category"),
    };
    // Use the real resolver, retaining a seed only when every building has a recorded hit.
    // The mixed initial levels exercise both complete demolition and surviving structures.
    for seed in 0..64 {
        let mut rng = DeterministicRngState::from_u64(seed).next_rng();
        let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
        origin.colonize(1);
        let mut planet = Planet::new_with_rng(1, "Asterion".into(), Vec2::X, false, 1.0, &mut rng);
        planet.colonize(2);
        let mut defenders: Army = Unit::resource_buildings()
            .into_iter()
            .chain(Unit::industrial_buildings())
            .zip([1, 3, 5, 1, 3, 5])
            .collect();
        defenders.insert(Unit::Defense(Defense::RocketLauncher), 4);
        defenders.insert(Unit::planetary_shield(), 1);
        planet.army = defenders.into();
        let mission = Mission::new_with_id(
            2,
            1,
            1,
            &origin,
            &planet,
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::Bomber), 36)]),
            raid.clone(),
            false,
            false,
            None,
        );
        let report = resolve_combat_with_rng(2, &mission, &planet, &mut rng);
        if buildings
            .iter()
            .all(|unit| report.surviving_defender.amount(unit) < report.planet.army.amount(unit))
        {
            return report;
        }
    }
    panic!("capture fixture must exercise hits on all three buildings");
}

fn stalemate_battle() -> crate::core::combat::report::MissionReport {
    let mut rng = DeterministicRngState::from_u64(9).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
    origin.colonize(1);
    let mut planet = Planet::new_with_rng(1, "Stalemate".into(), Vec2::X, false, 1.0, &mut rng);
    planet.colonize(2);
    // Neither probe deals damage, so the real resolver reaches its bounded draw outcome.
    planet.army = Army::from([(Unit::probe(), 1)]).into();
    let mission = Mission::new_with_id(
        4,
        1,
        1,
        &origin,
        &planet,
        Icon::Attack,
        Army::from([(Unit::probe(), 1)]),
        BombingRaid::None,
        true,
        false,
        None,
    );
    let report = resolve_combat_with_rng(4, &mission, &planet, &mut rng);
    assert!(report.is_stalemate());
    report
}

#[test]
#[ignore = "offscreen GPU review; writes PNGs under target/cinematic-preview"]
fn render_cinematic_preview() {
    use crate::core::assets::WorldAssets;
    use crate::core::audio::{draw_audio_controls, ChangeAudioMsg, VolumeFeedbackMsg};
    use crate::core::basis_texture::{BasisTexturePlugin, BasisTextureSettings};
    use crate::core::states::AppState;
    use bevy::asset::RenderAssetUsages;
    use bevy::camera::RenderTarget;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
    use bevy::render::view::screenshot::{save_to_disk, Screenshot};
    use bevy::render::RenderPlugin;
    use bevy::time::TimeUpdateStrategy;
    use bevy::window::ExitCondition;
    use bevy::winit::WinitPlugin;
    use bevy_egui::input::EguiInputEvent;
    use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, EguiTextureHandle, PrimaryEguiContext};
    use bevy_tweening::TweeningPlugin;

    #[derive(Resource)]
    struct CaptureImages(Vec<(String, Handle<Image>)>);

    let report = battle(false);
    let playback = CinematicPlayback::new(&report);
    let mut samples = vec![
        ("entrance".to_string(), 3.0),
        ("orbital-entrance".to_string(), playback.timeline.entrance_duration * 0.3),
        ("battle".to_string(), playback.timeline.entrance_duration + 1.1),
        ("maneuver-a".to_string(), playback.timeline.entrance_duration + 3.0),
        ("maneuver-b".to_string(), playback.timeline.entrance_duration + 7.0),
    ];
    // Sample actual simultaneous counterfire, while the dock still guards the planet.
    // An arbitrary battle time can miss the short turret volleys and hide their return fire.
    let dock = playback
        .timeline
        .actors
        .iter()
        .position(|actor| actor.unit == Unit::space_dock())
        .expect("surface-combat capture requires a Space Dock");
    assert!(playback.actor_visible(dock), "The combat dock must appear above the planet");
    let turret_shots: Vec<_> = playback
        .timeline
        .shots
        .iter()
        .filter(|shot| {
            let source = &playback.timeline.actors[shot.source];
            source.side == Side::Defender
                && source.unit.is_turret()
                && playback.actor_visible(shot.source)
                && shot.target.is_some_and(|target| {
                    let actor = &playback.timeline.actors[target];
                    actor.side == Side::Attacker
                        && actor.unit.is_ship()
                        && playback.actor_visible(target)
                })
        })
        .collect();
    let counterfire = turret_shots
        .iter()
        .map(|shot| shot.launch_at + (shot.impact_at - shot.launch_at) * 0.65)
        .filter(|time| playback.timeline.actors[dock].death_at.is_none_or(|death| *time < death))
        .max_by_key(|time| {
            turret_shots
                .iter()
                .filter(|shot| shot.launch_at < *time && *time < shot.impact_at)
                .count()
        })
        .expect("surface-combat capture requires recorded turret fire while the dock survives");
    samples.push(("turret-counterfire".into(), counterfire));
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
        samples.push(("repair-approach".into(), (repair.start_at - 0.75).max(0.0)));
        samples.push(("repair".into(), (repair.start_at + repair.end_at) * 0.5));
        samples.push(("repair-parked".into(), repair.end_at + 0.3));
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
    .add_plugins((EguiPlugin::default(), BasisTexturePlugin, TweeningPlugin))
    .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f64(
        1.0 / 60.0,
    )))
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
    .init_asset::<bevy_kira_audio::AudioSource>()
    .init_resource::<WorldAssets>()
    .init_resource::<NextState<GameState>>()
    .add_message::<ChangeAudioMsg>()
    .add_message::<VolumeFeedbackMsg>()
    .add_systems(
        EguiPrimaryContextPass,
        (
            crate::core::ui::systems::set_ui_style,
            draw_cinematic.run_if(in_state(GameState::Combat)),
            draw_audio_controls,
        )
            .chain(),
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
        "resources",
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
            // Exercise the production result loader, including its separate alpha conventions.
            if (directory == "bg" && matches!(name.as_str(), "victory" | "defeat" | "draw"))
                || (directory == "animations" && name == "explosion")
                || (directory == "planets" && matches!(name.as_str(), "planet0" | "moon0"))
            {
                continue;
            }
            if (directory == "icons" && name != "planetary shield marker")
                || (directory == "ui" && name != "long button")
                || (directory == "resources"
                    && !matches!(name.as_str(), "combat schematic" | "combat cinematic"))
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
    handles.extend(
        app.world_mut()
            .run_system_once(|server: Res<AssetServer>, mut assets: ResMut<WorldAssets>| {
                assets.load_combat_result_images(&server);
                assets.load_combat_shared_images(&server);
                assert_ne!(assets.ui_images["explosion"].id(), assets.image("explosion").id());
                ["victory", "defeat", "draw", "explosion", "planet0", "moon0"]
                    .into_iter()
                    .flat_map(|name| {
                        [
                            (name.to_string(), assets.ui_images[name].clone()),
                            (format!("schematic {name}"), assets.image(name)),
                        ]
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap(),
    );
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
        // The movie stays at its sampled time while the shared result artwork finishes
        // its 1.5-second entrance on the deterministic 60 Hz UI clock.
        let settling_frames = if name == "result" {
            100
        } else {
            8
        };
        for _ in 0..settling_frames {
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
    let approach_finished =
        app.world().resource::<CinematicPlayback>().timeline.entrance_duration + 1.1;
    app.world_mut().resource_mut::<CinematicPlayback>().elapsed = approach_finished;
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
    // Navigate the frozen battle through real egui wheel and drag events. These captures
    // also verify that the fixed HUD and pause marker stay outside the camera transform.
    let was_paused = app.world().resource::<Settings>().combat_paused;
    let volume = app.world().resource::<Settings>().volume;
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    let viewport = app
        .world_mut()
        .run_system_once(|mut contexts: EguiContexts| contexts.ctx_mut().unwrap().content_rect())
        .unwrap();
    let pointer = viewport.min + viewport.size() * egui::vec2(0.70, 0.57);
    let input = |app: &mut App, event| {
        app.world_mut().write_message(EguiInputEvent {
            context: camera,
            event,
        });
        app.update();
    };
    for _ in 0..3 {
        input(&mut app, egui::Event::PointerMoved(pointer));
    }
    input(
        &mut app,
        egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Line,
            delta: egui::vec2(0.0, 6.0),
            phase: egui::TouchPhase::Move,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let zoomed = app.world().resource::<CinematicPlayback>().camera.transform(viewport);
    assert!(zoomed.scaling > 1.5, "the live cinematic wheel path must zoom the camera");
    for _ in 0..8 {
        app.update();
    }
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk("target/cinematic-preview/camera-zoomed.png"));
    for _ in 0..8 {
        app.update();
    }
    // Close-up counterfire review catches detached joints and large-gun proportions.
    app.world_mut().resource_mut::<Settings>().combat_paused = false;
    for (name, time) in
        [("mounts-closeup", counterfire), ("mounts-closeup-later", counterfire + 0.3)]
    {
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
    app.world_mut().resource_mut::<CinematicPlayback>().elapsed = approach_finished;
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    input(
        &mut app,
        egui::Event::PointerButton {
            pos: pointer,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let drag = egui::vec2(220.0, -90.0);
    for fraction in [0.25, 0.5, 0.75, 1.0] {
        input(&mut app, egui::Event::PointerMoved(pointer + drag * fraction));
    }
    input(
        &mut app,
        egui::Event::PointerButton {
            pos: pointer + drag,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: egui::Modifiers::NONE,
        },
    );
    let panned = app.world().resource::<CinematicPlayback>().camera.transform(viewport);
    assert_eq!(panned.scaling, zoomed.scaling);
    assert!((panned.translation - zoomed.translation).length() > 100.0);
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, approach_finished);
    assert_eq!(app.world().resource::<Settings>().volume, volume);
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk("target/cinematic-preview/camera-panned.png"));
    for _ in 0..8 {
        app.update();
    }
    app.world_mut().resource_mut::<CinematicPlayback>().camera = Default::default();
    app.world_mut().resource_mut::<Settings>().combat_paused = was_paused;
    input(&mut app, egui::Event::PointerGone);
    // The battle-selection gear uses the same controls, with image tiles in its hover panel.
    // Keep the fixture independent of map setup; only the menu controls paint this sample.
    app.insert_resource(State::new(GameState::CombatMenu));
    app.world_mut().write_message(EguiInputEvent {
        context: camera,
        event: egui::Event::PointerMoved(gear),
    });
    for _ in 0..8 {
        app.update();
    }
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk("target/cinematic-preview/combat-view-settings.png"));
    for _ in 0..8 {
        app.update();
    }
    app.insert_resource(State::new(GameState::Combat));
    app.world_mut().write_message(EguiInputEvent {
        context: camera,
        event: egui::Event::PointerGone,
    });
    // Four owners on two sides expose accidental attacker/defender-only coloring.
    let mut allied = battle(false);
    allied.id = 100;
    for round in &mut allied.combat_report.as_mut().unwrap().rounds {
        for record in &mut round.attacker {
            record.owner = Some(if record.id % 2 == 0 {
                1
            } else {
                3
            });
        }
        for record in &mut round.defender {
            record.owner = Some(if record.unit.is_ship() {
                4
            } else {
                2
            });
        }
    }
    let first = &allied.combat_report.as_ref().unwrap().rounds[0];
    let army = |records: &[crate::core::combat::resolution::CombatUnit], owner| {
        let mut army = Army::new();
        for record in records.iter().filter(|record| record.owner == Some(owner)) {
            *army.entry(record.unit).or_default() += 1;
        }
        army
    };
    allied.mission.joint_attack = Some(crate::core::missions::JointAttackMission {
        attackers: [(1, army(&first.attacker, 1)), (3, army(&first.attacker, 3))].into(),
        ..default()
    });
    allied.planet.army = crate::core::map::planet::Garrison::from_parts(
        army(&first.defender, 2),
        [(4, army(&first.defender, 4))].into(),
    );
    let mut playback = CinematicPlayback::new(&allied);
    playback.elapsed = playback.timeline.entrance_duration + 1.1;
    for (name, handle) in &app.world().resource::<CaptureImages>().0 {
        let texture = app.world().resource::<Assets<Image>>().get(handle).unwrap();
        playback.set_sprite_size(name, texture.width(), texture.height());
    }
    app.world_mut().resource_mut::<UiState>().in_combat = Some(allied.id);
    app.world_mut().resource_mut::<Player>().reports.push(allied);
    app.insert_resource(playback);
    for _ in 0..8 {
        app.update();
    }
    app.world_mut()
        .spawn(Screenshot::image(target.clone()))
        .observe(save_to_disk("target/cinematic-preview/allied-owner-colors.png"));
    for _ in 0..8 {
        app.update();
    }
    // Multiple surviving War Suns must contribute to the same recorded discharge.
    let report = battle(true);
    let decisive_report = report.clone();
    assert!(report.planet_destroyed, "capture requires a successful recorded destruction");
    let mut playback = CinematicPlayback::new(&report);
    assert!(!playback.timeline.planet_attacks.is_empty());
    for (name, handle) in &app.world().resource::<CaptureImages>().0 {
        let texture = app.world().resource::<Assets<Image>>().get(handle).unwrap();
        playback.set_sprite_size(name, texture.width(), texture.height());
    }
    let ray = playback.timeline.planet_attacks.iter().find(|attack| attack.destroyed).unwrap();
    assert!(ray.sources.len() > 1);
    let ray_samples = [
        ("war-sun-charge", ray.start_at + (ray.discharge_at - ray.start_at) * 0.75),
        ("war-sun-beam", (ray.discharge_at + ray.end_at) * 0.5),
        ("planet-before-swap", ray.end_at + 0.66),
        ("planet-after-swap", ray.end_at + 0.70),
        ("planet-blast", ray.end_at + 0.7),
        ("planet-blast-late", ray.end_at + 1.25),
        ("planet-debris", ray.end_at + 2.5),
        ("planet-destroyed", playback.timeline.duration - 0.1),
    ];
    app.world_mut().resource_mut::<UiState>().in_combat = Some(report.id);
    app.world_mut().resource_mut::<Player>().reports = vec![report];
    app.insert_resource(playback);
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

    // Capture the real failed outcome too: its surface should ripple, then settle intact.
    let report = (0..64)
        .map(|seed| battle_with_seed(true, seed))
        .find(|report| {
            !report.planet_destroyed
                && report.combat_report.as_ref().is_some_and(|combat| {
                    combat.rounds.iter().any(|round| round.destroy_probability > 0.0)
                })
        })
        .expect("capture requires a failed recorded planetary discharge");
    let mut playback = CinematicPlayback::new(&report);
    for (name, handle) in &app.world().resource::<CaptureImages>().0 {
        let texture = app.world().resource::<Assets<Image>>().get(handle).unwrap();
        playback.set_sprite_size(name, texture.width(), texture.height());
    }
    let ray = playback.timeline.planet_attacks.last().unwrap();
    assert!(!ray.destroyed);
    let samples =
        [("failed-ray-ripple", ray.end_at + 0.4), ("failed-ray-settled", ray.end_at + 1.4)];
    app.world_mut().resource_mut::<UiState>().in_combat = Some(report.id);
    app.world_mut().resource_mut::<Player>().reports = vec![report];
    app.insert_resource(playback);
    for (name, elapsed) in samples {
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

    // Review all three shared banners using real resolved outcomes. Victory and defeat
    // are the two player perspectives on the same decisive battle, not altered reports.
    for (index, (status, mut report, player_id)) in [
        ("victory", decisive_report.clone(), 1),
        ("defeat", decisive_report, 2),
        ("draw", stalemate_battle(), 1),
    ]
    .into_iter()
    .enumerate()
    {
        report.id = 10 + index as u64;
        let mut player = Player::new(player_id, 0);
        assert_eq!(report.status(&player), status);
        let mut playback = CinematicPlayback::new(&report);
        for (name, handle) in &app.world().resource::<CaptureImages>().0 {
            let texture = app.world().resource::<Assets<Image>>().get(handle).unwrap();
            playback.set_sprite_size(name, texture.width(), texture.height());
        }
        playback.elapsed = playback.timeline.duration;
        app.world_mut().resource_mut::<UiState>().in_combat = Some(report.id);
        player.reports.push(report);
        app.insert_resource(player);
        app.insert_resource(playback);
        for _ in 0..100 {
            app.update();
        }
        app.world_mut()
            .spawn(Screenshot::image(target.clone()))
            .observe(save_to_disk(format!("target/cinematic-preview/result-{status}.png")));
        for _ in 0..8 {
            app.update();
        }
        // Render the production Bevy result hierarchy against the same offscreen camera,
        // checking its centering, clipping, and tween independently of the egui movie HUD.
        app.insert_resource(State::new(GameState::CombatMenu));
        let schematic_banner = app
            .world_mut()
            .run_system_once(move |mut commands: Commands, assets: Res<WorldAssets>| {
                let root = crate::core::combat::systems::spawn_combat_result_banner(
                    &mut commands,
                    &assets,
                    status,
                );
                commands.entity(root).insert(UiTargetCamera(camera));
                root
            })
            .unwrap();
        for _ in 0..100 {
            app.update();
        }
        app.world_mut().spawn(Screenshot::image(target.clone())).observe(save_to_disk(format!(
            "target/cinematic-preview/schematic-result-{status}.png"
        )));
        for _ in 0..8 {
            app.update();
        }
        app.world_mut().despawn(schematic_banner);
        app.insert_resource(State::new(GameState::Combat));
    }

    for (category, raid, kind) in [
        ("economic", BombingRaid::Economic, crate::core::map::planet::PlanetKind::Dry),
        ("industrial", BombingRaid::Industrial, crate::core::map::planet::PlanetKind::Water),
        ("gas-economic", BombingRaid::Economic, crate::core::map::planet::PlanetKind::Gas),
        ("gas-industrial", BombingRaid::Industrial, crate::core::map::planet::PlanetKind::Gas),
    ] {
        let mut report = bombing_battle(raid);
        // The report's original kind selects presentation only; recorded damage stays intact.
        report.planet.kind = kind;
        let mut playback = CinematicPlayback::new(&report);
        for (name, handle) in &app.world().resource::<CaptureImages>().0 {
            let texture = app.world().resource::<Assets<Image>>().get(handle).unwrap();
            playback.set_sprite_size(name, texture.width(), texture.height());
        }
        let losses = &playback.timeline.level_losses;
        assert!(!losses.is_empty());
        let samples = [
            ("buildings", playback.timeline.entrance_duration),
            ("bombing", losses[0].impact_at + 0.24),
            ("damage", losses.last().unwrap().impact_at + 0.65),
        ];
        app.world_mut().resource_mut::<UiState>().in_combat = Some(report.id);
        app.world_mut().resource_mut::<Player>().reports = vec![report];
        app.insert_resource(playback);
        for (phase, elapsed) in samples {
            app.world_mut().resource_mut::<CinematicPlayback>().elapsed = elapsed;
            for _ in 0..8 {
                app.update();
            }
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(save_to_disk(format!("target/cinematic-preview/{category}-{phase}.png")));
            for _ in 0..8 {
                app.update();
            }
        }
    }

    // Inspect all globe types through the production camera, including both stable variants.
    for kind in crate::core::map::planet::PlanetKind::iter() {
        for variant in 1..=2 {
            let mut report = battle(false);
            report.planet.kind = kind;
            report.planet.id = variant;
            let mut playback = CinematicPlayback::new(&report);
            for (name, handle) in &app.world().resource::<CaptureImages>().0 {
                let texture = app.world().resource::<Assets<Image>>().get(handle).unwrap();
                playback.set_sprite_size(name, texture.width(), texture.height());
            }
            playback.elapsed = playback.timeline.entrance_duration + 0.1;
            app.world_mut().resource_mut::<UiState>().in_combat = Some(report.id);
            app.world_mut().resource_mut::<Player>().reports = vec![report];
            app.insert_resource(playback);
            for _ in 0..8 {
                app.update();
            }
            app.world_mut().spawn(Screenshot::image(target.clone())).observe(save_to_disk(
                format!("target/cinematic-preview/planet-{}-{variant}.png", kind.to_lowername()),
            ));
            for _ in 0..8 {
                app.update();
            }
        }
    }

    // Resize only after every full-resolution sample; swapping an old render target back
    // can retain the small egui viewport for a frame and produce an upscaled capture.
    let small = make_target(&mut app, 640, 480);
    app.world_mut().entity_mut(camera).insert(RenderTarget::Image(small.clone().into()));
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
