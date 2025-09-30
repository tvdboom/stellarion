use crate::core::constants::{LERP_FACTOR, MAX_ZOOM, MIN_ZOOM, ZOOM_FACTOR};
use crate::core::map::map::Map;
use bevy::input::mouse::{MouseMotion, MouseWheel};
use bevy::prelude::*;
use bevy::window::{SystemCursorIcon};
use bevy::winit::cursor::CursorIcon;

#[derive(Component)]
pub struct MainCamera;

pub fn clamp_to_rect(pos: Vec2, view_size: Vec2, bounds: Rect) -> Vec2 {
    let min_x = bounds.min.x + view_size.x * 0.5;
    let min_y = bounds.min.y + view_size.y * 0.5;
    let max_x = bounds.max.x - view_size.x * 0.5;
    let max_y = bounds.max.y - view_size.y * 0.5;

    if min_x > max_x || min_y > max_y {
        Vec2::new(
            (bounds.min.x + bounds.max.x) * 0.5,
            (bounds.min.y + bounds.max.y) * 0.5,
        )
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

// pub fn move_camera(
//     mut commands: Commands,
//     mut camera_q: Query<
//         (
//             &Camera,
//             &GlobalTransform,
//             &mut Transform,
//             &mut Projection,
//         ),
//         With<MainCamera>,
//     >,
//     mut scroll_ev: EventReader<MouseWheel>,
//     mut motion_ev: EventReader<MouseMotion>,
//     mouse: Res<ButtonInput<MouseButton>>,
//     window: Single<(Entity, &Window)>,
// ) {
//     let (camera, global_t, mut camera_t, mut projection) = camera_q.single_mut().unwrap();
//     let (window_e, window) = *window;
//
//     for ev in scroll_ev.read() {
//         // Get cursor position in window space
//         if let Some(cursor_pos) = window.cursor_position() {
//             // Convert to world space
//             if let Ok(world_pos) = camera.viewport_to_world_2d(global_t, cursor_pos) {
//                 let scale_change = if ev.y > 0. {
//                     1. / ZOOM_FACTOR
//                 } else {
//                     ZOOM_FACTOR
//                 };
//
//                 let new_scale = (projection.scale * scale_change).clamp(MIN_ZOOM, MAX_ZOOM);
//
//                 // Adjust camera position to keep focus on the cursor
//                 let shift = (world_pos - camera_t.translation.truncate())
//                     * (1. - new_scale / projection.scale);
//                 camera_t.translation += shift.extend(0.);
//
//                 projection.scale = new_scale;
//             }
//         }
//     }
//
//     if mouse.pressed(MouseButton::Middle) {
//         commands
//             .entity(window_e)
//             .insert(Into::<CursorIcon>::into(SystemCursorIcon::Grab));
//         for ev in motion_ev.read() {
//             commands
//                 .entity(window_e)
//                 .insert(Into::<CursorIcon>::into(SystemCursorIcon::Grabbing));
//             if ev.delta.x.is_nan() || ev.delta.y.is_nan() {
//                 continue;
//             }
//             camera_t.translation.x -= ev.delta.x * projection.scale;
//             camera_t.translation.y += ev.delta.y * projection.scale;
//         }
//     } else {
//         commands
//             .entity(window_e)
//             .insert(Into::<CursorIcon>::into(SystemCursorIcon::Default));
//     }
//
//     let mut position = camera_t.translation.truncate();
//
//     // Compute the camera's current view size based on projection
//     let view_size = projection.area.max - projection.area.min;
//
//     // Clamp camera position within bounds
//     let target_pos = clamp_to_rect(position, view_size, Rect::new(0., 0., Map::SIZE, Map::SIZE));
//     position = position.lerp(target_pos, LERP_FACTOR);
//
//     // Hard clamp to prevent escaping the map
//     position = clamp_to_rect(position, view_size, Rect::new(Map::SIZE * 0.1, Map::SIZE * 0.1, Map::SIZE * 0.9, Map::SIZE * 0.9));
//
//     camera_t.translation = position.extend(camera_t.translation.z);
// }

pub fn move_camera_keyboard(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut camera_q: Query<(&mut Transform, &Projection), With<MainCamera>>,
) {
    let (mut camera_t, projection) = camera_q.single_mut().unwrap();

    println!("Camera position: {:?}", projection);
    let scale = if let Projection::Orthographic(projection) = &projection {
        projection.scale
    } else {1.0};

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

pub fn reset_camera(
    mut camera_q: Query<(&mut Transform, &mut Projection), With<MainCamera>>,
) {
    let (mut camera_t, mut projection) = camera_q.single_mut().unwrap();
    camera_t.translation = Vec3::new(0., 0., 1.);

    if let Projection::Orthographic(projection) = &mut *projection {
        projection.scale = 1.;
    }
}
