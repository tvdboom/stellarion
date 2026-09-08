//! Client-only map pings for newly detected missions and public strategic structures.

use std::collections::BTreeSet;

use bevy::prelude::*;

use super::model::{Map, MapCmp};
use super::planet::PlanetId;
use super::systems::{
    draw_map, update_planet_defenses, OrbitalRailgunCmp, PlanetCmp, SpaceDockCmp,
};
use crate::core::assets::WorldAssets;
use crate::core::constants::MISSION_Z;
use crate::core::loading::{
    refresh_gameplay_projection, refresh_turn_draft, PublicStructure, PublicStructureChange,
    PublicStructureChangeMsg,
};
use crate::core::messages::{MessageAction, MessageMsg};
use crate::core::missions::{Mission, MissionId, Missions};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::units::Amount;
use crate::multiplayer::client::MultiplayerSession;

const DETECTION_SECONDS: f32 = 4.2;
const MISSION_MARKER_SIZE: f32 = 50.0;
const PULSE_COUNT: usize = 4;
const PULSE_INTERVAL_SECONDS: f32 = 0.34;
const PULSE_SECONDS: f32 = 1.35;
const STRUCTURE_EFFECT_SECONDS: f32 = 3.2;
const STRUCTURE_PULSE_COUNT: usize = 3;
const STRUCTURE_PULSE_INTERVAL_SECONDS: f32 = 0.42;
const STRUCTURE_PULSE_SECONDS: f32 = 1.55;

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

#[derive(Component)]
pub(crate) struct PublicStructureEffect {
    pub(crate) planet: PlanetId,
    pub(crate) structure: PublicStructure,
    pub(crate) change: PublicStructureChange,
    pub(crate) owner: u64,
    marker: Entity,
    turn: usize,
    timer: Timer,
}

#[derive(Component)]
pub(crate) struct PublicStructurePulse {
    delay: f32,
    radius: f32,
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
    mut messages: MessageWriter<MessageMsg>,
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
        messages.write(
            MessageMsg::warning("Enemy mission detected.")
                .with_action(MessageAction::OpenEnemyMissions),
        );
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

/// Places compact construction/destruction rings on the structure named by the matching toast.
pub(crate) fn show_public_structure_changes(
    mut commands: Commands,
    mut changes: MessageReader<PublicStructureChangeMsg>,
    map: Res<Map>,
    session: Res<MultiplayerSession>,
    settings: Res<Settings>,
    docks: Query<(Entity, &ChildOf), (With<SpaceDockCmp>, Without<OrbitalRailgunCmp>)>,
    railguns: Query<(Entity, &ChildOf), (With<OrbitalRailgunCmp>, Without<SpaceDockCmp>)>,
    planet_entities: Query<&PlanetCmp>,
    existing: Query<&PublicStructureEffect>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for change in changes.read() {
        let Some(planet) = map.try_get(change.planet) else {
            continue;
        };
        let exists = planet.army.amount(&change.structure.unit()) > 0;
        if matches!(change.change, PublicStructureChange::Built) != exists
            || existing
                .iter()
                .any(|effect| effect.planet == planet.id && effect.structure == change.structure)
        {
            continue;
        }
        let marker = match change.structure {
            PublicStructure::SpaceDock => docks.iter().find_map(|(marker, parent)| {
                planet_entities
                    .get(parent.parent())
                    .is_ok_and(|component| component.id == planet.id)
                    .then_some(marker)
            }),
            PublicStructure::OrbitalRailgun => railguns.iter().find_map(|(marker, parent)| {
                planet_entities
                    .get(parent.parent())
                    .is_ok_and(|component| component.id == planet.id)
                    .then_some(marker)
            }),
        };
        let Some(marker) = marker else {
            continue;
        };
        spawn_public_structure_effect(
            &mut commands,
            marker,
            *change,
            settings.turn,
            planet.size(),
            session.player_color(change.owner).color(),
            &mut meshes,
            &mut materials,
        );
    }
}

fn spawn_public_structure_effect(
    commands: &mut Commands,
    marker: Entity,
    change: PublicStructureChangeMsg,
    turn: usize,
    planet_size: f32,
    color: Color,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let ring = meshes.add(Annulus::new(0.965, 1.0));
    commands.entity(marker).with_children(|parent| {
        parent
            .spawn((
                Transform::default(),
                Visibility::Inherited,
                Pickable::IGNORE,
                PublicStructureEffect {
                    planet: change.planet,
                    structure: change.structure,
                    change: change.change,
                    owner: change.owner,
                    marker,
                    turn,
                    timer: Timer::from_seconds(STRUCTURE_EFFECT_SECONDS, TimerMode::Once),
                },
            ))
            .with_children(|parent| {
                for index in 0..STRUCTURE_PULSE_COUNT {
                    parent.spawn((
                        Mesh2d(ring.clone()),
                        MeshMaterial2d(materials.add(color.with_alpha(0.0))),
                        Transform::from_xyz(0.0, 0.0, 0.35),
                        Pickable::IGNORE,
                        PublicStructurePulse {
                            delay: index as f32 * STRUCTURE_PULSE_INTERVAL_SECONDS,
                            radius: planet_size * 0.34,
                        },
                    ));
                }
            });
    });
}

/// Keeps rings attached to their marker and delays destroyed-marker removal until they finish.
pub(crate) fn animate_public_structure_changes(
    mut commands: Commands,
    time: Res<Time>,
    game_state: Res<State<GameState>>,
    settings: Res<Settings>,
    map: Res<Map>,
    session: Res<MultiplayerSession>,
    mut effects: Query<(Entity, &mut PublicStructureEffect, &Children)>,
    mut markers: Query<
        (&mut Visibility, &mut Sprite, Option<&mut Pickable>),
        (Or<(With<SpaceDockCmp>, With<OrbitalRailgunCmp>)>, Without<PublicStructureEffect>),
    >,
    mut pulses: Query<
        (&PublicStructurePulse, &mut Transform, &MeshMaterial2d<ColorMaterial>),
        Without<PublicStructureEffect>,
    >,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, mut effect, children) in &mut effects {
        let valid = map.try_get(effect.planet).is_some_and(|planet| {
            let exists = planet.army.amount(&effect.structure.unit()) > 0;
            matches!(effect.change, PublicStructureChange::Built) == exists
        });
        if effect.turn != settings.turn || !valid {
            commands.entity(entity).despawn();
            continue;
        }
        let Ok((mut marker_visibility, mut sprite, pickable)) = markers.get_mut(effect.marker)
        else {
            commands.entity(entity).despawn();
            continue;
        };
        let playing = *game_state.get() == GameState::Playing;
        *marker_visibility = if playing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        sprite.color = session.player_color(effect.owner).color();
        if let Some(mut pickable) = pickable {
            *pickable = if playing && matches!(effect.change, PublicStructureChange::Built) {
                Pickable::default()
            } else {
                Pickable::IGNORE
            };
        }
        if !playing {
            continue;
        }

        effect.timer.tick(time.delta());
        if effect.timer.is_finished() {
            if matches!(effect.change, PublicStructureChange::Destroyed) {
                *marker_visibility = Visibility::Hidden;
            }
            commands.entity(entity).despawn();
            continue;
        }
        let elapsed = effect.timer.elapsed_secs();
        for child in children.iter() {
            let Ok((pulse, mut transform, material)) = pulses.get_mut(child) else {
                continue;
            };
            let progress = (elapsed - pulse.delay) / STRUCTURE_PULSE_SECONDS;
            let alpha = if progress > 0.0 && progress < 1.0 {
                let outward = 1.0 - (1.0 - progress).powi(2);
                transform.scale = Vec3::splat(pulse.radius * (0.45 + 1.25 * outward));
                let fade_in = (progress / 0.1).min(1.0);
                0.9 * fade_in * (1.0 - progress).powf(1.4)
            } else {
                0.0
            };
            if let Some(mut material) = materials.get_mut(&material.0) {
                material.color.set_alpha(alpha);
            }
        }
    }
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
            )
            .add_systems(
                Update,
                show_public_structure_changes
                    .after(refresh_gameplay_projection)
                    .after(refresh_turn_draft)
                    .run_if(in_state(AppState::Game)),
            )
            .add_systems(
                Update,
                animate_public_structure_changes
                    .after(show_public_structure_changes)
                    .after(update_planet_defenses)
                    .run_if(in_state(AppState::Game)),
            );
    }
}

#[cfg(test)]
#[path = "../../../tests/core/map_detection.rs"]
mod tests;
