use crate::core::game_settings::GameSettings;
use crate::core::map::map::Map;
use crate::core::states::{AppState, AudioState};
use bevy::prelude::*;
use bincode::config::standard;
use bincode::serde::{decode_from_slice, encode_to_vec};
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io;
use std::io::{Read, Write};

#[derive(Serialize, Deserialize)]
pub struct SaveAll {
    pub game_settings: GameSettings,
    pub map: Map,
}

#[derive(Event)]
pub struct LoadGameEv;

#[derive(Event)]
pub struct SaveGameEv;

fn save_to_bin(file_path: &str, data: &SaveAll) -> io::Result<()> {
    let mut file = File::create(file_path)?;

    let buffer = encode_to_vec(data, standard()).expect("Failed to serialize data.");
    file.write_all(&buffer)?;

    Ok(())
}

fn load_from_bin(file_path: &str) -> io::Result<SaveAll> {
    let mut file = File::open(file_path)?;

    let mut buffer = vec![];
    file.read_to_end(&mut buffer)?;

    let (data, _) = decode_from_slice(&buffer, standard()).expect("Failed to deserialize data.");
    Ok(data)
}

#[cfg(not(target_arch = "wasm32"))]
pub fn load_game(
    mut commands: Commands,
    mut load_game_ev: EventReader<LoadGameEv>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_audio_state: ResMut<NextState<AudioState>>,
) {
    for _ in load_game_ev.read() {
        if let Some(file_path) = FileDialog::new().pick_file() {
            let file_path_str = file_path.to_string_lossy().to_string();
            let data = load_from_bin(&file_path_str).expect("Failed to load the game.");

            next_audio_state.set(data.game_settings.audio);
            commands.insert_resource(data.game_settings);
            commands.insert_resource(data.map);

            next_app_state.set(AppState::Game);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_game(
    mut save_game_ev: EventReader<SaveGameEv>,
    game_settings: Res<GameSettings>,
    map: Res<Map>,
) {
    for _ in save_game_ev.read() {
        if let Some(mut file_path) = FileDialog::new().save_file() {
            if !file_path.extension().map(|e| e == "bin").unwrap_or(false) {
                file_path.set_extension("bin");
            }

            let file_path_str = file_path.to_string_lossy().to_string();
            let data = SaveAll {
                game_settings: game_settings.clone(),
                map: map.clone(),
            };

            save_to_bin(&file_path_str, &data).expect("Failed to save the game.");
        }
    }
}
