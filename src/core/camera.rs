//! Strategic camera setup, movement, zoom, clamping, and reset systems.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::core::constants::{LERP_FACTOR, MAX_ZOOM, MIN_ZOOM, ZOOM_FACTOR};
use crate::core::map::model::Map;
use crate::core::map::systems::PlanetCmp;
use crate::core::ui::systems::UiState;

/// Fraction of the full viewport available as elastic drag travel beyond the planet cluster.
const OVERSCROLL_SCREEN_FRACTION: f32 = 0.2;
/// Exponential return speed after input releases the camera outside its limits.
const BOUNDS_RETURN_RATE: f32 = 14.0;
/// Distance from the requested scale at which the zoom snaps to its exact destination.
const FOCUS_ZOOM_EPSILON: f32 = 0.005;

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

fn cross(origin: Vec2, left: Vec2, right: Vec2) -> f32 {
    (left - origin).perp_dot(right - origin)
}

/// Returns the convex outline of the planet centers in counter-clockwise order.
fn planet_hull(points: impl IntoIterator<Item = Vec2>) -> Vec<Vec2> {
    let mut points = points.into_iter().collect::<Vec<_>>();
    points
        .sort_by(|left, right| left.x.total_cmp(&right.x).then_with(|| left.y.total_cmp(&right.y)));
    points.dedup();
    if points.len() <= 2 {
        return points;
    }

    let mut lower = Vec::with_capacity(points.len());
    for &point in &points {
        while lower.len() >= 2
            && cross(lower[lower.len() - 2], lower[lower.len() - 1], point) <= 0.0
        {
            lower.pop();
        }
        lower.push(point);
    }

    let mut upper = Vec::with_capacity(points.len());
    for &point in points.iter().rev() {
        while upper.len() >= 2
            && cross(upper[upper.len() - 2], upper[upper.len() - 1], point) <= 0.0
        {
            upper.pop();
        }
        upper.push(point);
    }

    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

fn closest_point_on_segment(point: Vec2, start: Vec2, end: Vec2) -> Vec2 {
    let segment = end - start;
    let length_squared = segment.length_squared();
    if length_squared <= f32::EPSILON {
        return start;
    }
    start + segment * ((point - start).dot(segment) / length_squared).clamp(0.0, 1.0)
}

/// Keeps a camera center inside the planets' real outline instead of an empty bounding-box corner.
fn clamp_to_planet_hull(position: Vec2, hull: &[Vec2]) -> Option<Vec2> {
    match hull {
        [] => None,
        [point] => Some(*point),
        [start, end] => Some(closest_point_on_segment(position, *start, *end)),
        _ if hull
            .iter()
            .zip(hull.iter().cycle().skip(1))
            .all(|(&start, &end)| cross(start, end, position) >= -f32::EPSILON) =>
        {
            Some(position)
        },
        _ => hull
            .iter()
            .zip(hull.iter().cycle().skip(1))
            .map(|(&start, &end)| closest_point_on_segment(position, start, end))
            .min_by(|left, right| {
                position.distance_squared(*left).total_cmp(&position.distance_squared(*right))
            }),
    }
}

fn map_camera_target(map: &Map, position: Vec2) -> Option<Vec2> {
    let hull = planet_hull(map.planets.iter().map(|planet| planet.position));
    clamp_to_planet_hull(position, &hull)
}

fn clamp_overscroll(offset: Vec2, view_size: Vec2) -> Vec2 {
    let limit = view_size.abs() * OVERSCROLL_SCREEN_FRACTION;
    offset.clamp(-limit, limit)
}

/// Applies a map-drag delta with a 20%-of-screen limit beyond the planet cluster's outline.
pub(crate) fn drag_camera_position(
    position: Vec2,
    movement: Vec2,
    view_size: Vec2,
    map: &Map,
) -> Vec2 {
    let proposed = position + movement;
    map_camera_target(map, proposed)
        .map_or(proposed, |target| target + clamp_overscroll(proposed - target, view_size))
}

fn settle_position(position: Vec2, target: Vec2, delta_seconds: f32) -> Vec2 {
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
                    state.focus_zoom = None;
                }
            }
        }
    }

    let mut position = camera_t.translation.truncate();

    // Move camera on top of selected planet
    let mut shortcut_target = None;
    let mut shortcut_zoom_complete = true;
    if state.to_selected {
        if let Some(planet_id) = state.planet_selected.or(state.focus_planet) {
            if let Some((pos, _)) = planet_q.iter().find(|(_, p)| p.id == planet_id) {
                if let Some(target_scale) = state.focus_zoom {
                    (projection.scale, shortcut_zoom_complete) =
                        advance_focus_zoom(projection.scale, target_scale);
                }
                let planet_position = pos.translation.truncate();
                let target = map_camera_target(&map, planet_position).unwrap_or(planet_position);
                position = position.lerp(target, LERP_FACTOR);
                if state.planet_selected.is_none() && state.focus_planet == Some(planet_id) {
                    shortcut_target = Some(target);
                }
            }
        }
    }

    camera_t.translation = position.extend(camera_t.translation.z);
    if shortcut_zoom_complete
        && shortcut_target.is_some_and(|target| position.distance(target) < 0.75)
    {
        state.to_selected = false;
        state.focus_planet = None;
        state.focus_zoom = None;
    }
}

fn advance_focus_zoom(scale: f32, target: f32) -> (f32, bool) {
    let next = scale + (target - scale) * LERP_FACTOR;
    if (next - target).abs() <= FOCUS_ZOOM_EPSILON {
        (target, true)
    } else {
        (next.clamp(MIN_ZOOM, MAX_ZOOM), false)
    }
}

/// Applies the elastic camera boundary after every movement input for the frame.
///
/// Dragging may travel up to 20% of the full viewport beyond the planet cluster. Releasing the mouse
/// returns the camera smoothly to the closest point within the cluster's convex outline.
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
    let position = camera_t.translation.truncate();
    let Some(target) = map_camera_target(&map, position) else {
        return;
    };
    let limit = view_size.abs() * OVERSCROLL_SCREEN_FRACTION;
    let limited = target + (position - target).clamp(-limit, limit);
    let bounded = if mouse.pressed(MouseButton::Left) {
        limited
    } else {
        settle_position(limited, target, time.delta_secs())
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
        state.focus_zoom = None;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        camera_t.translation.x += transform;
        state.to_selected = false;
        state.focus_planet = None;
        state.focus_zoom = None;
    }
    if keyboard.pressed(KeyCode::KeyW) {
        camera_t.translation.y += transform;
        state.to_selected = false;
        state.focus_planet = None;
        state.focus_zoom = None;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        camera_t.translation.y -= transform;
        state.to_selected = false;
        state.focus_planet = None;
        state.focus_zoom = None;
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
