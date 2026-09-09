//! In-game notifications rendered with the egui version bundled by `bevy_egui`.

use std::collections::VecDeque;
use std::time::Duration;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use crate::core::audio::PlayAudioMsg;
use crate::core::constants::{MAX_ZOOM, MESSAGE_DURATION};
use crate::core::map::model::Map;
use crate::core::map::planet::PlanetId;
use crate::core::map::systems::select_planet;
use crate::core::missions::MissionId;
use crate::core::player::Player;
use crate::core::states::{AppState, GameState};
use crate::core::ui::systems::{resource_bar_bottom, MissionTab, UiState};

const DEFAULT_NOTIFICATION_TOP: f32 = 70.0;
const RESOURCE_BAR_NOTIFICATION_GAP: f32 = 12.0;
const MAX_NOTIFICATION_WIDTH: f32 = 440.0;

/// Severity used for notification color and sound selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageLevel {
    /// Routine status information.
    Info,
    /// A recoverable problem or caution.
    Warning,
    /// An operation that failed.
    Error,
}

/// Optional navigation performed when the player clicks a notification.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageAction {
    /// Opens the mission interface on the enemy-missions tab.
    OpenEnemyMissions,
    /// Opens the mission interface on the supplied persisted report.
    OpenMissionReport(MissionId),
    /// Opens the mission interface on the reports tab without selecting a hidden report.
    OpenMissionReports,
    /// Centers the strategic map on a still-owned colony and selects it.
    FocusColony(PlanetId),
    /// Centers the strategic map on a public world without opening its information panel.
    FocusPlanet(PlanetId),
    /// Centers the strategic map on a Railgun target and zooms out as far as possible.
    FocusRailgunTarget(PlanetId),
}

/// Requests a transient notification.
#[derive(Message, Clone, Debug)]
pub struct MessageMsg {
    /// User-facing notification text.
    pub message: String,
    /// Severity of the notification.
    pub level: MessageLevel,
    /// Optional navigation associated with clicking the notification.
    pub action: Option<MessageAction>,
    /// Suppresses the generic notification tone when the action supplies its own cue.
    pub silent: bool,
    /// Optional display lifetime used instead of the standard notification duration.
    pub display_duration: Option<Duration>,
}

impl MessageMsg {
    /// Creates a notification with an explicit severity.
    pub fn new(message: impl Into<String>, level: MessageLevel) -> Self {
        Self {
            message: message.into(),
            level,
            action: None,
            silent: false,
            display_duration: None,
        }
    }

    /// Makes this notification navigate when clicked.
    pub fn with_action(mut self, action: MessageAction) -> Self {
        self.action = Some(action);
        self
    }

    /// Keeps the notification visible while an action-specific sound plays.
    pub fn silent(mut self) -> Self {
        self.silent = true;
        self
    }

    /// Overrides how long this notification remains visible.
    pub fn with_duration(mut self, duration: Duration) -> Self {
        self.display_duration = Some(duration);
        self
    }

    /// Creates an informational notification.
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, MessageLevel::Info)
    }

    /// Creates a warning notification.
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, MessageLevel::Warning)
    }

    /// Creates an error notification.
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, MessageLevel::Error)
    }
}

#[derive(Clone, Debug)]
/// One queued transient notification with its remaining display timer.
struct ActiveMessage {
    text: String,
    level: MessageLevel,
    action: Option<MessageAction>,
    remaining_seconds: f32,
}

/// Active notification queue.
#[derive(Resource, Default)]
pub struct Messages(VecDeque<ActiveMessage>);

impl Messages {
    /// Queues a notification and bounds retained messages to the configured capacity.
    fn push(&mut self, message: &MessageMsg) {
        self.0.push_back(ActiveMessage {
            text: message.message.clone(),
            level: message.level,
            action: message.action,
            remaining_seconds: message.display_duration.map_or_else(
                || {
                    if matches!(
                        message.action,
                        Some(
                            MessageAction::FocusColony(_)
                                | MessageAction::FocusPlanet(_)
                                | MessageAction::FocusRailgunTarget(_)
                        )
                    ) {
                        10.0
                    } else {
                        MESSAGE_DURATION as f32
                    }
                },
                |duration| duration.as_secs_f32(),
            ),
        });

        // Keep an error storm from permanently covering the game viewport.
        while self.0.len() > 6 {
            self.0.pop_front();
        }
    }
}

/// Checks messages input/state and applies the resulting transition.
fn check_messages(
    mut contexts: EguiContexts,
    time: Res<Time>,
    mut messages: ResMut<Messages>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut message_msg: MessageReader<MessageMsg>,
    mut state: Option<ResMut<UiState>>,
    map: Option<Res<Map>>,
    player: Option<Res<Player>>,
    app_state: Option<Res<State<AppState>>>,
    game_state: Option<Res<State<GameState>>>,
) {
    // Only make one sound per severity per frame.
    let (mut info_sound, mut warning_sound, mut error_sound) = (true, true, true);

    for message in message_msg.read() {
        if message.silent {
            messages.push(message);
            continue;
        }
        let play = match message.level {
            MessageLevel::Info if info_sound => {
                info_sound = false;
                Some("message")
            },
            MessageLevel::Warning if warning_sound => {
                warning_sound = false;
                Some("warning")
            },
            MessageLevel::Error if error_sound => {
                error_sound = false;
                Some("error")
            },
            _ => None,
        };
        if let Some(name) = play {
            play_audio_msg.write(PlayAudioMsg::new(name));
        }
        messages.push(message);
    }

    let elapsed = time.delta_secs();
    let in_game = app_state.as_ref().is_some_and(|s| *s.get() == AppState::Game);
    let playing = in_game && game_state.as_ref().is_some_and(|s| *s.get() == GameState::Playing);
    messages.0.retain_mut(|message| {
        let actionable_planet_is_valid = match message.action {
            Some(MessageAction::FocusColony(id)) => {
                map.as_ref().zip(player.as_ref()).is_some_and(|(map, player)| {
                    map.try_get(id)
                        .is_some_and(|planet| player.owns(planet) && !planet.is_destroyed)
                })
            },
            Some(MessageAction::FocusPlanet(id)) => map
                .as_ref()
                .is_some_and(|map| map.try_get(id).is_some_and(|planet| !planet.is_destroyed)),
            Some(MessageAction::FocusRailgunTarget(id)) => {
                map.as_ref().is_some_and(|map| map.try_get(id).is_some())
            },
            _ => true,
        };
        if matches!(
            message.action,
            Some(
                MessageAction::FocusColony(_)
                    | MessageAction::FocusPlanet(_)
                    | MessageAction::FocusRailgunTarget(_)
            )
        ) {
            if !in_game || !actionable_planet_is_valid {
                return false;
            }
            if !playing {
                return true;
            }
        }
        message.remaining_seconds -= elapsed;
        message.remaining_seconds > 0.0
    });
    if messages.0.is_empty() {
        return;
    }

    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    if let Some((index, action)) = draw_notifications(context, &messages, playing) {
        messages.0.remove(index);
        if let Some(state) = state.as_mut() {
            match action {
                MessageAction::OpenEnemyMissions => {
                    open_enemy_missions(state);
                },
                MessageAction::OpenMissionReport(mission_id) => {
                    open_mission_reports(state, Some(mission_id));
                },
                MessageAction::OpenMissionReports => {
                    open_mission_reports(state, None);
                },
                MessageAction::FocusColony(planet_id) => {
                    if playing {
                        if let (Some(map), Some(player)) = (&map, &player) {
                            focus_colony(planet_id, map, player, state);
                        }
                    }
                },
                MessageAction::FocusPlanet(planet_id) => {
                    if playing {
                        if let Some(map) = &map {
                            focus_planet(planet_id, map, state);
                        }
                    }
                },
                MessageAction::FocusRailgunTarget(planet_id) => {
                    if playing {
                        if let Some(map) = &map {
                            focus_railgun_target(planet_id, map, state);
                        }
                    }
                },
            }
        }
    }
}

fn open_enemy_missions(state: &mut UiState) {
    state.planet_selected = None;
    state.mission = true;
    state.mission_tab = MissionTab::EnemyMissions;
    state.combat_report = None;
}

fn open_mission_reports(state: &mut UiState, mission_id: Option<MissionId>) {
    state.planet_selected = None;
    state.mission = true;
    state.mission_tab = MissionTab::MissionReports;
    if let Some(mission_id) = mission_id {
        state.mission_report = Some(mission_id);
    }
    state.combat_report = None;
}

fn draw_notifications(
    context: &egui::Context,
    messages: &Messages,
    playing: bool,
) -> Option<(usize, MessageAction)> {
    let mut clicked_message = None;
    let notification_top = if playing {
        DEFAULT_NOTIFICATION_TOP
            .max(resource_bar_bottom(context.content_rect().size()) + RESOURCE_BAR_NOTIFICATION_GAP)
    } else {
        DEFAULT_NOTIFICATION_TOP
    };
    egui::Area::new("stellarion_notifications".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, notification_top))
        // Combat uses foreground input layers of its own. Keep actionable notifications
        // above them so a visible toast always owns pointer input inside its frame.
        .order(egui::Order::Tooltip)
        .interactable(true)
        .layout(egui::Layout::top_down(egui::Align::Max))
        .show(context, |ui| {
            // Leave space for the outer anchor, frame margins, and border on narrow windows.
            ui.set_max_width(
                MAX_NOTIFICATION_WIDTH.min((context.content_rect().width() - 50.0).max(0.0)),
            );
            ui.spacing_mut().item_spacing.y = 6.0;
            // Each frame measures only its own label; the stack shares a right edge, not a width.
            for (index, message) in messages.0.iter().enumerate() {
                if !playing
                    && matches!(
                        message.action,
                        Some(
                            MessageAction::FocusColony(_)
                                | MessageAction::FocusPlanet(_)
                                | MessageAction::FocusRailgunTarget(_)
                        )
                    )
                {
                    continue;
                }
                let (fill, accent) = match message.level {
                    MessageLevel::Info => (
                        egui::Color32::from_rgba_unmultiplied(28, 36, 48, 235),
                        egui::Color32::from_rgb(112, 190, 255),
                    ),
                    MessageLevel::Warning => (
                        egui::Color32::from_rgba_unmultiplied(55, 43, 20, 240),
                        egui::Color32::from_rgb(255, 196, 82),
                    ),
                    MessageLevel::Error => (
                        egui::Color32::from_rgba_unmultiplied(58, 25, 29, 240),
                        egui::Color32::from_rgb(255, 105, 120),
                    ),
                };
                let response = egui::Frame::new()
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, accent))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(&message.text).small().color(accent),
                            )
                            .halign(egui::Align::Min)
                            .wrap(),
                        );
                    })
                    .response;
                if let Some(action) = message.action {
                    let response = response
                        .interact(egui::Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    if response.clicked() {
                        clicked_message = Some((index, action));
                    }
                }
            }
        });
    clicked_message
}

/// Selects and centers a colony without trusting stale toast targets.
fn focus_colony(planet_id: PlanetId, map: &Map, player: &Player, state: &mut UiState) -> bool {
    let Some(planet) = map.try_get(planet_id).filter(|p| player.owns(p) && !p.is_destroyed) else {
        return false;
    };
    select_planet(planet, state, player);
    state.to_selected = true;
    true
}

/// Centers the camera on a public world without opening intelligence the player does not have.
fn focus_planet(planet_id: PlanetId, map: &Map, state: &mut UiState) -> bool {
    let Some(planet) = map.try_get(planet_id).filter(|planet| !planet.is_destroyed) else {
        return false;
    };
    state.planet_selected = None;
    state.focus_planet = Some(planet.id);
    state.focus_zoom = None;
    state.to_selected = true;
    state.mission = false;
    state.combat_report = None;
    true
}

/// Centers and fully zooms out on a Railgun target, including a destroyed world.
fn focus_railgun_target(planet_id: PlanetId, map: &Map, state: &mut UiState) -> bool {
    let Some(planet) = map.try_get(planet_id) else {
        return false;
    };
    state.planet_selected = None;
    state.focus_planet = Some(planet.id);
    state.focus_zoom = Some(MAX_ZOOM);
    state.to_selected = true;
    state.mission = false;
    state.combat_report = None;
    true
}

/// Installs notification collection, expiration, sound, and rendering.
#[derive(Default)]
pub struct MessagesPlugin;

impl Plugin for MessagesPlugin {
    /// Registers this plugin's resources, messages, and ordered systems.
    fn build(&self, app: &mut App) {
        app.init_resource::<Messages>().add_systems(EguiPrimaryContextPass, check_messages);
    }
}

#[cfg(test)]
#[path = "../../tests/core/messages.rs"]
mod tests;
