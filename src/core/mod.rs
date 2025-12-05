mod assets;
mod audio;
mod camera;
pub mod combat;
pub mod constants;
mod map;
mod menu;
pub mod messages;
pub mod missions;
mod network;
mod persistence;
mod player;
mod resources;
mod settings;
mod states;
mod systems;
mod turns;
mod ui;
mod units;
mod utils;

use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use bevy_renet::renet::{RenetClient, RenetServer};
use missions::send_mission;
use strum::IntoEnumIterator;

use crate::core::audio::*;
use crate::core::camera::{move_camera, move_camera_keyboard, reset_camera, setup_camera};
use crate::core::combat::systems::{
    animate_combat, exit_combat, exit_combat_menu, run_combat_animations, setup_combat,
    setup_combat_menu, update_combat_stats, CombatCmp, CombatMenuCmp,
};
use crate::core::map::map::{Map, MapCmp};
use crate::core::map::systems::{
    draw_map, run_map_animations, update_end_turn, update_planet_info, update_voronoi,
};
use crate::core::menu::buttons::MenuCmp;
use crate::core::menu::systems::{
    setup_end_game, setup_game_menu, setup_game_settings, setup_menu, update_ip,
};
use crate::core::messages::MessageMsg;
use crate::core::missions::{update_missions, SendMissionMsg};
use crate::core::network::*;
use crate::core::persistence::{load_game, save_game, LoadGameMsg, SaveGameMsg};
use crate::core::settings::Settings;
use crate::core::states::{AppState, AudioState, CombatState, GameState};
use crate::core::systems::{check_keys, check_keys_combat, check_keys_menu, on_resize_system};
use crate::core::turns::{check_turn_ended, resolve_turn, start_turn, StartTurnMsg};
use crate::core::ui::systems::{add_ui_images, draw_ui, set_ui_style};
use crate::core::ui::utils::ImageIds;
use crate::core::utils::despawn;

pub struct GamePlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct InGameSet;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct InPlayingGameSet;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app
            // States
            .init_state::<AppState>()
            .init_state::<GameState>()
            .init_state::<CombatState>()
            .init_state::<AudioState>()
            // Messages
            .add_message::<PlayAudioMsg>()
            .add_message::<PauseAudioMsg>()
            .add_message::<StopAudioMsg>()
            .add_message::<ChangeAudioMsg>()
            .add_message::<SaveGameMsg>()
            .add_message::<LoadGameMsg>()
            .add_message::<ServerSendMsg>()
            .add_message::<ClientSendMsg>()
            .add_message::<MessageMsg>()
            .add_message::<StartTurnMsg>()
            .add_message::<SendMissionMsg>()
            // Resources
            .init_resource::<Ip>()
            .init_resource::<Settings>()
            .init_resource::<ImageIds>()
            .init_resource::<PlayingAudio>()
            // Sets
            .configure_sets(First, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(PreUpdate, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(Update, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(EguiPrimaryContextPass, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(PostUpdate, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(Last, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(
                First,
                InPlayingGameSet.run_if(in_state(GameState::Playing)).in_set(InGameSet),
            )
            .configure_sets(
                PreUpdate,
                InPlayingGameSet.run_if(in_state(GameState::Playing)).in_set(InGameSet),
            )
            .configure_sets(
                Update,
                InPlayingGameSet.run_if(in_state(GameState::Playing)).in_set(InGameSet),
            )
            .configure_sets(
                PostUpdate,
                InPlayingGameSet.run_if(in_state(GameState::Playing)).in_set(InGameSet),
            )
            .configure_sets(
                Last,
                InPlayingGameSet.run_if(in_state(GameState::Playing)).in_set(InGameSet),
            )
            // Camera
            .add_systems(Startup, setup_camera)
            .add_systems(Update, (move_camera, move_camera_keyboard).in_set(InPlayingGameSet))
            // Audio
            .add_systems(Startup, setup_audio)
            .add_systems(OnEnter(GameState::Playing), play_music)
            .add_systems(
                Update,
                (toggle_audio_keyboard, update_audio, play_audio, pause_audio, stop_audio),
            )
            //Networking
            .add_systems(
                First,
                (
                    server_receive_message.run_if(resource_exists::<RenetServer>),
                    client_receive_message.run_if(resource_exists::<RenetClient>),
                ),
            )
            .add_systems(Update, server_update.run_if(resource_exists::<RenetServer>))
            .add_systems(
                Last,
                (
                    server_send_message.run_if(resource_exists::<RenetServer>),
                    client_send_message.run_if(resource_exists::<RenetClient>),
                ),
            );

        // Menu
        for state in AppState::iter().filter(|s| *s != AppState::Game) {
            app.add_systems(OnEnter(state), setup_menu)
                .add_systems(OnExit(state), despawn::<MenuCmp>);
        }
        app.add_systems(Update, update_ip.run_if(in_state(AppState::MultiPlayerMenu)));

        app
            // Ui
            .add_systems(OnExit(AppState::MainMenu), (add_ui_images, set_ui_style))
            .add_systems(EguiPrimaryContextPass, draw_ui.in_set(InGameSet))
            // Persistence
            .add_systems(
                Update,
                (load_game, save_game.run_if(resource_exists::<Host>).in_set(InGameSet)),
            )
            // Utilities
            .add_systems(
                Update,
                (
                    check_keys_menu,
                    check_keys.in_set(InPlayingGameSet),
                    check_keys_combat
                        .run_if(in_state(GameState::CombatMenu).or(in_state(GameState::Combat)))
                        .in_set(InGameSet),
                ),
            )
            .add_systems(PostUpdate, on_resize_system)
            // In-game states
            .add_systems(OnEnter(AppState::Game), draw_map)
            .add_systems(First, start_turn.run_if(resource_exists::<Map>).in_set(InPlayingGameSet))
            .add_systems(
                Update,
                (
                    (update_end_turn, run_map_animations).in_set(InGameSet),
                    (update_voronoi, update_planet_info, send_mission, update_missions)
                        .in_set(InPlayingGameSet),
                ),
            )
            .add_systems(
                PostUpdate,
                check_turn_ended.run_if(resource_exists::<RenetClient>).in_set(InGameSet),
            )
            .add_systems(Last, resolve_turn.run_if(resource_exists::<Host>).in_set(InGameSet))
            .add_systems(OnExit(AppState::Game), (despawn::<MapCmp>, reset_camera))
            .add_systems(OnEnter(GameState::CombatMenu), setup_combat_menu)
            .add_systems(
                OnExit(GameState::CombatMenu),
                (despawn::<CombatMenuCmp>, exit_combat_menu),
            )
            .add_systems(OnEnter(GameState::Combat), setup_combat)
            .add_systems(
                Update,
                (animate_combat, run_combat_animations, update_combat_stats)
                    .run_if(in_state(GameState::Combat)),
            )
            .add_systems(OnExit(GameState::Combat), (despawn::<CombatCmp>, exit_combat))
            .add_systems(OnEnter(GameState::GameMenu), setup_game_menu)
            .add_systems(OnExit(GameState::GameMenu), despawn::<MenuCmp>)
            .add_systems(OnEnter(GameState::Settings), setup_game_settings)
            .add_systems(OnExit(GameState::Settings), despawn::<MenuCmp>)
            .add_systems(OnEnter(GameState::EndGame), setup_end_game)
            .add_systems(OnExit(GameState::EndGame), despawn::<MenuCmp>);
    }
}
