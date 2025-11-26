use std::collections::HashMap;

use bevy::prelude::*;
use bevy_renet::renet::RenetServer;
use itertools::Itertools;
use rand::rng;
use rand::seq::SliceRandom;

use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::combat::combat::resolve_combat;
use crate::core::combat::report::Side;
use crate::core::constants::{EXPLOSION_Z, PHALANX_DISTANCE, RADAR_DISTANCE};
use crate::core::map::icon::Icon;
use crate::core::map::map::Map;
use crate::core::map::planet::Planet;
use crate::core::map::systems::{ExplosionCmp, PlanetCmp};
use crate::core::messages::MessageMsg;
use crate::core::missions::{BombingRaid, Mission, Missions};
use crate::core::network::{ClientMessage, ClientSendMsg, Host, ServerMessage, ServerSendMsg};
use crate::core::persistence::SaveGameMsg;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Unit};
use crate::utils::NameFromEnum;

#[derive(Message)]
pub struct StartTurnMsg {
    pub skip_battle: bool,
    pub skip_end_game: bool,
}

impl StartTurnMsg {
    pub fn new(skip_battle: bool, skip_end_game: bool) -> Self {
        Self {
            skip_battle,
            skip_end_game,
        }
    }
}

#[derive(Resource)]
pub struct PreviousEndTurnState(bool);

impl Default for PreviousEndTurnState {
    /// Start on true to immediately trigger a message to the host when starting the game
    fn default() -> Self {
        PreviousEndTurnState(true)
    }
}

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
fn check_mission(mission: &mut Mission, map: &Map, turn: usize, settings: &Settings) {
    let old_objective = mission.objective;
    let destination = map.get(mission.destination);

    // If the destination planet is friendly, the mission changes to deploy
    // (the planet could have been colonized by another mission)
    // Except missile strikes, which always attack the destination planet
    if (destination.controlled == Some(mission.owner)
        && !matches!(mission.objective, Icon::Deploy | Icon::MissileStrike | Icon::Colonize))
        || (destination.owned == Some(mission.owner) && mission.objective == Icon::Colonize)
    {
        mission.objective = Icon::Deploy;
    }

    // If deploying to a planet that's no longer under control, convert to attack
    if destination.controlled != Some(mission.owner) && mission.objective == Icon::Deploy {
        mission.objective = Icon::Attack;
    }

    // If colonizing and the max. number of planets colonized is reached, change to deploy or attack
    let n_owned = map.planets.iter().filter(|p| p.owned == Some(mission.owner)).count();
    let n_max_owned =
        (map.planets.len() as f32 * settings.p_colonizable as f32 / 100.).ceil() as usize;
    if mission.objective == Icon::Colonize && n_owned >= n_max_owned {
        mission.objective = if destination.controlled != Some(mission.owner) {
            Icon::Attack
        } else {
            Icon::Deploy
        };
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
    }

    if old_objective != mission.objective {
        mission.logs.push_str(
            format!("\n- ({}) Objective changed to {}.", turn, mission.objective.to_name())
                .as_str(),
        );
    }
}

/// Select the missions a player is able to see
pub fn filter_missions(missions: &Vec<Mission>, map: &Map, player: &Player) -> Vec<Mission> {
    missions
        .iter()
        .filter(|m| {
            let destination = map.get(m.destination);
            let phalanx = destination.army.amount(&Unit::Building(Building::SensorPhalanx));
            m.owner == player.id
                || (player.owns(destination)
                    && PHALANX_DISTANCE * phalanx as f32 * Planet::SIZE + destination.size() * 0.5
                        >= destination.position.distance(m.position)
                    && !m.objective.is_hidden())
                || map.moons().into_iter().any(|moon| {
                    player.controls(moon)
                        && RADAR_DISTANCE
                            * moon.army.amount(&Unit::Building(Building::OrbitalRadar)) as f32
                            * Planet::SIZE
                            + moon.size() * 0.5
                            >= moon.position.distance(m.position)
                })
        })
        .cloned()
        .collect::<Vec<_>>()
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
) {
    // Collect all players and missions
    let mut all_players =
        std::iter::once(player.clone()).chain(host.clients.values().cloned()).collect::<Vec<_>>();

    let mut all_missions = missions
        .iter()
        .filter(|m| m.owner == player.id)
        .chain(host.missions.iter())
        .cloned()
        .collect::<Vec<_>>();

    let n_playing = all_players.iter().filter(|p| !p.spectator).count();
    let n_clients = server.map(|s| s.clients_id().len()).unwrap_or(0);

    if (state.end_turn || player.spectator) && host.turn_ended.len() == n_playing.saturating_sub(1)
    {
        settings.turn += 1;

        // Apply purchases and reset jump gates
        map.planets.iter_mut().for_each(|p| {
            p.produce();
            p.jump_gate = 0;
        });

        // Produce resources
        for player in &mut all_players {
            let production = player.resource_production(&map.planets);
            player.resources += production;
        }

        // Resolve missions in random player order
        let mut players_shuffled = all_players.clone();
        players_shuffled.shuffle(&mut rng());

        let planet_ids = map.planets.iter().map(|p| p.id).collect::<Vec<_>>();

        let mut new_missions = vec![];
        for player in players_shuffled {
            for planet_id in &planet_ids {
                // We loop since a player can change a destination which affects other of its own missions
                loop {
                    // Select only arriving missions owned by this player
                    let arrived = all_missions
                        .iter()
                        .filter(|m| {
                            m.owner == player.id
                                && m.destination == *planet_id
                                && m.turns_to_destination(&map) < 2
                        })
                        .cloned()
                        .collect::<Vec<_>>();

                    if arrived.is_empty() {
                        break; // No more missions to check for this player
                    }

                    // Resolve missions that reached destination
                    for mut mission in regroup_missions(&arrived) {
                        let new_origin = map.get(mission.check_origin(&map)).clone();
                        let destination = map.get_mut(mission.destination);

                        let mut report = resolve_combat(settings.turn, &mission, destination);

                        report.mission.logs.push_str(
                            format!(
                                "\n- ({}) Mission arrived in {}.",
                                settings.turn, destination.name
                            )
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

                                new_missions.push(Mission::new(
                                    settings.turn,
                                    report.mission.owner,
                                    destination,
                                    &new_origin,
                                    Icon::Deploy,
                                    report.surviving_attacker.clone(),
                                    BombingRaid::None,
                                    false,
                                    false,
                                    Some(
                                        report.mission.logs.clone()
                                            + format!(
                                                "\n- ({}) Returning to planet {}.",
                                                settings.turn, new_origin.name
                                            )
                                            .as_str(),
                                    ),
                                ));
                            } else {
                                // Send probes back that left combat after one round
                                new_missions.push(Mission::new(
                                    settings.turn,
                                    mission.owner,
                                    destination,
                                    &new_origin,
                                    Icon::Deploy,
                                    HashMap::from([(Unit::probe(), report.scout_probes)]),
                                    BombingRaid::None,
                                    false,
                                    false,
                                    None,
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

                                new_missions.push(Mission::new(
                                    settings.turn,
                                    report.mission.owner,
                                    destination,
                                    &new_origin,
                                    Icon::Deploy,
                                    report.surviving_attacker.clone(),
                                    BombingRaid::None,
                                    false,
                                    false,
                                    Some(
                                        report.mission.logs.clone()
                                            + format!(
                                                "\n- ({}) Returning to planet {}.",
                                                settings.turn, new_origin.name
                                            )
                                            .as_str(),
                                    ),
                                ));
                            } else if report.planet_colonized {
                                *mission.army.entry(Unit::colony_ship()).or_insert(1) -= 1;
                                destination.colonize(mission.owner);

                                report.mission.logs.push_str(
                                    format!(
                                        "\n- ({}) Planet {} colonized.",
                                        settings.turn, destination.name
                                    )
                                    .as_str(),
                                );

                                // If the planet has no buildings, build level 1 resource buildings
                                if !destination.has_buildings() {
                                    destination.army.insert(Unit::Building(Building::MetalMine), 1);
                                    destination
                                        .army
                                        .insert(Unit::Building(Building::CrystalMine), 1);
                                    destination
                                        .army
                                        .insert(Unit::Building(Building::DeuteriumSynthesizer), 1);
                                }
                            }

                            // Clear defenders from planet
                            if !(mission.objective == Icon::Deploy
                                || (mission.objective == Icon::Colonize
                                    && destination.controlled == Some(mission.owner)))
                            {
                                destination.army.retain(|u, _| u.is_building());
                            }

                            // Take control of the planet and dock the surviving fleet
                            if mission.objective != Icon::Destroy {
                                destination.control(mission.owner);
                                destination.dock(mission.army.clone());
                            }
                        } else {
                            // Merge surviving defenders with planet
                            destination.army = report.surviving_defender.clone();
                        }

                        // Update the ownership in the report
                        report.destination_owned = destination.owned;
                        report.destination_controlled = destination.controlled;

                        // Attach mission report to relevant players
                        all_players
                            .iter_mut()
                            .filter(|p| {
                                report.planet.controlled == Some(p.id)
                                    || report.mission.owner == p.id
                            })
                            .for_each(|p| p.reports.push(report.clone()));
                    }

                    // Update all missions whose destination changed
                    all_missions.retain_mut(|mission| {
                        check_mission(mission, &map, settings.turn, &settings);
                        !arrived.iter().map(|m| m.id).contains(&mission.id)
                    });
                }
            }
        }

        // After all missions that arrived have been resolved, advance all remaining missions
        // and add the new missions
        all_missions.iter_mut().for_each(|m| m.advance(&map));
        all_missions.extend(new_missions);

        // Reset missions in the host
        host.missions = vec![];

        // Update which players lost the game
        let n_lost = all_players
            .iter_mut()
            .map(|p| {
                p.spectator = !p.owns(map.get(p.home_planet));
                p
            })
            .filter(|p| p.spectator)
            .count();

        // If there are still players playing, cleanup resources from players that lost
        let playing = all_players.iter().filter(|p| !p.spectator).count();
        if playing >= 2 {
            // Remove all units, buys and missions from this player
            all_players.iter_mut().filter(|p| p.spectator).for_each(|p| {
                map.planets.iter_mut().filter(|pl| pl.controlled == Some(p.id)).for_each(|p| {
                    p.clean();
                });
                all_missions.retain(|m| m.owner != p.id);
            });
        }

        for p in &mut all_players {
            // Update spectator if the player is the winner
            p.spectator = p.spectator || (n_lost == n_clients && n_clients > 0);

            let new_missions = if p.spectator {
                all_missions.clone()
            } else {
                filter_missions(&all_missions, &map, &p)
            };

            if p.id == 0 {
                // Update the host
                *player = p.clone();
                missions.0 = new_missions;
            } else {
                // Update the host resource
                host.clients.get_mut(&p.id).map(|pl| *pl = p.clone());

                // Update the clients
                server_send_msg.write(ServerSendMsg::new(
                    ServerMessage::StartTurn {
                        turn: settings.turn,
                        map: map.clone(),
                        player: p.clone(),
                        missions: Missions(new_missions),
                    },
                    Some(p.id),
                ));
            }
        }

        let spectators =
            all_players.iter().filter_map(|p| p.spectator.then_some(p.id)).collect::<Vec<_>>();
        host.turn_ended.retain(|id| spectators.contains(id));
        host.received.retain(|id| spectators.contains(id));

        start_turn_msg.write(StartTurnMsg::new(false, false));
    }
}

pub fn start_turn(
    mut commands: Commands,
    mut start_turn_msg: MessageReader<StartTurnMsg>,
    planet_q: Query<(&Transform, &PlanetCmp)>,
    settings: Res<Settings>,
    mut state: ResMut<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    mut play_audio_ev: MessageWriter<PlayAudioMsg>,
    mut message: MessageWriter<MessageMsg>,
    mut save_game_ev: MessageWriter<SaveGameMsg>,
    mut next_game_state: ResMut<NextState<GameState>>,
    assets: Local<WorldAssets>,
) {
    for msg in start_turn_msg.read() {
        *state = UiState {
            lab: state.lab,
            mission_report: state.mission_report,
            ..default()
        };

        let new_reports = player
            .reports
            .iter()
            .filter(|r| r.turn == settings.turn && !r.hidden)
            .collect::<Vec<_>>();

        if new_reports
            .iter()
            .any(|r| r.combat_report.is_some() && r.can_see(&Side::Defender, player.id))
        {
            if !msg.skip_battle {
                next_game_state.set(GameState::InCombat);
                break;
            } else if !msg.skip_end_game && player.spectator {
                next_game_state.set(GameState::EndGame);
                break;
            }
        }

        if settings.autosave {
            save_game_ev.write(SaveGameMsg(true));
        }

        message.write(MessageMsg::info(format!("Turn {} started.", settings.turn)));

        // Spawn explosion animation for newly destroyed planets
        map.planets.iter().filter(|p| p.is_destroyed && p.image != 0).for_each(|p| {
            let (planet_t, _) = planet_q.iter().find(|(_, pc)| pc.id == p.id).unwrap();

            let texture = assets.texture("explosion");
            commands.spawn((
                Sprite {
                    image: texture.image,
                    texture_atlas: Some(texture.atlas),
                    custom_size: Some(Vec2::splat(1.5 * p.size())),
                    ..default()
                },
                Transform::from_xyz(planet_t.translation.x, planet_t.translation.y, EXPLOSION_Z),
                ExplosionCmp {
                    timer: Timer::from_seconds(0.1, TimerMode::Repeating),
                    last_index: texture.last_index,
                    planet: p.id,
                },
            ));

            play_audio_ev.write(PlayAudioMsg::new("explosion"));
        });

        if !new_reports.is_empty() {
            for report in &new_reports {
                let origin = map.get(report.mission.origin);
                let destination = map.get(report.mission.destination);

                match report.mission.objective {
                    Icon::Deploy if report.mission.origin_controlled != Some(player.id) => {
                        if report.mission.army.len() == 1
                            && report.mission.army.contains_key(&Unit::probe())
                        {
                            message.write(MessageMsg::info(format!(
                                "Probes returned from planet {}.",
                                origin.name
                            )));
                        } else {
                            message.write(MessageMsg::info(format!(
                                "Fleet returned from planet {}.",
                                origin.name
                            )));
                        }
                    },
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

            state.mission_tab = MissionTab::MissionReports;
            state.mission_report = Some(player.reports.last().unwrap().mission.id);
        }
    }
}
