//! In-game notifications rendered with the egui version bundled by `bevy_egui`.

use std::collections::VecDeque;
use std::time::Duration;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use crate::core::audio::PlayAudioMsg;
use crate::core::constants::{MAX_ZOOM, MESSAGE_DURATION};
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::Planet;
use crate::core::map::planet::PlanetId;
use crate::core::map::systems::select_planet;
use crate::core::missions::{Mission, MissionId, Missions};
use crate::core::player::Player;
use crate::core::states::{AppState, GameState};
use crate::core::ui::systems::{resource_bar_bottom, viewport_ui_scale, MissionTab, UiState};
use crate::core::units::Amount;
use crate::multiplayer::client::MultiplayerSession;

const DEFAULT_NOTIFICATION_TOP: f32 = 70.0;
const RESOURCE_BAR_NOTIFICATION_GAP: f32 = 12.0;
const MAX_NOTIFICATION_WIDTH: f32 = 560.0;
const NOTIFICATION_SPACING: f32 = 6.0;

pub(crate) fn notification_scale(viewport: egui::Vec2) -> f32 {
    (viewport_ui_scale(viewport) * 1.1).clamp(0.8, 1.35)
}

/// Appends a notification group below those already measured in this egui pass.
pub(crate) fn show_notification_area(
    context: &egui::Context,
    id: &'static str,
    playing: bool,
    max_width: f32,
    contents: impl FnOnce(&mut egui::Ui),
) {
    let stack_id = egui::Id::new("notification_stack_bottom");
    let pass = context.cumulative_pass_nr();
    let viewport = context.content_rect();
    let scale = notification_scale(viewport.size());
    let top = if playing {
        (DEFAULT_NOTIFICATION_TOP * scale)
            .max(resource_bar_bottom(viewport.size()) + RESOURCE_BAR_NOTIFICATION_GAP * scale)
    } else {
        DEFAULT_NOTIFICATION_TOP * scale
    };
    let top = context.data(|data| {
        data.get_temp::<(u64, f32)>(stack_id)
            .filter(|(last_pass, _)| *last_pass == pass)
            .map_or(top, |(_, bottom)| {
                top.max(bottom - viewport.top() + NOTIFICATION_SPACING * scale)
            })
    });
    let area_id = egui::Id::new(id);
    let transform =
        egui::emath::TSTransform::new(egui::vec2(viewport.right() * (1.0 - scale), 0.0), scale);
    context.set_transform_layer(egui::LayerId::new(egui::Order::Tooltip, area_id), transform);
    let response = egui::Area::new(area_id)
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, top / scale))
        .order(egui::Order::Tooltip)
        .interactable(true)
        // Overflow stays clipped below the viewport instead of moving over earlier toasts.
        .constrain(false)
        .layout(egui::Layout::top_down(egui::Align::Max))
        .show(context, |ui| {
            ui.set_max_width(max_width.min((viewport.width() / scale - 24.0).max(0.0)));
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.spacing_mut().item_spacing.y = NOTIFICATION_SPACING;
            contents(ui);
        })
        .response;
    if response.rect.height() > 0.0 {
        let bottom = transform.mul_rect(response.rect).bottom();
        context.data_mut(|data| data.insert_temp(stack_id, (pass, bottom)));
    }
}

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
    /// Opens the completed resource trade for review.
    OpenTrade(u64),
    /// Centers the strategic map on a still-owned colony and selects it.
    FocusColony(PlanetId),
    /// Centers the strategic map on a public world without opening its information panel.
    FocusPlanet(PlanetId),
    /// Centers a revoked protection world and opens a mission from its stationed fleet.
    OpenRevokedProtectionMission(PlanetId),
    /// Centers the strategic map on a Railgun target and zooms out as far as possible.
    FocusRailgunTarget(PlanetId),
    /// Centers the strategic map on a destroyed world without opening hidden information.
    FocusDestroyedPlanet(PlanetId),
    /// Centers the strategic map on an in-flight space-fauna encounter.
    FocusSpaceEncounter(MissionId),
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
        let mut message = message.into();
        if let Some(first) = message.get_mut(..1) {
            first.make_ascii_uppercase();
        }
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
                                | MessageAction::FocusDestroyedPlanet(_)
                                | MessageAction::FocusSpaceEncounter(_)
                        )
                    ) {
                        10.0
                    } else if matches!(
                        message.action,
                        Some(MessageAction::OpenRevokedProtectionMission(_))
                    ) {
                        f32::INFINITY
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
    missions: Option<Res<Missions>>,
    app_state: Option<Res<State<AppState>>>,
    game_state: Option<Res<State<GameState>>>,
    session: Option<Res<MultiplayerSession>>,
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
    let current_game_state = game_state.as_ref().map(|state| *state.get());
    let playing = in_game && current_game_state == Some(GameState::Playing);
    let combat_active = notifications_hidden_during_combat(in_game, current_game_state);
    if playing {
        if let (Some(map), Some(player)) = (&map, &player) {
            for planet in &map.planets {
                if revoked_protection_fleet(planet, player)
                    && !messages.0.iter().any(|message| {
                        message.action
                            == Some(MessageAction::OpenRevokedProtectionMission(planet.id))
                    })
                {
                    messages.push(
                        &MessageMsg::warning(format!(
                            "Protection access to {} {} was revoked.",
                            if planet.is_moon() {
                                "moon"
                            } else {
                                "planet"
                            },
                            planet.name
                        ))
                        .with_action(MessageAction::OpenRevokedProtectionMission(planet.id)),
                    );
                }
            }
        }
    }
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
            Some(MessageAction::OpenRevokedProtectionMission(id)) => {
                map.as_ref().zip(player.as_ref()).is_some_and(|(map, player)| {
                    map.try_get(id).is_some_and(|planet| revoked_protection_fleet(planet, player))
                })
            },
            Some(MessageAction::FocusRailgunTarget(id)) => {
                map.as_ref().is_some_and(|map| map.try_get(id).is_some())
            },
            Some(MessageAction::FocusDestroyedPlanet(id)) => map
                .as_ref()
                .is_some_and(|map| map.try_get(id).is_some_and(|planet| planet.is_destroyed)),
            Some(MessageAction::FocusSpaceEncounter(id)) => player.as_ref().is_some_and(|player| {
                space_encounter_position(id, missions.as_deref(), player).is_some()
            }),
            _ => true,
        };
        if matches!(
            message.action,
            Some(
                MessageAction::FocusColony(_)
                    | MessageAction::FocusPlanet(_)
                    | MessageAction::OpenRevokedProtectionMission(_)
                    | MessageAction::FocusRailgunTarget(_)
                    | MessageAction::FocusDestroyedPlanet(_)
                    | MessageAction::FocusSpaceEncounter(_)
            )
        ) {
            if !in_game || !actionable_planet_is_valid {
                return false;
            }
            if !playing {
                return true;
            }
        }
        if matches!(message.action, Some(MessageAction::OpenRevokedProtectionMission(_))) {
            true
        } else {
            advance_message_lifetime(message, elapsed, combat_active)
        }
    });
    if messages.0.is_empty() {
        return;
    }

    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    if let Some((index, action)) = draw_notifications(context, &messages, playing, combat_active) {
        if !matches!(action, MessageAction::OpenRevokedProtectionMission(_)) {
            messages.0.remove(index);
        }
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
                MessageAction::OpenTrade(trade_id) => {
                    if playing
                        && session.as_ref().zip(player.as_ref()).is_some_and(|(session, player)| {
                            session.active_game.as_ref().is_some_and(|game| {
                                session.trades.iter().any(|trade| {
                                    trade.id == trade_id
                                        && trade.turn == game.persisted.state.turn
                                        && trade.finalized
                                        && trade.participant(player.id).is_some()
                                })
                            })
                        })
                    {
                        state.trade_open = Some(trade_id);
                        state.trading_post_open = None;
                        state.planet_selected = None;
                    }
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
                MessageAction::OpenRevokedProtectionMission(planet_id) => {
                    if playing {
                        if let (Some(map), Some(player)) = (&map, &player) {
                            open_revoked_protection_mission(planet_id, map, player, state);
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
                MessageAction::FocusDestroyedPlanet(planet_id) => {
                    if playing {
                        if let Some(map) = &map {
                            focus_destroyed_planet(planet_id, map, state);
                        }
                    }
                },
                MessageAction::FocusSpaceEncounter(mission_id) => {
                    if playing {
                        if let Some(player) = &player {
                            focus_space_encounter(mission_id, missions.as_deref(), player, state);
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

fn revoked_protection_fleet(planet: &Planet, player: &Player) -> bool {
    !planet.is_destroyed
        && planet.controlled.is_some_and(|controller| {
            controller != player.id && player.protection_intel.get(&planet.id) == Some(&controller)
        })
        && !planet.allows_protection(player.id)
        && planet.army.protector(player.id).is_some_and(|army| army.has_army())
}

fn open_revoked_protection_mission(
    planet_id: PlanetId,
    map: &Map,
    player: &Player,
    state: &mut UiState,
) -> bool {
    let Some(planet) =
        map.try_get(planet_id).filter(|planet| revoked_protection_fleet(planet, player))
    else {
        return false;
    };
    let Some(home) = map.try_get(player.home_planet).filter(|home| !home.is_destroyed) else {
        return false;
    };
    state.planet_selected = None;
    state.focus_planet = Some(planet.id);
    state.focus_zoom = None;
    state.to_selected = true;
    state.mission = true;
    state.mission_tab = MissionTab::NewMission;
    state.mission_info = Mission {
        origin: planet.id,
        destination: home.id,
        objective: Icon::Deploy,
        ..default()
    };
    state.combat_report = None;
    true
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
    hidden_for_combat: bool,
) -> Option<(usize, MessageAction)> {
    if hidden_for_combat {
        return None;
    }

    let mut clicked_message = None;
    show_notification_area(
        context,
        "stellarion_notifications",
        playing,
        MAX_NOTIFICATION_WIDTH,
        |ui| {
            // Leave space for the outer anchor, frame margins, and border on narrow windows.
            ui.set_max_width(
                MAX_NOTIFICATION_WIDTH.min((context.content_rect().width() - 50.0).max(0.0)),
            );
            // Each frame measures only its own label; the stack shares a right edge, not a width.
            for (index, message) in messages.0.iter().enumerate() {
                if !playing
                    && matches!(
                        message.action,
                        Some(
                            MessageAction::FocusColony(_)
                                | MessageAction::FocusPlanet(_)
                                | MessageAction::OpenRevokedProtectionMission(_)
                                | MessageAction::FocusRailgunTarget(_)
                                | MessageAction::FocusDestroyedPlanet(_)
                                | MessageAction::FocusSpaceEncounter(_)
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
        },
    );
    clicked_message
}

/// Returns whether combat currently owns the complete game viewport.
const fn notifications_hidden_during_combat(in_game: bool, game_state: Option<GameState>) -> bool {
    in_game && matches!(game_state, Some(GameState::CombatMenu | GameState::Combat))
}

/// Advances a toast only while it is eligible to be shown.
fn advance_message_lifetime(
    message: &mut ActiveMessage,
    elapsed: f32,
    paused_for_combat: bool,
) -> bool {
    if paused_for_combat {
        true
    } else {
        message.remaining_seconds -= elapsed;
        message.remaining_seconds > 0.0
    }
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
    state.focus_position = None;
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
    state.focus_position = None;
    state.focus_zoom = Some(MAX_ZOOM);
    state.to_selected = true;
    state.mission = false;
    state.combat_report = None;
    true
}

/// Centers and fully zooms out on a destroyed world while keeping its information hidden.
fn focus_destroyed_planet(planet_id: PlanetId, map: &Map, state: &mut UiState) -> bool {
    let Some(planet) = map.try_get(planet_id).filter(|planet| planet.is_destroyed) else {
        return false;
    };
    state.planet_selected = None;
    state.focus_planet = Some(planet.id);
    state.focus_position = None;
    state.focus_zoom = Some(MAX_ZOOM);
    state.to_selected = true;
    state.mission = false;
    state.combat_report = None;
    true
}

fn space_encounter_position(
    mission_id: MissionId,
    missions: Option<&Missions>,
    player: &Player,
) -> Option<Vec2> {
    missions.and_then(|missions| missions.get(mission_id)).map(|mission| mission.position).or_else(
        || {
            player
                .reports
                .iter()
                .rev()
                .find(|report| report.mission.id == mission_id && report.is_space_fauna_encounter())
                .map(|report| report.mission.position)
        },
    )
}

fn focus_space_encounter(
    mission_id: MissionId,
    missions: Option<&Missions>,
    player: &Player,
    state: &mut UiState,
) -> bool {
    let Some(position) = space_encounter_position(mission_id, missions, player) else {
        return false;
    };
    state.planet_selected = None;
    state.focus_planet = None;
    state.focus_position = Some(position);
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
        app.init_resource::<Messages>().add_systems(
            EguiPrimaryContextPass,
            check_messages.after(crate::core::ui::systems::draw_ui),
        );
    }
}

#[cfg(test)]
#[path = "../../tests/core/messages.rs"]
mod tests;
