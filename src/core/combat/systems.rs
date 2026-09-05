//! Bevy combat presentation, animation, report navigation, and cleanup systems.

use std::time::Duration;

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use bevy_tweening::lens::{TransformPositionLens, TransformScaleLens};
use bevy_tweening::{
    AnimCompletedEvent, PlaybackState, RepeatCount, RepeatStrategy, Tween, TweenAnim,
};
use strum::IntoEnumIterator;

use crate::core::assets::WorldAssets;
use crate::core::audio::{MuteAudioMsg, PauseAudioMsg, PlayAudioMsg, StopAudioMsg};
use crate::core::camera::MainCamera;
pub use crate::core::combat::effects::{
    restore_combat_camera, run_combat_animations, shake_combat_camera,
};
use crate::core::combat::effects::{Cinematic, PendingImpact, Wreck, DEATH_RAY_DURATION};
use crate::core::combat::playback::{CombatCardHome, CombatRoundJump};
use crate::core::combat::report::Side;
use crate::core::combat::resolution::ShotReport;
use crate::core::constants::{
    BG2_COLOR, COMBAT_BACKGROUND_Z, COMBAT_SHIP_Z, HEALTH_COLOR, PS_SHIELD_PER_LEVEL, PS_WIDTH,
    SETUP_TIME, SHIELD_COLOR, UNIT_SIZE,
};
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::utils::{
    spawn_main_button, UiTransformScaleLens, MAIN_BUTTON_BOTTOM, MAIN_BUTTON_HEIGHT,
    MAIN_BUTTON_RIGHT, MAIN_BUTTON_WIDTH,
};
use crate::core::menu::systems::MenuBackground;
use crate::core::menu::utils::{add_root_node, add_text};
use crate::core::missions::BombingRaid;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{CombatState, GameState};
use crate::core::turns::StartTurnMsg;
use crate::core::ui::systems::{UiCmp, UiState};
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Combat, Unit};
use crate::multiplayer::client::MultiplayerSession;
use crate::utils::NameFromEnum;

const COMBAT_IDENTITY_EDGE_INSET: f32 = 18.0;
const COMBAT_SHIELD_DEFENSE_GAP: f32 = 12.0;
const COMBAT_SPEED_BUTTON_GAP: f32 = 20.0;
const COMBAT_SPEED_WIDTH: f32 = 140.0;
const COMBAT_STATUS_FONT_SIZE: f32 = 36.0;
const COMBAT_STATUS_OFFSET: f32 = -120.0;
const PLANETARY_SHIELD_HEIGHT_FACTOR: f32 = 0.3;

#[derive(Component)]
/// Bevy component marking combat menu presentation entities.
pub struct CombatMenuCmp;

#[derive(Component)]
/// Bevy component marking combat presentation entities.
pub struct CombatCmp;

#[derive(Component)]
/// Bevy component marking background image presentation entities.
pub struct BackgroundImageCmp;

#[derive(Component)]
/// Bevy component marking speed presentation entities.
pub struct SpeedCmp;

#[derive(Component)]
/// Marker for the pause overlay at the combat round-label position.
pub struct CombatPausedCmp;

#[derive(Component)]
/// Bevy component marking display text presentation entities.
pub struct DisplayTextCmp;

#[derive(PartialEq, Default)]
/// Presentation phase for a unit firing during combat playback.
pub enum FireState {
    #[default]
    /// The idle value.
    Idle,
    /// The select value.
    Select,
    /// The pre fire value.
    PreFire,
    /// The firing value.
    Firing,
    /// The deselect value.
    Deselect,
    /// The after fire value.
    AfterFire,
    /// The fired value.
    Fired,
}

impl FireState {
    /// Returns whether this value has fired.
    pub fn has_fired(&self) -> bool {
        matches!(
            self,
            FireState::Firing | FireState::Deselect | FireState::AfterFire | FireState::Fired
        )
    }
}

#[derive(Component)]
/// Bevy component connecting one combat sprite to its unit, side, and animation state.
pub struct CombatUnitCmp {
    /// Unit kind represented by this record or presentation component.
    pub unit: Unit,
    /// Combat side to which the rendered unit belongs.
    pub side: Side,
    /// Current firing-animation phase for the rendered unit.
    pub fire: FireState,
    /// Shield points remaining at this stage of combat.
    pub shield: usize,
    /// Full shield value used to scale the presentation bar.
    pub max_shield: usize,
    /// Hull points remaining at this stage of combat.
    pub hull: usize,
    /// Full hull value used to scale the presentation bar.
    pub max_hull: usize,
}

#[derive(Component)]
/// Bevy component marking pscombat image presentation entities.
pub struct PSCombatImageCmp;

#[derive(Component)]
/// Bevy component marking count presentation entities.
pub struct CountCmp;

#[derive(Component)]
/// Bevy component marking hull presentation entities.
pub struct HullCmp;

#[derive(Component)]
/// Bevy component marking shield presentation entities.
pub struct ShieldCmp;

#[derive(Component)]
/// Bevy component marking death ray presentation entities.
pub struct DeathRayCmp;

#[derive(Message)]
/// Bevy message requesting one visible projectile or beam animation.
pub struct SpawnShotMsg {
    pub(super) shot: ShotReport,
    pub(super) repair: bool,
    pub(super) side: Side,
    /// Firing card and its world-space muzzle; never part of the persisted report.
    pub(super) source: Option<(Entity, Unit, Vec3)>,
}

fn spawn_combat_identity(
    commands: &mut Commands,
    role: &str,
    name: Option<&str>,
    color: Color,
    top: Option<f32>,
    bottom: Option<f32>,
    assets: &WorldAssets,
    window: &Window,
) {
    let has_name = name.is_some();
    let mut node = Node {
        position_type: PositionType::Absolute,
        left: Val::Px(18.0),
        width: Val::Px(if has_name {
            220.0
        } else {
            132.0
        }),
        height: Val::Px(if has_name {
            54.0
        } else {
            38.0
        }),
        padding: UiRect::px(10.0, 12.0, 7.0, 7.0),
        column_gap: Val::Px(10.0),
        align_items: AlignItems::Center,
        overflow: Overflow::clip(),
        ..default()
    };
    if let Some(top) = top {
        node.top = Val::Px(top);
    }
    if let Some(bottom) = bottom {
        node.bottom = Val::Px(bottom);
    }

    commands
        .spawn((
            node,
            BackgroundColor(Color::srgba(0.025, 0.045, 0.07, 0.88)),
            Pickable::IGNORE,
            ZIndex(5),
            CombatCmp,
        ))
        .with_children(|parent| {
            parent.spawn((
                Node {
                    width: Val::Px(3.0),
                    height: Val::Percent(100.0),
                    ..default()
                },
                BackgroundColor(color),
            ));
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Column,
                    justify_content: JustifyContent::Center,
                    row_gap: Val::Px(1.0),
                    overflow: Overflow::clip(),
                    ..default()
                })
                .with_children(|content| {
                    content.spawn((
                        add_text(
                            role.to_uppercase(),
                            "medium",
                            if has_name {
                                7.0
                            } else {
                                9.0
                            },
                            assets,
                            window,
                        ),
                        TextColor(if has_name {
                            Color::srgb_u8(166, 188, 211)
                        } else {
                            color
                        }),
                    ));
                    if let Some(name) = name {
                        content.spawn((
                            add_text(name, "medium", 9.0, assets, window),
                            TextColor(color),
                        ));
                    }
                });
        });
}

/// Creates the combat menu entities and resources required on state entry.
pub fn setup_combat_menu(
    mut commands: Commands,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut pause_audio_msg: MessageWriter<PauseAudioMsg>,
    assets: Res<WorldAssets>,
) {
    pause_audio_msg.write(PauseAudioMsg::new("music"));
    play_audio_msg.write(PlayAudioMsg::new("drums").background());

    commands.spawn((
        Node {
            width: Val::Percent(100.),
            height: Val::Percent(100.),
            position_type: PositionType::Absolute,
            ..default()
        },
        ImageNode::new(assets.image("combat")).with_mode(NodeImageMode::Stretch),
        Pickable {
            should_block_lower: true,
            is_hoverable: false,
        },
        ZIndex(4), // On top of end turn but below audio and Continue buttons.
        MenuBackground,
        CombatMenuCmp,
        UiCmp,
    ));

    spawn_main_button(&mut commands, "Continue", &assets)
        .insert((ZIndex(6), CombatMenuCmp))
        .observe(|_: On<Pointer<Click>>, mut next_game_state: ResMut<NextState<GameState>>| {
            next_game_state.set(GameState::Playing);
        });
}

/// Cleans up combat menu state and retained entities on state exit.
pub fn exit_combat_menu(
    mut start_turn_msg: MessageWriter<StartTurnMsg>,
    mut stop_audio_msg: MessageWriter<StopAudioMsg>,
) {
    start_turn_msg.write(StartTurnMsg::new(true, false));
    stop_audio_msg.write(StopAudioMsg::new("drums"));
}

/// Creates the combat entities and resources required on state entry.
pub fn setup_combat(
    mut commands: Commands,
    settings: Res<Settings>,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    session: Res<MultiplayerSession>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    camera: Single<(&Transform, &Projection), With<MainCamera>>,
    window: Single<&Window>,
    assets: Res<WorldAssets>,
) {
    let (camera_t, projection) = camera.into_inner();

    let pos = camera_t.translation;
    let Projection::Orthographic(projection) = projection else {
        return;
    };

    let (width, height) = (projection.area.width(), projection.area.height());

    play_audio_msg.write(PlayAudioMsg::new("horn"));

    let Some(report) = state
        .in_combat
        .and_then(|report_id| player.reports.iter().find(|report| report.id == report_id))
    else {
        return;
    };
    let Some(destination) = map.try_get(report.mission.destination) else {
        return;
    };

    commands.spawn((
        Sprite {
            image: assets.image(format!("{} large", destination.kind.to_lowername())),
            custom_size: Some(Vec2::new(width, height)),
            ..default()
        },
        Transform::from_xyz(pos.x, pos.y, COMBAT_BACKGROUND_Z),
        Pickable {
            should_block_lower: true,
            is_hoverable: false,
        },
        BackgroundImageCmp,
        CombatCmp,
    ));

    // Spawn units =================================================== >>
    let size = UNIT_SIZE * projection.scale;
    let spacing = size * 1.2;

    let spawn_row = |commands: &mut Commands,
                     units: Vec<(Unit, usize)>,
                     side: Side,
                     y_start: f32,
                     y_end: f32| {
        let total = units.len() as f32;
        let total_width = spacing * (total - 1.0);
        for (i, (u, c)) in units.iter().enumerate() {
            let x = -total_width * 0.5 + i as f32 * spacing;

            let w = size * (0.3 + 0.2 * (1. - 1. / c.to_string().len() as f32));
            let h = size * 0.3;

            commands
                .spawn((
                    Sprite {
                        image: assets.image(u.to_lowername()),
                        custom_size: Some(Vec2::splat(size)),
                        ..default()
                    },
                    Transform::from_xyz(pos.x, y_start, COMBAT_SHIP_Z),
                    CombatCardHome(Vec3::new(pos.x + x, y_end, COMBAT_SHIP_Z)),
                    CombatUnitCmp {
                        unit: *u,
                        side: side.clone(),
                        fire: FireState::Idle,
                        shield: c * u.shield(),
                        max_shield: c * u.shield(),
                        hull: c * u.hull(),
                        max_hull: c * u.hull(),
                    },
                    children![(
                        Sprite {
                            color: Color::BLACK.with_alpha(0.5),
                            custom_size: Some(Vec2::new(w, h)),
                            ..default()
                        },
                        Transform::from_xyz(-size * 0.5 + w * 0.5, -size * 0.5 + h * 0.5, 0.1),
                        children![(
                            Text2d::new(c.to_string()),
                            TextFont {
                                font: assets.font("bold").into(),
                                font_size: (600. * projection.scale).into(),
                                ..default()
                            },
                            TextColor(WHITE.into()),
                            Transform::from_scale(Vec3::splat(0.05)),
                            CountCmp,
                        )]
                    ),],
                    TweenAnim::new(Tween::new(
                        EaseFunction::QuadraticInOut,
                        Duration::from_secs(SETUP_TIME),
                        TransformPositionLens {
                            start: Vec3::new(pos.x, y_start, COMBAT_SHIP_Z),
                            end: Vec3::new(pos.x + x, y_end, COMBAT_SHIP_Z),
                        },
                    )),
                    Pickable::IGNORE,
                    CombatCmp,
                ))
                .with_children(|parent| {
                    // Missing stats have no bar. Shrinking a full bar toward zero leaves
                    // subpixel slivers that flicker along the left edge of missile cards.
                    if u.shield() > 0 {
                        parent.spawn((
                            Sprite {
                                color: BG2_COLOR,
                                custom_size: Some(Vec2::new(size, size * 0.14)),
                                ..default()
                            },
                            Transform::from_xyz(0., -size * 0.57, 0.1),
                            children![(
                                Sprite {
                                    color: SHIELD_COLOR,
                                    custom_size: Some(Vec2::new(size * 0.96, size * 0.14 * 0.75)),
                                    ..default()
                                },
                                Transform::from_xyz(0., 0., 0.2),
                                ShieldCmp,
                            )],
                        ));
                    }
                    if u.hull() > 0 {
                        parent.spawn((
                            Sprite {
                                color: BG2_COLOR,
                                custom_size: Some(Vec2::new(size, size * 0.14)),
                                ..default()
                            },
                            Transform::from_xyz(0., -size * 0.69, 0.1),
                            children![(
                                Sprite {
                                    color: HEALTH_COLOR,
                                    custom_size: Some(Vec2::new(size * 0.96, size * 0.14 * 0.75)),
                                    ..default()
                                },
                                Transform::from_xyz(0., 0., 0.2),
                                HullCmp,
                            )],
                        ));
                    }
                });
        }
    };

    let attacker_id = report.mission.owner;
    let defender_id = report.planet.controlled.or(report.planet.owned);
    let attack_c = session.player_color(attacker_id).color();
    let defend_c =
        defender_id.map_or(Color::srgb_u8(150, 158, 170), |id| session.player_color(id).color());
    let attacker_name = session
        .player_name(attacker_id)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Player {attacker_id}"));
    let defender_name = defender_id.map(|id| {
        session.player_name(id).map(str::to_owned).unwrap_or_else(|| format!("Player {id}"))
    });

    spawn_combat_identity(
        &mut commands,
        "Attacker",
        Some(&attacker_name),
        attack_c,
        Some(COMBAT_IDENTITY_EDGE_INSET),
        None,
        &assets,
        &window,
    );
    spawn_combat_identity(
        &mut commands,
        "Defender",
        defender_name.as_deref(),
        defend_c,
        None,
        Some(COMBAT_IDENTITY_EDGE_INSET),
        &assets,
        &window,
    );

    let attacking = Unit::all()
        .into_iter()
        .flatten()
        .filter_map(|u| {
            let amount = report.mission.army.amount(&u);
            (u != Unit::colony_ship() && amount > 0).then_some((u, amount))
        })
        .collect::<Vec<_>>();

    // Keep fleet cards out of the fixed UI identity cards at either screen edge.
    // Multiplying the pixel inset by the projection scale preserves the gap while zoomed.
    let attacker_row_y = pos.y + height * 0.5 - 150.0 * projection.scale;
    let defender_edge_row_y = pos.y - height * 0.5 + 180.0 * projection.scale;

    spawn_row(&mut commands, attacking, Side::Attacker, pos.y + height * 0.8, attacker_row_y);

    let defending_def = Unit::defenses()
        .into_iter()
        .filter_map(|u| {
            let amount = report.planet.army.amount(&u);
            ((!u.is_missile()
                || report.mission.objective == Icon::MissileStrike
                    && u == Unit::antiballistic_missile())
                && u != Unit::space_dock()
                && amount > 0)
                .then_some((u, amount))
        })
        .collect::<Vec<_>>();

    let defending_ships = if report.mission.objective != Icon::MissileStrike {
        Unit::ships()
            .into_iter()
            .chain(vec![Unit::space_dock()])
            .filter_map(|u| {
                let amount = report.planet.army.amount(&u);
                (u != Unit::colony_ship() && amount > 0).then_some((u, amount))
            })
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };

    let ps = report.planet.army.amount(&Unit::planetary_shield());
    let draw_ps = ps > 0
        && report.mission.objective != Icon::MissileStrike
        && (!defending_def.is_empty() || report.mission.bombing != BombingRaid::None);

    // Keep the ground-defense cards below the planetary-shield bar. Deriving the row from
    // both sprite heights preserves the gap across window sizes and projection scales.
    let defense_row_y = if draw_ps {
        let shield_y = pos.y - height * 0.25;
        let shield_height = size * PLANETARY_SHIELD_HEIGHT_FACTOR;
        shield_y - (size + shield_height) * 0.5 - COMBAT_SHIELD_DEFENSE_GAP * projection.scale
    } else {
        defender_edge_row_y
    };

    let ship_y = if defending_def.is_empty() && !draw_ps {
        defender_edge_row_y
    } else {
        pos.y - height * 0.1
    };

    spawn_row(&mut commands, defending_def, Side::Defender, pos.y - height * 0.7, defense_row_y);
    spawn_row(&mut commands, defending_ships, Side::Defender, pos.y - height * 0.7, ship_y);

    // Spawn Planetary Shield image
    if draw_ps {
        let (bar_width, bar_height) = (size * PS_WIDTH, size * PLANETARY_SHIELD_HEIGHT_FACTOR);
        let w = size * 0.3;

        commands.spawn((
            Sprite {
                color: BG2_COLOR,
                custom_size: Some(Vec2::new(bar_width, bar_height)),
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y - height * 0.7, COMBAT_SHIP_Z),
            CombatCardHome(Vec3::new(pos.x, pos.y - height * 0.25, COMBAT_SHIP_Z)),
            CombatUnitCmp {
                unit: Unit::planetary_shield(),
                side: Side::Defender,
                fire: FireState::Idle,
                shield: ps * PS_SHIELD_PER_LEVEL,
                max_shield: ps * PS_SHIELD_PER_LEVEL,
                hull: ps,
                max_hull: ps,
            },
            children![
                (
                    Sprite {
                        color: SHIELD_COLOR,
                        custom_size: Some(Vec2::new(bar_width * 0.997, bar_height * 0.9)),
                        ..default()
                    },
                    Transform::from_xyz(0., 0., 0.1),
                    ShieldCmp,
                ),
                (
                    Sprite {
                        image: assets.image("planetary shield"),
                        custom_size: Some(Vec2::splat(size)),
                        ..default()
                    },
                    Transform::from_xyz((-bar_width + size) * 0.5, (-bar_height - size) * 0.5, 0.,),
                    PSCombatImageCmp,
                    children![(
                        Sprite {
                            color: Color::BLACK.with_alpha(0.5),
                            custom_size: Some(Vec2::splat(w)),
                            ..default()
                        },
                        Transform::from_xyz(-size * 0.5 + w * 0.5, -size * 0.5 + w * 0.5, 0.1),
                        children![(
                            Text2d::new(ps.to_string()),
                            TextFont {
                                font: assets.font("bold").into(),
                                font_size: (600. * projection.scale).into(),
                                ..default()
                            },
                            TextColor(WHITE.into()),
                            Transform::from_scale(Vec3::splat(0.05)),
                        )]
                    )],
                )
            ],
            TweenAnim::new(Tween::new(
                EaseFunction::QuadraticInOut,
                Duration::from_secs(SETUP_TIME),
                TransformPositionLens {
                    start: Vec3::new(pos.x, pos.y - height * 0.7, COMBAT_SHIP_Z),
                    end: Vec3::new(pos.x, pos.y - height * 0.25, COMBAT_SHIP_Z),
                },
            )),
            Pickable::IGNORE,
            CombatCmp,
        ));
    }

    // Spawn buildings when bombing
    let buildings = match report.mission.bombing {
        BombingRaid::Economic => Unit::resource_buildings()
            .into_iter()
            .filter_map(|u| {
                let amount = report.planet.army.amount(&u);
                (amount > 0).then_some((u, amount))
            })
            .collect::<Vec<_>>(),
        BombingRaid::Industrial => Unit::industrial_buildings()
            .into_iter()
            .filter_map(|u| {
                let amount = report.planet.army.amount(&u);
                (amount > 0).then_some((u, amount))
            })
            .collect::<Vec<_>>(),
        BombingRaid::None => Vec::new(),
    };

    if !buildings.is_empty() {
        let size = size * 0.65;
        let spacing = size * 1.1;
        let total_width = spacing * (buildings.len() as f32 - 1.0);

        for (i, (u, c)) in buildings.iter().enumerate() {
            let x = -total_width * 0.5 + i as f32 * spacing;
            let w = size * 0.5;

            commands.spawn((
                Sprite {
                    image: assets.image(u.to_lowername()),
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y - height * 0.7, COMBAT_SHIP_Z),
                CombatCardHome(Vec3::new(
                    pos.x + size * 8.25 + x,
                    pos.y - height * 0.34,
                    COMBAT_SHIP_Z,
                )),
                CombatUnitCmp {
                    unit: *u,
                    side: Side::Defender,
                    fire: FireState::Idle,
                    shield: 0,
                    max_shield: 0,
                    hull: *c,
                    max_hull: *c,
                },
                children![(
                    Sprite {
                        color: Color::BLACK.with_alpha(0.5),
                        custom_size: Some(Vec2::splat(w)),
                        ..default()
                    },
                    Transform::from_xyz(-size * 0.5 + w * 0.5, -size * 0.5 + w * 0.5, 0.1),
                    children![(
                        Text2d::new(c.to_string()),
                        TextFont {
                            font: assets.font("bold").into(),
                            font_size: (600. * projection.scale).into(),
                            ..default()
                        },
                        TextColor(WHITE.into()),
                        Transform::from_scale(Vec3::splat(0.05)),
                        CountCmp,
                    )]
                )],
                TweenAnim::new(Tween::new(
                    EaseFunction::QuadraticInOut,
                    Duration::from_secs(SETUP_TIME),
                    TransformPositionLens {
                        start: Vec3::new(pos.x, pos.y - height * 0.7, COMBAT_SHIP_Z),
                        end: Vec3::new(
                            pos.x + size * 8.25 + x,
                            pos.y - height * 0.34,
                            COMBAT_SHIP_Z,
                        ),
                    },
                )),
                Pickable::IGNORE,
                CombatCmp,
            ));
        }
    }

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(MAIN_BUTTON_BOTTOM),
                right: Val::Px(MAIN_BUTTON_RIGHT + MAIN_BUTTON_WIDTH + COMBAT_SPEED_BUTTON_GAP),
                width: Val::Px(COMBAT_SPEED_WIDTH),
                height: Val::Px(MAIN_BUTTON_HEIGHT),
                justify_content: JustifyContent::FlexEnd,
                align_items: AlignItems::Center,
                ..default()
            },
            Pickable::IGNORE,
            ZIndex(6),
            CombatCmp,
        ))
        .with_child((
            add_text(format!("{}x", settings.combat_speed), "medium", 10., &assets, &window),
            SpeedCmp,
        ));

    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(100.),
                height: Val::Percent(105.),
                align_items: AlignItems::Center,
                justify_content: JustifyContent::Center,
                ..default()
            },
            if settings.combat_paused {
                Visibility::Inherited
            } else {
                Visibility::Hidden
            },
            Pickable::IGNORE,
            ZIndex(7),
            CombatPausedCmp,
            CombatCmp,
        ))
        .with_child((
            add_text("PAUSED", "medium", COMBAT_STATUS_FONT_SIZE, &assets, &window),
            UiTransform::from_translation(Val2::new(Val::ZERO, Val::Percent(COMBAT_STATUS_OFFSET))),
            TextShadow::default(),
            Pickable::IGNORE,
        ));

    spawn_main_button(&mut commands, "Exit combat", &assets)
        .insert((ZIndex(6), CombatCmp))
        .observe(|_: On<Pointer<Click>>, mut next_game_state: ResMut<NextState<GameState>>| {
            next_game_state.set(GameState::CombatMenu);
        });
}

/// Advances the combat presentation state machine after each animation timer.
pub fn animate_combat(
    mut commands: Commands,
    bg_q: Single<&mut Sprite, With<BackgroundImageCmp>>,
    text_q: Option<Single<Entity, With<DisplayTextCmp>>>,
    mut unit_q: Query<(Entity, &Transform, &mut CombatUnitCmp)>,
    death_ray_q: Query<Entity, With<DeathRayCmp>>,
    mut state: ResMut<UiState>,
    player: Res<Player>,
    combat_state: Res<State<CombatState>>,
    mut next_combat_state: ResMut<NextState<CombatState>>,
    mut spawn_shot_msg: MessageWriter<SpawnShotMsg>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut anim_completed_msg: MessageReader<AnimCompletedEvent>,
    camera: Single<(&Transform, &Projection), With<MainCamera>>,
    presentation: (Res<WorldAssets>, Single<&Window>, Option<Res<CombatRoundJump>>),
    pending_q: Query<(), Or<(With<PendingImpact>, With<Wreck>)>>,
    settings: Res<Settings>,
) {
    let (assets, window, round_jump) = presentation;
    if settings.combat_paused
        || (round_jump.is_some() && !matches!(*next_combat_state, NextState::Unchanged))
    {
        return;
    }
    let (camera_t, projection) = camera.into_inner();

    let pos = camera_t.translation;
    let Projection::Orthographic(projection) = projection else {
        return;
    };

    let units: Vec<_> = Unit::all_firing_order();

    let Some((report, combat, round)) = state.in_combat.and_then(|report_id| {
        let report = player.reports.iter().find(|report| report.id == report_id)?;
        let combat = report.combat_report.as_ref()?;
        let round = combat.rounds.get(state.combat_round)?;
        Some((report, combat, round))
    }) else {
        return;
    };

    let size = UNIT_SIZE * projection.scale;

    if matches!(
        combat_state.get(),
        CombatState::AntiBallistic
            | CombatState::Fire
            | CombatState::Repair
            | CombatState::Bomb
            | CombatState::DeathRay
    ) && unit_q.iter().all(|(_, _, cu)| matches!(cu.fire, FireState::Idle | FireState::Fired))
    {
        // Keep consuming tween completion messages while projectiles travel, but never
        // advance a round or remove simultaneous return-fire cards before they arrive.
        if !pending_q.is_empty() {
            return;
        }
        for side in Side::iter() {
            // Follow recorded weapon fire even after the last defender dies: shields
            // can still be hit, and simultaneous return fire can kill would-be Bombers.
            for unit in &units {
                if let Some((_, _, mut cu)) = unit_q.iter_mut().find(|(_, _, cu)| {
                    cu.fire == FireState::Idle
                        && cu.unit == *unit
                        && cu.side == side
                        && (cu.unit.damage() > 0 || cu.unit == Unit::crawler())
                        && (cu.unit == Unit::crawler()
                            || round.units(&side).iter().any(|shooter| {
                                shooter.unit == *unit
                                    && shooter.shots.iter().any(|shot| !shot.is_bombing())
                            }))
                        && (cu.unit != Unit::interplanetary_missile()
                            || round.missiles_shot() < round.n_missiles())
                }) {
                    cu.fire = FireState::Select;
                    return;
                }
            }
        }

        // No more units to fire -> explode destroyed units
        let mut destroying = false;
        for (unit_e, unit_t, cu) in &mut unit_q {
            if cu.hull == 0
                && cu.unit != Unit::planetary_shield()
                && (cu.unit != Unit::antiballistic_missile()
                    || round.antiballistic_fired >= round.n_antiballistic())
            {
                destroying = true;
                commands.entity(unit_e).insert(Wreck::new(unit_t.translation, size, cu.unit));
                // The wreck sequence owns removal after its staggered secondary blasts.
            }
        }
        if destroying {
            return;
        }

        // Scout probes fly away
        if state.combat_round == 0 && combat.rounds.len() > 1 {
            if let Some((unit_e, unit_t, _)) = unit_q.iter_mut().find(|(_, _, cu)| {
                cu.hull > 0 && cu.unit == Unit::probe() && cu.side == Side::Attacker
            }) {
                commands.entity(unit_e).insert(TweenAnim::new(Tween::new(
                    EaseFunction::QuadraticIn,
                    Duration::from_secs(SETUP_TIME),
                    TransformPositionLens {
                        start: unit_t.translation,
                        end: Vec3::new(
                            pos.x,
                            pos.y + projection.area.height() * 0.9,
                            COMBAT_SHIP_Z + 0.9,
                        ),
                    },
                )));
            }
        }

        // Crawlers repair defense turrets
        if round.units(&Side::Defender).iter().any(|cu| cu.repairs.iter().any(|r| *r > 0))
            && *combat_state.get() == CombatState::Fire
        {
            if let Some((_, _, mut cu)) =
                unit_q.iter_mut().find(|(_, _, cu)| cu.hull > 0 && cu.unit == Unit::crawler())
            {
                cu.fire = FireState::Select;
                next_combat_state.set(CombatState::Repair);
                return;
            }
        }

        // Replay only a recorded building raid. Weapon shots at the planetary shield
        // belong to Fire and must not trigger a second Bomber animation.
        if report.mission.bombing != BombingRaid::None
            && round
                .units(&Side::Attacker)
                .iter()
                .any(|cu| cu.shots.iter().any(ShotReport::is_bombing))
            && matches!(combat_state.get(), CombatState::Fire | CombatState::Repair)
        {
            if let Some((_, _, mut cu)) = unit_q.iter_mut().find(|(_, _, cu)| {
                cu.hull > 0 && cu.unit == Unit::Ship(Ship::Bomber) && cu.side == Side::Attacker
            }) {
                cu.fire = FireState::Select;
                next_combat_state.set(CombatState::Bomb);
                return;
            }
        }

        // Death ray
        if report.mission.objective == Icon::Destroy
            && round.destroy_probability > 0.
            && matches!(
                combat_state.get(),
                CombatState::Fire | CombatState::Repair | CombatState::Bomb
            )
        {
            if let Some((_, _, mut cu)) = unit_q.iter_mut().find(|(_, _, cu)| {
                cu.hull > 0 && cu.unit == Unit::war_sun() && cu.side == Side::Attacker
            }) {
                cu.fire = FireState::Select;
                next_combat_state.set(CombatState::DeathRay);
                return;
            }
        }

        next_combat_state.set(if state.combat_round == combat.rounds.len() - 1 {
            CombatState::EndCombat
        } else {
            state.combat_round += 1;
            CombatState::DisplayRound
        });
        return;
    }

    match combat_state.get() {
        CombatState::Setup => {
            if !anim_completed_msg.is_empty() {
                anim_completed_msg.clear();
                next_combat_state.set(
                    if let Some((_, _, mut cu)) = unit_q
                        .iter_mut()
                        .find(|(_, _, cu)| cu.unit == Unit::antiballistic_missile())
                    {
                        cu.fire = FireState::Select;
                        CombatState::AntiBallistic
                    } else {
                        CombatState::DisplayRound
                    },
                );
            }
        },
        CombatState::DisplayRound => {
            if let Some(round_q) = text_q {
                let entity = round_q.into_inner();
                for message in anim_completed_msg.read() {
                    if entity == message.anim_entity {
                        next_combat_state.set(CombatState::Fire);
                        commands.entity(message.anim_entity).despawn();
                    }
                }
            } else {
                commands.remove_resource::<CombatRoundJump>();
                // Reset all stats
                unit_q.iter_mut().for_each(|(_, _, mut cu)| {
                    if cu.unit != Unit::planetary_shield() {
                        let count =
                            round.units(&cu.side).iter().filter(|cu2| cu.unit == cu2.unit).count();

                        cu.max_shield = count * cu.unit.shield();
                        cu.max_hull = count * cu.unit.hull();
                        cu.shield = cu.max_shield;
                        cu.fire = FireState::Idle;
                    }
                });

                if combat.rounds.len() == 1 {
                    next_combat_state.set(CombatState::Fire);
                    return;
                }

                commands.spawn((
                    add_root_node(false),
                    children![(
                        add_text(
                            format!("Round {}", state.combat_round + 1),
                            "medium",
                            COMBAT_STATUS_FONT_SIZE,
                            &assets,
                            &window,
                        ),
                        TextShadow::default(),
                        UiTransform {
                            translation: Val2::new(Val::ZERO, Val::Percent(COMBAT_STATUS_OFFSET),),
                            scale: Vec2::ZERO,
                            ..default()
                        },
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::QuadraticInOut,
                                Duration::from_millis(if round_jump.is_some() {
                                    250
                                } else {
                                    1500
                                }),
                                UiTransformScaleLens {
                                    start: Vec2::ZERO,
                                    end: Vec2::ONE,
                                },
                            )
                            .with_repeat_count(RepeatCount::Finite(2))
                            .with_repeat_strategy(RepeatStrategy::MirroredRepeat)
                        ),
                        DisplayTextCmp,
                        CombatCmp, // Required for animation speed
                    )],
                    CombatCmp,
                ));
            }
        },
        CombatState::AntiBallistic
        | CombatState::Fire
        | CombatState::Repair
        | CombatState::Bomb
        | CombatState::DeathRay => {
            for (unit_e, unit_t, mut cu) in &mut unit_q {
                match cu.fire {
                    FireState::Select => {
                        commands.entity(unit_e).insert(TweenAnim::new(Tween::new(
                            EaseFunction::QuadraticInOut,
                            Duration::from_millis(500),
                            TransformScaleLens {
                                start: unit_t.scale,
                                end: unit_t.scale * 1.3,
                            },
                        )));
                        cu.fire = FireState::PreFire;
                    },
                    FireState::PreFire => {
                        for message in anim_completed_msg.read() {
                            if unit_e == message.anim_entity {
                                cu.fire = FireState::Firing;
                            }
                        }
                    },
                    FireState::Firing if *combat_state.get() == CombatState::Repair => {
                        let repaired = round
                            .units(&cu.side)
                            .iter()
                            .flat_map(|cu2| cu2.repairs.iter().map(move |r| (cu2.unit, r)))
                            .collect::<Vec<_>>();

                        for (unit, repair) in repaired {
                            // Hack the repair info into the shot report for code simplicity
                            spawn_shot_msg.write(SpawnShotMsg {
                                shot: ShotReport {
                                    unit: Some(unit),
                                    hull_damage: *repair,
                                    ..default()
                                },
                                repair: true,
                                side: cu.side.clone(),
                                source: Some((unit_e, cu.unit, unit_t.translation)),
                            });
                        }

                        cu.fire = FireState::Deselect;
                    },
                    FireState::Firing if *combat_state.get() == CombatState::DeathRay => {
                        if let Some(ray_e) = death_ray_q.iter().next() {
                            for message in anim_completed_msg.read() {
                                if ray_e == message.anim_entity {
                                    commands.entity(ray_e).despawn();
                                    cu.fire = FireState::Deselect;
                                    if report.planet_destroyed
                                        && state.combat_round == combat.rounds.len() - 1
                                    {
                                        bg_q.into_inner().image = assets.image("destroyed bg");
                                        return;
                                    }
                                }
                            }
                        } else {
                            commands.spawn((
                                Cinematic::new(
                                    unit_t.translation,
                                    pos,
                                    projection.area.size(),
                                    size,
                                    report.planet_destroyed
                                        && state.combat_round == combat.rounds.len() - 1,
                                ),
                                // TweenAnim requires a concrete target; a bare Delay panics.
                                // This stationary tween times the cinematic and emits completion.
                                Transform::default(),
                                TweenAnim::new(Tween::new(
                                    EaseFunction::Linear,
                                    Duration::from_secs_f32(DEATH_RAY_DURATION),
                                    TransformScaleLens {
                                        start: Vec3::ONE,
                                        end: Vec3::ONE,
                                    },
                                )),
                                DeathRayCmp,
                                CombatCmp,
                            ));
                            play_audio_msg.write(PlayAudioMsg::new("death ray"));
                        }
                    },
                    FireState::Firing => {
                        let shots = round
                            .units(&cu.side)
                            .iter()
                            .filter(|cu2| cu.unit == cu2.unit)
                            .flat_map(|cu2| &cu2.shots)
                            .filter(|s| {
                                s.is_bombing() == (*combat_state.get() == CombatState::Bomb)
                            })
                            .collect::<Vec<_>>();

                        for shot in shots {
                            spawn_shot_msg.write(SpawnShotMsg {
                                shot: shot.clone(),
                                repair: false,
                                side: cu.side.opposite(),
                                source: Some((unit_e, cu.unit, unit_t.translation)),
                            });
                        }

                        cu.fire = FireState::Deselect;
                    },
                    FireState::Deselect => {
                        commands.entity(unit_e).insert(TweenAnim::new(Tween::new(
                            EaseFunction::QuarticIn,
                            Duration::from_millis(1500),
                            TransformScaleLens {
                                start: unit_t.scale,
                                end: unit_t.scale / 1.3,
                            },
                        )));
                        cu.fire = FireState::AfterFire;
                    },
                    FireState::AfterFire => {
                        for message in anim_completed_msg.read() {
                            if unit_e == message.anim_entity {
                                cu.fire = FireState::Fired;
                            }
                        }
                    },
                    _ => (),
                }
            }
        },
        CombatState::EndCombat => {
            if text_q.is_none() {
                let result = report.status(&player);

                play_audio_msg.write(PlayAudioMsg::new(result));
                commands.spawn((
                    add_root_node(false),
                    children![(
                        Node {
                            max_width: Val::Vw(90.),
                            ..default()
                        },
                        ImageNode::new(assets.image(result)),
                        UiTransform {
                            translation: Val2::new(Val::ZERO, Val::Percent(-10.)),
                            scale: Vec2::ZERO,
                            ..default()
                        },
                        TweenAnim::new(Tween::new(
                            EaseFunction::QuadraticInOut,
                            Duration::from_millis(1500),
                            UiTransformScaleLens {
                                start: Vec2::ZERO,
                                end: Vec2::splat(match result {
                                    "victory" => 0.55,
                                    "draw" => 0.4,
                                    "defeat" => 0.6,
                                    _ => 0.5,
                                }),
                            },
                        )),
                        DisplayTextCmp,
                        CombatCmp,
                    )],
                    CombatCmp,
                ));
            }
        },
    }
}

/// Updates combat stats from the current canonical ECS projection.
pub fn update_combat_stats(
    unit_q: Query<(Entity, &CombatUnitCmp)>,
    mut anim_q: Query<&mut TweenAnim, With<CombatCmp>>,
    mut count_q: Query<&mut Text2d, With<CountCmp>>,
    mut shield_q: Query<(&mut Transform, &mut Sprite), With<ShieldCmp>>,
    mut hull_q: Query<(&mut Transform, &mut Sprite), (With<HullCmp>, Without<ShieldCmp>)>,
    mut speed_q: Single<&mut Text, With<SpeedCmp>>,
    mut paused_q: Single<&mut Visibility, With<CombatPausedCmp>>,
    mut display_q: Query<&mut Visibility, (With<DisplayTextCmp>, Without<CombatPausedCmp>)>,
    children_q: Query<&Children>,
    settings: Res<Settings>,
    state: Res<UiState>,
    player: Res<Player>,
    combat_state: Res<State<CombatState>>,
    camera_q: Single<&Projection, With<MainCamera>>,
    time: Res<Time>,
) {
    let Projection::Orthographic(projection) = camera_q.into_inner() else {
        return;
    };

    // Update speed indicator
    anim_q.iter_mut().for_each(|mut t| {
        if settings.combat_paused {
            t.playback_state = PlaybackState::Paused;
        } else {
            t.playback_state = PlaybackState::Playing;
            t.speed = settings.combat_speed as f64;
        }
    });

    speed_q.as_mut().0 = format!("{}x", settings.combat_speed);
    **paused_q = if settings.combat_paused {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
    // Pause replaces the round/result presentation while its animation is frozen.
    for mut visibility in &mut display_q {
        *visibility = if settings.combat_paused {
            Visibility::Hidden
        } else {
            Visibility::Inherited
        };
    }

    let Some((_report, _combat, round)) = state.in_combat.and_then(|report_id| {
        let report = player.reports.iter().find(|report| report.id == report_id)?;
        let combat = report.combat_report.as_ref()?;
        let round = combat.rounds.get(state.combat_round)?;
        Some((report, combat, round))
    }) else {
        return;
    };

    let size = UNIT_SIZE * projection.scale;
    let speed = (3. * time.delta_secs() * settings.speed()).clamp(0., 1.);

    let antiballistic_fired = unit_q
        .iter()
        .any(|(_, cu)| cu.unit == Unit::antiballistic_missile() && cu.fire.has_fired());
    let interplanetary_fired = unit_q
        .iter()
        .any(|(_, cu)| cu.unit == Unit::interplanetary_missile() && cu.fire.has_fired());

    for (unit_e, cu) in &unit_q {
        for child in children_q.iter_descendants(unit_e) {
            if let Ok(mut text) = count_q.get_mut(child) {
                let count = if cu.unit.is_building() {
                    cu.hull
                } else {
                    let mut count = round
                        .units(&cu.side)
                        .iter()
                        .filter(|cu2| {
                            cu2.unit == cu.unit
                                && (*combat_state.get() != CombatState::EndCombat
                                    || cu2.hull > 0
                                    || cu.unit.is_missile())
                        })
                        .count();

                    // Update the missile count immediately after antiballistic were fired
                    if cu.unit == Unit::antiballistic_missile() && antiballistic_fired {
                        count -= round.antiballistic_fired;
                    }
                    if cu.unit == Unit::interplanetary_missile() {
                        if interplanetary_fired {
                            count = 0;
                        } else if antiballistic_fired {
                            count -= round
                                .defender
                                .iter()
                                .filter(|cu| {
                                    cu.unit == Unit::antiballistic_missile()
                                        && cu.shots.iter().any(|s| s.killed)
                                })
                                .count();
                        }
                    }

                    count
                };

                text.0 = count.to_string();
            }

            if let Ok((mut shield_t, mut shield_s)) = shield_q.get_mut(child) {
                if let Some(shield_size) = shield_s.custom_size.as_mut() {
                    let full_size = if cu.unit == Unit::planetary_shield() {
                        size * PS_WIDTH * 0.997
                    } else {
                        size * 0.96
                    };
                    shield_size.x = shield_size
                        .x
                        .lerp(full_size * cu.shield as f32 / cu.max_shield.max(1) as f32, speed)
                        .clamp(0., full_size);
                    shield_t.translation.x = (shield_size.x - full_size) * 0.5;
                }
            }

            if let Ok((mut hull_t, mut hull_s)) = hull_q.get_mut(child) {
                if let Some(hull_size) = hull_s.custom_size.as_mut() {
                    let full_size = size * 0.96;
                    hull_size.x = hull_size
                        .x
                        .lerp(full_size * cu.hull as f32 / cu.max_hull.max(1) as f32, speed)
                        .clamp(0., full_size);
                    hull_t.translation.x = (hull_size.x - full_size) * 0.5;
                }
            }
        }
    }
}

/// Cleans up combat state and retained entities on state exit.
pub fn exit_combat(
    mut commands: Commands,
    mut state: ResMut<UiState>,
    mut next_combat_state: ResMut<NextState<CombatState>>,
    mut mute_audio_msg: MessageWriter<MuteAudioMsg>,
) {
    commands.remove_resource::<CombatRoundJump>();
    state.combat_round = 0;
    mute_audio_msg.write(MuteAudioMsg);
    next_combat_state.set(CombatState::default());
}

#[cfg(test)]
#[path = "../../../tests/core/combat_animation.rs"]
mod tests;
