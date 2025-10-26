use bevy::prelude::*;
use bevy_renet::renet::RenetServer;

use crate::core::combat::combat;
use crate::core::map::icon::Icon;
use crate::core::map::map::Map;
use crate::core::messages::MessageMsg;
use crate::core::missions::{Mission, Missions};
use crate::core::network::{ClientMessage, ClientSendMsg, Host, ServerMessage, ServerSendMsg};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::ui::systems::UiState;
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::Unit;

#[derive(Message)]
pub struct StartTurnMsg;

#[derive(Resource, Default)]
pub struct PreviousEndTurnState(bool);

pub fn check_turn_ended(
    state: Res<UiState>,
    mut prev_state: ResMut<PreviousEndTurnState>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    mut client_send_msg: MessageWriter<ClientSendMsg>,
) {
    if prev_state.0 != state.end_turn {
        client_send_msg.write(ClientSendMsg::new(ClientMessage::EndTurn {
            end_turn: state.end_turn,
            map: map.clone(),
            player: player.clone(),
            missions: missions.clone(),
        }));

        prev_state.0 = state.end_turn;
    }
}

pub fn resolve_turn(
    mut host: ResMut<Host>,
    server: Option<ResMut<RenetServer>>,
    state: Res<UiState>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut missions: ResMut<Missions>,
    mut server_send_msg: MessageWriter<ServerSendMsg>,
    mut start_turn_msg: MessageWriter<StartTurnMsg>,
) {
    if state.end_turn && host.turn_ended.len() == server.map(|s| s.clients_id().len()).unwrap_or(0)
    {
        // Apply purchases and reset jump gates
        map.planets.iter_mut().for_each(|p| {
            p.produce();
            p.jump_gate = 0;
        });

        // Collect all players and missions
        let mut all_players = std::iter::once(player.clone())
            .chain(host.clients.values().cloned())
            .collect::<Vec<_>>();

        let mut all_missions =
            missions.0.iter().cloned().chain(host.missions.values().cloned()).collect::<Vec<_>>();

        // Produce resources
        for player in &mut all_players {
            let production = player.resource_production(&map.planets);
            player.resources += production;
        }

        // Resolve missions
        let mut to_add = vec![];
        let mut to_drop = vec![];
        for mission in &mut all_missions {
            mission.advance(&map);

            let has_reached = mission.has_reached_destination(&map);

            let origin = map.get(mission.origin).clone();
            let destination = map.get_mut(mission.destination);

            // If the destination planet is friendly, the mission changes to deploy
            // (the planet could have been colonized by another mission)
            // Except missile strikes, which always attack the destination planet
            if destination.controlled == Some(mission.owner)
                && mission.objective != Icon::MissileStrike
            {
                mission.objective = Icon::Deploy;
            }

            if has_reached {
                let report = combat(
                    mission,
                    destination.owned,
                    destination.army(),
                    destination.get(&Unit::Building(Building::PlanetaryShield)),
                );

                all_players
                    .iter_mut()
                    .filter(|p| p.owns(destination) || report.winner() == Some(p.id))
                    .for_each(|p| p.reports.push(report.clone()));

                if report.winner() == Some(mission.owner) {
                    if report.planet_destroyed {
                        destination.destroy();

                        // Send fleet back to planet of origin
                        // Start a bit outside the origin planet to be able to see the image
                        to_add.push(Mission::new(
                            mission.owner,
                            destination,
                            &origin,
                            Icon::Deploy,
                            report.surviving_attacker,
                            false,
                            false,
                        ));
                    } else {
                        if mission.objective == Icon::Colonize {
                            *mission.army.entry(Unit::Ship(Ship::ColonyShip)).or_insert(1) -= 1;
                            destination.conquered(mission.owner);

                            // If the planet has no buildings, build a level 1 mine
                            if destination.complex.is_empty() {
                                destination.complex.insert(Building::Mine, 1);
                            }
                        }

                        // Take control of the planet and dock the surviving fleet
                        destination.controlled = Some(mission.owner);
                        destination.dock(mission.army.clone());
                    }
                }

                to_drop.push(mission.id);
            }
        }

        // Remove missions that reached the destination
        all_missions.retain(|m| !to_drop.contains(&m.id));

        // Add new missions to existing list
        all_missions.extend(to_add);

        // Select the missions every player is able to see
        let filter_missions = |missions: &Vec<Mission>, player: &Player| {
            missions
                .iter()
                .filter(|m| {
                    let destination = map.get(m.destination);
                    let phalanx = destination.get(&Unit::Building(Building::SensorPhalanx));
                    let distance = m.turns_to_destination(&map);
                    m.owner == player.id
                        || (player.owns(destination)
                            && phalanx >= distance
                            && m.objective != Icon::Spy)
                })
                .cloned()
                .collect::<Vec<_>>()
        };

        for p in all_players {
            // Update the host
            if p.id == 0 {
                missions.0 = filter_missions(&all_missions, &p);
                *player = p;
            } else {
                // Update the clients
                server_send_msg.write(ServerSendMsg::new(
                    ServerMessage::StartTurn {
                        map: map.clone(),
                        player: p.clone(),
                        missions: Missions(filter_missions(&all_missions, &p)),
                    },
                    Some(p.id),
                ));
            }
        }

        host.turn_ended.clear();
        start_turn_msg.write(StartTurnMsg);
    }
}

pub fn start_turn(
    mut commands: Commands,
    mut start_turn_msg: MessageReader<StartTurnMsg>,
    mut settings: ResMut<Settings>,
    mut message: MessageWriter<MessageMsg>,
) {
    for _ in start_turn_msg.read() {
        settings.turn += 1;
        commands.insert_resource(UiState::default());

        message.write(MessageMsg::info(format!("Turn {} started.", settings.turn)));
    }
}
