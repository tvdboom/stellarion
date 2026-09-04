//! Bevy turn-boundary presentation and submission adapter around the deterministic core.

use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::combat::report::{MissionReport, Side};
use crate::core::constants::EXPLOSION_Z;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::systems::{ExplosionCmp, PlanetCmp};
use crate::core::messages::{MessageAction, MessageMsg};
use crate::core::missions::Mission;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::Unit;
use crate::multiplayer::client::{MultiplayerRequest, PendingTurnCommands, SubmissionState};

/// Requests presentation work after a new canonical turn is installed.
#[derive(Message)]
pub struct StartTurnMsg {
    /// Suppresses combat playback when loading/resuming an existing turn.
    pub skip_battle: bool,
    /// Suppresses the end-game overlay when loading/resuming an existing turn.
    pub skip_end_game: bool,
}

impl StartTurnMsg {
    /// Creates a presentation request with explicit combat/end-game suppression.
    pub fn new(skip_battle: bool, skip_end_game: bool) -> Self {
        Self {
            skip_battle,
            skip_end_game,
        }
    }
}

/// Selects only missions visible to one player for the ECS/rendering projection.
pub fn filter_missions(missions: &[Mission], map: &Map, player: &Player) -> Vec<Mission> {
    missions
        .iter()
        .filter(|mission| {
            mission.owner == player.id
                || mission.is_seen_by_phalanx(map, player).is_some()
                || mission.is_seen_by_radar(map, player).is_some()
        })
        .cloned()
        .collect()
}

/// Toggles readiness; orders become final only when every player has finished.
pub fn check_turn_ended(
    mut state: ResMut<UiState>,
    mut pending: ResMut<PendingTurnCommands>,
    mut requests: MessageWriter<MultiplayerRequest>,
) {
    if std::mem::take(&mut state.end_turn) {
        if matches!(pending.submission, SubmissionState::Draft | SubmissionState::Retry) {
            requests.write(MultiplayerRequest::SubmitTurn);
        } else {
            pending.request_resume();
        }
    }
}

fn report_notification(
    report: &MissionReport,
    player: &Player,
    origin: &Planet,
    destination: &Planet,
) -> MessageMsg {
    let notification = match report.mission.objective {
        Icon::Deploy if report.mission.origin_controlled != Some(player.id) => {
            let probes_only =
                report.mission.army.len() == 1 && report.mission.army.contains_key(&Unit::probe());
            MessageMsg::info(format!(
                "{} returned from planet {}.",
                if probes_only {
                    "Probes"
                } else {
                    "Fleet"
                },
                origin.name
            ))
        },
        Icon::Deploy => MessageMsg::info(format!("Deployed fleet to planet {}.", destination.name)),
        Icon::Colonize if report.planet_colonized => {
            let text = if report.mission.owner == player.id {
                if report.planet.has_buildings() {
                    format!("Planet {} has been conquered.", destination.name)
                } else {
                    format!("Planet {} has been colonized.", destination.name)
                }
            } else {
                format!("Planet {} has been conquered by an enemy.", destination.name)
            };
            if report.mission.owner == player.id {
                MessageMsg::info(text)
            } else {
                MessageMsg::warning(text)
            }
        },
        Icon::Spy => {
            let text = if report.mission.owner == player.id && report.scout_probes > 0 {
                format!("Spy mission successful at planet {}.", destination.name)
            } else if report.mission.owner == player.id {
                format!("Spy mission failed at planet {}; all probes were lost.", destination.name)
            } else {
                format!("Enemy probes were detected around planet {}.", destination.name)
            };
            if report.mission.owner == player.id && report.scout_probes > 0 {
                MessageMsg::info(text)
            } else {
                MessageMsg::warning(text)
            }
        },
        Icon::MissileStrike => {
            let own = report.mission.owner == player.id;
            let text = if own {
                format!("Successful missile strike on planet {}.", destination.name)
            } else {
                format!("Planet {} was hit by a missile strike.", destination.name)
            };
            if own {
                MessageMsg::info(text)
            } else {
                MessageMsg::warning(text)
            }
        },
        Icon::Destroy if report.planet_destroyed => {
            MessageMsg::warning(format!("Planet {} has been destroyed.", destination.name))
        },
        _ if report.is_stalemate() => MessageMsg::info(format!(
            "Battle at planet {} ended in a draw; the attacking fleet is returning.",
            destination.name
        )),
        _ if report.winner() == Some(player.id) => {
            MessageMsg::info(format!("Battle won at planet {}.", destination.name))
        },
        _ => MessageMsg::warning(format!("Battle lost at planet {}.", destination.name)),
    };
    notification.with_action(if report.hidden {
        MessageAction::OpenMissionReports
    } else {
        MessageAction::OpenMissionReport(report.mission.id)
    })
}

fn should_start_planet_destruction(
    planet: &Planet,
    reports: &[&MissionReport],
    turn: usize,
    animating_planets: &mut BTreeSet<PlanetId>,
) -> bool {
    planet.is_destroyed
        && planet.image != 0
        && reports.iter().any(|report| {
            report.turn == turn
                && report.planet_destroyed
                && report.mission.destination == planet.id
        })
        && animating_planets.insert(planet.id)
}

/// Resets local presentation, announces reports, and spawns destruction effects for a new turn.
pub fn start_turn(
    mut commands: Commands,
    mut start_turn_messages: MessageReader<StartTurnMsg>,
    planet_query: Query<(&Transform, &PlanetCmp)>,
    active_destructions: Query<&ExplosionCmp>,
    settings: Res<Settings>,
    mut state: ResMut<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    mut play_audio: MessageWriter<PlayAudioMsg>,
    mut messages: MessageWriter<MessageMsg>,
    mut multiplayer: MessageWriter<MultiplayerRequest>,
    mut next_game_state: ResMut<NextState<GameState>>,
    assets: Res<WorldAssets>,
) {
    let mut animating_planets =
        active_destructions.iter().map(|effect| effect.planet).collect::<BTreeSet<_>>();

    for request in start_turn_messages.read() {
        *state = UiState {
            mission_hover: None,
            lab: state.lab,
            mission_report: state.mission_report,
            ..default()
        };

        let new_reports = player
            .reports
            .iter()
            .filter(|report| report.turn == settings.turn && !report.hidden)
            .collect::<Vec<_>>();
        let returned_reports = player.reports.iter().filter(|report| {
            report.turn == settings.turn
                && report.hidden
                && report.mission.owner == player.id
                && report.mission.objective == Icon::Deploy
                && report.mission.origin_controlled != Some(player.id)
        });

        if !request.skip_battle
            && new_reports.iter().any(|report| {
                report.combat_report.is_some()
                    && report.can_see(&Side::Attacker, player.id)
                    && report.can_see(&Side::Defender, player.id)
            })
        {
            next_game_state.set(GameState::CombatMenu);
            continue;
        }
        if !request.skip_end_game && player.spectator {
            next_game_state.set(GameState::EndGame);
            continue;
        }

        if settings.autosave {
            multiplayer.write(MultiplayerRequest::AutosaveGame);
        }
        messages.write(MessageMsg::info(format!("Turn {} started.", settings.turn)));

        for report in returned_reports {
            let origin = map.get(report.mission.origin);
            let destination = map.get(report.mission.destination);
            messages.write(report_notification(report, &player, origin, destination));
        }

        for planet in &map.planets {
            if !should_start_planet_destruction(
                planet,
                &new_reports,
                settings.turn,
                &mut animating_planets,
            ) {
                continue;
            }
            let Some((transform, _)) =
                planet_query.iter().find(|(_, marker)| marker.id == planet.id)
            else {
                continue;
            };
            let texture = assets.texture("explosion");
            commands.spawn((
                Sprite {
                    image: texture.image,
                    texture_atlas: Some(texture.atlas),
                    custom_size: Some(Vec2::splat(1.5 * planet.size())),
                    ..default()
                },
                Transform::from_xyz(transform.translation.x, transform.translation.y, EXPLOSION_Z),
                ExplosionCmp {
                    timer: Timer::from_seconds(0.1, TimerMode::Repeating),
                    last_index: texture.last_index,
                    planet: planet.id,
                },
            ));
            play_audio.write(PlayAudioMsg::new("explosion"));
        }

        for report in &new_reports {
            let origin = map.get(report.mission.origin);
            let destination = map.get(report.mission.destination);
            // Newly owned colonies have one map-navigation toast from the ownership observer.
            // Keep report navigation for enemy conquests and colonies lost again this turn.
            if report.planet_colonized
                && report.mission.owner == player.id
                && player.owns(destination)
                && !destination.is_destroyed
            {
                continue;
            }
            messages.write(report_notification(report, &player, origin, destination));
        }

        if let Some(last) = new_reports.last() {
            state.mission_tab = MissionTab::MissionReports;
            state.mission_report = Some(last.mission.id);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/core/turns.rs"]
mod tests;
