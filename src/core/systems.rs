//! Cross-state Bevy input, resize, and keyboard-navigation systems.

use bevy::prelude::*;
use bevy::window::{CursorIcon, SystemCursorIcon, WindowResized};
use itertools::Itertools;

use crate::core::camera::MainCamera;
use crate::core::combat::systems::BackgroundImageCmp;
use crate::core::map::model::{Map, MapCmp};
use crate::core::menu::utils::{add_root_node, TextSize};
use crate::core::player::Player;
use crate::core::settings::Settings;
#[cfg(debug_assertions)]
use crate::core::simulation::{preview_commands, TurnCommand};
use crate::core::states::{AppState, GameState};
use crate::core::turns::StartTurnMsg;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::multiplayer::client::MultiplayerRequest;
#[cfg(debug_assertions)]
use crate::multiplayer::client::{MultiplayerSession, PendingTurnCommands};

#[derive(Component)]
/// Invisible full-screen Bevy UI layer that blocks picking beneath in-game menus.
pub(crate) struct GameplayInputBlocker;

/// Blocks map picking and clears transient hover state without interrupting Egui menu input.
pub(crate) fn suspend_gameplay_interactions(
    mut commands: Commands,
    mut state: Option<ResMut<UiState>>,
    window: Query<Entity, With<Window>>,
    blockers: Query<Entity, With<GameplayInputBlocker>>,
) {
    if blockers.is_empty() {
        // A Bevy blocker also catches drags that began before Egui's modal area appeared.
        // Keep the picking pipeline running so button releases are not replayed on resume.
        commands.spawn((add_root_node(true), GameplayInputBlocker, MapCmp));
    }
    if let Some(state) = state.as_mut() {
        state.planet_hover = None;
        state.mission_hover = None;
        state.mission_hover_from_ui = false;
        state.combat_report_hover = None;
    }
    if let Ok(window) = window.single() {
        commands.entity(window).insert(CursorIcon::from(SystemCursorIcon::Default));
    }
}

/// Restores map picking when the player closes the in-game menus.
pub(crate) fn resume_gameplay_interactions(
    mut commands: Commands,
    blockers: Query<Entity, With<GameplayInputBlocker>>,
) {
    for blocker in &blockers {
        commands.entity(blocker).despawn();
    }
}

/// Handles the resize system interaction.
pub fn on_resize_system(
    mut resize_reader: MessageReader<WindowResized>,
    mut text: Query<(&mut TextFont, &TextSize)>,
    mut bg_q: Query<&mut Sprite, With<BackgroundImageCmp>>,
    camera: Single<&Projection, With<MainCamera>>,
) {
    let Projection::Orthographic(projection) = camera.into_inner() else {
        return;
    };

    let (width, height) = (projection.area.width(), projection.area.height());

    for window in resize_reader.read() {
        for (mut text, size) in text.iter_mut() {
            text.font_size = (size.0 * window.height / 460.).into()
        }

        // Resize background images to cover the whole screen
        for mut bg_s in &mut bg_q {
            bg_s.custom_size = Some(Vec2::new(width, height));
        }
    }
}

/// Checks keys menu input/state and applies the resulting transition.
pub fn check_keys_menu(
    app_state: Res<State<AppState>>,
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut state: Option<ResMut<UiState>>,
    mut start_turn_msg: MessageWriter<StartTurnMsg>,
    mut multiplayer: MessageWriter<MultiplayerRequest>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    let ctrl_pressed = keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);

    if keyboard.just_pressed(KeyCode::Escape) {
        match app_state.get() {
            AppState::SinglePlayerMenu
            | AppState::CreateGame
            | AppState::JoinGame
            | AppState::ResumeGame
            | AppState::Settings => next_app_state.set(AppState::MainMenu),
            AppState::RecoverPlayer => next_app_state.set(AppState::ResumeGame),
            AppState::Lobby => {
                multiplayer.write(MultiplayerRequest::LeaveGame);
            },
            AppState::Game => {
                // Open in-game menu or exit mission/planet selection
                match game_state.get() {
                    GameState::Playing => {
                        if let Some(state) = state.as_mut() {
                            if state.planet_selected.is_some() || state.mission {
                                state.planet_selected = None;
                                state.mission = false;
                                state.combat_report = None;
                            } else {
                                next_game_state.set(GameState::GameMenu)
                            }
                        }
                    },
                    GameState::CombatMenu | GameState::GameMenu => {
                        next_game_state.set(GameState::Playing)
                    },
                    GameState::Combat => next_game_state.set(GameState::CombatMenu),
                    GameState::EndGame => next_app_state.set(AppState::MainMenu),
                    GameState::Settings => next_game_state.set(GameState::GameMenu),
                }
            },
            AppState::Boot | AppState::MainMenu | AppState::LoadingGame => {},
        }
    }

    if ctrl_pressed && keyboard.just_pressed(KeyCode::Enter) && *app_state.get() == AppState::Game {
        if *game_state.get() == GameState::Playing {
            if let Some(mut state) = state {
                if !state.mission {
                    state.planet_selected = None;
                    state.mission = false;
                    state.combat_report = None;
                    state.end_turn = true;
                }
            }
        } else if *game_state.get() == GameState::CombatMenu {
            start_turn_msg.write(StartTurnMsg::new(true, false));
            next_game_state.set(GameState::Playing)
        }
    }
}

/// Checks keys combat input/state and applies the resulting transition.
pub fn check_keys_combat(mut settings: ResMut<Settings>, keyboard: Res<ButtonInput<KeyCode>>) {
    if keyboard.just_pressed(KeyCode::Space) {
        settings.combat_paused = !settings.combat_paused;
    } else if keyboard.just_released(KeyCode::ArrowRight) {
        settings.combat_speed = (settings.combat_speed * 2.).min(64.0);
    } else if keyboard.just_released(KeyCode::ArrowLeft) {
        settings.combat_speed = (settings.combat_speed * 0.5).max(0.25);
    }
}

/// Keeps display-preference shortcuts active while either Settings menu is open.
pub fn check_preference_keys(keyboard: Res<ButtonInput<KeyCode>>, mut settings: ResMut<Settings>) {
    if keyboard.just_pressed(KeyCode::KeyC) {
        settings.show_cells = !settings.show_cells;
    }
    if keyboard.just_pressed(KeyCode::KeyH) {
        settings.show_hover = !settings.show_hover;
    }
}

/// Checks keys input/state and applies the resulting transition.
pub fn check_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    map: Res<Map>,
    player: Res<Player>,
    mut state: ResMut<UiState>,
    mut settings: ResMut<Settings>,
) {
    let ctrl_pressed = keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    // Toggle show planet info
    if keyboard.just_pressed(KeyCode::KeyI) {
        settings.show_info = !settings.show_info;
    }

    // Toggle shop panel
    if keyboard.just_pressed(KeyCode::KeyB) {
        settings.show_menu = !settings.show_menu;
    }

    // Toggle mission panel
    if keyboard.just_pressed(KeyCode::KeyM) {
        state.planet_selected = None;
        state.mission = !state.mission;
        state.combat_report = None;
    }

    // Go back to home planet
    if keyboard.just_pressed(KeyCode::Space) {
        state.planet_selected = Some(player.home_planet);
        state.to_selected = true;
        state.mission = false;
    }

    // Move between owned planets / moons
    if ctrl_pressed {
        if keyboard.just_pressed(KeyCode::Tab) && !state.mission && state.combat_report.is_none() {
            if let Some(selected) = state.planet_selected {
                let planets: Vec<_> = map
                    .planets
                    .iter()
                    .sorted_by(|a, b| a.name.cmp(&b.name))
                    .filter_map(|p| {
                        (player.owns(p) || (p.is_moon() && player.controls(p))).then_some(p.id)
                    })
                    .collect();

                if let Some(pos) = planets.iter().position(|id| *id == selected) {
                    let len = planets.len();

                    let new_index = if shift_pressed {
                        (pos + len - 1) % len
                    } else {
                        (pos + 1) % len
                    };

                    state.planet_selected = Some(planets[new_index]);
                }
            }
        }
    } else if let Some(id) = state.combat_report {
        let Some(max_rounds) = player
            .reports
            .iter()
            .find(|report| report.id == id)
            .and_then(|report| report.combat_report.as_ref())
            .map(|combat| combat.rounds.len())
            .filter(|rounds| *rounds > 0)
        else {
            state.combat_report = None;
            return;
        };

        // Move between rounds
        if mouse.just_pressed(MouseButton::Forward) || keyboard.just_pressed(KeyCode::Tab) {
            state.combat_report_round = (state.combat_report_round + 1).min(max_rounds);
        } else if mouse.just_pressed(MouseButton::Back)
            || (shift_pressed && keyboard.just_pressed(KeyCode::Tab))
        {
            state.combat_report_round = state.combat_report_round.saturating_sub(1).max(1);
        }
    } else if state.mission {
        // Move between mission or shop tabs
        if mouse.just_pressed(MouseButton::Forward) || keyboard.just_pressed(KeyCode::Tab) {
            state.mission_tab = match &state.mission_tab {
                MissionTab::NewMission => MissionTab::ActiveMissions,
                MissionTab::ActiveMissions => MissionTab::EnemyMissions,
                MissionTab::EnemyMissions => MissionTab::MissionReports,
                MissionTab::MissionReports => MissionTab::NewMission,
            };
        } else if mouse.just_pressed(MouseButton::Back)
            || (shift_pressed && keyboard.just_pressed(KeyCode::Tab))
        {
            state.mission_tab = match &state.mission_tab {
                MissionTab::NewMission => MissionTab::MissionReports,
                MissionTab::ActiveMissions => MissionTab::NewMission,
                MissionTab::EnemyMissions => MissionTab::ActiveMissions,
                MissionTab::MissionReports => MissionTab::EnemyMissions,
            };
        }
    } else if settings.show_menu && state.planet_selected.is_some() {
        let Some(planet) = state.planet_selected.and_then(|id| map.try_get(id)) else {
            state.planet_selected = None;
            return;
        };
        if mouse.just_pressed(MouseButton::Back)
            || (shift_pressed && keyboard.just_pressed(KeyCode::Tab))
        {
            state.shop = state.shop.previous(planet.is_moon());
        } else if mouse.just_pressed(MouseButton::Forward) || keyboard.just_pressed(KeyCode::Tab) {
            state.shop = state.shop.next(planet.is_moon());
        }
    }
}

#[cfg(debug_assertions)]
/// Queues and previews the practice shortcut so subsequent orders use the same canonical draft.
pub fn debug_cheat_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    session: Res<MultiplayerSession>,
    mut pending: ResMut<PendingTurnCommands>,
    mut messages: MessageWriter<crate::core::messages::MessageMsg>,
) {
    let ctrl_pressed = keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    if !ctrl_pressed
        || !keyboard.just_pressed(KeyCode::ArrowUp)
        || !session.local_practice
        || !pending.is_editable()
    {
        return;
    }
    let Some(record) = &session.active_game else {
        return;
    };
    if pending.turn != record.persisted.state.turn {
        return;
    }

    if !pending.push(TurnCommand::PracticeBoost {
        owned_worlds_only: keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]),
    }) {
        messages.write(crate::core::messages::MessageMsg::error(
            "This turn already contains the maximum number of commands.",
        ));
        return;
    }
    let preview = preview_commands(&record.persisted.state, player.id, &pending.commands);
    match preview.and_then(|model| Ok((model.player(player.id)?.clone(), model.map))) {
        Ok((preview_player, preview_map)) => {
            *player = preview_player;
            *map = preview_map;
        },
        Err(error) => {
            pending.commands.pop();
            messages.write(crate::core::messages::MessageMsg::error(format!(
                "Could not apply testing shortcut: {error}"
            )));
        },
    }
}

#[cfg(test)]
#[path = "../../tests/core/systems.rs"]
mod tests;
