use bevy::prelude::*;
use bevy_renet::renet::RenetServer;

use crate::core::map::icon::Icon;
use crate::core::map::map::Map;
use crate::core::messages::MessageMsg;
use crate::core::network::{Host, ServerMessage, ServerSendMsg};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::ui::systems::UiState;
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::Unit;

#[derive(Message)]
pub struct StartTurnMsg;

pub fn check_turn(
    mut host: ResMut<Host>,
    server: Option<ResMut<RenetServer>>,
    state: Res<UiState>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut server_send_msg: MessageWriter<ServerSendMsg>,
    mut start_turn_msg: MessageWriter<StartTurnMsg>,
) {
    if state.end_turn && host.turn_ended.len() == server.map(|s| s.clients_id().len()).unwrap_or(0)
    {
        // Produce resources
        let production = player.resource_production(&map.planets);
        player.resources += production;

        // Apply purchases
        map.planets.iter_mut().for_each(|p| p.produce());

        // Resolve missions
        let id = player.id;
        player.missions.retain_mut(|mission| {
            mission.advance(&map);

            let has_reached = mission.has_reached_destination(&map);

            let destination = map.get_mut(mission.destination);

            // If the destination planet is friendly, the mission changes to deploy
            // (the planet could have been colonized by another mission)
            // Except missile strikes, which always attack the destination planet
            if destination.owned == Some(id) && mission.objective != Icon::MissileStrike {
                mission.objective = Icon::Deploy;
            }

            if has_reached {
                match mission.objective {
                    Icon::Colonize => {
                        *mission.army.entry(Unit::Ship(Ship::ColonyShip)).or_insert(1) -= 1;
                        destination.conquered(id);

                        // If the planet has no buildings, build a level 1 mine
                        if destination.complex.is_empty() {
                            destination.complex.insert(Building::Mine, 1);
                        }
                    },
                    _ => (),
                }

                // Take control of the planet and dock the surviving fleet
                destination.controlled = Some(id);
                destination.dock(mission.army.clone());

                false
            } else {
                true
            }
        });

        for (id, player) in host.clients.iter() {
            server_send_msg.write(ServerSendMsg::new(
                ServerMessage::StartTurn(player.clone()),
                Some(id.clone()),
            ));
        }

        host.turn_ended.clear();
        start_turn_msg.write(StartTurnMsg);
    }
}

pub fn start_turn_message(
    mut start_turn_msg: MessageReader<StartTurnMsg>,
    mut settings: ResMut<Settings>,
    mut state: ResMut<UiState>,
    mut message: MessageWriter<MessageMsg>,
) {
    for _ in start_turn_msg.read() {
        settings.turn += 1;
        *state = UiState::default();

        message.write(MessageMsg::info(format!("Turn {} started.", settings.turn)));
    }
}
