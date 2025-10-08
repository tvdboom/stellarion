mod assets;
mod audio;
mod camera;
pub mod constants;
mod map;
mod menu;
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

use crate::core::audio::{
    change_audio_event, play_audio_event, play_music, setup_music_btn, toggle_music_keyboard,
    ChangeAudioEv, PlayAudioEv,
};
use crate::core::camera::{move_camera, move_camera_keyboard, reset_camera, setup_camera};
use crate::core::map::map::MapCmp;
use crate::core::map::systems::{draw_map, update_planet_info};
use crate::core::menu::buttons::MenuCmp;
use crate::core::menu::systems::{setup_end_game, setup_in_game_menu, setup_menu, update_ip};
use crate::core::network::{
    client_receive_message, server_receive_message, server_update, ClientSendMessage, Ip,
    ServerSendMessage,
};
#[cfg(not(target_arch = "wasm32"))]
use crate::core::persistence::{load_game, save_game};
use crate::core::persistence::{LoadGameEv, SaveGameEv};
use crate::core::settings::Settings;
use crate::core::states::{AppState, AudioState, GameState};
use crate::core::systems::{check_keys, on_resize_system};
use crate::core::turns::{next_turn, NextTurnEv};
use crate::core::ui::systems::{add_ui_images, draw_ui, set_ui_style, ImageIds, UiState};
use crate::core::utils::despawn;
use bevy::prelude::*;
use bevy_egui::{EguiPrimaryContextPass, EguiStartupSet};
use bevy_renet::renet::{RenetClient, RenetServer};
use strum::IntoEnumIterator;

pub struct GamePlugin;

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
struct InGameSet;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app
            // States
            .init_state::<AppState>()
            .init_state::<GameState>()
            .init_state::<AudioState>()
            // Events
            .add_event::<PlayAudioEv>()
            .add_event::<ChangeAudioEv>()
            .add_event::<SaveGameEv>()
            .add_event::<LoadGameEv>()
            .add_event::<ServerSendMessage>()
            .add_event::<ClientSendMessage>()
            .add_event::<NextTurnEv>()
            // Resources
            .init_resource::<Ip>()
            .init_resource::<Settings>()
            .init_resource::<ImageIds>()
            .init_resource::<UiState>()
            // Sets
            .configure_sets(PreUpdate, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(Update, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(EguiPrimaryContextPass, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(PostUpdate, InGameSet.run_if(in_state(AppState::Game)))
            // Camera
            .add_systems(PreStartup, setup_camera.before(EguiStartupSet::InitContexts))
            .add_systems(Update, (move_camera, move_camera_keyboard).in_set(InGameSet))
            // Audio
            .add_systems(Startup, setup_music_btn)
            .add_systems(OnEnter(AudioState::Sound), play_music)
            .add_systems(Update, (change_audio_event, toggle_music_keyboard, play_audio_event))
            //Networking
            .add_systems(
                First,
                (
                    server_receive_message.run_if(resource_exists::<RenetServer>),
                    client_receive_message.run_if(resource_exists::<RenetClient>),
                )
                    .in_set(InGameSet),
            )
            .add_systems(
                Update,
                server_update
                    .run_if(resource_exists::<RenetServer>)
                    .run_if(not(in_state(AppState::Game))),
            );

        // Menu
        for state in AppState::iter().filter(|s| *s != AppState::Game) {
            app.add_systems(OnEnter(state), setup_menu)
                .add_systems(OnExit(state), despawn::<MenuCmp>);
        }
        app.add_systems(Update, update_ip.run_if(in_state(AppState::MultiPlayerMenu)));

        app
            // Ui
            .add_systems(Startup, (add_ui_images, set_ui_style))
            .add_systems(EguiPrimaryContextPass, draw_ui.in_set(InGameSet))
            // Utilities
            .add_systems(Update, check_keys.in_set(InGameSet))
            .add_systems(PostUpdate, on_resize_system)
            // In-game states
            .add_systems(OnEnter(AppState::Game), (despawn::<MapCmp>, draw_map))
            .add_systems(Update, (next_turn, update_planet_info).in_set(InGameSet))
            .add_systems(OnExit(AppState::Game), (despawn::<MapCmp>, reset_camera))
            .add_systems(OnEnter(GameState::InGameMenu), setup_in_game_menu)
            .add_systems(OnExit(GameState::InGameMenu), despawn::<MenuCmp>)
            .add_systems(OnEnter(GameState::EndGame), setup_end_game)
            .add_systems(OnExit(GameState::EndGame), despawn::<MenuCmp>);

        // Persistence
        #[cfg(not(target_arch = "wasm32"))]
        app.add_systems(Update, (load_game, save_game));
    }
}
