//#![windows_subsystem = "windows"]

mod core;
mod utils;

use crate::core::constants::{HEIGHT, WIDTH};
use crate::core::GamePlugin;
use bevy::asset::AssetMetaCheck;
use bevy::prelude::NonSend;
use bevy::prelude::*;
use bevy::window::{WindowMode, WindowResolution};
use bevy::winit::WinitWindows;
use bevy_kira_audio::AudioPlugin;
use bevy_renet::netcode::{NetcodeClientPlugin, NetcodeServerPlugin};
use bevy_renet::{RenetClientPlugin, RenetServerPlugin};
use winit::window::Icon;

pub const TITLE: &str = "Stellarion";

fn main() {
    let mut app = App::new();

    app.add_plugins(
        DefaultPlugins
            .set(ImagePlugin::default_nearest()) // Prevents blurry sprites
            .set(WindowPlugin {
                primary_window: Some(Window {
                    title: TITLE.into(),
                    mode: WindowMode::Windowed,
                    position: WindowPosition::Centered(MonitorSelection::Primary),
                    resolution: WindowResolution::new(WIDTH, HEIGHT),

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
                meta_check: AssetMetaCheck::Never,
                ..default()
            }),
    )
    // Networking: systems are disabled until server/client resource is added
    .add_plugins((
        RenetServerPlugin,
        NetcodeServerPlugin,
        RenetClientPlugin,
        NetcodeClientPlugin,
    ))
    .add_plugins((AudioPlugin, GamePlugin));

    #[cfg(target_os = "windows")]
    app.add_systems(Startup, set_window_icon);

    app.run();
}

#[cfg(target_os = "windows")]
pub fn set_window_icon(windows: NonSend<WinitWindows>) {
    let image = image::open("assets/images/icons/planet.png")
        .unwrap()
        .into_rgba8();
    let (width, height) = image.dimensions();
    let rgba = image.into_raw();

    let icon = Icon::from_rgba(rgba, width, height).unwrap();

    for window in windows.windows.values() {
        window.set_window_icon(Some(icon.clone()));
    }
}
