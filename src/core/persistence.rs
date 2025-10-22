use std::fs::File;
use std::io;
use std::io::{Read, Write};

use bevy::prelude::*;
use bevy_renet::renet::RenetServer;
use bincode::config::standard;
use bincode::serde::{decode_from_slice, encode_to_vec};
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
use serde::{Deserialize, Serialize};

use crate::core::map::map::Map;
use crate::core::messages::MessageMsg;
use crate::core::missions::Missions;
use crate::core::network::{Host, ServerMessage, ServerSendMsg};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, AudioState};
use crate::core::turns::PreviousEndTurnState;
use crate::core::ui::systems::UiState;

#[derive(Serialize, Deserialize)]
pub struct SaveAll {
    pub settings: Settings,
    pub map: Map,
    pub host: Player,
    pub clients: Vec<Player>,
    pub missions: Missions,
}

#[derive(Message)]
pub struct LoadGameMsg;

#[derive(Message)]
pub struct SaveGameMsg;

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
    mut load_game_ev: MessageReader<LoadGameMsg>,
    server: Option<Res<RenetServer>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_audio_state: ResMut<NextState<AudioState>>,
    mut message: MessageWriter<MessageMsg>,
    mut server_send_msg: MessageWriter<ServerSendMsg>,
) {
    for _ in load_game_ev.read() {
        if let Some(file_path) = FileDialog::new().pick_file() {
            let file_path_str = file_path.to_string_lossy().to_string();
            let mut data = load_from_bin(&file_path_str).expect("Failed to load the game.");

            let ids = data.clients.iter().map(|p| p.id).collect::<Vec<_>>();

            let n_opponents = ids.len();
            if n_opponents > 1 {
                if let Some(server) = &server {
                    let n_clients = server.clients_id().len();
                    if n_clients != n_opponents {
                        message.write(MessageMsg::error(format!("The loaded game has {n_opponents} opponents but the server has {n_clients} clients.")));
                    } else {
                        for (new_id, old_id) in server.clients_id().iter().zip(ids.iter()) {
                            let player = data.clients.iter_mut().find(|p| p.id == *old_id).unwrap();

                            // Update player and planets to use the new player id
                            player.id = *new_id;
                            data.map.planets.iter_mut().for_each(|p| {
                                if p.owned.is_some_and(|id| id == *old_id) {
                                    p.owned = Some(*new_id);
                                }
                                if p.controlled.is_some_and(|id| id == *old_id) {
                                    p.controlled = Some(*new_id);
                                }
                            });

                            server_send_msg.write(ServerSendMsg::new(
                                ServerMessage::LoadGame {
                                    turn: data.settings.turn,
                                    map: data.map.clone(),
                                    player: player.clone(),
                                    missions: data.missions.clone(),
                                },
                                Some(*new_id),
                            ));
                        }
                    }
                } else {
                    message.write(MessageMsg::error(format!("The loaded game contains {n_opponents} opponents but there is no server initiated.")));
                }
            }

            next_audio_state.set(data.settings.audio);

            commands.insert_resource(UiState::default());
            commands.insert_resource(PreviousEndTurnState::default());
            commands.insert_resource(data.settings);
            commands.insert_resource(data.map);
            commands.insert_resource(data.host);
            commands.insert_resource(data.missions);
            commands.insert_resource(Host::default());

            next_app_state.set(AppState::Game);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_game(
    mut save_game_ev: MessageReader<SaveGameMsg>,
    settings: Res<Settings>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    host: Option<Res<Host>>,
) {
    if let Some(host) = host {
        for _ in save_game_ev.read() {
            if let Some(mut file_path) = FileDialog::new().save_file() {
                if !file_path.extension().map(|e| e == "bin").unwrap_or(false) {
                    file_path.set_extension("bin");
                }

                let file_path_str = file_path.to_string_lossy().to_string();
                let data = SaveAll {
                    settings: settings.clone(),
                    map: map.clone(),
                    host: player.clone(),
                    clients: host.clients.values().cloned().collect(),
                    missions: missions.clone(),
                };

                save_to_bin(&file_path_str, &data).expect("Failed to save the game.");
            }
        }
    }
}
