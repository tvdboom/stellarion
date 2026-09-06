//! Client-only radar pings for enemy missions that have just become visible.

use std::collections::BTreeSet;

use bevy::prelude::*;

use super::model::{Map, MapCmp};
use super::systems::draw_map;
use crate::core::assets::WorldAssets;
use crate::core::constants::MISSION_Z;
use crate::core::loading::{refresh_gameplay_projection, refresh_turn_draft};
use crate::core::missions::{Mission, MissionId, Missions};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};

const DETECTION_SECONDS: f32 = 4.2;
const MISSION_MARKER_SIZE: f32 = 50.0;
const PULSE_COUNT: usize = 4;
const PULSE_INTERVAL_SECONDS: f32 = 0.34;
const PULSE_SECONDS: f32 = 1.35;

fn detected_by_scanner(mission: &Mission, map: &Map, player: &Player) -> bool {
    mission.owner != player.id
        && (mission.is_seen_by_phalanx(map, player).is_some()
            || mission.is_seen_by_radar(map, player).is_some())
}

/// Tracks visibility transitions without replaying detections on projection refreshes or resumes.
#[derive(Resource, Default)]
struct DetectedMissions {
    turn: usize,
    visible: BTreeSet<MissionId>,
    pending: BTreeSet<MissionId>,
    announced: BTreeSet<MissionId>,
}

impl DetectedMissions {
    fn observe(&mut self, missions: &Missions, map: &Map, player: &Player, turn: usize) -> bool {
        if self.turn != turn {
            self.turn = turn;
            self.pending.clear();
            self.announced.clear();
        }

        let visible = missions.iter().map(|mission| mission.id).collect::<BTreeSet<_>>();
        let mut added = false;
        for mission in missions.iter().filter(|mission| {
            !self.visible.contains(&mission.id)
                && !self.announced.contains(&mission.id)
                && detected_by_scanner(mission, map, player)
        }) {
            added |= self.pending.insert(mission.id);
        }
        self.pending.retain(|id| {
            missions.get(*id).is_some_and(|mission| detected_by_scanner(mission, map, player))
        });
        self.visible = visible;
        added
    }
}

/// Existing visible missions form the baseline when loading or resuming a game.
fn initialize_detections(
    mut detections: ResMut<DetectedMissions>,
    missions: Res<Missions>,
    settings: Res<Settings>,
) {
    *detections = DetectedMissions {
        turn: settings.turn,
        visible: missions.iter().map(|mission| mission.id).collect(),
        ..default()
    };
}

#[derive(Component)]
struct DetectionEffect {
    mission: MissionId,
    turn: usize,
    timer: Timer,
}

#[derive(Component)]
enum DetectionPart {
    Pulse {
        delay: f32,
        radius: f32,
    },
    Label {
        y: f32,
    },
}

fn show_detections(
    mut commands: Commands,
    mut detections: ResMut<DetectedMissions>,
    missions: Res<Missions>,
    map: Res<Map>,
    player: Res<Player>,
    settings: Res<Settings>,
    game_state: Res<State<GameState>>,
    assets: Res<WorldAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    if (missions.is_changed() || detections.turn != settings.turn)
        && detections.observe(&missions, &map, &player, settings.turn)
    {
        // Let StartTurnMsg open any combat presentation before revealing the map ping.
        return;
    }
    if *game_state.get() != GameState::Playing {
        return;
    }

    for id in std::mem::take(&mut detections.pending) {
        let Some(mission) =
            missions.get(id).filter(|mission| detected_by_scanner(mission, &map, &player))
        else {
            continue;
        };
        detections.announced.insert(id);
        spawn_detection(
            &mut commands,
            mission,
            settings.turn,
            player.color().color(),
            &assets,
            &mut meshes,
            &mut materials,
        );
    }
}

fn spawn_detection(
    commands: &mut Commands,
    mission: &Mission,
    turn: usize,
    color: Color,
    assets: &WorldAssets,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let ring = meshes.add(Annulus::new(0.97, 1.0));
    let label_y = super::aftermath_label_y(MISSION_MARKER_SIZE, 0);
    commands
        .spawn((
            Transform::from_translation(mission.position.extend(0.0)),
            Visibility::Inherited,
            Pickable::IGNORE,
            MapCmp,
            DetectionEffect {
                mission: mission.id,
                turn,
                timer: Timer::from_seconds(DETECTION_SECONDS, TimerMode::Once),
            },
        ))
        .with_children(|parent| {
            for index in 0..PULSE_COUNT {
                parent.spawn((
                    Mesh2d(ring.clone()),
                    MeshMaterial2d(materials.add(color.with_alpha(0.0))),
                    Transform::from_xyz(0.0, 0.0, MISSION_Z + 0.15),
                    Pickable::IGNORE,
                    DetectionPart::Pulse {
                        delay: index as f32 * PULSE_INTERVAL_SECONDS,
                        radius: MISSION_MARKER_SIZE * 0.72,
                    },
                ));
            }
            parent.spawn((
                Text2d::new("ENEMY MISSION DETECTED"),
                TextFont {
                    font: assets.font("bold").into(),
                    font_size: 17.0.into(),
                    ..default()
                },
                TextColor(color.with_alpha(0.0)),
                Transform::from_xyz(0.0, label_y, MISSION_Z + 0.25),
                Pickable::IGNORE,
                DetectionPart::Label {
                    y: label_y,
                },
            ));
        });
}

/// Pauses behind overlays and removes the radar ping after its label fades away.
fn animate_detections(
    mut commands: Commands,
    time: Res<Time>,
    game_state: Res<State<GameState>>,
    settings: Res<Settings>,
    missions: Res<Missions>,
    mut effects: Query<(Entity, &mut DetectionEffect, &Children, &mut Visibility)>,
    mut parts: Query<(
        Entity,
        &DetectionPart,
        &mut Transform,
        Option<&MeshMaterial2d<ColorMaterial>>,
        Option<&mut TextColor>,
    )>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, mut effect, children, mut visibility) in &mut effects {
        if effect.turn != settings.turn || missions.get(effect.mission).is_none() {
            commands.entity(entity).despawn();
            continue;
        }
        let playing = *game_state.get() == GameState::Playing;
        *visibility = if playing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if !playing {
            continue;
        }
        effect.timer.tick(time.delta());
        if effect.timer.is_finished() {
            commands.entity(entity).despawn();
            continue;
        }

        let elapsed = effect.timer.elapsed_secs();
        let settle = ((elapsed - 2.8) / (DETECTION_SECONDS - 2.8)).clamp(0.0, 1.0);
        for child in children.iter() {
            let Ok((child, part, mut transform, material, text)) = parts.get_mut(child) else {
                continue;
            };
            match part {
                DetectionPart::Pulse {
                    delay,
                    radius,
                } => {
                    let progress = (elapsed - delay) / PULSE_SECONDS;
                    if progress >= 1.0 {
                        commands.entity(child).despawn();
                    } else if progress > 0.0 {
                        let outward = 1.0 - (1.0 - progress).powi(2);
                        transform.scale = Vec3::splat(radius * (0.32 + 1.35 * outward));
                        if let Some(mut material) =
                            material.and_then(|handle| materials.get_mut(&handle.0))
                        {
                            let fade_in = (progress / 0.08).min(1.0);
                            material.color.set_alpha(0.8 * fade_in * (1.0 - progress).powf(1.35));
                        }
                    }
                },
                DetectionPart::Label {
                    y,
                } => {
                    let fade_in = ((elapsed - 0.25) / 0.4).clamp(0.0, 1.0);
                    transform.scale = Vec3::splat(1.0 - 0.12 * settle);
                    transform.translation.y = y + 8.0 * (1.0 - fade_in);
                    if let Some(mut text) = text {
                        text.0.set_alpha(fade_in * (1.0 - settle));
                    }
                },
            }
        }
    }
}

pub(crate) struct MissionDetectionPlugin;

impl Plugin for MissionDetectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DetectedMissions>()
            .add_systems(OnEnter(AppState::Game), initialize_detections.after(draw_map))
            .add_systems(
                Update,
                (show_detections, animate_detections)
                    .chain()
                    .after(refresh_gameplay_projection)
                    .after(refresh_turn_draft)
                    .run_if(in_state(AppState::Game)),
            );
    }
}

#[cfg(test)]
#[path = "../../../tests/core/map_detection.rs"]
mod tests;
