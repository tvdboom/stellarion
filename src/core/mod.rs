//! Shared deterministic rules plus the optional Bevy application plugin.

#[cfg(feature = "app")]
mod assets;
#[cfg(feature = "app")]
mod audio;
#[cfg(feature = "app")]
pub mod basis_texture;
#[cfg(feature = "app")]
mod camera;
pub mod combat;
pub mod constants;
pub mod identity;
#[cfg(feature = "app")]
mod loading;
pub mod map;
#[cfg(feature = "app")]
mod menu;
#[cfg(feature = "app")]
pub mod messages;
pub mod missions;
pub mod player;
pub mod random;
pub mod resources;
mod settings;
pub mod simulation;
pub mod states;
#[cfg(feature = "app")]
mod systems;
#[cfg(feature = "app")]
mod turns;
#[cfg(feature = "app")]
mod ui;
pub mod units;
#[cfg(feature = "app")]
mod utils;

#[cfg(feature = "app")]
use bevy::prelude::*;
#[cfg(feature = "app")]
use bevy_egui::EguiPrimaryContextPass;
#[cfg(feature = "app")]
use missions::send_mission;
#[cfg(feature = "app")]
use strum::IntoEnumIterator;

#[cfg(feature = "app")]
use crate::core::assets::WorldAssets;
#[cfg(feature = "app")]
use crate::core::audio::*;
#[cfg(feature = "app")]
use crate::core::basis_texture::BasisTexturePlugin;
#[cfg(feature = "app")]
use crate::core::camera::{move_camera, move_camera_keyboard, reset_camera, setup_camera};
#[cfg(feature = "app")]
use crate::core::combat::systems::{
    animate_combat, exit_combat, exit_combat_menu, run_combat_animations, setup_combat,
    setup_combat_menu, update_combat_stats, CombatCmp, CombatMenuCmp, SpawnShotMsg,
};
#[cfg(feature = "app")]
use crate::core::loading::{
    begin_gameplay_loading, finish_boot, finish_gameplay_loading, refresh_gameplay_projection,
};
#[cfg(feature = "app")]
use crate::core::map::model::{Map, MapCmp};
#[cfg(feature = "app")]
use crate::core::map::systems::{
    draw_map, run_map_animations, update_end_turn, update_planet_info, update_voronoi,
};
#[cfg(feature = "app")]
use crate::core::menu::buttons::MenuCmp;
#[cfg(feature = "app")]
use crate::core::menu::systems::{
    draw_game_overlay, draw_menu, exit_end_game, fit_menu_background, setup_menu,
};
#[cfg(feature = "app")]
use crate::core::messages::MessageMsg;
#[cfg(feature = "app")]
use crate::core::missions::{update_mission_route_arrow, update_missions, SendMissionMsg};
#[cfg(feature = "app")]
use crate::core::settings::Settings;
#[cfg(feature = "app")]
use crate::core::states::{AppState, AudioState, CombatState, GameState};
#[cfg(all(feature = "app", debug_assertions))]
use crate::core::systems::debug_cheat_keys;
#[cfg(feature = "app")]
use crate::core::systems::{
    check_keys, check_keys_combat, check_keys_menu, check_preference_keys, on_resize_system,
};
#[cfg(feature = "app")]
use crate::core::turns::{check_turn_ended, start_turn, StartTurnMsg};
#[cfg(feature = "app")]
use crate::core::ui::systems::{add_ui_images, draw_ui, set_ui_style};
#[cfg(feature = "app")]
use crate::core::ui::utils::ImageIds;
#[cfg(feature = "app")]
use crate::core::utils::despawn;
#[cfg(feature = "app")]
use crate::multiplayer::client::MultiplayerClientPlugin;

#[cfg(feature = "app")]
/// Bevy plugin that assembles Stellarion gameplay, presentation, menus, and multiplayer systems.
pub struct GamePlugin;

#[cfg(feature = "app")]
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
/// System set gated by the top-level in-game application state.
struct InGameSet;

#[cfg(feature = "app")]
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
/// System set gated by both in-game and actively playing states.
struct InPlayingGameSet;

#[cfg(feature = "app")]
impl Plugin for GamePlugin {
    /// Registers this plugin's resources, messages, and ordered systems.
    fn build(&self, app: &mut App) {
        app.add_plugins(BasisTexturePlugin)
            // States
            .init_state::<AppState>()
            .init_state::<GameState>()
            .init_state::<CombatState>()
            .init_state::<AudioState>()
            // Messages
            .add_message::<PlayAudioMsg>()
            .add_message::<PauseAudioMsg>()
            .add_message::<StopAudioMsg>()
            .add_message::<MuteAudioMsg>()
            .add_message::<ChangeAudioMsg>()
            .add_message::<MessageMsg>()
            .add_message::<StartTurnMsg>()
            .add_message::<SendMissionMsg>()
            .add_message::<SpawnShotMsg>()
            // Resources
            .init_resource::<Settings>()
            .init_resource::<ImageIds>()
            .init_resource::<PlayingAudio>()
            .init_resource::<WorldAssets>()
            .add_plugins(MultiplayerClientPlugin)
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
                (toggle_audio, update_audio, play_audio, pause_audio, stop_audio, mute_audio),
            );

        // Menu
        for state in AppState::iter().filter(|s| *s != AppState::Game) {
            app.add_systems(OnEnter(state), setup_menu)
                .add_systems(OnExit(state), despawn::<MenuCmp>);
        }

        app
            // Ui
            .add_systems(
                EguiPrimaryContextPass,
                (set_ui_style, draw_menu.run_if(not(in_state(AppState::Game)))).chain(),
            )
            .add_systems(
                EguiPrimaryContextPass,
                (draw_ui, draw_game_overlay).chain().in_set(InGameSet),
            )
            // Deferred loading
            .add_systems(Update, fit_menu_background.run_if(not(in_state(AppState::Game))))
            .add_systems(Update, finish_boot.run_if(in_state(AppState::Boot)))
            .add_systems(OnEnter(AppState::LoadingGame), begin_gameplay_loading)
            .add_systems(Update, finish_gameplay_loading.run_if(in_state(AppState::LoadingGame)))
            .add_systems(Update, refresh_gameplay_projection.in_set(InGameSet))
            .add_systems(OnExit(AppState::LoadingGame), add_ui_images)
            // Utilities
            .add_systems(
                Update,
                (
                    check_keys_menu,
                    check_preference_keys
                        .run_if(in_state(AppState::Game).or_else(in_state(AppState::Settings))),
                    check_keys.in_set(InPlayingGameSet),
                    check_keys_combat
                        .run_if(
                            in_state(GameState::CombatMenu).or_else(in_state(GameState::Combat)),
                        )
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
                    (update_end_turn, run_map_animations, update_voronoi).in_set(InGameSet),
                    (update_planet_info, send_mission, update_missions, update_mission_route_arrow)
                        .in_set(InPlayingGameSet),
                ),
            )
            .add_systems(PostUpdate, check_turn_ended.in_set(InGameSet))
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
                    .chain()
                    .run_if(in_state(GameState::Combat)),
            )
            .add_systems(OnExit(GameState::Combat), (despawn::<CombatCmp>, exit_combat))
            .add_systems(OnExit(GameState::EndGame), exit_end_game);

        #[cfg(debug_assertions)]
        app.add_systems(Update, debug_cheat_keys.in_set(InPlayingGameSet).before(check_keys));
    }
}
