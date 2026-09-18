//! Strategic map data plus its Bevy rendering and interaction adapters.

pub(crate) mod asteroids;
#[cfg(feature = "app")]
pub(crate) mod battle;
#[cfg(feature = "app")]
pub(crate) mod colonization;
#[cfg(feature = "app")]
pub(crate) mod details;
#[cfg(feature = "app")]
pub(crate) mod detection;
#[cfg(feature = "app")]
pub(crate) mod fauna;
pub mod icon;
pub mod model;
#[cfg(feature = "app")]
pub(crate) mod orbital_railgun;
pub mod planet;
#[cfg(feature = "app")]
mod scanner;
#[cfg(feature = "app")]
pub(crate) mod scenery;
#[cfg(feature = "app")]
pub mod systems;
#[cfg(feature = "app")]
pub mod utils;

/// Places transient map-result labels above the planet name in a shared vertical stack.
#[cfg(feature = "app")]
pub(super) fn aftermath_label_y(world_size: f32, row: usize) -> f32 {
    world_size * 0.7 + crate::core::constants::TITLE_TEXT_SIZE * (1.15 + row as f32 * 1.05)
}

#[cfg(feature = "app")]
const AFTERMATH_LABEL_RISE: f32 = 14.0;
#[cfg(feature = "app")]
pub(super) const AFTERMATH_LABEL_EXTENSION_SECONDS: f32 = 1.0;

/// Fades a map-result caption in, drifts it upward, and fades it out at the end.
#[cfg(feature = "app")]
pub(super) fn aftermath_label_motion(
    base_y: f32,
    elapsed: f32,
    appear_at: f32,
    disappear_at: f32,
    fade_in_seconds: f32,
    fade_out_seconds: f32,
) -> (f32, f32) {
    let lifetime = (disappear_at - appear_at).max(f32::EPSILON);
    let progress = ((elapsed - appear_at) / lifetime).clamp(0.0, 1.0);
    let fade_in = smoothstep((elapsed - appear_at) / fade_in_seconds);
    let fade_out = smoothstep((disappear_at - elapsed) / fade_out_seconds);
    (base_y + AFTERMATH_LABEL_RISE * progress, fade_in * fade_out)
}

#[cfg(feature = "app")]
fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

#[cfg(all(test, feature = "app"))]
#[path = "../../../tests/core/map_labels.rs"]
mod tests;
