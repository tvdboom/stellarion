use std::collections::HashMap;

use bevy::prelude::*;
use bevy_renet::renet::RenetServer;
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
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::Unit;

#[derive(Message)]
pub struct StartTurnMsg;

#[derive(Resource, Default)]
pub struct PreviousEndTurnState(bool);

/// Merge missions per objective and return ordered by objective priority
fn regroup_missions(mut missions: Vec<Mission>) -> Vec<Mission> {
    let mut deploy: Option<Mission> = None;
    let mut missile: Option<Mission> = None;
    let mut spy: Option<Mission> = None;
    let mut rest: Option<Mission> = None;

    for m in missions.drain(..) {
        let target = match m.objective {
            Icon::MissileStrike => &mut missile,
            Icon::Spy => &mut spy,
            Icon::Deploy => &mut deploy,
            _ => &mut rest,
        };

        if let Some(t) = target {
            t.merge(m);
        } else {
            *target = Some(m);
        }
    }

    [deploy, missile, spy, rest].into_iter().flatten().collect()
}

/// Check if a mission objective has to change because the destination
/// planet changed owner or was destroyed
fn check_mission(mission: &mut Mission, map: &Map) {
    let destination = map.get(mission.destination);

    // If the destination planet is friendly, the mission changes to deploy
    // (the planet could have been colonized by another mission)
    // Except missile strikes, which always attack the destination planet
    if destination.controlled == Some(mission.owner) && mission.objective != Icon::MissileStrike {
        mission.objective = Icon::Deploy;
    }

    // If deploying to a planet that's no longer under control, convert to attack
    if destination.controlled != Some(mission.owner) && mission.objective == Icon::Deploy {
        mission.objective = Icon::Attack;
    }

    // If going towards a planet that has been destroyed, deploy back to planet of origin,
    // except if conquered, then to the closest owned planet
    if map.get(mission.destination).is_destroyed {
        mission.destination = mission.check_origin(map);
        mission.objective = Icon::Deploy;
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
    settings: Res<Settings>,
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

        // Resolve missions in random player order
        let mut players_shuffled = all_players.clone();
        players_shuffled.shuffle(&mut rng());

        let mut to_drop = vec![];
        for player in players_shuffled {
            let arrived = all_missions
                .iter()
                // Select only arriving missions owned by this player
                .filter(|m| m.owner == player.id && m.turns_to_destination(&map) == 1)
                .cloned()
                // Update mission if the destination planet changed
                .map(|mut m| {
                    check_mission(&mut m, &map);
                    m
                })
                // Filter again since it could have changed after check
                .filter(|m| m.turns_to_destination(&map) == 1)
                .collect::<Vec<_>>();

            // All that arrived can be dropped later from all_missions
            to_drop.extend(arrived.iter().map(|m| m.id));

            // Resolve missions that reached destination
            for mut mission in regroup_missions(arrived) {
                let new_origin = map.get(mission.check_origin(&map)).clone();
                let destination = map.get_mut(mission.destination);

                let report = combat(
                    settings.turn + 1,
                    &mission,
                    destination.owned,
                    destination.army(),
                    &destination.complex,
                );

                all_players
                    .iter_mut()
                    .filter(|p| p.owns(destination) || p.id == report.mission.owner)
                    .for_each(|p| p.reports.push(report.clone()));

                // Send probes back that left combat after one round
                if report.scout_probes > 0 {
                    all_missions.push(Mission::new(
                        mission.owner,
                        destination,
                        &new_origin,
                        Icon::Deploy,
                        HashMap::from([(Unit::Ship(Ship::Probe), report.scout_probes)]),
                        false,
                        false,
                    ));
                }

                if report.winner() == Some(mission.owner) {
                    if report.planet_destroyed {
                        destination.destroy();

                        // Send fleet back
                        all_missions.push(Mission::new(
                            mission.owner,
                            destination,
                            &new_origin,
                            Icon::Deploy,
                            report.surviving_attacker,
                            false,
                            false,
                        ));
                    } else {
                        if report.planet_colonized {
                            *mission.army.entry(Unit::Ship(Ship::ColonyShip)).or_insert(1) -= 1;
                            destination.conquered(mission.owner);

                            // If the planet has no buildings, build a level 1 mine
                            if destination.complex.is_empty() {
                                destination.complex.insert(Building::Mine, 1);
                            }
                        }

                        // Clear all defenders
                        if mission.objective != Icon::Deploy {
                            destination.fleet = HashMap::new();
                            destination.battery = HashMap::new();
                        }

                        // Take control of the planet and dock the surviving fleet
                        destination.controlled = Some(mission.owner);
                        destination.dock(mission.army.clone());
                    }
                } else {
                    // Merge surviving defenders with planet
                    destination.fleet = report
                        .surviving_defense
                        .iter()
                        .filter_map(|(u, v)| {
                            if let Unit::Ship(s) = u {
                                Some((*s, *v))
                            } else {
                                None
                            }
                        })
                        .collect();

                    destination.battery = report
                        .surviving_defense
                        .iter()
                        .filter_map(|(u, v)| {
                            if let Unit::Defense(d) = u {
                                Some((*d, *v))
                            } else {
                                None
                            }
                        })
                        .collect();
                }
            }
        }

        all_missions.retain_mut(|m| {
            // Update mission if destination changed
            check_mission(m, &map);

            !! after the update, a send mission could be returning and arrive that same turn
            !! also change phalanx to scan on distance and not turns remaining
            // Move the mission forward
            m.advance(&map);

            // Remove all missions that were previously resolved
            !to_drop.contains(&m.id)
        });

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
    mut start_turn_msg: MessageReader<StartTurnMsg>,
    mut settings: ResMut<Settings>,
    mut state: ResMut<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    mut message: MessageWriter<MessageMsg>,
) {
    for _ in start_turn_msg.read() {
        settings.turn += 1;
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
                            if report.defender.is_none() {
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
