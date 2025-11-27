use std::f32::consts::TAU;
use std::fmt::Debug;

use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_tweening::Lens;

pub fn set_button_index(button_q: &mut ImageNode, index: usize) {
    if let Some(texture) = &mut button_q.texture_atlas {
        texture.index = index;
    }
}

pub fn cursor<T: Debug + Clone + Reflect>(
    icon: SystemCursorIcon,
) -> impl FnMut(On<Pointer<T>>, Commands, Single<Entity, With<Window>>) {
    move |_: On<Pointer<T>>, mut commands: Commands, window_e: Single<Entity, With<Window>>| {
        commands.entity(*window_e).insert(CursorIcon::from(icon));
    }
}

/// Tween: circular motion
#[derive(Debug, Clone, Copy)]
pub struct TransformOrbitLens(pub f32);

impl Lens<Transform> for TransformOrbitLens {
    fn lerp(&mut self, mut target: Mut<Transform>, ratio: f32) {
        let angle = TAU * ratio;
        target.translation.x = self.0 * angle.cos();
        target.translation.y = self.0 * angle.sin();
    }
}

/// Tween: circular motion
#[derive(Debug, Clone, Copy)]
pub struct SpriteFrameLens(pub usize);

impl Lens<Sprite> for SpriteFrameLens {
    fn lerp(&mut self, mut target: Mut<Sprite>, ratio: f32) {
        if let Some(texture) = &mut target.texture_atlas {
            texture.index = (ratio * self.0 as f32) as usize % self.0;
        }
    }
}
