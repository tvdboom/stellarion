use crate::core::resources::Resources;
use bevy::prelude::*;

#[derive(Component)]
pub struct NoRotationChildCmp;

#[derive(Component)]
pub struct NoRotationParentCmp;

pub trait Description {
    fn description(&self) -> String;
}

pub trait Price {
    fn price(&self) -> Resources;
}

pub trait Combat {
    fn health(&self) -> usize;
    fn shield(&self) -> usize;
    fn damage(&self) -> usize;
    fn speed(&self) -> f32;
    fn fuel_consumption(&self) -> usize;
}

/// Generic system that despawns all entities with a specific component
pub fn despawn<T: Component>(mut commands: Commands, query_c: Query<Entity, With<T>>) {
    for entity in &query_c {
        commands.entity(entity).try_despawn();
    }
}

// /// Update the transform of children entities that shouldn't inherit the parent's rotation
// pub fn update_transform_no_rotation(
//     mut child_q: Query<(&Parent, &mut Transform), With<NoRotationChildCmp>>,
//     parent_q: Query<&Transform, (With<NoRotationParentCmp>, Without<NoRotationChildCmp>)>,
// ) {
//     for (parent, mut transform) in child_q.iter_mut() {
//         if let Ok(parent_transform) = parent_q.get(parent.get()) {
//             transform.rotation = parent_transform.rotation.inverse();
//         }
//     }
// }
