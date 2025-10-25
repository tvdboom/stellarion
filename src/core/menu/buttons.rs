use bevy::prelude::*;
use bevy_renet::netcode::{NetcodeClientTransport, NetcodeServerTransport};
use bevy_renet::renet::{RenetClient, RenetServer};
use rand::prelude::IteratorRandom;
use rand::rng;

use crate::core::assets::WorldAssets;
use crate::core::constants::*;
use crate::core::map::map::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::menu::utils::{add_text, recolor};
use crate::core::missions::Missions;
use crate::core::network::{
    new_renet_client, new_renet_server, Host, Ip, ServerMessage, ServerSendMsg,
};
use crate::core::persistence::{LoadGameMsg, SaveGameMsg};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::turns::PreviousEndTurnState;
use crate::core::ui::systems::UiState;
use crate::utils::NameFromEnum;

#[derive(Component)]
pub struct MenuCmp;

#[derive(Component, Clone, Debug, PartialEq)]
pub enum MenuBtn {
    Singleplayer,
    StartGame,
    NewGame,
    LoadGame,
    HostGame,
    FindGame,
    Back,
    Continue,
    SaveGame,
    Settings,
    Quit,
}

#[derive(Component)]
pub struct DisabledButton;

#[derive(Component)]
pub struct LobbyTextCmp;

#[derive(Component)]
pub struct IpTextCmp;

pub fn on_click_menu_button(
    event: On<Pointer<Click>>,
    mut commands: Commands,
    btn_q: Query<(Option<&DisabledButton>, &MenuBtn)>,
    server: Option<ResMut<RenetServer>>,
    mut client: Option<ResMut<RenetClient>>,
    settings: Res<Settings>,
    ip: Res<Ip>,
    mut load_game_ev: MessageWriter<LoadGameMsg>,
    mut save_game_ev: MessageWriter<SaveGameMsg>,
    mut server_send_msg: MessageWriter<ServerSendMsg>,
    app_state: Res<State<AppState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    let (disabled, btn) = btn_q.get(event.entity).unwrap();

    if disabled.is_some() {
        return;
    }

    match btn {
        MenuBtn::Singleplayer => {
            let mut map = Map::new(settings.n_planets);

            // Alter home planet's stats
            map.planets.iter_mut().find(|p| p.id == 0).unwrap().make_home_planet(0);

            commands.insert_resource(UiState::default());
            commands.insert_resource(PreviousEndTurnState::default());
            commands.insert_resource(map);
            commands.insert_resource(Player::new(0, 0));
            commands.insert_resource(Missions::default());
            commands.insert_resource(Host::default());
            next_app_state.set(AppState::Game);
        },
        MenuBtn::StartGame => {
            next_app_state.set(AppState::MultiPlayerMenu);
        },
        MenuBtn::NewGame => {
            let server = server.unwrap();

            let clients = server.clients_id();
            let n_players = clients.len() + 1;

            let mut map = Map::new(settings.n_planets * n_players);

            // Determine home planets
            let mut home_planets: Vec<(PlanetId, Vec2)> = vec![];
            while home_planets.len() < n_players {
                let candidate =
                    map.planets.iter().choose(&mut rng()).map(|p| (p.id, p.position)).unwrap();

                if home_planets.iter().all(|&p| p.1.distance(candidate.1) > Planet::SIZE * 5.) {
                    home_planets.push(candidate);
                }
            }

            // Alter home planets
            let players = std::iter::once(&0).chain(&clients).collect::<Vec<_>>();
            home_planets.iter().zip(players).for_each(|((planet_id, _), client_id)| {
                if let Some(planet) = map.planets.iter_mut().find(|p| p.id == *planet_id) {
                    planet.make_home_planet(*client_id);
                }
            });

            // Send the start game signal to all clients with their player id
            for (client_id, (planet_id, _)) in clients.iter().zip(home_planets.iter().skip(1)) {
                server_send_msg.write(ServerSendMsg {
                    message: ServerMessage::StartGame {
                        id: *client_id,
                        home_planet: *planet_id,
                        map: map.clone(),
                    },
                    client: Some(*client_id),
                });
            }

            commands.insert_resource(UiState::default());
            commands.insert_resource(PreviousEndTurnState::default());
            commands.insert_resource(map);
            commands.insert_resource(Player::new(0, home_planets.first().unwrap().0));
            commands.insert_resource(Missions::default());
            commands.insert_resource(Host::default());

            next_app_state.set(AppState::Game);
        },
        MenuBtn::LoadGame => {
            load_game_ev.write(LoadGameMsg);
        },
        MenuBtn::HostGame => {
            // Remove client resources if they exist
            if client.is_some() {
                commands.remove_resource::<RenetClient>();
                commands.remove_resource::<NetcodeClientTransport>();
            }

            let (server, transport) = new_renet_server();
            commands.insert_resource(server);
            commands.insert_resource(transport);

            next_app_state.set(AppState::Lobby);
        },
        MenuBtn::FindGame => {
            let (server, transport) = new_renet_client(&ip.0);
            commands.insert_resource(server);
            commands.insert_resource(transport);

            next_app_state.set(AppState::Lobby);
        },
        MenuBtn::Back => match *app_state.get() {
            AppState::MultiPlayerMenu | AppState::Settings => {
                next_app_state.set(AppState::MainMenu);
            },
            AppState::Lobby | AppState::ConnectedLobby => {
                if let Some(client) = client.as_mut() {
                    client.disconnect();
                    commands.remove_resource::<RenetClient>();
                } else if let Some(mut server) = server {
                    server.disconnect_all();
                    commands.remove_resource::<RenetServer>();
                    commands.remove_resource::<NetcodeServerTransport>();
                }

                next_app_state.set(AppState::MultiPlayerMenu);
            },
            _ => unreachable!(),
        },
        MenuBtn::Continue => {
            next_game_state.set(GameState::Playing);
        },
        MenuBtn::SaveGame => {
            save_game_ev.write(SaveGameMsg);
        },
        MenuBtn::Settings => {
            next_app_state.set(AppState::Settings);
        },
        MenuBtn::Quit => match *app_state.get() {
            AppState::Game => {
                if let Some(client) = client.as_mut() {
                    client.disconnect();
                    commands.remove_resource::<RenetClient>();
                } else if let Some(mut server) = server {
                    server.disconnect_all();
                    commands.remove_resource::<RenetServer>();
                    commands.remove_resource::<NetcodeServerTransport>();
                }

                next_game_state.set(GameState::default());
                next_app_state.set(AppState::MainMenu)
            },
            AppState::MainMenu => std::process::exit(0),
            _ => unreachable!(),
        },
    }
}

pub fn spawn_menu_button(
    parent: &mut ChildSpawnerCommands,
    btn: MenuBtn,
    assets: &WorldAssets,
    window: &Window,
) {
    parent
        .spawn((
            Node {
                width: Val::Percent(25.),
                height: Val::Percent(10.),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                margin: UiRect::all(Val::Percent(1.)),
                ..default()
            },
            BackgroundColor(NORMAL_BUTTON_COLOR),
            btn.clone(),
        ))
        .observe(recolor::<Over>(HOVERED_BUTTON_COLOR))
        .observe(recolor::<Out>(NORMAL_BUTTON_COLOR))
        .observe(recolor::<Press>(PRESSED_BUTTON_COLOR))
        .observe(recolor::<Release>(HOVERED_BUTTON_COLOR))
        .observe(on_click_menu_button)
        .with_children(|parent| {
            parent.spawn(add_text(btn.to_title(), "bold", BUTTON_TEXT_SIZE, assets, window));
        });
}
