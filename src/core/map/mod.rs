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

/// Places transient map-result labels above the planet name in a shared vertical stack.
#[cfg(feature = "app")]
pub(super) fn aftermath_label_y(world_size: f32, row: usize) -> f32 {
    world_size * 0.7 + crate::core::constants::TITLE_TEXT_SIZE * (1.15 + row as f32 * 1.05)
}
