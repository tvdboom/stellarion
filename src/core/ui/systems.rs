//! Egui systems for the strategic HUD, shops, missions, and combat reports.

use std::collections::{BTreeMap, HashMap, HashSet};

use bevy::prelude::*;
use bevy_egui::egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::{
    emath, Align, Align2, Color32, ComboBox, CursorIcon, FontData, FontFamily, Layout, Order,
    Response, RichText, ScrollArea, Sense, Separator, Slider, Stroke, StrokeKind, TextStyle, Ui,
    UiBuilder,
};
use bevy_egui::{egui, EguiContexts, EguiTextureHandle};
use itertools::Itertools;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::core::assets::WorldAssets;
use crate::core::audio::{set_ui_sound, SoundEffect};
#[cfg(test)]
use crate::core::combat::report::RoundReport;
use crate::core::combat::report::{
    combat_fleet_strength, combat_strength_ranges, MissionReport, ReportId, Side,
};
#[cfg(test)]
use crate::core::combat::resolution::CombatUnit;
use crate::core::combat::stats::CombatStats;
use crate::core::constants::{
    BG2_COLOR, HEALTH_COLOR, HEIGHT, HOME_CROWN_INDICES, HOME_CROWN_VERTICES, HOME_PLANET_COLOR,
    SHIELD_COLOR, TERRAFORMER_FOCUS_BONUS_PERCENT_PER_LEVEL,
    TERRAFORMER_OTHER_PENALTY_PERCENT_PER_LEVEL, WIDTH,
};
use crate::core::energy::EnergyGrid;
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId, PlanetKind, SolarBand};
use crate::core::map::systems::select_planet;
use crate::core::messages::MessageMsg;
use crate::core::missions::{
    BombingRaid, JointAttackMission, JointMissionLaunch, Mission, MissionId, Missions,
    RecallMissionMsg, RecallProtectionMsg, SendMissionMsg,
};
use crate::core::orders::{purchase_limit, validate_mission};
use crate::core::player::{PlanetInfo, Player};
use crate::core::recycling::{debris_sites, recycler_sources};
use crate::core::resources::{ResourceName, Resources};
use crate::core::settings::Settings;
use crate::core::simulation::{
    orbital_railgun_destruction_basis_points, orbital_railgun_fire_cost,
    orbital_railgun_fire_energy_cost, orbital_railgun_origins, JointAttackContribution,
    TurnCommand,
};
use crate::core::states::GameState;
use crate::core::trading::{visible_trading_post_owner, ResourceLoanTerm};
use crate::core::ui::aesthetics::Aesthetics;
use crate::core::ui::dark::NordDark;
use crate::core::ui::utils::{toggle, CustomResponse, CustomUi, ImageIds};
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::fauna::SpaceFauna;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Description, Price, Unit};
use crate::multiplayer::client::{
    MultiplayerRequest, MultiplayerSession, PendingTurnCommands, COMMAND_LIMIT_REACHED_MESSAGE,
};
use crate::multiplayer::model::{
    GameMembership, JointAttackInvitation, JointAttackParticipant, JointAttackResponse,
};
use crate::utils::{format_thousands, FmtNumb, NameFromEnum, SafeDiv, ToColor32};

mod combat;
use combat::{CombatRoundView, CombatStatistics};

mod missions;
use missions::{
    draw_joint_attack_notifications, draw_mission, mission_arrival_tooltip, mission_arrival_turn,
    mission_movement_tooltip,
};
mod shop;
use shop::draw_shop;
mod trading;
use trading::draw_trade_notifications;

#[derive(Component)]
/// Marker for entities owned by the in-game UI projection.
pub struct UiCmp;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// Selected constructible-unit category in the local shop panel.
pub enum Shop {
    #[default]
    Buildings,
    /// Planet-only orbital infrastructure.
    Orbitals,
    Fleet,
    Defenses,
}

impl Shop {
    /// Returns the category reached by moving one tab to the right.
    pub(crate) fn next(self, is_moon: bool) -> Self {
        match self {
            Self::Buildings if is_moon => Self::Fleet,
            Self::Buildings => Self::Orbitals,
            Self::Orbitals => Self::Fleet,
            Self::Fleet if is_moon => Self::Buildings,
            Self::Fleet => Self::Defenses,
            Self::Defenses => Self::Buildings,
        }
    }

    /// Returns the category reached by moving one tab to the left.
    pub(crate) fn previous(self, is_moon: bool) -> Self {
        match self {
            Self::Buildings if is_moon => Self::Fleet,
            Self::Buildings => Self::Defenses,
            Self::Orbitals => Self::Buildings,
            Self::Fleet if is_moon => Self::Buildings,
            Self::Fleet => Self::Orbitals,
            Self::Defenses => Self::Fleet,
        }
    }
}

#[derive(EnumIter, Copy, Clone, Debug, Default, PartialEq)]
/// Selected section of the local mission panel.
pub enum MissionTab {
    #[default]
    NewMission,
    ActiveMissions,
    EnemyMissions,
    MissionReports,
}

#[derive(Resource, Default)]
/// Local-only panel, selection, hover, and report navigation state.
pub struct UiState {
    pub planet_hover: Option<PlanetId>,
    /// World-list hover, refreshed each UI pass independently of map picking events.
    pub(crate) world_shortcut_hover: Option<PlanetId>,
    /// Owned Jump Gate whose live network is currently being previewed.
    pub(crate) jump_gate_hover: Option<PlanetId>,
    /// Infrastructure marker whose strategic-map range is currently being previewed.
    pub(crate) range_preview: Option<MapRangePreview>,
    /// Mission-panel world hover, which previews known units without map or planet details.
    pub(crate) mission_planet_hover: Option<PlanetId>,
    pub planet_selected: Option<PlanetId>,
    /// Planet awaiting confirmation before its abandon command is added to the turn draft.
    pub(crate) abandon_confirmation: Option<PlanetId>,
    /// Controlled planet awaiting confirmation before colonization is added to the turn draft.
    pub(crate) colonize_confirmation: Option<PlanetId>,
    /// Controlled world whose immediate protection-access modal is open.
    pub(crate) protection_access: Option<PlanetId>,
    /// Players hidden from each world's combined garrison overview. Missing entries mean all
    /// stationed players are included, so newly arriving fleets are visible by default.
    pub(crate) garrison_hidden_players: HashMap<PlanetId, HashSet<PlayerId>>,
    /// Trading Post marker whose bilateral commerce panel is open.
    pub(crate) trading_post_open: Option<PlanetId>,
    /// Existing trade negotiation currently open in the bilateral commerce panel.
    pub(crate) trade_open: Option<u64>,
    /// Trade identifier whose local resource controls are currently initialized.
    pub(crate) trade_draft_id: Option<u64>,
    /// Resources selected for the local side of a Trading Post negotiation.
    pub(crate) trade_resources: Resources,
    /// Resources selected for a new loan from the player's own Trading Post.
    pub(crate) resource_hub_resources: Resources,
    /// Repayment schedule selected in the Resource Hub panel.
    pub(crate) resource_hub_term: ResourceLoanTerm,
    /// Camera-only world focus used by shortcuts that must not open a world panel.
    pub focus_planet: Option<PlanetId>,
    /// Camera-only coordinate focus used by transient in-flight encounter notifications.
    pub(crate) focus_position: Option<Vec2>,
    /// Optional orthographic scale approached while moving to a camera-only world focus.
    pub(crate) focus_zoom: Option<f32>,
    pub to_selected: bool,
    pub shop: Shop,
    pub lab: (ResourceName, ResourceName),
    pub lab_amount: usize,
    /// Target awaiting confirmation before all in-range Orbital Railguns are committed.
    pub(crate) railgun_confirmation: Option<PlanetId>,
    pub mission: bool,
    pub mission_tab: MissionTab,
    pub mission_info: Mission,
    /// Player slots selected for the current joint-attack invitation.
    pub(crate) joint_attack_invitees: std::collections::BTreeSet<PlayerId>,
    /// Last drawn invite picker bounds and UI pass, for crossing from the icon into the panel.
    pub(crate) joint_attack_invite_panel_rect: Option<(u64, egui::Rect)>,
    /// Whether the current mission is being coordinated with invited players.
    pub(crate) allied_mission: bool,
    /// Invitation linked to the current editable mission draft.
    pub(crate) joint_attack_draft_id: Option<u64>,
    /// Preserved owner draft, independent of planet selection while looking around the map.
    pub(crate) joint_attack_owner_draft: Option<Mission>,
    /// Shared route underlying the owner's current local draft.
    pub(crate) joint_attack_owner_shared_route: Option<(usize, Icon)>,
    /// Prevents multiple UI passes from withdrawing the same owner acceptance twice.
    pub(crate) joint_attack_owner_withdrawal: Option<(u64, u64)>,
    /// Invitation currently opened from its persistent notification.
    pub(crate) joint_attack_open: Option<u64>,
    /// Origin and units being prepared in the invitation response modal.
    pub(crate) joint_attack_contribution: Mission,
    /// Invitation whose draft has been restored into the response editor.
    pub(crate) joint_attack_loaded: Option<u64>,
    /// Queues one short confirmation after publishing an owner or participant proposal.
    pub(crate) joint_attack_proposal_notice: bool,
    pub jump_gate_history: bool,
    pub mission_hover: Option<MissionId>,
    /// UI hover expires each pass; map hover persists until a picking event changes it.
    pub(crate) mission_hover_from_ui: bool,
    pub mission_report: Option<MissionId>,
    pub combat_report: Option<ReportId>,
    pub combat_report_total: bool,
    pub combat_report_round: usize,
    pub combat_report_hover: Option<(Unit, Side)>,
    pub in_combat: Option<ReportId>,
    /// Local replay presentation; schematic remains the default for every new game.
    pub(crate) combat_view: crate::core::combat::cinematic_ui::CombatView,
    pub combat_round: usize,
    pub end_turn: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
enum NegotiationNotice {
    MissionCanceled(u64),
    MissionRejected(u64, PlayerId),
    TradeClosed(u64),
}

#[derive(Clone, Default)]
struct NegotiationNoticeHistory {
    game: Option<crate::core::identity::GameId>,
    seen: HashSet<(PlayerId, NegotiationNotice)>,
}

/// Notification history outlives UiState, which is rebuilt when changing practice players.
/// Scope it to the current game so each recipient sees each outcome once, even during reloads.
fn first_negotiation_notice(
    context: &egui::Context,
    session: &MultiplayerSession,
    player: PlayerId,
    notice: NegotiationNotice,
) -> bool {
    context.data_mut(|data| {
        let history = data.get_temp_mut_or_default::<NegotiationNoticeHistory>(egui::Id::new(
            "negotiation_notice_history",
        ));
        let game = session.active_game.as_ref().map(|game| &game.id);
        if history.game.as_ref() != game {
            history.game = game.cloned();
            history.seen.clear();
        }
        history.seen.insert((player, notice))
    })
}

impl UiState {
    /// Uses the world-list row under the pointer for the same preview as a map planet.
    pub(crate) fn hovered_planet(&self) -> Option<PlanetId> {
        self.world_shortcut_hover.or(self.planet_hover)
    }

    /// Prevents readiness while shared choices still require agreement.
    pub(crate) fn end_turn_blocked(
        &self,
        session: Option<&MultiplayerSession>,
        pending: &PendingTurnCommands,
    ) -> bool {
        // Practice resolves all local empires together and declines unfinished proposals.
        if session.is_some_and(|session| session.local_practice) {
            return false;
        }
        if !matches!(
            pending.submission,
            crate::multiplayer::client::SubmissionState::Draft
                | crate::multiplayer::client::SubmissionState::Retry
        ) {
            return false;
        }
        self.allied_mission
            || self.joint_attack_open.is_some()
            || self.trading_post_open.is_some()
            || session.is_some_and(|session| session.has_open_negotiation(pending))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Hover-only range previews exposed by stationary strategic-map infrastructure markers.
pub(crate) enum MapRangePreview {
    SensorPhalanx(PlanetId),
    OrbitalRailgun(PlanetId),
    TradingPost(PlanetId),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanetPanelMode {
    Full,
    UnitsOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ConfirmationAction {
    Confirm,
    Cancel,
}

const ABANDON_CONFIRMATION_TEXT_COLOR: Color32 = Color32::from_rgb(166, 188, 211);
const ABANDON_CONFIRMATION_BUTTON_FILL: Color32 = Color32::from_rgb(18, 28, 39);
const MODAL_ICON_SIZE: f32 = 44.0;
const MODAL_ICON_TOP_INSET: f32 = 32.0;
const MODAL_ICON_RIGHT_INSET: f32 = 18.0;
const MODAL_HEADER_HEIGHT: f32 = 54.0;
const MODAL_BUTTON_HEIGHT: f32 = 40.0;
const RAILGUN_CONFIRMATION_HEIGHT: f32 = 300.0;
const PROTECTION_PLAYER_ROW_WIDTH_FRACTION: f32 = 0.44;
const PROTECTION_PLAYER_ROW_MIN_WIDTH: f32 = 176.0;
const PROTECTION_PLAYER_ROW_MAX_WIDTH: f32 = 240.0;
const PLANET_ACTION_ICON_IDLE_TINT: Color32 = Color32::from_gray(205);
const PLANET_ACTION_ICON_HOVER_TINT: Color32 = Color32::from_gray(245);
const PLANET_ACTION_ICON_PRESSED_TINT: Color32 = Color32::from_gray(165);

/// Gives compact planet actions a visible rest, hover, and pointer-down response.
fn planet_action_icon_tint(response: &Response) -> Color32 {
    if response.is_pointer_button_down_on() {
        PLANET_ACTION_ICON_PRESSED_TINT
    } else if response.hovered() {
        PLANET_ACTION_ICON_HOVER_TINT
    } else {
        PLANET_ACTION_ICON_IDLE_TINT
    }
}

/// Keeps modal content away from the ornamental edges of the shared panel artwork.
fn modal_content_rect(panel: egui::Rect) -> egui::Rect {
    let horizontal = (panel.width() * 0.07).clamp(12.0, 34.0);
    let vertical = (panel.height() * 0.07).clamp(10.0, 24.0);
    panel.shrink2(egui::vec2(horizontal, vertical))
}

/// Centers and paints the shared panel behind an input-blocking modal's custom contents.
fn show_panel_modal<R>(
    context: &egui::Context,
    images: &ImageIds,
    modal_id: egui::Id,
    size: egui::Vec2,
    content: impl FnOnce(&mut Ui, egui::Rect, egui::Rect) -> R,
) -> egui::ModalResponse<R> {
    show_panel_modal_with_offset(context, images, modal_id, size, egui::Vec2::ZERO, content)
}

/// Keeps a mission modal centered while its entire interactive layer is scaled.
fn scaled_modal<R>(
    context: &egui::Context,
    images: &ImageIds,
    modal_id: egui::Id,
    size: egui::Vec2,
    scale: f32,
    content: impl FnOnce(&mut Ui, egui::Rect, egui::Rect) -> R,
) -> egui::ModalResponse<R> {
    scaled_modal_with_offset(context, images, modal_id, size, scale, egui::Vec2::ZERO, content)
}

fn scaled_modal_with_offset<R>(
    context: &egui::Context,
    images: &ImageIds,
    modal_id: egui::Id,
    size: egui::Vec2,
    scale: f32,
    visual_offset: egui::Vec2,
    content: impl FnOnce(&mut Ui, egui::Rect, egui::Rect) -> R,
) -> egui::ModalResponse<R> {
    context.set_transform_layer(
        egui::LayerId::new(Order::Foreground, modal_id),
        egui::emath::TSTransform::from_scaling(scale),
    );
    let offset = context.content_rect().center().to_vec2() * (1.0 / scale - 1.0) + visual_offset;
    show_panel_modal_unscaled(context, images, modal_id, size, scale, offset, content)
}

/// Applies the viewport scale before placing a shared modal at the viewport center.
fn show_panel_modal_with_offset<R>(
    context: &egui::Context,
    images: &ImageIds,
    modal_id: egui::Id,
    size: egui::Vec2,
    offset: egui::Vec2,
    content: impl FnOnce(&mut Ui, egui::Rect, egui::Rect) -> R,
) -> egui::ModalResponse<R> {
    scaled_modal_with_offset(
        context,
        images,
        modal_id,
        size,
        game_modal_scale(context.content_rect().size(), size),
        offset,
        content,
    )
}

fn show_panel_modal_unscaled<R>(
    context: &egui::Context,
    images: &ImageIds,
    modal_id: egui::Id,
    size: egui::Vec2,
    scale: f32,
    offset: egui::Vec2,
    content: impl FnOnce(&mut Ui, egui::Rect, egui::Rect) -> R,
) -> egui::ModalResponse<R> {
    let content_rect = context.content_rect();
    let logical_viewport =
        egui::Rect::from_min_max(content_rect.min / scale, content_rect.max / scale);
    let panel_offset = content_rect.center() - size * 0.5 - content_rect.min + offset;
    let area = egui::Modal::default_area(modal_id)
        .anchor(Align2::LEFT_TOP, panel_offset)
        .constrain_to(logical_viewport);
    egui::Modal::new(modal_id).area(area).frame(egui::Frame::NONE).show(context, |ui| {
        ui.set_clip_rect(logical_viewport);
        let (panel, _) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().image(
            images.get("panel"),
            panel,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        content(ui, panel, modal_content_rect(panel))
    })
}

/// Paints a shared modal header with a truly centered title and a corner action icon.
fn draw_modal_header(
    ui: &mut Ui,
    panel: egui::Rect,
    content: egui::Rect,
    title: impl Into<RichText>,
    icon: egui::TextureId,
) -> egui::Rect {
    let height = MODAL_HEADER_HEIGHT.min(content.height());
    let header = egui::Rect::from_min_size(content.min, egui::vec2(content.width(), height));
    let icon_size = MODAL_ICON_SIZE.min(header.width() * 0.18);
    let icon_rect = egui::Rect::from_min_size(
        egui::pos2(
            panel.right() - MODAL_ICON_RIGHT_INSET - icon_size,
            panel.top() + MODAL_ICON_TOP_INSET,
        ),
        egui::vec2(icon_size, icon_size),
    );
    ui.painter().image(
        icon,
        icon_rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    // Reserve the same space on both sides so the title remains centered in the panel rather
    // than merely centered in the area left of the icon.
    let reserve = icon_size + 10.0;
    let title_rect = header.shrink2(egui::vec2(reserve.min(header.width() * 0.25), 0.0));
    ui.scope_builder(UiBuilder::new().max_rect(title_rect), |ui| {
        ui.centered_and_justified(|ui| {
            ui.label(title.into());
        });
    });
    header
}

/// Draws a modal action inside a measured rectangle, including compact footers.
fn draw_modal_button(ui: &mut Ui, rect: egui::Rect, label: &str, enabled: bool) -> Response {
    ui.add_enabled_ui(enabled, |ui| {
        ui.put(
            rect,
            egui::Button::new(
                RichText::new(label).size(17.0).strong().color(ABANDON_CONFIRMATION_TEXT_COLOR),
            ),
        )
    })
    .inner
    .on_hover_cursor(CursorIcon::PointingHand)
}

/// Compact response badge shared by trade offers and joint attack contributions.
fn draw_status_badge(ui: &mut Ui, status: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.12))
        .stroke(Stroke::new(1.0, color.gamma_multiply(0.65)))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.label(RichText::new(status).size(13.0).strong().color(color));
        });
}

/// Applies the restrained dark button treatment shared by confirmation modal footers.
fn style_modal_buttons(ui: &mut Ui) {
    let widgets = &mut ui.style_mut().visuals.widgets;
    for (visuals, fill, stroke) in [
        (&mut widgets.inactive, ABANDON_CONFIRMATION_BUTTON_FILL, Color32::from_rgb(74, 99, 122)),
        (&mut widgets.hovered, Color32::from_rgb(31, 47, 61), Color32::from_rgb(123, 158, 188)),
        (&mut widgets.active, Color32::from_rgb(39, 94, 123), Color32::from_rgb(139, 183, 216)),
    ] {
        visuals.bg_fill = fill;
        visuals.weak_bg_fill = fill;
        visuals.bg_stroke = Stroke::new(1.0, stroke);
        visuals.fg_stroke.width = 1.0;
        visuals.corner_radius = egui::CornerRadius::same(6);
        visuals.expansion = 0.0;
    }
}

/// Keeps selectors and their keyboard-edit fields in the same dark modal palette.
fn style_selection_boxes(ui: &mut Ui) {
    style_modal_buttons(ui);
    ui.visuals_mut().text_edit_bg_color = Some(ABANDON_CONFIRMATION_BUTTON_FILL);
}

/// Draws a centered Yes/No footer entirely inside the supplied modal region.
fn draw_confirmation_buttons(
    ui: &mut Ui,
    footer: egui::Rect,
    confirm_enabled: bool,
) -> Option<ConfirmationAction> {
    let gap = 12.0_f32.min(footer.width() * 0.05);
    let width = 96.0_f32.min(((footer.width() - gap) * 0.5).max(1.0));
    let row_width = width * 2.0 + gap;
    let left = footer.center().x - row_width * 0.5;
    let yes_rect = egui::Rect::from_min_size(
        egui::pos2(left, footer.center().y - MODAL_BUTTON_HEIGHT * 0.5),
        egui::vec2(width, MODAL_BUTTON_HEIGHT),
    );
    let no_rect = yes_rect.translate(egui::vec2(width + gap, 0.0));
    let button = |label| {
        egui::Button::new(
            RichText::new(label).size(17.0).strong().color(ABANDON_CONFIRMATION_TEXT_COLOR),
        )
    };

    let mut action = None;
    ui.scope(|ui| {
        style_modal_buttons(ui);
        let yes = ui.add_enabled_ui(confirm_enabled, |ui| ui.put(yes_rect, button("Yes"))).inner;
        if yes.enabled() && yes.on_hover_cursor(CursorIcon::PointingHand).clicked() {
            action = Some(ConfirmationAction::Confirm);
        }
        if ui.put(no_rect, button("No")).on_hover_cursor(CursorIcon::PointingHand).clicked() {
            action = Some(ConfirmationAction::Cancel);
        }
    });
    action
}

fn visible_planet_panel(state: &UiState) -> Option<(PlanetId, PlanetPanelMode)> {
    state
        .mission_planet_hover
        .map(|id| (id, PlanetPanelMode::UnitsOnly))
        .or_else(|| state.hovered_planet().map(|id| (id, PlanetPanelMode::Full)))
}

/// Combat details replace the mission panel while retaining its state for the close action.
fn mission_panel_visible(state: &UiState) -> bool {
    state.mission && state.combat_report.is_none()
}

/// Hover placement follows the pointer independently of the selected mission origin.
fn planet_hover_panel_target(
    state: &UiState,
    cursor_x: Option<f32>,
    viewport_width: f32,
) -> Option<PlanetPanelSlideTarget> {
    visible_planet_panel(state).map(|(id, mode)| PlanetPanelSlideTarget {
        id,
        mode,
        right_side: cursor_x.is_some_and(|x| x < viewport_width * 0.5),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PlanetPanelSlideTarget {
    id: PlanetId,
    mode: PlanetPanelMode,
    right_side: bool,
}

#[derive(Default)]
pub(crate) struct PlanetPanelSlide {
    target: Option<PlanetPanelSlideTarget>,
    elapsed: f32,
}

impl PlanetPanelSlide {
    fn update(
        &mut self,
        target: Option<PlanetPanelSlideTarget>,
        delta_seconds: f32,
    ) -> Option<(PlanetPanelSlideTarget, f32)> {
        let delta_seconds = delta_seconds.max(0.0);

        if let Some(target) = target {
            if self.target != Some(target) {
                self.target = Some(target);
                self.elapsed = 0.0;
            } else {
                self.elapsed = (self.elapsed + delta_seconds).min(PLANET_PANEL_TOTAL_DURATION);
            }
        } else if self.elapsed - delta_seconds <= f32::EPSILON {
            self.elapsed = 0.0;
            self.target = None;
        } else {
            self.elapsed -= delta_seconds;
        }

        self.target.map(|target| {
            let progress = (self.elapsed / PLANET_PANEL_SLIDE_DURATION).min(1.0);
            (target, progress)
        })
    }

    fn detail_progress(&self, line: usize) -> f32 {
        if self.elapsed >= PLANET_PANEL_TOTAL_DURATION {
            return 1.0;
        }
        let start = PLANET_PANEL_SLIDE_DURATION + line as f32 * PLANET_DETAIL_LINE_STAGGER;
        ((self.elapsed - start) / PLANET_DETAIL_LINE_DURATION).clamp(0.0, 1.0)
    }

    fn is_animating(&self) -> bool {
        self.target.is_some() && self.elapsed < PLANET_PANEL_TOTAL_DURATION
    }

    fn skip_detail_exit(&mut self) {
        // Detail rows have already left the interaction path. Clamp straight to the panel portion
        // so an incoming mission card makes the planet card move on this frame, not after the
        // staggered detail timeline has first unwound.
        self.elapsed = self.elapsed.min(PLANET_PANEL_SLIDE_DURATION);
    }

    fn hide(&mut self) {
        self.target = None;
        self.elapsed = 0.0;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct MissionHoverPanelSlideTarget {
    id: MissionId,
    from_ui: bool,
    right_side: bool,
}

#[derive(Default)]
pub(crate) struct MissionHoverPanelSlide {
    target: Option<MissionHoverPanelSlideTarget>,
    elapsed: f32,
}

impl MissionHoverPanelSlide {
    fn update(
        &mut self,
        target: Option<MissionHoverPanelSlideTarget>,
        delta_seconds: f32,
    ) -> Option<(MissionHoverPanelSlideTarget, f32)> {
        let delta_seconds = delta_seconds.max(0.0);

        if let Some(target) = target {
            if self.target != Some(target) {
                self.target = Some(target);
                self.elapsed = 0.0;
            } else {
                self.elapsed = (self.elapsed + delta_seconds).min(PLANET_PANEL_SLIDE_DURATION);
            }
        } else if self.elapsed - delta_seconds <= f32::EPSILON {
            self.elapsed = 0.0;
            self.target = None;
        } else {
            // There is intentionally no hover hold here: PointerOut starts the exit immediately.
            self.elapsed -= delta_seconds;
        }

        self.target.map(|target| {
            let progress = (self.elapsed / PLANET_PANEL_SLIDE_DURATION).min(1.0);
            (target, progress)
        })
    }

    fn is_animating(&self) -> bool {
        self.target.is_some() && self.elapsed < PLANET_PANEL_SLIDE_DURATION
    }

    fn hide(&mut self) {
        self.target = None;
        self.elapsed = 0.0;
    }
}

#[derive(Default)]
pub(crate) struct PlanetPanelHoverHold {
    target: Option<PlanetPanelSlideTarget>,
    pending_target: Option<PlanetPanelSlideTarget>,
    pending_elapsed: f32,
    panel_rects: [Option<egui::Rect>; 2],
    remaining: f32,
}

impl PlanetPanelHoverHold {
    fn update(
        &mut self,
        direct_target: Option<PlanetPanelSlideTarget>,
        pointer: Option<egui::Pos2>,
        delta_seconds: f32,
    ) -> Option<PlanetPanelSlideTarget> {
        let over_panel = pointer.is_some_and(|pointer| {
            self.panel_rects.iter().flatten().any(|rect| rect.contains(pointer))
        });

        // The UI panel owns pointer intent even if map picking still reports a planet behind it.
        // This also makes the final step onto an action button cancel any pending hover switch.
        if self.target.is_some() && over_panel {
            self.pending_target = None;
            self.pending_elapsed = 0.0;
            self.remaining = PLANET_PANEL_HOVER_HOLD_DURATION;
            return self.target;
        }

        if let Some(target) = direct_target {
            if target.mode == PlanetPanelMode::Full {
                // Once a full panel opens for one world, keep its side fixed while the pointer
                // travels from the world into the panel. Recomputing the side at the viewport
                // midpoint makes controls jump away before they can be reached.
                if let Some(current) = self
                    .target
                    .filter(|current| current.id == target.id && current.mode == target.mode)
                {
                    self.target = Some(current);
                    self.pending_target = None;
                    self.pending_elapsed = 0.0;
                } else if self.target.is_none() {
                    self.target = Some(target);
                } else {
                    // Crossing another planet on the way to the open panel must not steal it.
                    // A deliberate switch still works after the pointer rests on the new planet.
                    if self.pending_target.is_some_and(|pending| {
                        pending.id == target.id && pending.mode == target.mode
                    }) {
                        self.pending_elapsed += delta_seconds.max(0.0);
                    } else {
                        self.pending_target = Some(target);
                        self.pending_elapsed = 0.0;
                    }

                    if self.pending_elapsed >= PLANET_PANEL_HOVER_SWITCH_DELAY {
                        self.target = self.pending_target.take();
                        self.pending_elapsed = 0.0;
                    }
                }
                self.remaining = PLANET_PANEL_HOVER_HOLD_DURATION;
            } else {
                // Mission links are transient unit previews and must not revive a previous map
                // hover once their own pointer leaves.
                self.clear();
                return Some(target);
            }
        } else if self.target.is_some() {
            self.pending_target = None;
            self.pending_elapsed = 0.0;
            self.remaining = (self.remaining - delta_seconds.max(0.0)).max(0.0);
            if self.remaining == 0.0 {
                self.clear();
            }
        }

        self.target
    }

    fn set_panel_rects(&mut self, panel_rects: [Option<egui::Rect>; 2]) {
        self.panel_rects = panel_rects;
    }

    fn clear(&mut self) {
        self.target = None;
        self.pending_target = None;
        self.pending_elapsed = 0.0;
        self.panel_rects = [None; 2];
        self.remaining = 0.0;
    }
}

fn include_planet_panel_rect(panel_rects: &mut [Option<egui::Rect>; 2], rect: Option<egui::Rect>) {
    if let (Some(slot), Some(rect)) = (panel_rects.iter_mut().find(|slot| slot.is_none()), rect) {
        *slot = Some(rect);
    }
}

const PLANET_PANEL_SLIDE_DURATION: f32 = 0.22;
const PLANET_PANEL_HOVER_HOLD_DURATION: f32 = 0.8;
const PLANET_PANEL_HOVER_SWITCH_DELAY: f32 = 0.15;
const PLANET_DETAIL_LINE_DURATION: f32 = 0.12;
const PLANET_DETAIL_LINE_STAGGER: f32 = 0.04;
const PLANET_DETAIL_LINE_COUNT: usize = 4;
const PLANET_PANEL_TOTAL_DURATION: f32 = PLANET_PANEL_SLIDE_DURATION
    + PLANET_DETAIL_LINE_STAGGER * (PLANET_DETAIL_LINE_COUNT - 1) as f32
    + PLANET_DETAIL_LINE_DURATION;

/// Returns the horizontal remainder of a fast cubic ease-out from the viewport edge.
fn planet_panel_slide_offset(progress: f32, right_side: bool, distance: f32) -> f32 {
    let remaining = (1.0 - progress.clamp(0.0, 1.0)).powi(3) * distance.max(0.0);
    if right_side {
        remaining
    } else {
        -remaining
    }
}

/// Lays text out at its final position while painting it as a clipped horizontal entrance.
fn draw_sliding_text(ui: &mut Ui, text: RichText, progress: f32, right_side: bool) -> Response {
    let (position, galley, response) = egui::Label::new(text).selectable(false).layout_in_ui(ui);
    let offset = planet_panel_slide_offset(progress, right_side, galley.size().x);
    let text_color = ui.visuals().text_color();
    ui.painter().with_clip_rect(response.rect).add(egui::epaint::TextShape::new(
        position + egui::vec2(offset, 0.0),
        galley,
        text_color,
    ));
    response
}

const MISSION_HOVER_FLEET_WIDTH: f32 = 110.0;
const MISSION_HOVER_INFO_WIDTH: f32 = 330.0;
const MISSION_HOVER_INFO_BASE_HEIGHT: f32 = 280.0;
const MISSION_HOVER_ALLIED_HEADER_HEIGHT: f32 = 48.0;
const MISSION_HOVER_ALLIED_ROW_HEIGHT: f32 = 24.0;
const MISSION_HOVER_ALLIED_ROW_INSET: f32 = 52.0;
const MISSION_HOVER_ALLIED_FONT_SIZE: f32 = 16.0;
const MISSION_HOVER_ALLIED_NAME_ICON_GAP: f32 = 10.0;
const MISSION_HOVER_ALLIED_ICON_SIZE: f32 = 16.0;
const MISSION_HOVER_ALLIED_ICON_VALUE_GAP: f32 = 4.0;
const MISSION_HOVER_PANEL_GAP: f32 = 1.0;
const PLANET_UNITS_PANEL_WIDTH: f32 = 270.0;
const MOON_UNITS_PANEL_WIDTH: f32 = 145.0;
const WORLD_UNITS_PANEL_BASE_HEIGHT: f32 = 630.0;
const GARRISON_FILTER_HEIGHT: f32 = 20.0;
const GARRISON_FILTER_GAP: f32 = 2.0;
const GARRISON_FILTER_FONT_SIZE: f32 = 16.0;
const WORLD_OVERVIEW_HEADER_HEIGHT: f32 = 24.0;
const WORLD_OVERVIEW_ICON_SIZE: f32 = 20.0;
const WORLD_OVERVIEW_STRENGTH_FONT_SIZE: f32 = 20.0;
const WORLD_OVERVIEW_ICON_TEXT_GAP: f32 = 4.0;
const WORLD_OVERVIEW_PROTECTION_GAP: f32 = 8.0;
const WORLD_OVERVIEW_BADGE_GAP: f32 = 10.0;
const WORLD_OVERVIEW_RIGHT_MARGIN: f32 = 14.0;

const HUD_PANEL_FILL: Color32 = Color32::from_rgba_unmultiplied_const(10, 16, 23, 226);
const HUD_PANEL_STROKE: Color32 = Color32::from_rgba_unmultiplied_const(130, 170, 215, 95);
const HUD_REFERENCE_WIDTH: f32 = 1280.0;
const HUD_REFERENCE_HEIGHT: f32 = 720.0;
const HUD_MIN_SCALE: f32 = 0.8;
const HUD_MAX_SCALE: f32 = 1.6;

/// Scales the strategic HUD with the limiting viewport dimension, within readable bounds.
pub(crate) fn strategic_hud_scale(viewport: egui::Vec2) -> f32 {
    (viewport.x / HUD_REFERENCE_WIDTH)
        .min(viewport.y / HUD_REFERENCE_HEIGHT)
        .clamp(HUD_MIN_SCALE, HUD_MAX_SCALE)
}

/// Keeps the left-side HUD readable on short screens and proportional on large ones.
fn owned_worlds_hud_scale(viewport: egui::Vec2) -> f32 {
    (viewport.x / HUD_REFERENCE_WIDTH).min(viewport.y / HUD_REFERENCE_HEIGHT).max(1.0)
}

/// Matches the clear space between the resource bar and the world shortcuts.
#[cfg(test)]
fn strategic_hud_panel_gap(viewport: egui::Vec2) -> f32 {
    (OWNED_WORLDS_TOP * owned_worlds_hud_scale(viewport) - resource_bar_bottom(viewport)).max(0.0)
}

/// Keeps menus and controls at their original size in the default game window.
pub(crate) fn viewport_ui_scale(viewport: egui::Vec2) -> f32 {
    let relative = (viewport.x / WIDTH).min(viewport.y / HEIGHT).max(0.0);
    relative.sqrt().clamp(0.5, 1.25)
}

/// Fits the largest paired hover panels inside smaller windows.
pub(crate) fn game_panel_scale(viewport: egui::Vec2) -> f32 {
    let preferred = viewport_ui_scale(viewport);
    let max_width = (viewport.x - 16.0).max(1.0) / 850.0;
    let max_height = (viewport.y - 16.0).max(1.0) / 640.0;
    preferred.min(max_width).min(max_height).max(0.001)
}

/// Uses the shared growth curve while keeping smaller dialogs readable and fully on screen.
fn game_modal_scale(viewport: egui::Vec2, size: egui::Vec2) -> f32 {
    viewport_ui_scale(viewport)
        .max(0.7)
        .min((viewport.x - 32.0).max(1.0) / size.x.max(1.0))
        .min((viewport.y - 32.0).max(1.0) / size.y.max(1.0))
}

fn scaled_margin(horizontal: f32, vertical: f32, scale: f32) -> egui::Margin {
    egui::Margin::symmetric((horizontal * scale).round() as i8, (vertical * scale).round() as i8)
}

/// Builds the translucent frame shared by the compact strategic HUD widgets.
fn hud_panel_frame() -> egui::Frame {
    egui::Frame::new()
        .fill(HUD_PANEL_FILL)
        .stroke(Stroke::new(1.0, HUD_PANEL_STROKE))
        .corner_radius(6.0)
        .inner_margin(egui::Margin::symmetric(9, 9))
}

fn scaled_hud_panel_frame(scale: f32) -> egui::Frame {
    hud_panel_frame()
        .stroke(Stroke::new(scale, HUD_PANEL_STROKE))
        .corner_radius((6.0 * scale).round() as u8)
        .inner_margin(scaled_margin(9.0, 9.0, scale))
}

/// Places mission hover panels at the screen edge opposite the pointer.
#[cfg(test)]
fn mission_hover_panel_x_positions(cursor_x: Option<f32>, viewport_width: f32) -> (f32, f32) {
    mission_hover_panel_x_positions_on_side(
        cursor_x.is_none_or(|x| x < viewport_width * 0.5),
        viewport_width,
    )
}

fn mission_hover_panel_x_positions_on_side(
    panels_on_right: bool,
    viewport_width: f32,
) -> (f32, f32) {
    let left_edge = viewport_width * 0.002;
    let right_edge = viewport_width * 0.998;

    if panels_on_right {
        let fleet_x = right_edge - MISSION_HOVER_FLEET_WIDTH;
        let info_x = fleet_x - MISSION_HOVER_PANEL_GAP - MISSION_HOVER_INFO_WIDTH;
        (fleet_x, info_x)
    } else {
        let fleet_x = left_edge;
        let info_x = fleet_x + MISSION_HOVER_FLEET_WIDTH + MISSION_HOVER_PANEL_GAP;
        (fleet_x, info_x)
    }
}

/// Map mission hovers include route details; mission-list hovers preview only the fleet.
fn mission_hover_shows_info_panel(mission_hover_from_ui: bool) -> bool {
    !mission_hover_from_ui
}

/// Scales the complete panel layer, including its artwork, text, controls, and hit targets.
fn draw_scaled_panel<R>(
    contexts: &mut EguiContexts,
    name: &str,
    image: &str,
    pos: (f32, f32),
    size: (f32, f32),
    scale: f32,
    images: &ImageIds,
    content: impl FnOnce(&mut Ui) -> R,
) {
    let _ = draw_panel_with_horizontal_overflow(
        contexts, name, image, pos, size, 0.0, scale, images, content,
    );
}

/// Draws a panel while permitting an animated horizontal entrance beyond the viewport edge.
fn draw_scaled_sliding_panel<R>(
    contexts: &mut EguiContexts,
    name: &str,
    image: &str,
    pos: (f32, f32),
    size: (f32, f32),
    horizontal_overflow: f32,
    scale: f32,
    images: &ImageIds,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<egui::Rect> {
    draw_panel_with_horizontal_overflow(
        contexts,
        name,
        image,
        pos,
        size,
        horizontal_overflow,
        scale,
        images,
        content,
    )
}

fn draw_panel_with_horizontal_overflow<R>(
    contexts: &mut EguiContexts,
    name: &str,
    image: &str,
    pos: (f32, f32),
    size: (f32, f32),
    horizontal_overflow: f32,
    scale: f32,
    images: &ImageIds,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<egui::Rect> {
    let Ok(context) = contexts.ctx_mut() else {
        return None;
    };
    draw_panel_on_context(
        context,
        name,
        image,
        pos,
        size,
        horizontal_overflow,
        scale,
        images,
        content,
    )
}

fn draw_panel_on_context<R>(
    context: &egui::Context,
    name: &str,
    image: &str,
    pos: (f32, f32),
    size: (f32, f32),
    horizontal_overflow: f32,
    scale: f32,
    images: &ImageIds,
    content: impl FnOnce(&mut Ui) -> R,
) -> Option<egui::Rect> {
    let order = if name == "combat report" {
        Order::Foreground
    } else {
        Order::Middle
    };
    let panel_id = egui::Id::new(name);
    context.set_transform_layer(
        egui::LayerId::new(order, panel_id),
        egui::emath::TSTransform::from_scaling(scale),
    );
    let mut window = egui::Window::new(name)
        .id(panel_id)
        .frame(egui::Frame {
            fill: Color32::TRANSPARENT,
            ..default()
        })
        .order(order)
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_pos(egui::pos2(pos.0 / scale, pos.1 / scale))
        .fixed_size(size);

    let viewport = context.content_rect();
    let logical_viewport = egui::Rect::from_min_max(viewport.min / scale, viewport.max / scale);
    window =
        window.constrain_to(logical_viewport.expand2(egui::vec2(horizontal_overflow / scale, 0.0)));

    window
        .show(context, |ui| {
            let response =
                ui.add(egui::Image::new(SizedTexture::new(images.get(image), ui.available_size())));

            ui.scope_builder(UiBuilder::new().max_rect(response.rect), content);
        })
        .map(|response| {
            egui::emath::TSTransform::from_scaling(scale).mul_rect(response.response.rect)
        })
}

/// Draws a centered, input-blocking planet action prompt over the game interface.
fn draw_planet_confirmation(
    context: &egui::Context,
    images: &ImageIds,
    planet_name: &str,
    action: &str,
    image: &str,
) -> Option<ConfirmationAction> {
    let content_rect = context.content_rect();
    let available =
        content_rect.size() / game_panel_scale(content_rect.size()) - egui::vec2(32.0, 32.0);
    let size = egui::vec2(520.0_f32.min(available.x), 250.0_f32.min(available.y));
    let modal_id = egui::Id::new((action, "planet confirmation"));
    let response = show_panel_modal(context, images, modal_id, size, |ui, rect, content| {
        let header = draw_modal_header(
            ui,
            rect,
            content,
            RichText::new(format!("{} PLANET", action.to_uppercase()))
                .size(21.0)
                .strong()
                .color(ABANDON_CONFIRMATION_TEXT_COLOR),
            images.get(image),
        );
        let footer = egui::Rect::from_min_size(
            egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
            egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
        );
        let body = egui::Rect::from_min_max(
            egui::pos2(content.left(), header.bottom() + 8.0),
            egui::pos2(content.right(), (footer.top() - 10.0).max(header.bottom() + 8.0)),
        );
        ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
            ui.set_clip_rect(body);
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Are you sure you want to {action} planet {planet_name}?"
                    ))
                    .size(19.0)
                    .strong()
                    .color(Color32::WHITE),
                );
            });
        });
        draw_confirmation_buttons(ui, footer, true)
    });

    if response.should_close() {
        Some(ConfirmationAction::Cancel)
    } else {
        response.inner
    }
}

fn draw_abandon_confirmation(
    context: &egui::Context,
    images: &ImageIds,
    planet_name: &str,
) -> Option<ConfirmationAction> {
    draw_planet_confirmation(context, images, planet_name, "abandon", "abandon")
}

fn draw_colonize_confirmation(
    context: &egui::Context,
    images: &ImageIds,
    planet_name: &str,
) -> Option<ConfirmationAction> {
    draw_planet_confirmation(context, images, planet_name, "colonize", "colonize")
}

/// Explains protection access and lists every current player allowed to use it.
fn draw_protection_access_tooltip(ui: &mut Ui, planet: &Planet, session: &MultiplayerSession) {
    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
    ui.small("Manage protection access. Revoking access sends protection fleets home.");

    let local_player = session.membership.as_ref().map(|member| member.player_id);
    let allowed_players = session
        .active_game
        .as_ref()
        .map(|game| {
            game.members
                .iter()
                .filter(|member| {
                    Some(member.player_id) != local_player
                        && planet.protection_permissions.contains(&member.player_id)
                        && game
                            .persisted
                            .state
                            .player(member.player_id)
                            .is_ok_and(|player| !player.spectator)
                })
                .collect_vec()
        })
        .unwrap_or_default();

    if allowed_players.is_empty() {
        return;
    }

    ui.add_space(5.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.small(RichText::new("Players currently allowed:").strong());
        ui.add_space(4.0);
        for (index, member) in allowed_players.into_iter().enumerate() {
            if index > 0 {
                ui.small(", ");
            }
            let color = session.player_color(member.player_id).color().to_color32();
            ui.label(RichText::new(&member.display_name).small().color(color));
        }
    });
}

/// Draws a compact player choice shared by protection and attack invitation pickers.
fn modal_player_row(
    ui: &mut Ui,
    width: f32,
    height: f32,
    display_name: &str,
    player_color: Color32,
    allowed: bool,
    enabled: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width.min(ui.available_width()), height),
        if enabled {
            Sense::click()
        } else {
            Sense::hover()
        },
    );
    let response = if enabled {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    };
    let fill = if !enabled {
        Color32::from_rgba_unmultiplied(12, 19, 27, 180)
    } else if response.is_pointer_button_down_on() {
        Color32::from_rgb(10, 27, 39)
    } else if response.hovered() {
        Color32::from_rgb(27, 44, 58)
    } else if allowed {
        Color32::from_rgb(18, 39, 53)
    } else {
        Color32::from_rgb(13, 23, 32)
    };
    let stroke = if allowed {
        Stroke::new(1.5, Color32::from_rgb(91, 178, 218))
    } else if response.hovered() && enabled {
        Stroke::new(1.0, Color32::from_rgb(105, 148, 181))
    } else {
        Stroke::new(1.0, Color32::from_rgb(54, 75, 93))
    };
    ui.painter().rect(rect, 7.0, fill, stroke, StrokeKind::Inside);

    let marker = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 20.0, rect.center().y),
        egui::vec2(18.0, 18.0),
    );
    ui.painter().rect(
        marker,
        4.0,
        if allowed {
            player_color
        } else {
            Color32::TRANSPARENT
        },
        Stroke::new(
            1.5,
            if allowed {
                player_color
            } else {
                Color32::from_rgb(103, 130, 151)
            },
        ),
        StrokeKind::Inside,
    );
    if allowed {
        let check_stroke = Stroke::new(2.0, Color32::WHITE);
        let check_midpoint = egui::pos2(marker.left() + 8.0, marker.bottom() - 4.0);
        ui.painter().line_segment(
            [egui::pos2(marker.left() + 4.0, marker.center().y), check_midpoint],
            check_stroke,
        );
        ui.painter().line_segment(
            [check_midpoint, egui::pos2(marker.right() - 3.0, marker.top() + 4.0)],
            check_stroke,
        );
    }

    ui.painter().with_clip_rect(rect.shrink2(egui::vec2(10.0, 0.0))).text(
        egui::pos2(marker.right() + 12.0, rect.center().y),
        Align2::LEFT_CENTER,
        display_name,
        egui::FontId::proportional(17.0),
        if enabled || allowed {
            player_color
        } else {
            player_color.gamma_multiply(0.55)
        },
    );
    response
}

/// Draws the immediate, per-player protection-access controls for one world.
fn draw_protection_access_modal(
    context: &egui::Context,
    images: &ImageIds,
    planet: &Planet,
    session: &MultiplayerSession,
) -> (bool, Vec<(crate::core::identity::PlayerId, bool)>) {
    let local_player = session.membership.as_ref().map(|member| member.player_id);
    let members = session
        .active_game
        .as_ref()
        .map(|game| {
            game.members
                .iter()
                .filter(|member| {
                    Some(member.player_id) != local_player
                        && game
                            .persisted
                            .state
                            .player(member.player_id)
                            .is_ok_and(|player| !player.spectator)
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let content_rect = context.content_rect();
    let available =
        content_rect.size() / game_panel_scale(content_rect.size()) - egui::vec2(32.0, 32.0);
    let desired_height = 242.0 + members.len() as f32 * 46.0;
    let size = egui::vec2(560.0_f32.min(available.x), desired_height.min(available.y));
    let modal_id = egui::Id::new(("protection access", planet.id));
    let response = show_panel_modal(context, images, modal_id, size, |ui, rect, content| {
        let mut changes = Vec::new();
        let mut close = false;
        let header = draw_modal_header(
            ui,
            rect,
            content,
            RichText::new("Protection Access")
                .size(21.0)
                .strong()
                .color(ABANDON_CONFIRMATION_TEXT_COLOR),
            images.get("protect"),
        );
        let footer = egui::Rect::from_min_size(
            egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
            egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
        );
        let intro = egui::Rect::from_min_max(
            egui::pos2(content.left(), header.bottom() + 12.0),
            egui::pos2(content.right(), (header.bottom() + 52.0).min(footer.top())),
        );
        ui.scope_builder(UiBuilder::new().max_rect(intro), |ui| {
            ui.set_clip_rect(intro);
            ui.vertical_centered(|ui| {
                ui.small(format!(
                    "Choose who may send fleets to {}. Changes apply immediately.",
                    planet.name
                ));
            });
        });

        let list_top = (intro.bottom() + 20.0).min(footer.top());
        let list_bottom = (footer.top() - 12.0).max(list_top);
        let list = egui::Rect::from_min_max(
            egui::pos2(content.left(), list_top),
            egui::pos2(content.right(), list_bottom),
        );
        ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
            ui.set_clip_rect(list);
            egui::ScrollArea::vertical()
                .id_salt(("protection access players", planet.id))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(list.width());
                    ui.spacing_mut().item_spacing.y = 7.0;
                    for member in &members {
                        let allowed = planet.protection_permissions.contains(&member.player_id);
                        let available_width = ui.available_width();
                        let row_width = (available_width * PROTECTION_PLAYER_ROW_WIDTH_FRACTION)
                            .clamp(PROTECTION_PLAYER_ROW_MIN_WIDTH, PROTECTION_PLAYER_ROW_MAX_WIDTH)
                            .min(available_width);
                        let response = ui
                            .horizontal(|ui| {
                                ui.add_space(((ui.available_width() - row_width) * 0.5).max(0.0));
                                modal_player_row(
                                    ui,
                                    row_width,
                                    42.0,
                                    &member.display_name,
                                    session.player_color(member.player_id).color().to_color32(),
                                    allowed,
                                    !session.protection_update_pending,
                                )
                            })
                            .inner;
                        if response.clicked() {
                            changes.push((member.player_id, !allowed));
                        }
                    }
                });
        });

        ui.scope(|ui| {
            style_modal_buttons(ui);
            let width = 110.0_f32.min(footer.width());
            let button_rect = egui::Rect::from_center_size(
                footer.center(),
                egui::vec2(width, MODAL_BUTTON_HEIGHT),
            );
            if ui
                .put(
                    button_rect,
                    egui::Button::new(
                        RichText::new("Close")
                            .size(17.0)
                            .strong()
                            .color(ABANDON_CONFIRMATION_TEXT_COLOR),
                    ),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
                .clicked()
            {
                close = true;
            }
        });
        (close, changes)
    });
    let should_close = response.should_close();
    let (mut close, changes) = response.inner;
    close |= should_close;
    (close, changes)
}

/// Draws the input-blocking confirmation for a synchronized Railgun strike.
fn draw_railgun_confirmation(
    context: &egui::Context,
    images: &ImageIds,
    target_name: &str,
    railgun_count: usize,
    deuterium_cost: usize,
    energy_cost: usize,
    chance_basis_points: u16,
    has_deuterium: bool,
) -> Option<ConfirmationAction> {
    let content_rect = context.content_rect();
    let available =
        content_rect.size() / game_panel_scale(content_rect.size()) - egui::vec2(32.0, 32.0);
    let size = egui::vec2(610.0_f32.min(available.x), RAILGUN_CONFIRMATION_HEIGHT.min(available.y));
    let modal_id = egui::Id::new("orbital railgun confirmation");
    let response = show_panel_modal(context, images, modal_id, size, |ui, rect, content| {
        let header = draw_modal_header(
            ui,
            rect,
            content,
            RichText::new("ORBITAL RAILGUN STRIKE")
                .size(21.0)
                .strong()
                .color(ABANDON_CONFIRMATION_TEXT_COLOR),
            images.get("railgun strike"),
        );
        let footer = egui::Rect::from_min_size(
            egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
            egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
        );
        let body_top = (header.bottom() + 6.0).min(footer.top());
        let body = egui::Rect::from_min_max(
            egui::pos2(content.left(), body_top),
            egui::pos2(content.right(), (footer.top() - 10.0).max(body_top)),
        );
        ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
            ui.set_clip_rect(body);
            egui::ScrollArea::vertical()
                .id_salt("railgun confirmation details")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(body.width());
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        ui.label(
                            RichText::new(format!(
                                "Fire every available Railgun at {target_name}?"
                            ))
                            .size(18.0)
                            .strong()
                            .color(Color32::WHITE),
                        );
                        ui.add_space(24.0);
                        let deuterium_amount = RichText::new(format_thousands(deuterium_cost))
                            .size(20.0)
                            .strong()
                            .color(Color32::WHITE);
                        let energy_amount = RichText::new(format_thousands(energy_cost))
                            .size(20.0)
                            .strong()
                            .color(Color32::WHITE);
                        let text_width = |text: RichText| {
                            egui::WidgetText::from(text)
                                .into_galley(
                                    ui,
                                    Some(egui::TextWrapMode::Extend),
                                    f32::INFINITY,
                                    TextStyle::Body,
                                )
                                .size()
                                .x
                        };
                        let icon_width = 42.0;
                        let row_width = icon_width
                            + 7.0
                            + text_width(deuterium_amount.clone())
                            + 22.0
                            + icon_width
                            + 7.0
                            + text_width(energy_amount.clone());
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            ui.add_space(((ui.available_width() - row_width) * 0.5).max(0.0));
                            let (deuterium_icon, _) = ui
                                .allocate_exact_size(egui::vec2(icon_width, 30.0), Sense::hover());
                            paint_bordered_resource_image(
                                ui,
                                images.get("deuterium"),
                                deuterium_icon,
                                2.0,
                            );
                            ui.add_space(7.0);
                            ui.label(deuterium_amount);
                            ui.add_space(22.0);
                            let (energy_icon, _) = ui
                                .allocate_exact_size(egui::vec2(icon_width, 30.0), Sense::hover());
                            paint_bordered_resource_image(
                                ui,
                                images.get("energy"),
                                energy_icon,
                                2.0,
                            );
                            ui.add_space(7.0);
                            ui.label(energy_amount);
                        });
                        ui.add_space(14.0);
                        ui.label(
                            RichText::new(format!("Orbital Railguns firing: {railgun_count}"))
                                .size(17.0)
                                .strong(),
                        );
                        ui.add_space(5.0);
                        ui.label(
                            RichText::new(format!(
                                "Destruction chance: {}%",
                                chance_basis_points / 100,
                            ))
                            .size(17.0)
                            .strong(),
                        );
                    });
                });
        });
        draw_confirmation_buttons(ui, footer, has_deuterium)
    });

    if response.should_close() {
        Some(ConfirmationAction::Cancel)
    } else {
        response.inner
    }
}

/// Selects the stationed fleet silhouette shown beside a world shortcut.
fn world_shortcut_fleet_image(army: &Army) -> Option<&'static str> {
    if army.amount(&Unit::war_sun()) > 0 {
        Some("mission destroy")
    } else if !army.iter().any(|(unit, count)| unit.is_ship() && *count > 0) {
        None
    } else if army
        .iter()
        .all(|(unit, count)| *count == 0 || !unit.is_ship() || *unit == Unit::probe())
    {
        Some("mission spy")
    } else {
        Some("mission")
    }
}

/// Iterates the controller fleet first, then protection fleets in stable player order.
fn world_shortcut_fleet_icons<'a>(
    planet: &'a Planet,
    controller_fleet_color: Color32,
    session: &'a MultiplayerSession,
) -> impl Iterator<Item = (&'static str, Color32)> + 'a {
    std::iter::once((planet.army.controller(), controller_fleet_color))
        .chain(
            planet.army.protectors().map(|(player_id, army)| {
                (army, session.player_color(player_id).color().to_color32())
            }),
        )
        .filter_map(|(army, color)| world_shortcut_fleet_image(army).map(|image| (image, color)))
}

/// Draws one compact, fully clickable world shortcut.
fn draw_world_shortcut(
    ui: &mut Ui,
    planet: &Planet,
    is_home: bool,
    is_selected: bool,
    player_color: Color32,
    session: &MultiplayerSession,
    images: &ImageIds,
    scale: f32,
) -> egui::Response {
    draw_world_shortcut_with_controller_color(
        ui,
        planet,
        is_home,
        is_selected,
        player_color,
        player_color,
        session,
        images,
        scale,
    )
}

/// Draws a shortcut whose selected state and controlling fleet can belong to different players.
fn draw_world_shortcut_with_controller_color(
    ui: &mut Ui,
    planet: &Planet,
    is_home: bool,
    is_selected: bool,
    player_color: Color32,
    controller_fleet_color: Color32,
    session: &MultiplayerSession,
    images: &ImageIds,
    scale: f32,
) -> egui::Response {
    let available_width = ui.available_width();
    let fleet_icon_count =
        world_shortcut_fleet_icons(planet, controller_fleet_color, session).count();
    let fleet_icon_width = world_shortcut_fleet_width(fleet_icon_count, scale);
    let text_width =
        (available_width - WORLD_SHORTCUT_FIXED_WIDTH * scale - fleet_icon_width).max(0.0);
    let name = egui::WidgetText::from(
        RichText::new(&planet.name).size(14.0 * scale).strong().color(Color32::WHITE),
    )
    .into_galley(ui, Some(egui::TextWrapMode::Truncate), text_width, TextStyle::Body);

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, WORLD_SHORTCUT_HEIGHT * scale),
        Sense::click(),
    );
    let response = response.on_hover_cursor(CursorIcon::PointingHand);

    let fill_alpha = match (is_selected, response.hovered()) {
        (true, true) => 104,
        (true, false) => 80,
        (false, true) => 36,
        (false, false) => 0,
    };
    if fill_alpha > 0 {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same((4.0 * scale).round() as u8),
            Color32::from_rgba_unmultiplied(
                player_color.r(),
                player_color.g(),
                player_color.b(),
                fill_alpha,
            ),
        );
    }
    if is_selected {
        ui.painter().rect_filled(
            egui::Rect::from_min_max(
                egui::pos2(rect.left(), rect.top() + 5.0 * scale),
                egui::pos2(rect.left() + 3.0 * scale, rect.bottom() - 5.0 * scale),
            ),
            1.0 * scale,
            player_color,
        );
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.left() + 24.0 * scale,
            rect.center().y
                + if is_home {
                    3.0 * scale
                } else {
                    0.0
                },
        ),
        egui::Vec2::splat(30.0 * scale),
    );
    ui.painter().image(
        images.get(planet.image()),
        icon_rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    let text_x = icon_rect.right() + 8.0 * scale;
    let name_size = name.size();
    let name_top = rect.center().y - name_size.y * 0.5;
    ui.painter().galley(egui::pos2(text_x, name_top), name, Color32::WHITE);
    if is_home {
        let crown_rect = egui::Rect::from_center_size(
            egui::pos2(icon_rect.center().x, icon_rect.top() - 2.5 * scale),
            egui::vec2(14.0, 11.0) * scale,
        );
        let mut crown = egui::Mesh::default();
        for [x, y] in HOME_CROWN_VERTICES {
            crown.colored_vertex(
                egui::pos2(
                    crown_rect.left() + x * crown_rect.width(),
                    crown_rect.bottom() - y * crown_rect.height(),
                ),
                HOME_PLANET_COLOR.to_color32(),
            );
        }
        crown.indices.extend(HOME_CROWN_INDICES);
        ui.painter().add(crown);
    }

    for (index, (fleet_image, fleet_color)) in
        world_shortcut_fleet_icons(planet, controller_fleet_color, session).enumerate()
    {
        let fleet_icon_rect = egui::Rect::from_center_size(
            egui::pos2(
                text_x
                    + name_size.x
                    + WORLD_SHORTCUT_FLEET_ICON_GAP * scale
                    + WORLD_SHORTCUT_FLEET_ICON_SIZE * scale * 0.5
                    + index as f32
                        * (WORLD_SHORTCUT_FLEET_ICON_SIZE + WORLD_SHORTCUT_FLEET_ICON_SPACING)
                        * scale,
                rect.center().y,
            ),
            egui::Vec2::splat(WORLD_SHORTCUT_FLEET_ICON_SIZE * scale),
        );
        ui.painter().image(
            images.get(fleet_image),
            fleet_icon_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            fleet_color,
        );
    }

    response
}

/// Draws a world-group label and its item count using the same panel heading style.
fn draw_world_group_header(ui: &mut Ui, title: &str, count: &str, scale: f32) {
    let heading_color = Color32::from_rgb(166, 188, 211);
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(11.0 * scale).strong().color(heading_color));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(RichText::new(count).size(11.0 * scale).strong().color(heading_color));
        });
    });
}

const OWNED_WORLDS_LEFT: f32 = 9.0;
const OWNED_WORLDS_TOP: f32 = 112.0;
const OWNED_WORLDS_WIDTH: f32 = 164.0;
const WORLD_SHORTCUT_HEIGHT: f32 = 40.0;
const WORLD_LIST_ITEM_SPACING: f32 = 3.0;
const WORLD_SHORTCUT_FIXED_WIDTH: f32 = 51.0;
const WORLD_SHORTCUT_FLEET_ICON_GAP: f32 = 6.0;
const WORLD_SHORTCUT_FLEET_ICON_SIZE: f32 = 20.0;
const WORLD_SHORTCUT_FLEET_ICON_SPACING: f32 = 3.0;
const HUD_PANEL_BOTTOM_INSET: f32 = 18.0;
const OWNED_WORLDS_MAX_HEIGHT_FRACTION: f32 = 0.6;

fn world_shortcut_fleet_width(icon_count: usize, scale: f32) -> f32 {
    if icon_count == 0 {
        0.0
    } else {
        (WORLD_SHORTCUT_FLEET_ICON_GAP
            + WORLD_SHORTCUT_FLEET_ICON_SIZE * icon_count as f32
            + WORLD_SHORTCUT_FLEET_ICON_SPACING * icon_count.saturating_sub(1) as f32)
            * scale
    }
}

/// Measures a row before the panel is laid out so names and fleet icons get real space.
fn world_shortcut_required_width(
    ui: &mut Ui,
    planet: &Planet,
    session: &MultiplayerSession,
    scale: f32,
) -> f32 {
    let name_width = egui::WidgetText::from(
        RichText::new(&planet.name).size(14.0 * scale).strong().color(Color32::WHITE),
    )
    .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, TextStyle::Body)
    .size()
    .x;
    let fleet_icon_count = world_shortcut_fleet_icons(planet, Color32::WHITE, session).count();

    WORLD_SHORTCUT_FIXED_WIDTH * scale
        + name_width
        + world_shortcut_fleet_width(fleet_icon_count, scale)
}

/// Uses persisted acquisition history for both shortcut groups, with home always first.
fn world_shortcut_order(planet: &Planet, player: &Player) -> (bool, usize, PlanetId) {
    (
        planet.id != player.home_planet,
        player.world_acquisition_order.iter().position(|id| *id == planet.id).unwrap_or(usize::MAX),
        planet.id,
    )
}

/// Shows the local player's owned, controlled, and protected worlds as quick map shortcuts.
fn draw_owned_worlds_widget(
    context: &egui::Context,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    state: &mut UiState,
    settings: &mut Settings,
    images: &ImageIds,
) -> egui::Rect {
    let scale = owned_worlds_hud_scale(context.content_rect().size());
    let panel_top = OWNED_WORLDS_TOP * scale;
    let max_bottom = (panel_top
        + context.content_rect().height() * OWNED_WORLDS_MAX_HEIGHT_FRACTION)
        .min(context.content_rect().bottom() - HUD_PANEL_BOTTOM_INSET * scale);
    draw_owned_worlds_widget_with_max_bottom(
        context, map, player, session, state, settings, images, max_bottom,
    )
}

/// Caps the shortcut list at an explicit screen edge, scrolling any rows that do not fit.
fn draw_owned_worlds_widget_with_max_bottom(
    context: &egui::Context,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    state: &mut UiState,
    settings: &mut Settings,
    images: &ImageIds,
    max_bottom: f32,
) -> egui::Rect {
    state.world_shortcut_hover = None;
    if player.spectator {
        return egui::Rect::NOTHING;
    }
    let scale = owned_worlds_hud_scale(context.content_rect().size());
    let mut owned = map
        .planets
        .iter()
        .filter(|planet| !planet.is_destroyed && !planet.is_moon() && player.owns(planet))
        .collect::<Vec<_>>();
    let mut controlled = map
        .planets
        .iter()
        .filter(|planet| !planet.is_destroyed && player.controls(planet) && !player.owns(planet))
        .collect::<Vec<_>>();
    let mut protected = map
        .planets
        .iter()
        .filter(|planet| {
            !planet.is_destroyed
                && !player.owns(planet)
                && !player.controls(planet)
                && planet.is_protected_by(player.id)
        })
        .collect::<Vec<_>>();
    owned.sort_by_key(|planet| world_shortcut_order(planet, player));
    controlled.sort_by_key(|planet| world_shortcut_order(planet, player));
    protected.sort_by_key(|planet| world_shortcut_order(planet, player));
    let fleet_color = player.color().color().to_color32();
    let frame = scaled_hud_panel_frame(scale);
    let frame_margin = frame.total_margin();
    let panel_top = OWNED_WORLDS_TOP * scale;
    let max_content_height =
        (max_bottom - panel_top - frame_margin.top - frame_margin.bottom).max(0.0);
    if max_content_height <= 0.0 {
        return egui::Rect::NOTHING;
    }

    egui::Area::new("stellarion_owned_worlds".into())
        .fixed_pos(egui::pos2(OWNED_WORLDS_LEFT * scale, panel_top))
        .movable(false)
        .constrain(true)
        .order(Order::Middle)
        .show(context, |ui| {
            frame.show(ui, |ui| {
                let max_width = (context.content_rect().width() - 62.0 * scale).max(0.0);
                let required_width = owned
                    .iter()
                    .chain(&controlled)
                    .chain(&protected)
                    .map(|planet| world_shortcut_required_width(ui, planet, session, scale))
                    .fold(OWNED_WORLDS_WIDTH * scale, f32::max);
                ui.set_width(required_width.min(max_width));
                ui.spacing_mut().item_spacing =
                    egui::vec2(5.0 * scale, WORLD_LIST_ITEM_SPACING * scale);

                egui::ScrollArea::vertical()
                    .max_height(max_content_height)
                    // The parent Area remembers its previous frame's size. Give the scroll area
                    // the full cap while measuring so newly acquired rows can enlarge that Area;
                    // auto-shrink still keeps shorter lists fitted to their actual contents.
                    .min_scrolled_height(max_content_height)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        ui.set_width(required_width.min(max_width));

                        if owned.is_empty() && controlled.is_empty() && protected.is_empty() {
                            ui.label(
                                RichText::new("No worlds under your control")
                                    .size(11.0 * scale)
                                    .color(Color32::from_rgb(145, 156, 168)),
                            );
                        }

                        if !owned.is_empty() {
                            let (n_owned, n_max_owned) = player.planets_owned(map, settings);
                            draw_world_group_header(
                                ui,
                                "OWNED PLANETS",
                                &format!("{n_owned}/{n_max_owned}"),
                                scale,
                            );
                            for planet in &owned {
                                let response = draw_world_shortcut(
                                    ui,
                                    planet,
                                    planet.id == player.home_planet,
                                    state.planet_selected == Some(planet.id),
                                    fleet_color,
                                    session,
                                    images,
                                    scale,
                                );
                                if response.hovered() {
                                    state.world_shortcut_hover = Some(planet.id);
                                    state.mission_planet_hover = None;
                                }
                                if response.clicked() {
                                    select_planet(planet, state, player);
                                    state.to_selected = true;
                                    state.planet_hover = None;
                                    settings.show_menu = true;
                                }
                            }
                        }

                        if !controlled.is_empty() {
                            if !owned.is_empty() {
                                ui.add_space(8.0 * scale);
                            }
                            draw_world_group_header(
                                ui,
                                "CONTROLLED WORLDS",
                                &controlled.len().to_string(),
                                scale,
                            );
                            for planet in &controlled {
                                let response = draw_world_shortcut(
                                    ui,
                                    planet,
                                    planet.id == player.home_planet,
                                    state.planet_selected == Some(planet.id),
                                    fleet_color,
                                    session,
                                    images,
                                    scale,
                                );
                                if response.hovered() {
                                    state.world_shortcut_hover = Some(planet.id);
                                    state.mission_planet_hover = None;
                                }
                                if response.clicked() {
                                    select_planet(planet, state, player);
                                    state.to_selected = true;
                                    state.planet_hover = None;
                                    settings.show_menu = true;
                                }
                            }
                        }

                        if !protected.is_empty() {
                            if !owned.is_empty() || !controlled.is_empty() {
                                ui.add_space(8.0 * scale);
                            }
                            draw_world_group_header(
                                ui,
                                "PROTECTED WORLDS",
                                &protected.len().to_string(),
                                scale,
                            );
                            for planet in &protected {
                                let controller_fleet_color = planet
                                    .controlled
                                    .or(planet.owned)
                                    .map(|player_id| {
                                        session.player_color(player_id).color().to_color32()
                                    })
                                    .unwrap_or(fleet_color);
                                let response = draw_world_shortcut_with_controller_color(
                                    ui,
                                    planet,
                                    false,
                                    state.planet_selected == Some(planet.id),
                                    fleet_color,
                                    controller_fleet_color,
                                    session,
                                    images,
                                    scale,
                                );
                                if response.hovered() {
                                    state.world_shortcut_hover = Some(planet.id);
                                    state.mission_planet_hover = None;
                                }
                                if response.clicked() {
                                    select_planet(planet, state, player);
                                    state.to_selected = true;
                                    state.planet_hover = None;
                                    settings.show_menu = true;
                                }
                            }
                        }
                    });
            });
        })
        .response
        .rect
}

/// Counts only ownership and controller intelligence visible through the map and reports.
pub(crate) fn known_planet_counts(
    map: &Map,
    player: &Player,
    missions: &[Mission],
) -> HashMap<u64, usize> {
    let mut counts = HashMap::new();
    for planet in map.planets.iter().filter(|planet| !planet.is_moon() && !planet.is_destroyed) {
        let controller = if planet.owned.is_some()
            && (planet.army.amount(&Unit::space_dock()) > 0
                || planet.army.amount(&Unit::Building(Building::OrbitalRailgun)) > 0
                || visible_trading_post_owner(map, player.id, planet).is_some())
        {
            // Visible public orbitals identify their planet's owner. This is ownership
            // intelligence even when another player temporarily controls the world.
            planet.owned
        } else if player.controls(planet) {
            planet.controlled
        } else {
            player.known_controller(planet, missions)
        };
        if let Some(controller) = controller {
            *counts.entry(controller).or_default() += 1;
        }
    }
    counts
}

/// Shows the local player first, followed by opponents and their territorial progress.
fn draw_players_widget_with_controls(
    context: &egui::Context,
    session: &MultiplayerSession,
    local_player: &Player,
    map: &Map,
    missions: &[Mission],
    mut requests: Option<&mut MessageWriter<MultiplayerRequest>>,
) -> egui::Rect {
    #[cfg(not(debug_assertions))]
    let _ = &mut requests;
    let Some(game) = &session.active_game else {
        return egui::Rect::NOTHING;
    };
    let mut members = game.members.iter().collect::<Vec<_>>();
    if members.is_empty() {
        return egui::Rect::NOTHING;
    }
    members.sort_by_key(|member| member.player_id != local_player.id);
    let counts = known_planet_counts(map, local_player, missions);
    let local_progress = map
        .planets
        .iter()
        .filter(|planet| !planet.is_moon() && !planet.is_destroyed && local_player.controls(planet))
        .count();
    let target = game.persisted.state.planets_to_win();
    let winner = game.persisted.state.winner();
    let is_eliminated = |player_id| {
        game.persisted.state.player(player_id).is_ok_and(|player| player.spectator)
            && winner != Some(player_id)
    };
    let scale = owned_worlds_hud_scale(context.content_rect().size());

    egui::Area::new("stellarion_players".into())
        .anchor(
            Align2::LEFT_BOTTOM,
            egui::vec2(OWNED_WORLDS_LEFT * scale, -HUD_PANEL_BOTTOM_INSET * scale),
        )
        .movable(false)
        .constrain(true)
        .order(Order::Middle)
        .show(context, |ui| {
            scaled_hud_panel_frame(scale)
                .fill(Color32::from_rgba_unmultiplied(10, 16, 23, 218))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0) * scale;
                    let progress_spacing = 12.0 * scale;
                    let trailing_space = 8.0 * scale;
                    let progress_for = |member: &GameMembership| {
                        let count = if is_eliminated(member.player_id) {
                            "0".to_owned()
                        } else if member.player_id == local_player.id {
                            local_progress.to_string()
                        } else {
                            counts
                                .get(&member.player_id)
                                .map_or_else(|| "?".to_owned(), usize::to_string)
                        };
                        format!("{count}/{target}")
                    };
                    let max_name_width = members
                        .iter()
                        .map(|member| {
                            egui::WidgetText::from(
                                RichText::new(&member.display_name).size(14.0 * scale).strong(),
                            )
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Extend),
                                f32::INFINITY,
                                TextStyle::Body,
                            )
                            .size()
                            .x
                        })
                        .fold(0.0_f32, f32::max);
                    let progress_reserve = members
                        .iter()
                        .map(|member| {
                            egui::WidgetText::from(
                                RichText::new(progress_for(member)).size(13.0 * scale),
                            )
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Extend),
                                f32::INFINITY,
                                TextStyle::Body,
                            )
                            .size()
                            .x
                        })
                        .fold(0.0_f32, f32::max);
                    let status_reserve = if members.iter().any(|member| {
                        !session.local_practice && !member.connected
                    }) {
                        let status_width = egui::WidgetText::from(
                            RichText::new("DISCONNECTED").size(9.0 * scale).strong(),
                        )
                        .into_galley(
                            ui,
                            Some(egui::TextWrapMode::Extend),
                            f32::INFINITY,
                            TextStyle::Body,
                        )
                        .size()
                        .x;
                        status_width + 12.0 * scale + 2.0 * ui.spacing().item_spacing.x
                    } else {
                        0.0
                    };
                    let max_width = (context.content_rect().width() - 62.0 * scale).max(0.0);
                    let row_width = 18.0 * scale
                        + 2.0 * ui.spacing().item_spacing.x
                        + max_name_width
                        + progress_spacing
                        + progress_reserve
                        + status_reserve
                        + trailing_space;
                    ui.set_width(row_width.min(max_width));
                    ui.label(
                        RichText::new("PLAYERS")
                            .size(11.0 * scale)
                            .strong()
                            .color(Color32::from_rgb(166, 188, 211)),
                    );
                    ui.add_space(5.0 * scale);
                    // Reserve one shared name column so every progress value starts at the same x.
                    let name_column_width = max_name_width.min(
                        (ui.available_width()
                            - 18.0 * scale
                            - 2.0 * ui.spacing().item_spacing.x
                            - progress_spacing
                            - progress_reserve
                            - status_reserve
                            - trailing_space)
                            .max(0.0),
                    );
                    for member in members {
                        ui.horizontal(|ui| {
                            let is_local = member.player_id == local_player.id;
                            let is_eliminated = is_eliminated(member.player_id);
                            let connected = session.local_practice || member.connected;
                            let color = session.player_color(member.player_id);
                            let [red, green, blue] = color.rgb();
                            let (rect, _) = ui
                                .allocate_exact_size(
                                    egui::vec2(18.0, 22.0) * scale,
                                    egui::Sense::hover(),
                                );
                            ui.painter().circle_filled(
                                rect.center(),
                                6.0 * scale,
                                Color32::from_rgba_unmultiplied(
                                    red,
                                    green,
                                    blue,
                                    if connected {
                                        255
                                    } else {
                                        110
                                    },
                                ),
                            );
                            let status = (!connected).then(|| {
                                egui::WidgetText::from(
                                    RichText::new("DISCONNECTED")
                                        .size(9.0 * scale)
                                        .strong()
                                        .color(Color32::from_rgb(255, 112, 112)),
                                )
                                .into_galley(
                                    ui,
                                    Some(egui::TextWrapMode::Extend),
                                    f32::INFINITY,
                                    TextStyle::Body,
                                )
                            });
                            let progress = egui::WidgetText::from(
                                RichText::new(progress_for(member))
                                .size(13.0 * scale)
                                .color(Color32::from_rgb(166, 188, 211)),
                            )
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Extend),
                                f32::INFINITY,
                                TextStyle::Body,
                            );
                            // Keep progress and connection status visible even with long names.
                            let status_width = status.as_ref().map_or(0.0, |galley| {
                                galley.size().x
                                    + 12.0 * scale
                                    + 2.0 * ui.spacing().item_spacing.x
                            });
                            let name = egui::WidgetText::from(
                                RichText::new(&member.display_name).size(14.0 * scale).strong().color(
                                    if is_eliminated {
                                        Color32::from_rgb(142, 148, 156)
                                    } else if connected {
                                        ui.visuals().text_color()
                                    } else {
                                        Color32::from_rgb(174, 181, 190)
                                    },
                                ),
                            )
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                name_column_width.min(
                                    (ui.available_width()
                                        - status_width
                                        - progress.size().x
                                        - progress_spacing
                                        - ui.spacing().item_spacing.x)
                                        .max(0.0),
                                ),
                                TextStyle::Body,
                            );
                            let name_response = if session.local_practice {
                                let interactive = !is_local && !session.busy;
                                let sense = if interactive {
                                    Sense::click()
                                } else {
                                    Sense::hover()
                                };
                                let response = ui.add(egui::Label::new(name).sense(sense));
                                let response = if interactive {
                                    response.on_hover_cursor(CursorIcon::PointingHand)
                                } else {
                                    response
                                };
                                if interactive && response.hovered() {
                                    ui.painter().line_segment(
                                        [response.rect.left_bottom(), response.rect.right_bottom()],
                                        Stroke::new(
                                            scale,
                                            Color32::from_rgba_unmultiplied(red, green, blue, 190),
                                        ),
                                    );
                                }
                                #[cfg(not(debug_assertions))]
                                let _ = &response;
                                #[cfg(debug_assertions)]
                                if !is_local && response.clicked() {
                                    set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
                                    if let Some(requests) = requests.as_deref_mut() {
                                        requests.write(
                                            MultiplayerRequest::SwitchLocalPracticePlayer(
                                                member.player_id,
                                            ),
                                        );
                                    }
                                }
                                response
                            } else {
                                ui.add(egui::Label::new(name))
                            };
                            if is_eliminated {
                                ui.painter().line_segment(
                                    [name_response.rect.left_center(), name_response.rect.right_center()],
                                    Stroke::new(1.4 * scale, Color32::from_rgb(190, 82, 82)),
                                );
                            }
                            ui.add_space(
                                progress_spacing
                                    + (name_column_width - name_response.rect.width()).max(0.0),
                            );
                            let progress = ui.add(egui::Label::new(progress));
                            if is_eliminated {
                                progress.on_hover_small("This player has been eliminated.");
                            } else if is_local {
                                progress.on_hover_small(
                                    "Your controlled planets and the number needed to win. You must also retain your home planet.",
                                );
                            } else {
                                progress.on_hover_small(
                                    "Known controlled planets needed to win. Intelligence may be outdated.",
                                );
                            }
                            if let Some(status) = status {
                                draw_disconnected_icon(ui, scale);
                                ui.add(egui::Label::new(status));
                            }
                        });
                    }
                });
        })
        .response
        .rect
}

#[cfg(test)]
fn draw_players_widget(
    context: &egui::Context,
    session: &MultiplayerSession,
    local_player: &Player,
    map: &Map,
    missions: &[Mission],
) -> egui::Rect {
    draw_players_widget_with_controls(context, session, local_player, map, missions, None)
}

/// Draws a small slashed Wi-Fi symbol without depending on a font's icon coverage.
fn draw_disconnected_icon(ui: &mut Ui, scale: f32) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 14.0) * scale, Sense::hover());
    let center = rect.center();
    let color = Color32::from_rgb(255, 112, 112);
    let stroke = Stroke::new(1.2 * scale, color);
    for (width, top, bottom) in [(5.0, -5.0, -1.0), (3.0, -2.0, 1.0)] {
        ui.painter().add(egui::epaint::QuadraticBezierShape::from_points_stroke(
            [
                center + egui::vec2(-width, bottom) * scale,
                center + egui::vec2(0.0, top) * scale,
                center + egui::vec2(width, bottom) * scale,
            ],
            false,
            Color32::TRANSPARENT,
            stroke,
        ));
    }
    ui.painter().circle_filled(center + egui::vec2(0.0, 3.0) * scale, scale, color);
    ui.painter().line_segment(
        [center - egui::vec2(5.0, 5.0) * scale, center + egui::vec2(5.0, 5.0) * scale],
        stroke,
    );
}

/// Draws the army grid interface and emits any resulting local actions.
fn draw_mission_report_unit(
    ui: &mut Ui,
    unit: &Unit,
    report: &MissionReport,
    player: &Player,
    side: Side,
    visual: (f32, TextStyle),
    images: &ImageIds,
) {
    let (image_size, text_style) = visual;
    let can_see = report.can_see(&side, player.id);
    let (survived, total) = if side == Side::Attacker {
        (report.surviving_attacker.amount(unit), report.mission.army.amount(unit))
    } else {
        (
            report
                .surviving_defender
                .combined_amount(unit)
                .saturating_add(report.escaped_defenders(unit)),
            report.planet.army.combined_amount(unit),
        )
    };
    let lost = total.saturating_sub(survived);

    let text = if can_see {
        if lost > 0 {
            format!("{lost}/{total}")
        } else {
            total.to_string()
        }
    } else if mission_report_unit_is_revealed_by_probes(report, player.id, &side, unit) {
        // Even if attacker lost combat, he can see enemy starting units with scouts.
        total.to_string()
    } else {
        "?".to_string()
    };

    ui.add_enabled_ui(text != "0", |ui| {
        let response = ui
            .add_image(images.get(unit.to_lowername()), [image_size; 2])
            .on_hover_small_ext(unit.to_name())
            .on_disabled_hover_small_ext(unit.to_name());

        let escaped = if side == Side::Defender && can_see {
            report.escaped_defenders(unit)
        } else {
            0
        };
        let response = if escaped > 0 {
            response.on_hover_text(format!("{escaped} withdrew to the homeworld."))
        } else {
            response
        };

        ui.add_text_on_image(
            text,
            if can_see && lost > 0 {
                Color32::RED
            } else {
                Color32::WHITE
            },
            text_style,
            response.rect.left_bottom(),
            Align2::LEFT_BOTTOM,
        );
    });
}

/// Returns whether a report discloses one unit through participation, victory, or spy intel.
fn mission_report_unit_is_visible(
    report: &MissionReport,
    player_id: PlayerId,
    side: &Side,
    unit: &Unit,
) -> bool {
    report.can_see(side, player_id)
        || mission_report_unit_is_revealed_by_probes(report, player_id, side, unit)
}

fn mission_report_unit_is_revealed_by_probes(
    report: &MissionReport,
    player_id: PlayerId,
    side: &Side,
    unit: &Unit,
) -> bool {
    report.mission.owner == player_id
        && *side == Side::Defender
        && unit.revealed_by_probes_on_world(report.scout_probes, report.planet.is_moon())
}

/// Draws a full-size, two-column unit group in a mission report.
fn draw_army_grid(
    ui: &mut Ui,
    name: &str,
    army: &[Unit],
    report: &MissionReport,
    player: &Player,
    images: &ImageIds,
) {
    let side = if name == "attacker" {
        Side::Attacker
    } else {
        Side::Defender
    };

    egui::Grid::new(name).striped(false).num_columns(2).spacing([8., 8.]).show(ui, |ui| {
        for (i, unit) in army.iter().enumerate() {
            draw_mission_report_unit(
                ui,
                unit,
                report,
                player,
                side.clone(),
                (65.0, TextStyle::Body),
                images,
            );

            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
}

const CRAWLER_SALVAGE_ICON_SIZE: [f32; 2] = [48.0, 30.0];
const CRAWLER_SALVAGE_RESOURCE_GAP: f32 = 18.0;
const CRAWLER_SALVAGE_ICON_VALUE_GAP: f32 = 6.0;
const CRAWLER_SALVAGE_LABEL_VERTICAL_OFFSET: f32 = 6.0;
const COMBAT_DETAILS_FOOTER_BOTTOM_GAP: f32 = 20.0;
const CRAWLER_SALVAGE_RESOURCE_ORDER: [ResourceName; 3] =
    [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium];

fn combat_report_shows_salvage(total: bool, round: usize, round_count: usize) -> bool {
    total || round == round_count
}

/// Draws the final resources recovered by the defending Crawlers when that outcome is visible.
fn draw_crawler_salvage(
    ui: &mut Ui,
    report: &MissionReport,
    player: &Player,
    images: &ImageIds,
) -> Option<Response> {
    if !report.can_see(&Side::Defender, player.id) {
        return None;
    }

    let salvage = report.defender_salvage();
    if salvage == Resources::default() {
        return None;
    }

    Some(
        ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = CRAWLER_SALVAGE_RESOURCE_GAP;
            let label = ui.painter().layout_no_wrap(
                "Recovered:".to_string(),
                TextStyle::Body.resolve(ui.style()),
                ui.visuals().text_color(),
            );
            let (label_rect, _) = ui.allocate_exact_size(
                egui::vec2(label.size().x, CRAWLER_SALVAGE_ICON_SIZE[1]),
                Sense::hover(),
            );
            ui.painter().galley(
                label_rect.center() - label.size() * 0.5
                    + egui::vec2(0.0, CRAWLER_SALVAGE_LABEL_VERTICAL_OFFSET),
                label,
                ui.visuals().text_color(),
            );
            for resource in CRAWLER_SALVAGE_RESOURCE_ORDER {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = CRAWLER_SALVAGE_ICON_VALUE_GAP;
                    ui.add_image(images.get(resource.to_lowername()), CRAWLER_SALVAGE_ICON_SIZE);
                    ui.label(format_thousands(salvage.get(&resource)));
                });
            }
        })
        .response
        .on_hover_small_ext(
            "Resources recovered from destroyed ground defenses by the surviving Crawlers.",
        ),
    )
}

/// Draws the combat army grid interface and emits any resulting local actions.
fn draw_combat_army_grid(
    ui: &mut Ui,
    name: &str,
    state: &mut UiState,
    round: &CombatRoundView<'_>,
    units: Vec<Unit>,
    side: Side,
    planetary_shield_overloaded: bool,
    dock_mode: crate::core::units::operations::SpaceDockMode,
    images: &ImageIds,
) -> bool {
    let own = round.units(&side);
    let enemy_filter = state
        .combat_report_hover
        .as_ref()
        .and_then(|(unit, hovered_side)| (*hovered_side != side).then_some(*unit));
    let enemy = round
        .units(&side.opposite())
        .filter(|unit| enemy_filter.is_none_or(|filter| unit.unit == filter));

    let total_ps = EnergyGrid::default().planetary_shield(
        round.buildings.amount(&Unit::planetary_shield()),
        planetary_shield_overloaded,
    );

    let n_columns = if units.iter().any(Unit::is_building) {
        1
    } else {
        2
    };

    let mut any_hovered = false;
    egui::Grid::new(name).striped(false).num_columns(n_columns).spacing([8., 25.]).show(ui, |ui| {
        for (i, (unit, count)) in units
            .into_iter()
            .filter_map(|u| {
                if u.is_building() {
                    Some((u, round.buildings.amount(&u)))
                } else {
                    let mut seen = HashSet::new();
                    let n = own.clone().filter(|cu| cu.unit == u && seen.insert(cu.id)).count();
                    (n > 0).then_some((u, n))
                }
            })
            .enumerate()
        {
            let n_repaired = own
                .clone()
                .filter_map(|cu| (cu.unit == unit).then_some(cu.repairs.len()))
                .sum::<usize>();
            let (n_shots, kills) = enemy
                .clone()
                .flat_map(|unit| &unit.shots)
                .filter(|shot| shot.unit == Some(unit))
                .fold((0, 0), |(shots, kills), shot| (shots + 1, kills + usize::from(shot.killed)));
            let lost = match unit {
                Unit::Defense(Defense::InterplanetaryMissile) => count,
                Unit::Defense(Defense::AntiballisticMissile) => round.antiballistic_fired,
                _ => kills,
            };

            let hovering_repair_truck =
                matches!(state.combat_report_hover, Some((Unit::Defense(Defense::RepairTruck), _)));

            ui.add_enabled_ui(
                state
                    .combat_report_hover
                    .as_ref()
                    .is_none_or(|(u, s)| (*s != side || *u == Unit::repair_truck()) || *u == unit),
                |ui| {
                    let response = ui
                        .add_image(images.get(unit.to_lowername()), [70.; 2])
                        .on_hover_small_ext(unit.to_name());

                    if response.hovered() && !unit.is_building() {
                        any_hovered = true;
                        state.combat_report_hover = Some((unit, side.clone()));
                    }

                    let text = if hovering_repair_truck && side == Side::Defender {
                        if n_repaired > 0 {
                            Some(format!("❤{n_repaired}"))
                        } else {
                            None
                        }
                    } else if n_shots > 0 {
                        Some(format!("💥{n_shots}"))
                    } else {
                        None
                    };

                    if let Some(text) = text {
                        ui.add_text_on_image(
                            text,
                            Color32::WHITE,
                            TextStyle::Small,
                            response.rect.right_top() - egui::Vec2::new(2., -3.),
                            Align2::RIGHT_TOP,
                        );
                    }

                    ui.add_text_on_image(
                        if lost > 0 {
                            format!("{lost}/{count}")
                        } else {
                            count.to_string()
                        },
                        if lost > 0 {
                            Color32::RED
                        } else {
                            Color32::WHITE
                        },
                        TextStyle::Body,
                        response.rect.left_bottom(),
                        Align2::LEFT_BOTTOM,
                    );

                    let all_cu = own.clone().filter(|cu| cu.unit == unit);
                    let (hull, shield) = if hovering_repair_truck && side == Side::Defender {
                        (
                            all_cu
                                .clone()
                                .map(|cu| cu.repairs.iter().sum::<usize>() as f32)
                                .sum::<f32>()
                                .safe_div(
                                    (count * dock_mode.combat_stat(unit, unit.hull())) as f32,
                                ),
                            0.,
                        )
                    } else if unit.is_building() {
                        if unit == Unit::planetary_shield() {
                            let mut ps = round.planetary_shield as f32;
                            if let Some((hu, hs)) = &state.combat_report_hover {
                                if *hs != side {
                                    ps = enemy
                                        .clone()
                                        .filter(|cu| cu.unit == *hu)
                                        .flat_map(|cu| cu.shots.iter())
                                        .filter(|s| s.unit.is_some_and(|u| u == unit))
                                        .fold(0., |s_acc, s| {
                                            s_acc + s.planetary_shield_damage as f32
                                        });
                                }
                            }

                            (f32::NAN, ps / total_ps as f32)
                        } else {
                            (f32::NAN, f32::NAN)
                        }
                    } else {
                        let mut shield = all_cu
                            .clone()
                            .map(|cu| {
                                if lost == count {
                                    0.
                                } else {
                                    cu.shield as f32
                                }
                            })
                            .sum::<f32>()
                            .safe_div(
                                (all_cu.clone().count()
                                    * dock_mode.combat_stat(unit, unit.shield()))
                                    as f32,
                            );

                        let mut hull = all_cu
                            .clone()
                            .fold(HashMap::<_, f32>::new(), |mut map, cu| {
                                let val = if lost == count {
                                    0.
                                } else {
                                    cu.hull as f32
                                };
                                map.entry(cu.id).and_modify(|m| *m = (*m).min(val)).or_insert(val);
                                map
                            })
                            .values()
                            .sum::<f32>()
                            .safe_div((count * dock_mode.combat_stat(unit, unit.hull())) as f32);

                        if let Some((hu, hs)) = &state.combat_report_hover {
                            if *hs != side {
                                let (s_sum, h_sum) = enemy
                                    .clone()
                                    .filter(|cu| cu.unit == *hu)
                                    .flat_map(|cu| cu.shots.iter())
                                    .filter(|s| s.unit.is_some_and(|u| u == unit))
                                    .fold((0., 0.), |(s_acc, h_acc), s| {
                                        (
                                            s_acc + s.shield_damage as f32,
                                            h_acc + s.hull_damage as f32,
                                        )
                                    });

                                // Total shield when hover is not well-defined -> clamp to range for now
                                shield = s_sum
                                    .safe_div(
                                        (count * dock_mode.combat_stat(unit, unit.shield())) as f32,
                                    )
                                    .min(1.);
                                hull = h_sum.safe_div(
                                    (count * dock_mode.combat_stat(unit, unit.hull())) as f32,
                                );
                            }
                        }

                        (hull, shield)
                    };

                    for (i, (value, color)) in [shield, hull]
                        .into_iter()
                        .zip([SHIELD_COLOR.to_color32(), HEALTH_COLOR.to_color32()])
                        .enumerate()
                    {
                        if !value.is_nan() {
                            let bar = egui::Rect::from_min_max(
                                egui::pos2(
                                    response.rect.left(),
                                    response.rect.bottom() + i as f32 * 10.,
                                ),
                                egui::pos2(
                                    response.rect.right(),
                                    response.rect.bottom() + (i + 1) as f32 * 10.,
                                ),
                            );

                            ui.painter().rect_filled(bar, 0., BG2_COLOR.to_color32());

                            let filled = egui::Rect::from_min_max(
                                bar.min,
                                egui::pos2(bar.min.x + bar.width() * value, bar.max.y),
                            );

                            ui.painter().rect_filled(filled, 0., color);
                        }
                    }
                },
            );

            if n_columns == 1 || i % 2 == 1 {
                ui.end_row();
            }
        }
    });

    any_hovered
}

/// Returns the stationary structures shown in the defender's final combat-details column.
fn combat_defender_structure_column(
    report: &MissionReport,
    round: &CombatRoundView<'_>,
) -> Vec<Unit> {
    if report.mission.objective == Icon::MissileStrike {
        return Vec::new();
    }

    let mut units = Vec::with_capacity(2);
    if round.units(&Side::Defender).any(|unit| unit.unit == Unit::space_dock()) {
        units.push(Unit::space_dock());
    }
    if report.planet.army.amount(&Unit::planetary_shield()) > 0 {
        units.push(Unit::planetary_shield());
    }
    units
}

const RESOURCE_BAR_TOP: f32 = 9.0;
const RESOURCE_BAR_SIDE_INSET: f32 = 9.0;
const RESOURCE_BAR_ROW_HEIGHT: f32 = 50.0;
const RESOURCE_BAR_VERTICAL_MARGIN: f32 = 4.0;
const RESOURCE_SUMMARY_HORIZONTAL_PADDING: f32 = 4.0;
const RESOURCE_SUMMARY_TEXT_VERTICAL_OFFSET: f32 = 2.0;
const RESOURCE_IMAGE_BORDER_COLOR: Color32 = Color32::from_rgb(116, 211, 245);

fn paint_bordered_resource_image(ui: &Ui, image: egui::TextureId, rect: egui::Rect, width: f32) {
    ui.painter().image(
        image,
        rect.shrink(width * 0.5),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(6),
        Stroke::new(width, RESOURCE_IMAGE_BORDER_COLOR),
        StrokeKind::Inside,
    );
}

/// Returns the bottom edge reserved by the scaled top resource bar.
pub(crate) fn resource_bar_bottom(viewport: egui::Vec2) -> f32 {
    let scale = strategic_hud_scale(viewport);
    RESOURCE_BAR_TOP * scale
        + RESOURCE_BAR_ROW_HEIGHT * scale
        + 2.0 * (RESOURCE_BAR_VERTICAL_MARGIN * scale).round()
        + 2.0 * scale
}

fn resource_summary_style(compact: bool, scale: f32) -> (egui::Vec2, f32, f32, f32) {
    let (icon_size, spacing, label_size, value_size) = if compact {
        (egui::vec2(48.0, 31.0), 8.0, 9.0, 22.0)
    } else {
        (egui::vec2(64.0, 40.0), 11.0, 11.0, 28.0)
    };

    (icon_size * scale, spacing * scale, label_size * scale, value_size * scale)
}

fn resource_summary_width(ui: &Ui, label: &str, value: &str, compact: bool, scale: f32) -> f32 {
    let (icon_size, spacing, label_size, value_size) = resource_summary_style(compact, scale);
    let label_width = ui
        .painter()
        .layout_no_wrap(
            label.to_owned(),
            egui::FontId::new(label_size, FontFamily::Proportional),
            Color32::WHITE,
        )
        .size()
        .x;
    let value_width = ui
        .painter()
        .layout_no_wrap(
            value.to_owned(),
            egui::FontId::new(value_size, FontFamily::Proportional),
            Color32::WHITE,
        )
        .size()
        .x;

    RESOURCE_SUMMARY_HORIZONTAL_PADDING * scale * 2.0
        + icon_size.x
        + spacing
        + label_width.max(value_width)
}

/// Draws one labeled value in the compact resource summary row.
fn draw_resource_summary(
    ui: &mut Ui,
    icon: egui::TextureId,
    label: &str,
    value: &str,
    compact: bool,
    scale: f32,
    bordered: bool,
) -> Response {
    draw_resource_summary_with_value_color(
        ui,
        icon,
        label,
        value,
        Color32::WHITE,
        compact,
        scale,
        bordered,
    )
}

/// Draws one resource summary with an explicit value color for warning states.
fn draw_resource_summary_with_value_color(
    ui: &mut Ui,
    icon: egui::TextureId,
    label: &str,
    value: &str,
    value_color: Color32,
    compact: bool,
    scale: f32,
    bordered: bool,
) -> Response {
    let (icon_size, spacing, label_size, value_size) = resource_summary_style(compact, scale);
    let label = ui.painter().layout_no_wrap(
        label.to_owned(),
        egui::FontId::new(label_size, FontFamily::Proportional),
        Color32::from_rgb(166, 188, 211),
    );
    let value = ui.painter().layout_no_wrap(
        value.to_owned(),
        egui::FontId::new(value_size, FontFamily::Proportional),
        value_color,
    );
    let horizontal_padding = RESOURCE_SUMMARY_HORIZONTAL_PADDING * scale;
    let width =
        horizontal_padding * 2.0 + icon_size.x + spacing + label.size().x.max(value.size().x);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, RESOURCE_BAR_ROW_HEIGHT * scale), Sense::hover());

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + horizontal_padding + icon_size.x * 0.5, rect.center().y),
        icon_size,
    );
    if bordered {
        paint_bordered_resource_image(ui, icon, icon_rect, 2.0 * scale);
    } else {
        ui.painter().image(
            icon,
            icon_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
    }

    let text_x = icon_rect.right() + spacing;
    // The font's visible glyphs sit slightly above its line box, so this small optical offset
    // makes the label/value stack look centered rather than mathematically centered but high.
    let text_height = label.size().y + value.size().y;
    let text_top =
        rect.center().y - text_height * 0.5 + RESOURCE_SUMMARY_TEXT_VERTICAL_OFFSET * scale;
    let value_top = text_top + label.size().y;
    ui.painter().galley(egui::pos2(text_x, text_top), label, Color32::WHITE);
    ui.painter().galley(egui::pos2(text_x, value_top), value, value_color);

    response
}

fn energy_balance_text(energy: EnergyGrid) -> String {
    match energy.balance() {
        balance if balance > 0 => format!("+{balance}"),
        balance => balance.to_string(),
    }
}

fn energy_balance_color(energy: EnergyGrid) -> Color32 {
    if energy.balance() < 0 {
        Color32::RED
    } else {
        Color32::WHITE
    }
}

fn projected_energy(map: &Map, player: &Player, action_demand: usize) -> EnergyGrid {
    EnergyGrid::for_player_next_turn(player.id, map).with_action_demand(action_demand)
}

fn pending_railgun_energy_demand(
    map: &Map,
    player_id: crate::core::identity::PlayerId,
    pending: &PendingTurnCommands,
) -> usize {
    pending
        .commands
        .iter()
        .chain(&pending.queued_commands)
        .find_map(|command| match command {
            TurnCommand::FireOrbitalRailguns {
                target,
            } => Some(orbital_railgun_origins(map, player_id, *target).len()),
            _ => None,
        })
        .map_or(0, orbital_railgun_fire_energy_cost)
}

fn energy_world_breakdown(map: &Map, player: &Player) -> Vec<(String, EnergyGrid)> {
    map.planets
        .iter()
        .filter(|planet| {
            (player.owns(planet) || (planet.is_moon() && player.controls(planet)))
                && (!planet.is_moon() || planet.has_buildings())
                && !planet.is_destroyed
        })
        .sorted_by_key(|planet| world_shortcut_order(planet, player))
        .map(|planet| {
            let projected = next_turn_planet(planet);
            (planet.name.clone(), EnergyGrid::for_world(map, &projected))
        })
        .collect()
}

#[derive(Debug, PartialEq, Eq)]
struct ResourceWorldProduction {
    name: String,
    amount: usize,
    terraformer_modifier_percent: i32,
    recycler: ResourceProductionRange,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ResourceProductionRange {
    minimum: usize,
    maximum: usize,
}

impl ResourceProductionRange {
    fn rounded_mean(self) -> usize {
        self.minimum.saturating_add(self.half_range())
    }

    fn half_range(self) -> usize {
        let span = self.maximum.saturating_sub(self.minimum);
        span / 2 + span % 2
    }
}

type RecyclerProductionRanges = BTreeMap<PlanetId, (Resources, Resources)>;

fn next_turn_planet(planet: &Planet) -> Planet {
    let mut projected = planet.clone();
    projected.produce();
    projected
}

fn resource_production(map: &Map, player: &Player, action_demand: usize) -> Resources {
    let raw = map
        .planets
        .iter()
        .filter(|planet| player.owns(planet))
        .map(|planet| next_turn_planet(planet).resource_production())
        .sum();
    projected_energy(map, player, action_demand).scale_resources(raw)
}

fn projected_recycler_production_ranges(
    map: &Map,
    player: &Player,
    turn: usize,
) -> RecyclerProductionRanges {
    let debris = debris_sites(player.reports.iter(), turn);
    let sources = recycler_sources(map, &debris);

    map.planets
        .iter()
        .filter(|planet| player.owns(planet))
        .filter_map(|planet| {
            let projected = next_turn_planet(planet);
            let level =
                projected.army.amount(&Unit::Building(Building::Recycler)).min(Building::MAX_LEVEL);
            let source = sources.get(&planet.id).copied()?;
            (level > 0).then(|| {
                let (minimum, maximum) = source.output_range();
                let minimum = projected.operations.recycler_output(minimum * level);
                let maximum = projected.operations.recycler_output(maximum * level);
                (planet.id, (minimum, maximum))
            })
        })
        .collect()
}

fn resource_production_text(production: usize, recycler: ResourceProductionRange) -> String {
    if recycler.maximum > 0 {
        format!(
            "+{} ± {}",
            production.saturating_add(recycler.rounded_mean()),
            recycler.half_range()
        )
    } else {
        format!("+{production}")
    }
}

fn terraformer_modifier_percent(planet: &Planet, resource: ResourceName) -> i32 {
    let level = planet.army.amount(&Unit::Building(Building::Terraformer)).min(Building::MAX_LEVEL);
    if level == 0 || planet.terraformer_focus.is_none() {
        return 0;
    }

    let (positive, percent_per_level) = if planet.terraformer_focus == Some(resource) {
        (true, TERRAFORMER_FOCUS_BONUS_PERCENT_PER_LEVEL)
    } else {
        (false, TERRAFORMER_OTHER_PENALTY_PERCENT_PER_LEVEL)
    };
    let percent = percent_per_level.saturating_mul(level).min(i32::MAX as usize) as i32;
    if positive {
        percent
    } else {
        -percent
    }
}

fn resource_world_breakdown(
    map: &Map,
    player: &Player,
    resource: ResourceName,
    action_demand: usize,
    recycler_ranges: &RecyclerProductionRanges,
) -> Vec<ResourceWorldProduction> {
    let energy = projected_energy(map, player, action_demand);
    let mut accumulated_raw = 0usize;
    let mut accumulated_scaled = 0usize;
    map.planets
        .iter()
        .filter(|planet| player.owns(planet))
        .sorted_by_key(|planet| world_shortcut_order(planet, player))
        .map(|planet| {
            let projected = next_turn_planet(planet);
            let raw = projected.resource_production().get(&resource);
            accumulated_raw = accumulated_raw.saturating_add(raw);
            let mut accumulated = Resources::default();
            *accumulated.get_mut(&resource) = accumulated_raw;
            let scaled = energy.scale_resources(accumulated).get(&resource);
            let amount = scaled.saturating_sub(accumulated_scaled);
            accumulated_scaled = scaled;
            ResourceWorldProduction {
                name: planet.name.clone(),
                amount,
                terraformer_modifier_percent: terraformer_modifier_percent(&projected, resource),
                recycler: recycler_ranges.get(&planet.id).map_or_else(
                    ResourceProductionRange::default,
                    |(minimum, maximum)| ResourceProductionRange {
                        minimum: minimum.get(&resource),
                        maximum: maximum.get(&resource),
                    },
                ),
            }
        })
        .collect()
}

fn draw_resource_world_breakdown(
    ui: &mut Ui,
    map: &Map,
    player: &Player,
    resource: ResourceName,
    action_demand: usize,
    recycler_ranges: &RecyclerProductionRanges,
) {
    ui.spacing_mut().item_spacing.y = 2.0;
    for world in resource_world_breakdown(map, player, resource, action_demand, recycler_ranges) {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.small(format!(
                "{}: {}",
                world.name,
                resource_production_text(world.amount, world.recycler)
            ));
            if world.terraformer_modifier_percent != 0 {
                let color = if world.terraformer_modifier_percent > 0 {
                    HEALTH_COLOR.to_color32()
                } else {
                    Color32::RED
                };
                ui.colored_label(
                    color,
                    RichText::new(format!("({:+}%)", world.terraformer_modifier_percent)).small(),
                );
            }
        });
    }
}

fn draw_resource_production_row(
    ui: &mut Ui,
    map: &Map,
    player: &Player,
    resource: ResourceName,
    action_demand: usize,
    recycler_ranges: &RecyclerProductionRanges,
) -> Response {
    let energy = projected_energy(map, player, action_demand);
    let production = resource_production(map, player, action_demand).get(&resource);
    let recycler = recycler_ranges.values().fold(
        ResourceProductionRange::default(),
        |mut total, (minimum, maximum)| {
            total.minimum = total.minimum.saturating_add(minimum.get(&resource));
            total.maximum = total.maximum.saturating_add(maximum.get(&resource));
            total
        },
    );
    let response = ui
        .horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let production =
                ui.small(format!("Production: {}", resource_production_text(production, recycler)));
            let energy_penalty = 100usize.saturating_sub(energy.efficiency_percent());
            if energy_penalty > 0 {
                production.union(ui.colored_label(
                    Color32::RED,
                    RichText::new(format!("(-{energy_penalty}%)")).small(),
                ))
            } else {
                production
            }
        })
        .inner
        .on_hover_cursor(CursorIcon::Default);
    if response.hovered() {
        ui.add_space(2.0);
        ui.indent("Production", |ui| {
            draw_resource_world_breakdown(
                ui,
                map,
                player,
                resource,
                action_demand,
                recycler_ranges,
            );
        });
        ui.add_space(2.0);
    }
    response
}

const ENERGY_DESCRIPTION: &str = "Energy powers and maintains buildings across your empire.";

fn draw_energy_world_breakdown(ui: &mut Ui, map: &Map, player: &Player) {
    ui.spacing_mut().item_spacing.y = 2.0;
    for (name, energy) in energy_world_breakdown(map, player) {
        ui.colored_label(
            Color32::WHITE,
            RichText::new(format!("{name}: {}", energy_balance_text(energy))).small(),
        );
    }
}

fn draw_energy_production_row(
    ui: &mut Ui,
    map: &Map,
    player: &Player,
    action_demand: usize,
) -> Response {
    let energy = projected_energy(map, player, action_demand);
    let response = ui
        .small(format!("Production: {}", energy_balance_text(energy)))
        .on_hover_cursor(CursorIcon::Default);
    if response.hovered() {
        ui.add_space(2.0);
        ui.indent("Production", |ui| {
            draw_energy_world_breakdown(ui, map, player);
            if action_demand > 0 {
                ui.colored_label(
                    Color32::WHITE,
                    RichText::new(format!("Railgun fire: -{action_demand}")).small(),
                );
            }
        });
        ui.add_space(2.0);
    }
    response
}

fn draw_energy_tooltip(
    ui: &mut Ui,
    map: &Map,
    player: &Player,
    images: &ImageIds,
    action_demand: usize,
    brief: bool,
) -> egui::Rect {
    ui.horizontal(|ui| {
        let image_rect = ui.add_image(images.get("energy"), [130.0, 90.0]).rect;
        ui.vertical(|ui| {
            ui.set_max_width(360.0);
            ui.label(RichText::new("Energy").strong());
            ui.separator();
            ui.scope(|ui| {
                let labels_selectable = ui.style().interaction.selectable_labels;
                ui.style_mut().interaction.selectable_labels = true;
                ui.scope(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    draw_energy_production_row(ui, map, player, action_demand);
                });
                ui.style_mut().interaction.selectable_labels = labels_selectable;
                if !brief {
                    ui.add_space(3.0);
                    ui.small(ENERGY_DESCRIPTION);
                }
            });
        });
        image_rect
    })
    .inner
}

/// Reserves breathing room between resource summaries.
fn draw_resource_gap(ui: &mut Ui, width: f32, scale: f32) {
    ui.allocate_exact_size(egui::vec2(width, RESOURCE_BAR_ROW_HEIGHT * scale), Sense::hover());
}

fn resource_bar_gap(compact: bool, scale: f32) -> f32 {
    let gap = if compact {
        10.0
    } else {
        24.0
    };
    gap * scale
}

fn resource_bar_content_width(
    ui: &Ui,
    settings: &Settings,
    map: &Map,
    player: &Player,
    compact: bool,
    scale: f32,
    action_demand: usize,
) -> f32 {
    let mut width = resource_summary_width(ui, "TURN", &settings.turn.to_string(), compact, scale);
    for resource in ResourceName::iter() {
        width += resource_summary_width(
            ui,
            &resource.to_name().to_uppercase(),
            &player.resources.get(&resource).to_string(),
            compact,
            scale,
        );
    }
    let energy = projected_energy(map, player, action_demand);
    width += resource_summary_width(ui, "ENERGY", &energy_balance_text(energy), compact, scale);

    width + resource_bar_gap(compact, scale) * 4.0
}

fn draw_resource_tooltip_with_trade(
    ui: &mut Ui,
    resource: ResourceName,
    map: &Map,
    player: &Player,
    images: &ImageIds,
    action_demand: usize,
    trade_incoming: Resources,
    hub_repayment: Resources,
    brief: bool,
    recycler_ranges: &RecyclerProductionRanges,
) -> egui::Rect {
    ui.horizontal(|ui| {
        let image_rect = ui.add_image(images.get(resource.to_lowername()), [130.0, 90.0]).rect;
        ui.painter().rect_stroke(
            image_rect,
            egui::CornerRadius::same(6),
            Stroke::new(2.0, RESOURCE_IMAGE_BORDER_COLOR),
            StrokeKind::Inside,
        );
        ui.vertical(|ui| {
            ui.set_max_width(360.0);
            ui.label(RichText::new(resource.to_name()).strong());
            ui.separator();
            ui.scope(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                ui.style_mut().interaction.selectable_labels = true;
                draw_resource_production_row(
                    ui,
                    map,
                    player,
                    resource,
                    action_demand,
                    recycler_ranges,
                );
                let incoming = trade_incoming.get(&resource);
                if incoming > 0 {
                    ui.small(format!("Trade: +{incoming}"));
                }
                let repayment = hub_repayment.get(&resource);
                if repayment > 0 {
                    ui.colored_label(
                        Color32::from_rgb(229, 190, 107),
                        RichText::new(format!(
                            "Resource Market: -{repayment} after production this turn"
                        ))
                        .small(),
                    );
                }
            });
            if !brief {
                ui.add_space(3.0);
                ui.small(resource.description());
            }
        });
        image_rect
    })
    .inner
}

#[cfg(test)]
fn draw_resource_tooltip(
    ui: &mut Ui,
    resource: ResourceName,
    map: &Map,
    player: &Player,
    images: &ImageIds,
    action_demand: usize,
) -> egui::Rect {
    let recycler_ranges = projected_recycler_production_ranges(map, player, 0);
    draw_resource_tooltip_with_trade(
        ui,
        resource,
        map,
        player,
        images,
        action_demand,
        Resources::default(),
        Resources::default(),
        false,
        &recycler_ranges,
    )
}

/// Draws the resources interface and emits any resulting local actions.
fn draw_resources_with_trade(
    ui: &mut Ui,
    settings: &Settings,
    map: &Map,
    player: &Player,
    images: &ImageIds,
    compact: bool,
    scale: f32,
    action_demand: usize,
    trade_incoming: Resources,
    hub_repayment: Resources,
) {
    let gap = resource_bar_gap(compact, scale);
    let resource_count = ResourceName::iter().count();
    let recycler_ranges = projected_recycler_production_ranges(map, player, settings.turn);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

        let response = draw_resource_summary(
            ui,
            images.get("turn"),
            "TURN",
            &settings.turn.to_string(),
            compact,
            scale,
            true,
        );
        response.on_hover_ui(|ui| {
            ui.horizontal(|ui| {
                ui.add_image(images.get("turn"), [130.0, 90.0]);
                ui.vertical(|ui| {
                    ui.label(RichText::new("Turn").strong());
                    ui.separator();
                    if !settings.brief_hover_info {
                        ui.small("Current turn in the game.");
                    }
                });
            });
        });

        draw_resource_gap(ui, gap, scale);

        for (index, resource) in ResourceName::iter().enumerate() {
            let response = draw_resource_summary(
                ui,
                images.get(resource.to_lowername()),
                &resource.to_name().to_uppercase(),
                &player.resources.get(&resource).to_string(),
                compact,
                scale,
                true,
            );

            response.on_hover_ui(|ui| {
                draw_resource_tooltip_with_trade(
                    ui,
                    resource,
                    map,
                    player,
                    images,
                    action_demand,
                    trade_incoming,
                    hub_repayment,
                    settings.brief_hover_info,
                    &recycler_ranges,
                );
            });

            if index + 1 < resource_count {
                draw_resource_gap(ui, gap, scale);
            }
        }

        draw_resource_gap(ui, gap, scale);
        let energy = projected_energy(map, player, action_demand);
        let response = draw_resource_summary_with_value_color(
            ui,
            images.get("energy"),
            "ENERGY",
            &energy_balance_text(energy),
            energy_balance_color(energy),
            compact,
            scale,
            true,
        );
        response.on_hover_ui(|ui| {
            draw_energy_tooltip(ui, map, player, images, action_demand, settings.brief_hover_info);
        });
    });
}

#[cfg(test)]
fn draw_resources(
    ui: &mut Ui,
    settings: &Settings,
    map: &Map,
    player: &Player,
    images: &ImageIds,
    compact: bool,
    scale: f32,
    action_demand: usize,
) {
    draw_resources_with_trade(
        ui,
        settings,
        map,
        player,
        images,
        compact,
        scale,
        action_demand,
        Resources::default(),
        Resources::default(),
    );
}

/// Shows turn and resource totals in the same framed style as the world list.
fn draw_resources_widget_with_trade(
    context: &egui::Context,
    settings: &Settings,
    map: &Map,
    player: &Player,
    images: &ImageIds,
    action_demand: usize,
    trade_incoming: Resources,
    hub_repayment: Resources,
) -> egui::Rect {
    let scale = strategic_hud_scale(context.content_rect().size());
    egui::Area::new("stellarion_resources".into())
        .anchor(Align2::CENTER_TOP, egui::vec2(0.0, RESOURCE_BAR_TOP * scale))
        .movable(false)
        .constrain(true)
        .order(Order::Middle)
        .show(context, |ui| {
            let frame = scaled_hud_panel_frame(scale).inner_margin(scaled_margin(
                14.0,
                RESOURCE_BAR_VERTICAL_MARGIN,
                scale,
            ));
            let frame_width = frame.total_margin().sum().x;
            frame.show(ui, |ui| {
                let max_width = (context.content_rect().width()
                    - 2.0 * RESOURCE_BAR_SIDE_INSET * scale
                    - frame_width)
                    .max(1.0);
                let compact = resource_bar_content_width(
                    ui,
                    settings,
                    map,
                    player,
                    false,
                    scale,
                    action_demand,
                ) > max_width;
                draw_resources_with_trade(
                    ui,
                    settings,
                    map,
                    player,
                    images,
                    compact,
                    scale,
                    action_demand,
                    trade_incoming,
                    hub_repayment,
                );
            });
        })
        .response
        .rect
}

#[cfg(test)]
fn draw_resources_widget(
    context: &egui::Context,
    settings: &Settings,
    map: &Map,
    player: &Player,
    images: &ImageIds,
    action_demand: usize,
) -> egui::Rect {
    draw_resources_widget_with_trade(
        context,
        settings,
        map,
        player,
        images,
        action_demand,
        Resources::default(),
        Resources::default(),
    )
}

/// Explains climate and Solar Satellite output.
fn planet_temperature_tooltip(planet: &Planet, solar_band: Option<SolarBand>) -> String {
    let climate = if planet.is_moon() {
        match planet.temperature_emoji() {
            "🔥" => "Extreme heat and cold alternate because this moon has almost no atmosphere.",
            "☀" => "Large temperature swings occur because this moon has almost no atmosphere.",
            _ => "Frigid temperatures persist because this moon has almost no atmosphere.",
        }
    } else {
        match planet.kind {
            PlanetKind::Dry => {
                "High temperatures result from intense heating and a thin, dry atmosphere."
            },
            PlanetKind::Water => {
                "Moderate temperatures are stabilized by the planet's oceans and atmosphere."
            },
            PlanetKind::Gas => {
                "Low temperatures reflect the cold upper atmosphere of this gas giant."
            },
            PlanetKind::Metallic => {
                "Cool temperatures persist because the thin atmosphere retains little heat."
            },
            PlanetKind::Ice => "Frigid temperatures keep most of the surface frozen.",
            PlanetKind::Blue
            | PlanetKind::Brown
            | PlanetKind::Gray
            | PlanetKind::Red
            | PlanetKind::Yellow => unreachable!("lunar kind used by a planet"),
        }
    };
    if let Some(band) = solar_band {
        format!(
            "{climate} Solar Satellites produce {} Energy per level here.",
            band.satellite_energy()
        )
    } else {
        climate.to_string()
    }
}

/// Draws the planet overview interface and emits any resulting local actions.
fn draw_planet_overview(
    ui: &mut Ui,
    id: PlanetId,
    map: &mut Map,
    player: &mut Player,
    settings: &Settings,
    pending: &mut PendingTurnCommands,
    recall_protection: &mut MessageWriter<RecallProtectionMsg>,
    session: &MultiplayerSession,
    state: &mut UiState,
    detail_line_progress: &[f32; PLANET_DETAIL_LINE_COUNT],
    right_side: bool,
    images: &ImageIds,
) {
    let (n_owned, n_max_owned) = player.planets_owned(map, settings);
    let solar_band = map.solar_band(id);

    let planet = map.get_mut(id);

    ui.add_space(19.);

    let size = ui.available_size() - egui::vec2(15., 5.);
    let (rect, _) = ui.allocate_exact_size(size, Sense::click());

    let image = egui::Image::new(SizedTexture::new(images.get(planet.kind.to_lowername()), size));
    image.paint_at(ui, rect.translate(egui::vec2(8., 0.)));

    // Now overlay elements on top
    ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(5.)), |ui| {
        ui.vertical_centered(|ui| {
            ui.heading(&planet.name);
        });

        ui.add_space(10.);

        ui.with_layout(Layout::top_down(Align::RIGHT), |ui| {
            ui.spacing_mut().item_spacing.y = 6.;
            draw_sliding_text(
                ui,
                RichText::new(format!(
                    "🌎 Planet Kind: {}",
                    if !planet.is_moon() {
                        planet.kind.to_name()
                    } else {
                        "Moon".to_string()
                    }
                ))
                .small(),
                detail_line_progress[0],
                right_side,
            )
            .on_hover_small(planet.kind.description());
            draw_sliding_text(
                ui,
                RichText::new(format!("📐 Diameter: {}km", format_thousands(planet.diameter)))
                    .small(),
                detail_line_progress[1],
                right_side,
            )
            .on_hover_small(format!(
                "Smaller worlds are easier to destroy with War Suns and Orbital Railguns. \
                Size modifier: {:+}%.",
                f32::from(planet.death_ray_size_modifier_basis_points()) / 100.0,
            ));
            draw_sliding_text(
                ui,
                RichText::new(format!(
                    "{} Temperature: {}°C to {}°C",
                    planet.temperature_emoji(),
                    planet.temperature.0,
                    planet.temperature.1
                ))
                .small(),
                detail_line_progress[2],
                right_side,
            )
            .on_hover_small(planet_temperature_tooltip(planet, solar_band));
            draw_sliding_text(
                ui,
                RichText::new(format!(
                    "🗺 Coordinates: ({}, {})",
                    planet.position.x.round(),
                    planet.position.y.round()
                ))
                .small(),
                detail_line_progress[3],
                right_side,
            )
            .on_hover_small_ext("Position of the planet relative to the system's center.");
        });
    });

    let protection_available = player.controls(planet)
        && session.active_game.as_ref().is_some_and(|game| {
            game.persisted.state.players.iter().filter(|player| !player.spectator).count() >= 3
        });
    let action_size = egui::vec2(40.0, 40.0);
    let action_position = |index: usize| {
        egui::Rect::from_min_size(
            rect.left_bottom() + egui::vec2(20.0 + index as f32 * 48.0, -action_size.y - 7.0),
            action_size,
        )
    };
    let mut action_index = 0;
    if protection_available {
        let action_rect = action_position(action_index);
        action_index += 1;
        let response = ui
            .interact(
                action_rect,
                ui.id().with(("protection access action", planet.id)),
                Sense::click(),
            )
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_ui(|ui| draw_protection_access_tooltip(ui, planet, session));
        ui.add_tinted_image_painter(
            images.get("protect"),
            action_rect,
            planet_action_icon_tint(&response),
        );
        if response.clicked() {
            state.protection_access = Some(planet.id);
        }
    }

    if planet.is_protected_by(player.id) {
        let action_rect = action_position(action_index);
        action_index += 1;
        ui.add_enabled_ui(pending.can_accept_commands(), |ui| {
            let response = ui
                .interact(
                    action_rect,
                    ui.id().with(("recall protection action", planet.id)),
                    Sense::click(),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_small_ext("Recall your entire protection fleet to your home planet.")
                .on_disabled_hover_small_ext(
                    "Continue your turn before changing protection orders.",
                );
            ui.painter().image(
                images.get("recall"),
                action_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                planet_action_icon_tint(&response),
            );
            if response.clicked() {
                recall_protection.write(RecallProtectionMsg::new(planet.id));
            }
        });
    }

    if !planet.is_moon() {
        let owned =
            pending.can_accept_commands() && player.owns(planet) && player.home_planet != planet.id;
        let controlled =
            pending.can_accept_commands() && player.controls(planet) && !player.owns(planet);

        let rect = action_position(action_index);

        if owned {
            ui.add_enabled_ui(planet.buy.is_empty(), |ui| {
                let mut response = ui
                    .interact(rect, ui.id(), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_small_ext(
                        "Abandon this planet. The buildings on the planet remain. \
                        Defenses on the planet are destroyed.",
                    )
                    .on_disabled_hover_small_ext(
                        "A planet can't be abandoned when there are units being built.",
                    );

                if response.enabled() {
                    response = response.on_hover_cursor(CursorIcon::PointingHand);
                }

                ui.add_tinted_image_painter(
                    images.get("abandon"),
                    rect,
                    planet_action_icon_tint(&response),
                );

                if response.clicked() {
                    state.abandon_confirmation = Some(planet.id);
                }
            });
        } else if controlled {
            ui.add_enabled_ui(
                planet.army.controller().amount(&Unit::colony_ship()) > 0 && n_owned < n_max_owned,
                |ui| {
                    let mut response = ui
                        .interact(rect, ui.id(), Sense::click())
                        .on_hover_small_ext("Colonize this planet.")
                        .on_disabled_hover_small_ext(if n_owned >= n_max_owned {
                            "Maximum number of colonized planets reached."
                        } else {
                            "A Colony Ship is required on this planet to colonize it."
                        });

                    if response.enabled() {
                        response = response.on_hover_cursor(CursorIcon::PointingHand);
                    }

                    ui.add_tinted_image_painter(
                        images.get("colonize"),
                        rect,
                        planet_action_icon_tint(&response),
                    );

                    if response.clicked() {
                        state.colonize_confirmation = Some(planet.id);
                    }
                },
            );
        }
    }
}

/// Adds an approved colonize command and updates the local turn projection.
fn colonize_planet(
    id: PlanetId,
    map: &mut Map,
    player: &Player,
    message: &mut MessageWriter<MessageMsg>,
    pending: &mut PendingTurnCommands,
) {
    if !pending.push(TurnCommand::ColonizePlanet {
        planet_id: id,
    }) {
        message.write(MessageMsg::error(COMMAND_LIMIT_REACHED_MESSAGE));
        return;
    }

    let planet = map.get_mut(id);
    let colony_ships = planet.army.entry(Unit::colony_ship()).or_insert(1);
    *colony_ships = colony_ships.saturating_sub(1);
    planet.colonize(player.id);
    // The map presentation announces the ownership change once, including direct colonization
    // and colonies established by arriving missions.
}

/// Adds an approved abandon command and updates the local turn projection.
fn abandon_planet(
    id: PlanetId,
    map: &mut Map,
    player: &mut Player,
    settings: &Settings,
    message: &mut MessageWriter<MessageMsg>,
    pending: &mut PendingTurnCommands,
) {
    let planet = map.get_mut(id);
    let mission =
        Mission::from_mission(settings.turn, player.id, planet, planet, &Mission::default());

    if !pending.push(TurnCommand::AbandonPlanet {
        planet_id: planet.id,
    }) {
        message.write(MessageMsg::error(COMMAND_LIMIT_REACHED_MESSAGE));
        return;
    }
    planet.abandon();

    // Inject hidden report to show last_info that the planet is abandoned.
    if planet.controlled.is_none() {
        player.push_report(MissionReport {
            id: rand::random(),
            turn: settings.turn,
            mission,
            planet: planet.clone(),
            scout_probes: 0,
            surviving_attacker: Army::new(),
            surviving_defender: Army::new().into(),
            planet_colonized: false,
            planet_destroyed: false,
            destination_owned: None,
            destination_controlled: None,
            combat_report: None,
            hidden: true,
        });
    }

    // The map presentation observes this command-backed ownership change and announces it with
    // the same focused animation used for newly colonized and conquered planets.
}

/// Draws the overview interface and emits any resulting local actions.
fn draw_overview(
    ui: &mut Ui,
    planet: &Planet,
    home_planet: PlanetId,
    session: &MultiplayerSession,
    state: &mut UiState,
    images: &ImageIds,
) {
    ui.add_space(17.);

    let garrison_players = garrison_player_ids(planet);
    if garrison_players.len() <= 1 {
        state.garrison_hidden_players.remove(&planet.id);
    }
    let strength =
        filtered_garrison_fleet_strength(planet, state.garrison_hidden_players.get(&planet.id));
    let defense_strength = (!planet.is_moon()).then(|| {
        format_thousands(filtered_garrison_defense_strength(
            planet,
            state.garrison_hidden_players.get(&planet.id),
        ))
    });
    let (_, protection_filter) = draw_world_overview_header(
        ui,
        &format_thousands(strength),
        defense_strength.as_deref(),
        images,
        garrison_players.len() > 1,
    );
    if let Some(protection_filter) = protection_filter {
        on_immediate_hover_ui(protection_filter, |ui| {
            draw_garrison_filters(ui, planet, session, state);
        });
    }
    let hidden = state.garrison_hidden_players.get(&planet.id);

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

        ui.add_space(10.);

        for units in Unit::all_for_world(planet.is_moon(), planet.id == home_planet) {
            ui.add_space(5.);

            ui.vertical(|ui| {
                for unit in units {
                    let count = filtered_garrison_amount(planet, &unit, hidden);

                    let response = ui
                        .add_enabled_ui(count > 0, |ui| {
                            let response = ui.add_image(images.get(unit.to_lowername()), [50.; 2]);
                            draw_overview_unit_count(ui, response.rect, count);
                        })
                        .response;
                    if count > 0 {
                        response.on_hover_small(unit.to_name());
                    } else {
                        response.on_disabled_hover_small(unit.to_name());
                    }
                }
            });
        }
    });
}

/// Paints the combined count selected by the world's player filters.
fn draw_overview_unit_count(ui: &mut Ui, image_rect: egui::Rect, count: usize) {
    ui.set_clip_rect(ui.clip_rect().intersect(image_rect));
    ui.add_text_on_image(
        count.to_string(),
        Color32::WHITE,
        TextStyle::Body,
        image_rect.left_bottom(),
        Align2::LEFT_BOTTOM,
    );
}

/// Shows an interactive hover panel without the normal tooltip reveal delay.
fn on_immediate_hover_ui(response: Response, add_contents: impl FnOnce(&mut Ui)) -> Response {
    let open = response.hovered() || egui::Tooltip::should_show_tooltip(&response, true);
    let mut tooltip = egui::Tooltip::for_widget(&response);
    tooltip.popup = tooltip.popup.open(open);
    tooltip.show(add_contents);
    response
}

/// Returns every commander represented in this garrison, with the controller first.
fn garrison_player_ids(planet: &Planet) -> Vec<PlayerId> {
    let mut players = Vec::new();
    if planet.army.controller().has_army() {
        if let Some(controller) = planet.controlled.or(planet.owned) {
            players.push(controller);
        }
    }
    for (player_id, army) in planet.army.protectors() {
        if army.has_army() && !players.contains(&player_id) {
            players.push(player_id);
        }
    }
    players
}

/// Counts one unit across the controller and every protection fleet not hidden by the player.
fn filtered_garrison_amount(
    planet: &Planet,
    unit: &Unit,
    hidden: Option<&HashSet<PlayerId>>,
) -> usize {
    let controller = planet.controlled.or(planet.owned);
    let mut total = if controller
        .is_none_or(|player_id| hidden.is_none_or(|hidden| !hidden.contains(&player_id)))
    {
        planet.army.controller().amount(unit)
    } else {
        0
    };
    for (player_id, army) in planet.army.protectors() {
        if hidden.is_none_or(|hidden| !hidden.contains(&player_id)) {
            total = total.saturating_add(army.amount(unit));
        }
    }
    total
}

/// Returns the production-weighted strength of the visible ships stationed on one world.
fn filtered_garrison_fleet_strength(planet: &Planet, hidden: Option<&HashSet<PlayerId>>) -> usize {
    Unit::ships().iter().fold(0_usize, |strength, unit| {
        strength.saturating_add(
            filtered_garrison_amount(planet, unit, hidden).saturating_mul(unit.production()),
        )
    })
}

/// Returns the production-weighted strength of the visible stationary defenses on one planet.
fn filtered_garrison_defense_strength(
    planet: &Planet,
    hidden: Option<&HashSet<PlayerId>>,
) -> usize {
    Unit::defenses().into_iter().chain(std::iter::once(Unit::space_dock())).fold(
        0_usize,
        |strength, unit| {
            strength.saturating_add(
                filtered_garrison_amount(planet, &unit, hidden).saturating_mul(unit.production()),
            )
        },
    )
}

/// Adds the right-aligned fleet strength and optional protection-fleet filter.
fn draw_world_overview_header(
    ui: &mut Ui,
    fleet_strength: &str,
    defense_strength: Option<&str>,
    images: &ImageIds,
    show_protection_filter: bool,
) -> (Response, Option<Response>) {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), WORLD_OVERVIEW_HEADER_HEIGHT),
        Sense::hover(),
    );
    let strength_font = egui::FontId::proportional(WORLD_OVERVIEW_STRENGTH_FONT_SIZE);
    let strength_width = ui
        .painter()
        .layout_no_wrap(fleet_strength.to_owned(), strength_font.clone(), Color32::WHITE)
        .size()
        .x;
    let defense_width = defense_strength.map(|strength| {
        ui.painter()
            .layout_no_wrap(strength.to_owned(), strength_font.clone(), Color32::WHITE)
            .size()
            .x
    });
    let layout =
        world_overview_header_layout(rect, strength_width, defense_width, show_protection_filter);
    ui.painter().image(
        images.get("fleet"),
        layout.fleet_icon,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.painter().text(
        egui::pos2(
            layout.fleet_icon.right() + WORLD_OVERVIEW_ICON_TEXT_GAP,
            layout.fleet_badge.center().y,
        ),
        Align2::LEFT_CENTER,
        fleet_strength,
        strength_font.clone(),
        Color32::WHITE,
    );
    ui.interact(layout.fleet_badge, ui.id().with("world fleet strength"), Sense::hover())
        .on_hover_small("Fleet strength: total production points of the ships.");

    if let (Some(strength), Some(badge), Some(icon)) =
        (defense_strength, layout.defense_badge, layout.defense_icon)
    {
        ui.painter().image(
            images.get("defenses"),
            icon,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        ui.painter().text(
            egui::pos2(icon.right() + WORLD_OVERVIEW_ICON_TEXT_GAP, badge.center().y),
            Align2::LEFT_CENTER,
            strength,
            strength_font,
            Color32::WHITE,
        );
        ui.interact(badge, ui.id().with("world defense strength"), Sense::hover())
            .on_hover_small("Defense strength: total production points of the defenses.");
    }

    let protection_response = layout.protection_icon.map(|icon| {
        ui.painter().image(
            images.get("protect"),
            icon,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        ui.interact(icon, ui.id().with("world garrison filters"), Sense::hover())
    });

    (response, protection_response)
}

struct WorldOverviewHeaderLayout {
    fleet_badge: egui::Rect,
    fleet_icon: egui::Rect,
    protection_icon: Option<egui::Rect>,
    defense_badge: Option<egui::Rect>,
    defense_icon: Option<egui::Rect>,
}

fn world_overview_header_layout(
    rect: egui::Rect,
    strength_width: f32,
    defense_width: Option<f32>,
    show_protection_filter: bool,
) -> WorldOverviewHeaderLayout {
    let right_edge = rect.right() - WORLD_OVERVIEW_RIGHT_MARGIN;
    let protection_icon = show_protection_filter.then(|| {
        egui::Rect::from_center_size(
            egui::pos2(right_edge - WORLD_OVERVIEW_ICON_SIZE * 0.5, rect.center().y),
            egui::Vec2::splat(WORLD_OVERVIEW_ICON_SIZE),
        )
    });
    let content_right =
        protection_icon.map_or(right_edge, |icon| icon.left() - WORLD_OVERVIEW_PROTECTION_GAP);
    let defense_badge = defense_width.map(|width| {
        let badge_width = WORLD_OVERVIEW_ICON_SIZE + WORLD_OVERVIEW_ICON_TEXT_GAP + width;
        egui::Rect::from_center_size(
            egui::pos2(content_right - badge_width * 0.5, rect.center().y),
            egui::vec2(badge_width, rect.height()),
        )
    });
    let defense_icon = defense_badge.map(|badge| {
        egui::Rect::from_center_size(
            egui::pos2(badge.left() + WORLD_OVERVIEW_ICON_SIZE * 0.5, badge.center().y),
            egui::Vec2::splat(WORLD_OVERVIEW_ICON_SIZE),
        )
    });

    let badge_width = WORLD_OVERVIEW_ICON_SIZE + WORLD_OVERVIEW_ICON_TEXT_GAP + strength_width;
    let fleet_right =
        defense_badge.map_or(content_right, |badge| badge.left() - WORLD_OVERVIEW_BADGE_GAP);
    let fleet_badge = egui::Rect::from_center_size(
        egui::pos2(fleet_right - badge_width * 0.5, rect.center().y),
        egui::vec2(badge_width, rect.height()),
    );
    let fleet_icon = egui::Rect::from_center_size(
        egui::pos2(fleet_badge.left() + WORLD_OVERVIEW_ICON_SIZE * 0.5, fleet_badge.center().y),
        egui::Vec2::splat(WORLD_OVERVIEW_ICON_SIZE),
    );
    WorldOverviewHeaderLayout {
        fleet_badge,
        fleet_icon,
        protection_icon,
        defense_badge,
        defense_icon,
    }
}

/// Compact player-fleet checkbox used in the world overview's protection filter.
fn draw_garrison_filter_checkbox(
    ui: &mut Ui,
    width: f32,
    display_name: &str,
    player_color: Color32,
    checked: &mut bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(width.min(ui.available_width()), GARRISON_FILTER_HEIGHT),
        Sense::click(),
    );
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    if response.clicked() {
        *checked = !*checked;
    }

    let marker = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 13.0, rect.center().y),
        egui::Vec2::splat(13.0),
    );
    ui.painter().rect(
        marker,
        3.0,
        if *checked {
            player_color
        } else {
            Color32::TRANSPARENT
        },
        Stroke::new(
            1.25,
            if *checked {
                player_color
            } else {
                Color32::from_rgb(103, 130, 151)
            },
        ),
        StrokeKind::Inside,
    );
    if *checked {
        let check_stroke = Stroke::new(1.5, Color32::WHITE);
        let midpoint = egui::pos2(marker.left() + 5.5, marker.bottom() - 3.0);
        ui.painter().line_segment(
            [egui::pos2(marker.left() + 3.0, marker.center().y), midpoint],
            check_stroke,
        );
        ui.painter().line_segment(
            [midpoint, egui::pos2(marker.right() - 2.0, marker.top() + 3.0)],
            check_stroke,
        );
    }

    ui.painter().with_clip_rect(rect.shrink2(egui::vec2(5.0, 0.0))).text(
        egui::pos2(marker.right() + 6.0, rect.center().y),
        Align2::LEFT_CENTER,
        display_name,
        egui::FontId::proportional(GARRISON_FILTER_FONT_SIZE),
        player_color,
    );
    response
}

/// Draws persistent per-world player filters inside the protection-icon hover panel.
fn draw_garrison_filters(
    ui: &mut Ui,
    planet: &Planet,
    session: &MultiplayerSession,
    state: &mut UiState,
) {
    let players = garrison_player_ids(planet);
    if players.len() <= 1 {
        state.garrison_hidden_players.remove(&planet.id);
        return;
    }

    let font = egui::FontId::proportional(GARRISON_FILTER_FONT_SIZE);
    let content_width = players
        .iter()
        .map(|player_id| {
            let display_name = session
                .player_name(*player_id)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Player {player_id}"));
            ui.painter().layout_no_wrap(display_name, font.clone(), Color32::WHITE).size().x + 36.0
        })
        .fold(140.0_f32, f32::max)
        .min(220.0);
    let hidden = state.garrison_hidden_players.entry(planet.id).or_default();
    hidden.retain(|player_id| players.contains(player_id));

    ui.set_width(content_width);
    ui.spacing_mut().item_spacing.y = GARRISON_FILTER_GAP;
    for player_id in players {
        let display_name = session
            .player_name(player_id)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Player {player_id}"));
        let color = session.player_color(player_id).color().to_color32();
        let mut checked = !hidden.contains(&player_id);
        draw_garrison_filter_checkbox(ui, content_width, &display_name, color, &mut checked);
        if checked {
            hidden.remove(&player_id);
        } else {
            hidden.insert(player_id);
        }
    }

    if hidden.is_empty() {
        state.garrison_hidden_players.remove(&planet.id);
    }
}

/// Draws the report overview interface and emits any resulting local actions.
fn draw_report_overview(
    ui: &mut Ui,
    planet: &Planet,
    info: &PlanetInfo,
    home_planet: PlanetId,
    images: &ImageIds,
) {
    ui.add_space(17.);

    let strength = if Unit::ships().iter().all(|unit| info.army.contains_key(unit)) {
        format_thousands(Unit::ships().iter().fold(0_usize, |total, unit| {
            total.saturating_add(info.army.amount(unit).saturating_mul(unit.production()))
        }))
    } else {
        "?".to_owned()
    };
    let defense_strength = (!planet.is_moon()).then(|| {
        let mut defenses = Unit::defenses();
        defenses.push(Unit::space_dock());
        if defenses.iter().all(|unit| info.army.contains_key(unit)) {
            format_thousands(defenses.iter().fold(0_usize, |total, unit| {
                total.saturating_add(info.army.amount(unit).saturating_mul(unit.production()))
            }))
        } else {
            "?".to_owned()
        }
    });
    draw_world_overview_header(ui, &strength, defense_strength.as_deref(), images, false)
        .0
        .on_hover_small(format!("Intelligence from turn {}.", info.turn));

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

        ui.add_space(10.);
        for units in Unit::all_for_world(planet.is_moon(), planet.id == home_planet) {
            ui.add_space(5.);

            ui.vertical(|ui| {
                for unit in units {
                    let text = if let Some(n) = info.army.get(&unit) {
                        n.to_string()
                    } else {
                        "?".to_string()
                    };

                    ui.add_enabled_ui(text != "0", |ui| {
                        let response = ui.add_image(images.get(unit.to_lowername()), [50.; 2]);
                        ui.add_text_on_image(
                            text,
                            Color32::WHITE,
                            TextStyle::Body,
                            response.rect.left_bottom(),
                            Align2::LEFT_BOTTOM,
                        );
                    })
                    .response
                    .on_hover_small(unit.to_name())
                    .on_disabled_hover_small(unit.to_name());
                }
            });
        }
    });
}

/// Draws the mission fleet hover interface and emits any resulting local actions.
fn draw_mission_fleet_hover(
    ui: &mut Ui,
    mission: &Mission,
    map: &Map,
    player: &Player,
    images: &ImageIds,
) {
    let army = match mission.objective {
        Icon::MissileStrike => vec![Unit::interplanetary_missile()],
        Icon::Spy => vec![Unit::probe()],
        _ => Unit::ships(),
    };

    ui.add_space(17.);
    draw_world_overview_header(
        ui,
        &mission_fleet_strength_label(mission, map, player),
        None,
        images,
        false,
    );

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

        ui.add_space(32.);

        ui.vertical(|ui| {
            for unit in army.iter() {
                let n = mission.army.amount(unit);

                ui.add_enabled_ui(n > 0, |ui| {
                    let response = ui.add_image(images.get(unit.to_lowername()), [50.; 2]);
                    ui.add_text_on_image(
                        if mission_unit_amount_is_visible(mission, unit, map, player) {
                            n.to_string()
                        } else {
                            "?".to_string()
                        },
                        Color32::WHITE,
                        TextStyle::Body,
                        response.rect.left_bottom(),
                        Align2::LEFT_BOTTOM,
                    );
                })
                .response
                .on_hover_small(unit.to_name())
                .on_disabled_hover_small(unit.to_name());
            }
        });
    });
}

/// Returns whether this viewer can see one unit count in a travelling fleet.
fn mission_unit_amount_is_visible(
    mission: &Mission,
    unit: &Unit,
    map: &Map,
    player: &Player,
) -> bool {
    mission.owner == player.id
        || mission.is_incoming_protection_for(player.id)
        || player.spectator
        || mission.is_seen_by_phalanx(map, player).is_some_and(|level| unit.production() <= level)
        || mission.is_seen_by_radar(map, player).is_some_and(|level| unit.production() <= level)
}

/// Formats a mission's production-weighted ship strength without exposing concealed counts.
fn mission_fleet_strength_label(mission: &Mission, map: &Map, player: &Player) -> String {
    let fully_visible = mission.is_joint_attack_participant(player.id)
        || Unit::ships().iter().all(|unit| {
            mission.army.amount(unit) == 0
                || mission_unit_amount_is_visible(mission, unit, map, player)
        });
    if fully_visible {
        format_thousands(missions::fleet_strength(&mission.army))
    } else {
        "?".to_owned()
    }
}

struct CombatParticipant {
    name: String,
    color: Color32,
    strength: u128,
}

const NEUTRAL_COMBAT_COLOR: Color32 = Color32::from_rgb(190, 198, 210);

/// Returns only the fauna kinds that were present when this encounter began.
fn combat_report_fauna_units(report: &MissionReport) -> Vec<Unit> {
    let mut fauna = SpaceFauna::iter()
        .map(Unit::Fauna)
        .filter(|unit| report.planet.army.combined_amount(unit) > 0)
        .collect::<Vec<_>>();
    fauna.sort_by_key(|unit| (unit.production(), unit.hull(), unit.damage(), *unit));
    fauna
}

fn encounter_image_corner_radius(report: &MissionReport, size: f32) -> egui::CornerRadius {
    if report.is_space_fauna_encounter() {
        egui::CornerRadius::same((size * 0.5).round() as u8)
    } else {
        egui::CornerRadius::ZERO
    }
}

fn mission_report_destination_name<'a>(report: &MissionReport, destination: &'a Planet) -> &'a str {
    if report.is_space_fauna_encounter() {
        "Space Encounter"
    } else {
        &destination.name
    }
}

fn add_combat_report_destination_image(
    ui: &mut Ui,
    report: &MissionReport,
    images: &ImageIds,
    size: f32,
) -> Response {
    ui.add(
        egui::Image::new(SizedTexture::new(
            images.get(combat_selection_planet_image(report)),
            egui::Vec2::splat(size),
        ))
        .corner_radius(encounter_image_corner_radius(report, size)),
    )
}

fn combat_side_participants(
    report: &MissionReport,
    side: &Side,
    session: &MultiplayerSession,
) -> Vec<CombatParticipant> {
    if *side == Side::Defender && report.is_space_fauna_encounter() {
        return vec![CombatParticipant {
            name: report.planet.name.clone(),
            color: NEUTRAL_COMBAT_COLOR,
            strength: combat_fleet_strength(&report.planet.army.combined()),
        }];
    }
    if *side == Side::Defender && report.is_independent_population_encounter() {
        return vec![CombatParticipant {
            name: report
                .independent_population_name()
                .unwrap_or_else(|| "Independent Population".to_owned()),
            color: NEUTRAL_COMBAT_COLOR,
            strength: combat_fleet_strength(&report.planet.army.combined()),
        }];
    }
    let players = match side {
        Side::Attacker => report.attacker_players(),
        Side::Defender => report.defender_players(),
    };
    players
        .into_iter()
        .map(|id| CombatParticipant {
            name: session
                .player_name(id)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Player {id}")),
            color: session.player_color(id).color().to_color32(),
            strength: report.participant_fleet_strength(side, id),
        })
        .collect()
}

/// Draws a combat role followed by independently colored participant names.
fn draw_colored_combat_heading(ui: &mut Ui, role: &str, participants: &[CombatParticipant]) {
    let mut job = egui::text::LayoutJob::default();
    let font_id = TextStyle::Body.resolve(ui.style());
    let mut append = |text: &str, color| {
        job.append(
            text,
            0.0,
            egui::text::TextFormat {
                font_id: font_id.clone(),
                color,
                ..Default::default()
            },
        );
    };

    append(
        &if participants.is_empty() {
            role.to_owned()
        } else {
            format!("{role} · ")
        },
        Color32::WHITE,
    );
    for (index, participant) in participants.iter().enumerate() {
        if index > 0 {
            append(" + ", Color32::WHITE);
        }
        append(&participant.name, participant.color);
    }

    let available_width = ui.available_width().max(1.0);
    let full_size_galley = ui.painter().layout_job(job.clone());
    let heading_row_height = full_size_galley.size().y;
    if participants.len() > 1 {
        let heading_width = full_size_galley.size().x;
        if heading_width > available_width {
            let scale = available_width / heading_width;
            for section in &mut job.sections {
                section.format.font_id.size *= scale;
            }
        }
    }

    // Reserve the unscaled line height even when long allied names need a smaller font. This
    // keeps both strength bars on the same baseline and centers the fitted text beside the
    // full-size opposing heading.
    let (row, _) = ui
        .allocate_exact_size(egui::vec2(available_width, heading_row_height), egui::Sense::hover());
    let layout = if ui.layout().prefer_right_to_left() {
        Layout::right_to_left(Align::Center)
    } else {
        Layout::left_to_right(Align::Center)
    };
    let mut row_ui = ui.new_child(UiBuilder::new().max_rect(row).layout(layout));
    // A single label keeps the text's reading order stable when the attacker column is
    // right-aligned. Separate labels inherit that right-to-left layout and appear reversed.
    row_ui.add(egui::Label::new(job).wrap_mode(egui::TextWrapMode::Extend));
}

fn paint_combat_strength_bar(ui: &mut Ui, rect: egui::Rect, participants: &[CombatParticipant]) {
    let strengths = participants.iter().map(|participant| participant.strength).collect::<Vec<_>>();
    for (participant, (start, end)) in participants.iter().zip(combat_strength_ranges(&strengths)) {
        if end > start {
            ui.painter().rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(rect.left() + rect.width() * start, rect.top()),
                    egui::pos2(rect.left() + rect.width() * end, rect.bottom()),
                ),
                0.0,
                participant.color,
            );
        }
    }
}

fn draw_combat_strength_bar_line(ui: &mut Ui, width: f32, participants: &[CombatParticipant]) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 6.0), egui::Sense::hover());
    paint_combat_strength_bar(ui, rect, participants);
}

fn draw_combat_strength_bar(ui: &mut Ui, participants: &[CombatParticipant]) {
    ui.add_space(8.);
    draw_combat_strength_bar_line(ui, ui.available_width(), participants);
}

fn draw_mission_report_strength_bars(
    ui: &mut Ui,
    report: &MissionReport,
    session: &MultiplayerSession,
    attacker_width: f32,
) {
    let attackers = combat_side_participants(report, &Side::Attacker, session);
    let defenders = combat_side_participants(report, &Side::Defender, session);

    draw_combat_strength_bar_line(ui, attacker_width, &attackers);
    draw_combat_strength_bar_line(ui, ui.available_width(), &defenders);
}

/// Draws the combat report interface and emits any resulting local actions.
fn draw_combat_report(
    ui: &mut Ui,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    images: &ImageIds,
) {
    let Some(report_id) = state.combat_report else {
        return;
    };
    let Some((report, combat)) = player
        .reports
        .iter()
        .find(|report| report.id == report_id)
        .and_then(|report| report.combat_report.as_ref().map(|combat| (report, combat)))
        .filter(|(_, combat)| !combat.rounds.is_empty())
    else {
        state.combat_report = None;
        return;
    };
    state.combat_report_round = state.combat_report_round.clamp(1, combat.rounds.len());

    let origin = map.get(report.mission.origin);
    let destination = map.get(report.mission.destination);

    ui.add_space(5.);

    ui.horizontal(|ui| {
        ui.set_height(55.);
        ui.spacing_mut().item_spacing.x = 8.;

        ui.add_space(70.);

        ui.add_image(images.get(origin.image()), [35., 35.]);
        ui.add_space(5.);
        ui.small(&origin.name);

        ui.add_space(25.);

        ui.add_image(images.get(report.mission.objective.asset_key()), [25.; 2]);
        ui.add_image(images.get(report.mission.image(player)), [50.; 2]);
        ui.small(report.turn.to_string());

        ui.add_space(25.);

        let destination_name = if report.is_space_fauna_encounter() {
            &report.planet.name
        } else {
            &destination.name
        };
        ui.add(
            egui::Label::new(RichText::new(destination_name).small().color(
                if report.is_space_fauna_encounter() {
                    NEUTRAL_COMBAT_COLOR
                } else {
                    ui.visuals().text_color()
                },
            ))
            .wrap_mode(egui::TextWrapMode::Extend),
        );
        ui.add_space(5.);
        let resp = add_combat_report_destination_image(ui, report, images, 35.0);

        let size = [15., 15.];
        let pos = resp.rect.right_top() - egui::vec2(size[0], 0.);
        ui.put(
            egui::Rect::from_min_size(pos, size.into()),
            egui::Image::new(SizedTexture::new(images.get(report.image(player)), size)),
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(70.);

            ui.add_enabled_ui(!state.combat_report_total, |ui| {
                ui.add(
                    Slider::new(&mut state.combat_report_round, 1..=combat.rounds.len())
                        .step_by(1f64)
                        .show_value(false),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_small("Combat round for which to show the details.");

                ui.add_space(10.);

                ui.small(format!("Round: {}/{}", state.combat_report_round, combat.rounds.len()));
            });

            ui.add_space(30.);

            ui.add(toggle(&mut state.combat_report_total)).on_hover_small(
                "If enabled, the panel shows the total statistics over the whole combat \
                (sum over all rounds). If disabled, it shows the statistics per round.",
            );

            ui.add_space(10.);

            ui.small("Total:")
        });
    });

    let rounds = if state.combat_report_total {
        combat.rounds.as_slice()
    } else {
        &combat.rounds[state.combat_report_round - 1..state.combat_report_round]
    };
    let Some(round) = CombatRoundView::new(rounds) else {
        return;
    };

    let draw_stats = |ui: &mut Ui, side: Side, hover: &Option<(Unit, Side)>| {
        let stats = CombatStatistics::for_units(round.units(&side).filter(|unit| {
            hover
                .as_ref()
                .is_none_or(|(kind, hovered_side)| *kind == unit.unit && *hovered_side == side)
        }));

        let draw_row = |ui: &mut Ui, icon: &str, val: String, hover: &str| {
            ui.vertical_centered(|ui| {
                ui.label(icon).on_hover_small(hover);
            });
            ui.label(if stats.units == 0 {
                "--".to_string()
            } else {
                val
            })
            .on_hover_small(hover);
            ui.end_row();
        };

        egui::Grid::new("stats_grid").striped(false).num_columns(2).spacing([2., 6.]).show(
            ui,
            |ui| {
                draw_row(ui, "🛡", stats.shield_damage.fmt(), "Damage dealt to shields.");
                draw_row(ui, "🔰", stats.hull_damage.fmt(), "Damage dealt to hulls.");
                if side == Side::Attacker {
                    draw_row(
                        ui,
                        "🌐",
                        stats.ps_damage.fmt(),
                        "Damage dealt to the planetary shield.",
                    );
                }
                draw_row(
                    ui,
                    "⚔",
                    (stats.shield_damage + stats.hull_damage + stats.ps_damage).fmt(),
                    "Total damage dealt.",
                );
                if side == Side::Defender {
                    draw_row(
                        ui,
                        "❤",
                        stats.total_repaired.to_string(),
                        "Total hull points repaired by Repair Trucks.",
                    );
                }
                draw_row(
                    ui,
                    "❌",
                    format!(
                        "{:.0}%",
                        (stats.shots_missed as f32).safe_div(stats.unit_shots as f32) * 100.
                    ),
                    "Percentage of shots that missed a target. A shot misses when it \
                    fires on a unit that was already destroyed that round.",
                );
                draw_row(
                    ui,
                    "🔥",
                    format!(
                        "{:.0}%",
                        (stats.rapid_fire as f32).safe_div(stats.unit_shots as f32) * 100.
                    ),
                    "Percentage of shots that gained rapid fire.",
                );
                if report.mission.objective == Icon::MissileStrike && side == Side::Defender {
                    draw_row(
                        ui,
                        "🚀",
                        format!(
                            "{:.0}%",
                            (stats.missiles_hit as f32).safe_div(stats.missile_shots as f32) * 100.
                        ),
                        "Percentage of Antiballistic Missiles that intercepted an \
                        incoming Interplanetary Missile.",
                    );
                }
                if report.mission.bombing != BombingRaid::None && side == Side::Attacker {
                    draw_row(
                        ui,
                        "💣",
                        format!(
                            "{:.0}%",
                            (stats.bombs_hit as f32).safe_div(stats.building_shots as f32) * 100.
                        ),
                        "Percentage of bombs that hit enemy buildings.",
                    );
                }
                draw_row(ui, "💀", stats.enemies_killed.fmt(), "Number of enemy units destroyed.");
                if report.mission.objective == Icon::Destroy && side == Side::Attacker {
                    draw_row(
                        ui,
                        "☠",
                        format!("{:.0}%", round.destroy_probability * 100.),
                        "Probability of successfully destroying the planet.",
                    );
                }
            },
        );
    };

    let mut any_hovered = false;

    let (attacker_w, defender_w) = (ui.available_width() * 0.3, ui.available_width() * 0.6);

    let attackers = combat_side_participants(report, &Side::Attacker, session);
    let defenders = combat_side_participants(report, &Side::Defender, session);

    ui.horizontal(|ui| {
        ui.add_space(40.);

        ui.vertical(|ui| {
            ui.set_width(attacker_w);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                draw_colored_combat_heading(ui, "Attacker", &attackers);
            });
            draw_combat_strength_bar(ui, &attackers);
        });
        ui.vertical(|ui| {
            ui.set_width(defender_w);
            draw_colored_combat_heading(
                ui,
                if report.is_space_fauna_encounter() {
                    "Space Fauna"
                } else {
                    "Defender"
                },
                &defenders,
            );
            draw_combat_strength_bar(ui, &defenders);
        });
    });

    ui.horizontal(|ui| {
        ui.add_space(40.);

        ui.vertical(|ui| {
            ui.set_width(attacker_w);

            ui.add_space(10.);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(135.);

                    draw_stats(ui, Side::Attacker, &state.combat_report_hover);
                });

                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 12.);

                    let hovered = draw_combat_army_grid(
                        ui,
                        "combat_attacker",
                        state,
                        &round,
                        if report.mission.objective == Icon::MissileStrike {
                            vec![Unit::interplanetary_missile()]
                        } else {
                            Unit::ships()
                        },
                        Side::Attacker,
                        report.planet.shield_overload.is_overloaded(),
                        report.planet.operations.space_dock,
                        images,
                    );
                    any_hovered = any_hovered || hovered;
                });

                ui.set_height(470.);
                ui.separator();
            });
        });

        ui.vertical(|ui| {
            ui.set_width(defender_w);

            ui.add_space(10.);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(520.);

                    if round.units(&Side::Defender).next().is_none() {
                        ui.label("No defending units.");
                    } else {
                        ui.horizontal_top(|ui| {
                            let hovered1 = if report.mission.objective != Icon::MissileStrike {
                                if round.units(&Side::Defender).any(|cu| cu.unit.is_ship()) {
                                    draw_combat_army_grid(
                                        ui,
                                        "combat_defender1",
                                        state,
                                        &round,
                                        if report.is_space_fauna_encounter() {
                                            combat_report_fauna_units(report)
                                        } else {
                                            Unit::ships()
                                        },
                                        Side::Defender,
                                        report.planet.shield_overload.is_overloaded(),
                                        report.planet.operations.space_dock,
                                        images,
                                    )
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            let defenses: Vec<Unit> = round
                                .units(&Side::Defender)
                                .filter_map(|cu| {
                                    (cu.unit.is_defense()
                                        && (report.mission.objective != Icon::MissileStrike
                                            || cu.unit != Unit::space_dock()))
                                    .then_some(cu.unit)
                                })
                                .collect();

                            let hovered2 = if !defenses.is_empty() {
                                draw_combat_army_grid(
                                    ui,
                                    "combat_defender2",
                                    state,
                                    &round,
                                    Unit::defenses()
                                        .into_iter()
                                        .filter(|u| defenses.contains(u))
                                        .collect(),
                                    Side::Defender,
                                    report.planet.shield_overload.is_overloaded(),
                                    report.planet.operations.space_dock,
                                    images,
                                )
                            } else {
                                false
                            };

                            any_hovered = any_hovered || hovered1 || hovered2;

                            let structures = combat_defender_structure_column(report, &round);
                            if !structures.is_empty() {
                                draw_combat_army_grid(
                                    ui,
                                    "combat_defender_structures",
                                    state,
                                    &round,
                                    structures,
                                    Side::Defender,
                                    report.planet.shield_overload.is_overloaded(),
                                    report.planet.operations.space_dock,
                                    images,
                                );
                            }

                            let units = match report.mission.bombing {
                                BombingRaid::Economic
                                    if report
                                        .planet
                                        .army
                                        .iter()
                                        .any(|(u, c)| u.is_economic_building() && *c > 0) =>
                                {
                                    Unit::resource_buildings()
                                },
                                BombingRaid::Industrial
                                    if report
                                        .planet
                                        .army
                                        .iter()
                                        .any(|(u, c)| u.is_industrial_building() && *c > 0) =>
                                {
                                    Unit::industrial_buildings()
                                },
                                _ => vec![],
                            };

                            if !units.is_empty() {
                                draw_combat_army_grid(
                                    ui,
                                    "combat_buildings2",
                                    state,
                                    &round,
                                    units,
                                    Side::Defender,
                                    report.planet.shield_overload.is_overloaded(),
                                    report.planet.operations.space_dock,
                                    images,
                                );
                            }
                        });
                    }
                });

                ui.horizontal_top(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.vertical(|ui| {
                            draw_stats(ui, Side::Defender, &state.combat_report_hover);
                        });
                    });
                });
            });
        });
    });

    if !any_hovered {
        state.combat_report_hover = None;
    }

    ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
        ui.add_space(COMBAT_DETAILS_FOOTER_BOTTOM_GAP);
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 50.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                // Match the defender grid's first unit rather than the divider itself.
                ui.add_space(60. + attacker_w);
                if combat_report_shows_salvage(
                    state.combat_report_total,
                    state.combat_report_round,
                    combat.rounds.len(),
                ) {
                    draw_crawler_salvage(ui, report, player, images);
                }

                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_space(40.);
                    if ui.add_custom_button("Close details", images).clicked() {
                        state.combat_report = None;
                    }
                });
            },
        );
    });
}

/// Draws the mission info hover interface and emits any resulting local actions.
fn draw_mission_info_hover(
    ui: &mut Ui,
    mission: &Mission,
    settings: &Settings,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    images: &ImageIds,
) {
    let origin = map.get(mission.origin);
    let destination = map.get(mission.destination);

    ui.add_space(40.);

    ui.spacing_mut().item_spacing.y = 10.;

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.small("Origin:");

        ui.spacing_mut().item_spacing.x = 4.;
        ui.add_image(images.get(origin.image()), [25.; 2]);
        ui.small(origin.name.to_name());
    });

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.small("Destination:");

        ui.spacing_mut().item_spacing.x = 4.;
        ui.add_image(images.get(destination.image()), [25.; 2]);
        ui.small(destination.name.to_name());
    });

    ui.add(Separator::default().shrink(20.));

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.small("🎯 Objective:");

        ui.spacing_mut().item_spacing.x = 4.;
        let objective = mission.displayed_objective(player.id);
        ui.add_image(images.get(objective.asset_key()), [20.; 2]);
        ui.small(mission_objective_label(objective));
    });

    ui.add(Separator::default().shrink(20.));

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.vertical(|ui| {
            ui.small(format!("📏 Distance: {:.1} AU", mission.distance(map)));

            ui.small(format!("🚀 Movement: {:.2} AU", mission.next_turn_movement(map)))
                .on_hover_small(mission_movement_tooltip(mission.jump_gate));

            let duration = mission.duration(map);
            ui.small(format!(
                "⏱ Duration: +{} turn{} ({})",
                duration,
                if duration == 1 {
                    ""
                } else {
                    "s"
                },
                mission_arrival_turn(settings.turn, duration)
            ))
            .on_hover_small(mission_arrival_tooltip(settings.turn, duration));
        });
    });

    let participants = mission_hover_participants(mission, player.id).collect::<Vec<_>>();
    if !participants.is_empty() {
        ui.add(Separator::default().shrink(20.));

        ui.horizontal(|ui| {
            ui.add_space(25.);
            ui.spacing_mut().item_spacing.x = 7.;
            ui.add_image(images.get(Icon::AlliedAttack.asset_key()), [20.; 2]);
            ui.small("Allied Attack");
        });

        let participant_font = egui::FontId::proportional(MISSION_HOVER_ALLIED_FONT_SIZE);
        let name_column_width = participants
            .iter()
            .map(|(participant, _)| {
                ui.painter()
                    .layout_no_wrap(
                        mission_participant_name(session, *participant),
                        participant_font.clone(),
                        Color32::WHITE,
                    )
                    .size()
                    .x
            })
            .fold(0.0_f32, f32::max);
        let value_column_width = participants
            .iter()
            .map(|(_, army)| {
                ui.painter()
                    .layout_no_wrap(
                        format_thousands(army.total_production()),
                        participant_font.clone(),
                        Color32::WHITE,
                    )
                    .size()
                    .x
            })
            .fold(0.0_f32, f32::max);
        ui.spacing_mut().item_spacing.y = 4.;
        for (participant, army) in participants {
            draw_mission_participant_row(
                ui,
                participant,
                army,
                name_column_width,
                value_column_width,
                session,
                images,
            );
        }
        ui.spacing_mut().item_spacing.y = 10.;
    }
}

/// Returns every dispatched contingent to a participant, with the hovered fleet's owner first.
fn mission_hover_participants<'a>(
    mission: &'a Mission,
    viewer: PlayerId,
) -> impl Iterator<Item = (PlayerId, &'a Army)> + 'a {
    let visible = mission.is_joint_attack_participant(viewer);
    mission.joint_attack.iter().filter(move |_| visible).flat_map(move |attack| {
        std::iter::once((
            mission.owner,
            attack.attackers.get(&mission.owner).unwrap_or(&mission.army),
        ))
        .chain(
            attack
                .attackers
                .iter()
                .filter(move |(participant, _)| **participant != mission.owner)
                .map(|(participant, army)| (*participant, army)),
        )
    })
}

/// Keeps participant names, fleet icons, and strength values in stable compact columns.
fn draw_mission_participant_row(
    ui: &mut Ui,
    participant: PlayerId,
    army: &Army,
    name_column_width: f32,
    value_column_width: f32,
    session: &MultiplayerSession,
    images: &ImageIds,
) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(MISSION_HOVER_ALLIED_ROW_INSET);

        let name = mission_participant_name(session, participant);
        let (name_rect, name_response) = ui.allocate_exact_size(
            egui::vec2(name_column_width, MISSION_HOVER_ALLIED_ICON_SIZE),
            Sense::hover(),
        );
        let name_color = session.player_color(participant).color().to_color32();
        let name_galley = ui.painter().layout_no_wrap(
            name.clone(),
            egui::FontId::proportional(MISSION_HOVER_ALLIED_FONT_SIZE),
            name_color,
        );
        let name_pos =
            egui::pos2(name_rect.left(), name_rect.center().y - name_galley.size().y * 0.5);
        ui.painter().galley(name_pos, name_galley, name_color);
        name_response.on_hover_text(&name);

        ui.add_space(MISSION_HOVER_ALLIED_NAME_ICON_GAP);
        let (icon_rect, icon) = ui
            .allocate_exact_size(egui::Vec2::splat(MISSION_HOVER_ALLIED_ICON_SIZE), Sense::hover());
        ui.painter().image(
            images.get("fleet"),
            icon_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        ui.add_space(MISSION_HOVER_ALLIED_ICON_VALUE_GAP);
        let (strength_rect, strength) = ui.allocate_exact_size(
            egui::vec2(value_column_width, MISSION_HOVER_ALLIED_ICON_SIZE),
            Sense::hover(),
        );
        let strength_galley = ui.painter().layout_no_wrap(
            format_thousands(army.total_production()),
            egui::FontId::proportional(MISSION_HOVER_ALLIED_FONT_SIZE),
            ui.visuals().text_color(),
        );
        let strength_pos = egui::pos2(
            strength_rect.left(),
            strength_rect.center().y - strength_galley.size().y * 0.5,
        );
        ui.painter().galley(strength_pos, strength_galley, ui.visuals().text_color());
        icon.union(strength).on_hover_small(format!(
            "{name}'s fleet strength: total production points sent with this allied attack."
        ));
    });
}

fn mission_participant_name(session: &MultiplayerSession, participant: PlayerId) -> String {
    session
        .player_name(participant)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Player {participant}"))
}

fn mission_info_hover_height(mission: &Mission, viewer: PlayerId) -> f32 {
    let allied_rows = mission_hover_participants(mission, viewer).count() as f32;
    MISSION_HOVER_INFO_BASE_HEIGHT
        + if allied_rows > 0.0 {
            MISSION_HOVER_ALLIED_HEADER_HEIGHT
        } else {
            0.0
        }
        + allied_rows * MISSION_HOVER_ALLIED_ROW_HEIGHT
}

/// Names a concealed mission without implying that its hidden objective is necessarily combat.
fn mission_objective_label(objective: Icon) -> String {
    if objective == Icon::EnemyFleet {
        "Unknown".to_owned()
    } else {
        objective.to_name()
    }
}

/// Uses the pre-mission world artwork so selection cannot reveal the resolved outcome.
fn combat_selection_planet_image(report: &MissionReport) -> String {
    report
        .planet
        .army
        .combined()
        .iter()
        .find_map(|(unit, count)| match unit {
            Unit::Fauna(fauna) if *count > 0 => Some(fauna.to_lowername()),
            _ => None,
        })
        .unwrap_or_else(|| report.planet.image())
}

fn combat_selection_title(report: &MissionReport, map: &Map) -> String {
    if report.is_space_fauna_encounter() {
        "Deep Space Encounter".to_owned()
    } else {
        format!("Battle of {}", map.get(report.mission.destination).name)
    }
}

const COMBAT_SELECTION_MIN_WIDTH: f32 = 380.0;
const COMBAT_SELECTION_PLANET_SIZE: f32 = 42.0;
const COMBAT_SELECTION_PLANET_LEFT_INSET: f32 = 17.0;
const COMBAT_SELECTION_TEXT_PLANET_GAP: f32 = 20.0;
const COMBAT_SELECTION_TEXT_RIGHT_INSET: f32 = 12.0;
const COMBAT_SELECTION_ROW_TEXT_RESERVE: f32 = COMBAT_SELECTION_PLANET_LEFT_INSET
    + COMBAT_SELECTION_PLANET_SIZE
    + COMBAT_SELECTION_TEXT_PLANET_GAP
    + COMBAT_SELECTION_TEXT_RIGHT_INSET;
// Include the scroll area's horizontal inset when sizing the enclosing panel.
const COMBAT_SELECTION_TEXT_RESERVE: f32 = COMBAT_SELECTION_ROW_TEXT_RESERVE + 24.0;

fn combat_selection_row_content_rects(rect: egui::Rect) -> (egui::Rect, egui::Rect) {
    let planet_rect = egui::Rect::from_center_size(
        egui::pos2(
            rect.left() + COMBAT_SELECTION_PLANET_LEFT_INSET + COMBAT_SELECTION_PLANET_SIZE * 0.5,
            rect.center().y,
        ),
        egui::Vec2::splat(COMBAT_SELECTION_PLANET_SIZE),
    );
    let text_rect = egui::Rect::from_min_max(
        egui::pos2(planet_rect.right() + COMBAT_SELECTION_TEXT_PLANET_GAP, rect.top() + 8.0),
        egui::pos2(rect.right() - COMBAT_SELECTION_TEXT_RIGHT_INSET, rect.bottom() - 8.0),
    );
    (planet_rect, text_rect)
}

fn combat_selection_reports(player: &Player, turn: usize) -> Vec<&MissionReport> {
    player
        .reports
        .iter()
        .filter(|report| {
            report.turn == turn
                && !report.hidden
                && report.has_combat_playback()
                && report.can_see(&Side::Defender, player.id)
        })
        .collect()
}

/// Keeps every participant's own color while separators remain readable in white.
fn combat_selection_matchup(
    report: &MissionReport,
    session: &MultiplayerSession,
) -> egui::text::LayoutJob {
    let mut job = egui::text::LayoutJob::default();
    let font_id = egui::FontId::new(14.0, FontFamily::Proportional);
    let mut append = |text: &str, color| {
        job.append(
            text,
            0.0,
            egui::text::TextFormat {
                font_id: font_id.clone(),
                color,
                ..Default::default()
            },
        );
    };
    for (index, id) in report.attacker_players().into_iter().enumerate() {
        if index > 0 {
            append(" + ", Color32::WHITE);
        }
        let name =
            session.player_name(id).map(str::to_owned).unwrap_or_else(|| format!("Player {id}"));
        append(&name, session.player_color(id).color().to_color32());
    }
    append(" vs ", Color32::WHITE);
    let defenders = report.defender_players();
    if defenders.is_empty() {
        let neutral_name = report
            .independent_population_name()
            .or_else(|| report.space_fauna_name().map(str::to_owned))
            .unwrap_or_else(|| "Neutral".to_owned());
        append(
            &neutral_name,
            if report.is_space_fauna_encounter() || report.is_independent_population_encounter() {
                NEUTRAL_COMBAT_COLOR
            } else {
                Color32::WHITE
            },
        );
    }
    for (index, id) in defenders.into_iter().enumerate() {
        if index > 0 {
            append(" + ", Color32::WHITE);
        }
        let name =
            session.player_name(id).map(str::to_owned).unwrap_or_else(|| format!("Player {id}"));
        append(&name, session.player_color(id).color().to_color32());
    }
    job
}

fn combat_selection_panel_width(
    context: &egui::Context,
    reports: &[&MissionReport],
    map: &Map,
    session: &MultiplayerSession,
    viewport_width: f32,
) -> f32 {
    let title_font = TextStyle::Body.resolve(&context.style_of(context.theme()));
    let text_width = reports.iter().fold(0.0_f32, |width, report| {
        let title = combat_selection_title(report, map);
        let title_width = context.fonts_mut(|fonts| {
            fonts.layout_no_wrap(title, title_font.clone(), Color32::WHITE).size().x
        });
        let matchup_width = context.fonts_mut(|fonts| {
            fonts.layout_job(combat_selection_matchup(report, session)).size().x
        });
        width.max(title_width).max(matchup_width)
    });
    (text_width + COMBAT_SELECTION_TEXT_RESERVE)
        .max(COMBAT_SELECTION_MIN_WIDTH)
        .min((viewport_width - 16.0).max(1.0))
}

/// Draws the combat selection interface and emits any resulting local actions.
fn draw_combat_selection(
    ui: &mut Ui,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    settings: &mut Settings,
    next_game_state: &mut NextState<GameState>,
    images: &ImageIds,
) {
    let reports = combat_selection_reports(player, settings.turn);

    ui.add_space(5.);

    ui.vertical_centered(|ui| {
        ui.label("Select a battle");
    });

    ui.vertical_centered(|ui| {
        ui.add_space(10.);

        ScrollArea::vertical().id_salt("combat selection").show(ui, |ui| {
            ui.set_width((ui.available_width() - 24.).max(0.));

            ui.spacing_mut().item_spacing.y = 8.;

            for report in reports.iter().rev() {
                let text_width =
                    (ui.available_width() - COMBAT_SELECTION_ROW_TEXT_RESERVE).max(1.0);
                let mut title_job = egui::text::LayoutJob::simple_singleline(
                    combat_selection_title(report, map),
                    TextStyle::Body.resolve(ui.style()),
                    Color32::WHITE,
                );
                title_job.wrap.max_width = text_width;
                title_job.wrap.break_anywhere = true;
                let title = ui.painter().layout_job(title_job);
                let mut matchup_job = combat_selection_matchup(report, session);
                matchup_job.wrap.max_width = text_width;
                matchup_job.wrap.break_anywhere = true;
                let matchup = ui.painter().layout_job(matchup_job);
                let text_height = title.size().y + 2.0 + matchup.size().y;
                let row_height = (text_height + 20.0).max(72.0);
                let (rect, response) = ui
                    .allocate_exact_size([ui.available_width(), row_height].into(), Sense::click());
                let response = response.on_hover_cursor(CursorIcon::PointingHand);
                let hovered = response.hovered();
                let pressed = response.is_pointer_button_down_on();
                let attacker_id = report.mission.owner;
                let player_color = session.player_color(attacker_id).color().to_color32();

                let fill = if pressed {
                    Color32::from_rgba_unmultiplied(34, 61, 84, 248)
                } else if hovered {
                    Color32::from_rgba_unmultiplied(24, 42, 58, 246)
                } else {
                    Color32::from_rgba_unmultiplied(13, 22, 32, 238)
                };
                let border = if hovered {
                    Color32::from_rgba_unmultiplied(103, 196, 238, 190)
                } else {
                    Color32::from_rgba_unmultiplied(132, 177, 213, 88)
                };

                ui.painter().rect(
                    rect,
                    egui::CornerRadius::same(7),
                    fill,
                    Stroke::new(1., border),
                    StrokeKind::Inside,
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        rect.min + egui::vec2(1., 12.),
                        egui::pos2(rect.left() + 4., rect.bottom() - 12.),
                    ),
                    2.,
                    player_color,
                );

                let center_y = rect.center().y;
                let (planet_rect, text_rect) = combat_selection_row_content_rects(rect);
                egui::Image::new(SizedTexture::new(
                    images.get(combat_selection_planet_image(report)),
                    planet_rect.size(),
                ))
                .corner_radius(encounter_image_corner_radius(report, planet_rect.width()))
                .paint_at(ui, planet_rect);

                let text_painter = ui.painter().with_clip_rect(text_rect);
                let text_top = center_y - text_height * 0.5;
                text_painter.galley(
                    egui::pos2(text_rect.left(), text_top),
                    title.clone(),
                    Color32::WHITE,
                );
                text_painter.galley(
                    egui::pos2(text_rect.left(), text_top + title.size().y + 2.0),
                    matchup,
                    Color32::WHITE,
                );

                if response.clicked() {
                    state.in_combat = Some(report.id);
                    settings.combat_paused = false;
                    next_game_state.set(GameState::Combat);
                }
            }
        });
    });
}

/// Installs the original Fira/Nord theme once the primary egui context exists.
pub fn set_ui_style(mut contexts: EguiContexts, mut initialized: Local<bool>) {
    if *initialized {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    context.set_global_style(NordDark.custom_style());
    context.add_font(FontInsert::new(
        "firasans",
        FontData::from_static(include_bytes!("../../../assets/fonts/FiraSans-Bold.ttf")),
        vec![InsertFontFamily {
            family: FontFamily::Proportional,
            priority: FontPriority::Highest,
        }],
    ));
    *initialized = true;
}

/// Adds ui images to the current UI or asset registry.
pub fn add_ui_images(
    mut contexts: EguiContexts,
    mut images: ResMut<ImageIds>,
    assets: Res<WorldAssets>,
) {
    for (k, v) in assets.images.iter() {
        let v = assets.ui_images.get(k).unwrap_or(v);
        let id = contexts.add_image(EguiTextureHandle::Strong(v.clone()));
        images.0.insert(k.clone(), id);
    }
}

/// Draws the ui interface and emits any resulting local actions.
pub fn draw_ui(
    mut contexts: EguiContexts,
    (mut send_mission, mut recall_mission, mut recall_protection): (
        MessageWriter<SendMissionMsg>,
        MessageWriter<RecallMissionMsg>,
        MessageWriter<RecallProtectionMsg>,
    ),
    (mut message, mut multiplayer_requests): (
        MessageWriter<MessageMsg>,
        MessageWriter<MultiplayerRequest>,
    ),
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    missions: Res<Missions>,
    mut state: ResMut<UiState>,
    mut settings: ResMut<Settings>,
    mut pending: ResMut<PendingTurnCommands>,
    (session, end_game_presentation): (
        Res<MultiplayerSession>,
        Res<crate::core::turns::EndGamePresentation>,
    ),
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    images: Res<ImageIds>,
    window: Single<&Window>,
    (mut planet_panel_slide, mut planet_panel_hover_hold, mut mission_hover_panel_slide): (
        Local<PlanetPanelSlide>,
        Local<PlanetPanelHoverHold>,
        Local<MissionHoverPanelSlide>,
    ),
) {
    state.world_shortcut_hover = None;
    if player.spectator {
        state.mission = false;
        state.mission_planet_hover = None;
    }
    if end_game_presentation.is_pending() {
        planet_panel_slide.hide();
        planet_panel_hover_hold.clear();
        mission_hover_panel_slide.hide();
        return;
    }
    if game_state.get().is_modal_menu() {
        planet_panel_slide.hide();
        planet_panel_hover_hold.clear();
        mission_hover_panel_slide.hide();
        return;
    }

    if *game_state.get() == GameState::Combat
        && state.combat_view == crate::core::combat::cinematic_ui::CombatView::Cinematic
    {
        planet_panel_slide.hide();
        planet_panel_hover_hold.clear();
        mission_hover_panel_slide.hide();
        return;
    }

    let (width, height) = (window.width(), window.height());
    let panel_scale = game_panel_scale(egui::vec2(width, height));
    let action_energy_demand =
        pending_railgun_energy_demand(&map, player.id, &pending).saturating_add(
            crate::core::energy::jump_gate_energy_demand(&missions.0, player.id, settings.turn),
        );

    if *game_state.get() == GameState::Playing {
        if let Ok(context) = contexts.ctx_mut() {
            draw_joint_attack_notifications(
                context,
                &mut state,
                &map,
                &player,
                &session,
                &mut multiplayer_requests,
                &mut message,
                &images,
            );
            draw_trade_notifications(
                context,
                &mut state,
                &map,
                &player,
                &session,
                &mut pending,
                &mut multiplayer_requests,
                &mut message,
                &images,
            );
            draw_players_widget_with_controls(
                context,
                &session,
                &player,
                &map,
                &missions.0,
                Some(&mut multiplayer_requests),
            );
            draw_owned_worlds_widget(
                context,
                &map,
                &player,
                &session,
                &mut state,
                &mut settings,
                &images,
            );
            draw_resources_widget_with_trade(
                context,
                &settings,
                &map,
                &player,
                &images,
                action_energy_demand,
                session.active_game.as_ref().map_or_else(Resources::default, |game| {
                    game.persisted.state.trade_incoming(player.id)
                }),
                session.active_game.as_ref().map_or_else(Resources::default, |game| {
                    game.persisted.state.resource_hub_repayment_due(player.id)
                }),
            );
        }
    }

    if !state.mission {
        state.mission_planet_hover = None;
    }

    let cursor_position = window.cursor_position();
    let delta_seconds =
        contexts.ctx_mut().map_or(0.0, |context| context.input(|input| input.stable_dt));
    // Selection keeps the shop and mission origin without pinning the details open. A map-hover
    // panel remains briefly after PointerOut so the pointer can reach it, then stays visible for as
    // long as any of its constituent panels are hovered.
    let direct_planet_panel =
        planet_hover_panel_target(&state, cursor_position.map(|pos| pos.x), width);
    let planet_panel = if state.mission_hover.is_some() {
        // Mission hover owns the edge-card layer. Do not let the planet transfer grace period keep
        // the old card open behind it; the planet slide reverses on this same frame instead.
        planet_panel_hover_hold.clear();
        planet_panel_slide.skip_detail_exit();
        None
    } else {
        planet_panel_hover_hold.update(
            direct_planet_panel,
            cursor_position.map(|pos| egui::pos2(pos.x, pos.y)),
            delta_seconds,
        )
    };
    if direct_planet_panel.is_none() && planet_panel.is_some() {
        if let Ok(context) = contexts.ctx_mut() {
            context.request_repaint();
        }
    }

    let planet_panel = planet_panel_slide.update(planet_panel, delta_seconds);

    if let Some((target, slide_progress)) = planet_panel {
        let PlanetPanelSlideTarget {
            id,
            mode,
            right_side,
        } = target;

        let planet = map.get(id);
        let live_garrison_visible = player.controls(planet)
            || planet.army.protector(player.id).is_some()
            || player.spectator;
        let (window_w, window_h) = if planet.is_moon() {
            (MOON_UNITS_PANEL_WIDTH, WORLD_UNITS_PANEL_BASE_HEIGHT)
        } else {
            (PLANET_UNITS_PANEL_WIDTH, WORLD_UNITS_PANEL_BASE_HEIGHT)
        };
        let panel_scale = panel_scale.min(((height - 16.0) / window_h).max(0.001));

        let slide_distance = (window_w + 518.0) * panel_scale;
        let slide_x = planet_panel_slide_offset(slide_progress, right_side, slide_distance);
        let detail_line_progress =
            std::array::from_fn(|line| planet_panel_slide.detail_progress(line));
        if planet_panel_slide.is_animating() {
            if let Ok(context) = contexts.ctx_mut() {
                context.request_repaint();
            }
        }

        let mut draw_planet_info = |contexts, id, map, player, state, extension| {
            let (window_w2, window_h2) = (518., 216.);

            draw_scaled_sliding_panel(
                contexts,
                "planet overview",
                "panel",
                (
                    if right_side {
                        width * 0.998
                            - window_w2 * panel_scale
                            - if extension {
                                window_w * panel_scale
                            } else {
                                0.
                            }
                    } else {
                        width * 0.002
                            + if extension {
                                window_w * panel_scale
                            } else {
                                0.
                            }
                    } + slide_x,
                    height * 0.5 - window_h * panel_scale * 0.5 + 27. * panel_scale,
                ),
                (window_w2, window_h2),
                slide_distance,
                panel_scale,
                &images,
                |ui| {
                    draw_planet_overview(
                        ui,
                        id,
                        map,
                        player,
                        &settings,
                        &mut pending,
                        &mut recall_protection,
                        &session,
                        state,
                        &detail_line_progress,
                        right_side,
                        &images,
                    )
                },
            )
        };

        let mut panel_rects = [None; 2];

        // Check whether there is a report on this planet
        let info = player.last_info(planet, &missions.0);

        if live_garrison_visible {
            include_planet_panel_rect(
                &mut panel_rects,
                draw_scaled_sliding_panel(
                    &mut contexts,
                    "overview",
                    "panel",
                    (
                        if right_side {
                            width * 0.998 - window_w * panel_scale
                        } else {
                            width * 0.002
                        } + slide_x,
                        height * 0.5 - window_h * panel_scale * 0.5,
                    ),
                    (window_w, window_h),
                    slide_distance,
                    panel_scale,
                    &images,
                    |ui| {
                        draw_overview(ui, planet, player.home_planet, &session, &mut state, &images)
                    },
                ),
            );

            if mode == PlanetPanelMode::Full {
                include_planet_panel_rect(
                    &mut panel_rects,
                    draw_planet_info(&mut contexts, id, &mut map, &mut player, &mut state, true),
                );
            }
        } else if let Some(info) = info {
            // Don't use has_army since no units is also valid information
            if !planet.is_destroyed && !info.army.is_empty() {
                include_planet_panel_rect(
                    &mut panel_rects,
                    draw_scaled_sliding_panel(
                        &mut contexts,
                        "report overview",
                        "panel",
                        (
                            if right_side {
                                width * 0.998 - window_w * panel_scale
                            } else {
                                width * 0.002
                            } + slide_x,
                            height * 0.5 - window_h * panel_scale * 0.5,
                        ),
                        (window_w, window_h),
                        slide_distance,
                        panel_scale,
                        &images,
                        |ui| draw_report_overview(ui, planet, &info, player.home_planet, &images),
                    ),
                );

                if mode == PlanetPanelMode::Full {
                    include_planet_panel_rect(
                        &mut panel_rects,
                        draw_planet_info(
                            &mut contexts,
                            id,
                            &mut map,
                            &mut player,
                            &mut state,
                            true,
                        ),
                    );
                }
            } else if !planet.is_destroyed && mode == PlanetPanelMode::Full {
                include_planet_panel_rect(
                    &mut panel_rects,
                    draw_planet_info(&mut contexts, id, &mut map, &mut player, &mut state, false),
                );
            }
        } else if !planet.is_destroyed && mode == PlanetPanelMode::Full {
            include_planet_panel_rect(
                &mut panel_rects,
                draw_planet_info(&mut contexts, id, &mut map, &mut player, &mut state, false),
            );
        }
        planet_panel_hover_hold.set_panel_rects(panel_rects);
    } else {
        planet_panel_hover_hold.set_panel_rects([None; 2]);
    }

    let mission_hover_target = state.mission_hover.map(|id| MissionHoverPanelSlideTarget {
        id,
        from_ui: state.mission_hover_from_ui,
        right_side: cursor_position.is_none_or(|pos| pos.x < width * 0.5),
    });
    let mission_hover_panel = mission_hover_panel_slide.update(mission_hover_target, delta_seconds);
    if mission_hover_panel_slide.is_animating() {
        if let Ok(context) = contexts.ctx_mut() {
            context.request_repaint();
        }
    }

    if let Some((target, slide_progress)) = mission_hover_panel {
        let Some(mission) = missions.get(target.id) else {
            state.mission_hover = None;
            mission_hover_panel_slide.hide();
            return;
        };

        let (fleet_x, info_x) =
            mission_hover_panel_x_positions_on_side(target.right_side, width / panel_scale);
        let (fleet_x, info_x) = (fleet_x * panel_scale, info_x * panel_scale);
        let window_h = 630.0;
        let slide_distance = (MISSION_HOVER_FLEET_WIDTH
            + if target.from_ui {
                0.0
            } else {
                MISSION_HOVER_PANEL_GAP + MISSION_HOVER_INFO_WIDTH
            })
            * panel_scale;
        let slide_x = planet_panel_slide_offset(slide_progress, target.right_side, slide_distance);

        draw_scaled_sliding_panel(
            &mut contexts,
            "mission hover fleet",
            "panel",
            (fleet_x + slide_x, height * 0.5 - window_h * panel_scale * 0.5),
            (MISSION_HOVER_FLEET_WIDTH, window_h),
            slide_distance,
            panel_scale,
            &images,
            |ui| draw_mission_fleet_hover(ui, mission, &map, &player, &images),
        );

        if mission_hover_shows_info_panel(target.from_ui) {
            // Objective names such as "Missile Strike" must fit beside their icon and label.
            let window_h2 = mission_info_hover_height(mission, player.id);

            draw_scaled_sliding_panel(
                &mut contexts,
                "mission hover info",
                "panel",
                (
                    info_x + slide_x,
                    height * 0.5 - window_h * panel_scale * 0.5 + 27.0 * panel_scale,
                ),
                (MISSION_HOVER_INFO_WIDTH, window_h2),
                slide_distance,
                panel_scale,
                &images,
                |ui| {
                    draw_mission_info_hover(
                        ui, mission, &settings, &map, &player, &session, &images,
                    )
                },
            );
        }
    }

    // Keep the previous hover for drawing, but require the mission list to renew it.
    let mission_hover_from_ui = std::mem::take(&mut state.mission_hover_from_ui);

    if !player.spectator && mission_panel_visible(&state) {
        let mission_scale = missions::mission_panel_scale(egui::vec2(width, height));
        let size = missions::mission_panel_size(egui::vec2(width, height) / mission_scale);
        let (window_w, window_h) = (size.x, size.y);

        let is_hovered = contexts.ctx_mut().is_ok_and(|ctx| ctx.is_pointer_over_egui());
        draw_scaled_panel(
            &mut contexts,
            "mission",
            "panel",
            ((width - window_w * mission_scale) * 0.5, (height - window_h * mission_scale) * 0.5),
            (window_w, window_h),
            mission_scale,
            &images,
            |ui| {
                draw_mission(
                    ui,
                    &missions.0,
                    &mut send_mission,
                    &mut recall_mission,
                    &settings,
                    &mut state,
                    &mut map,
                    &mut player,
                    &session,
                    &mut multiplayer_requests,
                    is_hovered,
                    &keyboard,
                    &images,
                    pending.can_accept_commands(),
                )
            },
        );
    } else if let Some(id) = state.planet_selected {
        if settings.show_menu && !player.spectator {
            // Hide shop if hovering another planet
            if state.hovered_planet().is_none_or(|planet_id| planet_id == id) {
                let solar_band = map.solar_band(id);
                let next_turn_energy = projected_energy(&map, &player, action_energy_demand);
                let senate_level_limit = Player::senate_level_limit(&map, settings.p_colonizable);
                let senate = player.senate_support(&map);
                let senate_queues_fit =
                    crate::core::units::operations::SenatePolicy::ALL.map(|policy| {
                        crate::core::units::operations::SenateSupport {
                            policy,
                            ..senate
                        }
                        .supports_queues(&map)
                    });
                let planet = map.get_mut(id);

                if player.owns(planet) || (planet.is_moon() && player.controls(planet)) {
                    let (window_w, window_h) = (735., 340.);

                    draw_scaled_panel(
                        &mut contexts,
                        "shop",
                        "panel",
                        (
                            width * 0.5 - window_w * panel_scale * 0.5,
                            height * 0.995 - window_h * panel_scale,
                        ),
                        (window_w, window_h),
                        panel_scale,
                        &images,
                        |ui| {
                            draw_shop(
                                ui,
                                &mut state,
                                &settings,
                                &mut player,
                                planet,
                                solar_band,
                                next_turn_energy,
                                senate_level_limit,
                                senate,
                                senate_queues_fit,
                                &mut pending,
                                &mut message,
                                &images,
                            )
                        },
                    );
                }
            }
        }
    }

    if mission_hover_from_ui && !state.mission_hover_from_ui {
        state.mission_hover = None;
    }

    if state.combat_report.is_some() {
        let (window_w, window_h) = (1070., 700.);
        let scale = panel_scale
            .min(((width - 16.0) / window_w).max(0.001))
            .min(((height - 16.0) / window_h).max(0.001));

        draw_scaled_panel(
            &mut contexts,
            "combat report",
            "panel",
            (width * 0.5 - window_w * scale * 0.5, height * 0.9 - window_h * scale),
            (window_w, window_h),
            scale,
            &images,
            |ui| draw_combat_report(ui, &mut state, &map, &player, &session, &images),
        );
    }

    if *game_state.get() == GameState::CombatMenu {
        let reports = combat_selection_reports(&player, settings.turn);
        let window_h = 420.0;
        let scale = panel_scale.min(((height - 16.0) / window_h).max(0.001));
        let window_w = contexts
            .ctx_mut()
            .map(|context| {
                combat_selection_panel_width(context, &reports, &map, &session, width / scale)
            })
            .unwrap_or(COMBAT_SELECTION_MIN_WIDTH);

        draw_scaled_panel(
            &mut contexts,
            "combat list",
            "panel",
            ((width - window_w * scale) * 0.5, (height - window_h * scale) * 0.5),
            (window_w, window_h),
            scale,
            &images,
            |ui| {
                draw_combat_selection(
                    ui,
                    &mut state,
                    &map,
                    &player,
                    &session,
                    &mut settings,
                    &mut next_game_state,
                    &images,
                )
            },
        );
    }

    if let Some(id) = state.protection_access {
        let protection_enabled = session.active_game.as_ref().is_some_and(|game| {
            game.persisted.state.players.iter().filter(|player| !player.spectator).count() >= 3
        });
        let valid = protection_enabled
            && map
                .try_get(id)
                .is_some_and(|planet| !planet.is_destroyed && player.controls(planet));
        if !valid {
            state.protection_access = None;
        } else if let Ok(context) = contexts.ctx_mut() {
            let (close, changes) =
                draw_protection_access_modal(context, &images, map.get(id), &session);
            if close {
                state.protection_access = None;
            }
            for (protector, allowed) in changes {
                let planet = map.get_mut(id);
                if allowed {
                    planet.protection_permissions.insert(protector);
                } else {
                    planet.protection_permissions.remove(&protector);
                }
                multiplayer_requests.write(MultiplayerRequest::SetProtectionPermission {
                    planet_id: id,
                    protector,
                    allowed,
                });
                set_ui_sound(context, Some(SoundEffect::Button));
            }
        }
    }

    if let Some(target) = state.railgun_confirmation {
        let origins = orbital_railgun_origins(&map, player.id, target);
        let already_committed = pending
            .commands
            .iter()
            .chain(&pending.queued_commands)
            .any(|command| matches!(command, TurnCommand::FireOrbitalRailguns { .. }));
        let valid = pending.can_accept_commands()
            && !origins.is_empty()
            && !already_committed
            && map.try_get(target).is_some_and(|planet| {
                !planet.is_destroyed && !planet.blocks_hostile_action_by(player.id)
            });

        if !valid {
            state.railgun_confirmation = None;
        } else {
            let target_name = map.get(target).name.clone();
            let cost = orbital_railgun_fire_cost(origins.len());
            let energy_cost = orbital_railgun_fire_energy_cost(origins.len());
            let has_deuterium = player.resources.deuterium >= cost.deuterium;
            let chance = orbital_railgun_destruction_basis_points(&map, &origins, target);
            if let Ok(context) = contexts.ctx_mut() {
                if let Some(action) = draw_railgun_confirmation(
                    context,
                    &images,
                    &target_name,
                    origins.len(),
                    cost.deuterium,
                    energy_cost,
                    chance,
                    has_deuterium,
                ) {
                    state.railgun_confirmation = None;
                    if action == ConfirmationAction::Confirm {
                        if pending.push(TurnCommand::FireOrbitalRailguns {
                            target,
                        }) {
                            player.resources -= cost;
                            set_ui_sound(context, Some(SoundEffect::Button));
                            message.write(MessageMsg::info(format!(
                                "Railgun shots committed on {target_name}."
                            )));
                        } else {
                            message.write(MessageMsg::error(COMMAND_LIMIT_REACHED_MESSAGE));
                        }
                    }
                }
            }
        }
    }

    if let Some(id) = state.abandon_confirmation {
        let planet = map.get(id);
        let can_abandon = pending.can_accept_commands()
            && !planet.is_moon()
            && player.owns(planet)
            && player.home_planet != id
            && planet.buy.is_empty();

        if !can_abandon {
            state.abandon_confirmation = None;
        } else {
            if let Ok(context) = contexts.ctx_mut() {
                if let Some(action) = draw_abandon_confirmation(context, &images, &planet.name) {
                    state.abandon_confirmation = None;
                    if action == ConfirmationAction::Confirm {
                        abandon_planet(
                            id,
                            &mut map,
                            &mut player,
                            &settings,
                            &mut message,
                            &mut pending,
                        );
                    }
                }
            }
        }
    }

    if let Some(id) = state.colonize_confirmation {
        let (n_owned, n_max_owned) = player.planets_owned(&map, &settings);
        let can_colonize = pending.can_accept_commands()
            && map.try_get(id).is_some_and(|planet| {
                !planet.is_moon()
                    && player.controls(planet)
                    && !player.owns(planet)
                    && planet.army.controller().amount(&Unit::colony_ship()) > 0
            })
            && n_owned < n_max_owned;

        if !can_colonize {
            state.colonize_confirmation = None;
        } else {
            let planet_name = map.get(id).name.clone();
            if let Ok(context) = contexts.ctx_mut() {
                if let Some(action) = draw_colonize_confirmation(context, &images, &planet_name) {
                    state.colonize_confirmation = None;
                    if action == ConfirmationAction::Confirm {
                        colonize_planet(id, &mut map, &player, &mut message, &mut pending);
                    }
                }
            }
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/core/ui_systems.rs"]
mod tests;
