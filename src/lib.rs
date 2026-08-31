//! Stellarion's reusable game, multiplayer, platform, and Bevy integration library.

#![warn(missing_docs)]

pub mod core;
pub mod multiplayer;
pub mod platform;
pub mod utils;

/// Human-readable application title used by native and browser builds.
pub const TITLE: &str = "Stellarion";
