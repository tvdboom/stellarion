use crate::core::states::AudioState;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub audio: AudioState,
    pub n_players: u8,
    pub n_planets: u8,
    pub show_info: bool,
    pub show_hover: bool,
    pub turn: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio: AudioState::default(),
            n_players: 2,
            n_planets: 10,
            show_info: false,
            show_hover: true,
            turn: 1,
        }
    }
}
