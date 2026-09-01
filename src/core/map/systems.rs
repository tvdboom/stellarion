//! Bevy systems that render and animate the strategic map projection.

use std::collections::HashMap;
use std::f32::consts::TAU;
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::color::palettes::css::WHITE;
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_tweening::lens::ColorMaterialColorLens;
use bevy_tweening::{AnimTarget, RepeatCount, RepeatStrategy, Tween, TweenAnim};
use itertools::Itertools;
use rand::{rng, RngExt};
use strum::IntoEnumIterator;
use voronator::delaunator::Point;
use voronator::VoronoiDiagram;

use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{
    BACKGROUND_Z, BUTTON_TEXT_SIZE, ENEMY_COLOR, OWN_COLOR, PHALANX_DISTANCE, PLANET_Z,
    RADAR_DISTANCE, TITLE_TEXT_SIZE, VORONOI_Z,
};
use crate::core::map::icon::Icon;
use crate::core::map::model::{Map, MapCmp};
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
use crate::core::units::{Amount, Army, Unit};
use crate::multiplayer::client::MultiplayerSession;
use crate::utils::NameFromEnum;

#[derive(Component)]
/// Bevy component mapping a rendered planet entity to a stable planet ID.
pub struct PlanetCmp {
    /// Stable identifier used to cross-reference this value.
    pub id: PlanetId,
}

impl PlanetCmp {
    /// Creates a new value from the supplied state.
    pub fn new(id: PlanetId) -> Self {
        Self {
            id,
        }
    }
}

#[derive(Component)]
/// Bevy component mapping a rendered mission entity to a stable mission ID.
pub struct MissionCmp {
    /// Stable identifier used to cross-reference this value.
    pub id: MissionId,
}

impl MissionCmp {
    /// Creates a new value from the supplied state.
    pub fn new(id: MissionId) -> Self {
        Self {
            id,
        }
    }
}

#[derive(Component)]
/// Timed map explosion associated with a destroyed planet.
pub struct ExplosionCmp {
    /// Timer controlling the current effect frame.
    pub timer: Timer,
    /// Highest valid texture-atlas frame index.
    pub last_index: usize,
    /// Stable planet associated with this component.
    pub planet: PlanetId,
}

#[derive(Component)]
/// Bevy component marking planet name presentation entities.
pub struct PlanetNameCmp;

#[derive(Component)]
/// Bevy component marking planet resources presentation entities.
pub struct PlanetResourcesCmp;

/// Component for planetary shield visualization. It stores whether
/// the player owns the shield to swap the tween animation if it changes
#[derive(Component)]
pub struct PlanetaryShieldCmp {
    /// Owning player when colonized, or an ownership flag in presentation state.
    pub owned: bool,
}

impl PlanetaryShieldCmp {
    /// Creates a new value from the supplied state.
    pub fn new() -> Self {
        Self {
            owned: true,
        }
    }

    /// Tween that animates the Planetary Shield
    pub fn tween(c1: Color, c2: Color) -> Tween {
        Tween::new(
            EaseFunction::Linear,
            Duration::from_secs(1),
            ColorMaterialColorLens {
                start: c1,
                end: c2,
            },
        )
        .with_repeat_count(RepeatCount::Infinite)
        .with_repeat_strategy(RepeatStrategy::MirroredRepeat)
    }
}

impl Default for PlanetaryShieldCmp {
    /// Creates an owned planetary-shield marker.
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Component)]
/// Bevy component marking space dock presentation entities.
pub struct SpaceDockCmp;

#[derive(Component)]
/// Bevy component marking scanner presentation entities.
pub struct ScannerCmp(pub bool);

#[derive(Component)]
/// Bevy component marking voronoi presentation entities.
pub struct VoronoiCmp(pub PlanetId);

#[derive(Component)]
/// Rendered ownership-border edge with a canonical deduplication key.
pub struct VoronoiEdgeCmp {
    /// Stable planet associated with this component.
    pub planet: PlanetId,
    /// Canonical quantized endpoints used to deduplicate this border edge.
    pub key: (i32, i32, i32, i32),
}

#[derive(Component)]
/// Bevy component marking end turn label presentation entities.
pub struct EndTurnLabelCmp;

#[derive(Component)]
/// Bevy component marking end turn button presentation entities.
pub struct EndTurnButtonCmp;

#[derive(Component)]
/// Bevy component marking spectator label presentation entities.
pub struct SpectatorLabelCmp;

/// Canonicalizes an undirected Voronoi edge for deduplication.
fn edge_key(v1: Vec2, v2: Vec2) -> (i32, i32, i32, i32) {
    let precision = 5.0;
    let mut a = ((v1.x / precision).round() as i32, (v1.y / precision).round() as i32);
    let mut b = ((v2.x / precision).round() as i32, (v2.y / precision).round() as i32);
    if a > b {
        std::mem::swap(&mut a, &mut b);
    } // Make direction irrelevant
    (a.0, a.1, b.0, b.1)
}

/// Spawns ownership geometry with transform depths used by Bevy's transparent sort.
fn spawn_voronoi_cells(
    commands: &mut Commands,
    map: &Map,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ColorMaterial>,
) {
    let Some(voronoi) = VoronoiDiagram::<Point>::from_tuple(
        &(-10000., -10000.),
        &(10000., 10000.),
        &map.planets.iter().map(|p| (p.position.x as f64, p.position.y as f64)).collect::<Vec<_>>(),
    ) else {
        warn!("Could not generate ownership cells for the strategic map.");
        return;
    };

    for (planet, cell) in map.planets.iter().zip(voronoi.cells()) {
        let points = cell.points();
        let n = points.len();
        if n < 3 {
            continue;
        }
        // Keep mesh vertices on their local plane. Baking depth into vertices alone leaves
        // the entity at z=0, so transparent cells can sort behind the map background.
        let positions =
            points.iter().map(|p| Vec3::new(p.x as f32, p.y as f32, 0.0)).collect::<Vec<_>>();
        let indices = (1..n - 1).flat_map(|i| [0, i as u32, (i + 1) as u32]).collect();
        let mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
            .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
            .with_inserted_indices(Indices::U32(indices));
        commands.spawn((
            Mesh2d(meshes.add(mesh)),
            MeshMaterial2d(materials.add(OWN_COLOR.with_alpha(0.12))),
            Transform::from_xyz(0.0, 0.0, VORONOI_Z),
            Visibility::Hidden,
            Pickable::IGNORE,
            VoronoiCmp(planet.id),
            MapCmp,
        ));

        for j in 0..n {
            let a = points[j];
            let b = points[(j + 1) % n];
            let v1 = Vec2::new(a.x as f32, a.y as f32);
            let v2 = Vec2::new(b.x as f32, b.y as f32);
            let mesh = Mesh::new(PrimitiveTopology::LineList, RenderAssetUsages::default())
                .with_inserted_attribute(
                    Mesh::ATTRIBUTE_POSITION,
                    vec![v1.extend(0.0), v2.extend(0.0)],
                )
                .with_inserted_indices(Indices::U32(vec![0, 1]));
            commands.spawn((
                Mesh2d(meshes.add(mesh)),
                MeshMaterial2d(materials.add(OWN_COLOR.with_alpha(0.58))),
                Transform::from_xyz(0.0, 0.0, VORONOI_Z + 0.1),
                Visibility::Hidden,
                Pickable::IGNORE,
                VoronoiEdgeCmp {
                    planet: planet.id,
                    key: edge_key(v1, v2),
                },
                MapCmp,
            ));
        }
    }
}

/// Draws the map interface and emits any resulting local actions.
pub fn draw_map(
    mut commands: Commands,
    camera: Single<(&mut Transform, &mut Projection), With<MainCamera>>,
    map: Res<Map>,
    player: Res<Player>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    assets: Res<WorldAssets>,
) {
    let (mut camera_t, mut projection) = camera.into_inner();
    let Projection::Orthographic(projection) = &mut *projection else {
        return;
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
                        return;
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
                        font: assets.font("bold").into(),
                        font_size: TITLE_TEXT_SIZE.into(),
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
                                icon,
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
                                        let planet = map.get(planet_id);
                                        if icon.on_units()
                                            && (player.owns(planet)
                                                || (player.controls(planet) && planet.is_moon()))
                                        {
                                            state.planet_selected = Some(planet_id);
                                            state.mission = false;
                                            settings.show_menu = true;
                                            if let Some(shop) = icon.shop() {
                                                state.shop = shop;
                                            }
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
                                                        Icon::Colonize => Army::from([(
                                                            Unit::Ship(Ship::ColonyShip),
                                                            1,
                                                        )]),
                                                        Icon::Spy => Army::from([(
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
                                                        Icon::MissileStrike => Army::from([(
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
                                                        _ => Army::new(),
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
                                            font: assets.font("bold").into(),
                                            font_size: 25.0.into(),
                                            ..default()
                                        },
                                        TextColor(WHITE.into()),
                                        Transform::from_xyz(55., 0., 0.8),
                                    ));
                                });
                        }
                    }

                    // Draw planetary shield
                    let material = materials.add(ColorMaterial::from(OWN_COLOR));
                    parent.spawn((
                        Mesh2d(
                            meshes.add(Annulus::new(planet.size() * 0.55, planet.size() * 0.57)),
                        ),
                        MeshMaterial2d(material.clone()),
                        Transform::from_xyz(0., 0., 0.6),
                        TweenAnim::new(PlanetaryShieldCmp::tween(OWN_COLOR, Color::WHITE)),
                        AnimTarget::asset(&material),
                        Visibility::Hidden,
                        PlanetaryShieldCmp::new(),
                    ));

                    // Draw space dock on random position around planet
                    let r = planet.size() * 0.5;
                    let angle = rng().random_range(0.0..TAU);

                    parent.spawn((
                        Sprite {
                            image: assets.image("dock"),
                            custom_size: Some(Vec2::splat(planet.size() * 0.4)),
                            ..default()
                        },
                        Transform::from_xyz(angle.cos() * r, angle.sin() * r, 0.7),
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::Linear,
                                Duration::from_secs(12),
                                TransformOrbitLens {
                                    radius: planet.size() * 0.75,
                                    offset: angle,
                                },
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

    spawn_voronoi_cells(&mut commands, &map, &mut meshes, &mut materials);

    // Spawn end turn button
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(42.),
            right: Val::Px(270.),
            ..default()
        },
        Text::new("Waiting for other players to finish their turn..."),
        TextFont {
            font: assets.font("bold").into(),
            font_size: BUTTON_TEXT_SIZE.into(),
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

    // Spawn spectator mode label
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            bottom: Val::Px(30.),
            right: Val::Px(60.),
            ..default()
        },
        Text::new("Spectator Mode"),
        TextFont {
            font: assets.font("bold").into(),
            font_size: 30.0.into(),
            ..default()
        },
        TextColor(Color::WHITE),
        Visibility::Hidden,
        SpectatorLabelCmp,
        MapCmp,
    ));
}

/// Updates planet info from the current canonical ECS projection.
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
        (&mut Visibility, &mut TweenAnim, &mut PlanetaryShieldCmp),
        (
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
    assets: Res<WorldAssets>,
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
                        (player.owns(planet) || (player.controls(planet) && planet.is_moon()))
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
                    Icon::Defenses => {
                        player.owns(planet)
                            && !planet.is_moon()
                            && (selected || icon.condition(planet) || settings.show_info)
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
            if let Ok((mut visibility, mut tween, mut ps)) = ps_q.get_mut(child) {
                *visibility = if has_ps {
                    if ps.owned != controls {
                        ps.owned = controls;

                        let tween_def = if controls {
                            PlanetaryShieldCmp::tween(OWN_COLOR, Color::WHITE)
                        } else {
                            PlanetaryShieldCmp::tween(ENEMY_COLOR, Color::WHITE)
                        };

                        if let Err(err) = tween.set_tweenable(tween_def) {
                            warn!("Failed to swap PS tween. Error: {err}");
                        }
                    }

                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                }
            }

            // Show/hide the Space Dock
            if let Ok((mut visibility, mut sprite)) = dock_q.get_mut(child) {
                *visibility = if has_dock {
                    sprite.image = assets.image(if controls {
                        "dock"
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

/// Updates voronoi from the current canonical ECS projection.
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
    session: Res<MultiplayerSession>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    let known_controllers = map
        .planets
        .iter()
        .filter_map(|planet| {
            let controller = if player.controls(planet) {
                planet.controlled
            } else {
                player.last_info(planet, &missions.0).and_then(|info| info.controlled)
            };
            controller.map(|controller| (planet.id, controller))
        })
        .collect::<HashMap<_, _>>();

    for (mut cell_v, cell_m, cell) in &mut cell_q {
        let planet = map.get(cell.0);

        let visible = settings.show_cells
            && !planet.is_destroyed
            && known_controllers.contains_key(&planet.id);

        if visible {
            if let Some(mut material) = materials.get_mut(&*cell_m) {
                let controller = known_controllers[&planet.id];
                material.color = session.player_color(controller).color().with_alpha(0.12);
            }
        }

        *cell_v = if visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    let mut counts_by_owner = HashMap::new();

    for (_, _, edge) in &edge_q {
        if let Some(&controller) = known_controllers.get(&edge.planet) {
            *counts_by_owner.entry((edge.key, controller)).or_default() += 1;
        }
    }

    for (mut edge_v, edge_m, edge) in &mut edge_q {
        if !settings.show_cells {
            *edge_v = Visibility::Hidden;
            continue;
        }

        let Some(&controller) = known_controllers.get(&edge.planet) else {
            *edge_v = Visibility::Hidden;
            continue;
        };
        let visible = *counts_by_owner.get(&(edge.key, controller)).unwrap_or(&2) <= 1;
        let color = session.player_color(controller).color().with_alpha(0.58);

        if visible {
            if let Some(mut mat) = materials.get_mut(&*edge_m) {
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

/// Updates end turn from the current canonical ECS projection.
pub fn update_end_turn(
    mut button_c: Query<&mut Visibility, With<EndTurnButtonCmp>>,
    mut spectator_q: Query<&mut Visibility, (With<SpectatorLabelCmp>, Without<EndTurnButtonCmp>)>,
    mut button_q: Query<&mut Text, With<MainButtonLabelCmp>>,
    mut label_q: Query<
        &mut Visibility,
        (With<EndTurnLabelCmp>, Without<SpectatorLabelCmp>, Without<EndTurnButtonCmp>),
    >,
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

    for mut label_v in &mut spectator_q {
        *label_v = if player.spectator && *game_state.get() == GameState::Playing {
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

/// Advances map animations effects for the current frame.
pub fn run_map_animations(
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
                    commands.entity(animation_e).despawn();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::identity::{GameCode, GameId};
    use crate::core::player::PlayerColor;
    use crate::core::simulation::{GameModel, GameRules, PersistedGame};
    use crate::multiplayer::model::GameRecord;

    #[test]
    fn ownership_cells_render_above_background_in_local_and_multiplayer_games() {
        for player_count in [1, 2] {
            let mut model = GameModel::new(
                [7; 32],
                GameRules {
                    player_count,
                    practice_mode: player_count == 1,
                    ..default()
                },
            )
            .unwrap();
            if player_count == 2 {
                model.players[0].color = PlayerColor::new(4);
            }
            model.start().unwrap();
            let player = model.players[0].clone();
            let expected_color = player.color().color();
            if player_count == 1 {
                assert_eq!(expected_color, OWN_COLOR, "local games default to blue");
            }
            let home = player.home_planet;
            let planet_count = model.map.planets.len();
            let mut session = MultiplayerSession::default();
            session.local_practice = player_count == 1;
            session.active_game = Some(GameRecord {
                id: GameId::new("voronoi-test"),
                code: GameCode::new("ABCDEF"),
                revision: 0,
                max_players: player_count,
                status: model.status,
                persisted: PersistedGame::new(model.clone()),
                members: vec![],
            });
            let mut app = App::new();
            app.add_plugins(TransformPlugin)
                .init_resource::<Assets<Mesh>>()
                .init_resource::<Assets<ColorMaterial>>()
                .init_resource::<Settings>()
                .insert_resource(model.map)
                .insert_resource(Missions(model.missions))
                .insert_resource(player)
                .insert_resource(session)
                .add_systems(
                    Startup,
                    |mut commands: Commands,
                     map: Res<Map>,
                     mut meshes: ResMut<Assets<Mesh>>,
                     mut materials: ResMut<Assets<ColorMaterial>>| {
                        spawn_voronoi_cells(&mut commands, &map, &mut meshes, &mut materials);
                    },
                )
                .add_systems(Update, update_voronoi);
            app.update();

            let world = app.world_mut();
            let mut cells = world.query::<(
                Entity,
                &VoronoiCmp,
                &Visibility,
                &GlobalTransform,
                &Mesh2d,
                &MeshMaterial2d<ColorMaterial>,
            )>();
            assert_eq!(cells.iter(world).count(), planet_count);
            let mut home_entity = None;
            for (entity, cell, visibility, transform, mesh, material) in cells.iter(world) {
                assert!(transform.translation().z > BACKGROUND_Z);
                assert!(transform.translation().z < PLANET_Z);
                let positions = world
                    .resource::<Assets<Mesh>>()
                    .get(&mesh.0)
                    .unwrap()
                    .attribute(Mesh::ATTRIBUTE_POSITION)
                    .unwrap()
                    .as_float3()
                    .unwrap();
                assert!(positions.iter().all(|position| position[2] == 0.0));
                if cell.0 == home {
                    home_entity = Some(entity);
                    assert_eq!(*visibility, Visibility::Inherited);
                    assert_eq!(
                        world.resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
                        expected_color.with_alpha(0.12)
                    );
                } else {
                    assert_eq!(*visibility, Visibility::Hidden, "unknown territory stays hidden");
                }
            }
            let home_entity = home_entity.expect("home world has an ownership cell");
            let mut edges = world.query::<(
                &VoronoiEdgeCmp,
                &Visibility,
                &GlobalTransform,
                &MeshMaterial2d<ColorMaterial>,
            )>();
            let mut home_edges = 0;
            for (edge, visibility, transform, material) in edges.iter(world) {
                assert!(transform.translation().z > VORONOI_Z);
                assert!(transform.translation().z < PLANET_Z);
                if edge.planet == home {
                    home_edges += 1;
                    assert_eq!(*visibility, Visibility::Inherited);
                    assert_eq!(
                        world.resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
                        expected_color.with_alpha(0.58)
                    );
                }
            }
            assert!(home_edges >= 3);

            app.world_mut().resource_mut::<Settings>().show_cells = false;
            app.update();
            let world = app.world_mut();
            assert!(world
                .query_filtered::<&Visibility, With<MapCmp>>()
                .iter(world)
                .all(|visibility| *visibility == Visibility::Hidden));
            app.world_mut().resource_mut::<Settings>().show_cells = true;
            app.update();
            assert_eq!(*app.world().get::<Visibility>(home_entity).unwrap(), Visibility::Inherited);
        }
    }
}
