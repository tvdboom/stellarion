use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;
use bevy_egui::{EguiContext, EguiPrimaryContextPass, EguiUserTextures, PrimaryEguiContext};

use super::*;

use crate::core::audio::PlayAudioMsg;
use crate::core::messages::{MessageMsg, MessagesPlugin};
use crate::core::states::GameState;

#[test]
fn progress_never_invents_completed_work_or_moves_backwards() {
    assert_eq!(advance_progress(0.0, 0.3, 10.0), 0.3);
    assert_eq!(advance_progress(0.7, 0.3, 0.1), 0.7);
    assert!(advance_progress(0.0, 1.0, 0.1) < 1.0);
    assert_eq!(advance_progress(0.95, 1.0, 1.0), 1.0);
}

#[test]
fn portrait_and_ultrawide_windows_crop_without_stretching() {
    for viewport in [Vec2::new(360.0, 800.0), Vec2::new(2560.0, 1080.0)] {
        let source = Vec2::new(1920.0, 900.0);
        let uv = cover_uv(source, viewport);
        assert!(uv.min.x >= 0.0 && uv.min.y >= 0.0);
        assert!(uv.max.x <= 1.0 && uv.max.y <= 1.0);
        assert_eq!(uv.center(), egui::pos2(0.5, 0.5));
        let cropped_aspect = source.x * uv.width() / (source.y * uv.height());
        assert!((cropped_aspect - viewport.x / viewport.y).abs() < 0.0001);
    }
}

#[test]
fn completion_waits_for_a_readable_full_bar_then_reveals() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()));
    app.init_asset::<Image>().init_resource::<LoadingScreen>();
    let mut screen = app.world_mut().resource_mut::<LoadingScreen>();
    screen.boot = false;
    assert!(!screen.finish());
    screen.elapsed = MIN_DISPLAY;
    screen.ready_elapsed = 1.0;
    screen.progress = 0.95;
    assert!(!screen.finish(), "the full bar must be painted before leaving loading");
    screen.progress = 1.0;
    assert!(screen.finish());
    screen.reveal_elapsed = Some(0.0);
    assert!(screen.visible(), "cover the first destination frame");
    screen.reveal_elapsed = Some(REVEAL);
    assert!(!screen.visible(), "release the overlay after the reveal");
}

#[test]
fn startup_completion_does_not_wait_for_a_minimum_display_or_progress_animation() {
    let mut screen = LoadingScreen {
        progress: 0.4,
        ..default()
    };
    assert!(screen.finish(), "a ready menu should begin its reveal immediately");
    assert_eq!(screen.progress, 1.0);
    assert_eq!(screen.elapsed, 0.0);
    assert_eq!(screen.ready_elapsed, 0.0);
}

#[test]
fn startup_uses_only_menu_art_and_loading_copy_through_the_menu_reveal() {
    for size in [egui::vec2(1600.0, 900.0), egui::vec2(320.0, 480.0)] {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, AssetPlugin::default()))
            .init_asset::<Image>()
            .init_asset::<Font>()
            .init_asset::<TextureAtlasLayout>()
            .init_asset::<bevy_kira_audio::AudioSource>()
            .init_resource::<WorldAssets>()
            .init_resource::<LoadingScreen>()
            .init_resource::<MultiplayerSession>()
            .init_resource::<EguiUserTextures>()
            .insert_resource(State::new(AppState::Boot))
            .add_message::<MultiplayerRequest>()
            .add_systems(EguiPrimaryContextPass, draw_loading_screen);
        let menu = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
        app.world_mut().resource_mut::<WorldAssets>().images.insert("menu".into(), menu);
        app.world_mut().run_system_once(begin_loading_screen).unwrap();
        let mut context = EguiContext::default();
        let egui = context.get_mut().clone();
        app.world_mut().spawn((context, PrimaryEguiContext));
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, size);

        for (state, progress, reveal) in [
            (AppState::Boot, 0.42, None),
            (AppState::Boot, 1.0, None),
            (AppState::MainMenu, 1.0, Some(REVEAL * 0.5)),
        ] {
            app.insert_resource(State::new(state));
            let mut screen = app.world_mut().resource_mut::<LoadingScreen>();
            screen.progress = progress;
            screen.ready = progress == 1.0;
            screen.reveal_elapsed = reveal;
            let mut output = egui::FullOutput::default();
            for _ in 0..3 {
                output = egui.run_ui(
                    egui::RawInput {
                        screen_rect: Some(viewport),
                        ..default()
                    },
                    |_| app.world_mut().run_schedule(EguiPrimaryContextPass),
                );
                output.textures_delta.clear();
            }
            let labels = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => {
                        assert!(viewport.contains_rect(text.visual_bounding_rect()));
                        Some(text.galley.job.text.as_str())
                    },
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                labels,
                [
                    "Loading game",
                    if progress == 1.0 {
                        "100%"
                    } else {
                        "42%"
                    }
                ]
            );
            let textures = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Mesh(mesh) => Some(mesh),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(textures.len(), 1, "startup must draw exactly the menu wallpaper");
            assert_eq!(
                Some(textures[0].texture_id),
                app.world().resource::<LoadingScreen>().menu_texture
            );
            assert_eq!(textures[0].calc_bounds(), viewport);
            let opacity = textures[0].vertices[0].color.a();
            if reveal.is_some() {
                assert!((1..255).contains(&opacity), "the menu must be revealed with a fade");
            } else {
                assert_eq!(opacity, 255);
            }
        }
    }
}

#[test]
fn start_game_fades_before_the_backend_handoff_and_keeps_the_same_slide() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<WorldAssets>()
        .init_resource::<MultiplayerSession>()
        .init_resource::<EguiUserTextures>()
        .insert_resource(State::new(AppState::Lobby))
        .insert_resource(LoadingScreen {
            reveal_elapsed: Some(REVEAL),
            ..default()
        })
        .add_message::<MultiplayerRequest>()
        .add_systems(EguiPrimaryContextPass, (preview_game_start, draw_loading_screen).chain());
    let menu = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
    app.world_mut().resource_mut::<WorldAssets>().images.insert("menu".into(), menu);
    let mut context = EguiContext::default();
    let egui = context.get_mut().clone();
    app.world_mut().spawn((context, PrimaryEguiContext));
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(320.0, 480.0));
    let frame = |app: &mut App| {
        let mut output = egui.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..default()
            },
            |_| app.world_mut().run_schedule(EguiPrimaryContextPass),
        );
        output.textures_delta.clear();
        output.shapes
    };

    app.world_mut().write_message(MultiplayerRequest::StartGame);
    frame(&mut app);
    let screen = app.world().resource::<LoadingScreen>();
    assert!(screen.visible() && screen.starting_game);
    assert_eq!(screen.current, None);
    let menu_texture = screen.menu_texture.unwrap();
    let shapes = frame(&mut app);
    assert!(shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Mesh(mesh) if mesh.texture_id == menu_texture)
    }));
    assert!(shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text)
            if text.galley.job.text == "Leave Lobby" && viewport.contains_rect(text.visual_bounding_rect()))
    }));

    let wallpaper = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
    app.world_mut().resource_mut::<LoadingScreen>().images[0] = Some(wallpaper.clone());
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(100));
    app.world_mut().run_system_once(advance_loading_screen).unwrap();
    let shapes = frame(&mut app);
    let screen = app.world().resource::<LoadingScreen>();
    assert_eq!(screen.current, Some(0));
    let match_texture = screen.textures[0].unwrap();
    assert!(shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Mesh(mesh)
            if mesh.texture_id == match_texture && (1..255).contains(&mesh.vertices[0].color.a()))
    }));
    let fade_elapsed = screen.artwork_elapsed;
    assert_eq!(screen.progress, 0.0);

    app.insert_resource(State::new(AppState::LoadingGame));
    app.world_mut().run_system_once(begin_loading_screen).unwrap();
    let screen = app.world().resource::<LoadingScreen>();
    assert!(screen.visible() && !screen.starting_game);
    assert_eq!(screen.current, Some(0));
    assert_eq!(screen.images[0].as_ref(), Some(&wallpaper));
    assert_eq!(screen.artwork_elapsed, fade_elapsed);
}

#[test]
fn failed_start_returns_control_to_the_lobby_and_allows_a_retry() {
    let mut app = App::new();
    app.init_resource::<Assets<Image>>()
        .init_resource::<MultiplayerSession>()
        .init_resource::<LoadingScreen>()
        .insert_resource(State::new(AppState::Lobby))
        .add_message::<MultiplayerRequest>()
        .add_systems(PreUpdate, dismiss_game_start_preview)
        .add_systems(Update, preview_game_start);

    app.world_mut().write_message(MultiplayerRequest::StartGame);
    app.update();
    assert!(app.world().resource::<LoadingScreen>().starting_game);
    app.world_mut().resource_mut::<MultiplayerSession>().menu_error = Some("Start failed".into());
    app.update();
    assert!(!app.world().resource::<LoadingScreen>().visible());

    let wallpaper = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
    app.world_mut().resource_mut::<LoadingScreen>().images[0] = Some(wallpaper);
    app.world_mut().write_message(MultiplayerRequest::StartGame);
    app.update();
    let screen = app.world().resource::<LoadingScreen>();
    assert!(screen.starting_game);
    assert_eq!(screen.current, Some(0));
}

#[test]
fn turn_start_notification_and_sound_wait_for_the_reveal_without_expiring() {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Image>()
        .init_resource::<LoadingScreen>()
        .init_resource::<EguiUserTextures>()
        .add_message::<MessageMsg>()
        .add_message::<PlayAudioMsg>()
        .insert_resource(State::new(AppState::Game))
        .insert_resource(State::new(GameState::Playing))
        .add_plugins(MessagesPlugin);
    let mut context = EguiContext::default();
    let egui = context.get_mut().clone();
    app.world_mut().spawn((context, PrimaryEguiContext));
    app.world_mut().write_message(MessageMsg::info("Turn 1 started."));
    let frame = |app: &mut App| {
        let mut output = egui.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(900.0, 700.0),
                )),
                ..default()
            },
            |_| app.world_mut().run_schedule(EguiPrimaryContextPass),
        );
        output.textures_delta.clear();
        output.shapes
    };
    let contains_turn = |shapes: &[egui::epaint::ClippedShape]| {
        shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Turn 1 started.")
        })
    };
    // Stay hidden longer than the notification's normal lifetime, including the fade.
    app.world_mut().resource_mut::<LoadingScreen>().reveal_elapsed = Some(REVEAL * 0.5);
    for _ in 0..8 {
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(1));
        assert!(!contains_turn(&frame(&mut app)));
        assert!(app.world().resource::<Messages<PlayAudioMsg>>().is_empty());
    }
    app.world_mut().resource_mut::<LoadingScreen>().reveal_elapsed = Some(REVEAL);
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(16));
    for _ in 0..3 {
        frame(&mut app);
    }
    assert!(contains_turn(&frame(&mut app)));
    assert_eq!(app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().count(), 1);
    frame(&mut app);
    assert!(app.world().resource::<Messages<PlayAudioMsg>>().is_empty());
}

#[test]
fn random_slides_exclude_the_previous_image_and_skip_unavailable_images() {
    for previous in 0..BACKGROUNDS.len() {
        for _ in 0..32 {
            let choice = choose_background([true; BACKGROUNDS.len()], Some(previous)).unwrap();
            assert_ne!(choice, previous);
        }
        let mut ready = [false; BACKGROUNDS.len()];
        ready[previous] = true;
        assert_eq!(choose_background(ready, Some(previous)), None);
        let next = (previous + 1) % BACKGROUNDS.len();
        ready[next] = true;
        assert_eq!(choose_background(ready, Some(previous)), Some(next));
    }
    assert_eq!(choose_background([false; BACKGROUNDS.len()], None), None);
}

#[test]
fn loading_art_registry_has_one_packaged_source_per_slide() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let provenance: serde_json::Value =
        serde_json::from_str(include_str!("../../docs/loading-art.json")).unwrap();
    let sources = provenance["images"].as_array().unwrap();
    assert_eq!(sources.len(), BACKGROUNDS.len());
    for name in BACKGROUNDS {
        let path = format!("assets/images/bg/loading/{name}.jpg");
        assert!(root.join(&path).is_file(), "missing loading artwork: {path}");
        assert_eq!(sources.iter().filter(|source| source["file"] == path).count(), 1);
    }
}
