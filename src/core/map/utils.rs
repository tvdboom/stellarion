use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use std::fmt::Debug;

pub fn cursor<T: Debug + Clone + Reflect>(
    icon: SystemCursorIcon,
) -> impl FnMut(On<Pointer<T>>, Commands, Single<Entity, With<Window>>) {
    move |_: On<Pointer<T>>, mut commands: Commands, window_e: Single<Entity, With<Window>>| {
        commands.entity(*window_e).insert(CursorIcon::from(icon));
    }
}
