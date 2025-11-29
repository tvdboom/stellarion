use std::time::Duration;

use bevy::color::palettes::basic::WHITE;
use bevy::prelude::*;
use bevy_tweening::EntityCommandsTweeningExtensions;

use crate::core::assets::WorldAssets;
use crate::core::audio::{PauseAudioMsg, PlayAudioMsg, StopAudioMsg};
use crate::core::camera::MainCamera;
use crate::core::constants::{
    COMBAT_BACKGROUND_Z, COMBAT_SHIP_Z, ENEMY_COLOR, OWN_COLOR, SHIELD_COLOR,
};
use crate::core::map::map::Map;
use crate::core::map::utils::spawn_main_button;
use crate::core::menu::utils::add_root_node;
use crate::core::player::Player;
use crate::core::states::GameState;
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
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    camera: Single<(&Transform, &Projection), With<MainCamera>>,
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
    let size = width / 15.;
    let spacing = size * 1.1;

    let spawn_row = |commands: &mut Commands,
                     units: Vec<(Unit, usize)>,
                     y_start: f32,
                     y_end: f32,
                     color: Color| {
        let total = units.len() as f32;
        if total == 0.0 {
            return;
        }

        let total_width = spacing * (total - 1.0);
        for (i, (u, c)) in units.iter().enumerate() {
            let x = -total_width * 0.5 + i as f32 * spacing;

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
                    CombatCmp,
                    children![
                        (
                            Text2d::new(c.to_string()),
                            TextFont {
                                font: assets.font("bold"),
                                font_size: 30. * projection.scale,
                                ..default()
                            },
                            TextColor(WHITE.into()),
                            Transform::from_xyz(-size * 0.3, -size * 0.3, 0.1),
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
                    Duration::from_secs(2),
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

    spawn_row(&mut commands, attacking, pos.y + height * 0.8, pos.y + height * 0.4, attack_c);

    let defending_def = Unit::defenses()
        .into_iter()
        .filter_map(|u| {
            let amount = report.planet.army.amount(&u);
            (!u.is_missile() && amount > 0).then_some((u, amount))
        })
        .collect::<Vec<_>>();

    let defending_ships = Unit::ships()
        .into_iter()
        .filter_map(|u| {
            let amount = report.planet.army.amount(&u);
            (u != Unit::colony_ship() && amount > 0).then_some((u, amount))
        })
        .collect::<Vec<_>>();

    let ship_y = if defending_def.len() > 0 {
        0.18
    } else {
        0.36
    };

    spawn_row(&mut commands, defending_def, pos.y - height * 0.7, pos.y - height * 0.36, defend_c);
    spawn_row(
        &mut commands,
        defending_ships,
        pos.y - height * 0.7,
        pos.y - height * ship_y,
        defend_c,
    );

    spawn_main_button(&mut commands, "Exit combat", &assets)
        .insert((ZIndex(6), CombatCmp))
        .observe(|_: On<Pointer<Click>>, mut next_game_state: ResMut<NextState<GameState>>| {
            next_game_state.set(GameState::CombatMenu);
        });
}

pub fn exit_combat(mut stop_audio_msg: MessageWriter<StopAudioMsg>) {
    stop_audio_msg.write(StopAudioMsg::new("horn"));
}
