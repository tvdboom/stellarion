//! Brief map-location echoes for recorded sites and displayed fleet icons.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::core::camera::MainCamera;
use crate::core::map::model::MapCmp;
use crate::core::map::systems::MissionCmp;
use crate::core::missions::MissionId;
use crate::core::ui::systems::UiState;

const RING_DURATION: f32 = 1.5;
const RING_DELAY: f32 = 0.25;
const DURATION: f32 = RING_DURATION + RING_DELAY * 2.0;
const ACCENT: egui::Color32 = egui::Color32::from_rgb(136, 192, 208);

/// World-space rings render beneath fleet sprites and are removed with the strategic map.
#[derive(Component)]
pub(crate) struct FleetMarkerRing {
    index: usize,
}

/// Recorded sites stay fixed; fleet highlights follow their presentation without moving the model.
#[derive(Debug)]
pub(crate) struct LocationMarker {
    pub(crate) position: Vec2,
    pub(crate) mission_id: Option<MissionId>,
    started_at: Option<f64>,
}

impl LocationMarker {
    pub(crate) fn new(position: Vec2) -> Self {
        Self {
            position,
            mission_id: None,
            started_at: None,
        }
    }

    pub(crate) fn for_mission(mission_id: MissionId, position: Vec2) -> Self {
        Self {
            mission_id: Some(mission_id),
            ..Self::new(position)
        }
    }

    /// Wait for camera arrival so a long pan cannot consume the entire animation offscreen.
    fn draw(&mut self, context: &egui::Context, center: egui::Pos2, waiting: bool) -> bool {
        if waiting {
            return true;
        }
        let now = context.input(|input| input.time);
        let elapsed = (now - *self.started_at.get_or_insert(now)).max(0.0) as f32;
        if elapsed >= DURATION {
            return false;
        }

        // Painting without an Area or an interaction rectangle leaves map input untouched.
        let painter = context.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("map location marker"),
        ));
        let fade = (1.0 - elapsed / DURATION).min(0.6) / 0.6;
        painter.circle_filled(center, 4.0, egui::Color32::BLACK.gamma_multiply(fade * 0.7));
        painter.circle_filled(center, 2.0, ACCENT.gamma_multiply(fade));
        for index in 0..3 {
            let progress = (elapsed - index as f32 * RING_DELAY) / RING_DURATION;
            if !(0.0..1.0).contains(&progress) {
                continue;
            }
            let radius = 7.0 + 37.0 * progress;
            let color = ACCENT.gamma_multiply((1.0 - progress).powi(2));
            painter.circle_stroke(
                center,
                radius,
                egui::Stroke::new(4.0, color.gamma_multiply(0.15)),
            );
            painter.circle_stroke(center, radius, egui::Stroke::new(1.5, color));
        }
        context.request_repaint();
        true
    }
}

/// Projects the current fleet image or a fixed site, keeping the pulse outside the fleet artwork.
pub(crate) fn draw_location_marker(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut state: ResMut<UiState>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    window: Single<&Window>,
    fleets: Query<(&MissionCmp, &GlobalTransform, &Sprite, &Visibility)>,
    mut rings: Query<
        (
            &FleetMarkerRing,
            &mut Transform,
            &mut GlobalTransform,
            &MeshMaterial2d<ColorMaterial>,
            &mut Visibility,
        ),
        (Without<MissionCmp>, Without<MainCamera>),
    >,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    mut ring_mesh: Local<Option<Handle<Mesh>>>,
) {
    for (_, _, _, material, mut visibility) in &mut rings {
        *visibility = Visibility::Hidden;
        if let Some(mut material) = materials.get_mut(&material.0) {
            material.color = Color::NONE;
        }
    }
    let waiting = state.to_selected && state.focus_position.is_some();
    let Some(marker) = state.location_marker.as_mut() else {
        return;
    };
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let (camera, transform) = camera.into_inner();
    let fleet = if let Some(id) = marker.mission_id {
        let Some(fleet) = fleets.iter().find(|(mission, _, _, visibility)| {
            mission.id == id && **visibility != Visibility::Hidden
        }) else {
            state.location_marker = None;
            return;
        };
        Some(fleet)
    } else {
        None
    };
    // A patrol's logical position is its zone center; its image can be anywhere along its loop.
    let world_position =
        fleet.map_or(marker.position.extend(0.0), |(_, fleet, _, _)| fleet.translation());
    let Ok(position) = camera.world_to_viewport(transform, world_position) else {
        return;
    };
    let display_scale = window.scale_factor() / context.pixels_per_point();
    let fleet_radius = fleet.map(|(_, fleet, sprite, _)| {
        let half_size = sprite.custom_size.unwrap_or(Vec2::ZERO) * 0.5;
        [
            Vec2::new(-half_size.x, -half_size.y),
            Vec2::new(half_size.x, -half_size.y),
            Vec2::new(half_size.x, half_size.y),
            Vec2::new(-half_size.x, half_size.y),
        ]
        .into_iter()
        .filter_map(|corner| {
            camera.world_to_viewport(transform, fleet.transform_point(corner.extend(0.0))).ok()
        })
        .map(|corner| corner.distance(position) * display_scale)
        .fold(0.0, f32::max)
    });
    if let Some(fleet_radius) = fleet_radius {
        if waiting {
            return;
        }
        let now = context.input(|input| input.time);
        let elapsed = (now - *marker.started_at.get_or_insert(now)).max(0.0) as f32;
        if elapsed >= DURATION {
            state.location_marker = None;
            return;
        }
        let Ok(unit) = camera.world_to_viewport(transform, world_position + Vec3::X) else {
            return;
        };
        let points_per_world_unit = unit.distance(position) * display_scale;
        if points_per_world_unit <= f32::EPSILON {
            return;
        }
        for index in 0..3 {
            let progress = (elapsed - index as f32 * RING_DELAY) / RING_DURATION;
            if !(0.0..1.0).contains(&progress) {
                continue;
            }
            let radius = (fleet_radius + 6.0 + 37.0 * progress) / points_per_world_unit;
            let color = Color::srgba_u8(136, 192, 208, 255).with_alpha((1.0 - progress).powi(2));
            // Egui layers always cover the map. Native geometry gives the image real depth priority.
            let pose = Transform::from_translation(world_position - Vec3::Z * 0.25)
                .with_scale(Vec3::new(radius, radius, 1.0));
            if let Some((_, mut transform, mut global, material, mut visibility)) =
                rings.iter_mut().find(|(ring, _, _, _, _)| ring.index == index)
            {
                *transform = pose;
                // This pass runs after transform propagation; update the rendered pose this frame.
                *global = GlobalTransform::from(pose);
                *visibility = Visibility::Inherited;
                if let Some(mut material) = materials.get_mut(&material.0) {
                    material.color = color;
                }
            } else {
                let mesh = ring_mesh
                    .get_or_insert_with(|| {
                        meshes.add(Annulus::new(0.97, 1.0).mesh().resolution(128))
                    })
                    .clone();
                commands.spawn((
                    FleetMarkerRing {
                        index,
                    },
                    Mesh2d(mesh),
                    MeshMaterial2d(materials.add(ColorMaterial {
                        color,
                        ..default()
                    })),
                    pose,
                    GlobalTransform::from(pose),
                    Visibility::Inherited,
                    Pickable::IGNORE,
                    MapCmp,
                ));
            }
        }
        context.request_repaint();
    } else {
        let position = position * display_scale;
        if !marker.draw(context, egui::pos2(position.x, position.y), waiting) {
            state.location_marker = None;
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/core/ui_location_marker.rs"]
mod tests;
