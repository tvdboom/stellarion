use crate::core::assets::WorldAssets;
use crate::core::constants::{BACKGROUND_Z, PLANET_Z};
use crate::core::map::map::{Map, MapCmp, Planet};
use bevy::prelude::*;

pub fn draw_map(mut commands: Commands, map: Res<Map>, assets: Local<WorldAssets>) {
    commands.spawn((
        Sprite::from_image(assets.image("bg")),
        Transform::from_xyz(0., 0., BACKGROUND_Z),
    ));

    let texture = assets.texture("planets");
    for planet in &map.planets {
        commands.spawn((
            Sprite {
                image: texture.image.clone_weak(),
                custom_size: Some(Vec2::splat(Planet::SIZE)),
                texture_atlas: Some(TextureAtlas {
                    layout: texture.layout.clone_weak(),
                    index: planet.image,
                }),
                ..default()
            },
            Transform {
                translation: planet.position.extend(PLANET_Z),
                ..default()
            },
            planet.clone(),
            MapCmp,
        ));
    }
}
