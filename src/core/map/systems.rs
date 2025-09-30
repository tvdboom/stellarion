use bevy::prelude::*;
use crate::core::assets::WorldAssets;
use crate::core::map::map::{Map, MapCmp};

pub fn draw_map(
    mut commands: Commands,
    map: Res<Map>,
    assets: Local<WorldAssets>,
) {
    println!("Drawing map with {} planets", map.planets.len());
    for planet in &map.planets {
        println!("Spawning planet at {:?} with image {}", planet.position, planet.image);
        commands.spawn((
            Sprite {
                image: assets.image(&planet.image),
                custom_size: Some(Vec2::splat(50.)),
                ..default()
            },
           Transform {
                translation: planet.position.extend(0.),
                ..default()
            },
            MapCmp,
        ));
    }
}
