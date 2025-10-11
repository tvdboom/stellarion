use crate::core::constants::{LERP_FACTOR, MAX_ZOOM, MIN_ZOOM, ZOOM_FACTOR};
use crate::core::map::map::Map;
use bevy::input::mouse::MouseWheel;
use bevy::prelude::*;

#[derive(Component)]
pub struct MainCamera;

#[derive(Component)]
pub struct ParallaxCmp;

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

pub fn setup_camera(mut commands: Commands) {
    commands.spawn((
        Camera2d,
        IsDefaultUiCamera,
        Msaa::Off, // Solves white lines on map issue (partially)
        MainCamera,
    ));
}

pub fn move_camera(
    map: Res<Map>,
    camera_q: Single<
        (&Camera, &GlobalTransform, &mut Transform, &mut Projection),
        With<MainCamera>,
    >,
    mut parallax_q: Query<&mut Transform, (With<ParallaxCmp>, Without<MainCamera>)>,
    mut scroll_ev: EventReader<MouseWheel>,
    window: Single<&Window>,
) {
    let (camera, global_t, mut camera_t, mut projection) = camera_q.into_inner();

    let Projection::Orthographic(projection) = &mut *projection else {
        panic!("Expected Orthographic projection");
    };

    for ev in scroll_ev.read() {
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
            }
        }
    }

    let mut position = camera_t.translation.truncate();

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

    for mut parallax_t in parallax_q.iter_mut() {
        parallax_t.translation.x = camera_t.translation.x / 1.2;
        parallax_t.translation.y = camera_t.translation.y / 1.2;

        parallax_t.scale = 0.6 * camera_t.scale.powf(0.8);
    }
}

pub fn move_camera_keyboard(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera_q: Query<(&mut Transform, &Projection), With<MainCamera>>,
) {
    let (mut camera_t, projection) = camera_q.single_mut().unwrap();

    let scale = if let Projection::Orthographic(projection) = &projection {
        projection.scale
    } else {
        1.0
    };

    let transform = 10. * scale;
    if keyboard.pressed(KeyCode::KeyA) {
        camera_t.translation.x -= transform;
    }
    if keyboard.pressed(KeyCode::KeyD) {
        camera_t.translation.x += transform;
    }
    if keyboard.pressed(KeyCode::KeyW) {
        camera_t.translation.y += transform;
    }
    if keyboard.pressed(KeyCode::KeyS) {
        camera_t.translation.y -= transform;
    }
}

pub fn reset_camera(mut camera_q: Query<(&mut Transform, &mut Projection), With<MainCamera>>) {
    let (mut camera_t, mut projection) = camera_q.single_mut().unwrap();
    camera_t.translation = Vec3::new(0., 0., 1.);

    if let Projection::Orthographic(projection) = &mut *projection {
        projection.scale = 1.;
    }
}
