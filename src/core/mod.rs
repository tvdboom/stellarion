//! Shared deterministic rules plus the optional Bevy application plugin.

#[cfg(feature = "app")]
mod app;
#[cfg(feature = "app")]
mod assets;
#[cfg(feature = "app")]
mod audio;
#[cfg(feature = "app")]
pub mod basis_texture;
#[cfg(feature = "app")]
mod camera;
pub mod combat;
pub mod constants;
pub mod identity;
#[cfg(feature = "app")]
mod loading;
pub mod map;
#[cfg(feature = "app")]
mod menu;
#[cfg(feature = "app")]
pub mod messages;
#[cfg(feature = "app")]
mod mission_systems;
pub mod missions;
pub mod orders;
pub mod player;
pub mod random;
pub mod resources;
mod settings;
pub mod simulation;
pub mod states;
#[cfg(feature = "app")]
mod systems;
#[cfg(feature = "app")]
mod turns;
#[cfg(feature = "app")]
mod ui;
pub mod units;
#[cfg(feature = "app")]
mod utils;

#[cfg(feature = "app")]
pub use app::GamePlugin;
