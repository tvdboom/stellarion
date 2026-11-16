//! Brief map-location echoes for recorded sites and displayed ships and structures.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use crate::core::camera::MainCamera;
use crate::core::map::model::MapCmp;
use crate::core::map::planet::PlanetId;
use crate::core::map::systems::{
    MissionCmp, OrbitalRailgunCmp, PlanetCmp, PublicStructure, SpaceDockCmp,
};
use crate::core::missions::MissionId;
use crate::core::ui::systems::UiState;

const RING_DURATION: f32 = 1.5;
const RING_DELAY: f32 = 0.25;
const DURATION: f32 = RING_DURATION + RING_DELAY * 2.0;
const ACCENT: egui::Color32 = egui::Color32::from_rgb(136, 192, 208);

/// World-space rings render beneath their target sprite and are removed with the map.
#[derive(Component)]
pub(crate) struct SpriteMarkerRing {
    index: usize,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LocationMarkerTarget {
    Site(Vec2),
    Fleet(MissionId),
    Structure(PlanetId, PublicStructure),
}

/// Recorded sites stay fixed. Sprite highlights follow the rendered entity.
#[derive(Debug)]
pub(crate) struct LocationMarker {
    pub(crate) target: LocationMarkerTarget,
    started_at: Option<f64>,
}

impl LocationMarker {
    pub(crate) fn new(position: Vec2) -> Self {
        Self {
            target: LocationMarkerTarget::Site(position),
            started_at: None,
        }
    }

    pub(crate) fn for_mission(mission_id: MissionId) -> Self {
        Self {
            target: LocationMarkerTarget::Fleet(mission_id),
            started_at: None,
        }
    }

    pub(crate) fn for_structure(planet: PlanetId, structure: PublicStructure) -> Self {
        Self {
            target: LocationMarkerTarget::Structure(planet, structure),
            started_at: None,
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

/// Projects the fixed site or follows the displayed sprite, keeping rings outside its artwork.
pub(crate) fn draw_location_marker(
    mut commands: Commands,
    mut contexts: EguiContexts,
    mut state: ResMut<UiState>,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    window: Single<&Window>,
    fleets: Query<(&MissionCmp, &GlobalTransform, &Sprite, &Visibility)>,
    planets: Query<(&PlanetCmp, &Children)>,
    structures: Query<
        (&GlobalTransform, &Sprite, &Visibility, Has<SpaceDockCmp>),
        Or<(With<SpaceDockCmp>, With<OrbitalRailgunCmp>)>,
    >,
    mut rings: Query<
        (
            &SpriteMarkerRing,
            &mut Transform,
            &mut GlobalTransform,
            &MeshMaterial2d<ColorMaterial>,
            &mut Visibility,
        ),
        (
            Without<MissionCmp>,
            Without<MainCamera>,
            Without<SpaceDockCmp>,
            Without<OrbitalRailgunCmp>,
        ),
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
    let waiting_for_site = state.to_selected && state.focus_position.is_some();
    let focusing_planet = state.focus_planet.filter(|_| state.to_selected);
    let Some(marker) = state.location_marker.as_mut() else {
        return;
    };
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let (camera, transform) = camera.into_inner();
    let display_scale = window.scale_factor() / context.pixels_per_point();
    let (target, waiting, ring_travel, depth_offset) = match marker.target {
        LocationMarkerTarget::Site(site) => {
            let Ok(position) = camera.world_to_viewport(transform, site.extend(0.0)) else {
                return;
            };
            let position = position * display_scale;
            if !marker.draw(context, egui::pos2(position.x, position.y), waiting_for_site) {
                state.location_marker = None;
            }
            return;
        },
        LocationMarkerTarget::Fleet(id) => {
            // A patrol's logical position is its zone center; follow the ship on its loop.
            let target = fleets.iter().find_map(|(mission, pose, sprite, visibility)| {
                (mission.id == id && *visibility != Visibility::Hidden).then_some((pose, sprite))
            });
            (target, waiting_for_site, 37.0, 0.25)
        },
        LocationMarkerTarget::Structure(id, structure) => {
            let target =
                planets.iter().find(|(planet, _)| planet.id == id).and_then(|(_, children)| {
                    children.iter().find_map(|child| {
                        let (pose, sprite, visibility, is_dock) = structures.get(child).ok()?;
                        (is_dock == (structure == PublicStructure::SpaceDock)
                            && *visibility != Visibility::Hidden)
                            .then_some((pose, sprite))
                    })
                });
            // Keep the smaller rings above the planet surface and just behind its orbital.
            (target, focusing_planet == Some(id), 18.0, 0.01)
        },
    };
    let Some((sprite_pose, sprite)) = target else {
        state.location_marker = None;
        return;
    };
    if waiting {
        return;
    }

    let now = context.input(|input| input.time);
    let elapsed = (now - *marker.started_at.get_or_insert(now)).max(0.0) as f32;
    if elapsed >= DURATION {
        state.location_marker = None;
        return;
    }
    let world_position = sprite_pose.translation();
    let Ok(position) = camera.world_to_viewport(transform, world_position) else {
        return;
    };
    let half_size = sprite.custom_size.unwrap_or(Vec2::ZERO) * 0.5;
    let sprite_radius = [
        Vec2::new(-half_size.x, -half_size.y),
        Vec2::new(-half_size.x, half_size.y),
        Vec2::new(half_size.x, -half_size.y),
        Vec2::new(half_size.x, half_size.y),
    ]
    .into_iter()
    .filter_map(|corner| {
        camera.world_to_viewport(transform, sprite_pose.transform_point(corner.extend(0.0))).ok()
    })
    .map(|corner| corner.distance(position) * display_scale)
    .fold(0.0, f32::max);
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
        let radius = (sprite_radius + 6.0 + ring_travel * progress) / points_per_world_unit;
        let color = Color::srgba_u8(136, 192, 208, 255).with_alpha((1.0 - progress).powi(2));
        // Egui layers always cover the map. Native geometry gives the image real depth priority.
        let pose = Transform::from_translation(world_position - Vec3::Z * depth_offset)
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
                .get_or_insert_with(|| meshes.add(Annulus::new(0.97, 1.0).mesh().resolution(128)))
                .clone();
            commands.spawn((
                SpriteMarkerRing {
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
}

#[cfg(test)]
#[path = "../../../tests/core/ui_location_marker.rs"]
mod tests;
