use bevy::prelude::*;
use bevy::window::WindowResized;
use bevy_renet::netcode::NetcodeServerTransport;
use bevy_renet::renet::{RenetClient, RenetServer};
use itertools::Itertools;

use crate::core::camera::MainCamera;
use crate::core::combat::systems::BackgroundImageCmp;
use crate::core::map::map::Map;
use crate::core::menu::utils::TextSize;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::turns::StartTurnMsg;
use crate::core::ui::systems::{MissionTab, Shop, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::Unit;

pub fn on_resize_system(
    mut resize_reader: MessageReader<WindowResized>,
    mut text: Query<(&mut TextFont, &TextSize)>,
    mut bg_q: Query<&mut Sprite, With<BackgroundImageCmp>>,
    camera: Single<&Projection, With<MainCamera>>,
) {
    let Projection::Orthographic(projection) = camera.into_inner() else {
        panic!("Expected Orthographic projection.");
    };

    let (width, height) = (projection.area.width(), projection.area.height());

    for window in resize_reader.read() {
        for (mut text, size) in text.iter_mut() {
            text.font_size = size.0 * window.height / 460.
        }

        // Resize background images to cover the whole screen
        for mut bg_s in &mut bg_q {
            bg_s.custom_size = Some(Vec2::new(width, height));
        }
    }
}

pub fn check_keys_menu(
    mut commands: Commands,
    app_state: Res<State<AppState>>,
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    server: Option<ResMut<RenetServer>>,
    mut client: Option<ResMut<RenetClient>>,
    mut state: Option<ResMut<UiState>>,
    mut start_turn_msg: MessageWriter<StartTurnMsg>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let ctrl_pressed = keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);

    if keyboard.just_pressed(KeyCode::Escape) {
        match app_state.get() {
            AppState::SinglePlayerMenu | AppState::MultiPlayerMenu | AppState::Settings => {
                next_app_state.set(AppState::MainMenu)
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

                next_app_state.set(AppState::MultiPlayerMenu)
            },
            AppState::Game => {
                // Open in-game menu or exit mission/planet selection
                match game_state.get() {
                    GameState::Playing => {
                        let state = state.as_mut().unwrap();
                        if state.planet_selected.is_some() || state.mission {
                            state.planet_selected = None;
                            state.mission = false;
                            state.combat_report = None;
                        } else {
                            next_game_state.set(GameState::GameMenu)
                        }
                    },
                    GameState::CombatMenu | GameState::GameMenu => {
                        next_game_state.set(GameState::Playing)
                    },
                    GameState::Combat => next_game_state.set(GameState::CombatMenu),
                    GameState::EndGame => next_app_state.set(AppState::MainMenu),
                    GameState::Settings => next_game_state.set(GameState::GameMenu),
                }
            },
            _ => (),
        }
    }

    if ctrl_pressed && keyboard.just_pressed(KeyCode::Enter) && *app_state.get() == AppState::Game {
        if *game_state.get() == GameState::Playing {
            let mut state = state.unwrap();
            if !state.mission {
                state.planet_selected = None;
                state.mission = false;
                state.combat_report = None;
                state.end_turn = !state.end_turn;
            }
        } else if *game_state.get() == GameState::CombatMenu {
            start_turn_msg.write(StartTurnMsg::new(true, false));
            next_game_state.set(GameState::Playing)
        }
    }
}

pub fn check_keys_combat(mut settings: ResMut<Settings>, keyboard: Res<ButtonInput<KeyCode>>) {
    if keyboard.just_pressed(KeyCode::Space) {
        settings.combat_paused = !settings.combat_paused;
    } else if !settings.combat_paused {
        if keyboard.just_released(KeyCode::ArrowRight) {
            settings.combat_speed = (settings.combat_speed * 2.).min(16.0);
        } else if keyboard.just_released(KeyCode::ArrowLeft) {
            settings.combat_speed = (settings.combat_speed * 0.5).max(0.25);
        }
    }
}

pub fn check_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut state: ResMut<UiState>,
    mut settings: ResMut<Settings>,
) {
    let ctrl_pressed = keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    // Hack to add resources and bump building levels to max
    #[cfg(debug_assertions)]
    if ctrl_pressed && keyboard.just_pressed(KeyCode::ArrowUp) {
        player.resources += 1_000usize;

        let mut planets = if shift_pressed {
            map.planets.iter_mut().filter(|p| p.owned == Some(player.id)).collect::<Vec<_>>()
        } else {
            map.planets.iter_mut().collect::<Vec<_>>()
        };

        planets.iter_mut().for_each(|p| {
            for unit in Unit::all().iter().flatten() {
                if unit.is_building() {
                    *p.army.entry(*unit).or_insert(0) = Building::MAX_LEVEL;
                } else {
                    *p.army.entry(*unit).or_insert(0) += 3;
                }
            }
        });
    }

    // Toggle show voronoi cells
    if keyboard.just_pressed(KeyCode::KeyC) {
        settings.show_cells = !settings.show_cells;
    }

    // Toggle show planet info
    if keyboard.just_pressed(KeyCode::KeyI) {
        settings.show_info = !settings.show_info;
    }

    // Toggle show hover info
    if keyboard.just_pressed(KeyCode::KeyH) {
        settings.show_hover = !settings.show_hover;
    }

    // Toggle shop panel
    if keyboard.just_pressed(KeyCode::KeyB) {
        settings.show_menu = !settings.show_menu;
    }

    // Toggle mission panel
    if keyboard.just_pressed(KeyCode::KeyM) {
        state.planet_selected = None;
        state.mission = !state.mission;
        state.combat_report = None;
    }

    // Go back to home planet
    if keyboard.just_pressed(KeyCode::Space) {
        state.planet_selected = Some(player.home_planet);
        state.to_selected = true;
        state.mission = false;
    }

    // Move between owned planets / moons
    if ctrl_pressed {
        if keyboard.just_pressed(KeyCode::Tab) && !state.mission && state.combat_report.is_none() {
            if let Some(selected) = state.planet_selected {
                let planets: Vec<_> = map
                    .planets
                    .iter()
                    .sorted_by(|a, b| a.name.cmp(&b.name))
                    .filter_map(|p| {
                        (player.owns(p) || (p.is_moon() && player.controls(p))).then_some(p.id)
                    })
                    .collect();

                if let Some(pos) = planets.iter().position(|id| *id == selected) {
                    let len = planets.len();

                    let new_index = if shift_pressed {
                        (pos + len - 1) % len
                    } else {
                        (pos + 1) % len
                    };

                    state.planet_selected = Some(planets[new_index]);
                }
            }
        }
    } else if let Some(id) = state.combat_report {
        let report = player.reports.iter().find(|r| r.id == id).unwrap();
        let max_rounds = report.combat_report.as_ref().unwrap().rounds.len();

        // Move between rounds
        if mouse.just_pressed(MouseButton::Forward) || keyboard.just_pressed(KeyCode::Tab) {
            state.combat_report_round = (state.combat_report_round + 1).min(max_rounds);
        } else if mouse.just_pressed(MouseButton::Back)
            || (shift_pressed && keyboard.just_pressed(KeyCode::Tab))
        {
            state.combat_report_round = (state.combat_report_round - 1).max(1);
        }
    } else if state.mission {
        // Move between mission or shop tabs
        if mouse.just_pressed(MouseButton::Forward) || keyboard.just_pressed(KeyCode::Tab) {
            state.mission_tab = match &state.mission_tab {
                MissionTab::NewMission => MissionTab::ActiveMissions,
                MissionTab::ActiveMissions => MissionTab::EnemyMissions,
                MissionTab::EnemyMissions => MissionTab::MissionReports,
                MissionTab::MissionReports => MissionTab::NewMission,
            };
        } else if mouse.just_pressed(MouseButton::Back)
            || (shift_pressed && keyboard.just_pressed(KeyCode::Tab))
        {
            state.mission_tab = match &state.mission_tab {
                MissionTab::NewMission => MissionTab::MissionReports,
                MissionTab::ActiveMissions => MissionTab::NewMission,
                MissionTab::EnemyMissions => MissionTab::ActiveMissions,
                MissionTab::MissionReports => MissionTab::EnemyMissions,
            };
        }
    } else if settings.show_menu && state.planet_selected.is_some() {
        let planet = map.planets.get(state.planet_selected.unwrap()).unwrap();
        if mouse.just_pressed(MouseButton::Back)
            || (shift_pressed && keyboard.just_pressed(KeyCode::Tab))
        {
            state.shop = match &state.shop {
                Shop::Buildings => {
                    if planet.is_moon() {
                        Shop::Fleet
                    } else {
                        Shop::Defenses
                    }
                },
                Shop::Fleet => Shop::Buildings,
                Shop::Defenses => Shop::Fleet,
            };
        } else if mouse.just_pressed(MouseButton::Forward) || keyboard.just_pressed(KeyCode::Tab) {
            state.shop = match &state.shop {
                Shop::Buildings => Shop::Fleet,
                Shop::Fleet => {
                    if planet.is_moon() {
                        Shop::Buildings
                    } else {
                        Shop::Defenses
                    }
                },
                Shop::Defenses => Shop::Buildings,
            };
        }
    }
}
