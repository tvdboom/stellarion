//! Bevy systems that render and animate the strategic map projection.

pub use super::scanner::ScannerCmp;

use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use std::time::Duration;

use bevy::asset::RenderAssetUsages;
use bevy::color::{palettes::css::WHITE, Mix};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon};
use bevy_tweening::lens::ColorMaterialColorLens;
use bevy_tweening::{AnimTarget, EaseMethod, RepeatCount, Tween, TweenAnim, Tweenable};
use itertools::Itertools;
use rand::{rng, RngExt};
use strum::IntoEnumIterator;
use voronator::delaunator::Point;
use voronator::VoronoiDiagram;

use crate::core::assets::WorldAssets;
use crate::core::camera::{MainCamera, ParallaxCmp};
use crate::core::constants::{
    BACKGROUND_Z, BUTTON_TEXT_SIZE, OWN_COLOR, PHALANX_DISTANCE, PLANET_Z, RADAR_DISTANCE,
    TITLE_TEXT_SIZE, VORONOI_Z,
};
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::{Map, MapCmp};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::utils::{
    cursor, spawn_main_button, MainButtonLabelCmp, TransformOrbitLens, TransformOrbitSpinLens,
};
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

/// Tracks the displayed owner's color so a shield's pulse updates after control changes.
#[derive(Component, Default)]
pub struct PlanetaryShieldCmp {
    color: Option<Color>,
}

impl PlanetaryShieldCmp {
    /// Creates a new value from the supplied state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Breathes in and out with a smooth reversal, preserving the owner's hue.
    pub fn tween(color: Color) -> Tween {
        Tween::new(
            // One complete wave has the same opacity and zero slope at both ends, so the
            // loop never jumps or abruptly reverses when it wraps back to its start.
            EaseMethod::CustomFunction(|phase| 0.5 - 0.5 * (TAU * phase).cos()),
            Duration::from_secs(3),
            ColorMaterialColorLens {
                start: color.with_alpha(0.0),
                end: color.with_alpha(0.95),
            },
        )
        .with_repeat_count(RepeatCount::Infinite)
    }
}

#[derive(Component)]
/// Bevy component marking space dock presentation entities.
pub struct SpaceDockCmp;

#[derive(Component)]
/// Animated, faction-tinted jump-gate marker orbiting a world.
pub struct JumpGateCmp;

const TERRITORY_TRANSITION_SECONDS: f32 = 1.35;

#[derive(Component, Debug)]
/// Local presentation state for smooth ownership-color and visibility changes.
pub struct TerritoryTransitionCmp {
    target: Color,
    target_visible: bool,
    start: Color,
    elapsed: f32,
    initialized: bool,
}

impl Default for TerritoryTransitionCmp {
    fn default() -> Self {
        Self {
            target: OWN_COLOR.with_alpha(0.0),
            target_visible: false,
            start: OWN_COLOR.with_alpha(0.0),
            elapsed: 0.0,
            initialized: false,
        }
    }
}

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
            MeshMaterial2d(materials.add(OWN_COLOR.with_alpha(0.01))),
            Transform::from_xyz(0.0, 0.0, VORONOI_Z),
            Visibility::Hidden,
            Pickable::IGNORE,
            VoronoiCmp(planet.id),
            TerritoryTransitionCmp::default(),
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
                TerritoryTransitionCmp::default(),
                MapCmp,
            ));
        }
    }
}

/// Selects a planet and updates the mission origin for owned planets.
pub(crate) fn select_planet(planet: &Planet, state: &mut UiState, player: &Player) {
    state.planet_selected = Some(planet.id);
    state.focus_planet = None;
    state.to_selected = true;
    state.mission = false;
    state.combat_report = None;
    if player.owns(planet) {
        state.mission_info.origin = planet.id;
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
                    state.focus_planet = None;
                    state.to_selected = false;
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
                        state.focus_planet = None;
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
                      mut pending: ResMut<crate::multiplayer::client::PendingTurnCommands>,
                      settings: Res<Settings>,
                      map: Res<Map>,
                      player: Res<Player>| {
                    let planet = map.get(planet_id);
                    if event.button == PointerButton::Primary {
                        state.end_turn = false;
                        pending.request_resume();
                        select_planet(planet, &mut state, &player);
                    } else if event.button == PointerButton::Secondary && !planet.is_destroyed {
                        state.end_turn = false;
                        pending.request_resume();
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
                                    state.mission_hover_from_ui = false;
                                    state.mission_hover = None;
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
                                state.mission_hover_from_ui = false;
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
                    let material = materials.add(ColorMaterial {
                        color: OWN_COLOR.with_alpha(0.0),
                        // ColorMaterial::from an opaque color ignores the tween's alpha changes.
                        alpha_mode: bevy::sprite_render::AlphaMode2d::Blend,
                        ..default()
                    });
                    parent.spawn((
                        Mesh2d(
                            meshes.add(Annulus::new(planet.size() * 0.55, planet.size() * 0.57)),
                        ),
                        MeshMaterial2d(material.clone()),
                        Transform::from_xyz(0., 0., 0.6),
                        TweenAnim::new(PlanetaryShieldCmp::tween(OWN_COLOR)),
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

                    // Keep the gate opposite the dock on a wider, slower orbit. Its own spin
                    // makes it read as an active portal while avoiding the planet and UI icons.
                    let gate_angle = angle + PI;
                    let gate_radius = planet.size() * 0.9;
                    parent.spawn((
                        Sprite {
                            image: assets.image("jump gate marker"),
                            custom_size: Some(Vec2::splat(planet.size() * 0.36)),
                            ..default()
                        },
                        Transform::from_xyz(
                            gate_angle.cos() * gate_radius,
                            gate_angle.sin() * gate_radius,
                            0.72,
                        ),
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::Linear,
                                Duration::from_secs(17),
                                TransformOrbitSpinLens {
                                    radius: gate_radius,
                                    offset: gate_angle,
                                    rotations: -2.0,
                                },
                            )
                            .with_repeat_count(RepeatCount::Infinite),
                        ),
                        Pickable::IGNORE,
                        Visibility::Hidden,
                        JumpGateCmp,
                    ));

                    // Vertex alpha supplies the scanner's soft field, rim glow, and fading trails.
                    let scanner_material = materials.add(ColorMaterial {
                        color: Color::srgb(0.25, 0.95, 0.62),
                        ..default()
                    });
                    for (index, scanner) in ScannerCmp::layers().into_iter().enumerate() {
                        parent.spawn((
                            Mesh2d::default(),
                            MeshMaterial2d(scanner_material.clone()),
                            Transform::from_xyz(0., 0., -0.12 + index as f32 * 0.01),
                            Pickable::IGNORE,
                            Visibility::Hidden,
                            scanner,
                        ));
                    }
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
            state.end_turn = true;
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

/// Hides map details on menu entry, independently of selection and the show-info preference.
pub fn hide_planet_details(
    mut details: Query<
        &mut Visibility,
        Or<(With<PlanetNameCmp>, With<PlanetResourcesCmp>, With<Icon>, With<ScannerCmp>)>,
    >,
) {
    // Map presentation updates stop while a menu is open, so explicitly clear the last frame's
    // visible details instead of relying on a pointer-out event or normal display preferences.
    for mut visibility in &mut details {
        *visibility = Visibility::Hidden;
    }
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
    mut scanner_q: Query<
        (&mut Visibility, &mut Mesh2d, &mut Transform, &mut ScannerCmp),
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
    time: Res<Time>,
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

            // Show/hide scanner indicator
            if let Ok((mut visibility, mut mesh, mut transform, mut scanner)) =
                scanner_q.get_mut(child)
            {
                // Range previews belong to map hover, using only the local player's scanners.
                let hovered = state.planet_hover == Some(planet.id);
                let mut radius = if hovered && !planet.is_moon() && player.owns(planet) {
                    PHALANX_DISTANCE
                        * Planet::SIZE
                        * planet.army.amount(&Unit::Building(Building::SensorPhalanx)) as f32
                } else if hovered && planet.is_moon() && player.controls(planet) {
                    RADAR_DISTANCE
                        * Planet::SIZE
                        * planet.army.amount(&Unit::Building(Building::OrbitalRadar)) as f32
                } else {
                    0.
                };

                if radius > 0. && !planet.is_destroyed {
                    radius += planet.size() * 0.5; // Start at the edge of the planet

                    *visibility = Visibility::Inherited;
                    scanner.update(
                        radius,
                        time.elapsed_secs_f64(),
                        &mut transform,
                        &mut mesh,
                        &mut meshes,
                    );
                } else {
                    *visibility = Visibility::Hidden;
                }
            }
        }
    }
}

/// Colors visible defenses from the same controller knowledge as territorial borders.
pub fn update_planet_defenses(
    planet_q: Query<(Entity, &PlanetCmp)>,
    children_q: Query<&Children>,
    mut ps_q: Query<(
        &mut Visibility,
        &mut TweenAnim,
        &mut PlanetaryShieldCmp,
        &MeshMaterial2d<ColorMaterial>,
    )>,
    mut dock_q: Query<
        (&mut Visibility, &mut Sprite),
        (With<SpaceDockCmp>, Without<JumpGateCmp>, Without<PlanetaryShieldCmp>),
    >,
    mut gate_q: Query<
        (&mut Visibility, &mut Sprite),
        (With<JumpGateCmp>, Without<SpaceDockCmp>, Without<PlanetaryShieldCmp>),
    >,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    mut materials: ResMut<Assets<ColorMaterial>>,
) {
    for (entity, planet_c) in &planet_q {
        let planet = map.get(planet_c.id);
        let controls = player.controls(planet);
        // Read intelligence once per planet. A hidden capture must not change its displayed color.
        let info = (!controls).then(|| player.last_info(planet, &missions.0)).flatten();
        let (army, controller) = if controls {
            (Some(&planet.army), planet.controlled)
        } else {
            (info.as_ref().map(|info| &info.army), info.as_ref().and_then(|info| info.controlled))
        };
        let has_ps = !planet.is_destroyed
            && army.is_some_and(|army| army.amount(&Unit::planetary_shield()) > 0);
        let has_dock =
            !planet.is_destroyed && army.is_some_and(|army| army.amount(&Unit::space_dock()) > 0);
        let has_gate = !planet.is_destroyed
            && army.is_some_and(|army| army.amount(&Unit::Building(Building::JumpGate)) > 0);
        // Defenses left on an unclaimed world have no player color.
        let color = controller
            .map(|id| session.player_color(id).color())
            .unwrap_or(Color::srgb_u8(190, 198, 210));

        for child in children_q.iter_descendants(entity) {
            if let Ok((mut visibility, mut tween, mut ps, material)) = ps_q.get_mut(child) {
                *visibility = if has_ps {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if has_ps && ps.color != Some(color) {
                    let mut pulse = PlanetaryShieldCmp::tween(color);
                    // Recolor the field without restarting its pulse midway through a fade.
                    pulse.set_elapsed(tween.tweenable().elapsed());
                    match tween.set_tweenable(pulse) {
                        Ok(_) => {
                            ps.color = Some(color);
                            if let Some(mut material) = materials.get_mut(&material.0) {
                                material.color = color.with_alpha(material.color.alpha());
                            }
                        },
                        Err(error) => warn!("Failed to update planetary shield color: {error}"),
                    }
                }
            }
            if let Ok((mut visibility, mut sprite)) = dock_q.get_mut(child) {
                *visibility = if has_dock {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if has_dock {
                    sprite.color = color;
                }
            }
            if let Ok((mut visibility, mut sprite)) = gate_q.get_mut(child) {
                *visibility = if has_gate {
                    Visibility::Inherited
                } else {
                    Visibility::Hidden
                };
                if has_gate {
                    sprite.color = color.with_alpha(0.94);
                }
            }
        }
    }
}

/// Smoothly applies one territory cell or border's target color and visibility.
#[allow(clippy::too_many_arguments)]
fn update_territory_visual(
    visibility: &mut Visibility,
    material: &mut ColorMaterial,
    transition: &mut TerritoryTransitionCmp,
    base_color: Option<Color>,
    target_visible: bool,
    opacity: f32,
    show_cells: bool,
    animate: bool,
    delta_seconds: f32,
) {
    if !show_cells {
        *visibility = Visibility::Hidden;
        // Re-enabling the user's display preference should be immediate, not mistaken for a
        // gameplay ownership change that happened while borders were deliberately hidden.
        transition.initialized = false;
        return;
    }

    let target = base_color.map_or_else(
        || transition.target.with_alpha(0.0),
        |color| {
            color.with_alpha(if target_visible {
                opacity
            } else {
                0.0
            })
        },
    );

    if !transition.initialized {
        material.color = target;
        transition.target = target;
        transition.target_visible = target_visible;
        transition.start = target;
        transition.elapsed = TERRITORY_TRANSITION_SECONDS;
        transition.initialized = true;
        *visibility = if target_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        return;
    }

    if transition.target != target || transition.target_visible != target_visible {
        transition.start = material.color;
        transition.target = target;
        transition.target_visible = target_visible;
        transition.elapsed = 0.0;
    }

    if transition.elapsed < TERRITORY_TRANSITION_SECONDS {
        if animate {
            transition.elapsed =
                (transition.elapsed + delta_seconds).min(TERRITORY_TRANSITION_SECONDS);
        }
        let linear = (transition.elapsed / TERRITORY_TRANSITION_SECONDS).clamp(0.0, 1.0);
        let eased = linear * linear * (3.0 - 2.0 * linear);
        material.color = transition.start.mix(&transition.target, eased);
        *visibility = if transition.elapsed >= TERRITORY_TRANSITION_SECONDS && !target_visible {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    } else {
        material.color = transition.target;
        *visibility = if target_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Updates Voronoi ownership cells with smooth capture, loss, and recolor transitions.
pub fn update_voronoi(
    mut cell_q: Query<(
        &mut Visibility,
        &MeshMaterial2d<ColorMaterial>,
        &VoronoiCmp,
        &mut TerritoryTransitionCmp,
    )>,
    mut edge_q: Query<
        (
            &mut Visibility,
            &MeshMaterial2d<ColorMaterial>,
            &VoronoiEdgeCmp,
            &mut TerritoryTransitionCmp,
        ),
        Without<VoronoiCmp>,
    >,
    settings: Res<Settings>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    time: Option<Res<Time>>,
    game_state: Option<Res<State<GameState>>>,
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
        .collect::<HashMap<PlanetId, PlayerId>>();

    let animate = game_state.as_ref().is_none_or(|state| *state.get() == GameState::Playing);
    let delta_seconds =
        time.as_ref().map_or(TERRITORY_TRANSITION_SECONDS, |time| time.delta_secs());

    for (mut cell_v, cell_m, cell, mut transition) in &mut cell_q {
        let planet = map.get(cell.0);
        let controller = known_controllers.get(&planet.id).copied();
        let visible = !planet.is_destroyed && controller.is_some();
        let base_color = controller.map(|id| session.player_color(id).color());
        if let Some(mut material) = materials.get_mut(&cell_m.0) {
            update_territory_visual(
                &mut cell_v,
                &mut material,
                &mut transition,
                base_color,
                visible,
                0.01,
                settings.show_cells,
                animate,
                delta_seconds,
            );
        }
    }

    let mut counts_by_owner = HashMap::new();

    for (_, _, edge, _) in &edge_q {
        if let Some(&controller) = known_controllers.get(&edge.planet) {
            *counts_by_owner.entry((edge.key, controller)).or_default() += 1;
        }
    }

    for (mut edge_v, edge_m, edge, mut transition) in &mut edge_q {
        let controller = known_controllers.get(&edge.planet).copied();
        let visible = controller.is_some_and(|controller| {
            !map.get(edge.planet).is_destroyed
                && *counts_by_owner.get(&(edge.key, controller)).unwrap_or(&2) <= 1
        });
        let base_color = controller.map(|id| session.player_color(id).color());
        if let Some(mut material) = materials.get_mut(&edge_m.0) {
            update_territory_visual(
                &mut edge_v,
                &mut material,
                &mut transition,
                base_color,
                visible,
                0.58,
                settings.show_cells,
                animate,
                delta_seconds,
            );
        }
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
    pending: Res<crate::multiplayer::client::PendingTurnCommands>,
    player: Res<Player>,
) {
    let playing = *game_state.get() == GameState::Playing;
    for mut button_v in &mut button_c {
        *button_v = if playing && !player.spectator {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    for mut label_v in &mut spectator_q {
        *label_v = if player.spectator && playing {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }

    if playing {
        for mut button_t in &mut button_q {
            button_t.0 = pending.button_label().to_string();
        }
    }

    for mut label_v in &mut label_q {
        *label_v = if playing
            && !pending.resume_requested
            && matches!(
                pending.submission,
                crate::multiplayer::client::SubmissionState::Sending
                    | crate::multiplayer::client::SubmissionState::Accepted
                    | crate::multiplayer::client::SubmissionState::Retry
            )
            && !player.spectator
        {
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
#[path = "../../../tests/core/map_systems.rs"]
mod tests;
