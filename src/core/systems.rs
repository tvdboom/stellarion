use crate::core::game_settings::GameSettings;
use crate::core::map::map::Map;
use crate::core::player::Players;
use crate::core::ui::utils::TextSize;
use bevy::prelude::*;
use bevy::window::WindowResized;

pub fn initialize_game(mut commands: Commands, game_settings: Res<GameSettings>) {
    commands.insert_resource(Map::new(game_settings.n_planets));
}

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

pub fn check_keys(keyboard: Res<ButtonInput<KeyCode>>, mut players: ResMut<Players>) {
    let player = players.main_mut();

    if keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]) {
        if keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]) {
            if keyboard.just_pressed(KeyCode::ArrowUp) {
                player.resources += 1e4;
            }
        }
    }
}
