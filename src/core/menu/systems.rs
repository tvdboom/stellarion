//! Egui-driven native/WASM menus for creation, joining, recovery, resume, and lobby flows.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};

use crate::core::assets::WorldAssets;
use crate::core::audio::MuteAudioMsg;
use crate::core::constants::{
    DISABLED_BUTTON_COLOR, HOVERED_BUTTON_COLOR, NORMAL_BUTTON_COLOR, PRESSED_BUTTON_COLOR,
};
use crate::core::menu::buttons::MenuCmp;
use crate::core::settings::Settings;
use crate::core::simulation::GameRules;
use crate::core::states::{AppState, AudioState, GameState};
use crate::multiplayer::client::{MultiplayerForm, MultiplayerRequest, MultiplayerSession};
use crate::utils::ToColor32;
use crate::TITLE;

/// Spawns the lightweight menu background; gameplay assets are not requested here.
pub fn setup_menu(mut commands: Commands, assets: Res<WorldAssets>) {
    commands.spawn((
        Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            position_type: PositionType::Absolute,
            ..default()
        },
        ImageNode::new(assets.image("menu")).with_mode(NodeImageMode::Stretch),
        Pickable::IGNORE,
        ZIndex(-1),
        MenuBackground,
        MenuCmp,
    ));
}

#[derive(Component)]
/// Marks the menu image whose source rectangle is cropped to cover the viewport.
pub(crate) struct MenuBackground;

/// Keeps the menu art aspect-correct while filling every viewport edge.
pub fn fit_menu_background(
    window: Single<&Window, With<PrimaryWindow>>,
    images: Res<Assets<Image>>,
    mut backgrounds: Query<&mut ImageNode, With<MenuBackground>>,
) {
    let viewport = Vec2::new(window.width(), window.height());
    for mut background in &mut backgrounds {
        let Some(image) = images.get(&background.image) else {
            continue;
        };
        let rect = cover_source_rect(image.size_f32(), viewport);
        if background.rect != Some(rect) {
            background.rect = Some(rect);
        }
    }
}

/// Returns the centered source crop equivalent to CSS `background-size: cover`.
fn cover_source_rect(source: Vec2, viewport: Vec2) -> Rect {
    if source.min_element() <= 0.0 || viewport.min_element() <= 0.0 {
        return Rect::from_corners(Vec2::ZERO, source.max(Vec2::ZERO));
    }

    let source_aspect = source.x / source.y;
    let viewport_aspect = viewport.x / viewport.y;
    if viewport_aspect > source_aspect {
        let cropped_height = source.x / viewport_aspect;
        let offset = (source.y - cropped_height) * 0.5;
        Rect::from_corners(Vec2::new(0.0, offset), Vec2::new(source.x, offset + cropped_height))
    } else {
        let cropped_width = source.y * viewport_aspect;
        let offset = (source.x - cropped_width) * 0.5;
        Rect::from_corners(Vec2::new(offset, 0.0), Vec2::new(offset + cropped_width, source.y))
    }
}

/// Draws all non-game screens and emits transport-neutral multiplayer requests.
pub fn draw_menu(
    mut contexts: EguiContexts,
    app_state: Res<State<AppState>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut form: ResMut<MultiplayerForm>,
    session: Res<MultiplayerSession>,
    mut settings: ResMut<Settings>,
    mut requests: MessageWriter<MultiplayerRequest>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let mut viewport_ui = egui::Ui::new(
        context.clone(),
        "stellarion_menu_viewport".into(),
        egui::UiBuilder::new()
            .layer_id(egui::LayerId::background())
            .max_rect(context.viewport_rect()),
    );
    egui::CentralPanel::default().frame(egui::Frame::NONE).show(&mut viewport_ui, |ui| {
        apply_menu_style(ui);
        ui.vertical_centered(|ui| {
            let viewport = ui.ctx().viewport_rect().size();
            let viewport_height = viewport.y;
            ui.add_space((viewport_height * 0.06).clamp(24.0, 54.0));
            if *app_state.get() != AppState::Settings {
                ui.heading(
                    egui::RichText::new(TITLE).size((viewport_height * 0.08).clamp(48.0, 76.0)),
                );
                ui.add_space((viewport_height * 0.045).clamp(20.0, 42.0));
            }
            let content_width = (viewport.x * 0.4)
                .clamp(320.0, 640.0)
                .min((ui.available_width() - 32.0).max(240.0));
            ui.allocate_ui_with_layout(
                egui::vec2(content_width, ui.available_height()),
                egui::Layout::top_down(egui::Align::Center),
                |ui| {
                    match app_state.get() {
                        AppState::Boot => boot_screen(ui),
                        AppState::MainMenu => main_screen(ui, &mut next_state),
                        AppState::MultiPlayerMenu => multiplayer_screen(ui, &mut next_state),
                        AppState::CreateGame => create_screen(
                            ui,
                            &mut form,
                            &mut settings,
                            session.busy,
                            &mut requests,
                            &mut next_state,
                        ),
                        AppState::JoinGame => {
                            join_screen(ui, &mut form, session.busy, &mut requests, &mut next_state)
                        },
                        AppState::RecoverPlayer => recovery_screen(
                            ui,
                            &mut form,
                            session.busy,
                            &mut requests,
                            &mut next_state,
                        ),
                        AppState::ResumeGame => {
                            resume_screen(ui, &session, &mut requests, &mut next_state)
                        },
                        AppState::Lobby => {
                            lobby_screen(ui, &session, &mut requests, &mut next_state)
                        },
                        AppState::Settings => settings_screen(ui, &mut settings, &mut next_state),
                        AppState::LoadingGame => loading_screen(ui, &session),
                        AppState::SinglePlayerMenu => {
                            ui.label(
                                "Local practice mode remains available through tests/debug builds.",
                            );
                            if ui.button("Back").clicked() {
                                next_state.set(AppState::MainMenu);
                            }
                        },
                        AppState::Game => {},
                    }
                    if let Some(notice) = &session.notice {
                        ui.add_space(18.0);
                        ui.label(egui::RichText::new(notice).color(egui::Color32::LIGHT_YELLOW));
                    }
                },
            );
        });
    });

    egui::Area::new("stellarion_menu_footer".into())
        .anchor(egui::Align2::RIGHT_BOTTOM, egui::vec2(-24.0, -18.0))
        .show(context, |ui| {
            let mode = if session.mock_backend {
                " · mock backend"
            } else {
                ""
            };
            ui.set_min_width(220.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                ui.label(format!("{}{}", session.connection.label(), mode));
                ui.label("Created by Mavs");
            });
        });
}

/// Shows progress while public configuration and anonymous authentication initialize.
fn boot_screen(ui: &mut egui::Ui) {
    ui.spinner();
    ui.label("Loading menu and restoring your player identity…");
}

/// Draws top-level actions required by the multiplayer product flow.
fn main_screen(ui: &mut egui::Ui, next_state: &mut NextState<AppState>) {
    menu_button(ui, "New Game", || next_state.set(AppState::CreateGame));
    menu_button(ui, "Resume / Join Game", || next_state.set(AppState::MultiPlayerMenu));
    #[cfg(debug_assertions)]
    menu_button(ui, "Local Practice", || next_state.set(AppState::SinglePlayerMenu));
    menu_button(ui, "Settings", || next_state.set(AppState::Settings));
    #[cfg(not(target_arch = "wasm32"))]
    menu_button(ui, "Quit", || std::process::exit(0));
}

/// Draws join, resume, and replacement-device recovery choices.
fn multiplayer_screen(ui: &mut egui::Ui, next_state: &mut NextState<AppState>) {
    menu_button(ui, "Join Game", || next_state.set(AppState::JoinGame));
    menu_button(ui, "Resume Game", || next_state.set(AppState::ResumeGame));
    menu_button(ui, "Recover Player", || next_state.set(AppState::RecoverPlayer));
    menu_button(ui, "Back", || next_state.set(AppState::MainMenu));
}

/// Collects creator name, exact 2..=4 capacity, and existing gameplay settings.
fn create_screen(
    ui: &mut egui::Ui,
    form: &mut MultiplayerForm,
    settings: &mut Settings,
    busy: bool,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    form_fields(ui, form, false);
    centered_row(ui, |ui| {
        ui.label("Players");
        for count in 2..=4 {
            ui.selectable_value(&mut form.player_count, count, count.to_string());
        }
    });
    form_slider(ui, &mut settings.n_planets, 5..=20, "Planets per player");
    form_slider(ui, &mut settings.p_colonizable, 1..=100, "Colonizable %");
    form_slider(ui, &mut settings.p_moons, 0..=100, "Moons %");
    if ui
        .add_enabled(!busy && valid_name(&form.display_name), egui::Button::new("Create Game"))
        .clicked()
    {
        requests.write(MultiplayerRequest::CreateGame {
            display_name: form.display_name.clone(),
            rules: GameRules {
                planets_per_player: settings.n_planets,
                colonizable_percent: settings.p_colonizable,
                moons_percent: settings.p_moons,
                player_count: form.player_count,
            },
        });
    }
    back_button(ui, next_state, AppState::MainMenu);
}

/// Collects the share code and display name for a new slot or identity reconnect.
fn join_screen(
    ui: &mut egui::Ui,
    form: &mut MultiplayerForm,
    busy: bool,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    form_fields(ui, form, true);
    if ui
        .add_enabled(
            !busy && valid_name(&form.display_name) && valid_game_code(&form.game_code),
            egui::Button::new("Join"),
        )
        .clicked()
    {
        requests.write(MultiplayerRequest::JoinGame {
            display_name: form.display_name.clone(),
            code: form.game_code.clone(),
        });
    }
    back_button(ui, next_state, AppState::MultiPlayerMenu);
}

/// Collects the two codes needed to rotate a player identity and recovery secret.
fn recovery_screen(
    ui: &mut egui::Ui,
    form: &mut MultiplayerForm,
    busy: bool,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    ui.label("Game code");
    form_text_edit(ui, &mut form.game_code, false);
    ui.label("Recovery code");
    form_text_edit(ui, &mut form.recovery_code, true);
    if ui
        .add_enabled(
            !busy && valid_game_code(&form.game_code) && !form.recovery_code.trim().is_empty(),
            egui::Button::new("Recover Player"),
        )
        .clicked()
    {
        requests.write(MultiplayerRequest::RecoverPlayer {
            code: form.game_code.clone(),
            recovery_code: form.recovery_code.clone(),
        });
    }
    back_button(ui, next_state, AppState::MultiPlayerMenu);
}

/// Lists authenticated memberships and lets a user refresh or load one record.
fn resume_screen(
    ui: &mut egui::Ui,
    session: &MultiplayerSession,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    if session.games.is_empty() {
        ui.label("No games are associated with this browser or installation yet.");
    }
    for game in &session.games {
        let label = format!(
            "{} · turn {} · {}/{} players · {:?}",
            game.code.0, game.turn, game.player_count, game.max_players, game.status
        );
        if ui.add_enabled(!session.busy, egui::Button::new(label)).clicked() {
            requests.write(MultiplayerRequest::ResumeGame(game.id.clone()));
        }
    }
    if ui.add_enabled(!session.busy, egui::Button::new("Refresh")).clicked() {
        requests.write(MultiplayerRequest::RefreshGames);
    }
    back_button(ui, next_state, AppState::MultiPlayerMenu);
}

/// Displays share/recovery codes, exact capacity, members, and creator start controls.
fn lobby_screen(
    ui: &mut egui::Ui,
    session: &MultiplayerSession,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    let Some(game) = &session.active_game else {
        ui.label("No lobby is selected.");
        back_button(ui, next_state, AppState::MultiPlayerMenu);
        return;
    };
    ui.heading(format!("Game code: {}", game.code.0));
    if let Some(code) = &session.issued_recovery_code {
        ui.horizontal_wrapped(|ui| {
            ui.label(format!("Recovery code: {code}"));
            if ui.small_button("Copy").clicked() {
                ui.ctx().copy_text(code.clone());
            }
        });
        ui.label("Store this code safely. It is shown in plaintext only on your client.");
    }
    ui.separator();
    ui.label(format!("Players: {}/{}", game.members.len(), game.max_players));
    for member in &game.members {
        let suffix = if member.is_creator {
            " (creator)"
        } else {
            ""
        };
        ui.label(format!("{}. {}{suffix}", member.player_id, member.display_name));
    }
    let can_start = session.membership.as_ref().is_some_and(|member| member.is_creator)
        && game.members.len() == usize::from(game.max_players);
    if ui.add_enabled(can_start && !session.busy, egui::Button::new("Start Game")).clicked() {
        requests.write(MultiplayerRequest::StartGame);
    }
    if ui.add_enabled(!session.busy, egui::Button::new("Refresh Lobby")).clicked() {
        requests.write(MultiplayerRequest::ResumeGame(game.id.clone()));
    }
    if ui.button("Leave Lobby").clicked() {
        requests.write(MultiplayerRequest::LeaveGame);
    }
}

/// Shows loading progress while the canonical state and deferred world assets are prepared.
fn loading_screen(ui: &mut egui::Ui, session: &MultiplayerSession) {
    ui.spinner();
    let game = session.active_game.as_ref().map(|game| game.code.as_str()).unwrap_or("game");
    ui.label(format!("Loading {game} and gameplay assets…"));
}

/// Edits preferences that do not affect deterministic turn resolution.
fn settings_screen(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    next_state: &mut NextState<AppState>,
) {
    centered_row(ui, |ui| {
        ui.label("Audio");
        ui.selectable_value(&mut settings.audio, AudioState::Mute, "Muted");
        ui.selectable_value(&mut settings.audio, AudioState::NoMusic, "Effects");
        ui.selectable_value(&mut settings.audio, AudioState::Sound, "All sound");
    });
    ui.checkbox(&mut settings.show_cells, "Show map cells");
    ui.checkbox(&mut settings.show_hover, "Show hover information");
    back_button(ui, next_state, AppState::MainMenu);
}

/// Draws pause/settings/end-game overlays while the map remains loaded.
pub fn draw_game_overlay(
    mut contexts: EguiContexts,
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    mut settings: ResMut<Settings>,
    session: Res<MultiplayerSession>,
    mut requests: MessageWriter<MultiplayerRequest>,
) {
    if matches!(*game_state.get(), GameState::Playing | GameState::Combat) {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    egui::Window::new(match game_state.get() {
        GameState::GameMenu => "Game",
        GameState::Settings => "Settings",
        GameState::EndGame => "Game finished",
        GameState::CombatMenu => return,
        GameState::Playing | GameState::Combat => return,
    })
    .collapsible(false)
    .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
    .show(context, |ui| match game_state.get() {
        GameState::GameMenu => {
            if ui.button("Continue").clicked() {
                next_game_state.set(GameState::Playing);
            }
            if session.has_active_game() && ui.button("Save Game").clicked() {
                requests.write(MultiplayerRequest::SaveGame);
            }
            if ui.button("Settings").clicked() {
                next_game_state.set(GameState::Settings);
            }
            if ui.button("Return to Main Menu").clicked() {
                requests.write(MultiplayerRequest::LeaveGame);
                next_game_state.set(GameState::Playing);
                next_app_state.set(AppState::MainMenu);
            }
        },
        GameState::Settings => {
            ui.checkbox(&mut settings.show_cells, "Show map cells");
            ui.checkbox(&mut settings.show_hover, "Show hover information");
            if ui.button("Back").clicked() {
                next_game_state.set(GameState::GameMenu);
            }
        },
        GameState::EndGame => {
            if ui.button("Spectate").clicked() {
                next_game_state.set(GameState::Playing);
            }
            if ui.button("Return to Main Menu").clicked() {
                requests.write(MultiplayerRequest::LeaveGame);
                next_game_state.set(GameState::Playing);
                next_app_state.set(AppState::MainMenu);
            }
        },
        GameState::Playing | GameState::CombatMenu | GameState::Combat => {},
    });
}

/// Stops terminal-state audio when the user exits its overlay.
pub fn exit_end_game(mut mute_audio_msg: MessageWriter<MuteAudioMsg>) {
    mute_audio_msg.write(MuteAudioMsg);
}

/// Draws a consistently sized main navigation button.
fn menu_button(ui: &mut egui::Ui, label: &str, on_click: impl FnOnce()) {
    let viewport = ui.ctx().viewport_rect().size();
    let width = (viewport.x * 0.25).clamp(260.0, 420.0);
    let height = (viewport.y * 0.09).clamp(52.0, 84.0);
    let text_size = (viewport.y / 30.0).clamp(20.0, 32.0);
    if ui
        .add_sized([width, height], egui::Button::new(egui::RichText::new(label).size(text_size)))
        .clicked()
    {
        on_click();
    }
    ui.add_space((viewport.y * 0.01).clamp(6.0, 10.0));
}

/// Restores the original flat Stellarion menu interaction palette inside egui.
fn apply_menu_style(ui: &mut egui::Ui) {
    let visuals = &mut ui.style_mut().visuals;
    visuals.button_frame = true;
    let states = [
        (&mut visuals.widgets.inactive, NORMAL_BUTTON_COLOR.to_color32()),
        (&mut visuals.widgets.hovered, HOVERED_BUTTON_COLOR.to_color32()),
        (&mut visuals.widgets.active, PRESSED_BUTTON_COLOR.to_color32()),
        (&mut visuals.widgets.open, HOVERED_BUTTON_COLOR.to_color32()),
        (&mut visuals.widgets.noninteractive, DISABLED_BUTTON_COLOR.to_color32()),
    ];
    for (state, color) in states {
        state.bg_fill = color;
        state.weak_bg_fill = color;
        state.bg_stroke = egui::Stroke::NONE;
        state.corner_radius = egui::CornerRadius::ZERO;
        state.expansion = 0.0;
    }
}

/// Draws shared display-name and optional game-code fields.
fn form_fields(ui: &mut egui::Ui, form: &mut MultiplayerForm, include_code: bool) {
    ui.label("Player name");
    form_text_edit(ui, &mut form.display_name, false);
    if include_code {
        ui.label("Game code");
        form_text_edit(ui, &mut form.game_code, false);
    }
}

/// Draws a centered form row without letting horizontal layouts span the viewport.
fn centered_row(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    ui.allocate_ui_with_layout(
        egui::vec2(ui.available_width(), 32.0),
        egui::Layout::left_to_right(egui::Align::Center).with_main_align(egui::Align::Center),
        add_contents,
    );
}

/// Keeps text inputs readable and centered across desktop and portrait layouts.
fn form_text_edit(ui: &mut egui::Ui, value: &mut String, password: bool) {
    let width = ui.available_width().min(360.0);
    let editor = egui::TextEdit::singleline(value).password(password);
    ui.add_sized([width, 32.0], editor);
}

/// Keeps settings sliders inside the same centered form column as text inputs.
fn form_slider(
    ui: &mut egui::Ui,
    value: &mut usize,
    range: std::ops::RangeInclusive<usize>,
    label: &str,
) {
    let width = ui.available_width().min(520.0);
    ui.add_sized([width, 32.0], egui::Slider::new(value, range).text(label));
}

/// Draws a simple state-navigation back button.
fn back_button(ui: &mut egui::Ui, next_state: &mut NextState<AppState>, target: AppState) {
    if ui.button("Back").clicked() {
        next_state.set(target);
    }
}

/// Validates the backend's documented display-name boundary before enabling a request.
fn valid_name(value: &str) -> bool {
    (1..=32).contains(&value.trim().chars().count())
}

/// Validates the fixed-width share-code form before a backend round trip.
fn valid_game_code(value: &str) -> bool {
    value.trim().chars().filter(|character| *character != '-').count() == 6
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    /// Wide viewports crop the top and bottom instead of exposing empty side bars.
    fn cover_crop_fills_widescreen() {
        let rect = cover_source_rect(Vec2::new(1536.0, 1024.0), Vec2::new(1600.0, 900.0));
        assert_eq!(rect.min, Vec2::new(0.0, 80.0));
        assert_eq!(rect.max, Vec2::new(1536.0, 944.0));
    }

    #[test]
    /// Tall viewports crop both horizontal edges while preserving source proportions.
    fn cover_crop_fills_portrait() {
        let rect = cover_source_rect(Vec2::new(1536.0, 1024.0), Vec2::new(900.0, 1600.0));
        assert_eq!(rect.min, Vec2::new(480.0, 0.0));
        assert_eq!(rect.max, Vec2::new(1056.0, 1024.0));
    }
}
