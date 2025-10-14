use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{BACKGROUND_Z, BUTTON_TEXT_SIZE, PLANET_Z, TITLE_TEXT_SIZE};
use crate::core::map::icon::Icon;
use crate::core::map::map::{Map, MapCmp};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::utils::cursor;
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::turns::NextTurnMsg;
use crate::core::ui::systems::UiState;
use crate::core::units::missions::Mission;
use crate::utils::NameFromEnum;
use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use strum::IntoEnumIterator;

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
        .observe(cursor::<Over>(SystemCursorIcon::Default))
        .observe(
            |event: On<Pointer<Press>>,
             mut commands: Commands,
             window_e: Single<Entity, With<Window>>| {
                if event.button == PointerButton::Primary {
                    commands.entity(*window_e).insert(CursorIcon::from(SystemCursorIcon::Grabbing));
                }
            },
        )
        .observe(cursor::<Release>(SystemCursorIcon::Default))
        .observe(
            |event: On<Pointer<Move>>,
             camera_q: Single<(&mut Transform, &Projection), With<MainCamera>>,
             mut state: ResMut<UiState>,
             mouse: Res<ButtonInput<MouseButton>>,
             window: Single<&CursorIcon, With<Window>>| {
                if mouse.pressed(MouseButton::Left)
                    && matches!(*window, CursorIcon::System(SystemCursorIcon::Grabbing))
                {
                    let (mut camera_t, projection) = camera_q.into_inner();

                    let Projection::Orthographic(projection) = projection else {
                        panic!("Expected Orthographic projection");
                    };

                    if !event.delta.x.is_nan() && !event.delta.y.is_nan() {
                        camera_t.translation.x -= event.delta.x * projection.scale;
                        camera_t.translation.y += event.delta.y * projection.scale;
                        state.to_selected = false;
                    }
                }
            },
        )
        .observe(|_: On<Pointer<Click>>, mut state: ResMut<UiState>| {
            state.selected_planet = None;
            state.mission = false;
        });

    for planet in &map.planets {
        let planet_id = planet.id;
        let owner = planet.owner;

        commands
            .spawn((
                Sprite {
                    image: assets.image(format!(
                        "planet{}",
                        if planet.is_destroyed {
                            0
                        } else {
                            planet.image
                        }
                    )),
                    custom_size: Some(Vec2::splat(Planet::SIZE)),
                    ..default()
                },
                Transform {
                    translation: planet.position.extend(PLANET_Z),
                    ..default()
                },
                Pickable::default(),
                PlanetCmp::new(planet.id),
                MapCmp,
            ))
            .observe(cursor::<Over>(SystemCursorIcon::Pointer))
            .observe(cursor::<Out>(SystemCursorIcon::Default))
            .observe(move |_: On<Pointer<Over>>, mut state: ResMut<UiState>| {
                state.hovered_planet = Some(planet_id);
            })
            .observe(|_: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                state.hovered_planet = None;
            })
            .observe(
                move |event: On<Pointer<Click>>,
                      mut state: ResMut<UiState>,
                      map: Res<Map>,
                      player: Res<Player>| {
                    if event.button == PointerButton::Primary {
                        // Only owned planets can be selected
                        if owner == Some(player.id) {
                            state.selected_planet = Some(planet_id);
                            state.to_selected = true;
                            state.mission_info.origin = planet_id;
                        }
                    } else {
                        state.mission = true;
                        state.mission_info.origin = state
                            .selected_planet
                            .filter(|&p| map.get(p).owner == Some(player.id))
                            .unwrap_or(player.home_planet);
                        state.mission_info.destination = planet_id;
                    }
                },
            )
            .with_children(|parent| {
                // Destroyed planets have no resources nor icons
                if !planet.is_destroyed {
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

                    for (i, icon) in Icon::iter()
                        .filter(|icon| player.controls(&planet) == icon.on_own_planet())
                        .enumerate()
                    {
                        parent
                            .spawn((
                                Sprite {
                                    image: assets.image(icon.to_lowername().as_str()),
                                    custom_size: Some(Vec2::splat(Icon::SIZE)),
                                    ..default()
                                },
                                Transform::from_translation(Vec3::new(
                                    Planet::SIZE * 0.4,
                                    Planet::SIZE * 0.35 - i as f32 * Icon::SIZE,
                                    0.8,
                                )),
                                Pickable::default(),
                                icon.clone(),
                            ))
                            .observe(cursor::<Over>(SystemCursorIcon::Pointer))
                            .observe(cursor::<Out>(SystemCursorIcon::Default))
                            .observe(move |_: On<Pointer<Over>>, mut state: ResMut<UiState>| {
                                state.hovered_planet = Some(planet_id);
                            })
                            .observe(|_: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                                state.hovered_planet = None;
                            })
                            .observe(
                                move |event: On<Pointer<Click>>,
                                      mut state: ResMut<UiState>,
                                      map: Res<Map>,
                                      player: Res<Player>| {
                                    if event.button == PointerButton::Primary {
                                        if icon.on_units() {
                                            state.selected_planet = Some(planet_id);
                                            state.mission = false;
                                            state.shop = icon.shop();
                                        } else {
                                            // The origin is determined as follows: the selected
                                            // planet if owned and fulfills condition, else the
                                            // first planet of the player that fulfills condition
                                            state.mission = true;
                                            state.mission_info = Mission {
                                                objective: icon,
                                                origin: state
                                                    .selected_planet
                                                    .filter(|&id| icon.condition(map.get(id)))
                                                    .unwrap_or(
                                                        player
                                                            .planets(&map.planets)
                                                            .iter()
                                                            .find_map(|p| {
                                                                icon.condition(p).then_some(p.id)
                                                            })
                                                            .unwrap(),
                                                    ),
                                                destination: planet_id,
                                                ..state.mission_info.clone()
                                            };
                                        }
                                    }
                                },
                            );
                    }

                    for (i, resource) in ResourceName::iter().enumerate() {
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
    let texture = assets.texture("long button");
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
                texture.image.clone(),
                TextureAtlas {
                    layout: texture.layout.clone(),
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
        .observe(cursor::<Over>(SystemCursorIcon::Pointer))
        .observe(
            |_: On<Pointer<Over>>, button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                set_button_index(&mut button_q.into_inner(), 1);
            },
        )
        .observe(|_: On<Pointer<Out>>, button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
            set_button_index(&mut button_q.into_inner(), 0);
        })
        .observe(
            |_: On<Pointer<Press>>, button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                set_button_index(&mut button_q.into_inner(), 0);
            },
        )
        .observe(
            |_: On<Pointer<Release>>, button_q: Single<&mut ImageNode, With<EndTurnButtonCmp>>| {
                set_button_index(&mut button_q.into_inner(), 1);
            },
        )
        .observe(
            |_: On<Pointer<Click>>,
             mut state: ResMut<UiState>,
             mut next_turn_ev: MessageWriter<NextTurnMsg>| {
                state.end_turn = true;
                next_turn_ev.write(NextTurnMsg);
            },
        );
}

pub fn update_planet_info(
    planet_q: Query<(Entity, &PlanetCmp)>,
    mut icon_q: Query<(&mut Visibility, &mut Transform, &Icon)>,
    mut show_q: Query<&mut Visibility, (With<ShowOnHoverCmp>, Without<Icon>)>,
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
                let visible = match icon {
                    Icon::Attacked => true,
                    Icon::Buildings | Icon::Fleet | Icon::Defenses => {
                        selected || icon.condition(planet)
                    },
                    _ => {
                        // Show icon if there is a mission with this objective towards this
                        // planet or, if there's selected planet, it fulfills the condition,
                        // else if any of the player's planets fulfills the condition
                        let has_mission = player
                            .missions
                            .iter()
                            .any(|m| m.objective == *icon && m.destination == planet.id);

                        let has_condition = selected && {
                            if let Some(id) = state.selected_planet {
                                let p = map.get(id);
                                p.id != planet.id && icon.condition(p)
                            } else {
                                player
                                    .planets(&map.planets)
                                    .iter()
                                    .any(|p| p.id != planet.id && icon.condition(p))
                            }
                        };

                        has_mission || has_condition
                    },
                };

                *icon_v = if visible {
                    icon_t.translation.y = Planet::SIZE * 0.35 - count as f32 * Icon::SIZE;
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
