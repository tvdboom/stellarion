//! Strategic map data plus its Bevy rendering and interaction adapters.

pub mod icon;
pub mod model;
pub mod planet;
#[cfg(feature = "app")]
pub mod systems;
#[cfg(feature = "app")]
pub mod utils;
