//! Shared deterministic rules plus the optional Bevy application plugin.

#[cfg(feature = "app")]
mod app;
#[cfg(feature = "app")]
mod assets;
#[cfg(feature = "app")]
pub(crate) mod audio;
#[cfg(feature = "app")]
pub mod basis_texture;
#[cfg(feature = "app")]
mod camera;
pub mod combat;
pub mod constants;
pub mod energy;
pub mod identity;
#[cfg(feature = "app")]
mod loading;
#[cfg(feature = "app")]
mod loading_screen;
pub mod map;
#[cfg(feature = "app")]
mod menu;
#[cfg(feature = "app")]
pub mod messages;
#[cfg(feature = "app")]
mod mission_systems;
pub mod missions;
pub mod orders;
#[cfg(feature = "app")]
mod patrol_visuals;
pub mod patrols;
pub mod player;
pub mod raids;
pub mod random;
pub mod recycling;
pub mod resources;
pub(crate) mod settings;
pub mod simulation;
pub mod states;
#[cfg(feature = "app")]
mod systems;
pub mod trading;
#[cfg(feature = "app")]
mod turns;
#[cfg(feature = "app")]
mod ui;
pub mod units;
#[cfg(feature = "app")]
mod utils;

#[cfg(feature = "app")]
pub use app::GamePlugin;

#[cfg(feature = "app")]
pub(crate) use audio::ChangeAudioMsg;

#[cfg(all(test, feature = "app", not(target_arch = "wasm32")))]
#[path = "../../tests/core/readme_screenshots.rs"]
mod readme_screenshots;
