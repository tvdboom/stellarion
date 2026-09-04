//! Egui systems for the strategic HUD, shops, missions, and combat reports.

use std::collections::{HashMap, HashSet};

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
use crate::core::combat::report::{MissionReport, ReportId, RoundReport, Side};
use crate::core::combat::resolution::CombatUnit;
use crate::core::combat::stats::CombatStats;
use crate::core::constants::{
    BG2_COLOR, HEALTH_COLOR, PROBES_PER_PRODUCTION_LEVEL, PS_SHIELD_PER_LEVEL, SHIELD_COLOR,
};
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::systems::select_planet;
use crate::core::messages::MessageMsg;
use crate::core::missions::{BombingRaid, Mission, MissionId, Missions, SendMissionMsg};
use crate::core::orders::{purchase_limit, validate_mission};
use crate::core::player::{PlanetInfo, Player};
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::simulation::TurnCommand;
use crate::core::states::GameState;
use crate::core::ui::aesthetics::Aesthetics;
use crate::core::ui::dark::NordDark;
use crate::core::ui::utils::{toggle, CustomResponse, CustomUi, ImageIds};
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Description, Price, Unit};
use crate::multiplayer::client::{MultiplayerSession, PendingTurnCommands};
use crate::utils::{format_thousands, FmtNumb, NameFromEnum, SafeDiv, ToColor32};

mod missions;
use missions::draw_mission;
mod shop;
use shop::draw_shop;

#[derive(Component)]
/// Marker for entities owned by the in-game UI projection.
pub struct UiCmp;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
/// Selected constructible-unit category in the local shop panel.
pub enum Shop {
    #[default]
    Buildings,
    Fleet,
    Defenses,
}

impl Shop {
    /// Returns the category reached by moving one tab to the right.
    pub(crate) fn next(self, is_moon: bool) -> Self {
        match self {
            Self::Buildings => Self::Fleet,
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
            Self::Fleet => Self::Buildings,
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
    /// Mission-panel world hover, which previews known units without map or planet details.
    pub(crate) mission_planet_hover: Option<PlanetId>,
    pub planet_selected: Option<PlanetId>,
    /// Planet awaiting confirmation before its abandon command is added to the turn draft.
    pub(crate) abandon_confirmation: Option<PlanetId>,
    /// Camera-only world focus used by shortcuts that must not open a world panel.
    pub focus_planet: Option<PlanetId>,
    pub to_selected: bool,
    pub shop: Shop,
    pub lab: (ResourceName, ResourceName),
    pub lab_amount: usize,
    pub mission: bool,
    pub mission_tab: MissionTab,
    pub mission_info: Mission,
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
    pub combat_round: usize,
    pub end_turn: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PlanetPanelMode {
    Full,
    UnitsOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbandonConfirmationAction {
    Confirm,
    Cancel,
}

const ABANDON_CONFIRMATION_TEXT_COLOR: Color32 = Color32::from_rgb(166, 188, 211);
const ABANDON_CONFIRMATION_BUTTON_FILL: Color32 = Color32::from_rgb(18, 28, 39);

fn visible_planet_panel(state: &UiState) -> Option<(PlanetId, PlanetPanelMode)> {
    state.mission_planet_hover.map(|id| (id, PlanetPanelMode::UnitsOnly)).or_else(|| {
        state.planet_hover.or(state.planet_selected).map(|id| (id, PlanetPanelMode::Full))
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
    fn progress(&mut self, target: PlanetPanelSlideTarget, delta_seconds: f32) -> f32 {
        if self.target != Some(target) {
            self.target = Some(target);
            self.elapsed = 0.0;
        } else {
            self.elapsed = (self.elapsed + delta_seconds.max(0.0)).min(PLANET_PANEL_TOTAL_DURATION);
        }

        (self.elapsed / PLANET_PANEL_SLIDE_DURATION).min(1.0)
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

    fn hide(&mut self) {
        self.target = None;
        self.elapsed = 0.0;
    }
}

const PLANET_PANEL_SLIDE_DURATION: f32 = 0.22;
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
const MISSION_HOVER_PANEL_GAP: f32 = 1.0;

const HUD_PANEL_FILL: Color32 = Color32::from_rgba_unmultiplied_const(10, 16, 23, 226);
const HUD_PANEL_STROKE: Color32 = Color32::from_rgba_unmultiplied_const(130, 170, 215, 95);
const HUD_REFERENCE_WIDTH: f32 = 1280.0;
const HUD_REFERENCE_HEIGHT: f32 = 720.0;
const HUD_MIN_SCALE: f32 = 0.8;
const HUD_MAX_SCALE: f32 = 1.6;

/// Scales the strategic HUD with the limiting viewport dimension, within readable bounds.
fn strategic_hud_scale(viewport: egui::Vec2) -> f32 {
    (viewport.x / HUD_REFERENCE_WIDTH)
        .min(viewport.y / HUD_REFERENCE_HEIGHT)
        .clamp(HUD_MIN_SCALE, HUD_MAX_SCALE)
}

/// Keeps the world shortcuts readable on short screens while still growing them on large ones.
fn owned_worlds_hud_scale(viewport: egui::Vec2) -> f32 {
    strategic_hud_scale(viewport).max(1.0)
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
fn mission_hover_panel_x_positions(cursor_x: Option<f32>, viewport_width: f32) -> (f32, f32) {
    let panels_on_right = cursor_x.is_none_or(|x| x < viewport_width * 0.5);
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

/// Draws the panel interface and emits any resulting local actions.
fn draw_panel<R>(
    contexts: &mut EguiContexts,
    name: &str,
    image: &str,
    pos: (f32, f32),
    size: (f32, f32),
    images: &ImageIds,
    content: impl FnOnce(&mut Ui) -> R,
) {
    draw_panel_with_horizontal_overflow(contexts, name, image, pos, size, 0.0, images, content);
}

/// Draws a panel while permitting an animated horizontal entrance beyond the viewport edge.
fn draw_sliding_panel<R>(
    contexts: &mut EguiContexts,
    name: &str,
    image: &str,
    pos: (f32, f32),
    size: (f32, f32),
    horizontal_overflow: f32,
    images: &ImageIds,
    content: impl FnOnce(&mut Ui) -> R,
) {
    draw_panel_with_horizontal_overflow(
        contexts,
        name,
        image,
        pos,
        size,
        horizontal_overflow,
        images,
        content,
    );
}

fn draw_panel_with_horizontal_overflow<R>(
    contexts: &mut EguiContexts,
    name: &str,
    image: &str,
    pos: (f32, f32),
    size: (f32, f32),
    horizontal_overflow: f32,
    images: &ImageIds,
    content: impl FnOnce(&mut Ui) -> R,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let mut window = egui::Window::new(name)
        .frame(egui::Frame {
            fill: Color32::TRANSPARENT,
            ..default()
        })
        .order(if name == "combat report" {
            Order::Foreground
        } else {
            Order::Middle
        })
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_pos(pos)
        .fixed_size(size);

    if horizontal_overflow > 0.0 {
        window = window
            .constrain_to(context.content_rect().expand2(egui::vec2(horizontal_overflow, 0.0)));
    }

    window.show(context, |ui| {
        let response =
            ui.add(egui::Image::new(SizedTexture::new(images.get(image), ui.available_size())));

        ui.scope_builder(UiBuilder::new().max_rect(response.rect), content);
    });
}

/// Draws a centered, input-blocking abandon prompt over the game interface.
fn draw_abandon_confirmation(
    context: &egui::Context,
    images: &ImageIds,
) -> Option<AbandonConfirmationAction> {
    let content_rect = context.content_rect();
    let available = content_rect.size() - egui::vec2(32.0, 32.0);
    let size = egui::vec2(520.0_f32.min(available.x), 230.0_f32.min(available.y));
    let modal_id = egui::Id::new("abandon planet confirmation");
    let panel_offset = content_rect.center() - size * 0.5 - content_rect.min;
    let area = egui::Modal::default_area(modal_id).anchor(Align2::LEFT_TOP, panel_offset);
    let response =
        egui::Modal::new(modal_id).area(area).frame(egui::Frame::NONE).show(context, |ui| {
            let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
            ui.painter().image(
                images.get("panel"),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                Color32::WHITE,
            );

            let mut action = None;
            ui.scope_builder(
                UiBuilder::new().max_rect(rect.shrink2(egui::vec2(34.0, 24.0))),
                |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add_space(42.0);
                        ui.label(
                            RichText::new("Are you sure you want to abandon this planet?")
                                .size(22.0)
                                .strong()
                                .color(ABANDON_CONFIRMATION_TEXT_COLOR),
                        );
                        ui.add_space(24.0);
                        ui.scope(|ui| {
                            let widgets = &mut ui.style_mut().visuals.widgets;
                            for (visuals, fill, stroke) in [
                                (
                                    &mut widgets.inactive,
                                    ABANDON_CONFIRMATION_BUTTON_FILL,
                                    Color32::from_rgb(74, 99, 122),
                                ),
                                (
                                    &mut widgets.hovered,
                                    Color32::from_rgb(31, 47, 61),
                                    Color32::from_rgb(123, 158, 188),
                                ),
                                (
                                    &mut widgets.active,
                                    Color32::from_rgb(39, 94, 123),
                                    Color32::from_rgb(139, 183, 216),
                                ),
                            ] {
                                visuals.bg_fill = fill;
                                visuals.weak_bg_fill = fill;
                                visuals.bg_stroke = Stroke::new(1.0, stroke);
                                visuals.corner_radius = egui::CornerRadius::same(6);
                                visuals.expansion = 0.0;
                            }

                            ui.horizontal(|ui| {
                                const BUTTON_WIDTH: f32 = 96.0;
                                const BUTTON_GAP: f32 = 12.0;
                                let row_width = BUTTON_WIDTH * 2.0 + BUTTON_GAP;
                                ui.spacing_mut().item_spacing.x = BUTTON_GAP;
                                ui.add_space(((ui.available_width() - row_width) * 0.5).max(0.0));
                                let button = |label| {
                                    egui::Button::new(
                                        RichText::new(label)
                                            .size(17.0)
                                            .strong()
                                            .color(ABANDON_CONFIRMATION_TEXT_COLOR),
                                    )
                                };
                                if ui
                                    .add_sized([BUTTON_WIDTH, 36.0], button("Yes"))
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    action = Some(AbandonConfirmationAction::Confirm);
                                }
                                if ui
                                    .add_sized([BUTTON_WIDTH, 36.0], button("No"))
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    action = Some(AbandonConfirmationAction::Cancel);
                                }
                            });
                        });
                    });
                },
            );
            action
        });

    if response.should_close() {
        Some(AbandonConfirmationAction::Cancel)
    } else {
        response.inner
    }
}

/// Selects the stationed fleet silhouette shown beside a world shortcut.
fn world_shortcut_fleet_image(planet: &Planet) -> Option<&'static str> {
    if planet.has(&Unit::war_sun()) {
        Some("mission destroy")
    } else if !planet.has_fleet() {
        None
    } else if planet
        .army
        .iter()
        .all(|(unit, count)| *count == 0 || !unit.is_ship() || *unit == Unit::probe())
    {
        Some("mission spy")
    } else {
        Some("mission")
    }
}

/// Draws one compact, fully clickable world shortcut.
fn draw_world_shortcut(
    ui: &mut Ui,
    planet: &Planet,
    fleet_color: Color32,
    images: &ImageIds,
    scale: f32,
) -> bool {
    let available_width = ui.available_width();
    const FLEET_ICON_GAP: f32 = 6.0;
    const FLEET_ICON_SIZE: f32 = 20.0;
    let fleet_image = world_shortcut_fleet_image(planet);
    let fleet_icon_width = if fleet_image.is_some() {
        (FLEET_ICON_GAP + FLEET_ICON_SIZE) * scale
    } else {
        0.0
    };
    let text_width = (available_width - 46.0 * scale - fleet_icon_width).max(0.0);
    let name = egui::WidgetText::from(
        RichText::new(&planet.name).size(14.0 * scale).strong().color(Color32::WHITE),
    )
    .into_galley(ui, Some(egui::TextWrapMode::Truncate), text_width, TextStyle::Body);

    let (rect, response) = ui.allocate_exact_size(
        egui::vec2(available_width, WORLD_SHORTCUT_HEIGHT * scale),
        Sense::click(),
    );
    let response = response.on_hover_cursor(CursorIcon::PointingHand);

    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            egui::CornerRadius::same((4.0 * scale).round() as u8),
            Color32::from_rgba_unmultiplied(62, 105, 137, 74),
        );
    }

    let icon_rect = egui::Rect::from_center_size(
        egui::pos2(rect.left() + 19.0 * scale, rect.center().y),
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
    ui.painter().galley(
        egui::pos2(text_x, rect.center().y - name_size.y * 0.5),
        name,
        Color32::WHITE,
    );

    if let Some(fleet_image) = fleet_image {
        let fleet_icon_rect = egui::Rect::from_center_size(
            egui::pos2(
                text_x + name_size.x + FLEET_ICON_GAP * scale + FLEET_ICON_SIZE * scale * 0.5,
                rect.center().y,
            ),
            egui::Vec2::splat(FLEET_ICON_SIZE * scale),
        );
        ui.painter().image(
            images.get(fleet_image),
            fleet_icon_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            fleet_color,
        );
    }

    response.clicked()
}

/// Draws a world-group label and its prominent item count using the shared panel heading style.
fn draw_world_group_header(ui: &mut Ui, title: &str, count: usize, scale: f32) {
    let heading_color = Color32::from_rgb(166, 188, 211);
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(11.0 * scale).strong().color(heading_color));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.label(
                RichText::new(count.to_string()).size(16.0 * scale).strong().color(heading_color),
            );
        });
    });
}

const OWNED_WORLDS_LEFT: f32 = 9.0;
const OWNED_WORLDS_TOP: f32 = 112.0;
const OWNED_WORLDS_WIDTH: f32 = 210.0;
const WORLD_SHORTCUT_HEIGHT: f32 = 40.0;
const WORLD_LIST_ITEM_SPACING: f32 = 3.0;

/// Shows the local player's owned and controlled worlds as quick map shortcuts.
fn draw_owned_worlds_widget(
    context: &egui::Context,
    map: &Map,
    player: &Player,
    state: &mut UiState,
    settings: &mut Settings,
    images: &ImageIds,
) -> egui::Rect {
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
    owned.sort_by(|left, right| left.name.cmp(&right.name));
    controlled.sort_by(|left, right| left.name.cmp(&right.name));
    let fleet_color = player.color().color().to_color32();

    egui::Area::new("stellarion_owned_worlds".into())
        .fixed_pos(egui::pos2(OWNED_WORLDS_LEFT * scale, OWNED_WORLDS_TOP * scale))
        .movable(false)
        .constrain(true)
        .order(Order::Middle)
        .show(context, |ui| {
            scaled_hud_panel_frame(scale).show(ui, |ui| {
                let max_width = (context.content_rect().width() - 62.0 * scale).max(0.0);
                ui.set_width((OWNED_WORLDS_WIDTH * scale).min(max_width));
                ui.spacing_mut().item_spacing =
                    egui::vec2(5.0 * scale, WORLD_LIST_ITEM_SPACING * scale);

                if owned.is_empty() && controlled.is_empty() {
                    ui.label(
                        RichText::new("No worlds under your control")
                            .size(11.0 * scale)
                            .color(Color32::from_rgb(145, 156, 168)),
                    );
                }

                if !owned.is_empty() {
                    draw_world_group_header(ui, "OWNED PLANETS", owned.len(), scale);
                    for planet in &owned {
                        if draw_world_shortcut(ui, planet, fleet_color, images, scale) {
                            select_planet(planet, state, player);
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
                        "CONTROLLED PLANETS AND MOONS",
                        controlled.len(),
                        scale,
                    );
                    for planet in &controlled {
                        if draw_world_shortcut(ui, planet, fleet_color, images, scale) {
                            select_planet(planet, state, player);
                            state.planet_hover = None;
                            settings.show_menu = true;
                        }
                    }
                }
            });
        })
        .response
        .rect
}

/// Shows every opposing player's name beside the color used by their map cells.
fn draw_enemy_players_widget(
    context: &egui::Context,
    session: &MultiplayerSession,
    local_player: &Player,
) {
    let Some(game) = &session.active_game else {
        return;
    };
    let enemies = game
        .members
        .iter()
        .filter(|member| member.player_id != local_player.id)
        .collect::<Vec<_>>();
    if enemies.is_empty() {
        return;
    }

    egui::Area::new("stellarion_enemy_players".into())
        .anchor(Align2::LEFT_BOTTOM, egui::vec2(18.0, -18.0))
        .movable(false)
        .constrain(true)
        .order(Order::Middle)
        .show(context, |ui| {
            hud_panel_frame()
                .fill(Color32::from_rgba_unmultiplied(10, 16, 23, 218))
                .inner_margin(egui::Margin::symmetric(12, 9))
                .show(ui, |ui| {
                    let max_width = (context.content_rect().width() - 62.0).max(0.0);
                    ui.set_width(220.0_f32.min(max_width));
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                    ui.label(
                        RichText::new("ENEMY PLAYERS")
                            .size(11.0)
                            .strong()
                            .color(Color32::from_rgb(166, 188, 211)),
                    );
                    ui.add_space(5.0);
                    for member in enemies {
                        ui.horizontal(|ui| {
                            let color = session.player_color(member.player_id);
                            let [red, green, blue] = color.rgb();
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(18.0, 22.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                rect.center(),
                                6.0,
                                Color32::from_rgba_unmultiplied(
                                    red,
                                    green,
                                    blue,
                                    if member.connected {
                                        255
                                    } else {
                                        110
                                    },
                                ),
                            );
                            let status = (!member.connected).then(|| {
                                egui::WidgetText::from(
                                    RichText::new("DISCONNECTED")
                                        .size(9.0)
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
                            // Reserve the status width before truncating a long player name.
                            let status_width = status.as_ref().map_or(0.0, |galley| {
                                galley.size().x + 12.0 + 2.0 * ui.spacing().item_spacing.x
                            });
                            let name = egui::WidgetText::from(
                                RichText::new(&member.display_name).size(14.0).strong().color(
                                    if member.connected {
                                        ui.visuals().text_color()
                                    } else {
                                        Color32::from_rgb(174, 181, 190)
                                    },
                                ),
                            )
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                (ui.available_width() - status_width).max(0.0),
                                TextStyle::Body,
                            );
                            ui.add(egui::Label::new(name));
                            if let Some(status) = status {
                                draw_disconnected_icon(ui);
                                ui.add(egui::Label::new(status));
                            }
                        });
                    }
                });
        });
}

/// Draws a small slashed Wi-Fi symbol without depending on a font's icon coverage.
fn draw_disconnected_icon(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 14.0), Sense::hover());
    let center = rect.center();
    let color = Color32::from_rgb(255, 112, 112);
    let stroke = Stroke::new(1.2, color);
    for (width, top, bottom) in [(5.0, -5.0, -1.0), (3.0, -2.0, 1.0)] {
        ui.painter().add(egui::epaint::QuadraticBezierShape::from_points_stroke(
            [
                center + egui::vec2(-width, bottom),
                center + egui::vec2(0.0, top),
                center + egui::vec2(width, bottom),
            ],
            false,
            Color32::TRANSPARENT,
            stroke,
        ));
    }
    ui.painter().circle_filled(center + egui::vec2(0.0, 3.0), 1.0, color);
    ui.painter()
        .line_segment([center - egui::vec2(5.0, 5.0), center + egui::vec2(5.0, 5.0)], stroke);
}

/// Draws the army grid interface and emits any resulting local actions.
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
        let can_see = report.can_see(&side, player.id);

        for (i, unit) in army.iter().enumerate() {
            let (survived, total) = if side == Side::Attacker {
                (report.surviving_attacker.amount(unit), report.mission.army.amount(unit))
            } else {
                (report.surviving_defender.amount(unit), report.planet.army.amount(unit))
            };
            let lost = total - survived;

            let text = if can_see {
                if lost > 0 {
                    format!("{lost}/{total}")
                } else {
                    total.to_string()
                }
            } else if report.mission.owner == player.id
                && side == Side::Defender
                && report.scout_probes > (unit.production() - 1) * PROBES_PER_PRODUCTION_LEVEL
            {
                // Even if attacker lost combat, he can see enemy starting units with scouts
                total.to_string()
            } else {
                "?".to_string()
            };

            ui.add_enabled_ui(text != "0", |ui| {
                let response = ui
                    .add_image(images.get(unit.to_lowername()), [65., 65.])
                    .on_hover_small_ext(unit.to_name())
                    .on_disabled_hover_small_ext(unit.to_name());

                ui.add_text_on_image(
                    text,
                    if can_see && lost > 0 {
                        Color32::RED
                    } else {
                        Color32::WHITE
                    },
                    TextStyle::Body,
                    response.rect.left_bottom(),
                    Align2::LEFT_BOTTOM,
                );
            });

            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
}

/// Draws the combat army grid interface and emits any resulting local actions.
fn draw_combat_army_grid(
    ui: &mut Ui,
    name: &str,
    state: &mut UiState,
    round: &RoundReport,
    units: Vec<Unit>,
    side: Side,
    images: &ImageIds,
) -> bool {
    let (own, mut enemy) = match side {
        Side::Attacker => (&round.attacker, round.defender.clone()),
        Side::Defender => (&round.defender, round.attacker.clone()),
    };

    if let Some((u, s)) = &state.combat_report_hover {
        if *s != side {
            enemy = enemy.into_iter().filter(|cu| cu.unit == *u).collect::<Vec<_>>();
        }
    }

    let total_ps = round.buildings.amount(&Unit::planetary_shield()) * PS_SHIELD_PER_LEVEL;

    let n_columns = if name.contains("building") {
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
                    let n = own.iter().filter(|cu| cu.unit == u && seen.insert(cu.id)).count();
                    (n > 0).then_some((u, n))
                }
            })
            .enumerate()
        {
            let n_repaired = own
                .iter()
                .filter_map(|cu| (cu.unit == unit).then_some(cu.repairs.len()))
                .sum::<usize>();
            let shots = enemy
                .iter()
                .flat_map(|u| &u.shots)
                .filter(|s| s.unit == Some(unit))
                .collect::<Vec<_>>();
            let n_shots = shots.len();
            let lost = match unit {
                Unit::Defense(Defense::InterplanetaryMissile) => count,
                Unit::Defense(Defense::AntiballisticMissile) => round.antiballistic_fired,
                _ => shots.iter().filter(|s| s.killed).count(),
            };

            let hovering_crawler =
                matches!(state.combat_report_hover, Some((Unit::Defense(Defense::Crawler), _)));

            ui.add_enabled_ui(
                state
                    .combat_report_hover
                    .as_ref()
                    .is_none_or(|(u, s)| (*s != side || *u == Unit::crawler()) || *u == unit),
                |ui| {
                    let response = ui
                        .add_image(images.get(unit.to_lowername()), [70.; 2])
                        .on_hover_small_ext(unit.to_name());

                    if response.hovered() && !unit.is_building() {
                        any_hovered = true;
                        state.combat_report_hover = Some((unit, side.clone()));
                    }

                    let text = if hovering_crawler && side == Side::Defender {
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

                    let all_cu: Vec<_> = own.iter().filter(|cu| cu.unit == unit).collect();
                    let (hull, shield) = if hovering_crawler && side == Side::Defender {
                        (
                            all_cu
                                .iter()
                                .map(|cu| cu.repairs.iter().sum::<usize>() as f32)
                                .sum::<f32>()
                                .safe_div((count * unit.hull()) as f32),
                            0.,
                        )
                    } else if unit.is_building() {
                        if unit == Unit::planetary_shield() {
                            let mut ps = round.planetary_shield as f32;
                            if let Some((hu, hs)) = &state.combat_report_hover {
                                if *hs != side {
                                    ps = enemy
                                        .iter()
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
                            .iter()
                            .map(|cu| {
                                if lost == count {
                                    0.
                                } else {
                                    cu.shield as f32
                                }
                            })
                            .sum::<f32>()
                            .safe_div((all_cu.len() * unit.shield()) as f32);

                        let mut hull = all_cu
                            .iter()
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
                            .safe_div((count * unit.hull()) as f32);

                        if let Some((hu, hs)) = &state.combat_report_hover {
                            if *hs != side {
                                let (s_sum, h_sum) = enemy
                                    .iter()
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
                                shield = s_sum.safe_div((count * unit.shield()) as f32).min(1.);
                                hull = h_sum.safe_div((count * unit.hull()) as f32);
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

const RESOURCE_BAR_TOP: f32 = 9.0;
const RESOURCE_BAR_SIDE_INSET: f32 = 9.0;
const RESOURCE_BAR_ROW_HEIGHT: f32 = 50.0;
const RESOURCE_BAR_VERTICAL_MARGIN: f32 = 4.0;
const RESOURCE_SUMMARY_HORIZONTAL_PADDING: f32 = 4.0;
const RESOURCE_SUMMARY_TEXT_VERTICAL_OFFSET: f32 = 2.0;

fn resource_summary_style(compact: bool, scale: f32) -> (egui::Vec2, f32, f32, f32) {
    let (icon_size, spacing, label_size, value_size) = if compact {
        (egui::vec2(52.0, 34.0), 8.0, 9.0, 24.0)
    } else {
        (egui::vec2(70.0, 44.0), 11.0, 11.0, 30.0)
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
        Color32::WHITE,
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
    ui.painter().image(
        icon,
        icon_rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    let text_x = icon_rect.right() + spacing;
    // The font's visible glyphs sit slightly above its line box, so this small optical offset
    // makes the label/value stack look centered rather than mathematically centered but high.
    let text_height = label.size().y + value.size().y;
    let text_top =
        rect.center().y - text_height * 0.5 + RESOURCE_SUMMARY_TEXT_VERTICAL_OFFSET * scale;
    let value_top = text_top + label.size().y;
    ui.painter().galley(egui::pos2(text_x, text_top), label, Color32::WHITE);
    ui.painter().galley(egui::pos2(text_x, value_top), value, Color32::WHITE);

    response
}

/// Reserves breathing room between resource summaries and optionally paints a section divider.
fn draw_resource_gap(ui: &mut Ui, width: f32, divided: bool, scale: f32) {
    let (rect, _) =
        ui.allocate_exact_size(egui::vec2(width, RESOURCE_BAR_ROW_HEIGHT * scale), Sense::hover());
    if divided {
        ui.painter().line_segment(
            [
                egui::pos2(rect.center().x, rect.top() + 5.0 * scale),
                egui::pos2(rect.center().x, rect.bottom() - 5.0 * scale),
            ],
            Stroke::new(scale, Color32::from_rgba_unmultiplied(130, 170, 215, 55)),
        );
    }
}

fn resource_bar_gap(compact: bool, scale: f32) -> f32 {
    let gap = if compact {
        10.0
    } else {
        24.0
    };
    gap * scale
}

fn resource_bar_section_gap(compact: bool, scale: f32) -> f32 {
    let gap = if compact {
        24.0
    } else {
        48.0
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
) -> f32 {
    let (n_owned, n_max_owned) = player.planets_owned(map, settings);
    let mut width = resource_summary_width(ui, "TURN", &settings.turn.to_string(), compact, scale)
        + resource_summary_width(
            ui,
            "PLANETS",
            &format!("{n_owned}/{n_max_owned}"),
            compact,
            scale,
        );
    for resource in ResourceName::iter() {
        width += resource_summary_width(
            ui,
            &resource.to_name().to_uppercase(),
            &player.resources.get(&resource).to_string(),
            compact,
            scale,
        );
    }

    width + resource_bar_gap(compact, scale) * 3.0 + resource_bar_section_gap(compact, scale)
}

fn draw_resource_tooltip(
    ui: &mut Ui,
    resource: ResourceName,
    map: &Map,
    player: &Player,
    images: &ImageIds,
) -> egui::Rect {
    ui.horizontal(|ui| {
        let image_rect = ui.add_image(images.get(resource.to_lowername()), [130.0, 90.0]).rect;
        ui.vertical(|ui| {
            ui.set_max_width(360.0);
            ui.label(RichText::new(resource.to_name()).strong());
            ui.separator();
            ui.scope(|ui| {
                ui.style_mut().interaction.selectable_labels = true;
                ui.small(format!(
                    "Production: +{}",
                    player.resource_production(&map.planets).get(&resource)
                ))
                .on_hover_cursor(CursorIcon::Default)
                .on_hover_text_at_pointer(
                    RichText::new(
                        map.planets()
                            .iter()
                            .filter_map(|planet| {
                                player.owns(planet).then_some((
                                    planet.name.clone(),
                                    planet.resource_production().get(&resource),
                                ))
                            })
                            .sorted_by(|left, right| right.1.cmp(&left.1))
                            .map(|(name, production)| format!("{name}: {production}"))
                            .join("\n"),
                    )
                    .small(),
                );
            });
            ui.add_space(3.0);
            ui.small(resource.description());
        });
        image_rect
    })
    .inner
}

/// Draws the resources interface and emits any resulting local actions.
fn draw_resources(
    ui: &mut Ui,
    settings: &Settings,
    map: &Map,
    player: &Player,
    images: &ImageIds,
    compact: bool,
    scale: f32,
) {
    let gap = resource_bar_gap(compact, scale);
    let section_gap = resource_bar_section_gap(compact, scale);
    let (n_owned, n_max_owned) = player.planets_owned(map, settings);
    let resource_count = ResourceName::iter().count();

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = egui::Vec2::ZERO;

        let response = draw_resource_summary(
            ui,
            images.get("turn"),
            "TURN",
            &settings.turn.to_string(),
            compact,
            scale,
        );
        if settings.show_hover {
            response.on_hover_ui(|ui| {
                ui.horizontal(|ui| {
                    ui.add_image(images.get("turn"), [130.0, 90.0]);
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Turn").strong());
                        ui.separator();
                        ui.small("Current turn in the game.");
                    });
                });
            });
        }

        draw_resource_gap(ui, gap, false, scale);

        let response = draw_resource_summary(
            ui,
            images.get("owned"),
            "PLANETS",
            &format!("{n_owned}/{n_max_owned}"),
            compact,
            scale,
        );
        if settings.show_hover {
            response.on_hover_ui(|ui| {
                ui.horizontal(|ui| {
                    ui.add_image(images.get("owned"), [130.0, 90.0]);
                    ui.vertical(|ui| {
                        ui.set_max_width(320.0);
                        ui.label(RichText::new("Planets owned / Max. owned").strong());
                        ui.separator();
                        ui.small(
                            "The current number of planets owned and the maximum number of \
                            planets that can be owned this game. A spot becomes available if an \
                            owned planet is abandoned, conquered, or destroyed.",
                        );
                    });
                });
            });
        }

        draw_resource_gap(ui, section_gap, true, scale);

        for (index, resource) in ResourceName::iter().enumerate() {
            let response = draw_resource_summary(
                ui,
                images.get(resource.to_lowername()),
                &resource.to_name().to_uppercase(),
                &player.resources.get(&resource).to_string(),
                compact,
                scale,
            );

            if settings.show_hover {
                response.on_hover_ui(|ui| {
                    draw_resource_tooltip(ui, resource, map, player, images);
                });
            }

            if index + 1 < resource_count {
                draw_resource_gap(ui, gap, false, scale);
            }
        }
    });
}

/// Shows turn, ownership, and stockpile totals in the same framed style as the world list.
fn draw_resources_widget(
    context: &egui::Context,
    settings: &Settings,
    map: &Map,
    player: &Player,
    images: &ImageIds,
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
                let compact =
                    resource_bar_content_width(ui, settings, map, player, false, scale) > max_width;
                draw_resources(ui, settings, map, player, images, compact, scale);
            });
        })
        .response
        .rect
}

/// Draws the planet overview interface and emits any resulting local actions.
fn draw_planet_overview(
    ui: &mut Ui,
    id: PlanetId,
    map: &mut Map,
    player: &mut Player,
    settings: &Settings,
    message: &mut MessageWriter<MessageMsg>,
    pending: &mut PendingTurnCommands,
    abandon_confirmation: &mut Option<PlanetId>,
    detail_line_progress: &[f32; PLANET_DETAIL_LINE_COUNT],
    right_side: bool,
    images: &ImageIds,
) {
    let (n_owned, n_max_owned) = player.planets_owned(map, settings);

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
                RichText::new(format!(
                    "📐 Diameter: {}km ({:.0}%)",
                    format_thousands(planet.diameter),
                    planet.destroy_probability() * 100.,
                ))
                .small(),
                detail_line_progress[1],
                right_side,
            )
            .on_hover_small(
                "Smaller planets are easier to destroy than larger ones, since it's easier \
                to reach their core with a Death Ray, the weapon used by War Suns. The percentage \
                indicates the initial probability a War Sun has of destroying this planet after a \
                combat round.",
            );
            draw_sliding_text(
                ui,
                RichText::new(format!(
                    "{} Temperature: {}°C to {}°C",
                    planet.kind.temperature_emoji(),
                    planet.temperature.0,
                    planet.temperature.1
                ))
                .small(),
                detail_line_progress[2],
                right_side,
            );
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

    if !planet.is_moon() {
        let owned = pending.is_editable() && player.owns(planet) && player.home_planet != planet.id;
        let controlled = pending.is_editable() && player.controls(planet) && !player.owns(planet);

        let size = egui::vec2(40., 40.);
        let pos = rect.left_bottom() - egui::vec2(-20., size.y + 7.);
        let rect = egui::Rect::from_min_size(pos, size);

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

                ui.add_image_painter(images.get("abandon"), rect);

                if response.clicked() {
                    *abandon_confirmation = Some(planet.id);
                }
            });
        } else if controlled {
            ui.add_enabled_ui(
                planet.army.amount(&Unit::colony_ship()) > 0 && n_owned < n_max_owned,
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

                    ui.add_image_painter(images.get("colonize"), rect);

                    if response.clicked() {
                        if !pending.push(TurnCommand::ColonizePlanet {
                            planet_id: planet.id,
                        }) {
                            message.write(MessageMsg::error(
                                "This turn already contains the maximum number of commands.",
                            ));
                            return;
                        }
                        let colony_ships = planet.army.entry(Unit::colony_ship()).or_insert(1);
                        *colony_ships = colony_ships.saturating_sub(1);
                        planet.colonize(player.id);
                        // The map presentation announces the ownership change once, including
                        // direct colonization and colonies established by arriving missions.
                    }
                },
            );
        }
    }
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
        message
            .write(MessageMsg::error("This turn already contains the maximum number of commands."));
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
            surviving_defender: Army::new(),
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
fn draw_overview(ui: &mut Ui, planet: &Planet, images: &ImageIds) {
    ui.add_space(17.);

    ui.horizontal(|ui| {
        let text = &planet.name;
        let size_x = ui
            .painter()
            .layout_no_wrap(text.clone(), TextStyle::Small.resolve(ui.style()), Color32::WHITE)
            .size()
            .x;

        ui.spacing_mut().item_spacing.x = 7.;
        ui.add_space((ui.available_width() - size_x - 27.) * 0.5);
        ui.add_image(images.get("overview"), [20.; 2]);
        ui.small(text);
    });

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

        ui.add_space(10.);

        for units in Unit::all_valid(planet.is_moon()) {
            ui.add_space(5.);

            ui.vertical(|ui| {
                for unit in units {
                    let n = planet.army.amount(&unit);

                    ui.add_enabled_ui(n > 0, |ui| {
                        let response = ui.add_image(images.get(unit.to_lowername()), [50.; 2]);
                        ui.add_text_on_image(
                            n.to_string(),
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

/// Draws the report overview interface and emits any resulting local actions.
fn draw_report_overview(ui: &mut Ui, planet: &Planet, info: &PlanetInfo, images: &ImageIds) {
    ui.add_space(17.);

    ui.horizontal(|ui| {
        let text = format!("{} ({})", planet.name, info.turn);
        let size_x = ui
            .painter()
            .layout_no_wrap(text.clone(), TextStyle::Small.resolve(ui.style()), Color32::WHITE)
            .size()
            .x;

        ui.add_space((ui.available_width() - size_x) * 0.5);
        ui.small(text);
    })
    .response
    .on_hover_small(format!("Intelligence from turn {}.", info.turn));

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

        ui.add_space(10.);
        for units in Unit::all_valid(planet.is_moon()) {
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

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.;
        ui.add_space(10.);
        ui.add_image(images.get(mission.image(player)), [25.; 2]);
        ui.small("Mission");
    });

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
                        if mission.owner != player.id
                            && !player.spectator
                            && mission
                                .is_seen_by_phalanx(map, player)
                                .map(|lvl| unit.production() > lvl)
                                .unwrap_or(true)
                            && mission
                                .is_seen_by_radar(map, player)
                                .map(|lvl| unit.production() > lvl)
                                .unwrap_or(true)
                        {
                            "?".to_string()
                        } else {
                            n.to_string()
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

        ui.add_image(images.get(report.mission.objective.to_lowername()), [25.; 2]);
        ui.add_image(images.get(report.mission.image(player)), [50.; 2]);
        ui.small(report.turn.to_string());

        ui.add_space(25.);

        ui.small(&destination.name);
        ui.add_space(5.);
        let resp = ui.add_image(images.get(destination.image()), [35., 35.]);

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

    let round = if state.combat_report_total {
        let mut rr = combat.rounds.iter().fold(RoundReport::default(), |mut rr, r| {
            rr.attacker.extend(r.attacker.clone());
            rr.defender.extend(r.defender.clone());
            rr.planetary_shield = rr.planetary_shield.saturating_add(r.planetary_shield);
            rr.antiballistic_fired = rr.antiballistic_fired.saturating_add(r.antiballistic_fired);
            if rr.buildings.is_empty() {
                rr.buildings = r.buildings.clone()
            }
            rr
        });

        rr.destroy_probability =
            1. - combat.rounds.iter().fold(1., |acc, p| acc * (1. - p.destroy_probability));
        rr
    } else {
        combat.rounds[state.combat_report_round - 1].clone()
    };

    let draw_stats = |ui: &mut Ui, units: Vec<&CombatUnit>, side: Side| {
        let shots = units.iter().flat_map(|u| &u.shots).collect::<Vec<_>>();

        let shield_damage = shots.iter().map(|a| a.shield_damage).sum::<usize>();
        let hull_damage = shots.iter().map(|a| a.hull_damage).sum::<usize>();
        let ps_damage = shots.iter().map(|a| a.planetary_shield_damage).sum::<usize>();

        let u_shots = shots
            .iter()
            .filter(|s| matches!(s.unit, Some(u) if !u.is_building()))
            .collect::<Vec<_>>();
        let m_shots = shots
            .iter()
            .filter(|s| s.unit == Some(Unit::interplanetary_missile()))
            .collect::<Vec<_>>();
        let b_shots = shots
            .iter()
            .filter(
                |s| matches!(s.unit, Some(u) if u.is_building() && u != Unit::planetary_shield()),
            )
            .collect::<Vec<_>>();
        let shots_missed = u_shots.iter().filter(|s| s.missed).count();
        let total_repaired = units.iter().map(|cu| cu.repairs.iter().sum::<usize>()).sum::<usize>();
        let missiles_hit = m_shots.iter().filter(|s| s.killed).count();
        let bombs_hit = b_shots.iter().filter(|s| s.killed).count();

        let rapid_fire = shots.iter().filter(|a| a.rapid_fire).count();
        let enemies_killed = shots.iter().filter(|a| a.killed).count();

        let draw_row = |ui: &mut Ui, icon: &str, val: String, hover: &str| {
            ui.vertical_centered(|ui| {
                ui.label(icon).on_hover_small(hover);
            });
            ui.label(if units.is_empty() {
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
                draw_row(ui, "🛡", shield_damage.fmt(), "Damage dealt to shields.");
                draw_row(ui, "🔰", hull_damage.fmt(), "Damage dealt to hulls.");
                if side == Side::Attacker {
                    draw_row(ui, "🌐", ps_damage.fmt(), "Damage dealt to the planetary shield.");
                }
                draw_row(
                    ui,
                    "⚔",
                    (shield_damage + hull_damage + ps_damage).fmt(),
                    "Total damage dealt.",
                );
                if side == Side::Defender {
                    draw_row(
                        ui,
                        "❤",
                        total_repaired.to_string(),
                        "Total hull points repaired by Crawlers.",
                    );
                }
                draw_row(
                    ui,
                    "❌",
                    format!("{:.0}%", (shots_missed as f32).safe_div(u_shots.len() as f32) * 100.),
                    "Percentage of shots that missed a target. A shot misses when it \
                    fires on a unit that was already destroyed that round.",
                );
                draw_row(
                    ui,
                    "🔥",
                    format!("{:.0}%", (rapid_fire as f32).safe_div(u_shots.len() as f32) * 100.),
                    "Percentage of shots that gained rapid fire.",
                );
                if report.mission.objective == Icon::MissileStrike && side == Side::Defender {
                    draw_row(
                        ui,
                        "🚀",
                        format!(
                            "{:.0}%",
                            (missiles_hit as f32).safe_div(m_shots.len() as f32) * 100.
                        ),
                        "Percentage of Antiballistic Missiles that intercepted an \
                        incoming Interplanetary Missile.",
                    );
                }
                if report.mission.bombing != BombingRaid::None && side == Side::Attacker {
                    draw_row(
                        ui,
                        "💣",
                        format!("{:.0}%", (bombs_hit as f32).safe_div(b_shots.len() as f32) * 100.),
                        "Percentage of bombs that hit enemy buildings.",
                    );
                }
                draw_row(ui, "💀", enemies_killed.fmt(), "Number of enemy units destroyed.");
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

    let attacker_id = report.mission.owner;
    let defender_id = report.planet.controlled.or(report.planet.owned);
    let attack_c = session.player_color(attacker_id).color();
    let defend_c =
        defender_id.map_or(Color::srgb_u8(150, 158, 170), |id| session.player_color(id).color());
    let attacker_name = session
        .player_name(attacker_id)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Player {attacker_id}"));
    let defender_name = defender_id.map(|id| {
        session.player_name(id).map(str::to_owned).unwrap_or_else(|| format!("Player {id}"))
    });

    ui.horizontal(|ui| {
        ui.add_space(40.);

        ui.visuals_mut().widgets.noninteractive.bg_stroke.width = 6.;

        ui.vertical(|ui| {
            ui.set_width(attacker_w);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("Attacker · {attacker_name}"))
                        .strong()
                        .color(attack_c.to_color32()),
                );
            });
            ui.visuals_mut().widgets.noninteractive.bg_stroke.color = attack_c.to_color32();
            ui.separator();
        });
        ui.vertical(|ui| {
            ui.set_width(defender_w);
            ui.label(
                RichText::new(
                    defender_name.map_or_else(
                        || "Defender".to_string(),
                        |name| format!("Defender · {name}"),
                    ),
                )
                .strong()
                .color(defend_c.to_color32()),
            );
            ui.visuals_mut().widgets.noninteractive.bg_stroke.color = defend_c.to_color32();
            ui.separator();
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

                    let units = round
                        .attacker
                        .iter()
                        .filter(|cu| {
                            state
                                .combat_report_hover
                                .as_ref()
                                .is_none_or(|(u, s)| *u == cu.unit && *s == Side::Attacker)
                        })
                        .collect::<Vec<_>>();

                    draw_stats(ui, units, Side::Attacker);
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

                    if round.defender.is_empty() {
                        ui.label("No defending units.");
                    } else {
                        ui.horizontal_top(|ui| {
                            let hovered1 = if report.mission.objective != Icon::MissileStrike {
                                if round.defender.iter().any(|cu| cu.unit.is_ship()) {
                                    draw_combat_army_grid(
                                        ui,
                                        "combat_defender1",
                                        state,
                                        &round,
                                        Unit::ships(),
                                        Side::Defender,
                                        images,
                                    )
                                } else {
                                    false
                                }
                            } else {
                                false
                            };

                            let defenses: Vec<Unit> = round
                                .defender
                                .iter()
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
                                    images,
                                )
                            } else {
                                false
                            };

                            any_hovered = any_hovered || hovered1 || hovered2;

                            if report.planet.army.amount(&Unit::planetary_shield()) > 0
                                && report.mission.objective != Icon::MissileStrike
                            {
                                draw_combat_army_grid(
                                    ui,
                                    "combat_buildings1",
                                    state,
                                    &round,
                                    vec![Unit::planetary_shield()],
                                    Side::Defender,
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
                                    images,
                                );
                            }
                        });
                    }
                });

                ui.horizontal_top(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.vertical(|ui| {
                            let units = round
                                .defender
                                .iter()
                                .filter(|cu| {
                                    state
                                        .combat_report_hover
                                        .as_ref()
                                        .is_none_or(|(u, s)| *u == cu.unit && *s == Side::Defender)
                                })
                                .collect::<Vec<_>>();

                            draw_stats(ui, units, Side::Defender);
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
        ui.add_space(50.);
        ui.horizontal(|ui| {
            ui.add_space(40.);
            if ui.add_custom_button("Close details", images).clicked() {
                state.combat_report = None;
            }

            ui.add_space(310.);

            ui.small("Hover over a unit to show the statistics for that unit only.");
        });
    });
}

/// Draws the mission info hover interface and emits any resulting local actions.
fn draw_mission_info_hover(
    ui: &mut Ui,
    mission: &Mission,
    settings: &Settings,
    map: &Map,
    player: &Player,
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
        let objective = if mission.owner == player.id {
            mission.objective
        } else {
            Icon::Attacked
        };
        ui.add_image(images.get(objective.to_lowername()), [20.; 2]);
        ui.small(objective.to_name());
    });

    ui.add(Separator::default().shrink(20.));

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.vertical(|ui| {
            ui.small(format!("📏 Distance: {:.1} AU", mission.distance(map)));

            let speed = mission.speed();
            ui.small(format!(
                "🚀 Speed: {}",
                if speed == f32::MAX {
                    "---".to_string()
                } else {
                    format!("{speed} AU/turn")
                }
            ));

            let duration = mission.duration(map);
            ui.small(format!(
                "⏱ Duration: +{} turn{} ({})",
                duration,
                if duration == 1 {
                    ""
                } else {
                    "s"
                },
                settings.turn + duration
            ));
        });
    });
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
    let reports = player
        .reports
        .iter()
        .filter(|r| {
            r.turn == settings.turn
                && !r.hidden
                && r.combat_report.is_some()
                && r.can_see(&Side::Defender, player.id)
        })
        .collect::<Vec<_>>();

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
                let destination = map.get(report.mission.destination);

                let (rect, response) =
                    ui.allocate_exact_size([ui.available_width(), 72.].into(), Sense::click());
                let response = response.on_hover_cursor(CursorIcon::PointingHand);
                let hovered = response.hovered();
                let pressed = response.is_pointer_button_down_on();
                let attacker_id = report.mission.owner;
                let defender_id = report.planet.controlled.or(report.planet.owned);
                let player_color = session.player_color(attacker_id).color().to_color32();
                let opponent_id = if attacker_id == player.id {
                    defender_id
                } else {
                    Some(attacker_id)
                };
                let opponent = opponent_id.map(|id| {
                    (
                        session
                            .player_name(id)
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("Player {id}")),
                        session.player_color(id).color().to_color32(),
                    )
                });

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
                let planet_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.right() - 33., center_y),
                    egui::vec2(42., 42.),
                );
                let fleet_rect = egui::Rect::from_center_size(
                    egui::pos2(planet_rect.left() - 28., center_y),
                    egui::vec2(36., 36.),
                );
                let objective_rect = egui::Rect::from_center_size(
                    egui::pos2(fleet_rect.left() - 22., center_y),
                    egui::vec2(24., 24.),
                );
                let text_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 17., rect.top() + 8.),
                    egui::pos2(objective_rect.left() - 10., rect.bottom() - 8.),
                );
                let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1., 1.));

                ui.painter().image(
                    images.get(report.mission.objective.to_lowername()),
                    objective_rect,
                    uv,
                    Color32::WHITE,
                );
                ui.painter().image(
                    images.get(report.mission.image(player)),
                    fleet_rect,
                    uv,
                    player_color,
                );
                ui.painter().image(
                    images.get(destination.image()),
                    planet_rect,
                    uv,
                    Color32::WHITE,
                );

                let text_painter = ui.painter().with_clip_rect(text_rect);
                text_painter.text(
                    egui::pos2(
                        text_rect.left(),
                        if opponent.is_some() {
                            center_y - 9.0
                        } else {
                            center_y
                        },
                    ),
                    Align2::LEFT_CENTER,
                    format!("Battle of {}", destination.name),
                    TextStyle::Body.resolve(ui.style()),
                    Color32::WHITE,
                );
                if let Some((opponent_name, opponent_color)) = opponent {
                    text_painter.circle_filled(
                        egui::pos2(text_rect.left() + 2.5, center_y + 13.0),
                        2.5,
                        opponent_color,
                    );
                    text_painter.text(
                        egui::pos2(text_rect.left() + 10.0, center_y + 13.0),
                        Align2::LEFT_CENTER,
                        opponent_name,
                        egui::FontId::new(14.0, FontFamily::Proportional),
                        opponent_color,
                    );
                }

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
    mut send_mission: MessageWriter<SendMissionMsg>,
    mut message: MessageWriter<MessageMsg>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    missions: Res<Missions>,
    mut state: ResMut<UiState>,
    mut settings: ResMut<Settings>,
    mut pending: ResMut<PendingTurnCommands>,
    session: Res<MultiplayerSession>,
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    images: Res<ImageIds>,
    window: Single<&Window>,
    mut planet_panel_slide: Local<PlanetPanelSlide>,
) {
    if game_state.get().is_modal_menu() {
        return;
    }

    let (width, height) = (window.width(), window.height());

    if *game_state.get() == GameState::Playing {
        if let Ok(context) = contexts.ctx_mut() {
            draw_enemy_players_widget(context, &session, &player);
            draw_owned_worlds_widget(context, &map, &player, &mut state, &mut settings, &images);
            draw_resources_widget(context, &settings, &map, &player, &images);
        }
    }

    if !state.mission {
        state.mission_planet_hover = None;
    }

    // Mission-panel planet links preview only known units. Map hover and selection retain the
    // complete planet interface and map annotations.
    let planet_panel = visible_planet_panel(&state);

    if let Some((id, mode)) = planet_panel {
        let right_side = state.planet_selected.is_some()
            || window.cursor_position().map(|pos| pos.x < width * 0.5).unwrap_or_default();

        let planet = map.get(id);

        let (window_w, window_h) = if planet.is_moon() {
            (145., 630.)
        } else {
            (205., 630.)
        };

        let delta_seconds =
            contexts.ctx_mut().map_or(0.0, |context| context.input(|input| input.stable_dt));
        let slide_progress = planet_panel_slide.progress(
            PlanetPanelSlideTarget {
                id,
                mode,
                right_side,
            },
            delta_seconds,
        );
        let slide_distance = window_w + 518.0;
        let slide_x = planet_panel_slide_offset(slide_progress, right_side, slide_distance);
        let detail_line_progress =
            std::array::from_fn(|line| planet_panel_slide.detail_progress(line));
        if planet_panel_slide.is_animating() {
            if let Ok(context) = contexts.ctx_mut() {
                context.request_repaint();
            }
        }

        let mut draw_planet_info = |contexts, id, map, player, abandon_confirmation, extension| {
            let (window_w2, window_h2) = (518., 216.);

            draw_sliding_panel(
                contexts,
                "planet overview",
                "panel",
                (
                    if right_side {
                        width * 0.998
                            - window_w2
                            - if extension {
                                window_w
                            } else {
                                0.
                            }
                    } else {
                        width * 0.002
                            + if extension {
                                window_w
                            } else {
                                0.
                            }
                    } + slide_x,
                    height * 0.5 - window_h * 0.5 + 27.,
                ),
                (window_w2, window_h2),
                slide_distance,
                &images,
                |ui| {
                    draw_planet_overview(
                        ui,
                        id,
                        map,
                        player,
                        &settings,
                        &mut message,
                        &mut pending,
                        abandon_confirmation,
                        &detail_line_progress,
                        right_side,
                        &images,
                    )
                },
            );
        };

        // Check whether there is a report on this planet
        let info = player.last_info(planet, &missions.0);

        if player.controls(planet) || player.spectator {
            draw_sliding_panel(
                &mut contexts,
                "overview",
                "panel",
                (
                    if right_side {
                        width * 0.998 - window_w
                    } else {
                        width * 0.002
                    } + slide_x,
                    height * 0.5 - window_h * 0.5,
                ),
                (window_w, window_h),
                slide_distance,
                &images,
                |ui| draw_overview(ui, planet, &images),
            );

            if mode == PlanetPanelMode::Full {
                draw_planet_info(
                    &mut contexts,
                    id,
                    &mut map,
                    &mut player,
                    &mut state.abandon_confirmation,
                    true,
                );
            }
        } else if let Some(info) = info {
            // Don't use has_army since no units is also valid information
            if !planet.is_destroyed && !info.army.is_empty() {
                draw_sliding_panel(
                    &mut contexts,
                    "report overview",
                    "panel",
                    (
                        if right_side {
                            width * 0.998 - window_w
                        } else {
                            width * 0.002
                        } + slide_x,
                        height * 0.5 - window_h * 0.5,
                    ),
                    (window_w, window_h),
                    slide_distance,
                    &images,
                    |ui| draw_report_overview(ui, planet, &info, &images),
                );

                if mode == PlanetPanelMode::Full {
                    draw_planet_info(
                        &mut contexts,
                        id,
                        &mut map,
                        &mut player,
                        &mut state.abandon_confirmation,
                        true,
                    );
                }
            } else if !planet.is_destroyed && mode == PlanetPanelMode::Full {
                draw_planet_info(
                    &mut contexts,
                    id,
                    &mut map,
                    &mut player,
                    &mut state.abandon_confirmation,
                    false,
                );
            }
        } else if !planet.is_destroyed && mode == PlanetPanelMode::Full {
            draw_planet_info(
                &mut contexts,
                id,
                &mut map,
                &mut player,
                &mut state.abandon_confirmation,
                false,
            );
        }
    } else {
        planet_panel_slide.hide();
    }

    if let Some(mission_id) = state.mission_hover {
        let Some(mission) = missions.get(mission_id) else {
            state.mission_hover = None;
            return;
        };

        let (fleet_x, info_x) =
            mission_hover_panel_x_positions(window.cursor_position().map(|pos| pos.x), width);
        let window_h = 630.0;

        draw_panel(
            &mut contexts,
            "mission hover fleet",
            "panel",
            (fleet_x, height * 0.5 - window_h * 0.5),
            (MISSION_HOVER_FLEET_WIDTH, window_h),
            &images,
            |ui| draw_mission_fleet_hover(ui, mission, &map, &player, &images),
        );

        // Objective names such as "Missile Strike" must fit beside their icon and label.
        let window_h2 = 280.0;

        draw_panel(
            &mut contexts,
            "mission hover info",
            "panel",
            (info_x, height * 0.5 - window_h * 0.5 + 27.0),
            (MISSION_HOVER_INFO_WIDTH, window_h2),
            &images,
            |ui| draw_mission_info_hover(ui, mission, &settings, &map, &player, &images),
        );
    }

    // Keep the previous hover for drawing, but require the mission list to renew it.
    let mission_hover_from_ui = std::mem::take(&mut state.mission_hover_from_ui);

    if state.mission {
        let (window_w, window_h) = (850., 640.);

        let is_hovered = contexts.ctx_mut().is_ok_and(|ctx| ctx.is_pointer_over_egui());
        draw_panel(
            &mut contexts,
            "mission",
            "panel",
            ((width - window_w) * 0.5, (height - window_h) * 0.5),
            (window_w, window_h),
            &images,
            |ui| {
                draw_mission(
                    ui,
                    &missions.0,
                    &mut send_mission,
                    &settings,
                    &mut state,
                    &mut map,
                    &mut player,
                    &session,
                    is_hovered,
                    &keyboard,
                    &images,
                    pending.is_editable(),
                )
            },
        );
    } else if let Some(id) = state.planet_selected {
        if settings.show_menu && !player.spectator {
            // Hide shop if hovering another planet
            if state.planet_hover.is_none_or(|planet_id| planet_id == id) {
                let planet = map.get_mut(id);

                if player.owns(planet) || (planet.is_moon() && player.controls(planet)) {
                    let (window_w, window_h) = (735., 340.);

                    draw_panel(
                        &mut contexts,
                        "shop",
                        "panel",
                        (width * 0.5 - window_w * 0.5, height * 0.995 - window_h),
                        (window_w, window_h),
                        &images,
                        |ui| {
                            draw_shop(
                                ui,
                                &mut state,
                                &settings,
                                &mut player,
                                planet,
                                &mut pending,
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

        draw_panel(
            &mut contexts,
            "combat report",
            "panel",
            (width * 0.5 - window_w * 0.5, height * 0.9 - window_h),
            (window_w, window_h),
            &images,
            |ui| draw_combat_report(ui, &mut state, &map, &player, &session, &images),
        );
    }

    if *game_state.get() == GameState::CombatMenu {
        let (window_w, window_h) = (380., 420.);

        draw_panel(
            &mut contexts,
            "combat list",
            "panel",
            ((width - window_w) * 0.5, (height - window_h) * 0.5),
            (window_w, window_h),
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

    if let Some(id) = state.abandon_confirmation {
        let planet = map.get(id);
        let can_abandon = pending.is_editable()
            && !planet.is_moon()
            && player.owns(planet)
            && player.home_planet != id
            && planet.buy.is_empty();

        if !can_abandon {
            state.abandon_confirmation = None;
        } else {
            if let Ok(context) = contexts.ctx_mut() {
                if let Some(action) = draw_abandon_confirmation(context, &images) {
                    state.abandon_confirmation = None;
                    if action == AbandonConfirmationAction::Confirm {
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
}

#[cfg(test)]
#[path = "../../../tests/core/ui_systems.rs"]
mod tests;
