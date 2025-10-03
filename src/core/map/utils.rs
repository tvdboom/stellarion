use bevy::prelude::*;

#[derive(Component)]
pub struct Hovered;

pub fn on_over(trigger: Trigger<Pointer<Over>>, mut commands: Commands) {
    commands.entity(trigger.target()).insert(Hovered);
}

pub fn on_out(trigger: Trigger<Pointer<Out>>, mut commands: Commands) {
    commands.entity(trigger.target()).remove::<Hovered>();
}
