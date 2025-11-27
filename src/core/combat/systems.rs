use bevy::prelude::*;

use crate::core::assets::WorldAssets;
use crate::core::audio::PlayAudioMsg;
use crate::core::map::map::Map;
use crate::core::map::utils::spawn_main_button;
use crate::core::menu::utils::add_root_node;
use crate::core::player::Player;
use crate::core::states::GameState;
use crate::core::turns::StartTurnMsg;
use crate::core::ui::systems::UiState;
use crate::utils::NameFromEnum;

#[derive(Component)]
pub struct CombatMenuCmp;

#[derive(Component)]
pub struct CombatCmp;

pub fn setup_combat_menu(
    mut commands: Commands,
    mut play_audio_ev: MessageWriter<PlayAudioMsg>,
    assets: Local<WorldAssets>,
) {
    play_audio_ev.write(PlayAudioMsg::new("horn"));

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

pub fn setup_combat(
    mut commands: Commands,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    mut play_audio_ev: MessageWriter<PlayAudioMsg>,
    assets: Local<WorldAssets>,
) {
    play_audio_ev.write(PlayAudioMsg::new("drums"));

    let report = player.reports.iter().find(|r| r.id == state.in_combat.unwrap()).unwrap();
    let destination = map.get(report.mission.destination);

    commands.spawn((
        add_root_node(true),
        ImageNode::new(assets.image(format!("{} large", destination.kind.to_lowername()))),
        CombatCmp,
    ));

    spawn_main_button(&mut commands, "Exit combat", &assets)
        .insert((ZIndex(6), CombatCmp))
        .observe(|_: On<Pointer<Click>>, mut next_game_state: ResMut<NextState<GameState>>| {
            next_game_state.set(GameState::CombatMenu);
        });
}
