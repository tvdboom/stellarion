use std::f32::consts::TAU;
use std::fmt::Debug;

use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_tweening::Lens;

use crate::core::assets::WorldAssets;
use crate::core::constants::BUTTON_TEXT_SIZE;
use crate::core::map::map::MapCmp;

#[derive(Component)]
pub struct MainButtonCmp;

#[derive(Component)]
pub struct MainButtonLabelCmp;

pub fn set_button_index(button_q: &mut Query<&mut ImageNode, With<MainButtonCmp>>, index: usize) {
    for mut button in button_q {
        if let Some(texture) = button.texture_atlas.as_mut() {
            texture.index = index;
        }
    }
}

pub fn spawn_main_button<'a>(
    commands: &'a mut Commands,
    text: impl Into<String>,
    assets: &WorldAssets,
) -> EntityCommands<'a> {
    let texture = assets.texture("long button");
    let id = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(30.),
                right: Val::Px(50.),
                width: Val::Px(200.),
                height: Val::Px(40.),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ImageNode::from_atlas_image(
                texture.image.clone(),
                TextureAtlas {
                    layout: texture.atlas.layout.clone(),
                    index: 0,
                },
            ),
            Pickable::default(),
            MainButtonCmp,
            MapCmp,
            children![(
                Text::new(text),
                TextFont {
                    font: assets.font("bold"),
                    font_size: BUTTON_TEXT_SIZE,
                    ..default()
                },
                MainButtonLabelCmp,
            )],
        ))
        .observe(cursor::<Over>(SystemCursorIcon::Pointer))
        .observe(cursor::<Out>(SystemCursorIcon::Default))
        .observe(
            |_: On<Pointer<Over>>, mut button_q: Query<&mut ImageNode, With<MainButtonCmp>>| {
                set_button_index(&mut button_q, 1);
            },
        )
        .observe(|_: On<Pointer<Out>>, mut button_q: Query<&mut ImageNode, With<MainButtonCmp>>| {
            set_button_index(&mut button_q, 0);
        })
        .observe(
            |_: On<Pointer<Press>>, mut button_q: Query<&mut ImageNode, With<MainButtonCmp>>| {
                set_button_index(&mut button_q, 0);
            },
        )
        .observe(
            |_: On<Pointer<Release>>, mut button_q: Query<&mut ImageNode, With<MainButtonCmp>>| {
                set_button_index(&mut button_q, 1);
            },
        )
        .id();

    commands.entity(id)
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
pub struct TransformOrbitLens {
    pub radius: f32,
    pub offset: f32,
}

impl Lens<Transform> for TransformOrbitLens {
    fn lerp(&mut self, mut target: Mut<Transform>, ratio: f32) {
        let angle = self.offset + TAU * ratio;
        target.translation.x = self.radius * angle.cos();
        target.translation.y = self.radius * angle.sin();
    }
}

/// Tween: sprite texture cycle
#[derive(Debug, Clone, Copy)]
pub struct SpriteFrameLens(pub usize);

impl Lens<Sprite> for SpriteFrameLens {
    fn lerp(&mut self, mut target: Mut<Sprite>, ratio: f32) {
        if let Some(texture) = &mut target.texture_atlas {
            texture.index = (ratio * self.0 as f32) as usize % self.0;
        }
    }
}
