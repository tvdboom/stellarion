//! Strategic camera setup, movement, zoom, clamping, and reset systems.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::core::constants::{LERP_FACTOR, MAX_ZOOM, MIN_ZOOM, ZOOM_FACTOR};
use crate::core::map::model::Map;
use crate::core::map::systems::PlanetCmp;
use crate::core::ui::systems::UiState;

/// At a camera limit, the outermost world remains this far inside the matching screen edge.
const GALAXY_EDGE_SCREEN_FRACTION: f32 = 0.2;
/// Maximum elastic travel beyond a camera limit while the map is being dragged.
const OVERSCROLL_SCREEN_FRACTION: f32 = 0.15;
/// Exponential return speed after input releases the camera outside its limits.
const BOUNDS_RETURN_RATE: f32 = 14.0;

#[derive(Component)]
/// Marker component for the unique strategic 2D camera.
pub struct MainCamera;

#[derive(Component)]
/// Presentation settings for a map layer that follows the camera at a reduced rate.
pub struct ParallaxCmp {
    /// Fraction of camera translation inherited by the layer.
    pub camera_follow: f32,
    /// Scale applied before responding to orthographic zoom.
    pub base_scale: f32,
    /// Exponent controlling how strongly the layer responds to zoom.
    pub zoom_power: f32,
    /// Slow world-space drift, measured in pixels per second.
    pub drift: Vec2,
}

impl ParallaxCmp {
    /// Creates a parallax layer with explicit depth, zoom, and drift behavior.
    pub const fn new(camera_follow: f32, base_scale: f32, zoom_power: f32, drift: Vec2) -> Self {
        Self {
            camera_follow,
            base_scale,
            zoom_power,
            drift,
        }
    }
}

fn bounds_from_points(points: impl IntoIterator<Item = Vec2>) -> Option<Rect> {
    let mut points = points.into_iter();
    let first = points.next()?;
    let (min, max) =
        points.fold((first, first), |(min, max), point| (min.min(point), max.max(point)));
    Some(Rect::from_corners(min, max))
}

/// Computes the legal camera-center range for a viewport and collection of world centers.
///
/// At either limit, the matching outermost world sits 20% inside the viewport. If the galaxy is
/// already narrower than the resulting visible span on an axis, that axis stays centered instead.
fn camera_center_bounds(points: impl IntoIterator<Item = Vec2>, view_size: Vec2) -> Option<Rect> {
    let world_bounds = bounds_from_points(points)?;
    let center_inset = view_size.abs() * (0.5 - GALAXY_EDGE_SCREEN_FRACTION);
    let proposed_min = world_bounds.min + center_inset;
    let proposed_max = world_bounds.max - center_inset;
    let center = world_bounds.center();
    let min = Vec2::new(
        if proposed_min.x <= proposed_max.x {
            proposed_min.x
        } else {
            center.x
        },
        if proposed_min.y <= proposed_max.y {
            proposed_min.y
        } else {
            center.y
        },
    );
    let max = Vec2::new(
        if proposed_min.x <= proposed_max.x {
            proposed_max.x
        } else {
            center.x
        },
        if proposed_min.y <= proposed_max.y {
            proposed_max.y
        } else {
            center.y
        },
    );
    Some(Rect::from_corners(min, max))
}

fn map_camera_bounds(map: &Map, view_size: Vec2) -> Option<Rect> {
    camera_center_bounds(map.planets.iter().map(|planet| planet.position), view_size)
}

fn rubber_band_axis(value: f32, min: f32, max: f32, limit: f32) -> f32 {
    let compress = |distance: f32| {
        if limit > 0.0 {
            limit * distance / (limit + distance)
        } else {
            0.0
        }
    };
    if value < min {
        min - compress(min - value)
    } else if value > max {
        max + compress(value - max)
    } else {
        value
    }
}

fn rubber_band_position(position: Vec2, bounds: Rect, view_size: Vec2) -> Vec2 {
    let limit = view_size.abs() * OVERSCROLL_SCREEN_FRACTION;
    Vec2::new(
        rubber_band_axis(position.x, bounds.min.x, bounds.max.x, limit.x),
        rubber_band_axis(position.y, bounds.min.y, bounds.max.y, limit.y),
    )
}

/// Applies a map-drag delta with progressively stronger resistance beyond the galaxy boundary.
pub(crate) fn drag_camera_position(
    position: Vec2,
    movement: Vec2,
    view_size: Vec2,
    map: &Map,
) -> Vec2 {
    let proposed = position + movement;
    map_camera_bounds(map, view_size)
        .map_or(proposed, |bounds| rubber_band_position(proposed, bounds, view_size))
}

fn settle_position(position: Vec2, bounds: Rect, delta_seconds: f32) -> Vec2 {
    let target = position.clamp(bounds.min, bounds.max);
    if position.distance_squared(target) < 0.01 {
        target
    } else {
        let fraction = 1.0 - (-BOUNDS_RETURN_RATE * delta_seconds.max(0.0)).exp();
        position.lerp(target, fraction.clamp(0.0, 1.0))
    }
}

/// Creates the camera entities and resources required on state entry.
pub fn setup_camera(mut commands: Commands) {
    commands.spawn((Camera2d, Msaa::Off, MainCamera));
}

/// Applies cursor drag and wheel zoom while respecting map bounds.
pub fn move_camera(
    mut context: EguiContexts,
    camera_q: Single<
        (&Camera, &GlobalTransform, &mut Transform, &mut Projection),
        With<MainCamera>,
    >,
    planet_q: Query<(&Transform, &PlanetCmp), (Without<MainCamera>, Without<ParallaxCmp>)>,
    map: Res<Map>,
    mut state: ResMut<UiState>,
    mut scroll_msg: MessageReader<MouseWheel>,
    window: Single<&Window>,
) {
    let (camera, global_t, mut camera_t, mut projection) = camera_q.into_inner();

    let Projection::Orthographic(projection) = &mut *projection else {
        return;
    };

    // Ignore scrolling if pointer is over UI
    let pointer_over_ui = context.ctx_mut().is_ok_and(|ctx| ctx.is_pointer_over_egui());
    if !pointer_over_ui {
        for ev in scroll_msg.read() {
            // Get cursor position in window space
            if let Some(cursor_pos) = window.cursor_position() {
                // Convert to world space
                if let Ok(world_pos) = camera.viewport_to_world_2d(global_t, cursor_pos) {
                    let scale_change = if ev.y > 0. {
                        1. / ZOOM_FACTOR
                    } else {
                        ZOOM_FACTOR
                    };

                    let new_scale = (projection.scale * scale_change).clamp(MIN_ZOOM, MAX_ZOOM);

                    // Adjust camera position to keep focus on the cursor
                    let shift = (world_pos - camera_t.translation.truncate())
                        * (1. - new_scale / projection.scale);
                    camera_t.translation += shift.extend(0.);

                    projection.scale = new_scale;
                    state.to_selected = false;
                    state.focus_planet = None;
                }
            }
        }
    }

    let mut position = camera_t.translation.truncate();

    // Move camera on top of selected planet
    let mut shortcut_target = None;
    if state.to_selected {
        if let Some(planet_id) = state.planet_selected.or(state.focus_planet) {
            if let Some((pos, _)) = planet_q.iter().find(|(_, p)| p.id == planet_id) {
                let planet_position = pos.translation.truncate();
                let target = map_camera_bounds(&map, projection.area.size())
                    .map_or(planet_position, |bounds| {
                        planet_position.clamp(bounds.min, bounds.max)
                    });
                position = position.lerp(target, LERP_FACTOR);
                if state.planet_selected.is_none() && state.focus_planet == Some(planet_id) {
                    shortcut_target = Some(target);
                }
            }
        }
    }

    camera_t.translation = position.extend(camera_t.translation.z);
    if shortcut_target.is_some_and(|target| position.distance(target) < 0.75) {
        state.to_selected = false;
        state.focus_planet = None;
    }
}

/// Applies the elastic camera boundary after every movement input for the frame.
///
/// Dragging may travel a short distance beyond the normal range. Releasing the mouse returns the
/// camera smoothly until the outermost world is again at least 20% inside the viewport edge.
pub fn clamp_camera_to_worlds(
    mut camera_q: Query<(&mut Transform, &Projection), With<MainCamera>>,
    map: Res<Map>,
    mouse: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
) {
    let Ok((mut camera_t, projection)) = camera_q.single_mut() else {
        return;
    };
    let Projection::Orthographic(projection) = projection else {
        return;
    };
    let view_size = projection.area.size();
    let Some(bounds) = map_camera_bounds(&map, view_size) else {
        return;
    };
    let position = camera_t.translation.truncate();
    let bounded = if mouse.pressed(MouseButton::Left) {
        let overscroll = view_size.abs() * OVERSCROLL_SCREEN_FRACTION;
        position.clamp(bounds.min - overscroll, bounds.max + overscroll)
    } else {
        settle_position(position, bounds, time.delta_secs())
    };
    camera_t.translation = bounded.extend(camera_t.translation.z);
}

/// Moves the strategic camera from keyboard input using frame time.
pub fn move_camera_keyboard(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera_q: Query<(&mut Transform, &Projection), With<MainCamera>>,
    mut state: ResMut<UiState>,
    time: Res<Time>,
) {
    let Ok((mut camera_t, projection)) = camera_q.single_mut() else {
        return;
    };

    let scale = if let Projection::Orthographic(projection) = projection {
        projection.scale
    } else {
        1.0
    };

    // Move at 600 screen pixels per second without changing speed on high-refresh displays
    // or after a slow frame.
    let transform = 600. * scale * time.delta_secs();
    if keyboard.pressed(KeyCode::KeyA) {
        camera_t.translation.x -= transform;
        state.to_selected = false;
        state.focus_planet = None;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        camera_t.translation.x += transform;
        state.to_selected = false;
        state.focus_planet = None;
    }
    if keyboard.pressed(KeyCode::KeyW) {
        camera_t.translation.y += transform;
        state.to_selected = false;
        state.focus_planet = None;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        camera_t.translation.y -= transform;
        state.to_selected = false;
        state.focus_planet = None;
    }
}

fn parallax_state(
    parallax: &ParallaxCmp,
    camera_position: Vec2,
    zoom: f32,
    elapsed: f32,
) -> (Vec2, f32) {
    (
        camera_position * parallax.camera_follow + parallax.drift * elapsed,
        parallax.base_scale * zoom.powf(parallax.zoom_power),
    )
}

/// Updates all map depth planes after camera input has been applied for this frame.
pub fn update_parallax(
    camera_q: Single<(&Transform, &Projection), With<MainCamera>>,
    mut parallax_q: Query<(&ParallaxCmp, &mut Transform), Without<MainCamera>>,
    time: Res<Time>,
) {
    let (camera_t, projection) = camera_q.into_inner();
    let scale = if let Projection::Orthographic(projection) = projection {
        projection.scale
    } else {
        1.0
    };
    let elapsed = time.elapsed_secs_f64() as f32;

    for (parallax, mut transform) in &mut parallax_q {
        let (position, layer_scale) =
            parallax_state(parallax, camera_t.translation.truncate(), scale, elapsed);
        transform.translation.x = position.x;
        transform.translation.y = position.y;
        transform.scale = Vec3::splat(layer_scale);
    }
}

/// Restores the strategic camera transform and orthographic scale on game exit.
pub fn reset_camera(mut camera_q: Query<(&mut Transform, &mut Projection), With<MainCamera>>) {
    let Ok((mut camera_t, mut projection)) = camera_q.single_mut() else {
        return;
    };
    camera_t.translation = Vec3::new(0., 0., 1.);

    if let Projection::Orthographic(projection) = &mut *projection {
        projection.scale = 1.;
    }
}

#[cfg(test)]
#[path = "../../tests/core/camera.rs"]
mod tests;
