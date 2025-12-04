use std::time::Duration;

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use bevy_tweening::lens::TransformScaleLens;
use bevy_tweening::{
    AnimCompletedEvent, EntityCommandsTweeningExtensions, RepeatCount, RepeatStrategy, Tween,
    TweenAnim,
};
use strum::IntoEnumIterator;

use crate::core::assets::WorldAssets;
use crate::core::audio::{PauseAudioMsg, PlayAudioMsg, StopAudioMsg};
use crate::core::camera::MainCamera;
use crate::core::combat::report::Side;
use crate::core::constants::{
    COMBAT_BACKGROUND_Z, COMBAT_SHIP_Z, ENEMY_COLOR, OWN_COLOR, SHIELD_COLOR,
};
use crate::core::map::map::Map;
use crate::core::map::utils::{spawn_main_button, UiTransformScaleLens};
use crate::core::menu::utils::{add_root_node, add_text};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{CombatState, GameState};
use crate::core::turns::StartTurnMsg;
use crate::core::ui::systems::UiState;
use crate::core::units::{Amount, Unit};
use crate::utils::NameFromEnum;

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

#[derive(Component, PartialEq)]
pub enum FireCmp {
    Idle,
    Select,
    PreFire,
    Firing,
    Deselect,
    AfterFire,
    Fired,
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

    let size = 120. * projection.scale;
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
                    u.clone(),
                    side.clone(),
                    FireCmp::Idle,
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
                            )]
                        ),
                        (
                            Sprite {
                                color: Color::BLACK,
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
                            )],
                        ),
                        (
                            Sprite {
                                color: Color::BLACK,
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
        let (bar_width, bar_height) = (size * 11., size * 0.3);
        let w = size * 0.3;

        commands
            .spawn((
                Sprite {
                    color: Color::BLACK,
                    custom_size: Some(Vec2::new(bar_width, bar_height)),
                    ..default()
                },
                Transform::from_xyz(pos.x, pos.y - height * 0.7, COMBAT_SHIP_Z),
                children![
                    (
                        Sprite {
                            color: SHIELD_COLOR,
                            custom_size: Some(Vec2::new(bar_width * 0.997, bar_height * 0.9)),
                            ..default()
                        },
                        Transform::from_xyz(0., 0., 0.1),
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
                            COMBAT_SHIP_Z,
                        ),
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
                Unit::planetary_shield(),
                Side::Defender,
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
    mut unit_q: Query<(Entity, &Transform, &mut FireCmp, &Unit, &Side)>,
    settings: Res<Settings>,
    mut state: ResMut<UiState>,
    player: Res<Player>,
    combat_state: Res<State<CombatState>>,
    mut next_combat_state: ResMut<NextState<CombatState>>,
    mut anim_completed_msg: MessageReader<AnimCompletedEvent>,
    window: Single<&Window>,
    assets: Local<WorldAssets>,
) {
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

    if state.combat_round > combat.rounds.len() {
        next_combat_state.set(CombatState::EndCombat);
    } else if *combat_state.get() == CombatState::Fire
        && unit_q.iter().all(|(_, _, f, _, _)| matches!(f, FireCmp::Idle | FireCmp::Fired))
    {
        // Select the next unit that should fire
        for side in Side::iter() {
            for unit in &units {
                if let Some((_, _, mut f, _, _)) = unit_q
                    .iter_mut()
                    .find(|(_, _, f, u, s)| **f == FireCmp::Idle && *u == unit && **s == side)
                {
                    *f = FireCmp::Select;
                    return;
                }
            }
        }

        // No more units to fire -> next round
        state.combat_round += 1;
        next_combat_state.set(CombatState::DisplayRound);
    }

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
                // Reset all firing components
                unit_q.iter_mut().for_each(|(_, _, mut f, _, _)| {
                    *f = FireCmp::Idle;
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
            for (unit_e, unit_t, mut fire, _, _) in &mut unit_q {
                match *fire {
                    FireCmp::Select => {
                        commands.entity(unit_e).insert(TweenAnim::new(Tween::new(
                            EaseFunction::QuadraticInOut,
                            Duration::from_millis((500. * settings.combat_speed.recip()) as u64),
                            TransformScaleLens {
                                start: unit_t.scale,
                                end: unit_t.scale * 1.3,
                            },
                        )));
                        *fire = FireCmp::PreFire;
                    },
                    FireCmp::PreFire => {
                        for message in anim_completed_msg.read() {
                            if unit_e == message.anim_entity {
                                *fire = FireCmp::Firing;
                            }
                        }
                    },
                    FireCmp::Firing => {
                        *fire = FireCmp::Deselect;
                    },
                    FireCmp::Deselect => {
                        commands.entity(unit_e).insert(TweenAnim::new(Tween::new(
                            EaseFunction::QuadraticInOut,
                            Duration::from_millis((500. * settings.combat_speed.recip()) as u64),
                            TransformScaleLens {
                                start: unit_t.scale,
                                end: unit_t.scale / 1.3,
                            },
                        )));
                        *fire = FireCmp::AfterFire;
                    },
                    FireCmp::AfterFire => {
                        for message in anim_completed_msg.read() {
                            if unit_e == message.anim_entity {
                                *fire = FireCmp::Fired;
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

pub fn exit_combat(
    mut state: ResMut<UiState>,
    mut next_combat_state: ResMut<NextState<CombatState>>,
    mut stop_audio_msg: MessageWriter<StopAudioMsg>,
) {
    state.combat_round = 0;
    stop_audio_msg.write(StopAudioMsg::new("horn"));
    next_combat_state.set(CombatState::default());
}
