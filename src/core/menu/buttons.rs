use crate::core::assets::WorldAssets;
use crate::core::constants::*;
use crate::core::map::map::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::menu::utils::{add_text, recolor};
use crate::core::network::{
    new_renet_client, new_renet_server, Ip, ServerMessage, ServerSendMessage,
};
use crate::core::persistence::{LoadGameEv, SaveGameEv};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::ui::systems::UiState;
use crate::utils::NameFromEnum;
use bevy::prelude::*;
use bevy_renet::netcode::{NetcodeClientTransport, NetcodeServerTransport};
use bevy_renet::renet::{RenetClient, RenetServer};
use rand::prelude::IteratorRandom;
use rand::rng;

#[derive(Component)]
pub struct MenuCmp;

#[derive(Component, Clone, Debug, PartialEq)]
pub enum MenuBtn {
    Singleplayer,
    Multiplayer,
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
    trigger: Trigger<Pointer<Click>>,
    mut commands: Commands,
    btn_q: Query<(Option<&DisabledButton>, &MenuBtn)>,
    server: Option<ResMut<RenetServer>>,
    mut client: Option<ResMut<RenetClient>>,
    settings: Res<Settings>,
    ip: Res<Ip>,
    mut load_game_ev: EventWriter<LoadGameEv>,
    mut save_game_ev: EventWriter<SaveGameEv>,
    mut server_send_message: EventWriter<ServerSendMessage>,
    app_state: Res<State<AppState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    let (disabled, btn) = btn_q.get(trigger.target()).unwrap();

    if disabled.is_some() {
        return;
    }

    match btn {
        MenuBtn::Singleplayer => {
            let mut map = Map::new(settings.n_planets);

            // Alter home planet's stats
            map.planets.iter_mut().find(|p| p.id == 0).unwrap().make_home_planet(0);

            commands.insert_resource(UiState::default());
            commands.insert_resource(map);
            commands.insert_resource(Player::new(0));
            next_app_state.set(AppState::Game);
        },
        MenuBtn::Multiplayer => {
            next_app_state.set(AppState::MultiPlayerMenu);
        },
        MenuBtn::NewGame => {
            let server = server.unwrap();

            let mut map = Map::new(settings.n_planets * settings.n_players);

            // Determine home planets
            let mut home_planets: Vec<(PlanetId, Vec2)> = vec![];
            while home_planets.len() < settings.n_players as usize {
                let candidate =
                    map.planets.iter().choose(&mut rng()).map(|p| (p.id, p.position)).unwrap();

                if !home_planets.iter().all(|&p| p.1.distance(candidate.1) < Planet::SIZE * 3.) {
                    home_planets.push(candidate);
                }
            }

            // Alter home planets
            let mut clients = server.clients_id();
            clients.push(0);

            home_planets.iter().zip(clients).for_each(|((planet_id, _), client_id)| {
                if let Some(planet) = map.planets.iter_mut().find(|p| p.id == *planet_id) {
                    planet.make_home_planet(client_id);
                }
            });

            // Send the start game signal to all clients with their player id
            for client in server.clients_id().iter() {
                server_send_message.write(ServerSendMessage {
                    message: ServerMessage::StartGame {
                        id: *client,
                        map: map.clone(),
                    },
                    client: Some(*client),
                });
            }

            commands.insert_resource(UiState::default());
            commands.insert_resource(map);
            commands.insert_resource(Player::new(0));

            next_app_state.set(AppState::Game);
        },
        MenuBtn::LoadGame => {
            load_game_ev.write(LoadGameEv);
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
            AppState::Lobby => {
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
            save_game_ev.write(SaveGameEv);
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
        .observe(recolor::<Pointer<Over>>(HOVERED_BUTTON_COLOR))
        .observe(recolor::<Pointer<Out>>(NORMAL_BUTTON_COLOR))
        .observe(recolor::<Pointer<Pressed>>(PRESSED_BUTTON_COLOR))
        .observe(recolor::<Pointer<Released>>(HOVERED_BUTTON_COLOR))
        .observe(on_click_menu_button)
        .with_children(|parent| {
            parent.spawn(add_text(btn.to_title(), "bold", BUTTON_TEXT_SIZE, assets, window));
        });
}
