mod assets;
mod audio;
mod camera;
pub mod constants;
mod game_settings;
mod map;
mod network;
mod persistence;
mod player;
mod resources;
mod states;
mod systems;
mod ui;
mod utils;

use crate::core::audio::{
    change_audio_event, play_audio_event, play_music, setup_music_btn, toggle_music_keyboard,
};
use crate::core::camera::{move_camera_keyboard, reset_camera, setup_camera};
use crate::core::game_settings::GameSettings;
use crate::core::map::map::{Map, MapCmp};
use crate::core::network::Ip;
#[cfg(not(target_arch = "wasm32"))]
use crate::core::persistence::{load_game, save_game};
use crate::core::persistence::{LoadGameEv, SaveGameEv};
use crate::core::states::{AppState, AudioState, GameState};
use crate::core::systems::{check_keys, initialize_game, on_resize_system};
use crate::core::utils::{despawn};
use bevy::prelude::*;
use crate::core::map::systems::draw_map;

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
            .add_event::<LoadGameEv>()
            .add_event::<SaveGameEv>()
            // Resources
            .init_resource::<Ip>()
            .init_resource::<GameSettings>()
            // Sets
            .configure_sets(PreUpdate, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(Update, InGameSet.run_if(in_state(AppState::Game)))
            .configure_sets(PostUpdate, InGameSet.run_if(in_state(AppState::Game)))
            // Camera
            .add_systems(Startup, (initialize_game, setup_camera, draw_map).chain());
            // .add_systems(
            //     Update,
            //     (move_camera_keyboard)
            //         .run_if(not(in_state(GameState::InGameMenu)))
            //         .in_set(InGameSet),
            // )
            // // Audio
            // .add_systems(Startup, setup_music_btn)
            // .add_systems(OnEnter(AudioState::Sound), play_music)
            // .add_systems(
            //     Update,
            //     (change_audio_event, toggle_music_keyboard, play_audio_event),
            // );
        //Networking
        // .add_systems(
        //     First,
        //     (
        //         server_receive_message.run_if(resource_exists::<RenetServer>),
        //         client_receive_message.run_if(resource_exists::<RenetClient>),
        //     )
        //         .in_set(InGameSet),
        // )
        // .add_systems(PreUpdate, update_population_event.in_set(InGameSet))
        // .add_systems(
        //     Update,
        //     server_update
        //         .run_if(resource_exists::<RenetServer>)
        //         .run_if(not(in_state(AppState::Game))),
        // )
        // .add_systems(
        //     Last,
        //     (
        //         (
        //             server_send_status.run_if(on_timer(Duration::from_millis(NETWORK_TIMER))),
        //             server_send_message,
        //         )
        //             .run_if(resource_exists::<RenetServer>),
        //         (
        //             client_send_status.run_if(on_timer(Duration::from_millis(NETWORK_TIMER))),
        //             client_send_message,
        //         )
        //             .run_if(resource_exists::<RenetClient>),
        //     )
        //         .in_set(InGameSet),
        // );

        // Menu
        // for state in AppState::iter().filter(|s| *s != AppState::Game) {
        //     app.add_systems(OnEnter(state), setup_menu)
        //         .add_systems(OnExit(state), despawn::<MenuCmp>);
        // }
        // app.add_systems(
        //     Update,
        //     update_ip.run_if(in_state(AppState::MultiPlayerMenu)),
        // );

        // Utilities
        // app.add_systems(Update, check_keys.in_set(InGameSet))
        //     .add_systems(
        //         PostUpdate,
        //         (
        //             on_resize_system,
        //             // update_transform_no_rotation.before(TransformSystems::Propagate),
        //         ),
        //     )
        //     // Map
        //     .add_systems(OnEnter(AppState::Game), (despawn::<MapCmp>, draw_map))
        //     // .add_systems(Update, (animate_ui, update_ui).in_set(InGameSet))
        //     .add_systems(
        //         OnExit(AppState::Game),
        //         (despawn::<MapCmp>, reset_camera, initialize_game, draw_map).chain(),
        //     );

        // Persistence
        // #[cfg(not(target_arch = "wasm32"))]
        // app.add_systems(Update, (load_game, save_game));
    }
}
