use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::states::AudioState;

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub audio: AudioState,
    pub n_planets: usize,
    pub show_cells: bool,
    pub show_info: bool,
    pub show_hover: bool,
    pub turn: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio: AudioState::default(),
            n_planets: 3,
            show_cells: true,
            show_info: false,
            show_hover: true,
            turn: 1,
        }
    }
}
