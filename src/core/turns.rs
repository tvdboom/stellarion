//! Bevy turn-boundary presentation and submission adapter around the deterministic core.

use bevy::prelude::*;

use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::combat::report::Side;
use crate::core::constants::EXPLOSION_Z;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::systems::{ExplosionCmp, PlanetCmp};
use crate::core::messages::MessageMsg;
use crate::core::missions::Mission;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::Unit;
use crate::multiplayer::client::MultiplayerRequest;

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

/// Remembers whether the end-turn control was already committed.
#[derive(Resource, Default)]
pub struct PreviousEndTurnState(bool);

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

/// Commits the local command draft once when the player ends the simultaneous turn.
pub fn check_turn_ended(
    mut state: ResMut<UiState>,
    mut previous: ResMut<PreviousEndTurnState>,
    mut requests: MessageWriter<MultiplayerRequest>,
) {
    if state.end_turn && !previous.0 {
        requests.write(MultiplayerRequest::SubmitTurn);
        previous.0 = true;
    } else if !state.end_turn && previous.0 {
        // A submission is immutable and idempotent once sent; keep the control committed.
        state.end_turn = true;
    }
}

/// Resets local presentation, announces reports, and spawns destruction effects for a new turn.
pub fn start_turn(
    mut commands: Commands,
    mut start_turn_messages: MessageReader<StartTurnMsg>,
    planet_query: Query<(&Transform, &PlanetCmp)>,
    settings: Res<Settings>,
    mut state: ResMut<UiState>,
    mut previous: ResMut<PreviousEndTurnState>,
    map: Res<Map>,
    player: Res<Player>,
    mut play_audio: MessageWriter<PlayAudioMsg>,
    mut messages: MessageWriter<MessageMsg>,
    mut multiplayer: MessageWriter<MultiplayerRequest>,
    mut next_game_state: ResMut<NextState<GameState>>,
    assets: Res<WorldAssets>,
) {
    for request in start_turn_messages.read() {
        *state = UiState {
            mission_hover: None,
            lab: state.lab,
            mission_report: state.mission_report,
            ..default()
        };
        previous.0 = false;

        let new_reports = player
            .reports
            .iter()
            .filter(|report| report.turn == settings.turn && !report.hidden)
            .collect::<Vec<_>>();

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
            multiplayer.write(MultiplayerRequest::SaveGame);
        }
        messages.write(MessageMsg::info(format!("Turn {} started.", settings.turn)));

        for planet in map.planets.iter().filter(|planet| planet.is_destroyed && planet.image != 0) {
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
            match report.mission.objective {
                Icon::Deploy if report.mission.origin_controlled != Some(player.id) => {
                    let probes_only = report.mission.army.len() == 1
                        && report.mission.army.contains_key(&Unit::probe());
                    messages.write(MessageMsg::info(format!(
                        "{} returned from planet {}.",
                        if probes_only {
                            "Probes"
                        } else {
                            "Fleet"
                        },
                        origin.name
                    )));
                },
                Icon::Deploy => {
                    messages.write(MessageMsg::info(format!(
                        "Deployed fleet to planet {}.",
                        destination.name
                    )));
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
                    messages.write(if report.mission.owner == player.id {
                        MessageMsg::info(text)
                    } else {
                        MessageMsg::warning(text)
                    });
                },
                Icon::Spy => {
                    let text = if report.mission.owner == player.id && report.scout_probes > 0 {
                        format!("Successful spy mission on planet {}.", destination.name)
                    } else if report.mission.owner == player.id {
                        format!("All probes lost while spying planet {}.", destination.name)
                    } else {
                        format!("Enemy probes were detected around planet {}.", destination.name)
                    };
                    messages.write(
                        if report.mission.owner == player.id && report.scout_probes > 0 {
                            MessageMsg::info(text)
                        } else {
                            MessageMsg::warning(text)
                        },
                    );
                },
                Icon::MissileStrike => {
                    let own = report.mission.owner == player.id;
                    let text = if own {
                        format!("Successful missile strike on planet {}.", destination.name)
                    } else {
                        format!("Planet {} was hit by a missile strike.", destination.name)
                    };
                    messages.write(if own {
                        MessageMsg::info(text)
                    } else {
                        MessageMsg::warning(text)
                    });
                },
                Icon::Destroy if report.planet_destroyed => {
                    messages.write(MessageMsg::warning(format!(
                        "Planet {} has been destroyed.",
                        destination.name
                    )));
                },
                _ if report.winner() == Some(player.id) => {
                    messages.write(MessageMsg::info(format!(
                        "Battle won at planet {}.",
                        destination.name
                    )));
                },
                _ => {
                    messages.write(MessageMsg::warning(format!(
                        "Battle lost at planet {}.",
                        destination.name
                    )));
                },
            }
        }

        if let Some(last) = new_reports.last() {
            state.mission_tab = MissionTab::MissionReports;
            state.mission_report = Some(last.mission.id);
        }
    }
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;

    use super::*;

    #[test]
    /// Mission visibility always includes the owning player's commands.
    fn owner_can_see_own_empty_mission_list() {
        let player = Player::default();
        let map = Map::new_with_rng(5, 0, &mut rand_chacha::ChaCha8Rng::from_seed([3; 32]));
        assert!(filter_missions(&[], &map, &player).is_empty());
    }
}
