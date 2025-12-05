use std::time::Duration;

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use bevy_tweening::lens::TransformScaleLens;
use bevy_tweening::{
    AnimCompletedEvent, EntityCommandsTweeningExtensions, RepeatCount, RepeatStrategy, Tween,
    TweenAnim,
};
use rand::{rng, Rng};
use strum::IntoEnumIterator;

use crate::core::assets::WorldAssets;
use crate::core::audio::{PauseAudioMsg, PlayAudioMsg, StopAudioMsg};
use crate::core::camera::MainCamera;
use crate::core::combat::combat::ShotReport;
use crate::core::combat::report::Side;
use crate::core::constants::{
    BG2_COLOR, COMBAT_BACKGROUND_Z, COMBAT_EXPLOSION_Z, COMBAT_SHIP_Z, ENEMY_COLOR, OWN_COLOR,
    PS_SHIELD_PER_LEVEL, PS_WIDTH, SHIELD_COLOR, TITLE_TEXT_SIZE, UNIT_SIZE,
};
use crate::core::map::map::Map;
use crate::core::map::utils::{spawn_main_button, UiTransformScaleLens};
use crate::core::menu::utils::{add_root_node, add_text};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{CombatState, GameState};
use crate::core::turns::StartTurnMsg;
use crate::core::ui::systems::UiState;
use crate::core::units::{Amount, Combat, Unit};
use crate::utils::{scale_duration, NameFromEnum};

#[derive(Component)]
pub struct CombatMenuCmp;

#[derive(Component)]
pub struct CombatCmp;

#[derive(Component)]
pub struct BackgroundImageCmp;

#[derive(Component)]
pub struct SpeedCmp;

#[derive(Component)]
pub struct DisplayRoundCmp;

#[derive(PartialEq, Default)]
pub enum FireState {
    #[default]
    Idle,
    Select,
    PreFire,
    Firing,
    Deselect,
    AfterFire,
    Fired,
}

#[derive(Component)]
pub struct CombatUnitCmp {
    pub unit: Unit,
    pub side: Side,
    pub fire: FireState,
    pub shield: usize,
    pub max_shield: usize,
    pub hull: usize,
    pub max_hull: usize,
}

#[derive(Component)]
pub struct PSCombatCmp {
    pub shield: usize,
    pub max_shield: usize,
}

#[derive(Component)]
pub struct PSCombatImageCmp;

#[derive(Component)]
pub struct CountCmp;

#[derive(Component)]
pub struct HullCmp;

#[derive(Component)]
pub struct ShieldCmp;

#[derive(Message)]
pub struct SpawnShotMsg {
    shot: ShotReport,
    side: Side,
}

#[derive(Component)]
pub struct UnitExplosionCmp {
    pub timer: Timer,
    pub delay: Timer,
    pub last_index: usize,
    pub target_entity: Entity,
}

pub fn setup_combat_menu(
    mut commands: Commands,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut pause_audio_msg: MessageWriter<PauseAudioMsg>,
    assets: Local<WorldAssets>,
) {
    pause_audio_msg.write(PauseAudioMsg::new("music"));
    play_audio_msg.write(PlayAudioMsg::new("drums").background());

    commands.spawn((add_root_node(true), ImageNode::new(assets.image("combat")), CombatMenuCmp));

    spawn_main_button(&mut commands, "Continue", &assets)
        .insert((ZIndex(6), CombatMenuCmp))
        .observe(
            |_: On<Pointer<Click>>,
             mut start_turn_msg: MessageWriter<StartTurnMsg>,
             mut next_game_state: ResMut<NextState<GameState>>| {
                start_turn_msg.write(StartTurnMsg::new(true, false));
                next_game_state.set(GameState::Playing);
            },
        );
}

pub fn exit_combat_menu(mut stop_audio_msg: MessageWriter<StopAudioMsg>) {
    stop_audio_msg.write(StopAudioMsg::new("drums"));
}

pub fn setup_combat(
    mut commands: Commands,
    settings: Res<Settings>,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    camera: Single<(&Transform, &Projection), With<MainCamera>>,
    window: Single<&Window>,
    assets: Local<WorldAssets>,
) {
    let (camera_t, projection) = camera.into_inner();

    let pos = camera_t.translation;
    let Projection::Orthographic(projection) = projection else {
        panic!("Expected Orthographic projection.");
    };

    let (width, height) = (projection.area.width(), projection.area.height());

    play_audio_msg.write(PlayAudioMsg::new("horn"));

    let report = player.reports.iter().find(|r| r.id == state.in_combat.unwrap()).unwrap();
    let destination = map.get(report.mission.destination);

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
    let delay = (2000. * settings.combat_speed.recip()) as u64;

    let size = UNIT_SIZE * projection.scale;
    let spacing = size * 1.2;

    let spawn_row = |commands: &mut Commands,
                     units: Vec<(Unit, usize)>,
                     side: Side,
                     y_start: f32,
                     y_end: f32,
                     color: Color| {
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
                    Pickable::IGNORE,
                    CombatUnitCmp {
                        unit: u.clone(),
                        side: side.clone(),
                        fire: FireState::Idle,
                        shield: c * u.shield(),
                        max_shield: c * u.shield(),
                        hull: c * u.hull(),
                        max_hull: c * u.hull(),
                    },
                    CombatCmp,
                    children![
                        (
                            Sprite {
                                color: Color::BLACK.with_alpha(0.5),
                                custom_size: Some(Vec2::new(w, h)),
                                ..default()
                            },
                            Transform::from_xyz(-size * 0.5 + w * 0.5, -size * 0.5 + h * 0.5, 0.1),
                            children![(
                                Text2d::new(c.to_string()),
                                TextFont {
                                    font: assets.font("bold"),
                                    font_size: 600. * projection.scale,
                                    ..default()
                                },
                                TextColor(WHITE.into()),
                                Transform::from_scale(Vec3::splat(0.05)),
                                CountCmp,
                            )]
                        ),
                        (
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
                        ),
                        (
                            Sprite {
                                color: BG2_COLOR,
                                custom_size: Some(Vec2::new(size, size * 0.14)),
                                ..default()
                            },
                            Transform::from_xyz(0., -size * 0.69, 0.1),
                            children![(
                                Sprite {
                                    color,
                                    custom_size: Some(Vec2::new(size * 0.96, size * 0.14 * 0.75)),
                                    ..default()
                                },
                                Transform::from_xyz(0., 0., 0.2),
                                HullCmp,
                            )],
                        )
                    ],
                ))
                .move_to(
                    Vec3::new(pos.x + x, y_end, COMBAT_SHIP_Z),
                    Duration::from_millis(delay),
                    EaseFunction::QuadraticInOut,
                );
        }
    };

    let (attack_c, defend_c) = if report.mission.owner == player.id {
        (OWN_COLOR, ENEMY_COLOR)
    } else {
        (ENEMY_COLOR, OWN_COLOR)
    };

    let attacking = Unit::all()
        .into_iter()
        .flatten()
        .filter_map(|u| {
            let amount = report.mission.army.amount(&u);
            (u != Unit::colony_ship() && amount > 0).then_some((u, amount))
        })
        .collect::<Vec<_>>();

    spawn_row(
        &mut commands,
        attacking,
        Side::Attacker,
        pos.y + height * 0.8,
        pos.y + height * 0.4,
        attack_c,
    );

    let defending_def = Unit::defenses()
        .into_iter()
        .filter_map(|u| {
            let amount = report.planet.army.amount(&u);
            (!u.is_missile() && u != Unit::space_dock() && amount > 0).then_some((u, amount))
        })
        .collect::<Vec<_>>();

    let defending_ships = Unit::ships()
        .into_iter()
        .chain(vec![Unit::space_dock()])
        .filter_map(|u| {
            let amount = report.planet.army.amount(&u);
            (u != Unit::colony_ship() && amount > 0).then_some((u, amount))
        })
        .collect::<Vec<_>>();

    let ship_y = if defending_def.len() > 0 {
        0.1
    } else {
        0.36
    };

    spawn_row(
        &mut commands,
        defending_def,
        Side::Defender,
        pos.y - height * 0.7,
        pos.y - height * 0.36,
        defend_c,
    );
    spawn_row(
        &mut commands,
        defending_ships,
        Side::Defender,
        pos.y - height * 0.7,
        pos.y - height * ship_y,
        defend_c,
    );

    // Spawn Planetary Shield image
    let ps = report.planet.army.amount(&Unit::planetary_shield());
    if ps > 0 {
        let (bar_width, bar_height) = (size * PS_WIDTH, size * 0.3);
        let w = size * 0.3;

        commands
            .spawn((
                Sprite {
                    color: BG2_COLOR,
                    custom_size: Some(Vec2::new(bar_width, bar_height)),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y - height * 0.7, COMBAT_SHIP_Z),
                PSCombatCmp {
                    shield: ps * PS_SHIELD_PER_LEVEL,
                    max_shield: ps * PS_SHIELD_PER_LEVEL,
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
                        Transform::from_xyz(
                            (-bar_width + size) * 0.5,
                            (-bar_height - size) * 0.5,
                            0.,
                        ),
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
                                    font: assets.font("bold"),
                                    font_size: 600. * projection.scale,
                                    ..default()
                                },
                                TextColor(WHITE.into()),
                                Transform::from_scale(Vec3::splat(0.05)),
                            )]
                        ),],
                    )
                ],
                Pickable::IGNORE,
                CombatCmp,
            ))
            .move_to(
                Vec3::new(pos.x, pos.y - height * 0.25, COMBAT_SHIP_Z),
                Duration::from_millis(delay),
                EaseFunction::QuadraticInOut,
            );
    }

    commands.spawn((
        Node {
            bottom: Val::Px(10.),
            left: Val::Px(10.),
            position_type: PositionType::Absolute,
            ..default()
        },
        add_text(format!("{}x", settings.combat_speed), "medium", 10., &assets, &window),
        SpeedCmp,
        CombatCmp,
    ));

    spawn_main_button(&mut commands, "Exit combat", &assets)
        .insert((ZIndex(6), CombatCmp))
        .observe(|_: On<Pointer<Click>>, mut next_game_state: ResMut<NextState<GameState>>| {
            next_game_state.set(GameState::CombatMenu);
        });
}

pub fn animate_combat(
    mut commands: Commands,
    mut speed_q: Single<&mut Text, With<SpeedCmp>>,
    round_q: Option<Single<Entity, With<DisplayRoundCmp>>>,
    mut unit_q: Query<(Entity, &Transform, &mut CombatUnitCmp)>,
    settings: Res<Settings>,
    mut state: ResMut<UiState>,
    player: Res<Player>,
    combat_state: Res<State<CombatState>>,
    mut next_combat_state: ResMut<NextState<CombatState>>,
    mut spawn_shot_msg: MessageWriter<SpawnShotMsg>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut anim_completed_msg: MessageReader<AnimCompletedEvent>,
    camera: Single<(&Transform, &Projection), With<MainCamera>>,
    window: Single<&Window>,
    assets: Local<WorldAssets>,
) {
    let (camera_t, projection) = camera.into_inner();

    let pos = camera_t.translation;
    let Projection::Orthographic(projection) = projection else {
        panic!("Expected Orthographic projection.");
    };

    // Update speed indicator
    speed_q.as_mut().0 = format!("{}x", settings.combat_speed);

    // Units in order of firing
    let units: Vec<_> = Unit::defenses()
        .into_iter()
        .filter(|u| *u != Unit::space_dock())
        .chain(Unit::ships())
        .chain(vec![Unit::space_dock()])
        .collect();

    let report = player.reports.iter().find(|r| r.id == state.in_combat.unwrap()).unwrap();
    let combat = report.combat_report.as_ref().unwrap();

    let size = UNIT_SIZE * projection.scale;

    let explosion = assets.texture("explosion");

    if *combat_state.get() == CombatState::Fire
        && unit_q.iter().all(|(_, _, cu)| matches!(cu.fire, FireState::Idle | FireState::Fired))
    {
        // Select the next unit that should fire
        for side in Side::iter() {
            for unit in &units {
                if let Some((_, _, mut cu)) = unit_q.iter_mut().find(|(_, _, cu)| {
                    cu.fire == FireState::Idle
                        && cu.unit == *unit
                        && cu.side == side
                        && cu.unit.damage() > 0
                }) {
                    cu.fire = FireState::Select;
                    return;
                }
            }
        }

        // No more units to fire -> resolve end round
        for (unit_e, unit_t, cu) in &unit_q {
            if cu.hull == 0 {
                // Spawn destruction explosion
                commands.spawn((
                    Sprite {
                        image: explosion.image.clone(),
                        texture_atlas: Some(explosion.atlas.clone()),
                        custom_size: Some(Vec2::splat(1.5 * size)),
                        ..default()
                    },
                    Transform::from_xyz(
                        unit_t.translation.x,
                        unit_t.translation.y,
                        COMBAT_EXPLOSION_Z,
                    ),
                    UnitExplosionCmp {
                        timer: Timer::from_seconds(0.05, TimerMode::Repeating),
                        delay: Timer::from_seconds(0., TimerMode::Once),
                        last_index: explosion.last_index,
                        target_entity: unit_e.clone(),
                    },
                    CombatCmp,
                ));

                play_audio_msg.write(PlayAudioMsg::new("explosion"));
            } else if state.combat_round == 0
                && cu.unit == Unit::probe()
                && cu.side == Side::Attacker
            {
                // Scout probes fly away
                commands.entity(unit_e).move_to(
                    Vec3::new(pos.x, pos.y + projection.area.height() * 0.9, COMBAT_SHIP_Z + 0.9),
                    Duration::from_millis((2000. * settings.combat_speed.recip()) as u64),
                    EaseFunction::QuadraticIn,
                );
            }
        }

        state.combat_round += 1;

        next_combat_state.set(if state.combat_round > combat.rounds.len() {
            CombatState::EndCombat
        } else {
            CombatState::DisplayRound
        });
        return;
    }

    let round = combat.rounds.get(state.combat_round).unwrap();

    match combat_state.get() {
        CombatState::Setup => {
            if !anim_completed_msg.is_empty() {
                anim_completed_msg.clear();
                next_combat_state.set(CombatState::DisplayRound);
            }
        },
        CombatState::DisplayRound => {
            if let Some(round_q) = round_q {
                let entity = round_q.into_inner();
                for message in anim_completed_msg.read() {
                    if entity == message.anim_entity {
                        next_combat_state.set(CombatState::Fire);
                        commands.entity(message.anim_entity).despawn();
                    }
                }
            } else {
                // Reset all stats
                unit_q.iter_mut().for_each(|(_, _, mut cu)| {
                    let count =
                        round.units(&cu.side).iter().filter(|cu2| cu.unit == cu2.unit).count();

                    cu.max_shield = count * cu.unit.shield();
                    cu.max_hull = count * cu.unit.hull();
                    cu.shield = cu.max_shield;
                    cu.fire = FireState::Idle;
                });

                commands.spawn((
                    add_root_node(false),
                    children![(
                        add_text(
                            format!("Round {}", state.combat_round + 1),
                            "bold",
                            40.,
                            &assets,
                            &window
                        ),
                        UiTransform {
                            translation: Val2::new(Val::ZERO, Val::Percent(-120.)),
                            scale: Vec2::ZERO,
                            ..default()
                        },
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::QuadraticInOut,
                                Duration::from_millis(
                                    (1500. * settings.combat_speed.recip()) as u64
                                ),
                                UiTransformScaleLens {
                                    start: Vec2::ZERO,
                                    end: Vec2::ONE,
                                },
                            )
                            .with_repeat_count(RepeatCount::Finite(2))
                            .with_repeat_strategy(RepeatStrategy::MirroredRepeat)
                        ),
                        DisplayRoundCmp,
                    )],
                    CombatCmp,
                ));
            }
        },
        CombatState::Fire => {
            for (unit_e, unit_t, mut cu) in &mut unit_q {
                match cu.fire {
                    FireState::Select => {
                        commands.entity(unit_e).insert(TweenAnim::new(Tween::new(
                            EaseFunction::QuadraticInOut,
                            Duration::from_millis((500. * settings.combat_speed.recip()) as u64),
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
                    FireState::Firing => {
                        let shots = round
                            .units(&cu.side)
                            .iter()
                            .filter(|cu2| cu.unit == cu2.unit)
                            .flat_map(|cu2| &cu2.shots)
                            .collect::<Vec<_>>();

                        for shot in shots {
                            spawn_shot_msg.write(SpawnShotMsg {
                                shot: shot.clone(),
                                side: cu.side.opposite(),
                            });
                        }

                        cu.fire = FireState::Deselect;
                    },
                    FireState::Deselect => {
                        commands.entity(unit_e).insert(TweenAnim::new(Tween::new(
                            EaseFunction::QuarticIn,
                            Duration::from_millis((1500. * settings.combat_speed.recip()) as u64),
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
        _ => (),
    }
}

pub fn update_combat_stats(
    unit_q: Query<(Entity, &CombatUnitCmp)>,
    ps_q: Query<(Entity, &PSCombatCmp)>,
    mut count_q: Query<&mut Text2d, With<CountCmp>>,
    mut shield_q: Query<(&mut Transform, &mut Sprite), With<ShieldCmp>>,
    mut hull_q: Query<(&mut Transform, &mut Sprite), (With<HullCmp>, Without<ShieldCmp>)>,
    children_q: Query<&Children>,
    state: Res<UiState>,
    player: Res<Player>,
    camera_q: Single<&Projection, With<MainCamera>>,
) {
    let Projection::Orthographic(projection) = camera_q.into_inner() else {
        panic!("Expected Orthographic projection.");
    };

    let report = player.reports.iter().find(|r| r.id == state.in_combat.unwrap()).unwrap();
    let combat = report.combat_report.as_ref().unwrap();
    let round = combat.rounds.get(state.combat_round).unwrap();

    let size = UNIT_SIZE * projection.scale;

    for (unit_e, cu) in &unit_q {
        for child in children_q.iter_descendants(unit_e) {
            if let Ok(mut text) = count_q.get_mut(child) {
                text.0 = round
                    .units(&cu.side)
                    .iter()
                    .filter(|cu2| cu.unit == cu2.unit)
                    .count()
                    .to_string();
            }

            if let Ok((mut shield_t, mut shield_s)) = shield_q.get_mut(child) {
                if let Some(shield_size) = shield_s.custom_size.as_mut() {
                    let full_size = size * 0.96;
                    shield_size.x = full_size * cu.shield as f32 / cu.max_shield as f32;
                    shield_t.translation.x = (shield_size.x - full_size) * 0.5;
                }
            }

            if let Ok((mut hull_t, mut hull_s)) = hull_q.get_mut(child) {
                if let Some(hull_size) = hull_s.custom_size.as_mut() {
                    let full_size = size * 0.96;
                    hull_size.x = full_size * cu.hull as f32 / cu.max_hull as f32;
                    hull_t.translation.x = (hull_size.x - full_size) * 0.5;
                }
            }
        }
    }

    for (ps_e, ps) in &ps_q {
        for child in children_q.iter_descendants(ps_e) {
            if let Ok((mut shield_t, mut shield_s)) = shield_q.get_mut(child) {
                if let Some(shield_size) = shield_s.custom_size.as_mut() {
                    let full_size = size * PS_WIDTH * 0.997;
                    shield_size.x = full_size * ps.shield as f32 / ps.max_shield as f32;
                    shield_t.translation.x = (shield_size.x - full_size) * 0.5;
                }
            }
        }
    }
}

pub fn run_combat_animations(
    mut commands: Commands,
    mut animation_q: Query<(Entity, &mut Sprite, Option<&ShotReport>, &mut UnitExplosionCmp)>,
    mut unit_q: Query<(Entity, &Transform, &mut CombatUnitCmp)>,
    mut ps_q: Query<(Entity, &mut PSCombatCmp)>,
    ps_image_q: Query<&GlobalTransform, With<PSCombatImageCmp>>,
    mut spawn_shot_msg: MessageReader<SpawnShotMsg>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    camera_q: Single<&Projection, With<MainCamera>>,
    settings: Res<Settings>,
    time: Res<Time>,
    assets: Local<WorldAssets>,
) {
    let mut rng = rng();

    let Projection::Orthographic(projection) = camera_q.into_inner() else {
        panic!("Expected Orthographic projection.");
    };

    let size = UNIT_SIZE * projection.scale;

    let short_explosion = assets.texture("short explosion");

    // Spawn shot explosions
    for message in spawn_shot_msg.read() {
        let target = if message.shot.unit == Some(Unit::planetary_shield()) {
            ps_q.iter().zip(ps_image_q.iter()).next().map(|((e, _), t)| (e, t.compute_transform()))
        } else {
            unit_q
                .iter()
                .find(|(_, _, cu)| message.shot.unit == Some(cu.unit) && cu.side == message.side)
                .map(|(e, t, _)| (e, *t))
        };

        if let Some((target_e, target_t)) = target {
            let id = if !message.shot.missed {
                play_audio_msg.write(PlayAudioMsg::new("short explosion"));
                commands
                    .spawn((
                        Sprite {
                            image: short_explosion.image.clone(),
                            texture_atlas: Some(short_explosion.atlas.clone()),
                            custom_size: Some(Vec2::splat(0.7 * size)),
                            ..default()
                        },
                        UnitExplosionCmp {
                            timer: Timer::from_seconds(0.035, TimerMode::Repeating),
                            delay: Timer::from_seconds(rng.random_range(0.0..0.5), TimerMode::Once),
                            last_index: short_explosion.last_index,
                            target_entity: target_e,
                        },
                        message.shot.clone(),
                        CombatCmp,
                    ))
                    .id()
            } else {
                commands
                    .spawn((
                        Text2d::new("Miss"),
                        TextFont {
                            font: assets.font("bold"),
                            font_size: 15.,
                            ..default()
                        },
                        TextColor(WHITE.into()),
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::QuadraticOut,
                                Duration::from_millis((750. * settings.combat_speed.recip()) as u64),
                                TransformScaleLens {
                                    start: Vec3::splat(0.),
                                    end: Vec3::splat(1.0),
                                },
                            )
                            .with_repeat_count(RepeatCount::Finite(2))
                            .with_repeat_strategy(RepeatStrategy::MirroredRepeat)
                        ),
                        CombatCmp,
                    ))
                    .id()
            };

            commands.entity(id).insert(Transform::from_xyz(
                rng.random_range(
                    target_t.translation.x - size * 0.4..target_t.translation.x + size * 0.4,
                ),
                rng.random_range(
                    target_t.translation.y - size * 0.4..target_t.translation.y + size * 0.4,
                ),
                COMBAT_EXPLOSION_Z,
            ));
        }
    }

    // Resolve explosions
    for (animation_e, mut sprite, shot, mut animation) in &mut animation_q {
        if !animation.delay.is_finished() {
            animation.delay.tick(scale_duration(time.delta(), settings.combat_speed));
            continue;
        }

        animation.timer.tick(scale_duration(time.delta(), settings.combat_speed));

        if animation.timer.just_finished() {
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index += 1;

                // Resolve damage at 1/5 of the animation
                if let Some(shot) = shot {
                    if atlas.index == animation.last_index / 5 {
                        if let Ok((_, _, mut cu)) = unit_q.get_mut(animation.target_entity) {
                            cu.shield -= shot.shield_damage;
                            cu.hull -= shot.hull_damage;
                        } else if let Ok((ps_e, mut ps)) = ps_q.get_mut(animation.target_entity) {
                            ps.shield -= shot.planetary_shield_damage;

                            if ps.shield == 0 {
                                let explosion = assets.texture("explosion");

                                let ps_t = ps_image_q.iter().next().unwrap().compute_transform();

                                commands.spawn((
                                    Sprite {
                                        image: explosion.image.clone(),
                                        texture_atlas: Some(explosion.atlas.clone()),
                                        custom_size: Some(Vec2::splat(1.5 * size)),
                                        ..default()
                                    },
                                    Transform::from_xyz(
                                        ps_t.translation.x,
                                        ps_t.translation.y,
                                        COMBAT_EXPLOSION_Z,
                                    ),
                                    UnitExplosionCmp {
                                        timer: Timer::from_seconds(0.05, TimerMode::Repeating),
                                        delay: Timer::from_seconds(0., TimerMode::Once),
                                        last_index: explosion.last_index,
                                        target_entity: ps_e.clone(),
                                    },
                                    CombatCmp,
                                ));

                                play_audio_msg.write(PlayAudioMsg::new("explosion"));
                            }
                        }
                    }
                } else if atlas.index == 3 * animation.last_index / 5 {
                    // Despawn the target entity at 3/5 of the animation
                    commands.entity(animation.target_entity).despawn();
                }

                if atlas.index == animation.last_index {
                    commands.entity(animation_e).try_despawn();
                }
            }
        }
    }
}

pub fn exit_combat(
    mut state: ResMut<UiState>,
    mut next_combat_state: ResMut<NextState<CombatState>>,
    mut stop_audio_msg: MessageWriter<StopAudioMsg>,
) {
    state.combat_round = 0;
    stop_audio_msg.write(StopAudioMsg::new("horn"));
    next_combat_state.set(CombatState::default());
}
