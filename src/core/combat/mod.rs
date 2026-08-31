//! Deterministic combat rules, reports, statistics, and optional Bevy presentation.

pub mod report;
pub mod resolution;
pub mod stats;
#[cfg(feature = "app")]
pub mod systems;
