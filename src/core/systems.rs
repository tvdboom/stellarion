use crate::core::map::map::Map;
use crate::core::menu::utils::TextSize;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::ui::systems::{Shop, UiState};
use crate::core::units::buildings::Building;
use bevy::prelude::*;
use bevy::window::WindowResized;
use std::collections::HashMap;
use strum::IntoEnumIterator;

pub fn on_resize_system(
    mut resize_reader: EventReader<WindowResized>,
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
                if state.mission || state.selected_planet.is_some() {
                    state.mission = false;
                    state.selected_planet = None;
                } else {
                    next_game_state.set(GameState::InGameMenu)
                }
            },
            GameState::InGameMenu => next_game_state.set(GameState::Playing),
            GameState::EndGame => next_app_state.set(AppState::MainMenu),
        }
    }

    // Toggle show planet info
    if keyboard.just_pressed(KeyCode::KeyI) {
        settings.show_info = !settings.show_info;
    }

    // Toggle show hover info
    if keyboard.just_pressed(KeyCode::KeyH) {
        settings.show_hover = !settings.show_hover;
    }

    // Toggle mission panel
    if keyboard.just_pressed(KeyCode::KeyM) {
        state.mission = true;
    }

    // Move between shop tabs
    if state.selected_planet.is_some() {
        if keyboard.just_pressed(KeyCode::Tab) {
            state.shop = match state.shop {
                Shop::Buildings => Shop::Fleet,
                Shop::Fleet => Shop::Defenses,
                Shop::Defenses => Shop::Buildings,
            }
        }
    }
}
