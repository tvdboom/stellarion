use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[derive(States, EnumIter, Debug, Clone, Copy, Eq, PartialEq, Hash, Default)]
pub enum AppState {
    MainMenu,
    SinglePlayerMenu,
    MultiPlayerMenu,
    Lobby,
    ConnectedLobby,
    Settings,
    #[default]
    Game,
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub enum GameState {
    #[default]
    Playing,
    InGameMenu,
    EndGame,
}

#[derive(States, Debug, Clone, Copy, Eq, PartialEq, Hash, Default, Serialize, Deserialize)]
pub enum AudioState {
    Mute,
    #[default]
    NoMusic,
    Sound,
}
