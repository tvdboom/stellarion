//! Egui-driven native/WASM menus for creation, joining, recovery, resume, and lobby flows.

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};

use crate::core::assets::WorldAssets;
use crate::core::audio::{set_ui_sound, ChangeAudioMsg, MuteAudioMsg, SoundEffect};
use crate::core::constants::{HOVERED_BUTTON_COLOR, NORMAL_BUTTON_COLOR, PRESSED_BUTTON_COLOR};
use crate::core::menu::buttons::MenuCmp;
use crate::core::player::{PlayerColor, PLAYER_COLOR_PALETTE};
use crate::core::settings::Settings;
use crate::core::simulation::{GameRules, MatchStatus, MAX_MULTIPLAYER_PLAYERS};
use crate::core::states::{AppState, AudioState, GameState};
use crate::multiplayer::client::{
    ConnectionIndicator, MultiplayerForm, MultiplayerRequest, MultiplayerSession,
};
use crate::multiplayer::model::{GameRecord, GameSummary};
use crate::utils::ToColor32;
use crate::TITLE;

const MAIN_MENU_BUTTON_HEIGHT: f32 = 55.0;
const MAIN_MENU_ACTION_COUNT: usize =
    4 + cfg!(debug_assertions) as usize + cfg!(not(target_arch = "wasm32")) as usize;

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
    window: Single<&Window, With<PrimaryWindow>>,
    app_state: Res<State<AppState>>,
    mut next_state: ResMut<NextState<AppState>>,
    mut form: ResMut<MultiplayerForm>,
    session: Res<MultiplayerSession>,
    connection_indicator: Res<ConnectionIndicator>,
    mut settings: ResMut<Settings>,
    mut requests: MessageWriter<MultiplayerRequest>,
    mut change_audio: MessageWriter<ChangeAudioMsg>,
    mut refreshing_games: Local<bool>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let menu_size = egui::vec2(window.width(), window.height());
    if !session.busy {
        *refreshing_games = false;
    }
    let content_width =
        (menu_size.x * 0.4).clamp(320.0, 640.0).min((menu_size.x - 32.0).max(240.0));
    let is_primary_navigation = *app_state.get() == AppState::MainMenu;
    let (content_pivot, content_y) = if is_primary_navigation {
        (egui::Align2::CENTER_TOP, main_menu_top(context, menu_size))
    } else if matches!(
        *app_state.get(),
        AppState::JoinGame
            | AppState::ResumeGame
            | AppState::RecoverPlayer
            | AppState::Lobby
            | AppState::SinglePlayerMenu
    ) {
        // Leave room for the connection/credit footer on smaller windows.
        (egui::Align2::CENTER_CENTER, (menu_size.y - 96.0).max(0.0) * 0.5)
    } else {
        (egui::Align2::CENTER_CENTER, menu_size.y * 0.5)
    };
    egui::Area::new(egui::Id::new(("stellarion_menu_content", *app_state.get())))
        .pivot(content_pivot)
        .fixed_pos(egui::pos2(menu_size.x * 0.5, content_y))
        .constrain(false)
        .show(context, |ui| {
            apply_menu_style(ui);
            ui.set_width(content_width);
            ui.vertical_centered(|ui| {
                match app_state.get() {
                    AppState::Boot => boot_screen(ui),
                    AppState::MainMenu => main_screen(
                        ui,
                        session.menu_error.as_deref(),
                        &mut requests,
                        &mut next_state,
                    ),
                    AppState::CreateGame => create_screen(
                        ui,
                        &mut form,
                        &mut settings,
                        session.busy,
                        &mut requests,
                        &mut next_state,
                    ),
                    AppState::JoinGame => join_screen(
                        ui,
                        &mut form,
                        session.busy,
                        session.menu_error.as_deref(),
                        &mut requests,
                        &mut next_state,
                    ),
                    AppState::RecoverPlayer => recovery_screen(
                        ui,
                        &mut form,
                        session.busy,
                        session.menu_error.as_deref(),
                        &mut requests,
                        &mut next_state,
                    ),
                    AppState::ResumeGame => resume_screen(
                        ui,
                        &session,
                        &mut refreshing_games,
                        &mut requests,
                        &mut next_state,
                    ),
                    AppState::Lobby => lobby_screen(ui, &session, &mut requests, &mut next_state),
                    AppState::Settings => {
                        settings_screen(ui, &mut settings, &mut change_audio, &mut next_state)
                    },
                    AppState::LoadingGame => loading_screen(ui, &session),
                    AppState::SinglePlayerMenu => {
                        #[cfg(debug_assertions)]
                        local_practice_screen(
                            ui,
                            &mut form,
                            &mut settings,
                            session.busy,
                            &mut requests,
                            &mut next_state,
                        );
                        #[cfg(not(debug_assertions))]
                        next_state.set(AppState::MainMenu);
                    },
                    AppState::Game => {},
                }
                if let Some(error) = session.menu_error.as_ref().filter(|_| {
                    !matches!(
                        *app_state.get(),
                        AppState::Boot
                            | AppState::MainMenu
                            | AppState::JoinGame
                            | AppState::RecoverPlayer
                            | AppState::Lobby
                    )
                }) {
                    ui.add_space(12.0);
                    menu_error_panel(ui, "Unable to complete action", error);
                }
            });
        });

    if *app_state.get() == AppState::Boot {
        return;
    }

    if is_primary_navigation {
        egui::Area::new("stellarion_main_title".into())
            .pivot(egui::Align2::CENTER_CENTER)
            .fixed_pos(egui::pos2(menu_size.x * 0.5, menu_size.y * 0.17))
            .constrain(false)
            .interactable(false)
            .show(context, |ui| {
                ui.heading(
                    egui::RichText::new(TITLE).size((menu_size.y * 0.11).clamp(68.0, 104.0)),
                );
            });
    }

    egui::Area::new("stellarion_menu_footer".into())
        .pivot(egui::Align2::RIGHT_BOTTOM)
        .fixed_pos(egui::pos2(menu_size.x - 24.0, menu_size.y - 18.0))
        .constrain(false)
        .show(context, |ui| {
            let mode = if session.local_practice {
                " · local practice"
            } else if session.mock_backend {
                " · mock backend"
            } else {
                ""
            };
            ui.set_min_width(220.0);
            ui.with_layout(egui::Layout::top_down(egui::Align::RIGHT), |ui| {
                ui.label(format!("{}{}", connection_indicator.status.label(), mode));
                ui.label("Created by Mavs");
            });
        });
}

/// Moves the actions up when needed, while preserving space below the measured title.
fn main_menu_top(context: &egui::Context, viewport: egui::Vec2) -> f32 {
    let title_height = context.fonts_mut(|fonts| {
        fonts
            .layout_no_wrap(
                TITLE.to_string(),
                egui::FontId::proportional((viewport.y * 0.11).clamp(68.0, 104.0)),
                egui::Color32::WHITE,
            )
            .size()
            .y
    });
    let title_bottom = viewport.y * 0.17 + title_height * 0.5;
    let count = MAIN_MENU_ACTION_COUNT as f32;
    let gap = context.global_style().spacing.item_spacing.y + 5.4;
    let actions_height = MAIN_MENU_BUTTON_HEIGHT * count + gap * (count - 1.0);
    (viewport.y * 0.365).min(viewport.y - 96.0 - actions_height).max(title_bottom + 24.0)
}

/// Shows progress while public configuration and anonymous authentication initialize.
fn boot_screen(ui: &mut egui::Ui) {
    let color = ui.visuals().text_color();
    let label =
        ui.painter().layout_no_wrap("Loading".to_string(), egui::FontId::proportional(40.0), color);
    let spinner_size = 24.0;
    let gap = 12.0;
    let size = egui::vec2(spinner_size + gap + label.size().x, spinner_size.max(label.size().y));
    let (rect, _) = ui.allocate_exact_size(size, egui::Sense::hover());
    let spinner_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + spinner_size * 0.5, rect.center().y),
        egui::Vec2::splat(spinner_size),
    );
    ui.put(spinner_rect, egui::Spinner::new().size(spinner_size));
    ui.painter().galley(
        egui::pos2(rect.left() + spinner_size + gap, rect.center().y - label.size().y * 0.5),
        label,
        color,
    );
}

/// Draws top-level actions required by the multiplayer product flow.
fn main_screen(
    ui: &mut egui::Ui,
    error: Option<&str>,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    // Keep every action and any feedback reachable above the footer on short windows.
    let available_height =
        (ui.ctx().content_rect().bottom() - ui.next_widget_position().y - 96.0).max(80.0);
    // An Area remembers its previous content height. Expand the parent before the
    // scroll area measures it, including after a small window grows again.
    ui.set_max_height(available_height);
    let count = MAIN_MENU_ACTION_COUNT as f32;
    let preferred_gap = ui.spacing().item_spacing.y + 5.4;
    let gap = preferred_gap
        .min(((available_height - count * MAIN_MENU_BUTTON_HEIGHT) / (count - 1.0)).max(8.0));
    let button_height =
        ((available_height - gap * (count - 1.0)) / count).clamp(44.0, MAIN_MENU_BUTTON_HEIGHT);
    let button_size = egui::vec2(308.0_f32.min(ui.available_width()), button_height);
    egui::ScrollArea::vertical()
        .id_salt("stellarion_main_actions")
        .auto_shrink([false, true])
        .max_height(available_height)
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = gap;
            ui.vertical_centered(|ui| {
                let button = |ui: &mut egui::Ui, label: &str| {
                    menu_button_widget(ui, label, true, button_size, 21.6)
                };
                if button(ui, "New Game") {
                    next_state.set(AppState::CreateGame);
                }
                if button(ui, "Join Game") {
                    next_state.set(AppState::JoinGame);
                }
                if button(ui, "Resume Game") {
                    requests.write(MultiplayerRequest::RefreshGames);
                    next_state.set(AppState::ResumeGame);
                }
                #[cfg(debug_assertions)]
                if button(ui, "Local Practice") {
                    next_state.set(AppState::SinglePlayerMenu);
                }
                if button(ui, "Settings") {
                    next_state.set(AppState::Settings);
                }
                #[cfg(not(target_arch = "wasm32"))]
                if button(ui, "Quit") {
                    std::process::exit(0);
                }
                if let Some(error) = error {
                    ui.add_space(12.0);
                    menu_error_panel(ui, "Unable to complete action", error);
                }
            });
        });
}

/// Configures and starts an isolated one-player deterministic match in debug builds.
#[cfg(debug_assertions)]
fn local_practice_screen(
    ui: &mut egui::Ui,
    form: &mut MultiplayerForm,
    settings: &mut Settings,
    busy: bool,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    let enter_pressed = menu_submit_pressed(ui);
    // Bound the full setup, including the new color row, above the menu footer.
    ui.set_max_height((ui.ctx().content_rect().height() - 128.0).max(160.0));
    ui.heading(egui::RichText::new("Local Practice").size(36.0));
    ui.add_space(28.0);
    let (back_clicked, start_clicked) = egui::ScrollArea::vertical()
        .id_salt("local_practice_setup")
        .auto_shrink([false, true])
        .max_height(ui.available_height())
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_enabled_ui(!busy, |ui| {
                    practice_color_picker(ui, &mut form.practice_color);
                    map_rule_rows(ui, settings);
                });
                ui.add_space(40.0);
                local_practice_action_buttons(ui, !busy)
            })
            .inner
        })
        .inner;
    if back_clicked {
        next_state.set(AppState::MainMenu);
    } else if !busy && (start_clicked || enter_pressed) {
        requests.write(MultiplayerRequest::StartLocalPractice {
            rules: GameRules {
                planets_per_player: settings.n_planets,
                colonizable_percent: settings.p_colonizable,
                moons_percent: settings.p_moons,
                player_count: 1,
                practice_mode: true,
            },
            player_color: form.practice_color,
        });
    }
}

/// Shows every empire color and keeps the selected swatch outlined.
#[cfg(debug_assertions)]
fn practice_color_picker(ui: &mut egui::Ui, selected: &mut PlayerColor) {
    ui.label(egui::RichText::new("Player color").size(20.0).strong());
    ui.add_space(3.0);
    let gap = 6.0;
    let row_width = PLAYER_COLOR_PALETTE.len() as f32 * (34.0 + gap) - gap;
    ui.allocate_ui_with_layout(
        egui::vec2(row_width, 34.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for color in PLAYER_COLOR_PALETTE {
                if player_color_swatch(ui, color, color == *selected) {
                    *selected = color;
                }
            }
        },
    );
    ui.add_space(6.0);
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
    let enter_pressed = menu_submit_pressed(ui);
    ui.heading(egui::RichText::new("Create Game").size(36.0));
    ui.add_space(28.0);
    player_name_field(ui, &mut form.display_name, 420.0);
    map_rule_rows(ui, settings);
    ui.add_space(40.0);
    let can_create = !busy && valid_name(&form.display_name);
    let (back_clicked, create_clicked) =
        menu_button_pair(ui, "Back", true, "Create Game", can_create, false);
    if back_clicked {
        next_state.set(AppState::MainMenu);
    } else if can_create && (create_clicked || enter_pressed) {
        requests.write(MultiplayerRequest::CreateGame {
            display_name: form.display_name.clone(),
            rules: GameRules {
                planets_per_player: settings.n_planets,
                colonizable_percent: settings.p_colonizable,
                moons_percent: settings.p_moons,
                player_count: MAX_MULTIPLAYER_PLAYERS,
                practice_mode: false,
            },
        });
    }
}

/// Collects the share code, asking for a player name only when none has been saved.
fn join_screen(
    ui: &mut egui::Ui,
    form: &mut MultiplayerForm,
    busy: bool,
    error: Option<&str>,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    let enter_pressed = menu_submit_pressed(ui);
    ui.spacing_mut().item_spacing.y = 8.0;
    let saved_name = form.saved_display_name.as_deref().filter(|name| valid_name(name));
    let (button_size, _, button_spacing) = menu_button_pair_metrics(ui);
    // Keep navigation visible while the form and a long error scroll on short viewports.
    let form_height =
        (ui.ctx().content_rect().height() - 128.0 - button_size.y - button_spacing).max(80.0);
    egui::ScrollArea::vertical()
        .id_salt("stellarion_join_form")
        .auto_shrink([false, true])
        .max_height(form_height)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(egui::RichText::new("Join Game").size(36.0));
                ui.add_space(28.0);
                if saved_name.is_none() {
                    player_name_field(ui, &mut form.display_name, 340.0);
                }
                ui.label(egui::RichText::new("Game code").size(20.0).strong());
                form_text_edit_width(ui, &mut form.game_code, false, 340.0);
                if let Some(error) = error {
                    ui.add_space(12.0);
                    menu_error_panel(ui, "Unable to join game", error);
                }
            });
        });
    let display_name = saved_name.unwrap_or(&form.display_name);
    let can_join = !busy && valid_name(display_name) && valid_game_code(&form.game_code);
    ui.add_space(16.0);
    let (back_clicked, join_clicked) = menu_button_pair(ui, "Back", true, "Join", can_join, false);
    if back_clicked {
        next_state.set(AppState::MainMenu);
    } else if can_join && (join_clicked || enter_pressed) {
        requests.write(MultiplayerRequest::JoinGame {
            display_name: display_name.to_string(),
            code: form.game_code.clone(),
        });
    }
}

/// Keeps menu failures readable over the artwork without a dismissal timer.
fn menu_error_panel(ui: &mut egui::Ui, title: &str, error: &str) -> egui::Response {
    let (button_size, _, _) = menu_button_pair_metrics(ui);
    let width = button_size.x * 2.0 + 12.0;
    menu_error_panel_width(ui, title, error, width)
}

/// Matches an error container to the outer width of the form it describes.
fn menu_error_panel_width(
    ui: &mut egui::Ui,
    title: &str,
    error: &str,
    width: f32,
) -> egui::Response {
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(67, 23, 32, 232))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(240, 112, 123, 150)))
        .corner_radius(8.0)
        .inner_margin(egui::Margin::symmetric(16, 12));
    frame
        .show(ui, |ui| {
            ui.set_width((width - frame.total_margin().sum().x).max(1.0));
            ui.with_layout(egui::Layout::top_down(egui::Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 6.0;
                ui.label(
                    egui::RichText::new(title)
                        .size(17.0)
                        .strong()
                        .color(egui::Color32::from_rgb(255, 171, 179)),
                );
                ui.add(
                    egui::Label::new(
                        egui::RichText::new(error)
                            .size(16.0)
                            .color(egui::Color32::from_rgb(255, 230, 232)),
                    )
                    .wrap(),
                );
            });
        })
        .response
}

/// Reveals a newly reported error once while preserving subsequent manual scrolling.
fn menu_error_changed(ui: &mut egui::Ui, key: &str, error: Option<&str>) -> bool {
    let id = ui.make_persistent_id(key);
    ui.data_mut(|data| {
        let previous = data.get_temp_mut_or_default::<String>(id);
        let current = error.unwrap_or_default();
        if previous == current {
            return false;
        }
        previous.clear();
        previous.push_str(current);
        error.is_some()
    })
}

/// Opens an unlisted saved game using its shared code and the player's private code.
fn recovery_screen(
    ui: &mut egui::Ui,
    form: &mut MultiplayerForm,
    busy: bool,
    error: Option<&str>,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    let enter_pressed = menu_submit_pressed(ui);
    ui.spacing_mut().item_spacing.y = 8.0;
    let scroll_to_error = menu_error_changed(ui, "recovery_error", error);
    let (button_size, _, button_spacing) = menu_button_pair_metrics(ui);
    let form_height =
        (ui.ctx().content_rect().height() - 132.0 - button_size.y - button_spacing).max(80.0);
    egui::ScrollArea::vertical()
        .id_salt("stellarion_recovery_form")
        .auto_shrink([false, true])
        .max_height(form_height)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(egui::RichText::new("Recover Game").size(36.0));
                ui.add_space(18.0);
                let recovery_field = ui.add_enabled_ui(!busy, |ui| {
                    recovery_code_field(
                        ui,
                        "Game code",
                        &mut form.game_code,
                        false,
                        "Enter game code",
                    );
                    ui.add_space(12.0);
                    recovery_code_field(
                        ui,
                        "Recovery code",
                        &mut form.recovery_code,
                        true,
                        "Enter recovery code",
                    )
                });
                if let Some(error) = error {
                    ui.add_space(12.0);
                    let panel = menu_error_panel_width(
                        ui,
                        "Unable to recover game",
                        error,
                        recovery_field.inner.rect.width(),
                    );
                    if scroll_to_error {
                        panel.scroll_to_me(Some(egui::Align::Max));
                    }
                }
            });
        });
    let can_recover =
        !busy && valid_game_code(&form.game_code) && !form.recovery_code.trim().is_empty();
    ui.add_space(20.0);
    let (back_clicked, recover_clicked) =
        menu_button_pair(ui, "Back", !busy, "Recover Game", can_recover, busy);
    if back_clicked {
        next_state.set(AppState::ResumeGame);
    } else if can_recover && (recover_clicked || enter_pressed) {
        requests.write(MultiplayerRequest::RecoverPlayer {
            code: form.game_code.clone(),
            recovery_code: form.recovery_code.clone(),
        });
    }
}

/// Lists known games and offers code recovery when a game is missing on this device.
fn resume_screen(
    ui: &mut egui::Ui,
    session: &MultiplayerSession,
    refreshing: &mut bool,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    ui.spacing_mut().item_spacing.y = 8.0;
    ui.heading(egui::RichText::new("Resume Game").size(38.0).strong());
    ui.add_space(20.0);
    let mut games =
        session.games.iter().filter(|game| game.status != MatchStatus::Lobby).peekable();
    if games.peek().is_none() {
        // Keep the recovery actions visible when guidance wraps on small screens.
        egui::ScrollArea::vertical()
            .id_salt("stellarion_resume_guidance")
            .auto_shrink([false, true])
            .max_height((ui.ctx().content_rect().height() - 260.0).max(80.0))
            .show(ui, |ui| {
                resume_empty_state(ui);
                ui.add_space(12.0);
                resume_recovery_toast(ui);
            });
    } else {
        // Fit five cards when space permits, leaving room for the heading and actions.
        let list_height = (ui.ctx().content_rect().height() - 260.0).clamp(94.0, 384.0);
        egui::ScrollArea::vertical()
            .id_salt("stellarion_resume_games")
            .auto_shrink([false, true])
            .max_height(list_height)
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.spacing_mut().item_spacing.y = 0.0;
                while let Some(game) = games.next() {
                    if resume_game_card(ui, game, !session.busy) {
                        requests.write(MultiplayerRequest::ResumeGame(game.id.clone()));
                    }
                    if games.peek().is_some() {
                        ui.add_space(6.0);
                    }
                }
            });
    }
    ui.add_space(16.0);
    let (back_clicked, refresh_clicked, recover_clicked) =
        resume_action_buttons(ui, session.busy, *refreshing);
    if back_clicked {
        next_state.set(AppState::MainMenu);
    }
    if refresh_clicked {
        *refreshing = true;
        requests.write(MultiplayerRequest::RefreshGames);
    }
    if recover_clicked {
        next_state.set(AppState::RecoverPlayer);
    }
}

/// Fits navigation, refresh, and recovery on one row, including narrow windows.
fn resume_action_buttons(ui: &mut egui::Ui, busy: bool, refreshing: bool) -> (bool, bool, bool) {
    let (size, text_size, spacing) = menu_button_metrics(ui);
    let gap = 12.0;
    let width = size.x.min(((ui.available_width() - gap * 2.0) / 3.0).max(1.0));
    let labels = ["Back", "Refresh", "Recover Game"];
    let measured_width = labels
        .iter()
        .map(|label| {
            ui.painter()
                .layout_no_wrap(
                    (*label).to_string(),
                    egui::FontId::proportional(text_size),
                    egui::Color32::WHITE,
                )
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);
    let text_size = text_size * ((width - 24.0) / measured_width).clamp(0.1, 1.0);
    let clicked = ui
        .allocate_ui_with_layout(
            egui::vec2(width * 3.0 + gap * 2.0, size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = gap;
                let button_size = egui::vec2(width, size.y);
                let back =
                    menu_action_button(ui, labels[0], true, false, false, button_size, text_size);
                let refresh = ui.scope(|ui| {
                    menu_action_button(
                        ui,
                        labels[1],
                        !busy && !refreshing,
                        false,
                        false,
                        button_size,
                        text_size,
                    )
                });
                if refreshing {
                    // Paint into the existing padding so progress never changes the layout.
                    let label_width = ui
                        .painter()
                        .layout_no_wrap(
                            labels[1].to_string(),
                            egui::FontId::proportional(text_size),
                            egui::Color32::WHITE,
                        )
                        .size()
                        .x;
                    let padding = (button_size.x - label_width) * 0.5;
                    let spinner_size = (padding - 8.0).clamp(4.0, 16.0);
                    let spinner_rect = egui::Rect::from_center_size(
                        egui::pos2(
                            refresh.response.rect.left() + padding * 0.5,
                            refresh.response.rect.center().y,
                        ),
                        egui::Vec2::splat(spinner_size),
                    );
                    egui::Spinner::new().paint_at(ui, spinner_rect);
                }
                let recover =
                    menu_action_button(ui, labels[2], !busy, false, false, button_size, text_size);
                (back, refresh.inner, recover)
            },
        )
        .inner;
    ui.add_space(spacing);
    clicked
}

/// Draws a saved game with the caller's identity, using an extra row when needed.
fn resume_game_card(ui: &mut egui::Ui, game: &GameSummary, enabled: bool) -> bool {
    let secondary = if enabled {
        egui::Color32::from_rgb(166, 188, 211)
    } else {
        egui::Color32::from_rgb(112, 124, 137)
    };
    let details_font = egui::FontId::proportional(14.0);
    let details = ui.painter().layout_no_wrap(
        format!(
            "Turn {}   ·   {} player{}",
            game.turn,
            game.player_count,
            if game.player_count == 1 {
                ""
            } else {
                "s"
            }
        ),
        details_font.clone(),
        secondary,
    );
    let content_width = (ui.available_width() - 55.0).max(1.0);
    let name =
        ui.painter().layout_no_wrap(game.display_name.clone(), details_font.clone(), secondary);
    let stacked = details.size().x + 36.0 + name.size().x > content_width;
    let name_offset = if stacked {
        18.0
    } else {
        details.size().x + 36.0
    };
    let mut name_job = egui::text::LayoutJob::simple_singleline(
        game.display_name.clone(),
        details_font.clone(),
        secondary,
    );
    name_job.wrap.max_width = (content_width - name_offset).max(1.0);
    name_job.wrap.max_rows = 1;
    name_job.wrap.break_anywhere = true;
    let name = ui.painter().layout_job(name_job);
    let size = egui::vec2(
        ui.available_width(),
        if stacked {
            94.0
        } else {
            72.0
        },
    );
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();
    let fill = if pressed {
        egui::Color32::from_rgba_unmultiplied(34, 61, 84, 248)
    } else if hovered {
        egui::Color32::from_rgba_unmultiplied(24, 42, 58, 246)
    } else {
        egui::Color32::from_rgba_unmultiplied(13, 22, 32, 238)
    };
    let border = if hovered {
        egui::Color32::from_rgba_unmultiplied(103, 196, 238, 190)
    } else {
        egui::Color32::from_rgba_unmultiplied(132, 177, 213, 88)
    };
    let foreground = if enabled {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(142, 153, 166)
    };

    ui.painter().rect(
        rect,
        egui::CornerRadius::same(7),
        fill,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    ui.painter().rect_filled(
        egui::Rect::from_min_max(
            rect.min + egui::vec2(0.0, 12.0),
            egui::pos2(rect.left() + 3.0, rect.bottom() - 12.0),
        ),
        2.0,
        if enabled {
            egui::Color32::from_rgb(86, 190, 232)
        } else {
            egui::Color32::from_rgb(74, 101, 116)
        },
    );

    let left = rect.left() + 17.0;
    ui.painter().text(
        egui::pos2(left, rect.top() + 13.0),
        egui::Align2::LEFT_TOP,
        game.code.as_str(),
        egui::FontId::proportional(19.0),
        foreground,
    );
    let details_y = rect.top() + 59.0 - details.size().y * 0.5;
    if !stacked {
        ui.painter().text(
            egui::pos2(left + details.size().x + 10.0, details_y),
            egui::Align2::CENTER_CENTER,
            "·",
            details_font,
            secondary,
        );
    }
    ui.painter().galley(egui::pos2(left, details_y - details.size().y * 0.5), details, secondary);
    let name_left = left + name_offset;
    let name_y = details_y
        + if stacked {
            22.0
        } else {
            0.0
        };
    let [red, green, blue] = game.player_color.rgb();
    let color = egui::Color32::from_rgb(red, green, blue);
    ui.painter().circle_filled(
        egui::pos2(name_left - 12.0, name_y),
        5.0,
        if enabled {
            color
        } else {
            color.gamma_multiply(0.5)
        },
    );
    ui.painter().galley(egui::pos2(name_left, name_y - name.size().y * 0.5), name, secondary);

    let (status, status_color) = resume_status_style(game.status, enabled);
    let status_right = rect.right() - 38.0;
    let status_rect = ui.painter().text(
        egui::pos2(status_right, rect.top() + 16.0),
        egui::Align2::RIGHT_TOP,
        status,
        egui::FontId::proportional(13.0),
        status_color,
    );
    ui.painter().circle_filled(
        egui::pos2(status_rect.left() - 10.0, status_rect.center().y),
        3.5,
        status_color,
    );

    let arrow_center = egui::pos2(rect.right() - 18.0, rect.center().y);
    let arrow_color = if hovered {
        egui::Color32::WHITE
    } else {
        secondary
    };
    ui.painter().line_segment(
        [arrow_center + egui::vec2(-2.0, -5.0), arrow_center + egui::vec2(3.0, 0.0)],
        egui::Stroke::new(1.8, arrow_color),
    );
    ui.painter().line_segment(
        [arrow_center + egui::vec2(3.0, 0.0), arrow_center + egui::vec2(-2.0, 5.0)],
        egui::Stroke::new(1.8, arrow_color),
    );

    let response = response.on_hover_text(format!("Playing as {}", game.display_name));
    if enabled {
        let clicked = response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        if clicked {
            mark_menu_click(ui);
        }
        clicked
    } else {
        false
    }
}

/// Returns the short status label and its restrained semantic accent color.
fn resume_status_style(status: MatchStatus, enabled: bool) -> (&'static str, egui::Color32) {
    if !enabled {
        return (
            match status {
                MatchStatus::Lobby => "Waiting for players",
                MatchStatus::Active => "In progress",
                MatchStatus::Finished => "Finished",
            },
            egui::Color32::from_rgb(112, 124, 137),
        );
    }
    match status {
        MatchStatus::Lobby => ("Waiting for players", egui::Color32::from_rgb(243, 190, 92)),
        MatchStatus::Active => ("In progress", egui::Color32::from_rgb(102, 224, 170)),
        MatchStatus::Finished => ("Finished", egui::Color32::from_rgb(167, 184, 202)),
    }
}

/// Shows a quiet placeholder when the signed-in device has no restorable games.
fn resume_empty_state(ui: &mut egui::Ui) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 72.0), egui::Sense::hover());
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(7),
        egui::Color32::from_rgba_unmultiplied(13, 22, 32, 218),
        egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(132, 177, 213, 70)),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "No games linked to this device",
        egui::FontId::proportional(15.0),
        egui::Color32::from_rgb(166, 188, 211),
    );
}

/// Places persistent recovery guidance in its own toast below the empty-state panel.
fn resume_recovery_toast(ui: &mut egui::Ui) {
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(13, 22, 32, 242))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(132, 177, 213, 70)))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::symmetric(18, 16));
    let width = (ui.available_width() - frame.total_margin().sum().x).max(1.0);
    frame.show(ui, |ui| {
        ui.set_width(width);
        ui.vertical_centered(|ui| {
            ui.add(
                egui::Label::new(
                    egui::RichText::new("Played in another window or on another device?")
                        .size(17.0)
                        .color(egui::Color32::from_rgb(225, 235, 247)),
                )
                .wrap(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new(
                        "Choose Recover Game, then enter the game code and your own private recovery code.",
                    )
                    .size(17.0)
                    .color(egui::Color32::from_rgb(225, 235, 247)),
                )
                .wrap(),
            );
            ui.add(
                egui::Label::new(
                    egui::RichText::new("Each player has a different recovery code.")
                        .size(16.0)
                        .color(egui::Color32::from_rgb(195, 214, 233)),
                )
                .wrap(),
            );
        });
    });
}

/// Displays access codes, current members, and creator start controls.
fn lobby_screen(
    ui: &mut egui::Ui,
    session: &MultiplayerSession,
    requests: &mut MessageWriter<MultiplayerRequest>,
    next_state: &mut NextState<AppState>,
) {
    let enter_pressed = menu_submit_pressed(ui);
    let Some(game) = &session.active_game else {
        ui.label("No lobby is selected.");
        back_button(ui, next_state, AppState::MainMenu);
        return;
    };

    let reconnecting = game.status == MatchStatus::Active && session.reconnect_lobby;
    ui.spacing_mut().item_spacing.y = 8.0;
    let scroll_to_error = menu_error_changed(ui, "lobby_error", session.menu_error.as_deref());
    let (button_size, _, button_spacing) = menu_button_pair_metrics(ui);
    let content_height =
        (ui.ctx().content_rect().height() - 128.0 - button_size.y - button_spacing).max(80.0);
    egui::ScrollArea::vertical()
        .id_salt("stellarion_lobby_content")
        .auto_shrink([false, true])
        .max_height(content_height)
        .show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.heading(
                    egui::RichText::new(if reconnecting {
                        "Resume Game"
                    } else {
                        "Game Lobby"
                    })
                    .size(36.0),
                );
                ui.add_space(20.0);
                lobby_code_card(ui, "Game code", game.code.as_str(), true);
                ui.add_space(10.0);
                if let Some(code) = &session.issued_recovery_code {
                    lobby_code_card(ui, "Recovery code", code, false);
                }
                ui.add_space(22.0);
                lobby_players_card(
                    ui,
                    game,
                    reconnecting,
                    session.membership.as_ref().map(|member| member.player_id),
                    session.busy,
                    requests,
                );
                let guidance = lobby_guidance(session);
                if !guidance.is_empty() {
                    ui.add_space(12.0);
                    ui.label(egui::RichText::new(guidance).size(16.0));
                }
                if let Some(error) = &session.menu_error {
                    ui.add_space(12.0);
                    let panel = menu_error_panel(ui, "Unable to continue game", error);
                    if scroll_to_error {
                        panel.scroll_to_me(Some(egui::Align::Max));
                    }
                }
            });
        });
    ui.add_space(16.0);

    let is_host = session.membership.as_ref().is_some_and(|member| member.is_creator);
    let can_continue = lobby_primary_enabled(session);
    let (leave_clicked, continue_clicked) = if is_host {
        menu_button_pair(
            ui,
            "Leave Lobby",
            true,
            if reconnecting {
                "Resume Game"
            } else {
                "Start Game"
            },
            can_continue,
            false,
        )
    } else {
        (lobby_leave_button(ui, true), false)
    };
    if leave_clicked {
        requests.write(MultiplayerRequest::LeaveGame);
    } else if can_continue && (continue_clicked || enter_pressed) {
        requests.write(if reconnecting {
            MultiplayerRequest::ResumeActiveGame
        } else {
            MultiplayerRequest::StartGame
        });
    }
}

/// Explains the next step for the local role, including a host who has not returned yet.
fn lobby_guidance(session: &MultiplayerSession) -> String {
    let Some(game) = &session.active_game else {
        return String::new();
    };
    let is_host = session.membership.as_ref().is_some_and(|member| member.is_creator);
    if game.status == MatchStatus::Lobby {
        return String::new();
    }
    if is_host {
        return if game.members.iter().all(|member| member.connected) {
            "Everyone is connected. Resume the saved game for all players.".to_string()
        } else {
            "Other players: open Resume Game in your window. If this game is missing, choose Recover Game with this game code and your own private recovery code.".to_string()
        };
    }
    let host = game.members.iter().find(|member| member.is_creator);
    let host_name = host.map_or("the host", |member| member.display_name.as_str());
    if host.is_some_and(|member| member.connected) {
        format!("Waiting for {host_name} to resume. You'll enter automatically when the host resumes. If they're in-game, ask them to open Game Lobby from the pause menu.")
    } else {
        format!("Waiting for {host_name} to reconnect. The host must open this game from Resume Game. You'll enter automatically when the host resumes.")
    }
}

/// Shares the host/readiness guard between the lobby button and its Enter shortcut.
fn lobby_primary_enabled(session: &MultiplayerSession) -> bool {
    let (Some(game), Some(member)) = (&session.active_game, &session.membership) else {
        return false;
    };
    !session.busy
        && member.is_creator
        && match game.status {
            MatchStatus::Lobby => game.members.len() >= 2,
            MatchStatus::Active => {
                session.reconnect_lobby && game.members.iter().all(|member| member.connected)
            },
            MatchStatus::Finished => false,
        }
}

/// Captures Enter before single-line fields or focused secondary buttons can consume it.
fn menu_submit_pressed(ui: &egui::Ui) -> bool {
    ui.input_mut(|input| take_menu_submit(&mut input.events))
}

/// Consumes plain Enter presses exactly once, ignoring held-key repeats and modified shortcuts.
fn take_menu_submit(events: &mut Vec<egui::Event>) -> bool {
    let mut pressed = false;
    events.retain(|event| {
        if let egui::Event::Key {
            key: egui::Key::Enter,
            pressed: true,
            repeat,
            modifiers,
            ..
        } = event
        {
            if modifiers.is_none() {
                pressed |= !repeat;
                return false;
            }
        }
        true
    });
    pressed
}

/// Shares the access-code container between code display and recovery inputs.
fn code_card_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(14, 22, 31, 232))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(130, 170, 215, 115)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(18, 14))
}

/// Labels an access-code card and keeps its explanation behind hover help.
fn code_card_heading(ui: &mut egui::Ui, label: &str, tooltip: &str, copy_value: Option<&str>) {
    ui.horizontal(|ui| {
        ui.label(
            egui::RichText::new(label.to_uppercase())
                .size(13.0)
                .strong()
                .color(egui::Color32::from_rgb(145, 184, 226)),
        );
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(value) = copy_value {
                copy_icon_button(ui, value, label);
            }
            code_info_icon(ui, tooltip);
        });
    });
}

/// Draws an editable recovery code in the same container used to share codes.
fn recovery_code_field(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut String,
    private: bool,
    hint: &str,
) -> egui::Response {
    let inner_width = (ui.available_width().min(560.0) - 36.0).max(1.0);
    let tooltip = if private {
        "Each player has a different private recovery code. Enter your own latest code, not another player's. After recovery, save the new code that replaces it. A connected player cannot be recovered in another window."
    } else {
        "Enter the shared code of the game you want to recover. Find it in your saved codes or ask another player in that game."
    };
    let panel = code_card_frame().show(ui, |ui| {
        ui.set_width(inner_width);
        code_card_heading(ui, label, tooltip, None);
        ui.add_space(8.0);
        ui.add_sized(
            egui::vec2(inner_width, 42.0),
            egui::TextEdit::singleline(value)
                .id_salt(label)
                .font(egui::FontId::proportional(18.0))
                .password(private)
                .hint_text(egui::RichText::new(hint).size(18.0))
                .margin(egui::vec2(12.0, 10.0)),
        );
    });
    panel.response
}

/// Draws one lobby access code as a compact card with an icon-only copy action.
fn lobby_code_card(ui: &mut egui::Ui, label: &str, code: &str, prominent: bool) {
    let card_width = ui.available_width().min(560.0);
    let tooltip = if label.eq_ignore_ascii_case("Game code") {
        "Share this code to invite players before the game starts. It also identifies the saved game when recovering with your private recovery code."
    } else {
        "Keep this code private. Enter it with the game code to restore your player and host or player role. The host resumes once everyone reconnects; finished games open for viewing."
    };
    code_card_frame().show(ui, |ui| {
        let inner_width = card_width - 36.0;
        ui.set_min_width(inner_width);
        ui.set_max_width(inner_width);
        code_card_heading(ui, label, tooltip, Some(code));
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                egui::RichText::new(code)
                    .size(if prominent {
                        27.0
                    } else {
                        16.0
                    })
                    .strong()
                    .monospace()
                    .color(egui::Color32::WHITE),
            )
            .wrap(),
        );
    });
}

/// Shows the page's explanation without permanently crowding the code card.
fn code_info_icon(ui: &mut egui::Ui, tooltip: &str) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(28.0, 30.0), egui::Sense::hover());
    let center = rect.center();
    let color = if response.hovered() {
        egui::Color32::WHITE
    } else {
        egui::Color32::from_rgb(170, 190, 214)
    };
    ui.painter().circle_stroke(center, 8.0, egui::Stroke::new(1.5, color));
    ui.painter().text(
        center,
        egui::Align2::CENTER_CENTER,
        "i",
        egui::FontId::proportional(14.0),
        color,
    );
    response.on_hover_ui(|ui| {
        ui.set_max_width(300.0);
        ui.label(egui::RichText::new(tooltip).size(13.0));
    });
}

/// Copies a value using a familiar overlapping-pages glyph instead of a text button.
fn copy_icon_button(ui: &mut egui::Ui, value: &str, label: &str) {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 30.0), egui::Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(format!("Copy {label}"));
    let fill = if response.is_pointer_button_down_on() {
        PRESSED_BUTTON_COLOR.to_color32()
    } else if response.hovered() {
        HOVERED_BUTTON_COLOR.to_color32()
    } else {
        NORMAL_BUTTON_COLOR.to_color32()
    };
    ui.painter().rect_filled(rect, 3.0, fill);

    let icon_color = egui::Color32::from_rgb(225, 231, 240);
    let back = egui::Rect::from_min_size(rect.min + egui::vec2(9.0, 6.0), egui::vec2(11.0, 13.0));
    let front =
        egui::Rect::from_min_size(rect.min + egui::vec2(14.0, 11.0), egui::vec2(11.0, 13.0));
    let stroke = egui::Stroke::new(1.5, icon_color);
    ui.painter().rect_stroke(back, 1.0, stroke, egui::StrokeKind::Inside);
    ui.painter().rect_stroke(front, 1.0, stroke, egui::StrokeKind::Inside);

    if response.clicked() {
        mark_menu_click(ui);
        ui.ctx().copy_text(value.to_string());
    }
}

/// Presents lobby capacity and members in a single automatically updating card.
fn lobby_players_card(
    ui: &mut egui::Ui,
    game: &GameRecord,
    reconnecting: bool,
    local_player_id: Option<u64>,
    busy: bool,
    requests: &mut MessageWriter<MultiplayerRequest>,
) {
    let is_host = game
        .members
        .iter()
        .any(|member| Some(member.player_id) == local_player_id && member.is_creator);
    let card_width = ui.available_width().min(560.0);
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(14, 22, 31, 232))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(130, 170, 215, 115)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(18, 14))
        .show(ui, |ui| {
            let inner_width = card_width - 36.0;
            ui.set_min_width(inner_width);
            ui.set_max_width(inner_width);
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new("Players").size(20.0).strong());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        egui::RichText::new(format!(
                            "{} {}",
                            game.members.len(),
                            if game.members.len() == 1 {
                                "player"
                            } else {
                                "players"
                            }
                        ))
                        .size(13.0)
                        .color(egui::Color32::from_rgb(175, 195, 218)),
                    );
                });
            });
            ui.add_space(8.0);
            ui.label(
                egui::RichText::new(if reconnecting {
                    if game.members.iter().all(|member| member.connected) {
                        "Everyone is connected. The host can resume the game."
                    } else {
                        "Waiting for every existing player to reconnect…"
                    }
                } else if !is_host {
                    "Waiting for the host to start..."
                } else if game.members.len() < 2 {
                    "Waiting for at least one more player…"
                } else {
                    "Ready to start — more players may still join."
                })
                .size(14.0)
                .color(egui::Color32::from_rgb(175, 195, 218)),
            );
            ui.add_space(7.0);

            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, member) in game.members.iter().enumerate() {
                let picker_id =
                    ui.make_persistent_id(("player_color_picker", &game.id, member.player_id));
                let editable =
                    game.status == MatchStatus::Lobby && local_player_id == Some(member.player_id);
                let row_margin_x = 10.0;
                let row_inner_width = inner_width - row_margin_x * 2.0;
                let color_width = 26.0;
                let id_width = 38.0;
                let role_width = 56.0;
                let status_width = if reconnecting {
                    118.0
                } else {
                    0.0
                };
                let gap = 8.0;
                let gap_count = 3.0 + u8::from(reconnecting) as f32;
                let name_width = (row_inner_width
                    - color_width
                    - id_width
                    - status_width
                    - role_width
                    - gap * gap_count)
                    .max(0.0);
                egui::Frame::new()
                    .fill(egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14))
                    .inner_margin(egui::Margin::symmetric(row_margin_x as i8, 4))
                    .show(ui, |ui| {
                        ui.set_min_width(row_inner_width);
                        ui.set_max_width(row_inner_width);
                        ui.spacing_mut().item_spacing.x = gap;
                        ui.horizontal(|ui| {
                            let color = game
                                .persisted
                                .state
                                .player(member.player_id)
                                .map(|player| player.color())
                                .unwrap_or_else(|_| PlayerColor::for_player(member.player_id));
                            let marker =
                                player_color_dot(ui, color, color_width, editable && !busy);
                            lobby_color_popover(
                                &marker,
                                picker_id,
                                game,
                                member.player_id,
                                editable && !busy,
                                requests,
                            );
                            ui.add_sized(
                                [id_width, 22.0],
                                egui::Label::new(
                                    egui::RichText::new(format!("#{:02}", member.player_id))
                                        .size(14.0)
                                        .monospace()
                                        .color(egui::Color32::from_rgb(145, 184, 226)),
                                ),
                            );
                            ui.allocate_ui_with_layout(
                                egui::vec2(name_width, 22.0),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    ui.set_min_size(egui::vec2(name_width, 22.0));
                                    ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(&member.display_name)
                                                .size(16.0)
                                                .strong(),
                                        )
                                        .halign(egui::Align::LEFT)
                                        .truncate(),
                                    );
                                },
                            );
                            if reconnecting {
                                let (status, status_color) = if member.connected {
                                    ("Connected", egui::Color32::from_rgb(102, 224, 170))
                                } else {
                                    ("Not connected", egui::Color32::from_rgb(243, 190, 92))
                                };
                                let (rect, _) = ui.allocate_exact_size(
                                    egui::vec2(status_width, 22.0),
                                    egui::Sense::hover(),
                                );
                                ui.painter().circle_filled(
                                    egui::pos2(rect.left() + 3.5, rect.center().y),
                                    3.5,
                                    status_color,
                                );
                                ui.painter().text(
                                    egui::pos2(rect.left() + 15.0, rect.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    status,
                                    egui::FontId::proportional(13.0),
                                    status_color,
                                );
                            }
                            ui.allocate_ui_with_layout(
                                egui::vec2(role_width, 22.0),
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    ui.label(
                                        egui::RichText::new(if member.is_creator {
                                            "HOST"
                                        } else {
                                            "CLIENT"
                                        })
                                        .size(12.0)
                                        .strong()
                                        .color(egui::Color32::from_rgb(175, 195, 218)),
                                    );
                                },
                            );
                        });
                    });
                if index + 1 < game.members.len() {
                    ui.add_space(3.0);
                }
            }
        });
}

/// Overlays the choices below the marker without changing the player card's size.
fn lobby_color_popover(
    marker: &egui::Response,
    picker_id: egui::Id,
    game: &GameRecord,
    player_id: u64,
    editable: bool,
    requests: &mut MessageWriter<MultiplayerRequest>,
) {
    let was_open = marker.ctx.data(|data| data.get_temp::<bool>(picker_id).unwrap_or(false));
    let colors = available_player_colors(game, player_id);
    let popup = egui::Popup::from_response(marker)
        .id(picker_id)
        .gap(0.0)
        .width(colors.len() as f32 * 44.0 - 10.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .frame(
            egui::Frame::new()
                .fill(egui::Color32::from_rgb(14, 22, 31))
                .stroke(egui::Stroke::new(
                    1.0,
                    egui::Color32::from_rgba_unmultiplied(130, 170, 215, 180),
                ))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(8, 6)),
        );
    // The shared hover region lets the pointer travel diagonally into the palette.
    let hovering_picker = popup.get_popup_rect().is_some_and(|rect| {
        marker
            .ctx
            .pointer_hover_pos()
            .is_some_and(|pos| rect.union(marker.rect).expand(3.0).contains(pos))
    });
    let mut open = editable && (marker.hovered() || (was_open && hovering_picker));
    if let Some(response) =
        popup.open_bool(&mut open).show(|ui| lobby_color_picker(ui, &colors, requests))
    {
        if response.inner {
            open = false;
        }
    }
    marker.ctx.data_mut(|data| data.insert_temp(picker_id, open));
}

/// Offers only other colors that have not been claimed by another player.
fn available_player_colors(game: &GameRecord, player_id: u64) -> Vec<PlayerColor> {
    let selected = game
        .persisted
        .state
        .player(player_id)
        .map(|player| player.color())
        .unwrap_or_else(|_| PlayerColor::for_player(player_id));

    PLAYER_COLOR_PALETTE
        .into_iter()
        .filter(|color| {
            *color != selected
                && !game.members.iter().any(|member| {
                    member.player_id != player_id
                        && game
                            .persisted
                            .state
                            .player(member.player_id)
                            .map(|player| player.color())
                            .unwrap_or_else(|_| PlayerColor::for_player(member.player_id))
                            == *color
                })
        })
        .collect()
}

/// Draws clickable swatches without informational hover tooltips.
fn lobby_color_picker(
    ui: &mut egui::Ui,
    colors: &[PlayerColor],
    requests: &mut MessageWriter<MultiplayerRequest>,
) -> bool {
    let mut chosen = false;
    ui.set_width(colors.len() as f32 * 44.0 - 10.0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        for color in colors {
            if player_color_swatch(ui, *color, false) {
                requests.write(MultiplayerRequest::SetPlayerColor(*color));
                chosen = true;
            }
        }
    });
    chosen
}

/// Draws a shared lobby/practice swatch with selection, hover, and click feedback.
fn player_color_swatch(ui: &mut egui::Ui, color: PlayerColor, selected: bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(34.0, 34.0), egui::Sense::click());
    let [red, green, blue] = color.rgb();
    ui.painter().circle_filled(rect.center(), 11.0, egui::Color32::from_rgb(red, green, blue));
    if selected || response.hovered() || response.has_focus() {
        ui.painter().circle_stroke(
            rect.center(),
            14.0,
            egui::Stroke::new(1.5, egui::Color32::WHITE),
        );
    }
    let clicked = response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
    if clicked {
        mark_menu_click(ui);
    }
    clicked
}

/// Paints the player marker used to hover-open the local player's color choices.
fn player_color_dot(
    ui: &mut egui::Ui,
    color: PlayerColor,
    width: f32,
    editable: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 26.0), egui::Sense::hover());
    let [red, green, blue] = color.rgb();
    let fill = egui::Color32::from_rgb(red, green, blue);
    ui.painter().circle_filled(rect.center(), 6.0, fill);
    ui.painter().circle_stroke(
        rect.center(),
        6.0,
        egui::Stroke::new(1.0, egui::Color32::from_white_alpha(180)),
    );
    if editable && response.hovered() {
        ui.painter().circle_stroke(
            rect.center(),
            10.0,
            egui::Stroke::new(1.0, egui::Color32::WHITE),
        );
    }
    if editable {
        response.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Draws the only lobby action available to a non-host player.
fn lobby_leave_button(ui: &mut egui::Ui, enabled: bool) -> bool {
    let (size, text_size, spacing) = menu_button_pair_metrics(ui);
    let clicked = ui
        .allocate_ui_with_layout(size, egui::Layout::left_to_right(egui::Align::Center), |ui| {
            menu_button_widget(ui, "Leave Lobby", enabled, size, text_size)
        })
        .inner;
    ui.add_space(spacing);
    clicked
}

/// Shows loading progress while the canonical state and deferred world assets are prepared.
fn loading_screen(ui: &mut egui::Ui, session: &MultiplayerSession) {
    ui.add(egui::Spinner::new().size(32.0));
    ui.add_space(12.0);
    ui.label(egui::RichText::new("Starting game…").size(30.0).strong());
    ui.add_space(8.0);
    let game = session.active_game.as_ref().map(|game| game.code.as_str()).unwrap_or("game");
    ui.label(
        egui::RichText::new(format!("Loading {game} and gameplay assets…")).size(24.0).strong(),
    );
}

/// Edits preferences that do not affect deterministic turn resolution.
fn settings_screen(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    change_audio: &mut MessageWriter<ChangeAudioMsg>,
    next_state: &mut NextState<AppState>,
) {
    ui.heading(egui::RichText::new("Settings").size(36.0));
    ui.add_space(28.0);
    settings_choices(ui, settings, change_audio);
    ui.add_space(40.0);
    back_button(ui, next_state, AppState::MainMenu);
}

/// Draws the shared menu and in-game preference controls.
fn settings_choices(
    ui: &mut egui::Ui,
    settings: &mut Settings,
    change_audio: &mut MessageWriter<ChangeAudioMsg>,
) {
    let previous_audio = settings.audio;
    choice_row(
        ui,
        "Audio",
        &mut settings.audio,
        &[
            (AudioState::Mute, "Muted"),
            (AudioState::NoMusic, "Effects"),
            (AudioState::Sound, "Music"),
        ],
    );
    if settings.audio != previous_audio {
        change_audio.write(ChangeAudioMsg(Some(settings.audio)));
    }
    ui.add_space(12.0);
    choice_row(ui, "Map cells", &mut settings.show_cells, &[(true, "Shown"), (false, "Hidden")]);
    ui.add_space(12.0);
    choice_row(
        ui,
        "Hover information",
        &mut settings.show_hover,
        &[(true, "Shown"), (false, "Hidden")],
    );
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
    mut change_audio: MessageWriter<ChangeAudioMsg>,
) {
    if matches!(*game_state.get(), GameState::Playing | GameState::Combat) {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    if game_state.get().is_modal_menu() {
        block_gameplay_pointer(context);
    }
    let viewport = context.viewport_rect();
    let content_width =
        (viewport.width() * 0.42).clamp(360.0, 560.0).min((viewport.width() - 32.0).max(280.0));
    egui::Area::new("stellarion_game_overlay".into())
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        .movable(false)
        .constrain(true)
        .order(egui::Order::Foreground)
        .show(context, |ui| {
            apply_menu_style(ui);
            ui.set_width(content_width);
            ui.vertical_centered(|ui| match game_state.get() {
                GameState::GameMenu => {
                    main_menu_button(ui, "Continue", || next_game_state.set(GameState::Playing));
                    if session.has_active_game() && !session.local_practice {
                        main_menu_button(ui, "Save Game", || {
                            requests.write(MultiplayerRequest::SaveGame);
                        });
                        if let Some(game) = session
                            .active_game
                            .as_ref()
                            .filter(|game| game.status == MatchStatus::Active)
                        {
                            main_menu_button(ui, "Game Lobby", || {
                                requests.write(MultiplayerRequest::ResumeGame(game.id.clone()));
                                next_game_state.set(GameState::Playing);
                            });
                        }
                    }
                    main_menu_button(ui, "Settings", || next_game_state.set(GameState::Settings));
                    main_menu_button(ui, "Return to Main Menu", || {
                        requests.write(MultiplayerRequest::LeaveGame);
                        next_game_state.set(GameState::Playing);
                        next_app_state.set(AppState::MainMenu);
                    });
                },
                GameState::Settings => {
                    ui.heading(egui::RichText::new("Settings").size(36.0));
                    ui.add_space(28.0);
                    settings_choices(ui, &mut settings, &mut change_audio);
                    ui.add_space(40.0);
                    main_menu_button(ui, "Back", || next_game_state.set(GameState::GameMenu));
                },
                GameState::EndGame => {
                    ui.heading(egui::RichText::new("Game finished").size(36.0));
                    ui.add_space(28.0);
                    main_menu_button(ui, "Spectate", || next_game_state.set(GameState::Playing));
                    main_menu_button(ui, "Return to Main Menu", || {
                        requests.write(MultiplayerRequest::LeaveGame);
                        next_game_state.set(GameState::Playing);
                        next_app_state.set(AppState::MainMenu);
                    });
                },
                GameState::Playing | GameState::CombatMenu | GameState::Combat => {},
            });
        });

    if *game_state.get() == GameState::GameMenu {
        game_access_codes(context, &session);
    }
}

/// Covers the viewport with an invisible interactive layer so Egui captures every pointer action.
fn block_gameplay_pointer(context: &egui::Context) {
    let viewport = context.viewport_rect();
    egui::Area::new("stellarion_gameplay_input_blocker".into())
        .fixed_pos(viewport.min)
        .movable(false)
        .constrain(false)
        .order(egui::Order::Middle)
        .show(context, |ui| {
            ui.allocate_exact_size(viewport.size(), egui::Sense::click_and_drag());
        });
}

/// Keeps the active game's share and recovery information accessible during play.
fn game_access_codes(context: &egui::Context, session: &MultiplayerSession) {
    if session.local_practice {
        return;
    }
    let Some(game) = &session.active_game else {
        return;
    };
    let viewport = context.viewport_rect();
    let panel_width =
        (viewport.width() * 0.28).clamp(300.0, 360.0).min((viewport.width() - 48.0).max(240.0));
    egui::Area::new("stellarion_game_access_codes".into())
        .anchor(egui::Align2::LEFT_BOTTOM, egui::vec2(12.0, -12.0))
        .movable(false)
        .constrain(true)
        .order(egui::Order::Foreground)
        .show(context, |ui| {
            apply_menu_style(ui);
            ui.set_width(panel_width);
            lobby_code_card(ui, "Game code", game.code.as_str(), true);
        });
}

/// Stops terminal-state audio when the user exits its overlay.
pub fn exit_end_game(mut mute_audio_msg: MessageWriter<MuteAudioMsg>) {
    mute_audio_msg.write(MuteAudioMsg);
}

/// Draws a slightly larger action for the top-level menu only.
fn main_menu_button(ui: &mut egui::Ui, label: &str, on_click: impl FnOnce()) {
    let (base_size, base_text_size, spacing) = menu_button_metrics(ui);
    let size = egui::vec2((base_size.x * 1.4).min(ui.available_width()), base_size.y * 1.25);
    if menu_button_widget(ui, label, true, size, base_text_size * 1.2) {
        on_click();
    }
    ui.add_space(spacing * 1.08);
}

/// Draws two standard menu actions on one centered row.
fn menu_button_pair(
    ui: &mut egui::Ui,
    left_label: &str,
    left_enabled: bool,
    right_label: &str,
    right_enabled: bool,
    right_busy: bool,
) -> (bool, bool) {
    let (button_size, text_size, spacing) = menu_button_pair_metrics(ui);
    let gap = 12.0;
    let row_width = button_size.x * 2.0 + gap;
    let clicked = ui
        .allocate_ui_with_layout(
            egui::vec2(row_width, button_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = gap;
                let left_clicked = menu_action_button(
                    ui,
                    left_label,
                    left_enabled,
                    false,
                    false,
                    button_size,
                    text_size,
                );
                let right_clicked = menu_action_button(
                    ui,
                    right_label,
                    right_enabled && !right_busy,
                    true,
                    right_busy,
                    button_size,
                    text_size,
                );
                (left_clicked, right_clicked)
            },
        )
        .inner;
    ui.add_space(spacing);
    clicked
}

/// Draws the larger Local Practice actions side by side, with navigation first.
#[cfg(debug_assertions)]
fn local_practice_action_buttons(ui: &mut egui::Ui, start_enabled: bool) -> (bool, bool) {
    let (base_size, base_text_size, spacing) = menu_button_metrics(ui);
    let gap = 12.0;
    let button_width = (base_size.x * 1.1).min(((ui.available_width() - gap) * 0.5).max(1.0));
    let button_size = egui::vec2(button_width, base_size.y * 1.1);
    let row_width = button_width * 2.0 + gap;
    let clicked = ui
        .allocate_ui_with_layout(
            egui::vec2(row_width, button_size.y),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.spacing_mut().item_spacing.x = gap;
                (
                    menu_action_button(
                        ui,
                        "Back",
                        true,
                        false,
                        false,
                        button_size,
                        base_text_size * 1.1,
                    ),
                    menu_action_button(
                        ui,
                        "Start Practice",
                        start_enabled,
                        true,
                        false,
                        button_size,
                        base_text_size * 1.1,
                    ),
                )
            },
        )
        .inner;
    ui.add_space(spacing);
    clicked
}

/// Returns the compact dimensions shared by all menu navigation actions.
fn menu_button_metrics(ui: &egui::Ui) -> (egui::Vec2, f32, f32) {
    let viewport = ui.max_rect().size();
    (
        egui::vec2((viewport.x * 0.20).clamp(220.0, 340.0), (viewport.y * 0.075).clamp(44.0, 68.0)),
        (viewport.y / 36.0).clamp(18.0, 26.0),
        (viewport.y * 0.008).clamp(5.0, 8.0),
    )
}

/// Keeps paired actions and the client's single lobby action the same size.
fn menu_button_pair_metrics(ui: &egui::Ui) -> (egui::Vec2, f32, f32) {
    let (size, text_size, spacing) = menu_button_metrics(ui);
    let width = size.x.min(((ui.available_width() - 12.0) * 0.5).max(1.0));
    (egui::vec2(width, size.y), text_size, spacing)
}

/// Draws one shared rounded menu button with the established hover behavior.
fn menu_button_widget(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    size: egui::Vec2,
    text_size: f32,
) -> bool {
    menu_action_button(ui, label, enabled, false, false, size, text_size)
}

/// Paints the translucent rounded action style shared by every menu screen.
fn menu_action_button(
    ui: &mut egui::Ui,
    label: &str,
    enabled: bool,
    primary: bool,
    busy: bool,
    size: egui::Vec2,
    text_size: f32,
) -> bool {
    let sense = if enabled {
        egui::Sense::click()
    } else {
        egui::Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(size, sense);
    let hovered = enabled && response.hovered();
    let pressed = enabled && response.is_pointer_button_down_on();
    let fill = match (primary, pressed, hovered, enabled) {
        (_, _, _, false) => egui::Color32::from_rgba_unmultiplied(24, 34, 45, 225),
        (true, true, _, _) => egui::Color32::from_rgb(45, 105, 139),
        (true, _, true, _) => egui::Color32::from_rgb(48, 119, 155),
        (true, _, _, _) => egui::Color32::from_rgb(39, 94, 123),
        (false, true, _, _) => egui::Color32::from_rgba_unmultiplied(46, 61, 78, 242),
        (false, _, true, _) => egui::Color32::from_rgba_unmultiplied(36, 50, 65, 238),
        (false, _, _, _) => egui::Color32::from_rgba_unmultiplied(18, 28, 39, 228),
    };
    let border = if primary && enabled {
        egui::Color32::from_rgba_unmultiplied(112, 206, 241, 145)
    } else {
        egui::Color32::from_rgba_unmultiplied(150, 184, 215, 92)
    };
    ui.painter().rect(
        rect,
        egui::CornerRadius::same(6),
        fill,
        egui::Stroke::new(1.0, border),
        egui::StrokeKind::Inside,
    );
    ui.painter().text(
        rect.center()
            + egui::vec2(
                if busy {
                    10.0
                } else {
                    0.0
                },
                0.0,
            ),
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(text_size),
        if enabled {
            egui::Color32::WHITE
        } else {
            egui::Color32::from_rgb(132, 145, 158)
        },
    );
    if busy {
        let spinner_size = 16.0;
        let spinner_rect = egui::Rect::from_center_size(
            egui::pos2(rect.left() + 22.0, rect.center().y),
            egui::vec2(spinner_size, spinner_size),
        );
        ui.put(spinner_rect, egui::Spinner::new().size(spinner_size));
    }
    if enabled {
        let clicked = response.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
        if clicked {
            mark_menu_click(ui);
        }
        clicked
    } else {
        false
    }
}

/// Applies the same rounded translucent palette to remaining standard egui controls.
fn apply_menu_style(ui: &mut egui::Ui) {
    let visuals = &mut ui.style_mut().visuals;
    visuals.button_frame = true;
    visuals.disabled_alpha = 0.65;
    visuals.extreme_bg_color = NORMAL_BUTTON_COLOR.to_color32();
    let states = [
        (&mut visuals.widgets.inactive, egui::Color32::from_rgba_unmultiplied(18, 28, 39, 228)),
        (&mut visuals.widgets.hovered, egui::Color32::from_rgba_unmultiplied(36, 50, 65, 238)),
        (&mut visuals.widgets.active, egui::Color32::from_rgb(45, 105, 139)),
        (&mut visuals.widgets.open, egui::Color32::from_rgb(48, 119, 155)),
        (
            &mut visuals.widgets.noninteractive,
            egui::Color32::from_rgba_unmultiplied(24, 34, 45, 225),
        ),
    ];
    for (state, color) in states {
        state.bg_fill = color;
        state.weak_bg_fill = color;
        state.bg_stroke =
            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(150, 184, 215, 92));
        state.corner_radius = egui::CornerRadius::same(6);
        state.expansion = 0.0;
    }
}

/// Draws the player-name field when creating a game or joining for the first time.
fn player_name_field(ui: &mut egui::Ui, display_name: &mut String, max_width: f32) {
    ui.label(egui::RichText::new("Player name").size(20.0).strong());
    form_text_edit_width(ui, display_name, false, max_width);
    ui.add_space(8.0);
}

/// Draws a centered text input capped to the supplied form width.
fn form_text_edit_width(ui: &mut egui::Ui, value: &mut String, password: bool, max_width: f32) {
    let width = ui.available_width().min(max_width);
    let editor = egui::TextEdit::singleline(value)
        .password(password)
        .frame(egui::Frame::NONE)
        .horizontal_align(egui::Align::Center);
    egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(18, 28, 39, 228))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(150, 184, 215, 92)))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.add_sized([width - 24.0, 28.0], editor);
        });
}

/// Draws the original menu's labeled rows of flat, discrete setting buttons.
fn choice_row<T: Copy + PartialEq>(
    ui: &mut egui::Ui,
    label: &str,
    value: &mut T,
    choices: &[(T, &str)],
) {
    ui.label(egui::RichText::new(label).size(20.0).strong());
    ui.add_space(3.0);
    let gap = 8.0;
    let width = ((ui.available_width().min(520.0) - gap * 2.0) / 3.0).max(72.0);
    let row_width = width * choices.len() as f32 + gap * (choices.len() - 1) as f32;
    ui.allocate_ui_with_layout(
        egui::vec2(row_width, 40.0),
        egui::Layout::left_to_right(egui::Align::Center),
        |ui| {
            ui.spacing_mut().item_spacing.x = gap;
            for (candidate, text) in choices {
                let selected = value == candidate;
                let button = egui::Button::new(
                    egui::RichText::new(*text)
                        .size(18.0)
                        .strong()
                        .color(egui::Color32::TRANSPARENT),
                )
                .fill(if selected {
                    egui::Color32::from_rgb(39, 94, 123)
                } else {
                    egui::Color32::from_rgba_unmultiplied(18, 28, 39, 228)
                })
                .stroke(egui::Stroke::new(
                    1.0,
                    if selected {
                        egui::Color32::from_rgba_unmultiplied(112, 206, 241, 145)
                    } else {
                        egui::Color32::from_rgba_unmultiplied(150, 184, 215, 92)
                    },
                ))
                .corner_radius(6.0);
                let response = ui
                    .add_sized([width, 40.0], button)
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                paint_choice_icon(ui, response.rect, text);
                if response.clicked() {
                    mark_menu_click(ui);
                    *value = *candidate;
                }
            }
        },
    );
    ui.add_space(6.0);
}

/// Records an explicit menu interaction so audio does not depend on egui's response history.
fn mark_menu_click(ui: &egui::Ui) {
    set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
}

/// Draws one centered icon-and-label group without relying on font icon glyphs.
fn paint_choice_icon(ui: &egui::Ui, rect: egui::Rect, label: &str) {
    let icon = match label {
        "Muted" => Some(0),
        "Effects" => Some(1),
        "Music" => Some(2),
        "Shown" => Some(3),
        "Hidden" => Some(4),
        _ => None,
    };
    let painter = ui.painter();
    let color = ui.visuals().strong_text_color();
    let font = egui::FontId::proportional(18.0);
    let label_galley = painter.layout_no_wrap(label.to_owned(), font, color);
    let Some(icon) = icon else {
        painter.galley(rect.center() - label_galley.size() * 0.5, label_galley, color);
        return;
    };
    let icon_width = 16.0;
    let gap = 7.0;
    let group_width = icon_width + gap + label_galley.size().x;
    let group_left = rect.center().x - group_width * 0.5;
    let center = egui::pos2(group_left + icon_width * 0.5, rect.center().y);
    let stroke = egui::Stroke::new(1.6, color);

    if icon <= 1 {
        painter.add(egui::Shape::convex_polygon(
            vec![
                center + egui::vec2(-6.0, -2.5),
                center + egui::vec2(-3.0, -2.5),
                center + egui::vec2(1.0, -6.0),
                center + egui::vec2(1.0, 6.0),
                center + egui::vec2(-3.0, 2.5),
                center + egui::vec2(-6.0, 2.5),
            ],
            color,
            egui::Stroke::NONE,
        ));
        if icon == 0 {
            painter.line_segment(
                [center + egui::vec2(4.0, -3.5), center + egui::vec2(10.0, 3.5)],
                stroke,
            );
            painter.line_segment(
                [center + egui::vec2(10.0, -3.5), center + egui::vec2(4.0, 3.5)],
                stroke,
            );
        } else {
            painter.line_segment(
                [center + egui::vec2(4.0, -3.5), center + egui::vec2(6.5, -1.5)],
                stroke,
            );
            painter.line_segment(
                [center + egui::vec2(6.5, -1.5), center + egui::vec2(6.5, 1.5)],
                stroke,
            );
            painter.line_segment(
                [center + egui::vec2(6.5, 1.5), center + egui::vec2(4.0, 3.5)],
                stroke,
            );
        }
    } else if icon == 2 {
        painter.line_segment(
            [center + egui::vec2(2.0, -6.0), center + egui::vec2(2.0, 3.5)],
            egui::Stroke::new(2.0, color),
        );
        painter.line_segment(
            [center + egui::vec2(2.0, -6.0), center + egui::vec2(7.0, -4.0)],
            egui::Stroke::new(2.0, color),
        );
        painter.circle_filled(center + egui::vec2(-1.0, 4.5), 3.0, color);
    } else {
        painter.add(egui::Shape::closed_line(
            vec![
                center + egui::vec2(-8.0, 0.0),
                center + egui::vec2(-4.0, -4.0),
                center + egui::vec2(0.0, -5.5),
                center + egui::vec2(4.0, -4.0),
                center + egui::vec2(8.0, 0.0),
                center + egui::vec2(4.0, 4.0),
                center + egui::vec2(0.0, 5.5),
                center + egui::vec2(-4.0, 4.0),
            ],
            stroke,
        ));
        painter.circle_filled(center, 2.3, color);
        if icon == 4 {
            painter.line_segment(
                [center + egui::vec2(-7.0, -7.0), center + egui::vec2(7.0, 7.0)],
                egui::Stroke::new(2.2, color),
            );
        }
    }

    painter.galley(
        egui::pos2(group_left + icon_width + gap, rect.center().y - label_galley.size().y * 0.5),
        label_galley,
        color,
    );
}

/// Draws map-generation settings with the same discrete values as the original menu.
fn map_rule_rows(ui: &mut egui::Ui, settings: &mut Settings) {
    choice_row(
        ui,
        "Planets per player",
        &mut settings.n_planets,
        &[(5, "5"), (10, "10"), (20, "20")],
    );
    choice_row(
        ui,
        "Colonizable planets",
        &mut settings.p_colonizable,
        &[(25, "25%"), (50, "50%"), (100, "100%")],
    );
    choice_row(
        ui,
        "Moons per planet",
        &mut settings.p_moons,
        &[(0, "0%"), (30, "30%"), (60, "60%")],
    );
}

/// Draws state navigation with the same dimensions and styling as primary menu actions.
fn back_button(ui: &mut egui::Ui, next_state: &mut NextState<AppState>, target: AppState) {
    main_menu_button(ui, "Back", || next_state.set(target));
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
#[path = "../../../tests/core/menu_systems.rs"]
mod tests;
