use crate::core::states::AudioState;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub enum GameMode {
    SinglePlayer,
    Multiplayer,
}

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct GameSettings {
    pub game_mode: GameMode,
    pub audio: AudioState,
    pub n_players: u8,
    pub n_planets: u8,
}

impl Default for GameSettings {
    fn default() -> Self {
        Self {
            game_mode: GameMode::SinglePlayer,
            audio: AudioState::default(),
            n_players: 2,
            n_planets: 20,
        }
    }
}
