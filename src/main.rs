#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use bevy::asset::AssetMetaCheck;
use bevy::prelude::*;
use bevy::sprite::{SpritePickingMode, SpritePickingSettings};
use bevy::window::{WindowMode, WindowResolution};
use bevy_egui::EguiPlugin;
use bevy_kira_audio::AudioPlugin;
use bevy_tweening::TweeningPlugin;

#[cfg(not(target_arch = "wasm32"))]
use std::fs::{File, OpenOptions};
#[cfg(not(target_arch = "wasm32"))]
use std::io::Write;
#[cfg(not(target_arch = "wasm32"))]
use std::panic;
#[cfg(not(target_arch = "wasm32"))]
use std::sync::Mutex;

#[cfg(target_os = "windows")]
use bevy::ecs::system::NonSendMarker;
#[cfg(target_os = "windows")]
use bevy::winit::WINIT_WINDOWS;
#[cfg(target_os = "windows")]
use winit::window::Icon;

use stellarion::core::constants::{HEIGHT, WIDTH};
use stellarion::core::messages::MessagesPlugin;
use stellarion::core::GamePlugin;
use stellarion::TITLE;

#[cfg(not(target_arch = "wasm32"))]
static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);

/// Builds and runs the same application graph on desktop and in the browser.
fn main() {
    #[cfg(not(target_arch = "wasm32"))]
    init_panic_logger();

    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest())
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: TITLE.into(),
                    mode: WindowMode::Windowed,
                    position: initial_window_position(),
                    resolution: WindowResolution::new(WIDTH as u32, HEIGHT as u32),

                    // Reuse the packaged canvas instead of creating a second, undersized one.
                    canvas: Some("#bevy".to_string()),

                    // Tells Wasm to resize the window according to the available canvas
                    fit_canvas_to_parent: true,

                    // Don't override browser's default behavior (ctrl+5, etc...)
                    prevent_default_event_handling: true,

                    ..default()
                }),
                ..default()
            })
            // Disable loading of asset meta since that fails on itch.io
            .set(AssetPlugin {
                file_path: "assets-runtime".to_string(),
                meta_check: AssetMetaCheck::Never,
                ..default()
            }),
    )
    .add_plugins((EguiPlugin::default(), MessagesPlugin, AudioPlugin, TweeningPlugin))
    .add_plugins(GamePlugin)
    .insert_resource(SpritePickingSettings {
        picking_mode: SpritePickingMode::BoundingBox,
        ..default()
    });

    #[cfg(target_os = "windows")]
    app.add_systems(Startup, set_window_icon);

    app.run();
}

/// Browsers own canvas placement, while native windows should open on the primary display.
#[cfg(target_arch = "wasm32")]
fn initial_window_position() -> WindowPosition {
    WindowPosition::Automatic
}

/// Centers the standalone build without relying on an unavailable "current" monitor.
#[cfg(not(target_arch = "wasm32"))]
fn initial_window_position() -> WindowPosition {
    WindowPosition::Centered(MonitorSelection::Primary)
}

#[cfg(not(target_arch = "wasm32"))]
/// Appends native panic details without touching browser-incompatible filesystem APIs.
fn init_panic_logger() {
    panic::set_hook(Box::new(|info| {
        let Ok(mut guard) = LOG_FILE.lock() else {
            eprintln!("Stellarion panic logger lock was poisoned: {info}");
            return;
        };

        if guard.is_none() {
            *guard = OpenOptions::new().create(true).append(true).open("stellarion-logs.txt").ok();
        }

        if let Some(file) = guard.as_mut() {
            let _ = writeln!(file, "=== PANIC ===");
            let _ = writeln!(file, "{}", info);
            let _ = writeln!(file);
        }
    }));
}

#[cfg(target_os = "windows")]
/// Applies the packaged application icon to every native Windows window.
fn set_window_icon(_: NonSendMarker) {
    let Ok(image) = image::open("assets-runtime/images/icons/planet.png") else {
        warn!("could not load the packaged Windows icon");
        return;
    };
    let image = image.into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    let Ok(icon) = Icon::from_rgba(rgba, width, height) else {
        warn!("packaged Windows icon dimensions or pixels are invalid");
        return;
    };

    WINIT_WINDOWS.with_borrow(|windows| {
        for window in windows.windows.values() {
            window.set_window_icon(Some(icon.clone()));
        }
    });
}
