use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{PLANET_Z, SUBTITLE_TEXT_SIZE};
use crate::core::game_settings::GameSettings;
use crate::core::map::map::{Map, MapCmp, Planet};
use crate::core::map::utils::{on_out, on_over, Hovered};
use crate::core::menu::buttons::MenuCmp;
use crate::core::player::Player;
use crate::core::resources::ResourceCmp;
use crate::utils::NameFromEnum;
use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use std::fmt::Debug;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(Component, EnumIter, Clone, Debug)]
pub enum PlanetIcon {
    Attack,
    Spy,
    Owned,
    Defend,
    Fleet,
}

#[derive(Component)]
pub struct ShowOnHoverCmp;

pub fn draw_map(
    mut commands: Commands,
    map: Res<Map>,
    player: Res<Player>,
    camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
    assets: Local<WorldAssets>,
) {
    let (mut camera_t, mut projection) = camera.into_inner();
    let Projection::Orthographic(projection) = &mut *projection else {
        panic!("Expected Orthographic projection");
    };

    commands.spawn((
        Sprite::from_image(assets.image("bg")),
        ParallaxCmp,
        Pickable::IGNORE,
        MenuCmp,
    ));

    let texture = assets.texture("planets");
    for planet in &map.planets {
        commands
            .spawn((
                if planet.is_destroyed {
                    Sprite {
                        image: assets.image("destroyed"),
                        custom_size: Some(Vec2::splat(Planet::SIZE)),
                        ..default()
                    }
                } else {
                    Sprite {
                        image: texture.image.clone_weak(),
                        custom_size: Some(Vec2::splat(Planet::SIZE)),
                        texture_atlas: Some(TextureAtlas {
                            layout: texture.layout.clone_weak(),
                            index: planet.image,
                        }),
                        ..default()
                    }
                },
                Transform {
                    translation: planet.position.extend(PLANET_Z),
                    ..default()
                },
                planet.clone(),
                Pickable::default(),
                MapCmp,
            ))
            .observe(on_over)
            .observe(on_out)
            .with_children(|parent| {
                parent.spawn((
                    Text2d::new(&planet.name),
                    TextFont {
                        font: assets.font("bold"),
                        font_size: SUBTITLE_TEXT_SIZE,
                        ..default()
                    },
                    TextColor(WHITE.into()),
                    Transform::from_xyz(-5., Planet::SIZE * 0.6, 0.9),
                    Pickable::IGNORE,
                    ShowOnHoverCmp,
                ));

                if !planet.is_destroyed {
                    for icon in PlanetIcon::iter() {
                        parent.spawn((
                            Sprite {
                                image: assets.image(icon.to_lowername().as_str()),
                                custom_size: Some(Vec2::splat(Planet::SIZE * 0.2)),
                                ..default()
                            },
                            Transform::from_translation(Vec3::new(
                                Planet::SIZE
                                    * (0.35
                                        - match icon {
                                            PlanetIcon::Attack | PlanetIcon::Owned => 0,
                                            PlanetIcon::Spy | PlanetIcon::Defend => 1,
                                            PlanetIcon::Fleet => 2,
                                        } as f32),
                                Planet::SIZE * 0.45,
                                0.8,
                            )),
                            Pickable::IGNORE,
                            icon,
                        ));
                    }

                    for (i, resource) in ResourceCmp::iter().take(3).enumerate() {
                        parent
                            .spawn((
                                Sprite {
                                    image: assets.image(resource.to_lowername().as_str()),
                                    custom_size: Some(Vec2::new(
                                        Planet::SIZE * 0.45,
                                        Planet::SIZE * 0.3,
                                    )),
                                    ..default()
                                },
                                Transform {
                                    translation: Vec3::new(
                                        -Planet::SIZE,
                                        Planet::SIZE * (0.27 - i as f32 * 0.25),
                                        0.7,
                                    ),
                                    scale: Vec3::splat(0.6),
                                    ..default()
                                },
                                Pickable::IGNORE,
                                ShowOnHoverCmp,
                            ))
                            .with_children(|parent| {
                                parent.spawn((
                                    Text2d::new(planet.resources.get(&resource).to_string()),
                                    TextFont {
                                        font: assets.font("bold"),
                                        font_size: 25.,
                                        ..default()
                                    },
                                    TextColor(WHITE.into()),
                                    Transform::from_xyz(55., 0., 0.),
                                ));
                            });
                    }
                }
            });

        if planet.id == *player.planets.first().unwrap() {
            // Place the camera on top of the player's home planet
            projection.scale = 0.8; // Increase zoom
            camera_t.translation = planet.position.extend(camera_t.translation.z);
        }
    }
}

pub fn update_planet_info(
    planet_q: Query<(Entity, Option<&Hovered>, &Planet)>,
    mut icon_q: Query<(&mut Visibility, &PlanetIcon)>,
    mut show_q: Query<&mut Visibility, (With<ShowOnHoverCmp>, Without<PlanetIcon>)>,
    children_q: Query<&Children>,
    player: Res<Player>,
    settings: Res<GameSettings>,
) {
    // Update visibility of planet icons
    for (planet_e, _, planet) in &planet_q {
        for child in children_q.iter_descendants(planet_e) {
            if let Ok((mut visibility, icon)) = icon_q.get_mut(child) {
                let visible = match icon {
                    PlanetIcon::Attack => false, // TODO: Implement attack logic
                    PlanetIcon::Defend => false, // TODO: Implement defend logic
                    PlanetIcon::Fleet => false,  // TODO: Implement fleet logic
                    PlanetIcon::Owned => player.planets.contains(&planet.id),
                    PlanetIcon::Spy => false, // TODO: Implement spy logic
                };

                *visibility = if visible {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }

    // Update visibility of planet info
    for (planet_e, hovered, _) in &planet_q {
        for child in children_q.iter_descendants(planet_e) {
            if let Ok(mut visibility) = show_q.get_mut(child) {
                *visibility = if hovered.is_some() || settings.show_info {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}
