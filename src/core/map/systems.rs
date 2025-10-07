use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{PLANET_Z, SUBTITLE_TEXT_SIZE};
use crate::core::map::map::{Map, MapCmp, Planet};
use crate::core::menu::buttons::MenuCmp;
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::utils::{on_out, on_over, Hovered};
use crate::utils::NameFromEnum;
use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use std::fmt::Debug;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(Component, EnumIter, Clone, Debug)]
pub enum PlanetIcon {
    Attacked,
    Owned,
    Fleet,
    Defense,
    Buildings,
    Attack,
    Spy,
    Destroy,
}

impl PlanetIcon {
    pub const SIZE: f32 = Planet::SIZE * 0.2;
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
                Pickable::default(),
                planet.clone(),
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
                    Transform::from_xyz(-4., Planet::SIZE * 0.6, 0.9),
                    Pickable::IGNORE,
                    ShowOnHoverCmp,
                ));

                for (i, icon) in PlanetIcon::iter()
                    .filter(|icon| {
                        if player.planets.contains(&planet.id) {
                            let attacked = false; // TODO: Implement when attacked logic

                            match icon {
                                PlanetIcon::Attacked => attacked,
                                PlanetIcon::Owned => !attacked,
                                PlanetIcon::Fleet | PlanetIcon::Defense | PlanetIcon::Buildings => {
                                    true
                                },
                                _ => false,
                            }
                        } else {
                            matches!(
                                icon,
                                PlanetIcon::Attack | PlanetIcon::Spy | PlanetIcon::Destroy
                            )
                        }
                    })
                    .enumerate()
                {
                    parent
                        .spawn((
                            Sprite {
                                image: assets.image(icon.to_lowername().as_str()),
                                custom_size: Some(Vec2::splat(PlanetIcon::SIZE)),
                                ..default()
                            },
                            Transform::from_translation(Vec3::new(
                                Planet::SIZE * 0.4,
                                Planet::SIZE * 0.35 - i as f32 * PlanetIcon::SIZE,
                                0.8,
                            )),
                            Pickable::default(),
                            icon,
                        ))
                        .observe(on_over)
                        .observe(on_out);
                }

                // Destroyed planets don't have any resources
                if !planet.is_destroyed {
                    for (i, resource) in ResourceName::iter().take(3).enumerate() {
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
    mut icon_q: Query<(&mut Visibility, &mut Transform, Option<&Hovered>, &PlanetIcon)>,
    mut show_q: Query<&mut Visibility, (With<ShowOnHoverCmp>, Without<PlanetIcon>)>,
    children_q: Query<&Children>,
    player: Res<Player>,
    settings: Res<Settings>,
) {
    // Update visibility of planet icons
    for (planet_e, planet_h, planet) in &planet_q {
        // Check if the planet or any icon is hovered
        let hovered = planet_h.is_some()
            || children_q
                .iter_descendants(planet_e)
                .any(|e| icon_q.get(e).map_or(false, |(_, _, h, i)| h.is_some()));

        let mut count = 0;
        for child in children_q.iter_descendants(planet_e) {
            if let Ok((mut icon_v, mut icon_t, _, icon)) = icon_q.get_mut(child) {
                let visible = match icon {
                    PlanetIcon::Attacked | PlanetIcon::Owned => true,
                    PlanetIcon::Fleet => hovered || player.fleets.contains_key(&planet.id),
                    PlanetIcon::Defense => hovered || player.defenses.contains_key(&planet.id),
                    PlanetIcon::Buildings => hovered || !planet.buildings.is_empty(),
                    _ => hovered,
                };

                *icon_v = if visible {
                    icon_t.translation.y = Planet::SIZE * 0.35 - count as f32 * PlanetIcon::SIZE;
                    count += 1;
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }

    // Update visibility of planet info
    for (planet_e, planet_h, _) in &planet_q {
        for child in children_q.iter_descendants(planet_e) {
            if let Ok(mut visibility) = show_q.get_mut(child) {
                *visibility = if planet_h.is_some() || settings.show_info {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}
