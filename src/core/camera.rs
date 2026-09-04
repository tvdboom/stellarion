//! Strategic camera setup, movement, zoom, clamping, and reset systems.

use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;
use bevy_egui::EguiContexts;

use crate::core::constants::{LERP_FACTOR, MAX_ZOOM, MIN_ZOOM, ZOOM_FACTOR};
use crate::core::map::model::Map;
use crate::core::map::systems::PlanetCmp;
use crate::core::ui::systems::UiState;

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

/// Clamps camera translation so the visible viewport remains inside the map.
pub fn clamp_to_rect(pos: Vec2, view_size: Vec2, bounds: Rect) -> Vec2 {
    let min_x = bounds.min.x + view_size.x * 0.5;
    let min_y = bounds.min.y + view_size.y * 0.5;
    let max_x = bounds.max.x - view_size.x * 0.5;
    let max_y = bounds.max.y - view_size.y * 0.5;

    if min_x > max_x || min_y > max_y {
        Vec2::new((bounds.min.x + bounds.max.x) * 0.5, (bounds.min.y + bounds.max.y) * 0.5)
    } else {
        Vec2::new(pos.x.clamp(min_x, max_x), pos.y.clamp(min_y, max_y))
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
                let target = pos.translation.truncate();
                position = position.lerp(target, LERP_FACTOR);
                if state.planet_selected.is_none() && state.focus_planet == Some(planet_id) {
                    shortcut_target = Some(target);
                }
            }
        }
    }

    // Compute the camera's current view size based on projection
    let view_size = projection.area.max - projection.area.min;

    // Clamp camera position within bounds
    position = position.lerp(
        clamp_to_rect(
            position,
            view_size,
            Rect {
                min: map.rect.min * 1.8,
                max: map.rect.max * 1.8,
            },
        ),
        LERP_FACTOR,
    );

    camera_t.translation = position.extend(camera_t.translation.z);
    if shortcut_target.is_some_and(|target| position.distance(target) < 0.75) {
        state.to_selected = false;
        state.focus_planet = None;
    }
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

    // Match the old 10-pixels-per-frame feel at 60 FPS without changing speed on high-refresh
    // displays or after a slow frame.
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
