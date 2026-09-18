//! Deterministic combat rules, reports, statistics, and optional Bevy presentation.

#[cfg(feature = "app")]
pub(crate) mod cinematic;
#[cfg(any(feature = "app", test))]
pub(crate) mod cinematic_timeline;
#[cfg(feature = "app")]
pub(crate) mod cinematic_ui;
#[cfg(feature = "app")]
pub(crate) mod effects;
#[cfg(feature = "app")]
pub(crate) mod playback;
pub mod report;
pub mod resolution;
pub mod stats;
#[cfg(feature = "app")]
pub mod systems;
