use crate::core::map::map::Map;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::ui::systems::UiState;
use bevy::prelude::*;

#[derive(Event)]
pub struct NextTurnEv;

pub fn next_turn(
    mut next_turn_ev: EventReader<NextTurnEv>,
    mut state: ResMut<UiState>,
    map: Res<Map>,
    mut player: ResMut<Player>,
    mut settings: ResMut<Settings>,
) {
    for _ in next_turn_ev.read() {
        settings.turn += 1;

        let production = player.production(&map.planets);
        player.resources += production;

        *state = UiState::default()
    }
}
