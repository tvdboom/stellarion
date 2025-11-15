use bevy::prelude::*;
use bevy::window::WindowResized;
use itertools::Itertools;

use crate::core::map::map::Map;
use crate::core::menu::utils::TextSize;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::{AppState, GameState};
use crate::core::ui::systems::{MissionTab, Shop, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::Unit;

pub fn on_resize_system(
    mut resize_reader: MessageReader<WindowResized>,
    mut text: Query<(&mut TextFont, &TextSize)>,
) {
    for ev in resize_reader.read() {
        for (mut text, size) in text.iter_mut() {
            text.font_size = size.0 * ev.height / 460.
        }
    }
}

pub fn check_keys_menu(
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut state: ResMut<UiState>,
    keyboard: Res<ButtonInput<KeyCode>>,
) {
    // Open in-game menu or exit mission/planet selection
    if keyboard.just_pressed(KeyCode::Escape) {
        match game_state.get() {
            GameState::Playing => {
                if state.planet_selected.is_some() || state.mission {
                    state.planet_selected = None;
                    state.mission = false;
                    state.combat_report = None;
                } else {
                    next_game_state.set(GameState::InGameMenu)
                }
            },
            GameState::InGameMenu => next_game_state.set(GameState::Playing),
            GameState::EndGame => next_app_state.set(AppState::MainMenu),
        }
    }
}

pub fn check_keys(
    keyboard: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut state: ResMut<UiState>,
    mut settings: ResMut<Settings>,
) {
    let ctrl_pressed = keyboard.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight]);
    let shift_pressed = keyboard.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);

    // Hack to add resources and bump building levels to max
    if ctrl_pressed && shift_pressed && keyboard.just_pressed(KeyCode::ArrowUp) {
        player.resources += 10_000usize;
        map.planets.iter_mut().filter(|p| p.owned == Some(player.id)).for_each(|p| {
            for unit in Unit::all().iter().flatten() {
                if unit.is_building() {
                    *p.army.entry(*unit).or_insert(0) = Building::MAX_LEVEL;
                } else {
                    *p.army.entry(*unit).or_insert(0) += 10;
                }
            }
        });
    }

    // Toggle show voronoi cells
    if keyboard.just_pressed(KeyCode::KeyC) {
        settings.show_cells = !settings.show_cells;
    }

    // Toggle show planet info
    if keyboard.just_pressed(KeyCode::KeyI) {
        settings.show_info = !settings.show_info;
    }

    // Toggle show hover info
    if keyboard.just_pressed(KeyCode::KeyH) {
        settings.show_hover = !settings.show_hover;
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

    // Move between owned planets
    if ctrl_pressed {
        if keyboard.just_pressed(KeyCode::Tab) && !state.mission && state.combat_report.is_none() {
            if let Some(selected) = state.planet_selected {
                let planets: Vec<_> = map
                    .planets
                    .iter()
                    .sorted_by(|a, b| a.name.cmp(&b.name))
                    .filter_map(|p| player.owns(p).then_some(p.id))
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
        let report = player.reports.iter().find(|r| r.id == id).unwrap();
        let max_rounds = report.combat_report.as_ref().unwrap().rounds.len();

        // Move between rounds
        if mouse.just_pressed(MouseButton::Forward) || keyboard.just_pressed(KeyCode::Tab) {
            state.combat_report_round = (state.combat_report_round + 1).min(max_rounds);
        } else if mouse.just_pressed(MouseButton::Back)
            || (shift_pressed && keyboard.just_pressed(KeyCode::Tab))
        {
            state.combat_report_round = (state.combat_report_round - 1).max(1);
        }
    } else if state.mission {
        // Move between mission or shop tabs
        if mouse.just_pressed(MouseButton::Forward) || keyboard.just_pressed(KeyCode::Tab) {
            state.mission_tab = match &state.mission_tab {
                MissionTab::NewMission => MissionTab::ActiveMissions,
                MissionTab::ActiveMissions => MissionTab::IncomingAttacks,
                MissionTab::IncomingAttacks => MissionTab::MissionReports,
                MissionTab::MissionReports => MissionTab::NewMission,
            };
        } else if mouse.just_pressed(MouseButton::Back)
            || (shift_pressed && keyboard.just_pressed(KeyCode::Tab))
        {
            state.mission_tab = match &state.mission_tab {
                MissionTab::NewMission => MissionTab::MissionReports,
                MissionTab::ActiveMissions => MissionTab::NewMission,
                MissionTab::IncomingAttacks => MissionTab::ActiveMissions,
                MissionTab::MissionReports => MissionTab::IncomingAttacks,
            };
        }
    } else if settings.show_menu && state.planet_selected.is_some() {
        if mouse.just_pressed(MouseButton::Back)
            || (shift_pressed && keyboard.just_pressed(KeyCode::Tab))
        {
            state.shop = match &state.shop {
                Shop::Buildings => Shop::Defenses,
                Shop::Fleet => Shop::Buildings,
                Shop::Defenses => Shop::Fleet,
            };
        } else if mouse.just_pressed(MouseButton::Forward) || keyboard.just_pressed(KeyCode::Tab) {
            state.shop = match &state.shop {
                Shop::Buildings => Shop::Fleet,
                Shop::Fleet => Shop::Defenses,
                Shop::Defenses => Shop::Buildings,
            };
        }
    }
}
