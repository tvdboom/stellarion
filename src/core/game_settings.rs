use crate::core::states::AudioState;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct GameSettings {
    pub audio: AudioState,
    pub n_players: u8,
    pub n_planets: u8,
    pub show_resources: bool,
    pub cycle: usize,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            audio: AudioState::default(),
            n_players: 2,
            n_planets: 10,
            show_resources: false,
            cycle: 1,
        }
    }
}
