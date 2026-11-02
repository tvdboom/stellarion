//! Local patrol flight and sensor-zone effects; these never move a simulated fleet.

use std::collections::BTreeMap;
use std::f32::consts::TAU;

use bevy::asset::RenderAssetUsages;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;

use crate::core::camera::MainCamera;
use crate::core::constants::MISSION_Z;
use crate::core::map::icon::Icon;
use crate::core::map::model::{Map, MapCmp};
use crate::core::map::systems::MissionCmp;
use crate::core::missions::{Mission, MissionId, Missions};
use crate::core::patrols::{valid_patrol_target, PATROL_RADIUS};
use crate::core::player::Player;
use crate::core::ui::systems::UiState;
use crate::multiplayer::client::MultiplayerSession;

const WAKE_PARTICLES: usize = 10;
const ZONE_SWEEPS: u8 = 3;
const ZONE_RESOLUTION: u32 = 256;

/// Starts the local route at the arrival point instead of snapping into an existing orbit.
#[derive(Component, Default)]
pub struct PatrolFlight {
    stationed_since: Option<f64>,
    last_position: Option<Vec2>,
    departure: Option<(f64, Vec2)>,
}

impl PatrolFlight {
    pub(crate) fn sample(&mut self, mission: &Mission, incoming: Vec2, now: f64) -> (Vec2, Vec2) {
        if !mission.is_patrolling() {
            if self.stationed_since.take().is_some() && mission.is_returning() {
                self.departure =
                    Some((now, self.last_position.unwrap_or(mission.position) - mission.position));
            }
            let mut position = mission.position;
            if let Some((started, offset)) = self.departure {
                let progress = ((now - started) / 0.9).clamp(0.0, 1.0) as f32;
                position += offset * (1.0 - progress).powi(2) * (1.0 + 2.0 * progress);
                if progress >= 1.0 {
                    self.departure = None;
                }
            }
            self.last_position = Some(position);
            return (position, incoming);
        }
        self.departure = None;
        let since = *self.stationed_since.get_or_insert(now);
        let pose = flight_pose(mission.position, incoming, (now - since).max(0.0), mission.id);
        self.last_position = Some(pose.0);
        pose
    }

    pub(crate) fn reset(&mut self) {
        self.stationed_since = None;
        self.last_position = None;
        self.departure = None;
    }
}

/// A continuous figure eight starts along the incoming course, gently accelerating into a loop.
fn flight_pose(center: Vec2, incoming: Vec2, age: f64, id: MissionId) -> (Vec2, Vec2) {
    let variation = (id % 997) as f32 / 997.0;
    let horizontal = PATROL_RADIUS * (0.39 + variation * 0.06);
    let vertical = PATROL_RADIUS * 0.21;
    let phase = ((age - 1.0 + (-age).exp()) * (0.27 + f64::from(variation) * 0.04))
        .rem_euclid(std::f64::consts::TAU) as f32;
    let heading = if incoming.length_squared() > f32::EPSILON {
        incoming.to_angle()
    } else {
        0.0
    };
    let rotation = Vec2::from_angle(heading - (2.0 * vertical).atan2(horizontal));
    let offset = Vec2::new(horizontal * phase.sin(), vertical * (2.0 * phase).sin());
    let tangent = Vec2::new(horizontal * phase.cos(), 2.0 * vertical * (2.0 * phase).cos());
    (center + offset.rotate(rotation), tangent.rotate(rotation).normalize_or_zero())
}

#[derive(Component)]
pub(crate) struct PatrolWake(usize);

pub(crate) fn spawn_patrol_wake(parent: &mut ChildSpawnerCommands) {
    for index in 0..WAKE_PARTICLES {
        parent.spawn((
            Sprite::from_color(Color::NONE, Vec2::ONE),
            Transform::default(),
            Visibility::Hidden,
            Pickable::IGNORE,
            PatrolWake(index),
        ));
    }
}

/// Reuses small curved exhaust dashes behind the displayed route, inheriting fleet visibility.
pub(crate) fn animate_patrol_wakes(
    time: Res<Time>,
    missions: Res<Missions>,
    map: Res<Map>,
    session: Res<MultiplayerSession>,
    roots: Query<(&MissionCmp, &PatrolFlight, &Transform, &Children)>,
    mut wakes: Query<
        (&PatrolWake, &mut Transform, &mut Sprite, &mut Visibility),
        Without<MissionCmp>,
    >,
) {
    for (root, flight, root_transform, children) in &roots {
        let patrol = flight.stationed_since.and_then(|since| {
            missions.get(root.id).filter(|mission| mission.is_patrolling()).map(|mission| {
                (
                    mission,
                    (mission.position - map.get(mission.origin).position).normalize_or_zero(),
                    (time.elapsed_secs_f64() - since).max(0.0),
                    session.player_color(mission.owner).color(),
                )
            })
        });
        let inverse = root_transform.rotation.inverse();
        for child in children.iter() {
            let Ok((wake, mut transform, mut sprite, mut visibility)) = wakes.get_mut(child) else {
                continue;
            };
            let Some((mission, incoming, age, color)) = patrol else {
                *visibility = Visibility::Hidden;
                continue;
            };
            let delay = 0.8 + wake.0 as f64 * 0.14;
            if age < delay {
                *visibility = Visibility::Hidden;
                continue;
            }
            let (position, direction) =
                flight_pose(mission.position, incoming, age - delay, mission.id);
            transform.translation =
                inverse * (position - root_transform.translation.truncate()).extend(0.0);
            transform.translation.z = -0.12;
            transform.rotation = inverse * Quat::from_rotation_z(direction.to_angle());
            let fade = 1.0 - wake.0 as f32 / WAKE_PARTICLES as f32;
            sprite.custom_size = Some(Vec2::new(7.0 + fade * 8.0, 0.8 + fade * 1.6));
            sprite.color = color.with_alpha(fade * fade * 0.4);
            *visibility = Visibility::Inherited;
        }
    }
}

#[derive(Clone, Copy)]
enum ZonePart {
    Boundary,
    Halo,
    Sweep(u8),
    Head(u8),
}

#[derive(Component)]
pub(crate) struct PatrolZone {
    id: MissionId,
    part: ZonePart,
}

pub(crate) struct ZoneMeshes {
    boundary: Handle<Mesh>,
    halo: Handle<Mesh>,
    sweep: Handle<Mesh>,
    head: Handle<Mesh>,
}

impl ZoneMeshes {
    fn new(meshes: &mut Assets<Mesh>) -> Self {
        Self {
            boundary: meshes.add(
                Annulus::new(PATROL_RADIUS - 1.7, PATROL_RADIUS).mesh().resolution(ZONE_RESOLUTION),
            ),
            halo: meshes.add(
                Annulus::new(PATROL_RADIUS - 7.0, PATROL_RADIUS - 6.0)
                    .mesh()
                    .resolution(ZONE_RESOLUTION),
            ),
            sweep: meshes.add(sweep_mesh()),
            head: meshes.add(head_mesh()),
        }
    }
}

fn colored_mesh(positions: Vec<[f32; 3]>, colors: Vec<[f32; 4]>, indices: Vec<u32>) -> Mesh {
    let count = positions.len();
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0, 0.0]; count])
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_indices(Indices::U32(indices))
}

fn sweep_mesh() -> Mesh {
    let mut positions = Vec::with_capacity(66);
    let mut colors = Vec::with_capacity(66);
    let mut indices = Vec::with_capacity(192);
    for index in 0..=32 {
        let progress = index as f32 / 32.0;
        let direction = Vec2::from_angle((progress - 1.0) * 0.8);
        let outer = PATROL_RADIUS - 2.0;
        let width = 0.6 + progress * progress * 3.0;
        for radius in [outer - width, outer] {
            positions.push((direction * radius).extend(0.0).to_array());
            colors.push([1.0, 1.0, 1.0, progress * progress]);
        }
        if index < 32 {
            let start = index * 2;
            indices.extend([start, start + 1, start + 2, start + 1, start + 3, start + 2]);
        }
    }
    colored_mesh(positions, colors, indices)
}

fn head_mesh() -> Mesh {
    let mut positions = vec![[0.0, 0.0, 0.0]];
    let mut colors = vec![[1.0; 4]];
    let mut indices = Vec::with_capacity(96);
    for index in 0..32 {
        positions.push((Vec2::from_angle(index as f32 * TAU / 32.0) * 3.5).extend(0.0).to_array());
        colors.push([1.0, 1.0, 1.0, 0.0]);
        indices.extend([0, index + 1, (index + 1) % 32 + 1]);
    }
    colored_mesh(positions, colors, indices)
}

fn zone_part_pose(
    id: MissionId,
    part: ZonePart,
    center: Vec2,
    color: Color,
    time: f32,
) -> (Transform, Color) {
    let phase = (id % 997) as f32 * TAU / 997.0;
    let pulse = (time * 0.85 + phase).sin() * 0.5 + 0.5;
    let mut transform = Transform::from_translation(center.extend(MISSION_Z - 0.5));
    let alpha = color.alpha();
    let tint = match part {
        ZonePart::Boundary => color.with_alpha(alpha * (0.8 + pulse * 0.15)),
        ZonePart::Halo => {
            transform.scale = Vec3::splat(0.995 + pulse * 0.005);
            color.with_alpha(alpha * (0.13 + pulse * 0.1))
        },
        ZonePart::Sweep(index) | ZonePart::Head(index) => {
            let angle = time * 0.32 + phase + f32::from(index) * TAU / f32::from(ZONE_SWEEPS);
            if matches!(part, ZonePart::Head(_)) {
                transform.translation +=
                    (Vec2::from_angle(angle) * (PATROL_RADIUS - 4.7)).extend(0.02);
                transform.scale = Vec3::splat(1.0 + pulse * 0.12);
                let rgb = color.to_srgba();
                Color::srgba(
                    rgb.red * 0.4 + 0.6,
                    rgb.green * 0.4 + 0.6,
                    rgb.blue * 0.4 + 0.6,
                    (alpha * 1.8).min(0.95),
                )
            } else {
                transform.rotation = Quat::from_rotation_z(angle);
                color.with_alpha((alpha * 1.4).min(0.9))
            }
        },
    };
    (transform, tint)
}

/// Keeps the coverage boundary fixed while light sweeps and an inner echo animate within it.
pub(crate) fn update_patrol_zones(
    mut commands: Commands,
    mut zones: Query<(Entity, &PatrolZone, &mut Transform, &MeshMaterial2d<ColorMaterial>)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut geometry: Local<Option<ZoneMeshes>>,
    state: Res<UiState>,
    missions: Res<Missions>,
    player: Res<Player>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    window: Single<&Window>,
    map: Res<Map>,
    time: Res<Time>,
) {
    let mut desired = BTreeMap::new();
    for mission in missions.iter().filter(|mission| {
        mission.owner == player.id && mission.patrol.is_some() && !mission.is_returning()
    }) {
        if let Some(patrol) = &mission.patrol {
            desired.insert(
                mission.id,
                (
                    patrol.center,
                    player.color().color().with_alpha(if mission.is_patrolling() {
                        0.5
                    } else {
                        0.2
                    }),
                    false,
                ),
            );
        }
    }
    if state.selecting_patrol_zone || state.mission && state.mission_info.objective == Icon::Patrol
    {
        let (camera, transform) = camera.into_inner();
        let center = if state.selecting_patrol_zone {
            window
                .cursor_position()
                .and_then(|cursor| camera.viewport_to_world_2d(transform, cursor).ok())
        } else {
            state.patrol_center
        };
        if let Some(center) = center {
            let color = if valid_patrol_target(&map, state.mission_info.origin, center, None) {
                player.color().color().with_alpha(0.7)
            } else {
                Color::srgba(1.0, 0.2, 0.2, 0.7)
            };
            desired.insert(0, (center, color, false));
        }
    }
    for (entity, zone, mut transform, material) in &mut zones {
        if let Some((center, color, present)) = desired.get_mut(&zone.id) {
            let (pose, tint) =
                zone_part_pose(zone.id, zone.part, *center, *color, time.elapsed_secs());
            *transform = pose;
            if let Some(mut material) = materials.get_mut(&material.0) {
                material.color = tint;
            }
            *present = true;
        } else {
            commands.entity(entity).despawn();
        }
    }
    for (id, (center, color, present)) in desired {
        if present {
            continue;
        }
        let geometry = geometry.get_or_insert_with(|| ZoneMeshes::new(&mut meshes));
        let parts = [
            (ZonePart::Boundary, geometry.boundary.clone()),
            (ZonePart::Halo, geometry.halo.clone()),
        ]
        .into_iter()
        .chain((0..ZONE_SWEEPS).flat_map(|index| {
            [
                (ZonePart::Sweep(index), geometry.sweep.clone()),
                (ZonePart::Head(index), geometry.head.clone()),
            ]
        }));
        for (part, mesh) in parts {
            let (pose, tint) = zone_part_pose(id, part, center, color, time.elapsed_secs());
            commands.spawn((
                Mesh2d(mesh),
                MeshMaterial2d(materials.add(tint)),
                pose,
                Pickable::IGNORE,
                PatrolZone {
                    id,
                    part,
                },
                MapCmp,
            ));
        }
    }
}

#[cfg(test)]
#[path = "../../tests/core/patrol_visuals.rs"]
mod tests;
