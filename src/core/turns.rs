use std::collections::HashMap;

use bevy::prelude::*;
use bevy_renet::renet::RenetServer;
use itertools::Itertools;
use rand::rng;
use rand::seq::SliceRandom;

use crate::core::combat::combat;
use crate::core::map::icon::Icon;
use crate::core::map::map::Map;
use crate::core::messages::MessageMsg;
use crate::core::missions::{Mission, Missions};
use crate::core::network::{ClientMessage, ClientSendMsg, Host, ServerMessage, ServerSendMsg};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Unit};
use crate::utils::NameFromEnum;

#[derive(Message)]
pub struct StartTurnMsg;

#[derive(Resource, Default)]
pub struct PreviousEndTurnState(bool);

/// Merge missions per objective and return ordered by objective priority
fn regroup_missions(missions: &Vec<Mission>) -> Vec<Mission> {
    let mut deploy: Option<Mission> = None;
    let mut missile: Option<Mission> = None;
    let mut spy: Option<Mission> = None;
    let mut rest: Option<Mission> = None;

    for m in missions.iter() {
        let target = match m.objective {
            Icon::MissileStrike => &mut missile,
            Icon::Spy => &mut spy,
            Icon::Deploy => &mut deploy,
            _ => &mut rest,
        };

        if let Some(t) = target {
            t.merge(m);
        } else {
            *target = Some(m.clone());
        }
    }

    [deploy, missile, spy, rest].into_iter().flatten().collect()
}

/// Check if a mission objective has to change because the destination
/// planet changed owner or was destroyed
fn check_mission(mission: &mut Mission, map: &Map, turn: usize) {
    let destination = map.get(mission.destination);

    // If the destination planet is friendly, the mission changes to deploy
    // (the planet could have been colonized by another mission)
    // Except missile strikes, which always attack the destination planet
    if destination.controlled() == Some(mission.owner)
        && !matches!(mission.objective, Icon::Deploy | Icon::MissileStrike)
    {
        mission.objective = Icon::Deploy;
        mission.logs.push_str(
            format!("\n- ({}) Objective changed to {}.", turn, Icon::Deploy.to_name()).as_str(),
        );
    }

    // If deploying to a planet that's no longer under control, convert to attack
    if destination.controlled() != Some(mission.owner) && mission.objective == Icon::Deploy {
        mission.objective = Icon::Attack;
        mission.logs.push_str(
            format!("\n- ({}) Objective changed to {}.", turn, Icon::Attack.to_name()).as_str(),
        );
    }

    // If going towards a planet that has been destroyed, deploy back to planet of origin,
    // except if conquered, then to the closest owned planet
    if destination.is_destroyed {
        mission.destination = mission.check_origin(map);
        mission.objective = Icon::Deploy;
        mission.logs.push_str(
            format!(
                "\n- ({}) Destination changed to planet {}.",
                turn,
                map.get(mission.destination).name
            )
            .as_str(),
        );
        mission.logs.push_str(
            format!("\n- ({}) Objective changed to {}.", turn, Icon::Deploy.to_name()).as_str(),
        );
    }
}

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
    mut settings: ResMut<Settings>,
    state: Res<UiState>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut missions: ResMut<Missions>,
    mut server_send_msg: MessageWriter<ServerSendMsg>,
    mut start_turn_msg: MessageWriter<StartTurnMsg>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    let n_clients = server.map(|s| s.clients_id().len()).unwrap_or(0);
    if state.end_turn && host.turn_ended.len() == n_clients {
        settings.turn += 1;

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

        // Resolve missions in random player order
        let mut players_shuffled = all_players.clone();
        players_shuffled.shuffle(&mut rng());

        let mut new_missions = vec![];
        for player in players_shuffled {
            // We loop since a player can change a destination which affects other of its own missions
            loop {
                // Select only arriving missions owned by this player
                let arrived = all_missions
                    .iter()
                    .filter(|m| m.owner == player.id && m.turns_to_destination(&map) < 2)
                    .cloned()
                    .collect::<Vec<_>>();

                if arrived.is_empty() {
                    break; // No more missions to check for this player
                }

                // Resolve missions that reached destination
                for mut mission in regroup_missions(&arrived) {
                    let new_origin = map.get(mission.check_origin(&map)).clone();
                    let destination = map.get_mut(mission.destination);

                    let mut report = combat(settings.turn, &mission, destination);

                    report.mission.logs.push_str(
                        format!("\n- ({}) Mission arrived in {}.", settings.turn, destination.name)
                            .as_str(),
                    );

                    if report.scout_probes > 0 {
                        if mission.objective == Icon::Spy {
                            report.mission.logs.push_str(
                                format!(
                                    "\n- ({}) Spied on planet {}.",
                                    settings.turn, destination.name
                                )
                                .as_str(),
                            );

                            let mut return_m = Mission {
                                id: rand::random(),
                                destination: new_origin.id,
                                objective: Icon::Deploy,
                                ..report.mission.clone()
                            };
                            return_m.logs.push_str(
                                format!(
                                    "\n- ({}) Returning to planet {}.",
                                    settings.turn, new_origin.name
                                )
                                .as_str(),
                            );
                            new_missions.push(return_m);
                        } else {
                            // Send probes back that left combat after one round
                            new_missions.push(Mission::new(
                                settings.turn,
                                mission.owner,
                                destination,
                                &new_origin,
                                Icon::Deploy,
                                HashMap::from([(Unit::Ship(Ship::Probe), report.scout_probes)]),
                                false,
                                false,
                            ));
                        }
                    }

                    if report.winner() == Some(mission.owner) {
                        if report.mission.objective == Icon::Destroy {
                            if report.planet_destroyed {
                                destination.destroy();
                                report.mission.logs.push_str(
                                    format!(
                                        "\n- ({}) Planet {} destroyed.",
                                        settings.turn, destination.name
                                    )
                                    .as_str(),
                                );
                            } else {
                                report.mission.logs.push_str(
                                    format!(
                                        "\n- ({}) Failed to destroy planet {}.",
                                        settings.turn, destination.name
                                    )
                                    .as_str(),
                                );
                            }

                            let mut return_m = Mission {
                                id: rand::random(),
                                destination: new_origin.id,
                                objective: Icon::Deploy,
                                ..report.mission.clone()
                            };
                            return_m.logs.push_str(
                                format!(
                                    "\n- ({}) Returning to planet {}.",
                                    settings.turn, new_origin.name
                                )
                                .as_str(),
                            );
                            new_missions.push(return_m);
                        } else {
                            if report.planet_colonized {
                                *mission.army.entry(Unit::colony_ship()).or_insert(1) -= 1;
                                destination.colonize(mission.owner);

                                report.mission.logs.push_str(
                                    format!(
                                        "\n- ({}) Planet {} colonized.",
                                        settings.turn, destination.name
                                    )
                                    .as_str(),
                                );

                                // If the planet has no buildings, build a level 1 mine
                                if !destination.has_buildings() {
                                    destination.army.insert(Unit::Building(Building::Mine), 1);
                                }
                            }

                            // Clear defenders from planet
                            if mission.objective != Icon::Deploy {
                                destination.army.retain(|u, _| u.is_building());
                            }

                            // Take control of the planet and dock the surviving fleet
                            destination.control(mission.owner);
                            destination.dock(mission.army.clone());
                        }
                    } else {
                        // Merge surviving defenders with planet
                        destination.army = report.surviving_defender.clone();
                    }

                    // Update the ownership in the report
                    report.destination_owned = destination.owned;

                    // Attach mission report to relevant players
                    all_players
                        .iter_mut()
                        .filter(|p| {
                            report.planet.controlled == Some(p.id) || report.mission.owner == p.id
                        })
                        .for_each(|p| p.reports.push(report.clone()));
                }

                // Update all missions whose destination changed
                all_missions.retain_mut(|m| {
                    check_mission(m, &map, settings.turn);
                    !arrived.iter().map(|m| m.id).contains(&m.id)
                });
            }
        }

        // After all missions that arrived have been resolved, advance all remaining missions
        // and add the new missions
        all_missions.iter_mut().for_each(|m| m.advance(&map));
        all_missions.extend(new_missions);

        // Select the missions every player is able to see
        let filter_missions = |missions: &Vec<Mission>, player: &Player| {
            missions
                .iter()
                .filter(|m| {
                    let destination = map.get(m.destination);
                    let phalanx = destination.army.amount(&Unit::Building(Building::SensorPhalanx));
                    m.owner == player.id
                        || (player.owns(destination)
                            && 0.6 * phalanx as f32 >= m.distance(&map)
                            && m.objective != Icon::Spy)
                })
                .cloned()
                .collect::<Vec<_>>()
        };

        // Update which players lost the game
        let n_lost = all_players
            .iter_mut()
            .map(|p| {
                p.spectator = !p.owns(map.get(p.home_planet));
                p
            })
            .filter(|p| p.spectator)
            .count();

        for p in all_players {
            // Update the host
            if p.id == 0 {
                missions.0 = filter_missions(&all_missions, &p);
                *player = p;
            } else {
                // Update the clients
                server_send_msg.write(ServerSendMsg::new(
                    ServerMessage::StartTurn {
                        turn: settings.turn,
                        map: map.clone(),
                        player: p.clone(),
                        missions: Missions(filter_missions(&all_missions, &p)),
                        end_game: p.spectator || (n_lost == n_clients && n_clients > 0),
                    },
                    Some(p.id),
                ));
            }
        }

        host.turn_ended.clear();
        host.received.clear();

        if player.spectator || (n_lost == n_clients && n_clients > 0) {
            player.spectator = true;
            next_game_state.set(GameState::EndGame);
        } else {
            start_turn_msg.write(StartTurnMsg);
        }
    }
}

pub fn start_turn(
    mut start_turn_msg: MessageReader<StartTurnMsg>,
    settings: Res<Settings>,
    mut state: ResMut<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    mut message: MessageWriter<MessageMsg>,
) {
    for _ in start_turn_msg.read() {
        *state = UiState {
            mission_report: state.mission_report,
            ..default()
        };

        message.write(MessageMsg::info(format!("Turn {} started.", settings.turn)));

        let new_reports =
            player.reports.iter().filter(|r| r.turn == settings.turn).collect::<Vec<_>>();
        if !new_reports.is_empty() {
            for report in &new_reports {
                let destination = map.get(report.mission.destination);

                match report.mission.objective {
                    Icon::Deploy => {
                        message.write(MessageMsg::info(format!(
                            "Deployed fleet to planet {}.",
                            destination.name
                        )));
                    },
                    Icon::Colonize if report.planet_colonized => {
                        if report.mission.owner == player.id {
                            if !report.planet.has_buildings() {
                                message.write(MessageMsg::info(format!(
                                    "Planet {} has been colonized.",
                                    destination.name
                                )));
                            } else {
                                message.write(MessageMsg::info(format!(
                                    "Planet {} has been conquered.",
                                    destination.name
                                )));
                            }
                        } else {
                            message.write(MessageMsg::warning(format!(
                                "Planet {} has been conquered by the enemy.",
                                destination.name
                            )));
                        }
                    },
                    Icon::Spy => {
                        if report.mission.owner == player.id {
                            if report.scout_probes > 0 {
                                message.write(MessageMsg::info(format!(
                                    "Successful spy mission on planet {}.",
                                    destination.name
                                )));
                            } else {
                                message.write(MessageMsg::warning(format!(
                                    "All probes lost while spying planet {}.",
                                    destination.name
                                )));
                            }
                        } else {
                            message.write(MessageMsg::warning(format!(
                                "Enemy Probes have been signaled around planet {}.",
                                destination.name
                            )));
                        }
                    },
                    Icon::MissileStrike => {
                        if report.mission.owner == player.id {
                            message.write(MessageMsg::info(format!(
                                "Successful missile strike on planet {}.",
                                destination.name
                            )));
                        } else {
                            message.write(MessageMsg::warning(format!(
                                "Planet {} has been hit by a missile strike.",
                                destination.name
                            )));
                        }
                    },
                    Icon::Destroy if report.planet_destroyed => {
                        message.write(MessageMsg::warning(format!(
                            "Planet {} has been destroyed.",
                            destination.name
                        )));
                    },
                    _ => {
                        if report.winner() == Some(player.id) {
                            message.write(MessageMsg::info(format!(
                                "Battle won at planet {}.",
                                destination.name
                            )));
                        } else {
                            message.write(MessageMsg::warning(format!(
                                "Battle lost at planet {}.",
                                destination.name
                            )));
                        }
                    },
                }
            }

            state.mission = true;
            state.mission_tab = MissionTab::MissionReports;
            state.mission_report = Some(player.reports.last().unwrap().mission.id);
        }
    }
}
