use std::collections::HashMap;

use bevy::asset::RenderAssetUsages;
use bevy::color::palettes::css::WHITE;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use strum::IntoEnumIterator;
use voronator::delaunator::Point;
use voronator::VoronoiDiagram;

use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{
    BACKGROUND_Z, BUTTON_TEXT_SIZE, PLANET_Z, TITLE_TEXT_SIZE, VORONOI_Z,
};
use crate::core::map::icon::Icon;
use crate::core::map::map::{Map, MapCmp};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::utils::cursor;
use crate::core::missions::{Mission, MissionId};
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::turns::NextTurnMsg;
use crate::core::ui::systems::UiState;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::Unit;
use crate::utils::NameFromEnum;

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
pub struct MissionCmp {
    pub id: MissionId,
}

impl MissionCmp {
    pub fn new(id: MissionId) -> Self {
        Self {
            id,
        }
    }
}

#[derive(Component)]
pub struct ShowOnHoverCmp;

#[derive(Component)]
pub struct VoronoiCmp(pub PlanetId);

#[derive(Component)]
pub struct VoronoiEdgeCmp {
    pub planet: PlanetId,
    pub key: usize,
}

#[derive(Component)]
pub struct EndTurnLabelCmp;

#[derive(Component)]
pub struct EndTurnButtonCmp;

#[derive(Component)]
pub struct EndTurnButtonLabelCmp;

fn set_button_index(button_q: &mut ImageNode, index: usize) {
    if let Some(texture) = &mut button_q.texture_atlas {
        texture.index = index;
    }
}

pub fn draw_map(
    mut commands: Commands,
    camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
    map: Res<Map>,
    player: Res<Player>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
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
             mut state: ResMut<UiState>,
             window_e: Single<Entity, With<Window>>| {
                if event.button == PointerButton::Primary {
                    state.planet_selected = None;
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
            state.mission = false;
        });

    for planet in &map.planets {
        let planet_id = planet.id;

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
                state.planet_hover = Some(planet_id);
            })
            .observe(|_: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                state.planet_hover = None;
            })
            .observe(
                move |event: On<Pointer<Click>>,
                      mut state: ResMut<UiState>,
                      map: Res<Map>,
                      player: Res<Player>| {
                    if event.button == PointerButton::Primary {
                        // Only owned planets can be selected
                        if player.owns(map.get(planet_id)) {
                            state.planet_selected = Some(planet_id);
                            state.to_selected = true;
                            state.mission_info.origin = planet_id;
                        }
                    } else {
                        state.mission = true;
                        state.mission_info.origin = state
                            .planet_selected
                            .filter(|&p| player.owns(map.get(p)))
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

                    for (i, icon) in Icon::iter().enumerate() {
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
                            .observe(
                                move |_: On<Pointer<Over>>,
                                      mut state: ResMut<UiState>,
                                      player: Res<Player>| {
                                    state.planet_hover = Some(planet_id);
                                    if let Some(mission) = player
                                        .missions
                                        .iter()
                                        .find(|m| m.destination == planet_id && m.objective == icon)
                                    {
                                        state.mission_hover = Some(mission.id);
                                    }
                                },
                            )
                            .observe(|_: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                                state.planet_hover = None;
                                state.mission_hover = None;
                            })
                            .observe(
                                move |mut event: On<Pointer<Click>>,
                                      mut state: ResMut<UiState>,
                                      map: Res<Map>,
                                      player: Res<Player>| {
                                    // Prevent the event from bubbling up to the planet
                                    event.propagate(false);

                                    if event.button == PointerButton::Primary {
                                        if icon.on_units() && player.owns(map.get(planet_id)) {
                                            state.planet_selected = Some(planet_id);
                                            state.mission = false;
                                            state.shop = icon.shop();
                                        } else if icon == Icon::Fleet {
                                            state.mission = true;
                                            state.mission_info = Mission {
                                                objective: Icon::Deploy,
                                                origin: planet_id,
                                                destination: state
                                                    .planet_selected
                                                    .filter(|&id| player.controls(map.get(id)))
                                                    .unwrap_or(player.home_planet),
                                                ..state.mission_info.clone()
                                            };
                                        } else if icon.is_mission() {
                                            state.mission = true;

                                            // The origin is determined as follows: the selected
                                            // planet if owned and fulfills condition, else the
                                            // first planet of the player that fulfills condition
                                            let origin_id = state
                                                .planet_selected
                                                .filter(|&id| {
                                                    id != planet_id && icon.condition(map.get(id))
                                                })
                                                .unwrap_or(
                                                    map.planets
                                                        .iter()
                                                        .find_map(|p| {
                                                            (p.id != planet_id
                                                                && player.controls(p)
                                                                && icon.condition(p))
                                                            .then_some(p.id)
                                                        })
                                                        .unwrap(),
                                                );

                                            let origin = map.get(origin_id);
                                            state.mission_info = Mission {
                                                objective: icon,
                                                origin: origin_id,
                                                destination: planet_id,
                                                army: match icon {
                                                    Icon::Colonize => HashMap::from([(
                                                        Unit::Ship(Ship::ColonyShip),
                                                        1,
                                                    )]),
                                                    Icon::Spy => HashMap::from([(
                                                        Unit::Ship(Ship::Probe),
                                                        origin.get(&Unit::Ship(Ship::Probe)),
                                                    )]),
                                                    Icon::Attack | Icon::Destroy => origin
                                                        .fleet
                                                        .iter()
                                                        .filter_map(|(s, c)| {
                                                            s.is_combat().then_some((
                                                                Unit::Ship(s.clone()),
                                                                *c,
                                                            ))
                                                        })
                                                        .collect(),
                                                    Icon::MissileStrike => HashMap::from([(
                                                        Unit::Defense(
                                                            Defense::InterplanetaryMissile,
                                                        ),
                                                        origin.get(&Unit::Defense(
                                                            Defense::InterplanetaryMissile,
                                                        )),
                                                    )]),
                                                    Icon::Deploy => origin
                                                        .fleet
                                                        .iter()
                                                        .map(|(s, c)| (Unit::Ship(s.clone()), *c))
                                                        .collect(),
                                                    _ => unreachable!(),
                                                },
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

        if player.owns(planet) {
            // Place the camera on top of the player's home planet
            projection.scale = 0.8; // Increase zoom
            camera_t.translation = planet.position.extend(camera_t.translation.z);
        }
    }

    // Draw Voronoi cells
    if let Some(voronoi) = VoronoiDiagram::<Point>::from_tuple(
        &(-10000., -10000.),
        &(10000., 10000.),
        &map.planets.iter().map(|p| (p.position.x as f64, p.position.y as f64)).collect::<Vec<_>>(),
    ) {
        for (i, cell) in voronoi.cells().iter().enumerate() {
            let planet_id = map.planets[i].id;

            let points = cell.points();
            let n = points.len();

            if n >= 3 {
                let positions = cell
                    .points()
                    .iter()
                    .map(|p| Vec3::new(p.x as f32, p.y as f32, VORONOI_Z))
                    .collect::<Vec<_>>();

                let indices: Vec<u32> =
                    (1..n - 1).flat_map(|i| vec![0u32, i as u32, (i + 1) as u32]).collect();

                let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
                    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
                    .with_inserted_indices(Indices::U32(indices));

                commands.spawn((
                    Mesh2d(meshes.add(mesh)),
                    MeshMaterial2d(materials.add(Color::srgba(0., 0.3, 0.5, 0.05))),
                    Visibility::Hidden,
                    VoronoiCmp(map.planets[i].id),
                ));

                for j in 0..n {
                    let a = points[j];
                    let b = points[(j + 1) % points.len()];
                    let v1 = Vec2::new(a.x as f32, a.y as f32);
                    let v2 = Vec2::new(b.x as f32, b.y as f32);

                    let mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
                        .with_inserted_attribute(
                            Mesh::ATTRIBUTE_POSITION,
                            vec![v1.extend(VORONOI_Z + 0.1), v2.extend(VORONOI_Z + 0.1)],
                        )
                        .with_inserted_indices(Indices::U32(vec![0, 1]));

                    commands.spawn((
                        Mesh2d(meshes.add(mesh)),
                        MeshMaterial2d(materials.add(Color::srgba(0., 0.3, 0.5, 0.5))),
                        Visibility::Hidden,
                        VoronoiEdgeCmp {
                            planet: planet_id,
                            key: v1.distance(v2) as usize,
                        },
                    ));
                }
            }
        }
    }

    // Spawn end turn button
    let texture = assets.texture("long button");
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(35.),
            right: Val::Px(270.),
            ..default()
        },
        Text::new("Waiting for other players to finish their turn..."),
        TextFont {
            font: assets.font("bold"),
            font_size: BUTTON_TEXT_SIZE,
            ..default()
        },
        Visibility::Hidden,
        EndTurnLabelCmp,
    ));

    commands
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
                    layout: texture.layout.clone(),
                    index: 0,
                },
            ),
            Pickable::default(),
            EndTurnButtonCmp,
            MapCmp,
            children![(
                Text::new("End turn"),
                TextFont {
                    font: assets.font("bold"),
                    font_size: BUTTON_TEXT_SIZE,
                    ..default()
                },
                EndTurnButtonLabelCmp,
            )],
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
                state.planet_selected = None;
                state.mission = false;
                state.end_turn = !state.end_turn;
                if state.end_turn {
                    next_turn_ev.write(NextTurnMsg);
                }
            },
        );
}

pub fn update_voronoi(
    mut cell_q: Query<(&mut Visibility, &VoronoiCmp)>,
    mut edge_q: Query<(&mut Visibility, &VoronoiEdgeCmp), Without<VoronoiCmp>>,
    map: Res<Map>,
    player: Res<Player>,
) {
    for (mut cell_v, cell) in &mut cell_q {
        *cell_v = if player.owns(map.get(cell.0)) {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    let counts: HashMap<usize, usize> = edge_q
        .iter()
        .filter_map(|(_, edge)| player.owns(map.get(edge.planet)).then_some(edge.key))
        .fold(HashMap::new(), |mut acc, key| {
            *acc.entry(key).or_default() += 1;
            acc
        });

    for (mut edge_v, edge) in &mut edge_q {
        *edge_v = if *counts.get(&edge.key).unwrap_or(&2) <= 1 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub fn update_end_turn(
    button_q: Single<&mut Text, With<EndTurnButtonLabelCmp>>,
    label_q: Single<&mut Visibility, With<EndTurnLabelCmp>>,
    state: Res<UiState>,
) {
    let mut button_t = button_q.into_inner();
    button_t.0 = if state.end_turn {
        "Continue turn".to_string()
    } else {
        "End turn".to_string()
    };

    let mut label_v = label_q.into_inner();
    *label_v = if state.end_turn {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
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

        let selected =
            state.planet_hover.or(state.planet_selected).map(|id| id == planet.id).unwrap_or(false);

        // Show/hide planet icons
        let mut count = 0;
        for child in children_q.iter_descendants(planet_e) {
            if let Ok((mut icon_v, mut icon_t, icon)) = icon_q.get_mut(child) {
                let visible = match icon {
                    Icon::Attacked => {
                        player.enemy_missions.iter().any(|m| m.destination == planet.id)
                    },
                    Icon::Buildings | Icon::Defenses => {
                        player.owns(planet) && (selected || icon.condition(planet))
                    },
                    Icon::Fleet => {
                        // A player can have a fleet on a not-owned planet
                        player.controls(planet) && (selected || icon.condition(planet))
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
                            if let Some(id) = state.planet_selected {
                                let p = map.get(id);
                                (*icon == Icon::Deploy) == player.controls(planet)
                                    && p.id != planet.id
                                    && icon.condition(p)
                            } else {
                                map.planets.iter().any(|p| {
                                    (*icon == Icon::Deploy) == player.controls(planet)
                                        && p.id != planet.id
                                        && icon.condition(p)
                                })
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
