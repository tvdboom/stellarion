use std::collections::{HashMap, HashSet};
use std::net::UdpSocket;
use std::time::SystemTime;

use bevy::prelude::*;
use bevy_renet::netcode::*;
use bevy_renet::renet::*;
use bincode::config::standard;
use bincode::serde::{decode_from_slice, encode_to_vec};
use serde::{Deserialize, Serialize};

use crate::core::audio::PlayAudioMsg;
use crate::core::map::map::Map;
use crate::core::map::planet::PlanetId;
use crate::core::menu::buttons::LobbyTextCmp;
use crate::core::missions::{Mission, MissionId, Missions};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::turns::{PreviousEndTurnState, StartTurnMsg};
use crate::core::ui::systems::UiState;
use crate::utils::get_local_ip;

const PROTOCOL_ID: u64 = 7;

#[derive(Resource)]
pub struct Ip(pub String);

impl Default for Ip {
    fn default() -> Self {
        Self(get_local_ip().to_string())
    }
}

#[derive(Resource, Default)]
pub struct Host {
    /// Maps client IDs to their respective players
    pub clients: HashMap<ClientId, Player>,

    /// All missions in the game with real stats
    pub missions: HashMap<MissionId, Mission>,

    /// Keeps track of which clients have ended their turn
    pub turn_ended: HashSet<ClientId>,
}

#[derive(Message)]
pub struct ServerSendMsg {
    pub message: ServerMessage,
    pub client: Option<ClientId>,
}

impl ServerSendMsg {
    pub fn new(message: ServerMessage, client: Option<ClientId>) -> Self {
        Self {
            message,
            client,
        }
    }
}

#[derive(Message)]
pub struct ClientSendMsg {
    pub message: ClientMessage,
}

impl ClientSendMsg {
    pub fn new(message: ClientMessage) -> Self {
        Self {
            message,
        }
    }
}

#[derive(Serialize, Deserialize)]
pub enum ServerMessage {
    LoadGame {
        turn: usize,
        map: Map,
        player: Player,
        missions: Missions,
    },
    NPlayers(usize),
    StartGame {
        id: ClientId,
        home_planet: PlanetId,
        map: Map,
    },
    StartTurn {
        map: Map,
        player: Player,
        missions: Missions,
    },
}

#[derive(Serialize, Deserialize)]
pub enum ClientMessage {
    EndTurn {
        end_turn: bool,
        player: Player,
        missions: Missions,
    },
}

pub fn new_renet_client(ip: &String) -> (RenetClient, NetcodeClientTransport) {
    let server_addr = format!("{ip}:5000").parse().unwrap();
    let socket = UdpSocket::bind("0.0.0.0:0").unwrap();
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let client_id = current_time.as_millis() as u64;
    let authentication = ClientAuthentication::Unsecure {
        client_id,
        protocol_id: PROTOCOL_ID,
        server_addr,
        user_data: None,
    };

    let transport = NetcodeClientTransport::new(current_time, authentication, socket).unwrap();
    let client = RenetClient::new(ConnectionConfig::default());

    (client, transport)
}

pub fn new_renet_server() -> (RenetServer, NetcodeServerTransport) {
    let public_addr = "0.0.0.0:5000".parse().unwrap();
    let socket = UdpSocket::bind(public_addr).expect("Socket already in use.");
    let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap();
    let server_config = ServerConfig {
        current_time,
        max_clients: 4,
        protocol_id: PROTOCOL_ID,
        public_addresses: vec![public_addr],
        authentication: ServerAuthentication::Unsecure,
    };

    let transport = NetcodeServerTransport::new(server_config, socket).unwrap();
    let server = RenetServer::new(ConnectionConfig::default());

    (server, transport)
}

pub fn server_update(
    mut n_players_q: Query<&mut Text, With<LobbyTextCmp>>,
    mut server: ResMut<RenetServer>,
    mut server_ev: MessageReader<ServerEvent>,
    mut play_audio_ev: MessageWriter<PlayAudioMsg>,
    app_state: Res<State<AppState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    for ev in server_ev.read() {
        if *app_state != AppState::Game {
            let n_players = server.clients_id().len() + 1;

            // Update the number of players in the lobby
            let message = encode_to_vec(&ServerMessage::NPlayers(n_players), standard()).unwrap();
            server.broadcast_message(DefaultChannel::ReliableOrdered, message);

            if let Ok(mut text) = n_players_q.single_mut() {
                if n_players == 1 {
                    text.0 = format!("Waiting for other players to join {}...", get_local_ip());
                    next_app_state.set(AppState::Lobby);
                } else {
                    text.0 = format!("There are {n_players} players in the lobby.\nWaiting for other players to join {}...", get_local_ip());
                    next_app_state.set(AppState::ConnectedLobby);
                }
            }
        } else {
            match ev {
                ServerEvent::ClientConnected {
                    client_id,
                } => {
                    println!("Client {client_id} connected");
                },
                ServerEvent::ClientDisconnected {
                    client_id,
                    reason,
                } => {
                    println!("Client {client_id} disconnected: {reason}");
                    play_audio_ev.write(PlayAudioMsg {
                        name: "error",
                        volume: 0.5,
                    });
                    next_game_state.set(GameState::InGameMenu);
                },
            }
        }
    }
}

pub fn server_send_message(
    mut server_send_msg: MessageReader<ServerSendMsg>,
    mut server: ResMut<RenetServer>,
) {
    for ev in server_send_msg.read() {
        let message = encode_to_vec(&ev.message, standard()).unwrap();
        if let Some(client_id) = ev.client {
            server.send_message(client_id, DefaultChannel::ReliableOrdered, message);
        } else {
            server.broadcast_message(DefaultChannel::ReliableOrdered, message);
        }
    }
}

pub fn server_receive_message(mut server: ResMut<RenetServer>, mut host: Option<ResMut<Host>>) {
    for id in server.clients_id() {
        while let Some(message) = server.receive_message(id, DefaultChannel::ReliableOrdered) {
            let (d, _) = decode_from_slice(&message, standard()).unwrap();
            match d {
                ClientMessage::EndTurn {
                    end_turn,
                    player,
                    missions,
                } => {
                    if let Some(host) = &mut host {
                        host.clients
                            .entry(id)
                            .and_modify(|p| *p = player.clone())
                            .or_insert(player);

                        for mission in missions.iter().filter(|m| m.owner == id) {
                            host.missions.insert(mission.id, mission.clone());
                        }

                        if end_turn {
                            host.turn_ended.insert(id);
                        } else {
                            host.turn_ended.remove(&id);
                        }
                    }
                },
            }
        }
    }
}

pub fn client_send_message(
    mut client_send_msg: MessageReader<ClientSendMsg>,
    mut client: ResMut<RenetClient>,
) {
    for ev in client_send_msg.read() {
        let message = encode_to_vec(&ev.message, standard()).unwrap();
        client.send_message(DefaultChannel::ReliableOrdered, message);
    }
}

pub fn client_receive_message(
    mut commands: Commands,
    mut n_players_q: Query<&mut Text, With<LobbyTextCmp>>,
    mut client: ResMut<RenetClient>,
    mut game_settings: ResMut<Settings>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut start_turn_msg: MessageWriter<StartTurnMsg>,
) {
    while let Some(message) = client.receive_message(DefaultChannel::ReliableOrdered) {
        let (d, _) = decode_from_slice(&message, standard()).unwrap();
        match d {
            ServerMessage::NPlayers(i) => {
                if let Ok(mut text) = n_players_q.single_mut() {
                    text.0 = format!("There are {i} players in the lobby.\nWaiting for the host to start the game...");
                }
            },
            ServerMessage::StartGame {
                id,
                home_planet,
                map,
            } => {
                *game_settings = game_settings.clone();

                commands.insert_resource(UiState::default());
                commands.insert_resource(PreviousEndTurnState::default());
                commands.insert_resource(Player::new(id, home_planet));
                commands.insert_resource(map);
                commands.insert_resource(Missions::default());

                next_app_state.set(AppState::Game);
            },
            ServerMessage::LoadGame {
                turn,
                map,
                player,
                missions,
            } => {
                game_settings.turn = turn;

                commands.insert_resource(UiState::default());
                commands.insert_resource(PreviousEndTurnState::default());
                commands.insert_resource(map);
                commands.insert_resource(player);
                commands.insert_resource(missions);

                next_app_state.set(AppState::Game);
            },
            ServerMessage::StartTurn {
                map,
                player,
                missions,
            } => {
                commands.insert_resource(map);
                commands.insert_resource(player);
                commands.insert_resource(missions);
                start_turn_msg.write(StartTurnMsg);
            },
        }
    }
}
