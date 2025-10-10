use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{BACKGROUND_Z, BUTTON_TEXT_SIZE, PLANET_Z, TITLE_TEXT_SIZE};
use crate::core::map::map::{Map, MapCmp};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::turns::NextTurnEv;
use crate::core::ui::systems::{Shop, UiState};
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
    Strike,
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
pub struct PlanetCmp {
    pub id: PlanetId,
}

impl PlanetCmp {
    pub fn new(id: PlanetId) -> Self {
        Self {
            id,
        }
    }
}

#[derive(Component)]
pub struct ShowOnHoverCmp;

#[derive(Component)]
pub struct EndTurnButtonCmp;

fn set_button_index(button_q: &mut ImageNode, index: usize) {
    if let Some(texture) = &mut button_q.texture_atlas {
        texture.index = index;
    }
}

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
                PlanetCmp::new(planet.id),
                MapCmp,
            ))
            .observe(move |_: Trigger<Pointer<Over>>, mut state: ResMut<UiState>| {
                state.hovered_planet = Some(planet_id);
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
                    .filter(|icon| player.controls(&planet) == icon.is_friendly_icon())
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
                        })
                        .observe(move |_: Trigger<Pointer<Out>>, mut state: ResMut<UiState>| {
                            state.hovered_planet = None;
                        })
                        .observe(move |_: Trigger<Pointer<Click>>, mut state: ResMut<UiState>| {
                            state.selected_planet = Some(planet_id);
                            match icon {
                                PlanetIcon::Buildings => state.shop = Shop::Buildings,
                                PlanetIcon::Fleet => state.shop = Shop::Ships,
                                PlanetIcon::Defense => state.shop = Shop::Defenses,
                                _ => (),
                            };
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

        if player.controls(planet) {
            // Place the camera on top of the player's home planet
            projection.scale = 0.8; // Increase zoom
            camera_t.translation = planet.position.extend(camera_t.translation.z);
        }
    }

    // Spawn end turn button
    let texture = assets.texture("button");
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Percent(3.),
                right: Val::Percent(3.),
                width: Val::Px(200.),
                height: Val::Px(40.),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                ..default()
            },
            ImageNode::from_atlas_image(
                texture.image.clone_weak(),
                TextureAtlas {
                    layout: texture.layout.clone_weak(),
                    index: 0,
                },
            ),
            Pickable::default(),
            EndTurnButtonCmp,
            MapCmp,
            children![
                Text::new("End Turn"),
                TextFont {
                    font: assets.font("bold"),
                    font_size: BUTTON_TEXT_SIZE,
                    ..default()
                },
            ],
        ))
        .observe(
            move |_: Trigger<Pointer<Over>>,
                  button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                set_button_index(&mut button_q.into_inner(), 1);
            },
        )
        .observe(
            move |_: Trigger<Pointer<Out>>,
                  button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                set_button_index(&mut button_q.into_inner(), 0);
            },
        )
        .observe(
            move |_: Trigger<Pointer<Pressed>>,
                  button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                set_button_index(&mut button_q.into_inner(), 0);
            },
        )
        .observe(
            move |_: Trigger<Pointer<Released>>,
                  button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                set_button_index(&mut button_q.into_inner(), 1);
            },
        )
        .observe(
            move |_: Trigger<Pointer<Click>>,
                  mut state: ResMut<UiState>,
                  mut next_turn_ev: EventWriter<NextTurnEv>| {
                state.end_turn = true;
                next_turn_ev.write(NextTurnEv);
            },
        );
}

pub fn update_planet_info(
    planet_q: Query<(Entity, &PlanetCmp)>,
    mut icon_q: Query<(&mut Visibility, &mut Transform, &PlanetIcon)>,
    mut show_q: Query<&mut Visibility, (With<ShowOnHoverCmp>, Without<PlanetIcon>)>,
    children_q: Query<&Children>,
    map: Res<Map>,
    player: Res<Player>,
    state: Res<UiState>,
    settings: Res<Settings>,
) {
    for (planet_e, planet_c) in &planet_q {
        let planet = map.get(planet_c.id);

        let selected = state
            .hovered_planet
            .or(state.selected_planet)
            .map(|id| id == planet.id)
            .unwrap_or(false);

        // Show/hide planet icons
        let mut count = 0;
        for child in children_q.iter_descendants(planet_e) {
            if let Ok((mut icon_v, mut icon_t, icon)) = icon_q.get_mut(child) {
                let visible =
                    match icon {
                        PlanetIcon::Attacked => true,
                        PlanetIcon::Buildings => selected || !planet.complex.is_empty(),
                        PlanetIcon::Fleet => selected || !planet.fleet.is_empty(),
                        PlanetIcon::Defense => selected || !planet.battery.is_empty(),
                        PlanetIcon::Transport => {
                            selected
                                && map
                                    .planets
                                    .iter()
                                    .filter(|p| p.id != planet.id && p.owner == Some(player.id))
                                    .any(|p| !p.fleet.is_empty())
                        },
                        PlanetIcon::Colonize => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Colonize && m.destination == planet.id
                            }) || (selected
                                && map
                                    .planets
                                    .iter()
                                    .filter(|p| p.owner == Some(player.id))
                                    .any(|p| p.fleet.contains_key(&Ship::ColonyShip)))
                        },
                        PlanetIcon::Attack => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Attack && m.destination == planet.id
                            }) || (selected
                                && map
                                    .planets
                                    .iter()
                                    .filter(|p| p.owner == Some(player.id))
                                    .any(|p| p.fleet.iter().any(|(s, _)| s.is_combat())))
                        },
                        PlanetIcon::Spy => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Spy && m.destination == planet.id
                            }) || (selected
                                && map
                                    .planets
                                    .iter()
                                    .filter(|p| p.owner == Some(player.id))
                                    .any(|p| p.fleet.contains_key(&Ship::Probe)))
                        },
                        PlanetIcon::Strike => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Strike && m.destination == planet.id
                            }) || (selected
                                && map.planets.iter().filter(|p| p.owner == Some(player.id)).any(
                                    |p| p.battery.contains_key(&Defense::InterplanetaryMissile),
                                ))
                        },
                        PlanetIcon::Destroy => {
                            player.missions.iter().any(|m| {
                                m.objective == Objective::Destroy && m.destination == planet.id
                            }) || (selected
                                && map
                                    .planets
                                    .iter()
                                    .filter(|p| p.owner == Some(player.id))
                                    .any(|p| p.fleet.contains_key(&Ship::WarSun)))
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
                *visibility = if selected || settings.show_info {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}
