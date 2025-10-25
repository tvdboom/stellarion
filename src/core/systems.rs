use std::collections::HashMap;

use bevy::prelude::*;
use bevy::window::WindowResized;
use itertools::Itertools;
use strum::IntoEnumIterator;

use crate::core::map::map::Map;
use crate::core::menu::utils::TextSize;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::ui::systems::{MissionTab, Shop, UiState};
use crate::core::units::buildings::Building;

pub fn on_resize_system(
    mut resize_reader: MessageReader<WindowResized>,
    mut text: Query<(&mut TextFont, &TextSize)>,
) {
    for ev in resize_reader.read() {
        for (mut text, size) in text.iter_mut() {
            text.font_size = size.0 * ev.height / 460.
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
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    // Hack to add resources and bump building levels to max
    if keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        if keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
            if keyboard.just_pressed(KeyCode::ArrowUp) {
                player.resources += 1000usize;
                map.planets.iter_mut().for_each(|p| {
                    p.complex = Building::iter()
                        .map(|c| (c, Building::MAX_LEVEL))
                        .collect::<HashMap<_, _>>()
                });
            }
        }
    }

    // Open in-game menu or exit planet selection
    if keyboard.just_pressed(KeyCode::Escape) {
        match game_state.get() {
            GameState::Playing => {
                if state.mission || state.planet_selected.is_some() {
                    state.mission = false;
                    state.planet_selected = None;
                } else {
                    next_game_state.set(GameState::InGameMenu)
                }
            },
            GameState::InGameMenu => next_game_state.set(GameState::Playing),
            GameState::EndGame => next_app_state.set(AppState::MainMenu),
        }
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
        state.mission = !state.mission;
    }

    // Go back to home planet
    if keyboard.just_pressed(KeyCode::Space) {
        state.planet_selected = Some(player.home_planet);
        state.to_selected = true;
    }

    // Move between owned planets
    if keyboard.just_pressed(KeyCode::Tab) {
        if let Some(selected) = state.planet_selected {
            let planets: Vec<_> = map
                .planets
                .iter()
                .sorted_by(|a, b| a.name.cmp(&b.name))
                .filter_map(|p| player.owns(p).then_some(p.id))
                .collect();

            if let Some(pos) = planets.iter().position(|id| *id == selected) {
                let len = planets.len();

                let new_index = if keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
                    (pos + len - 1) % len
                } else {
                    (pos + 1) % len
                };

                state.planet_selected = Some(planets[new_index]);
            }
        }
    }

    // Move between shop tabs
    if settings.show_menu && state.planet_selected.is_some() {
        if mouse.just_pressed(MouseButton::Forward) {
            state.shop = match &state.shop {
                Shop::Buildings => Shop::Fleet,
                Shop::Fleet => Shop::Defenses,
                Shop::Defenses => Shop::Buildings,
            };
        } else if mouse.just_pressed(MouseButton::Back) {
            state.shop = match &state.shop {
                Shop::Buildings => Shop::Defenses,
                Shop::Fleet => Shop::Buildings,
                Shop::Defenses => Shop::Fleet,
            };
        }
    } else if state.mission {
        if mouse.just_pressed(MouseButton::Forward) {
            state.mission_tab = match &state.mission_tab {
                MissionTab::NewMission => MissionTab::ActiveMissions,
                MissionTab::ActiveMissions => MissionTab::IncomingAttacks,
                MissionTab::IncomingAttacks => MissionTab::NewMission,
            };
        } else if mouse.just_pressed(MouseButton::Back) {
            state.mission_tab = match &state.mission_tab {
                MissionTab::NewMission => MissionTab::IncomingAttacks,
                MissionTab::ActiveMissions => MissionTab::NewMission,
                MissionTab::IncomingAttacks => MissionTab::ActiveMissions,
            };
        }
    }
}
