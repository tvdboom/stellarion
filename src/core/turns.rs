//! Bevy turn-boundary presentation and submission adapter around the deterministic core.

use std::collections::BTreeSet;

use bevy::prelude::*;

use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::camera::MainCamera;
use crate::core::combat::report::{MissionReport, Side};
use crate::core::constants::EXPLOSION_Z;
use crate::core::map::battle::BattleEffect;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::orbital_railgun::OrbitalStrikeEffect;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::systems::{ExplosionCmp, PlanetCmp};
use crate::core::menu::utils::add_root_node;
use crate::core::messages::{MessageAction, MessageMsg};
use crate::core::missions::Mission;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::systems::GameplayInputBlocker;
use crate::core::ui::systems::{known_planet_counts, MissionTab, UiState};
use crate::core::units::Unit;
use crate::multiplayer::client::{
    MultiplayerRequest, MultiplayerSession, PendingTurnCommands, SubmissionState,
};

const PLANET_DESTRUCTION_EXPLOSION_SCALE: f32 = 1.75;

/// Holds the terminal overlay until every visible turn-resolution effect has completed.
#[derive(Resource, Default)]
pub(crate) struct EndGamePresentation {
    pending: bool,
}

impl EndGamePresentation {
    /// Locks gameplay until the current turn's visible terminal effects finish.
    pub(crate) fn request(&mut self) {
        self.pending = true;
    }

    /// Returns whether gameplay is locked while the terminal turn is being presented.
    pub(crate) const fn is_pending(&self) -> bool {
        self.pending
    }
}

/// Allows interaction systems to run only outside terminal turn presentation.
pub(crate) fn end_game_presentation_inactive(presentation: Res<EndGamePresentation>) -> bool {
    !presentation.pending
}

/// Opens the terminal overlay once Railgun, combat-aftermath, and destruction effects are gone.
pub(crate) fn finish_end_game_presentation(
    presentation: Res<EndGamePresentation>,
    orbital_strikes: Query<(), With<OrbitalStrikeEffect>>,
    battle_aftermath: Query<(), With<BattleEffect>>,
    planet_destructions: Query<(), With<ExplosionCmp>>,
    mut next_game_state: ResMut<NextState<GameState>>,
) {
    if presentation.pending
        && orbital_strikes.is_empty()
        && battle_aftermath.is_empty()
        && planet_destructions.is_empty()
    {
        next_game_state.set(GameState::EndGame);
    }
}

/// Clears the presentation lock after the terminal state transition has taken effect.
pub(crate) fn clear_end_game_presentation(mut presentation: ResMut<EndGamePresentation>) {
    presentation.pending = false;
}

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
                || mission.is_incoming_protection_for(player.id)
                || mission.is_joint_attacker(player.id)
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
    session: Option<Res<MultiplayerSession>>,
    mut requests: MessageWriter<MultiplayerRequest>,
) {
    #[cfg(not(debug_assertions))]
    let _ = &session;
    if std::mem::take(&mut state.end_turn) {
        if matches!(pending.submission, SubmissionState::Draft | SubmissionState::Retry) {
            #[cfg(debug_assertions)]
            if session.as_deref().is_some_and(|session| session.local_practice) {
                requests.write(MultiplayerRequest::AdvanceLocalPracticeTurn);
                return;
            }
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
    let local_victory = report.winner().is_some_and(|winner| {
        winner == player.id || report.is_defender(player.id) && report.is_defender(winner)
    });
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
        Icon::Protect => {
            MessageMsg::info(format!("Protection fleet stationed at planet {}.", destination.name))
        },
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
        _ if local_victory => {
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

/// Warns only about enemy territorial progress already visible to the local player.
fn territorial_threat_notifications(
    session: &MultiplayerSession,
    map: &Map,
    player: &Player,
) -> Vec<MessageMsg> {
    let Some(game) = &session.active_game else {
        return Vec::new();
    };
    let target = game.persisted.state.planets_to_win();
    let Some(threat_count) = target.checked_sub(1) else {
        return Vec::new();
    };
    let visible_missions = filter_missions(&game.persisted.state.missions, map, player);
    let known_counts = known_planet_counts(map, player, &visible_missions);

    game.members
        .iter()
        .filter(|member| member.player_id != player.id)
        .filter(|member| known_counts.get(&member.player_id) == Some(&threat_count))
        .map(|member| {
            MessageMsg::warning(format!(
                "{} controls {threat_count} of {target} planets needed for victory.",
                member.display_name
            ))
        })
        .collect()
}

/// Resets local presentation, announces reports, and spawns destruction effects for a new turn.
pub fn start_turn(
    mut commands: Commands,
    mut start_turn_messages: MessageReader<StartTurnMsg>,
    planet_query: Query<(&Transform, &PlanetCmp), Without<MainCamera>>,
    active_destructions: Query<&ExplosionCmp>,
    gameplay_blockers: Query<(), With<GameplayInputBlocker>>,
    settings: Res<Settings>,
    mut state: ResMut<UiState>,
    mut end_game_presentation: Option<ResMut<EndGamePresentation>>,
    map: Res<Map>,
    player: Res<Player>,
    session: Option<Res<MultiplayerSession>>,
    mut play_audio: MessageWriter<PlayAudioMsg>,
    mut messages: MessageWriter<MessageMsg>,
    mut next_game_state: ResMut<NextState<GameState>>,
    assets: Res<WorldAssets>,
    mut camera: Query<&mut Transform, With<MainCamera>>,
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
                report.has_combat_playback()
                    && report.can_see(&Side::Attacker, player.id)
                    && report.can_see(&Side::Defender, player.id)
            })
        {
            next_game_state.set(GameState::CombatMenu);
            continue;
        }
        let defer_end_game = if !request.skip_end_game && player.spectator {
            if let Some(presentation) = end_game_presentation.as_deref_mut() {
                presentation.request();
                if gameplay_blockers.is_empty() {
                    commands.spawn((
                        add_root_node(true),
                        GameplayInputBlocker,
                        crate::core::map::model::MapCmp,
                    ));
                }
                true
            } else {
                next_game_state.set(GameState::EndGame);
                false
            }
        } else {
            false
        };

        messages.write(MessageMsg::info(format!("Turn {} started.", settings.turn)));
        // Initial game loading also emits StartTurnMsg, but it is not a newly resolved turn.
        // Delaying this warning until combat playback finishes keeps it with the actual turn start.
        if !request.skip_end_game {
            if let Some(session) = session.as_deref() {
                for notification in territorial_threat_notifications(session, &map, &player) {
                    messages.write(notification);
                }
            }
        }

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
            if defer_end_game
                && (planet.id == player.home_planet
                    || map.try_get(player.home_planet).is_none_or(|home| !home.is_destroyed))
            {
                if let Ok(mut camera) = camera.single_mut() {
                    camera.translation.x = transform.translation.x;
                    camera.translation.y = transform.translation.y;
                }
            }
            let texture = assets.texture("explosion");
            commands.spawn((
                Sprite {
                    image: texture.image,
                    texture_atlas: Some(texture.atlas),
                    // The atlas has transparent padding. This makes the bright blast cover the
                    // world before its sprite changes to the destroyed artwork.
                    custom_size: Some(Vec2::splat(
                        PLANET_DESTRUCTION_EXPLOSION_SCALE * planet.size(),
                    )),
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
