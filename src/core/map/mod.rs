//! Strategic map data plus its Bevy rendering and interaction adapters.

#[cfg(feature = "app")]
pub(crate) mod battle;
#[cfg(feature = "app")]
pub(crate) mod colonization;
pub mod icon;
pub mod model;
pub mod planet;
#[cfg(feature = "app")]
mod scanner;
#[cfg(feature = "app")]
pub mod systems;
#[cfg(feature = "app")]
pub mod utils;
