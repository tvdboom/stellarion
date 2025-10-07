use bevy::prelude::*;

#[derive(Component)]
pub struct Hovered;

pub fn on_over(trigger: Trigger<Pointer<Over>>, mut commands: Commands) {
    commands.entity(trigger.target()).insert(Hovered);
}

pub fn on_out(trigger: Trigger<Pointer<Out>>, mut commands: Commands) {
    commands.entity(trigger.target()).remove::<Hovered>();
}

/// Generic system that despawns all entities with a specific component
pub fn despawn<T: Component>(mut commands: Commands, query_c: Query<Entity, With<T>>) {
    for entity in &query_c {
        commands.entity(entity).try_despawn();
    }
}
