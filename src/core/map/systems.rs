use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{BACKGROUND_Z, PLANET_Z, TITLE_TEXT_SIZE};
use crate::core::map::map::{Map, MapCmp};
use crate::core::map::planet::Planet;
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::ui::systems::{UiState, Unit};
use crate::core::units::defense::Defense;
use crate::core::units::missions::Objective;
use crate::core::units::ships::Ship;
use crate::utils::NameFromEnum;
use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use std::fmt::Debug;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

#[derive(Component, EnumIter, Clone, Debug)]
pub enum PlanetIcon {
    Attacked,
    Buildings,
    Fleet,
    Defense,
    Transport,
    Colonize,
    Attack,
    Spy,
    Bomb,
    Destroy,
}

impl PlanetIcon {
    pub const SIZE: f32 = Planet::SIZE * 0.2;

    pub fn is_friendly_icon(&self) -> bool {
        matches!(
            self,
            PlanetIcon::Attacked
                | PlanetIcon::Buildings
                | PlanetIcon::Fleet
                | PlanetIcon::Defense
                | PlanetIcon::Transport
        )
    }
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

    commands
        .spawn((
            Sprite::from_image(assets.image("bg")),
            Transform::from_xyz(0., 0., BACKGROUND_Z),
            Pickable::default(),
            ParallaxCmp,
            MapCmp,
        ))
        .observe(|_: Trigger<Pointer<Click>>, mut state: ResMut<UiState>| {
            state.selected_planet = None;
        });

    let texture = assets.texture("planets");
    for planet in &map.planets {
        let planet_id = planet.id;

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
            .observe(move |_: Trigger<Pointer<Over>>, mut state: ResMut<UiState>| {
                state.hovered_planet = Some(planet_id);
                state.selected_planet = None;
            })
            .observe(move |_: Trigger<Pointer<Out>>, mut state: ResMut<UiState>| {
                state.hovered_planet = None;
            })
            .observe(move |_: Trigger<Pointer<Click>>, mut state: ResMut<UiState>| {
                state.selected_planet = Some(planet_id);
            })
            .with_children(|parent| {
                parent.spawn((
                    Text2d::new(&planet.name),
                    TextFont {
                        font: assets.font("bold"),
                        font_size: TITLE_TEXT_SIZE,
                        ..default()
                    },
                    TextColor(WHITE.into()),
                    Transform::from_xyz(-4., Planet::SIZE * 0.6, 0.9),
                    Pickable::IGNORE,
                    ShowOnHoverCmp,
                ));

                for (i, icon) in PlanetIcon::iter()
                    .filter(|icon| player.controls(&planet.id) == icon.is_friendly_icon())
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
                            icon.clone(),
                        ))
                        .observe(move |_: Trigger<Pointer<Over>>, mut state: ResMut<UiState>| {
                            state.hovered_planet = Some(planet_id);
                            state.selected_planet = None;
                        })
                        .observe(move |_: Trigger<Pointer<Out>>, mut state: ResMut<UiState>| {
                            state.hovered_planet = None;
                        })
                        .observe(move |_: Trigger<Pointer<Click>>, mut state: ResMut<UiState>| {
                            state.selected_planet = Some(planet_id);
                            if let PlanetIcon::Buildings | PlanetIcon::Fleet | PlanetIcon::Defense =
                                icon
                            {
                                state.unit = match icon {
                                    PlanetIcon::Buildings => Unit::Building,
                                    PlanetIcon::Fleet => Unit::Fleet,
                                    PlanetIcon::Defense => Unit::Defense,
                                    _ => unreachable!(),
                                };
                            }
                        });
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
    planet_q: Query<(Entity, &Planet)>,
    mut icon_q: Query<(&mut Visibility, &mut Transform, &PlanetIcon)>,
    mut show_q: Query<&mut Visibility, (With<ShowOnHoverCmp>, Without<PlanetIcon>)>,
    children_q: Query<&Children>,
    player: Res<Player>,
    state: Res<UiState>,
    settings: Res<Settings>,
) {
    for (planet_e, planet) in &planet_q {
        let hovered = state.hovered_planet.map(|id| id == planet.id).unwrap_or(false);

        // Show/hide planet icons
        let mut count = 0;
        for child in children_q.iter_descendants(planet_e) {
            if let Ok((mut icon_v, mut icon_t, icon)) = icon_q.get_mut(child) {
                let visible =
                    match icon {
                        PlanetIcon::Attacked => true,
                        PlanetIcon::Buildings => !planet.buildings.is_empty() || hovered,
                        PlanetIcon::Fleet => player.fleets.contains_key(&planet.id) || hovered,
                        PlanetIcon::Defense => player.defenses.contains_key(&planet.id) || hovered,
                        PlanetIcon::Transport => {
                            hovered && player.fleets.keys().any(|&k| k != planet.id)
                        },
                        PlanetIcon::Colonize => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Colonize && m.destination == planet.id
                            }) || (hovered
                                && player
                                    .fleets
                                    .values()
                                    .any(|f| f.0.contains_key(&Ship::ColonyShip)))
                        },
                        PlanetIcon::Attack => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Attack && m.destination == planet.id
                            }) || (hovered && !player.fleets.is_empty())
                        },
                        PlanetIcon::Spy => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Spy && m.destination == planet.id
                            }) || (hovered
                                && player.fleets.values().any(|f| f.0.contains_key(&Ship::Probe)))
                        },
                        PlanetIcon::Bomb => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Bomb && m.destination == planet.id
                            }) || (hovered
                                && player
                                    .defenses
                                    .values()
                                    .any(|b| b.0.contains_key(&Defense::InterplanetaryMissile)))
                        },
                        PlanetIcon::Destroy => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Destroy && m.destination == planet.id
                            }) || (hovered
                                && player.fleets.values().any(|f| f.0.contains_key(&Ship::WarSun)))
                        },
                    };

                *icon_v = if visible {
                    icon_t.translation.y = Planet::SIZE * 0.35 - count as f32 * PlanetIcon::SIZE;
                    count += 1;
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }

            // Show/hide planet resources and name
            if let Ok(mut visibility) = show_q.get_mut(child) {
                *visibility = if hovered || settings.show_info {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}
