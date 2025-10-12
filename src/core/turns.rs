use crate::core::map::map::Map;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::ui::systems::UiState;
use bevy::prelude::*;

#[derive(Message)]
pub struct NextTurnMsg;

pub fn next_turn(
    mut next_turn_ev: MessageReader<NextTurnMsg>,
    mut state: ResMut<UiState>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut settings: ResMut<Settings>,
) {
    for _ in next_turn_ev.read() {
        settings.turn += 1;

        // Produce resources
        let production = player.resource_production(&map.planets);
        player.resources += production;

        // Apply purchases
        map.planets.iter_mut().for_each(|p| p.produce());

        *state = UiState::default()
    }
}
