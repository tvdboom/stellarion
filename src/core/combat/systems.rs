use std::time::Duration;

use bevy::color::palettes::css::WHITE;
use bevy::prelude::*;
use bevy_tweening::lens::{TransformPositionLens, TransformScaleLens};
use bevy_tweening::{
    AnimCompletedEvent, CycleCompletedEvent, Delay, PlaybackState, RepeatCount, RepeatStrategy,
    Tween, TweenAnim,
};
use rand::{rng, Rng};
use strum::IntoEnumIterator;

use crate::core::assets::WorldAssets;
use crate::core::audio::{MuteAudioMsg, PauseAudioMsg, PlayAudioMsg, StopAudioMsg};
use crate::core::camera::MainCamera;
use crate::core::combat::combat::ShotReport;
use crate::core::combat::report::Side;
use crate::core::constants::{
    BG2_COLOR, COMBAT_BACKGROUND_Z, COMBAT_EXPLOSION_Z, COMBAT_SHIP_Z, ENEMY_COLOR, OWN_COLOR,
    PS_SHIELD_PER_LEVEL, PS_WIDTH, SETUP_TIME, SHIELD_COLOR, UNIT_SIZE,
};
use crate::core::map::icon::Icon;
use crate::core::map::map::Map;
use crate::core::map::utils::{
    spawn_main_button, SpriteAlphaLens, SpriteFrameLens, UiTransformScaleLens,
};
use crate::core::menu::utils::{add_root_node, add_text};
use crate::core::missions::BombingRaid;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{CombatState, GameState};
use crate::core::turns::StartTurnMsg;
use crate::core::ui::systems::UiState;
use crate::core::units::ships::Ship;
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
pub struct DisplayTextCmp;

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

impl FireState {
    pub fn has_fired(&self) -> bool {
        matches!(
            self,
            FireState::Firing | FireState::Deselect | FireState::AfterFire | FireState::Fired
        )
    }
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
pub struct PSCombatImageCmp;

#[derive(Component)]
pub struct CountCmp;

#[derive(Component)]
pub struct HullCmp;

#[derive(Component)]
pub struct ShieldCmp;

#[derive(Component)]
pub struct DeathRayCmp;

#[derive(Message)]
pub struct SpawnShotMsg {
    shot: ShotReport,
    repair: bool,
    side: Side,
}

#[derive(Component)]
pub struct RepairCmp {
    pub unit: Unit,
    pub amount: usize,
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
        .observe(|_: On<Pointer<Click>>, mut next_game_state: ResMut<NextState<GameState>>| {
            next_game_state.set(GameState::Playing);
        });
}

pub fn exit_combat_menu(
    mut start_turn_msg: MessageWriter<StartTurnMsg>,
    mut stop_audio_msg: MessageWriter<StopAudioMsg>,
) {
    start_turn_msg.write(StartTurnMsg::new(true, false));
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

            commands.spawn((
                Sprite {
                    image: assets.image(u.to_lowername()),
                    custom_size: Some(Vec2::splat(size)),
                    ..default()
                },
                Transform::from_xyz(pos.x, y_start, COMBAT_SHIP_Z),
                CombatUnitCmp {
                    unit: u.clone(),
                    side: side.clone(),
                    fire: FireState::Idle,
                    shield: c * u.shield(),
                    max_shield: c * u.shield(),
                    hull: c * u.hull(),
                    max_hull: c * u.hull(),
                },
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
                    ),
                ],
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
            ));
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

    let ship_y = if defending_def.is_empty() && !draw_ps {
        0.36
    } else {
        0.1
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
    if draw_ps {
        let (bar_width, bar_height) = (size * PS_WIDTH, size * 0.3);
        let w = size * 0.3;

        commands.spawn((
            Sprite {
                color: BG2_COLOR,
                custom_size: Some(Vec2::new(bar_width, bar_height)),
                ..default()
            },
            Transform::from_xyz(pos.x, pos.y - height * 0.7, COMBAT_SHIP_Z),
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
                                font: assets.font("bold"),
                                font_size: 600. * projection.scale,
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
                CombatUnitCmp {
                    unit: u.clone(),
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
                            font: assets.font("bold"),
                            font_size: 600. * projection.scale,
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
    assets: Local<WorldAssets>,
) {
    let (camera_t, projection) = camera.into_inner();

    let pos = camera_t.translation;
    let Projection::Orthographic(projection) = projection else {
        panic!("Expected Orthographic projection.");
    };

    let units: Vec<_> = Unit::all_firing_order();

    let report = player.reports.iter().find(|r| r.id == state.in_combat.unwrap()).unwrap();
    let combat = report.combat_report.as_ref().unwrap();
    let round = combat.rounds.get(state.combat_round).unwrap();

    let size = UNIT_SIZE * projection.scale;

    let explosion = assets.texture("explosion");

    if matches!(
        combat_state.get(),
        CombatState::AntiBallistic
            | CombatState::Fire
            | CombatState::Repair
            | CombatState::Bomb
            | CombatState::DeathRay
    ) && unit_q.iter().all(|(_, _, cu)| matches!(cu.fire, FireState::Idle | FireState::Fired))
    {
        'side: for side in Side::iter() {
            // If all enemy units are destroyed, end combat prematurely
            // (to avoid all misses of the remainder of units)
            if unit_q
                .iter()
                .filter(|(_, _, cu)| cu.side == side.opposite() && !cu.unit.is_building())
                .all(|(_, _, cu)| cu.hull == 0)
            {
                break 'side;
            }

            // Select the next unit that should fire
            for unit in &units {
                if let Some((_, _, mut cu)) = unit_q.iter_mut().find(|(_, _, cu)| {
                    cu.fire == FireState::Idle
                        && cu.unit == *unit
                        && cu.side == side
                        && (cu.unit.damage() > 0 || cu.unit == Unit::crawler())
                        && (cu.unit != Unit::interplanetary_missile()
                            || round.missiles_shot() < round.n_missiles())
                }) {
                    cu.fire = FireState::Select;
                    return;
                }
            }
        }

        // No more units to fire -> explode destroyed units
        for (unit_e, unit_t, cu) in &mut unit_q {
            if cu.hull == 0
                && cu.unit != Unit::planetary_shield()
                && (cu.unit != Unit::antiballistic_missile()
                    || round.antiballistic_fired >= round.n_antiballistic())
            {
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
                        target_entity: unit_e,
                    },
                    CombatCmp,
                ));

                play_audio_msg.write(PlayAudioMsg::new("explosion"));
            }
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

        // Bombing raid
        if report.mission.bombing != BombingRaid::None
            && round
                .units(&Side::Attacker)
                .iter()
                .any(|cu| cu.shots.iter().any(|s| s.unit.is_some_and(|u| u.is_building())))
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

                commands.spawn((
                    add_root_node(false),
                    children![(
                        Text::new(format!("Round {}", state.combat_round + 1)),
                        TextFont {
                            font: assets.font("bold"),
                            font_size: 80.,
                            ..default()
                        },
                        UiTransform {
                            translation: Val2::new(Val::ZERO, Val::Percent(-120.)),
                            scale: Vec2::ZERO,
                            ..default()
                        },
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::QuadraticInOut,
                                Duration::from_millis(1500),
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
                            });
                        }

                        cu.fire = FireState::Deselect;
                    },
                    FireState::Firing if *combat_state.get() == CombatState::DeathRay => {
                        if let Some(ray_e) = death_ray_q.iter().next() {
                            for message in anim_completed_msg.read() {
                                if ray_e == message.anim_entity {
                                    if report.planet_destroyed
                                        && state.combat_round == combat.rounds.len() - 1
                                    {
                                        let mut rng = rng();

                                        for _ in 0..100 {
                                            play_audio_msg
                                                .write(PlayAudioMsg::new("large explosion"));

                                            commands.spawn((
                                                Sprite {
                                                    image: explosion.image.clone(),
                                                    texture_atlas: Some(explosion.atlas.clone()),
                                                    custom_size: Some(Vec2::splat(5.0 * size)),
                                                    color: Color::WHITE.with_alpha(0.0001),
                                                    ..default()
                                                },
                                                Transform::from_xyz(
                                                    pos.x + rng.random_range(-6. * size..6. * size),
                                                    pos.y - rng.random_range(-6. * size..6. * size),
                                                    COMBAT_EXPLOSION_Z,
                                                ),
                                                TweenAnim::new(
                                                    Delay::new(Duration::from_millis(
                                                        rng.random_range(0..3000),
                                                    ))
                                                    .then(Tween::new(
                                                        EaseFunction::Linear,
                                                        Duration::from_millis(1),
                                                        SpriteAlphaLens {
                                                            start: 0.0,
                                                            end: 1.0,
                                                        },
                                                    ))
                                                    .then(Tween::new(
                                                        EaseFunction::Linear,
                                                        Duration::from_secs(4),
                                                        SpriteFrameLens(explosion.last_index),
                                                    )),
                                                ),
                                                DeathRayCmp,
                                                CombatCmp,
                                            ));
                                        }
                                    }

                                    commands.entity(ray_e).despawn();
                                    cu.fire = FireState::Deselect;
                                }
                            }
                        } else {
                            play_audio_msg.write(PlayAudioMsg::new("death ray"));

                            let texture = assets.texture("death ray");
                            commands.entity(unit_e).with_child((
                                Sprite::from_atlas_image(texture.image, texture.atlas),
                                Transform {
                                    translation: Vec3::new(0., -150., -0.1),
                                    scale: Vec3::splat(0.4),
                                    ..default()
                                },
                                TweenAnim::new(
                                    Tween::new(
                                        EaseFunction::Linear,
                                        Duration::from_millis(1000),
                                        SpriteFrameLens(texture.last_index),
                                    )
                                    .with_repeat_count(RepeatCount::For(Duration::from_secs(3))),
                                ),
                                DeathRayCmp,
                                CombatCmp,
                            ));
                        }
                    },
                    FireState::Firing => {
                        let shots = round
                            .units(&cu.side)
                            .iter()
                            .filter(|cu2| cu.unit == cu2.unit)
                            .flat_map(|cu2| &cu2.shots)
                            .filter(|s| {
                                s.unit.is_some_and(|u| {
                                    u.is_building() && u != Unit::planetary_shield()
                                }) == (*combat_state.get() == CombatState::Bomb)
                            })
                            .collect::<Vec<_>>();

                        for shot in shots {
                            spawn_shot_msg.write(SpawnShotMsg {
                                shot: shot.clone(),
                                repair: false,
                                side: cu.side.opposite(),
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
                                    "victory" => 0.7,
                                    "draw" => 0.4,
                                    "defeat" => 0.6,
                                    _ => unreachable!(),
                                }),
                            },
                        )),
                        DisplayTextCmp,
                        CombatCmp,
                    )],
                    CombatCmp,
                ));
            }

            let mut bg = bg_q.into_inner();
            for message in anim_completed_msg.read() {
                for ray_e in death_ray_q.iter() {
                    if ray_e == message.anim_entity {
                        bg.image = assets.image("destroyed bg");

                        commands.entity(ray_e).despawn();

                        for (unit_e, _, cu) in &unit_q {
                            if cu.side == Side::Defender {
                                commands.entity(unit_e).despawn();
                            }
                        }
                    }
                }
            }
        },
    }
}

pub fn update_combat_stats(
    unit_q: Query<(Entity, &CombatUnitCmp)>,
    mut anim_q: Query<&mut TweenAnim, With<CombatCmp>>,
    mut count_q: Query<&mut Text2d, With<CountCmp>>,
    mut shield_q: Query<(&mut Transform, &mut Sprite), With<ShieldCmp>>,
    mut hull_q: Query<(&mut Transform, &mut Sprite), (With<HullCmp>, Without<ShieldCmp>)>,
    mut speed_q: Single<&mut Text, With<SpeedCmp>>,
    children_q: Query<&Children>,
    settings: Res<Settings>,
    state: Res<UiState>,
    player: Res<Player>,
    combat_state: Res<State<CombatState>>,
    camera_q: Single<&Projection, With<MainCamera>>,
    time: Res<Time>,
) {
    let Projection::Orthographic(projection) = camera_q.into_inner() else {
        panic!("Expected Orthographic projection.");
    };

    // Update speed indicator
    speed_q.as_mut().0 = if settings.combat_paused {
        anim_q.iter_mut().for_each(|mut t| t.playback_state = PlaybackState::Paused);
        "Paused".to_string()
    } else {
        anim_q.iter_mut().for_each(|mut t| {
            t.playback_state = PlaybackState::Playing;
            t.speed = settings.combat_speed as f64;
        });
        format!("{}x", settings.combat_speed)
    };

    let report = player.reports.iter().find(|r| r.id == state.in_combat.unwrap()).unwrap();
    let combat = report.combat_report.as_ref().unwrap();
    let round = combat.rounds.get(state.combat_round).unwrap();

    let size = UNIT_SIZE * projection.scale;
    let speed = 3. * time.delta_secs() * settings.speed();

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
                        .lerp(full_size * cu.shield as f32 / cu.max_shield as f32, speed);
                    shield_t.translation.x = (shield_size.x - full_size) * 0.5;
                }
            }

            if let Ok((mut hull_t, mut hull_s)) = hull_q.get_mut(child) {
                if let Some(hull_size) = hull_s.custom_size.as_mut() {
                    let full_size = size * 0.96;
                    hull_size.x =
                        hull_size.x.lerp(full_size * cu.hull as f32 / cu.max_hull as f32, speed);
                    hull_t.translation.x = (hull_size.x - full_size) * 0.5;
                }
            }
        }
    }
}

pub fn run_combat_animations(
    mut commands: Commands,
    mut animation_q: Query<(Entity, &mut Sprite, Option<&ShotReport>, &mut UnitExplosionCmp)>,
    mut unit_q: Query<(Entity, &Sprite, &Transform, &mut CombatUnitCmp), Without<UnitExplosionCmp>>,
    mut repair_q: Query<(Entity, &RepairCmp)>,
    ps_image_q: Query<
        (&Sprite, &GlobalTransform),
        (With<PSCombatImageCmp>, Without<UnitExplosionCmp>),
    >,
    mut cycle_completed_msg: MessageReader<CycleCompletedEvent>,
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

    let short_explosion = assets.texture("short explosion");

    // Spawn shot/repair explosions
    for message in spawn_shot_msg.read() {
        let target = unit_q
            .iter()
            .find(|(_, _, _, cu)| message.shot.unit == Some(cu.unit) && cu.side == message.side)
            .map(|(e, s, t, cu)| {
                if cu.unit == Unit::planetary_shield() {
                    let (s, t) = ps_image_q
                        .iter()
                        .next()
                        .map(|(s, t)| (s.custom_size.unwrap().x, t.compute_transform()))
                        .unwrap();

                    (e, s, t)
                } else {
                    (e, s.custom_size.unwrap().x * projection.scale, *t)
                }
            });

        if let Some((target_e, size, target_t)) = target {
            let id = if message.repair {
                play_audio_msg.write(PlayAudioMsg::new("repair"));
                commands
                    .spawn((
                        Sprite {
                            image: assets.image("repair"),
                            custom_size: Some(Vec2::splat(0.5 * size)),
                            ..default()
                        },
                        Transform::from_scale(Vec3::splat(0.)),
                        TweenAnim::new(
                            Delay::new(Duration::from_millis(rng.random_range(1..500))).then(
                                Tween::new(
                                    EaseFunction::QuadraticOut,
                                    Duration::from_millis(750),
                                    TransformScaleLens {
                                        start: Vec3::splat(0.),
                                        end: Vec3::splat(1.0),
                                    },
                                )
                                .with_repeat_count(RepeatCount::Finite(2))
                                .with_repeat_strategy(RepeatStrategy::MirroredRepeat)
                                .with_cycle_completed_event(true),
                            ),
                        ),
                        RepairCmp {
                            unit: message.shot.unit.unwrap(),
                            amount: message.shot.hull_damage,
                        },
                    ))
                    .id()
            } else if message.shot.missed {
                commands
                    .spawn((
                        Text2d::new("Miss"),
                        TextFont {
                            font: assets.font("bold"),
                            font_size: 15.,
                            ..default()
                        },
                        TextColor(WHITE.into()),
                        Transform::from_scale(Vec3::splat(0.)),
                        TweenAnim::new(
                            Delay::new(Duration::from_millis(rng.random_range(1..500))).then(
                                Tween::new(
                                    EaseFunction::QuadraticOut,
                                    Duration::from_millis(750),
                                    TransformScaleLens {
                                        start: Vec3::splat(0.),
                                        end: Vec3::splat(1.0),
                                    },
                                )
                                .with_repeat_count(RepeatCount::Finite(2))
                                .with_repeat_strategy(RepeatStrategy::MirroredRepeat),
                            ),
                        ),
                    ))
                    .id()
            } else {
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
                    ))
                    .id()
            };

            commands.entity(id).insert((
                Transform {
                    translation: Vec3::new(
                        rng.random_range(
                            target_t.translation.x - size * 0.4
                                ..target_t.translation.x + size * 0.4,
                        ),
                        rng.random_range(
                            target_t.translation.y - size * 0.4
                                ..target_t.translation.y + size * 0.4,
                        ),
                        COMBAT_EXPLOSION_Z,
                    ),
                    scale: Vec3::splat(if message.repair {
                        0.
                    } else {
                        1.0
                    }),
                    ..default()
                },
                CombatCmp,
            ));
        }
    }

    // Resolve repairs
    for message in cycle_completed_msg.read() {
        if let Ok((repair_e, repair)) = repair_q.get_mut(message.anim_entity) {
            if let Some((_, _, _, mut cu)) =
                unit_q.iter_mut().find(|(_, _, _, cu)| cu.unit == repair.unit)
            {
                cu.hull += repair.amount;
            }

            // Remove component to not trigger again when return cycle finishes
            commands.entity(repair_e).remove::<RepairCmp>();
        }
    }

    // Resolve explosions
    for (animation_e, mut sprite, shot, mut animation) in &mut animation_q {
        if !animation.delay.is_finished() {
            animation.delay.tick(scale_duration(time.delta(), settings.speed()));
            continue;
        }

        animation.timer.tick(scale_duration(time.delta(), settings.speed()));

        if animation.timer.just_finished() {
            if let Some(atlas) = &mut sprite.texture_atlas {
                atlas.index += 1;

                // Resolve damage at 1/5 of the animation
                if let Some(shot) = shot {
                    if atlas.index == animation.last_index / 5 {
                        if let Ok((unit_e, _, _, mut cu)) = unit_q.get_mut(animation.target_entity)
                        {
                            if cu.unit == Unit::planetary_shield() {
                                cu.shield -= shot.planetary_shield_damage;

                                if cu.shield == 0 {
                                    let explosion = assets.texture("explosion");

                                    let (size, ps_t) = ps_image_q
                                        .iter()
                                        .next()
                                        .map(|(s, t)| {
                                            (s.custom_size.unwrap().x, t.compute_transform())
                                        })
                                        .unwrap();

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
                                            target_entity: unit_e,
                                        },
                                        CombatCmp,
                                    ));

                                    play_audio_msg.write(PlayAudioMsg::new("explosion"));
                                }
                            } else if cu.unit.is_building() {
                                if shot.killed {
                                    cu.hull -= 1;
                                }
                            } else {
                                cu.shield -= shot.shield_damage;
                                cu.hull -= shot.hull_damage;
                            }
                        }
                    }
                } else if atlas.index == 3 * animation.last_index / 5 {
                    // Despawn the target entity at 3/5 of the animation
                    commands.entity(animation.target_entity).despawn();
                }

                if atlas.index == animation.last_index {
                    commands.entity(animation_e).despawn();
                }
            }
        }
    }
}

pub fn exit_combat(
    mut state: ResMut<UiState>,
    mut next_combat_state: ResMut<NextState<CombatState>>,
    mut mute_audio_msg: MessageWriter<MuteAudioMsg>,
) {
    state.combat_round = 0;
    mute_audio_msg.write(MuteAudioMsg);
    next_combat_state.set(CombatState::default());
}
