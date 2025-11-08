use std::fs::File;
use std::io;
use std::io::{Read, Write};

use bevy::prelude::*;
use bevy_renet::renet::{ClientId, DefaultChannel, RenetServer};
use bincode::config::standard;
use bincode::serde::{decode_from_slice, encode_to_vec};
#[cfg(not(target_arch = "wasm32"))]
use rfd::FileDialog;
use serde::{Deserialize, Serialize};

use crate::core::map::map::Map;
use crate::core::messages::MessageMsg;
use crate::core::missions::{Mission, Missions};
use crate::core::network::{Host, ServerMessage, ServerSendMsg};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, AudioState};
use crate::core::turns::{filter_missions, PreviousEndTurnState};
use crate::core::ui::systems::UiState;

#[derive(Default)]
pub enum SaveState {
    #[default]
    WaitingForRequest,
    SaveGame,
    WaitingForClients,
}

#[derive(Serialize, Deserialize)]
pub struct SaveAll {
    pub settings: Settings,
    pub map: Map,
    pub host: Player,
    pub clients: Vec<Player>,
    pub missions: Vec<Mission>,
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

            let mut start_game = true;

            let ids = data.clients.iter().map(|p| p.id).collect::<Vec<_>>();

            let n_opponents = ids.len();
            if n_opponents > 0 {
                if let Some(server) = &server {
                    let n_clients = server.clients_id().len();
                    if n_clients != n_opponents {
                        start_game = false;
                        message.write(MessageMsg::error(format!("The loaded game has {n_opponents} opponents but the server has {n_clients} clients.")));
                    } else {
                        for (new_id, old_id) in server.clients_id().iter().zip(ids) {
                            let player = data.clients.iter_mut().find(|p| p.id == old_id).unwrap();

                            // Update player and planets to use the new player id
                            player.id = *new_id;

                            let upd = |v: &mut Option<ClientId>| {
                                if *v == Some(old_id) {
                                    *v = Some(*new_id)
                                }
                            };
                            let upd_id = |v: &mut ClientId| {
                                if *v == old_id {
                                    *v = *new_id
                                }
                            };

                            for p in &mut data.map.planets {
                                upd(&mut p.owned);
                                upd(&mut p.controlled);
                            }
                            for r in &mut player.reports {
                                upd_id(&mut r.mission.owner);
                                [
                                    &mut r.mission.origin_owned,
                                    &mut r.planet.owned,
                                    &mut r.planet.controlled,
                                    &mut r.destination_owned,
                                    &mut r.destination_controlled,
                                ]
                                .into_iter()
                                .for_each(|f| upd(f));
                            }
                            for m in data.missions.iter_mut() {
                                upd_id(&mut m.owner);
                                upd(&mut m.origin_owned);
                            }

                            server_send_msg.write(ServerSendMsg::new(
                                ServerMessage::LoadGame {
                                    turn: data.settings.turn,
                                    p_colonizable: data.settings.p_colonizable,
                                    map: data.map.clone(),
                                    player: player.clone(),
                                    missions: if !player.spectator {
                                        Missions(filter_missions(
                                            &data.missions,
                                            &data.map,
                                            &player,
                                        ))
                                    } else {
                                        Missions(data.missions.clone())
                                    },
                                },
                                Some(*new_id),
                            ));
                        }
                    }
                } else {
                    start_game = false;
                    message.write(MessageMsg::error(format!("The loaded game contains {n_opponents} opponents but there is no server initiated.")));
                }
            }

            if start_game {
                next_audio_state.set(data.settings.audio);

                commands.insert_resource(UiState::default());
                commands.insert_resource(PreviousEndTurnState::default());
                commands.insert_resource(data.settings);
                commands.insert_resource(if !data.host.spectator {
                    Missions(filter_missions(&data.missions, &data.map, &data.host))
                } else {
                    Missions(data.missions.clone())
                });
                commands.insert_resource(data.map);
                commands.insert_resource(data.host);
                commands.insert_resource(Host::default());

                next_app_state.set(AppState::Game);

                message.write(MessageMsg::info("Game loaded."));
            }
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn save_game(
    server: Option<ResMut<RenetServer>>,
    mut save_game_ev: MessageReader<SaveGameMsg>,
    settings: Res<Settings>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    mut host: ResMut<Host>,
    mut message: MessageWriter<MessageMsg>,
    mut state: Local<SaveState>,
) {
    if let Some(mut server) = server {
        match *state {
            SaveState::WaitingForRequest => {
                for _ in save_game_ev.read() {
                    // Request an update of every player's state
                    let msg = encode_to_vec(&ServerMessage::RequestUpdate, standard()).unwrap();
                    server.broadcast_message(DefaultChannel::ReliableOrdered, msg);

                    *state = SaveState::WaitingForClients;
                }
            },
            SaveState::WaitingForClients => {
                // Wait until all clients have sent an update
                if host.received.len() == server.clients_id().len() {
                    host.received.clear();
                    *state = SaveState::SaveGame;
                }
            },
            SaveState::SaveGame => {
                // Save the game
                if let Some(mut file_path) = FileDialog::new().save_file() {
                    if !file_path.extension().map(|e| e == "bin").unwrap_or(false) {
                        file_path.set_extension("bin");
                    }

                    let all_missions = missions
                        .iter()
                        .filter(|m| m.owner == player.id)
                        .chain(host.missions.iter())
                        .cloned()
                        .collect::<Vec<_>>();

                    let file_path_str = file_path.to_string_lossy().to_string();
                    let data = SaveAll {
                        settings: settings.clone(),
                        map: map.clone(),
                        host: player.clone(),
                        clients: host.clients.values().cloned().collect(),
                        missions: all_missions,
                    };

                    save_to_bin(&file_path_str, &data).expect("Failed to save the game.");

                    message.write(MessageMsg::info("Game saved."));
                }

                *state = SaveState::WaitingForRequest;
            },
        }
    }
}
