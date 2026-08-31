//! Marker components shared by menu setup and cleanup systems.

use bevy::prelude::*;

/// Marks menu/background entities that are removed when their state exits.
#[derive(Component)]
pub struct MenuCmp;
