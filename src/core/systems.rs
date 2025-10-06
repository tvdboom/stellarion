use crate::core::game_settings::GameSettings;
use crate::core::player::Player;
use crate::core::states::{AppState, GameState};
use crate::core::ui::utils::TextSize;
use bevy::prelude::*;
use bevy::window::WindowResized;

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
    mut player: ResMut<Player>,
    mut settings: ResMut<GameSettings>,
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
) {
    // Hack to add resources
    if keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        if keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
            if keyboard.just_pressed(KeyCode::ArrowUp) {
                player.resources += 1000usize;
            }
        }
    }

    // Open in-game menu
    if keyboard.just_pressed(KeyCode::Escape) {
        match game_state.get() {
            GameState::Playing => next_game_state.set(GameState::InGameMenu),
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
}
