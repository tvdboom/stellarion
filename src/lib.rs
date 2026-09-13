//! Stellarion's reusable game, multiplayer, platform, and Bevy integration library.

#![warn(missing_docs)]

pub mod core;
pub mod multiplayer;
pub mod platform;
mod serialization;
pub mod utils;

#[cfg(test)]
#[path = "../tests/core/support.rs"]
pub(crate) mod test_support;

/// Human-readable application title used by native and browser builds.
pub const TITLE: &str = "Stellarion";
