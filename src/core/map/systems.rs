use std::collections::HashMap;
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::color::palettes::css::WHITE;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_tweening::lens::ColorMaterialColorLens;
use bevy_tweening::{AnimTarget, RepeatCount, RepeatStrategy, Tween, TweenAnim};
use itertools::Itertools;
use strum::IntoEnumIterator;
use voronator::delaunator::Point;
use voronator::VoronoiDiagram;

use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{
    BACKGROUND_Z, BUTTON_TEXT_SIZE, ENEMY_COLOR, OWN_COLOR, OWN_COLOR_BASE, PHALANX_DISTANCE,
    PLANET_Z, RADAR_DISTANCE, TITLE_TEXT_SIZE, VORONOI_Z,
};
use crate::core::map::icon::Icon;
use crate::core::map::map::{Map, MapCmp};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::utils::{cursor, spawn_main_button, MainButtonLabelCmp, TransformOrbitLens};
use crate::core::missions::{Mission, MissionId, Missions};
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Unit};
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
pub struct ExplosionCmp {
    pub timer: Timer,
    pub last_index: usize,
    pub planet: PlanetId,
}

#[derive(Component)]
pub struct PlanetNameCmp;

#[derive(Component)]
pub struct PlanetResourcesCmp;

#[derive(Component)]
pub struct PlanetaryShieldCmp;

#[derive(Component)]
pub struct SpaceDockCmp;

#[derive(Component)]
pub struct ScannerCmp(pub bool);

#[derive(Component)]
pub struct VoronoiCmp(pub PlanetId);

#[derive(Component)]
pub struct VoronoiEdgeCmp {
    pub planet: PlanetId,
    pub key: (i32, i32, i32, i32),
}

#[derive(Component)]
pub struct EndTurnLabelCmp;

#[derive(Component)]
pub struct EndTurnButtonCmp;

fn edge_key(v1: Vec2, v2: Vec2) -> (i32, i32, i32, i32) {
    let precision = 5.0;
    let mut a = ((v1.x / precision).round() as i32, (v1.y / precision).round() as i32);
    let mut b = ((v2.x / precision).round() as i32, (v2.y / precision).round() as i32);
    if a > b {
        std::mem::swap(&mut a, &mut b);
    } // Make direction irrelevant
    (a.0, a.1, b.0, b.1)
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
        panic!("Expected Orthographic projection.");
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
                        panic!("Expected Orthographic projection.");
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
            state.combat_report = None;
        });

    for planet in &map.planets {
        let planet_id = planet.id;

        commands
            .spawn((
                Sprite {
                    image: assets.image(planet.image()),
                    custom_size: Some(Vec2::splat(planet.size())),
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
                      settings: Res<Settings>,
                      map: Res<Map>,
                      player: Res<Player>| {
                    let planet = map.get(planet_id);
                    if event.button == PointerButton::Primary {
                        state.planet_selected = Some(planet_id);
                        state.to_selected = true;
                        state.mission = false;
                        state.combat_report = None;
                        if player.owns(planet) {
                            state.mission_info.origin = planet_id;
                        }
                    } else if event.button == PointerButton::Secondary && !planet.is_destroyed {
                        state.mission = true;
                        state.combat_report = None;
                        state.mission_tab = MissionTab::NewMission;
                        state.mission_info = Mission::from_mission(
                            settings.turn,
                            player.id,
                            map.get(
                                state
                                    .planet_selected
                                    .filter(|&p| player.controls(map.get(p)))
                                    .unwrap_or(player.home_planet),
                            ),
                            map.get(planet_id),
                            &state.mission_info,
                        );
                        state.planet_selected = None;
                    }
                },
            )
            .with_children(|parent| {
                parent.spawn((
                    Text2d::new(&planet.name),
                    TextFont {
                        font: assets.font("bold"),
                        font_size: TITLE_TEXT_SIZE,
                        ..default()
                    },
                    TextColor(WHITE.into()),
                    Transform::from_xyz(0., planet.size() * 0.7, 0.9),
                    Pickable::IGNORE,
                    PlanetNameCmp,
                ));

                // Destroyed planets have no resources nor icons
                if !planet.is_destroyed {
                    for (i, icon) in Icon::iter().enumerate() {
                        parent
                            .spawn((
                                Sprite {
                                    image: assets.image(icon.to_lowername().as_str()),
                                    custom_size: Some(Vec2::splat(Icon::SIZE)),
                                    ..default()
                                },
                                Transform::from_translation(Vec3::new(
                                    planet.size() * 0.45,
                                    planet.size() * 0.4 - i as f32 * Icon::SIZE,
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
                                      map: Res<Map>,
                                      missions: Res<Missions>| {
                                    state.planet_hover = Some(planet_id);
                                    if let Some(mission) = missions
                                        .iter()
                                        .sorted_by(|a, b| {
                                            a.turns_to_destination(&map)
                                                .cmp(&b.turns_to_destination(&map))
                                        })
                                        .find(|m| {
                                            m.destination == planet_id
                                                && (m.objective == icon || icon == Icon::Attacked)
                                        })
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
                                      mut settings: ResMut<Settings>,
                                      map: Res<Map>,
                                      player: Res<Player>| {
                                    // Prevent the event from bubbling up to the planet
                                    event.propagate(false);

                                    if event.button == PointerButton::Primary {
                                        if icon.on_units() && player.owns(map.get(planet_id)) {
                                            state.planet_selected = Some(planet_id);
                                            state.mission = false;
                                            settings.show_menu = true;
                                            state.shop = icon.shop();
                                        } else if icon == Icon::Attacked {
                                            state.mission = true;
                                            state.planet_selected = None;
                                            state.mission_tab = MissionTab::EnemyMissions;
                                        } else if icon.is_mission() {
                                            state.mission = true;
                                            state.planet_selected = None;
                                            state.mission_tab = MissionTab::NewMission;

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
                                                        .unwrap_or(player.home_planet),
                                                );

                                            let origin = map.get(origin_id);
                                            state.mission_info =
                                                Mission::new(
                                                    settings.turn,
                                                    player.id,
                                                    map.get(origin_id),
                                                    map.get(planet_id),
                                                    icon,
                                                    match icon {
                                                        Icon::Colonize => HashMap::from([(
                                                            Unit::Ship(Ship::ColonyShip),
                                                            1,
                                                        )]),
                                                        Icon::Spy => HashMap::from([(
                                                            Unit::probe(),
                                                            origin.army.amount(&Unit::probe()),
                                                        )]),
                                                        Icon::Attack | Icon::Destroy => origin
                                                            .army
                                                            .iter()
                                                            .filter_map(|(u, c)| {
                                                                (*c > 0 && u.is_combat_ship())
                                                                    .then_some((*u, *c))
                                                            })
                                                            .collect(),
                                                        Icon::MissileStrike => HashMap::from([(
                                                            Unit::interplanetary_missile(),
                                                            origin.army.amount(
                                                                &Unit::interplanetary_missile(),
                                                            ),
                                                        )]),
                                                        Icon::Deploy => origin
                                                            .army
                                                            .iter()
                                                            .filter_map(|(u, c)| {
                                                                (*c > 0 && u.is_ship())
                                                                    .then_some((*u, *c))
                                                            })
                                                            .collect(),
                                                        _ => unreachable!(),
                                                    },
                                                    state.mission_info.bombing.clone(),
                                                    state.mission_info.combat_probes,
                                                    state.mission_info.jump_gate,
                                                    None,
                                                );
                                        }
                                    }
                                },
                            );
                    }

                    if !planet.is_moon() {
                        for (i, resource) in ResourceName::iter().enumerate() {
                            parent
                                .spawn((
                                    Sprite {
                                        image: assets.image(resource.to_lowername()),
                                        custom_size: Some(Vec2::new(
                                            planet.size() * 0.45,
                                            planet.size() * 0.3,
                                        )),
                                        ..default()
                                    },
                                    Transform {
                                        translation: Vec3::new(
                                            -planet.size() * 1.1,
                                            planet.size() * (0.27 - i as f32 * 0.25),
                                            0.7,
                                        ),
                                        scale: Vec3::splat(0.6),
                                        ..default()
                                    },
                                    Pickable::IGNORE,
                                    PlanetResourcesCmp,
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
                                        Transform::from_xyz(55., 0., 0.8),
                                    ));
                                });
                        }
                    }

                    // Draw planetary shield
                    let material =
                        materials.add(ColorMaterial::from(OWN_COLOR_BASE.with_alpha(0.)));
                    parent.spawn((
                        Mesh2d(
                            meshes.add(Annulus::new(planet.size() * 0.55, planet.size() * 0.57)),
                        ),
                        MeshMaterial2d(material.clone()),
                        Transform::from_xyz(0., 0., 0.6),
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::Linear,
                                Duration::from_secs(1),
                                ColorMaterialColorLens {
                                    start: OWN_COLOR_BASE.with_alpha(0.),
                                    end: OWN_COLOR_BASE.with_alpha(1.),
                                },
                            )
                            .with_repeat_count(RepeatCount::Infinite)
                            .with_repeat_strategy(RepeatStrategy::MirroredRepeat),
                        ),
                        AnimTarget::asset(&material),
                        Visibility::Hidden,
                        PlanetaryShieldCmp,
                    ));

                    // Draw space dock
                    parent.spawn((
                        Sprite {
                            image: assets.image("dock"),
                            custom_size: Some(Vec2::splat(planet.size() * 0.4)),
                            ..default()
                        },
                        Transform::from_xyz(-planet.size() * 0.5, -planet.size() * 0.5, 0.7),
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::Linear,
                                Duration::from_secs(12),
                                TransformOrbitLens(planet.size() * 0.75),
                            )
                            .with_repeat_count(RepeatCount::Infinite),
                        ),
                        Pickable::IGNORE,
                        Visibility::Hidden,
                        SpaceDockCmp,
                    ));

                    // Draw phalanx and orbital scanning radius
                    parent.spawn((
                        Mesh2d(meshes.add(Circle::new(0.))),
                        MeshMaterial2d(materials.add(Color::srgba(0., 0.5, 0.3, 0.05))),
                        Transform::from_xyz(0., 0., -0.1),
                        Visibility::Hidden,
                        ScannerCmp(true),
                    ));
                    parent.spawn((
                        Mesh2d(meshes.add(Annulus::new(0., 0.))),
                        MeshMaterial2d(materials.add(Color::srgba(0., 0.5, 0.3, 0.5))),
                        Transform::from_xyz(0., 0., -0.1),
                        Visibility::Hidden,
                        ScannerCmp(false),
                    ));
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
                    MeshMaterial2d(materials.add(OWN_COLOR.with_alpha(0.5))),
                    Visibility::Hidden,
                    VoronoiCmp(map.planets[i].id),
                    MapCmp,
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
                        MeshMaterial2d(materials.add(OWN_COLOR.with_alpha(0.01))),
                        Visibility::Hidden,
                        VoronoiEdgeCmp {
                            planet: planet_id,
                            key: edge_key(v1, v2),
                        },
                        MapCmp,
                    ));
                }
            }
        }
    }

    // Spawn end turn button
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
        MapCmp,
    ));

    spawn_main_button(&mut commands, "End turn", &assets)
        .insert((EndTurnButtonCmp, MapCmp))
        .observe(|_: On<Pointer<Click>>, mut state: ResMut<UiState>| {
            state.planet_selected = None;
            state.mission = false;
            state.combat_report = None;
            state.end_turn = !state.end_turn;
        });
}

pub fn update_planet_info(
    mut planet_q: Query<(Entity, &mut Sprite, &PlanetCmp)>,
    mut icon_q: Query<(&mut Visibility, &mut Transform, &Icon)>,
    mut name_q: Query<
        &mut Visibility,
        (
            With<PlanetNameCmp>,
            Without<Icon>,
            Without<PlanetResourcesCmp>,
            Without<ScannerCmp>,
            Without<SpaceDockCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    mut resources_q: Query<
        &mut Visibility,
        (
            With<PlanetResourcesCmp>,
            Without<Icon>,
            Without<PlanetNameCmp>,
            Without<ScannerCmp>,
            Without<SpaceDockCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    mut ps_q: Query<
        (&mut Visibility, &mut TweenAnim),
        (
            With<PlanetaryShieldCmp>,
            Without<Icon>,
            Without<PlanetNameCmp>,
            Without<PlanetResourcesCmp>,
            Without<ScannerCmp>,
            Without<PlanetCmp>,
            Without<SpaceDockCmp>,
        ),
    >,
    mut dock_q: Query<
        (&mut Visibility, &mut Sprite),
        (
            With<SpaceDockCmp>,
            Without<Icon>,
            Without<PlanetNameCmp>,
            Without<PlanetResourcesCmp>,
            Without<ScannerCmp>,
            Without<PlanetCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    mut scanner_q: Query<
        (&mut Visibility, &mut Mesh2d, &ScannerCmp),
        (
            Without<Icon>,
            Without<PlanetNameCmp>,
            Without<PlanetResourcesCmp>,
            Without<SpaceDockCmp>,
            Without<PlanetaryShieldCmp>,
        ),
    >,
    children_q: Query<&Children>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    state: Res<UiState>,
    settings: Res<Settings>,
    assets: Local<WorldAssets>,
    mut meshes: ResMut<Assets<Mesh>>,
) {
    let (n_owned, n_max_owned) = player.planets_owned(&map, &settings);

    for (planet_e, mut planet_s, planet_c) in &mut planet_q {
        let planet = map.get(planet_c.id);

        // Update destroyed planet image
        planet_s.image = assets.image(planet.image());

        let selected =
            state.planet_hover.or(state.planet_selected).map(|id| id == planet.id).unwrap_or(false);

        // Show/hide planet icons
        let mut count = 0;
        for child in children_q.iter_descendants(planet_e) {
            if let Ok((mut icon_v, mut icon_t, icon)) = icon_q.get_mut(child) {
                let visible = match icon {
                    Icon::Attacked => missions.iter().any(|m| {
                        player.owns(planet)
                            && m.objective != Icon::Deploy
                            && m.destination == planet.id
                    }),
                    Icon::Buildings => {
                        player.owns(planet)
                            && (selected || icon.condition(planet) || settings.show_info)
                    },
                    Icon::Defenses => {
                        player.owns(planet)
                            && !planet.is_moon()
                            && (selected || icon.condition(planet) || settings.show_info)
                    },
                    Icon::Fleet => {
                        // Shows when having an army on a not-owned planet, but hides when hovered
                        player.controls(planet)
                            && if player.owns(planet) || planet.is_moon() {
                                selected || icon.condition(planet) || settings.show_info
                            } else {
                                icon.condition(planet) && !selected && !settings.show_info
                            }
                    },
                    _ => {
                        // Show icon if there is a mission with this objective towards this
                        // planet or, if there's selected planet, it fulfills the condition,
                        // else if any of the player's planets fulfills the condition
                        let has_mission = missions.iter().any(|m| {
                            m.owner == player.id
                                && m.objective == *icon
                                && m.destination == planet.id
                        });

                        let has_condition = {
                            map.planets.iter().any(|p| {
                                p.id != planet.id
                                    && icon.condition(p)
                                    && match icon {
                                        Icon::Deploy => {
                                            player.controls(p) && player.controls(planet)
                                        },
                                        Icon::Colonize => {
                                            player.controls(p)
                                                && !player.owns(planet)
                                                && !planet.is_moon()
                                                && n_owned < n_max_owned
                                        },
                                        Icon::MissileStrike => {
                                            player.controls(p)
                                                && !player.controls(planet)
                                                && !planet.is_moon()
                                        },
                                        _ => player.controls(p) && !player.controls(planet),
                                    }
                            })
                        };

                        has_mission || ((selected || settings.show_info) && has_condition)
                    },
                };

                *icon_v = if visible && !planet.is_destroyed {
                    icon_t.translation.y = planet.size() * 0.4 - count as f32 * Icon::SIZE;
                    count += 1;
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }

            // Show/hide planet resources and name
            if let Ok(mut visibility) = name_q.get_mut(child) {
                *visibility = if selected || settings.show_info {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }
            if let Ok(mut visibility) = resources_q.get_mut(child) {
                *visibility = if (selected || settings.show_info) && !planet.is_destroyed {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }

            let controls = player.controls(planet);
            let (has_ps, has_dock) = if controls {
                (
                    planet.army.amount(&Unit::planetary_shield()) > 0,
                    planet.army.amount(&Unit::space_dock()) > 0,
                )
            } else {
                if let Some(info) = player.last_info(planet, &missions.0) {
                    (
                        info.army.amount(&Unit::planetary_shield()) > 0,
                        info.army.amount(&Unit::space_dock()) > 0,
                    )
                } else {
                    (false, false)
                }
            };

            // Show/hide the Planetary Shield
            if let Ok((mut visibility, tween)) = ps_q.get_mut(child) {
                *visibility = if has_ps {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            }

            // Show/hide the Space Dock
            if let Ok((mut visibility, mut sprite)) = dock_q.get_mut(child) {
                *visibility = if has_dock {
                    sprite.image = assets.image(if controls && selected {
                        "dock hover"
                    } else if controls {
                        "dock"
                    } else if selected {
                        "dock enemy hover"
                    } else {
                        "dock enemy"
                    });
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
            }

            // Show/hide scanner indicator
            if let Ok((mut visibility, mut mesh, scanner)) = scanner_q.get_mut(child) {
                let mut radius = if state.phalanx_hover == Some(planet.id) {
                    PHALANX_DISTANCE
                        * Planet::SIZE
                        * planet.army.amount(&Unit::Building(Building::SensorPhalanx)) as f32
                } else if state.radar_hover == Some(planet.id) {
                    RADAR_DISTANCE
                        * Planet::SIZE
                        * planet.army.amount(&Unit::Building(Building::OrbitalRadar)) as f32
                } else {
                    0.
                };

                if radius > 0. {
                    radius += planet.size() * 0.5; // Start at the edge of the planet

                    *visibility = Visibility::Inherited;
                    if scanner.0 {
                        *mesh = Mesh2d(meshes.add(Mesh::from(Circle::new(radius))));
                    } else {
                        *mesh = Mesh2d(meshes.add(Mesh::from(Annulus::new(radius - 2., radius))));
                    }
                } else {
                    *visibility = Visibility::Hidden;
                }
            }
        }
    }
}

pub fn update_voronoi(
    mut cell_q: Query<(&mut Visibility, &mut MeshMaterial2d<ColorMaterial>, &VoronoiCmp)>,
    mut edge_q: Query<
        (&mut Visibility, &mut MeshMaterial2d<ColorMaterial>, &VoronoiEdgeCmp),
        Without<VoronoiCmp>,
    >,
    settings: Res<Settings>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (mut cell_v, cell_m, cell) in &mut cell_q {
        let planet = map.get(cell.0);

        let visible = settings.show_cells
            && !planet.is_destroyed
            && (player.controls(planet)
                || player.last_info(planet, &missions.0).is_some_and(|i| i.controlled));

        if visible {
            if let Some(material) = materials.get_mut(&*cell_m) {
                material.color = if player.controls(planet) {
                    OWN_COLOR.with_alpha(0.01)
                } else {
                    ENEMY_COLOR.with_alpha(0.01)
                };
            }
        }

        *cell_v = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    let mut counts_enemy = HashMap::new();
    let mut counts_own = HashMap::new();

    for (_, _, edge) in &edge_q {
        let planet = map.get(edge.planet);
        if player.controls(planet) {
            *counts_own.entry(edge.key).or_default() += 1;
        } else if player.last_info(planet, &missions.0).is_some_and(|i| i.controlled) {
            *counts_enemy.entry(edge.key).or_default() += 1;
        }
    }

    for (mut edge_v, edge_m, edge) in &mut edge_q {
        if !settings.show_cells {
            *edge_v = Visibility::Hidden;
            continue;
        }

        let (visible, color) = if *counts_own.get(&edge.key).unwrap_or(&2) <= 1 {
            (true, OWN_COLOR.with_alpha(0.5))
        } else if *counts_enemy.get(&edge.key).unwrap_or(&2) <= 1 {
            (true, ENEMY_COLOR.with_alpha(0.5))
        } else {
            (false, Color::default())
        };

        if visible {
            if let Some(mat) = materials.get_mut(&*edge_m) {
                mat.color = color;
            }
        }

        *edge_v = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub fn update_end_turn(
    mut button_c: Query<&mut Visibility, With<EndTurnButtonCmp>>,
    mut button_q: Query<&mut Text, With<MainButtonLabelCmp>>,
    mut label_q: Query<&mut Visibility, (With<EndTurnLabelCmp>, Without<EndTurnButtonCmp>)>,
    game_state: Res<State<GameState>>,
    state: Res<UiState>,
    player: Res<Player>,
) {
    for mut button_v in &mut button_c {
        *button_v = if !player.spectator {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if *game_state.get() == GameState::Playing {
        for mut button_t in &mut button_q {
            button_t.0 = if state.end_turn {
                "Continue turn".to_string()
            } else {
                "End turn".to_string()
            };
        }
    }

    for mut label_v in &mut label_q {
        *label_v = if state.end_turn && !player.spectator {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

pub fn run_animations(
    mut commands: Commands,
    mut animation_q: Query<(Entity, &mut Sprite, &mut ExplosionCmp)>,
    mut map: ResMut<Map>,
    time: Res<Time>,
) {
    for (animation_e, mut sprite, mut animation) in &mut animation_q {
        animation.timer.tick(time.delta());

        let planet = map.get_mut(animation.planet);

        if animation.timer.just_finished() {
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index += 1;

                // Change planet's image at a third of the animation
                if atlas.index == animation.last_index / 3 {
                    planet.image = 0;
                } else if atlas.index == animation.last_index {
                    commands.entity(animation_e).try_despawn();
                }
            }
        }
    }
}
