//! Missions panels for the game interface.

use super::*;
use crate::core::combat::plan::BattlePlan;
use crate::core::identity::PlayerId;
use crate::core::map::systems::JUMP_GATE_HELIX_ANGULAR_SPEED;
use crate::core::messages::show_notification_area;
use crate::core::missions::MissionRouteStyle;
use crate::core::ui::utils::HOVER_TOOLTIP_TEXT_SIZE;

/// Shows the launch-only Energy charge while the draft uses a Jump Gate.
pub(super) fn draw_jump_energy_cost(ui: &mut Ui, mission: &Mission, images: &ImageIds) {
    if mission.jump_gate {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let (rect, _) = ui.allocate_exact_size(egui::vec2(30.0, 21.0), Sense::hover());
            paint_bordered_resource_image(ui, images.get("energy"), rect, 2.0);
            ui.small(mission.jump_energy_cost().to_string());
        });
    }
}

const MISSION_PLANET_COLUMN_WIDTH: f32 = 120.0;
const MISSION_PLANET_CELL_HEIGHT: f32 = 100.0;
const MISSION_PLANET_IMAGE_SIZE: f32 = 60.0;
const MISSION_PLANET_NAME_HEIGHT: f32 = 18.0;
const MISSION_PLANET_NAME_OVERLAP: f32 = 14.0;
const MISSION_ROUTE_PREVIEW_HEIGHT: f32 = 34.0;
const MISSION_ROUTE_ICON_SIZE: f32 = 25.0;
const MISSION_PREVIEW_HELIX_WAVELENGTH: f32 = 60.0;
const MISSION_RECALL_BUTTON_SIZE: f32 = 32.0;
const MISSION_INVITE_ICON_SIZE: f32 = 40.0;
const MISSION_OBJECTIVE_BUTTON_SIZE: egui::Vec2 = egui::vec2(44.0, 48.0);
const MISSION_OBJECTIVE_BUTTON_GAP: f32 = 3.0;
const MISSION_OBJECTIVE_ACCENT: Color32 = Color32::from_rgb(231, 184, 89);
const MISSION_PLAYERS_SUMMARY_WIDTH: f32 = 86.0;
const MISSION_ROUTE_FIXED_CONTENT_WIDTH: f32 = 122.0;
const MISSION_LOG_BADGE_SIZE: f32 = 20.0;
const MISSION_BADGE_ICON_SIZE: f32 = 28.0;
const MISSION_BADGE_FONT_SIZE: f32 = 24.0;
const MISSION_STRENGTH_TEXT_GAP: f32 = 6.0;
const MISSION_PANEL_HEIGHT: f32 = 640.0;
const MISSION_COLUMN_GAP: f32 = 8.0;
const MISSION_ROW_HORIZONTAL_INSET: f32 = 16.0;
const MISSION_ROUTE_COLUMN_MIN_WIDTH: f32 = 330.0;
const MISSION_ROUTE_COLUMN_MAX_WIDTH: f32 = 620.0;
const MISSION_FLEET_IMAGE_SCALE: f32 = 0.9;
const MISSION_COLONY_IMAGE_SCALE: f32 = 0.86;
const MISSION_SPY_IMAGE_SCALE: f32 = 0.82;
const MISSION_REPORT_IMAGE_SLOT_SIZE: f32 = 52.0;
const MISSION_REPORT_LIST_TOP_PADDING: f32 = 2.0;
const MISSION_REPORT_HOVER_STROKE_WIDTH: f32 = 1.5;
const MISSION_REPORT_IMAGE_SIZE: f32 = 48.0;
const MISSION_REPORT_SELECTED_IMAGE_SIZE: f32 = 52.0;
const MISSION_MISSILE_IMAGE_OFFSET_X: f32 = -4.0;
const MISSION_REPORT_ROUTE_PLANET_WIDTH: f32 = 100.0;
const MISSION_REPORT_ROUTE_PLAYER_WIDTH: f32 = 100.0;
const MISSION_REPORT_ROUTE_CENTER_WIDTH: f32 = 100.0;
const MISSION_REPORT_ROUTE_GAP: f32 = 10.0;
// Report endpoints need only their artwork and caption, without the route controls' padding.
const MISSION_REPORT_ROUTE_HEIGHT: f32 =
    MISSION_PLANET_IMAGE_SIZE - MISSION_PLANET_NAME_OVERLAP + MISSION_PLANET_NAME_HEIGHT;
const MISSION_REPORT_ROUTE_WIDTH: f32 = MISSION_REPORT_ROUTE_PLANET_WIDTH * 2.0
    + MISSION_REPORT_ROUTE_PLAYER_WIDTH * 2.0
    + MISSION_REPORT_ROUTE_CENTER_WIDTH
    + MISSION_REPORT_ROUTE_GAP * 4.0;
const MISSION_REPORT_STRENGTH_BAR_WIDTH: f32 = 140.0;
const MISSION_REPORT_FOOTER_RIGHT_INSET: f32 = 24.0;
const MISSION_REPORT_FOOTER_BOTTOM_INSET: f32 = 18.0;
const MISSION_REPORT_LOOT_TOP_GAP: f32 = 20.0;
const MISSION_REPORT_PLANET_INTEL_WIDTH: f32 = 138.0;
const MISSION_REPORT_INTEL_COLUMN_GAP: f32 = 4.0;
const MISSION_REPORT_INTEL_IMAGE_SIZE: f32 = (MISSION_REPORT_PLANET_INTEL_WIDTH
    - MISSION_REPORT_INTEL_COLUMN_GAP * (MISSION_REPORT_INTEL_COLUMNS - 1) as f32)
    / MISSION_REPORT_INTEL_COLUMNS as f32;
const MISSION_REPORT_INTEL_COLUMNS: usize = 3;
const MISSION_REPORT_INTEL_GROUP_GAP: f32 = 7.0;
const JOINT_ATTACK_ROSTER_NAME_SIZE: f32 = 15.0;
const JOINT_ATTACK_ROSTER_STATUS_SIZE: f32 = 13.0;
const JOINT_ATTACK_ROSTER_NAME_GAP: f32 = 12.0;
const JOINT_ATTACK_ROSTER_FLEET_ICON_SIZE: f32 = 14.0;
const JOINT_ATTACK_ROSTER_STRENGTH_GAP: f32 = 4.0;
const JOINT_ATTACK_ROSTER_BADGE_GAP: f32 = 12.0;
const JOINT_ATTACK_PROPOSAL_NOTICE_SECONDS: u64 = 3;
const FLEET_MOVEMENT_TOOLTIP: &str = "Distance the fleet will travel next turn. Fleets accelerate \
    each travel turn, so this is not a fixed per-turn speed.";
const JUMP_GATE_MOVEMENT_TOOLTIP: &str =
    "A Jump Gate bypasses normal fleet movement and delivers the fleet in one turn.";
const PATROL_FUEL_TOOLTIP: &str = "The first amount is the one-time deuterium cost to reach the \
    patrol zone. The amount after + is the deuterium consumed per turn after arrival. The fleet \
    returns automatically when there is not enough deuterium for another patrol turn.";
const PATROL_DETECTION_TOOLTIP: &str = "Maximum detection probability per turn for the selected \
    ships, assuming full exposure through the center of the patrol zone. The actual chance \
    depends on the fraction of the enemy's route inside the circle: crossing its edge or \
    spending only part of the turn inside gives a lower chance. Detection starts after arrival.";
const PATROL_REINFORCEMENT_TOOLTIP: &str = "Ships merge into your patrol on arrival, adopting \
    its orders and return destination. Probes can reinforce alone, but return if no combat patrol \
    remains. Reinforcements with combat ships can start a new patrol at the same location.";

/// Matches mission-planning tooltip text to the small labels in the panel.
trait MissionTooltip {
    fn on_hover_mission(self, text: impl Into<RichText>) -> Self;
    fn on_disabled_hover_mission(self, text: impl Into<RichText>) -> Self;
}

impl MissionTooltip for Response {
    fn on_hover_mission(self, text: impl Into<RichText>) -> Self {
        self.on_hover_ui(|ui| {
            ui.label(text.into().small());
        })
    }

    fn on_disabled_hover_mission(self, text: impl Into<RichText>) -> Self {
        self.on_disabled_hover_ui(|ui| {
            ui.label(text.into().small());
        })
    }
}

/// Keeps the cover option tied to a completed origin relay without revealing target levels.
fn draw_deep_cover_option(ui: &mut Ui, mission: &mut Mission, origin: &Planet) {
    let available =
        mission.objective == Icon::Spy && origin.has(&Unit::Building(Building::CommandRelay));
    if !available {
        mission.deep_cover = false;
    }
    if mission.objective != Icon::Spy {
        return;
    }
    let response = ui
        .add_enabled_ui(available, |ui| {
            ui.horizontal(|ui| {
                let label = ui
                    .add(
                        egui::Label::new(RichText::new("Deep Cover:").small())
                            .sense(Sense::click())
                            .selectable(false),
                    )
                    .on_hover_cursor(CursorIcon::PointingHand);
                if label.clicked() {
                    mission.deep_cover = !mission.deep_cover;
                    set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
                }
                label | ui.add(toggle(&mut mission.deep_cover))
            })
            .inner
        })
        .inner;
    response
        .on_hover_mission(format!(
            "Costs {} extra deuterium per Probe. If the origin's completed Command Relay level is \
        higher than the destination's when the mission arrives, all Probes scan and return \
        without combat or alerting the defender. Otherwise, the Probes face normal Spy combat. \
        Deep Cover missions never encounter space fauna.",
            crate::core::constants::DEEP_COVER_DEUTERIUM_PER_PROBE,
        ))
        .on_disabled_hover_mission("Build a Command Relay at the origin to enable Deep Cover.");
}

/// Keeps mission-planning and active-mission movement explanations in agreement.
pub(super) const fn mission_movement_tooltip(jump_gate: bool) -> &'static str {
    if jump_gate {
        JUMP_GATE_MOVEMENT_TOOLTIP
    } else {
        FLEET_MOVEMENT_TOOLTIP
    }
}

/// Keeps the displayed arrival turn and its explanatory tooltip in agreement.
pub(super) fn mission_arrival_turn(current_turn: usize, duration: usize) -> usize {
    current_turn.saturating_add(duration)
}

pub(super) fn mission_arrival_tooltip(current_turn: usize, duration: usize) -> String {
    format!("The fleet will arrive at turn {}.", mission_arrival_turn(current_turn, duration))
}

/// Returns whether an objective can be selected for the current origin and destination.
fn mission_objective_available(
    objective: Icon,
    origin_army: &Army,
    origin: PlanetId,
    destination: &Planet,
    n_owned: usize,
    n_max_owned: usize,
) -> bool {
    mission_objective_unavailable_reason(
        objective,
        origin_army,
        origin,
        destination,
        n_owned,
        n_max_owned,
    )
    .is_none()
}

/// Reports an actual blocker, prioritizing target restrictions that more ships cannot fix.
fn mission_objective_unavailable_reason(
    objective: Icon,
    origin_army: &Army,
    origin: PlanetId,
    destination: &Planet,
    n_owned: usize,
    n_max_owned: usize,
) -> Option<&'static str> {
    // Space drafts fall back to the origin when no usable friendly destination exists.
    if objective == Icon::Deploy && destination.id == origin {
        return Some("No planets to deploy to.");
    }
    if destination.is_moon() && objective.on_planet_only() {
        return Some(match objective {
            Icon::Raid => "Moons cannot be raided.",
            Icon::Colonize => "Moons cannot be colonized.",
            Icon::MissileStrike => "Interplanetary Missiles cannot target moons.",
            _ => "This objective cannot target moons.",
        });
    }
    if objective == Icon::Colonize && n_owned >= n_max_owned {
        return Some("Maximum number of colonized planets reached.");
    }
    if !objective.condition_for_army(origin_army) {
        return Some(objective.requirement());
    }
    None
}

/// Only hostile fleet objectives can be shared with invited players.
const fn mission_objective_supports_allied_attack(objective: Icon) -> bool {
    matches!(objective, Icon::Colonize | Icon::Attack | Icon::Destroy)
}

const fn mission_objective_selectable(
    objective: Icon,
    available: bool,
    allied_objectives_only: bool,
) -> bool {
    available && (!allied_objectives_only || mission_objective_supports_allied_attack(objective))
}

/// Keeps objective artwork steady while hover, press, and selection have separate cues.
fn draw_mission_objective_button(
    ui: &mut Ui,
    image: egui::TextureId,
    objective: Icon,
    selected: bool,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(MISSION_OBJECTIVE_BUTTON_SIZE, Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::RadioButton,
            ui.is_enabled(),
            selected,
            objective.to_name(),
        )
    });
    if ui.is_rect_visible(rect) {
        let enabled = ui.is_enabled();
        let pressed = enabled && response.is_pointer_button_down_on();
        let hover = ui.ctx().animate_bool_with_time(
            response.id.with("objective hover"),
            enabled && (response.hovered() || response.has_focus()),
            0.10,
        );
        let frame = egui::Rect::from_min_size(rect.min, egui::Vec2::splat(44.0));
        if hover > 0.0 {
            ui.painter().rect(
                frame,
                5.0,
                Color32::from_rgba_unmultiplied(
                    231,
                    184,
                    89,
                    (hover
                        * if pressed {
                            34.0
                        } else {
                            18.0
                        }) as u8,
                ),
                Stroke::new(1.0, MISSION_OBJECTIVE_ACCENT.gamma_multiply(hover * 0.7)),
                egui::StrokeKind::Inside,
            );
        }
        let image_rect = frame.shrink(2.0).translate(egui::vec2(
            0.0,
            if pressed {
                1.0
            } else {
                0.0
            },
        ));
        ui.painter().image(
            image,
            image_rect,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            if enabled {
                Color32::WHITE
            } else {
                Color32::from_gray(125)
            },
        );
        if selected {
            ui.painter().rect_filled(
                egui::Rect::from_center_size(
                    rect.center_bottom() - egui::vec2(0.0, 1.0),
                    egui::vec2(22.0, 2.0),
                ),
                1.0,
                MISSION_OBJECTIVE_ACCENT,
            );
        }
    }
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Sizes and centers the active-mission row while preserving equal outer breathing room.
fn mission_row_layout(available_width: f32) -> (f32, f32) {
    let centered_row_width = (available_width - 2.0 * MISSION_ROW_HORIZONTAL_INSET).max(0.0);
    let route_column_width =
        (centered_row_width - 2.0 * MISSION_PLANET_COLUMN_WIDTH - 2.0 * MISSION_COLUMN_GAP)
            .clamp(MISSION_ROUTE_COLUMN_MIN_WIDTH, MISSION_ROUTE_COLUMN_MAX_WIDTH);
    let row_width =
        2.0 * MISSION_PLANET_COLUMN_WIDTH + 2.0 * MISSION_COLUMN_GAP + route_column_width;
    let leading_space = ((available_width - row_width) * 0.5).max(0.0);

    (route_column_width, leading_space)
}

/// Active missions always need at least the next turn to finish, including a recall at home.
fn mission_display_turns(mission: &Mission, map: &Map) -> usize {
    mission.turns_to_destination(map).max(1)
}

/// Summarizes a private fleet without exposing its exact ship composition to co-attackers.
pub(super) fn fleet_strength(army: &Army) -> usize {
    army.iter().fold(0_usize, |strength, (unit, count)| {
        if unit.is_ship() {
            strength.saturating_add(count.saturating_mul(unit.production()))
        } else {
            strength
        }
    })
}

/// Keep fleets from accepted joint missions reserved while planning another mission.
pub(super) fn mission_origin_army_after_allied_reservations(
    map: &Map,
    session: &MultiplayerSession,
    player_id: PlayerId,
    origin_id: PlanetId,
    excluded_attack_id: Option<u64>,
) -> Army {
    let mut available = map
        .try_get(origin_id)
        .and_then(|origin| origin.mission_origin_army(player_id))
        .cloned()
        .unwrap_or_default();
    for reserved in session
        .joint_attacks
        .iter()
        .filter(|invitation| {
            !invitation.canceled
                && !invitation.launched
                && Some(invitation.id) != excluded_attack_id
        })
        .flat_map(|invitation| &invitation.participants)
        .filter(|participant| {
            participant.player_id == player_id
                && participant.response == JointAttackResponse::Accepted
        })
        .filter_map(|participant| participant.contribution.as_ref())
        .filter(|contribution| contribution.origin == origin_id)
    {
        for (unit, count) in &reserved.army {
            if let Some(remaining) = available.get_mut(unit) {
                *remaining = remaining.saturating_sub(*count);
            }
        }
    }
    available.retain(|_, count| *count > 0);
    available
}

fn joint_origin_has_ships(
    map: &Map,
    session: &MultiplayerSession,
    player_id: PlayerId,
    origin_id: PlanetId,
    attack_id: u64,
) -> bool {
    let available = mission_origin_army_after_allied_reservations(
        map,
        session,
        player_id,
        origin_id,
        Some(attack_id),
    );
    Unit::ships().iter().any(|unit| available.amount(unit) > 0)
}

/// Returns whether the local player should be offered the active-mission recall action.
fn mission_recall_available(mission: &Mission, map: &Map, player_id: PlayerId) -> bool {
    mission.owner == player_id
        && mission.joint_attack.is_none()
        && !mission.is_returning()
        && mission.objective.is_recallable()
        && !mission.recall_blocked_by_revoked_protection(map, player_id)
}

/// Paints the compact turn-back action used to recall an active mission.
fn draw_recall_button(ui: &mut Ui, images: &ImageIds, editable: bool) -> Response {
    let sense = if editable {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) =
        ui.allocate_exact_size(egui::Vec2::splat(MISSION_RECALL_BUTTON_SIZE), sense);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            editable && ui.is_enabled(),
            "Recall mission",
        )
    });
    let image = if editable && response.hovered() && !response.is_pointer_button_down_on() {
        images.get("recall hover")
    } else {
        images.get("recall")
    };
    // Preserve the icon's transparent exterior over both panel artwork and the map.
    ui.painter().image(
        image,
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );

    if editable {
        response.on_hover_cursor(CursorIcon::PointingHand)
    } else {
        response
    }
}

/// Returns marker centers on one continuous spacing grid, independent of the preview width.
fn route_marker_positions(
    left: f32,
    right: f32,
    spacing: f32,
    phase: f32,
) -> impl Iterator<Item = f32> {
    let (first, count) = if right >= left && spacing > 0.0 {
        let first = left + phase.rem_euclid(spacing);
        let count = (((right - first) / spacing).floor() as isize + 1).max(0) as usize;
        (first, count)
    } else {
        (left, 0)
    };

    (0..count).map(move |index| first + index as f32 * spacing)
}

/// Keeps a return leg on the same left-to-right planet layout as its outbound leg.
fn mission_route_presentation(mission: &Mission) -> (PlanetId, PlanetId, bool) {
    let returning = mission.is_returning();
    if returning {
        (mission.destination, mission.origin, true)
    } else {
        (mission.origin, mission.destination, false)
    }
}

/// Builds a route chevron that faces in the mission's displayed travel direction.
fn route_chevron(center: egui::Pos2, returning: bool) -> [[egui::Pos2; 2]; 2] {
    let direction = if returning {
        -1.0
    } else {
        1.0
    };
    let tail_x = center.x - 4.0 * direction;
    let tip_x = center.x + 2.0 * direction;

    [
        [egui::pos2(tail_x, center.y - 5.0), egui::pos2(tip_x, center.y)],
        [egui::pos2(tip_x, center.y), egui::pos2(tail_x, center.y + 5.0)],
    ]
}

/// Keeps both strands at a fixed pitch as the preview lane grows or shrinks.
fn jump_gate_preview_strands(
    left: f32,
    right: f32,
    center_y: f32,
    elapsed: f32,
    returning: bool,
) -> [Vec<egui::Pos2>; 2] {
    let direction = if returning {
        -1.0
    } else {
        1.0
    };
    let phase = elapsed * JUMP_GATE_HELIX_ANGULAR_SPEED * direction;
    let mut strands = [Vec::new(), Vec::new()];
    for x in route_marker_positions(left, right, 2.0, 0.0) {
        let angle = (x - left) / MISSION_PREVIEW_HELIX_WAVELENGTH * std::f32::consts::TAU - phase;
        let fade = ((x - left).min(right - x) / 10.0).clamp(0.0, 1.0);
        let offset = angle.sin() * 7.5 * fade;
        strands[0].push(egui::pos2(x, center_y + offset));
        strands[1].push(egui::pos2(x, center_y - offset));
    }
    strands
}

/// Shows continuous local maneuvers without suggesting progress toward another planet.
fn draw_patrol_preview(painter: &egui::Painter, rect: egui::Rect, time: f32, color: Color32) {
    let position = |angle: f32| {
        rect.center()
            + egui::vec2(angle.sin() * (rect.width() * 0.5 - 10.0), (angle * 2.0).sin() * 10.0)
    };
    let orbit = (0..=80).map(|step| position(step as f32 / 80.0 * std::f32::consts::TAU)).collect();
    painter.add(egui::Shape::line(orbit, Stroke::new(1.0, color.gamma_multiply(0.22))));

    let phase = time * 0.9;
    for step in 0..18 {
        let angle = phase - (18 - step) as f32 * 0.035;
        painter.line_segment(
            [position(angle), position(angle + 0.035)],
            Stroke::new(1.8, color.gamma_multiply((step + 1) as f32 / 18.0)),
        );
    }
    let head = position(phase);
    painter.circle_filled(head, 5.0, color.gamma_multiply(0.15));
    painter.circle_filled(head, 2.8, color);
    painter.circle_filled(head, 1.1, Color32::WHITE);
}

/// Draws the same compact route language used by hovered missions on the strategic map.
fn draw_route_preview(
    ui: &mut Ui,
    width: f32,
    mission: &Mission,
    map: &Map,
    player: &Player,
    color: Color32,
    returning: bool,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, MISSION_ROUTE_PREVIEW_HEIGHT), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            "Locate mission on map",
        )
    });
    let painter = ui.painter().with_clip_rect(rect);
    // Keep the helix and chevrons inside the preview lane.
    let left = rect.left() + 12.0;
    let right = rect.right() - 8.0;
    let center_y = rect.center().y;
    let time = ui.input(|input| input.time) as f32;
    if mission.is_patrolling() {
        draw_patrol_preview(&painter, rect, time, color);
        return response;
    }
    let speed = mission.route_animation_speed(map) as f32;
    let phase = time
        * speed
        * if returning {
            -1.0
        } else {
            1.0
        };

    let route_style = mission.route_style(player);
    painter.line_segment(
        [egui::pos2(left, center_y), egui::pos2(right, center_y)],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(143, 158, 174, 52)),
    );

    match route_style {
        MissionRouteStyle::Standard => {
            for x in route_marker_positions(left, right, 27.0, phase) {
                let marker = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 220);
                for segment in route_chevron(egui::pos2(x, center_y), returning) {
                    painter.line_segment(segment, Stroke::new(2.0, marker));
                }
            }
        },
        MissionRouteStyle::JumpGate => {
            let strands = jump_gate_preview_strands(left, right, center_y, time, returning);
            let colors = [
                color.lerp_to_gamma(Color32::from_rgb(64, 235, 255), 0.55),
                color.lerp_to_gamma(Color32::WHITE, 0.28),
            ];
            for (strand, color) in strands.into_iter().zip(colors) {
                painter.add(egui::Shape::line(strand, Stroke::new(1.4, color)));
            }
        },
    }

    response
}

/// Draws a faction-tinted mission sprite centered in a stable layout slot.
fn draw_mission_image(
    ui: &mut Ui,
    image: egui::TextureId,
    size: f32,
    slot_size: f32,
    offset: egui::Vec2,
    color: Color32,
) -> Response {
    let (slot, response) = ui.allocate_exact_size(egui::Vec2::splat(slot_size), Sense::hover());
    let image_rect = egui::Rect::from_center_size(slot.center() + offset, egui::Vec2::splat(size));
    ui.place(
        image_rect,
        egui::Image::new(SizedTexture::new(image, egui::Vec2::splat(size)))
            .fit_to_exact_size(egui::Vec2::splat(size))
            .tint(color),
    );

    response
}

/// Balances broad mission artwork within the report panel's fixed thumbnail slot.
fn mission_report_image_size(image: &str, size: f32) -> f32 {
    match image {
        "mission" => size * MISSION_FLEET_IMAGE_SCALE,
        "mission colonize" => size * MISSION_COLONY_IMAGE_SCALE,
        "mission spy" => size * MISSION_SPY_IMAGE_SCALE,
        _ => size,
    }
}

/// Optically centers asymmetric mission artwork without moving adjacent report columns.
fn mission_report_image_offset(image: &str) -> egui::Vec2 {
    match image {
        "mission missile" => egui::vec2(MISSION_MISSILE_IMAGE_OFFSET_X, 0.0),
        _ => egui::Vec2::ZERO,
    }
}

/// Keeps report thumbnails stable under the pointer while retaining selected-row emphasis.
fn mission_report_image_base_size(selected: bool) -> f32 {
    if selected {
        MISSION_REPORT_SELECTED_IMAGE_SIZE
    } else {
        MISSION_REPORT_IMAGE_SIZE
    }
}

/// Places a mission-row planet and its name on one shared layout for both route endpoints.
fn mission_planet_rects(cell: egui::Rect) -> (egui::Rect, egui::Rect) {
    let image = egui::Rect::from_center_size(
        egui::pos2(cell.center().x, cell.top() + MISSION_PLANET_IMAGE_SIZE * 0.5),
        egui::Vec2::splat(MISSION_PLANET_IMAGE_SIZE),
    );
    let name = egui::Rect::from_min_size(
        egui::pos2(cell.left(), image.bottom() - MISSION_PLANET_NAME_OVERLAP),
        egui::vec2(cell.width(), MISSION_PLANET_NAME_HEIGHT),
    );

    (image, name)
}

/// Aligns the route lane with the planet artwork rather than the taller planet-and-name cell.
fn mission_route_rect(cell: egui::Rect) -> egui::Rect {
    egui::Rect::from_center_size(
        egui::pos2(cell.center().x, cell.top() + MISSION_PLANET_IMAGE_SIZE * 0.5),
        egui::vec2(cell.width(), MISSION_ROUTE_PREVIEW_HEIGHT),
    )
}

/// Reserves one grid cell; drawing its contents must not advance the parent grid again.
fn mission_route_cell(ui: &mut Ui, width: f32) -> (Ui, Response) {
    let (cell, response) =
        ui.allocate_exact_size(egui::vec2(width, MISSION_PLANET_CELL_HEIGHT), Sense::hover());
    let child = ui.new_child(UiBuilder::new().max_rect(mission_route_rect(cell)));
    (child, response)
}

/// Matches the plan and recall meshes so pixel snapping cannot shift an icon off the route center.
fn draw_mission_route_icon(ui: &mut Ui, image: egui::TextureId) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::Vec2::splat(MISSION_ROUTE_ICON_SIZE), Sense::hover());
    ui.painter().image(
        image,
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    response
}

/// Consumes clicks over passive enemy route content without passing them to the map.
///
/// The animation has its own navigation action; the surrounding objective and arrival time
/// retain the default cursor and only block the pickable strategic map behind the panel.
fn block_enemy_route_clicks(ui: &mut Ui, response: Response, mission_id: MissionId) -> Response {
    ui.interact(
        response.rect,
        response.id.with(("enemy mission route", mission_id)),
        Sense::click(),
    )
    .on_hover_cursor(CursorIcon::Default)
}

/// Draws one active-mission planet link without letting overlay widgets shift its name.
fn draw_mission_planet_link(
    ui: &mut Ui,
    image: egui::TextureId,
    name: &str,
    sense: Sense,
) -> (Response, Response) {
    let (cell, _) = ui.allocate_exact_size(
        egui::vec2(MISSION_PLANET_COLUMN_WIDTH, MISSION_PLANET_CELL_HEIGHT),
        Sense::hover(),
    );
    let (image_rect, name_rect) = mission_planet_rects(cell);
    let image_response = ui
        .place(
            image_rect,
            egui::Image::new(SizedTexture::new(
                image,
                egui::Vec2::splat(MISSION_PLANET_IMAGE_SIZE),
            )),
        )
        .interact(sense);
    image_response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    let name = egui::WidgetText::from(RichText::new(name).text_style(TextStyle::Small))
        .into_galley(ui, Some(egui::TextWrapMode::Truncate), name_rect.width(), TextStyle::Small);
    let name_pos = name_rect.center() - name.size() * 0.5;
    ui.painter().galley(name_pos, name, ui.visuals().text_color());
    let name_response = ui.interact(name_rect, ui.next_auto_id(), sense);
    name_response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Select planet")
    });

    (image_response, name_response)
}

/// Applies the shared click, secondary-click, and hover behavior for mission planet links.
fn handle_mission_planet_link(
    image: &Response,
    name: &Response,
    planet: &Planet,
    changed_hover: &mut bool,
    state: &mut UiState,
    map: &Map,
    player: &Player,
) {
    if image.clicked() || name.clicked() {
        set_ui_sound(&image.ctx, Some(SoundEffect::Button));
        state.planet_hover = None;
        state.mission_planet_hover = None;
        state.planet_selected = Some(planet.id);
        state.focus_planet = None;
        state.focus_position = None;
        state.focus_zoom = None;
        state.to_selected = true;
        state.mission = false;
        if planet.can_launch_mission(player.id) {
            state.mission_info.origin = planet.id;
        }
    } else if (image.secondary_clicked() || name.secondary_clicked()) && !planet.is_destroyed {
        state.mission_tab = MissionTab::NewMission;
        state.mission_info.origin = state
            .planet_selected
            .filter(|&selected| map.get(selected).can_launch_mission(player.id))
            .unwrap_or(player.home_planet);
        state.mission_info.destination = planet.id;
    } else if image.hovered() || name.hovered() {
        state.planet_hover = None;
        state.mission_planet_hover = Some(planet.id);
        *changed_hover = true;
    }
}

/// Returns the administration whose identity the current report is allowed to disclose.
fn mission_report_administration(report: &MissionReport, player_id: PlayerId) -> Unit {
    let senate = Unit::Building(Building::Senate);
    if mission_report_unit_is_visible(report, player_id, &Side::Defender, &senate)
        && report.planet.army.combined_amount(&senate) > 0
    {
        senate
    } else {
        Unit::Building(Building::ColonialAdministration)
    }
}

/// Returns the prominent and compact unit groups shown in a planet mission report.
fn mission_report_planet_intel(shows_senate: bool) -> ([Unit; 2], Vec<Unit>, Vec<Unit>) {
    let critical = [Unit::planetary_shield(), Unit::space_dock()];
    let orbitals =
        Unit::orbitals().into_iter().filter(|unit| *unit != Unit::space_dock()).collect();
    let buildings = Unit::buildings_for_world(false, shows_senate)
        .into_iter()
        .filter(|unit| *unit != Unit::planetary_shield())
        .collect();

    (critical, orbitals, buildings)
}

/// Draws one compact intelligence group inside the final two defender columns.
fn draw_mission_report_intel_grid(
    ui: &mut Ui,
    name: &str,
    units: &[Unit],
    report: &MissionReport,
    player: &Player,
    images: &ImageIds,
) {
    egui::Grid::new(name)
        .striped(false)
        .num_columns(MISSION_REPORT_INTEL_COLUMNS)
        .spacing([MISSION_REPORT_INTEL_COLUMN_GAP, 4.0])
        .show(ui, |ui| {
            for (index, unit) in units.iter().enumerate() {
                draw_mission_report_unit(
                    ui,
                    unit,
                    report,
                    player,
                    Side::Defender,
                    (MISSION_REPORT_INTEL_IMAGE_SIZE, TextStyle::Small),
                    images,
                );

                if (index + 1) % MISSION_REPORT_INTEL_COLUMNS == 0 {
                    ui.end_row();
                }
            }
        });
}

/// Keeps strategic defenses prominent while fitting all planet intelligence in two columns.
fn draw_mission_report_planet_intel(
    ui: &mut Ui,
    report: &MissionReport,
    player: &Player,
    images: &ImageIds,
) {
    let administration = mission_report_administration(report, player.id);
    let (critical, orbitals, buildings) =
        mission_report_planet_intel(administration == Unit::Building(Building::Senate));

    ui.vertical(|ui| {
        ui.set_width(MISSION_REPORT_PLANET_INTEL_WIDTH);
        ui.spacing_mut().item_spacing.y = 0.0;

        draw_army_grid(ui, "defender_critical", &critical, report, player, images);

        ui.add_space(MISSION_REPORT_INTEL_GROUP_GAP);
        draw_mission_report_intel_grid(ui, "defender_orbitals", &orbitals, report, player, images);

        ui.add_space(MISSION_REPORT_INTEL_GROUP_GAP);
        draw_mission_report_intel_grid(
            ui,
            "defender_buildings",
            &buildings,
            report,
            player,
            images,
        );
    });
}

/// Overlays the mission log badge without advancing the surrounding grid cursor.
fn draw_mission_log_badge(
    ui: &mut Ui,
    image: egui::TextureId,
    planet_rect: egui::Rect,
) -> Response {
    let size = egui::Vec2::splat(MISSION_LOG_BADGE_SIZE);
    let rect = egui::Rect::from_min_size(
        planet_rect.right_top() - egui::vec2(MISSION_LOG_BADGE_SIZE + 5.0, -5.0),
        size,
    );

    ui.place(rect, egui::Image::new(SizedTexture::new(image, size)))
}

/// Returns whether the draft can currently use a jump gate for this route and fleet.
fn jump_gate_route_available(
    mission: &Mission,
    origin: &Planet,
    destination: &Planet,
    player: &Player,
) -> bool {
    matches!(mission.objective, Icon::Deploy | Icon::Protect)
        && player.owns(origin)
        && (player.owns(destination)
            || (mission.objective == Icon::Protect
                && mission.protected_player == destination.controlled
                && destination.allows_protection(player.id)))
        && origin.has(&Unit::Building(Building::JumpGate))
        && destination.has(&Unit::Building(Building::JumpGate))
        && mission.jump_cost() <= origin.max_jump_capacity().saturating_sub(origin.jump_gate)
}

/// Applies the remembered toggle only while the selected route can actually use a jump gate.
fn sync_jump_gate_selection(
    mission: &mut Mission,
    origin: &Planet,
    destination: &Planet,
    player: &Player,
    remembered: bool,
) {
    mission.jump_gate =
        remembered && jump_gate_route_available(mission, origin, destination, player);
}

/// Draws the mission tabs as one group centered within the panel's available width.
fn draw_mission_tabs(ui: &mut Ui, selected: &mut MissionTab) -> egui::Rect {
    let tabs = MissionTab::iter().collect::<Vec<_>>();
    let button_padding = egui::vec2(6.0, 0.0);

    let text_width = tabs
        .iter()
        .map(|tab| {
            egui::WidgetText::from(tab.to_title())
                .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, TextStyle::Body)
                .size()
                .x
        })
        .sum::<f32>();
    let tab_row_width = text_width
        + 2.0 * button_padding.x * tabs.len() as f32
        + ui.spacing().item_spacing.x * tabs.len().saturating_sub(1) as f32;
    let leading_space = ((ui.available_width() - tab_row_width) * 0.5).max(0.0);

    ui.horizontal(|ui| {
        ui.style_mut().spacing.button_padding = button_padding;
        ui.add_space(leading_space);
        let mut tab_row = None;

        for tab in tabs {
            let response = ui.selectable_value(selected, tab, tab.to_title());
            tab_row =
                Some(tab_row.map_or(response.rect, |rect: egui::Rect| rect.union(response.rect)));
        }

        tab_row.unwrap_or(egui::Rect::NOTHING)
    })
    .inner
}

fn proposal_fleet_valid(mission: &Mission, available: &Army) -> bool {
    mission.objective.accepts_army(&mission.army)
        && mission.army.iter().all(|(unit, count)| *count <= available.amount(unit))
}

/// Creates the initial invitation and publishes later edits only on Send proposal.
fn sync_allied_mission(
    state: &mut UiState,
    turn: usize,
    player: &Player,
    session: &MultiplayerSession,
    requests: &mut MessageWriter<MultiplayerRequest>,
    invitation: Option<&JointAttackInvitation>,
    publish_available: Option<&Army>,
) -> bool {
    let eligible =
        matches!(state.mission_info.objective, Icon::Colonize | Icon::Attack | Icon::Destroy);
    if !session.has_active_game() {
        return true;
    }
    if !session.joint_attacks_enabled() {
        state.joint_attack_invitees.clear();
        state.joint_attack_invite_panel_rect = None;
        state.allied_mission = false;
        state.joint_attack_draft_id = None;
        state.joint_attack_owner_draft = None;
        state.joint_attack_owner_shared_route = None;
        state.joint_attack_owner_withdrawal = None;
        return true;
    }
    if !eligible && invitation.is_some() {
        // An unsupported local objective is still just an unsent draft. Keep the
        // invitation intact until its owner either chooses a shared objective or cancels.
        return false;
    }
    if let Some(invitation) = invitation {
        state.joint_attack_invitees.extend(
            invitation.participants.iter().skip(1).map(|participant| participant.player_id),
        );
    }
    if !state.allied_mission || state.joint_attack_invitees.is_empty() {
        if let Some(invitation) = invitation {
            if !session.joint_attack_update_pending {
                requests.write(MultiplayerRequest::CancelJointAttack {
                    attack_id: invitation.id,
                });
                // Keep the draft linked until the canceled response arrives. Otherwise the
                // still-current invitation is restored on the next UI pass.
                return false;
            }
        }
        return !state.allied_mission && !session.joint_attack_update_pending;
    }
    let contribution = JointAttackContribution {
        player_id: player.id,
        origin: state.mission_info.origin,
        army: state.mission_info.army.clone(),
        bombing: state.mission_info.bombing.clone(),
        combat_probes: state.mission_info.combat_probes,
        battle_plan: state.mission_info.battle_plan,
    };
    let invitees_synced = invitation.is_some_and(|invitation| {
        invitation
            .participants
            .iter()
            .skip(1)
            .map(|item| item.player_id)
            .eq(state.joint_attack_invitees.iter().copied())
    });
    let synced = invitation.is_some_and(|invitation| {
        invitation.destination == state.mission_info.destination
            && invitation.objective == state.mission_info.objective
            && invitation.participants.first().and_then(|item| item.contribution.as_ref())
                == Some(&contribution)
            && invitees_synced
    });
    if !synced
        && publish_available
            .is_some_and(|available| proposal_fleet_valid(&state.mission_info, available))
        && !session.joint_attack_update_pending
    {
        let attack_id = invitation
            .map_or_else(|| (rand::random::<u64>() & i64::MAX as u64).max(1), |item| item.id);
        let mut participants = vec![JointAttackParticipant {
            player_id: player.id,
            response: JointAttackResponse::Accepted,
            contribution: Some(contribution),
        }];
        participants.extend(state.joint_attack_invitees.iter().map(|id| JointAttackParticipant {
            player_id: *id,
            response: JointAttackResponse::Pending,
            contribution: None,
        }));
        requests.write(MultiplayerRequest::CreateJointAttack(JointAttackInvitation {
            id: attack_id,
            revision: invitation.map_or(0, |item| item.revision),
            turn: turn as u64,
            inviter: player.id,
            destination: state.mission_info.destination,
            objective: state.mission_info.objective,
            bombing: state.mission_info.bombing.clone(),
            combat_probes: state.mission_info.combat_probes,
            canceled: false,
            launched: false,
            participants,
        }));
        state.joint_attack_draft_id = Some(attack_id);
    }
    synced && !session.joint_attack_update_pending
}

fn allied_synced_for_owner(state: &UiState, invitation: Option<&JointAttackInvitation>) -> bool {
    invitation.is_some_and(|invitation| {
        invitation.destination == state.mission_info.destination
            && invitation.objective == state.mission_info.objective
            && invitation
                .participants
                .first()
                .and_then(|item| item.contribution.as_ref())
                .is_some_and(|contribution| {
                    contribution.player_id == invitation.inviter
                        && contribution.origin == state.mission_info.origin
                        && contribution.army == state.mission_info.army
                        && contribution.bombing == state.mission_info.bombing
                        && contribution.combat_probes == state.mission_info.combat_probes
                        && contribution.battle_plan == state.mission_info.battle_plan
                })
            && invitation
                .participants
                .iter()
                .skip(1)
                .map(|item| item.player_id)
                .eq(state.joint_attack_invitees.iter().copied())
    })
}

/// Opens the picker from the invite icon and keeps it open across the icon/panel gap.
/// A row click updates the mission immediately.
fn draw_joint_attack_invite_picker(
    context: &egui::Context,
    state: &mut UiState,
    session: &MultiplayerSession,
    player: &Player,
    icon_rect: egui::Rect,
    editable: bool,
) -> bool {
    let Some(game) = &session.active_game else {
        state.joint_attack_invite_panel_rect = None;
        return false;
    };
    let members = game
        .members
        .iter()
        .filter(|member| {
            member.player_id != player.id
                && game
                    .persisted
                    .state
                    .player(member.player_id)
                    .is_ok_and(|player| !player.spectator)
        })
        .collect::<Vec<_>>();
    let viewport = context.content_rect();
    let width = 224.0_f32.min((viewport.right() - icon_rect.right() - 12.0).max(0.0));
    let height =
        (42.0 + members.len() as f32 * 39.0).max(65.0).min((viewport.height() - 24.0).max(0.0));
    if width < 100.0 || height < 50.0 {
        state.joint_attack_invite_panel_rect = None;
        return false;
    }
    let pos = egui::pos2(
        icon_rect.right() + 2.0,
        (icon_rect.bottom() - height - 18.0)
            .clamp(viewport.top() + 12.0, viewport.bottom() - height - 12.0),
    );
    let current_pass = context.cumulative_pass_nr();
    let hovering = context.input(|input| {
        input.pointer.hover_pos().is_some_and(|pointer| {
            icon_rect.contains(pointer)
                || state.joint_attack_invite_panel_rect.is_some_and(|(pass, panel)| {
                    let bridge = egui::Rect::from_min_max(
                        icon_rect.right_top(),
                        egui::pos2(panel.left(), icon_rect.bottom()),
                    );
                    pass.saturating_add(1) == current_pass
                        && (panel.contains(pointer) || bridge.contains(pointer))
                })
        })
    });
    if !hovering {
        state.joint_attack_invite_panel_rect = None;
        return false;
    }
    let enabled = editable;
    // The first published proposal fixes its roster. Before then, every local
    // selection is only a draft and can be toggled freely.
    let invitees_revocable = state.joint_attack_draft_id.is_none();
    let mut changed = false;
    let panel = egui::Area::new(egui::Id::new("joint attack invite players"))
        .fixed_pos(pos)
        .movable(false)
        .order(Order::Foreground)
        .show(context, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgb(15, 22, 29))
                .stroke(Stroke::new(1.0, Color32::from_rgb(65, 87, 106)))
                .corner_radius(7.0)
                .inner_margin(egui::Margin::symmetric(10, 9))
                .show(ui, |ui| {
                    ui.set_min_width(width - 20.0);
                    ui.set_max_width(width - 20.0);
                    ui.label(
                        RichText::new("Invite players").strong().size(HOVER_TOOLTIP_TEXT_SIZE),
                    );
                    ui.add_space(6.0);
                    ScrollArea::vertical()
                        .id_salt("joint attack invite players list")
                        .max_height((height - 42.0).max(0.0))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.y = 5.0;
                            if members.is_empty() {
                                ui.small("No players available.");
                            }
                            for member in &members {
                                let invited =
                                    state.joint_attack_invitees.contains(&member.player_id);
                                let row_width = ui.available_width();
                                let response = modal_player_row(
                                    ui,
                                    row_width,
                                    34.0,
                                    &member.display_name,
                                    session.player_color(member.player_id).color().to_color32(),
                                    invited,
                                    enabled && (!invited || invitees_revocable),
                                );
                                let response = if invited {
                                    response.on_hover_mission(if invitees_revocable {
                                        "Click to remove this player from the draft."
                                    } else {
                                        "Invited players cannot be removed after sending a proposal."
                                    })
                                } else {
                                    response
                                };
                                if response.clicked() {
                                    if invited {
                                        state.joint_attack_invitees.remove(&member.player_id);
                                    } else {
                                        state.joint_attack_invitees.insert(member.player_id);
                                    }
                                    changed = true;
                                }
                            }
                        });
                });
        });
    state.joint_attack_invite_panel_rect = Some((current_pass, panel.response.rect));
    state.allied_mission = !state.joint_attack_invitees.is_empty();
    changed
}

/// Keeps a space draft's Deploy alternative on a friendly world.
fn draft_planet_destination(state: &UiState, map: &Map, player: &Player) -> PlanetId {
    if state.mission_info.objective == Icon::Intercept {
        return state
            .intercept_previous_destination
            .and_then(|id| map.try_get(id))
            .filter(|planet| !planet.is_destroyed && planet.id != state.mission_info.origin)
            .or_else(|| {
                map.planets
                    .iter()
                    .find(|planet| !planet.is_destroyed && planet.id != state.mission_info.origin)
            })
            .map_or(state.mission_info.origin, |planet| planet.id);
    }
    if state.mission_info.objective == Icon::Patrol {
        state
            .patrol_previous_destination
            .and_then(|id| map.try_get(id))
            .filter(|planet| {
                !planet.is_destroyed
                    && planet.id != state.mission_info.origin
                    && (player.owns(planet) || player.controls(planet))
            })
            .or_else(|| {
                map.planets.iter().find(|planet| {
                    !planet.is_destroyed
                        && planet.id != state.mission_info.origin
                        && (player.owns(planet) || player.controls(planet))
                })
            })
            .map_or(state.mission_info.origin, |planet| planet.id)
    } else {
        state.mission_info.destination
    }
}

/// Space drafts keep navigation choices together; hostile actions stay with enemy targets.
fn draft_objectives(player: &Player, destination: &Planet, space_patrol: bool) -> Vec<Icon> {
    if space_patrol {
        return vec![Icon::Deploy, Icon::Patrol];
    }
    let mut objectives = Icon::objectives(
        player.owns(destination),
        player.controls(destination),
        destination.allows_protection(player.id),
        destination.blocks_hostile_action_by(player.id),
    );
    if let Some(index) = objectives.iter().position(|icon| *icon == Icon::Deploy) {
        objectives.insert(index + 1, Icon::Patrol);
    }
    if let Some(index) = objectives.iter().position(|icon| *icon == Icon::Destroy) {
        objectives.insert(index, Icon::Intercept);
    }
    objectives
}

/// Retains an existing intercept draft without adding it to friendly planet objectives.
fn draft_objectives_for_mission(
    player: &Player,
    destination: &Planet,
    mission: &Mission,
) -> Vec<Icon> {
    let mut objectives = draft_objectives(player, destination, mission.objective == Icon::Patrol);
    if mission.objective == Icon::Intercept && !objectives.contains(&Icon::Intercept) {
        objectives.push(Icon::Intercept);
    }
    objectives
}

#[path = "interception.rs"]
pub(super) mod interception;

fn draw_new_mission(
    ui: &mut Ui,
    send_mission: &mut MessageWriter<SendMissionMsg>,
    missions: &[Mission],
    settings: &Settings,
    state: &mut UiState,
    map: &mut Map,
    player: &mut Player,
    session: &MultiplayerSession,
    multiplayer_requests: &mut MessageWriter<MultiplayerRequest>,
    is_hovered: bool,
    keyboard: &ButtonInput<KeyCode>,
    images: &ImageIds,
) {
    // Keep the footer inside the original panel even when a wide route or fleet needs scrolling.
    let viewport = ui
        .ctx()
        .layer_transform_from_global(ui.layer_id())
        .unwrap_or(egui::emath::TSTransform::IDENTITY)
        .mul_rect(ui.ctx().content_rect());
    let panel_rect = ui.max_rect().intersect(viewport);
    let joint_attacks_enabled = session.joint_attacks_enabled();
    if state.joint_attack_draft_id.is_some() {
        if let Some(draft) = &state.joint_attack_owner_draft {
            state.mission_info = draft.clone();
        }
    }
    let active_invitation = joint_attacks_enabled
        .then_some(state.joint_attack_draft_id)
        .flatten()
        .and_then(|id| {
            session.joint_attacks.iter().find(|invitation| {
                invitation.id == id && !invitation.canceled && !invitation.launched
            })
        })
        .cloned();
    if active_invitation.as_ref().is_none_or(|invitation| {
        invitation
            .participants
            .first()
            .is_none_or(|owner| owner.response != JointAttackResponse::Accepted)
    }) {
        state.joint_attack_owner_withdrawal = None;
    }
    if let Some(invitation) = &active_invitation {
        let shared_route = (invitation.destination, invitation.objective);
        if state.joint_attack_owner_shared_route != Some(shared_route) {
            if state.joint_attack_owner_shared_route.is_none_or(|previous| {
                (state.mission_info.destination, state.mission_info.objective) == previous
            }) {
                state.mission_info.destination = invitation.destination;
                state.mission_info.objective = invitation.objective;
            }
            state.joint_attack_owner_shared_route = Some(shared_route);
        }
    }
    if state.joint_attack_draft_id.is_some()
        && active_invitation.is_none()
        && !session.joint_attack_update_pending
    {
        state.joint_attack_draft_id = None;
        state.joint_attack_invitees.clear();
        state.allied_mission = false;
        state.joint_attack_owner_shared_route = None;
    }
    if !map
        .try_get(state.mission_info.origin)
        .is_some_and(|planet| planet.can_launch_mission(player.id))
    {
        state.mission_info.origin = player.home_planet;
    }
    if !matches!(state.mission_info.objective, Icon::Patrol | Icon::Intercept)
        && state.mission_info.origin == state.mission_info.destination
    {
        if let Some(destination) = map.planets.iter().find(|planet| {
            !planet.is_destroyed
                && planet.id != state.mission_info.origin
                && (state.mission_info.objective != Icon::Deploy
                    || player.owns(planet)
                    || player.controls(planet))
        }) {
            state.mission_info.destination = destination.id;
        }
    }
    let origin = map.get(state.mission_info.origin);
    let destination = map.get(draft_planet_destination(state, map, player));
    super::battle_plan::initialize_mission_plan(
        &mut state.mission_info,
        origin,
        player.id,
        &mut state.mission_battle_plan_origin,
    );
    let origin_army = mission_origin_army_after_allied_reservations(
        map,
        session,
        player.id,
        origin.id,
        active_invitation.as_ref().map(|invitation| invitation.id),
    );

    let (n_owned, n_max_owned) = player.planets_owned(map, settings);

    // Block selection of any unit when in spectator mode to be unable to send missions
    if player.spectator {
        state.mission_info.army = Army::new();
    }

    // Recalculate position (in case origin changed)
    state.mission_info =
        Mission::from_mission(settings.turn, player.id, origin, destination, &state.mission_info);

    if state.mission_info.objective == Icon::Colonize && n_owned >= n_max_owned {
        state.mission_info.objective = Icon::Deploy;
    }

    let objectives = draft_objectives_for_mission(player, destination, &state.mission_info);
    if !objectives.contains(&state.mission_info.objective) {
        state.mission_info.objective = objectives.first().copied().unwrap_or_default();
    }

    // Keep an invited Protect draft selected even when its origin has no ships left.
    // Opening a gate shortcut must not silently turn it into a hostile mission.
    if !mission_objective_available(
        state.mission_info.objective,
        &origin_army,
        origin.id,
        destination,
        n_owned,
        n_max_owned,
    ) && !matches!(state.mission_info.objective, Icon::Protect | Icon::Patrol | Icon::Intercept)
    {
        state.mission_info.objective = objectives
            .iter()
            .copied()
            .find(|objective| {
                mission_objective_available(
                    *objective,
                    &origin_army,
                    origin.id,
                    destination,
                    n_owned,
                    n_max_owned,
                )
            })
            .unwrap_or_else(|| objectives.first().copied().unwrap_or_default());
    }

    // Normalize route state before choosing the center icon. The toggle preference deliberately
    // survives between drafts, but an ineligible route must never inherit its visual state.
    sync_jump_gate_selection(
        &mut state.mission_info,
        origin,
        destination,
        player,
        state.jump_gate_history,
    );

    let army = match state.mission_info.objective {
        Icon::MissileStrike => vec![Unit::interplanetary_missile()],
        Icon::Spy => vec![Unit::probe()],
        _ => Unit::ships(),
    };

    ui.add_space(10.);

    ScrollArea::horizontal()
        .id_salt("new mission route")
        .max_width(panel_rect.width())
        .auto_shrink([false, true])
        .show(ui, |ui| {
            ui.horizontal_top(|ui| {
                ui.add_space(((panel_rect.width() - 580.0) * 0.5).max(8.0));

                let action = |r: Response, planet: &Planet, h: &mut bool, state: &mut UiState| {
                    if r.clicked() {
                        set_ui_sound(&r.ctx, Some(SoundEffect::Button));
                        state.planet_hover = None;
                        state.mission_planet_hover = None;
                        state.planet_selected = Some(planet.id);
                        state.to_selected = true;
                        state.mission = false;
                        if planet.can_launch_mission(player.id) {
                            state.mission_info.origin = planet.id;
                        }
                    } else if r.secondary_clicked() && !planet.is_destroyed {
                        state.mission_tab = MissionTab::NewMission;
                        state.mission_info.destination = planet.id;
                    } else if r.hovered() {
                        state.planet_hover = None;
                        state.mission_planet_hover = Some(planet.id);
                        *h = true;
                    }
                };

                let mut changed_hover = false;
                let route_responses = draw_mission_route(
                    ui,
                    &mut state.mission_info,
                    map,
                    player,
                    missions,
                    session,
                    &origin_army,
                    &army,
                    images,
                    false,
                    state.intercept_target,
                );
                for (index, (response, planet)) in
                    route_responses.into_iter().zip([origin, destination]).enumerate()
                {
                    if let Some(response) = response {
                        if index == 1
                            && matches!(
                                state.mission_info.objective,
                                Icon::Patrol | Icon::Intercept
                            )
                        {
                            if response.clicked() {
                                if state.mission_info.objective == Icon::Intercept {
                                    state.begin_intercept_selection();
                                } else {
                                    state.begin_patrol_selection();
                                }
                            }
                        } else {
                            action(response, planet, &mut changed_hover, state);
                        }
                    }
                }

                // If not hovering anything, reset hover selection
                if is_hovered && !changed_hover {
                    state.planet_hover = None;
                    state.mission_planet_hover = None;
                }
            });
        });

    ui.add_space(-10.);
    ui.add(Separator::default().shrink(70.));

    // The origin selector runs inside the route row. Refresh its fleet before drawing the
    // cards below, so changing worlds does not leave the previous world's counts for a pass.
    let route_changed = origin.id != state.mission_info.origin
        || destination.id != draft_planet_destination(state, map, player);
    let origin = map.get(state.mission_info.origin);
    let destination = map.get(draft_planet_destination(state, map, player));
    super::battle_plan::initialize_mission_plan(
        &mut state.mission_info,
        origin,
        player.id,
        &mut state.mission_battle_plan_origin,
    );
    if route_changed {
        state.mission_info = Mission::from_mission(
            settings.turn,
            player.id,
            origin,
            destination,
            &state.mission_info,
        );
        sync_jump_gate_selection(
            &mut state.mission_info,
            origin,
            destination,
            player,
            state.jump_gate_history,
        );
    }
    let origin_army = mission_origin_army_after_allied_reservations(
        map,
        session,
        player.id,
        origin.id,
        active_invitation.as_ref().map(|invitation| invitation.id),
    );

    let mut invite_icon_rect = None;
    let mut proposal_sent = false;
    {
        let body_width = panel_rect.width();
        let can_cancel_mission =
            active_invitation.is_some() || !state.joint_attack_invitees.is_empty();
        let badges_width = mission_badges_width(
            ui,
            &state.mission_info.army,
            (state.mission_info.objective != Icon::MissileStrike)
                .then_some(state.mission_info.battle_plan),
        );
        let footer = mission_footer_layout(
            panel_rect.shrink(10.0),
            if joint_attacks_enabled {
                MISSION_INVITE_ICON_SIZE + 12.0 + MISSION_PLAYERS_SUMMARY_WIDTH
            } else {
                0.0
            },
            badges_width,
            can_cancel_mission,
        );
        let footer_icon_rect = egui::Rect::from_min_size(
            footer.players.left_center() - egui::vec2(0.0, MISSION_INVITE_ICON_SIZE * 0.5),
            egui::Vec2::splat(MISSION_INVITE_ICON_SIZE),
        );
        let mut editing_fleet = false;
        ScrollArea::both()
            .id_salt("mission fleet and details")
            .max_width(body_width)
            .max_height((footer.top - ui.cursor().top() - 20.0).max(0.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(body_width);
                ui.horizontal_top(|ui| {
                    ui.add_space(130.);

                    ui.vertical(|ui| {
                        ui.set_width(280.);

                        editing_fleet = draw_mission_fleet_picker(
                            ui,
                            &mut state.mission_info.army,
                            &origin_army,
                            &army,
                            images,
                        );
                    });

                    ui.add_space(15.);

                    ui.vertical(|ui| {
                        ui.set_width(330.);

                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing.x = MISSION_OBJECTIVE_BUTTON_GAP;

                            let on_hover =
                                |ui: &mut Ui, icon: &Icon, disabled_reason: Option<&str>| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.add_image(
                                            images.get(format!("{} cover", icon.asset_key())),
                                            [150., 150.],
                                        );
                                    });
                                    ui.vertical(|ui| {
                                        ui.label(icon.to_name());
                                        if disabled_reason.is_some() || !settings.brief_hover_info {
                                            ui.separator();
                                        }

                                        if let Some(disabled_reason) = disabled_reason {
                                            ui.colored_label(
                                                Color32::RED,
                                                RichText::new(disabled_reason).small(),
                                            );
                                        }

                                        if !settings.brief_hover_info
                                            && (disabled_reason.is_none() || *icon != Icon::Deploy)
                                        {
                                            ui.small(icon.description());
                                        }
                                    });
                                });
                            };

                            let allied_objectives_only = state.allied_mission
                                || !state.joint_attack_invitees.is_empty()
                                || active_invitation.is_some();

                            for icon in draft_objectives_for_mission(player, destination, &state.mission_info) {
                                let unavailable_reason = mission_objective_unavailable_reason(
                                    icon,
                                    &origin_army,
                                    origin.id,
                                    destination,
                                    n_owned,
                                    n_max_owned,
                                );
                                let unavailable_reason = if icon == Icon::Patrol && origin_army.amount(&Unit::probe()) > 0 && state.mission_info.reinforces_patrol(missions) {
                                    None
                                } else if icon == Icon::Intercept && state.mission_info.objective != Icon::Intercept && !missions.iter().any(|target| crate::core::interception::target_available(target, map, player)) {
                                    Some("No enemy mission can currently be intercepted.")
                                } else { unavailable_reason };
                                let available = unavailable_reason.is_none();
                                let allied_compatible = !allied_objectives_only
                                    || mission_objective_supports_allied_attack(icon);
                                let disabled_reason = if !allied_compatible {
                                    Some("Send or cancel the allied mission before choosing this objective.")
                                } else {
                                    unavailable_reason
                                };
                                ui.add_enabled_ui(
                                    mission_objective_selectable(
                                        icon,
                                        available,
                                        allied_objectives_only,
                                    ),
                                    |ui| {
                                        let button = draw_mission_objective_button(
                                                ui,
                                                images.get(icon.asset_key()),
                                                icon,
                                                state.mission_info.objective == icon,
                                            )
                                            .on_hover_ui(|ui| on_hover(ui, &icon, None))
                                            .on_disabled_hover_ui(|ui| {
                                                on_hover(ui, &icon, disabled_reason);
                                            })
                                            .on_hover_cursor(CursorIcon::PointingHand);

                                        if button.clicked() {
                                            match icon {
                                                Icon::Spy => {
                                                    state.mission_info.army.retain(|u, _| {
                                                        matches!(u, Unit::Ship(Ship::Probe))
                                                    })
                                                },
                                                Icon::MissileStrike => {
                                                    state.mission_info.army.retain(|u, _| {
                                                        matches!(
                                                            u,
                                                            Unit::Defense(
                                                                Defense::InterplanetaryMissile
                                                            )
                                                        )
                                                    })
                                                },
                                                _ => {
                                                    state.mission_info.army.remove(&Unit::Defense(
                                                        Defense::InterplanetaryMissile,
                                                    ));
                                                },
                                            }

                                            if icon == Icon::Intercept {
                                                state.begin_intercept_selection();
                                            } else if icon == Icon::Patrol {
                                                state.begin_patrol_selection();
                                            } else {
                                                state.mission_info.patrol = None;
                                                state.mission_info.intercept = None;
                                                state.mission_info.destination = destination.id;
                                                state.mission_info.objective = icon;
                                            }
                                        }
                                    },
                                );
                            }
                        });

                        ui.add_space(5.);

                        interception::refresh_preview(state, missions, map, player, settings.turn);
                        draw_mission_details(
                            ui,
                            &mut state.mission_info,
                            map,
                            player,
                            settings.turn,
                            images,
                            active_invitation.as_ref(),
                        );

                        if matches!(state.mission_info.objective, Icon::Deploy | Icon::Protect) {
                            if player.owns(origin)
                                && (player.owns(destination)
                                    || (state.mission_info.objective == Icon::Protect
                                        && state.mission_info.protected_player
                                            == destination.controlled
                                        && destination.allows_protection(player.id)))
                                && origin.has(&Unit::Building(Building::JumpGate))
                                && destination.has(&Unit::Building(Building::JumpGate))
                            {
                                let jump_cost = state.mission_info.jump_cost();
                                let can_jump = jump_gate_route_available(
                                    &state.mission_info,
                                    origin,
                                    destination,
                                    player,
                                );
                                sync_jump_gate_selection(
                                    &mut state.mission_info,
                                    origin,
                                    destination,
                                    player,
                                    state.jump_gate_history,
                                );

                                ui.horizontal(|ui| {
                                    ui.small(format!(
                                        "🌀 Jump Gate ({}/{}):",
                                        jump_cost,
                                        origin.max_jump_capacity() - origin.jump_gate
                                    ));
                                    if ui
                                        .add_enabled(
                                            can_jump,
                                            toggle(&mut state.mission_info.jump_gate),
                                        )
                                        .clicked()
                                    {
                                        state.jump_gate_history = !state.jump_gate_history;
                                    }
                                    draw_jump_energy_cost(ui, &state.mission_info, images);
                                })
                                .response
                                .on_hover_mission(
                                    "Whether to send this mission through the Jump Gate. Missions \
                                through the Jump Gate always take 1 turn and cost no fuel. The \
                                army's total production can't surpass the Gate's limit. Each jump \
                                uses 1 Energy per 5 production sent, rounded up. Unused gates consume no Energy.",
                                );
                            } else {
                                state.mission_info.jump_gate = false;
                            }
                        } else {
                            state.mission_info.jump_gate = false;
                        }
                    });
                });
            });

        let allied_synced = sync_allied_mission(
            state,
            settings.turn,
            player,
            session,
            multiplayer_requests,
            active_invitation.as_ref(),
            None,
        );
        if joint_attacks_enabled {
            let eligible = matches!(
                state.mission_info.objective,
                Icon::Colonize | Icon::Attack | Icon::Destroy,
            );
            ui.add_enabled_ui(eligible, |ui| {
                let icon_rect = footer_icon_rect;
                if eligible {
                    invite_icon_rect = Some(icon_rect);
                }
                let response = ui
                    .interact(icon_rect, ui.id().with("invite players"), Sense::hover())
                    .on_hover_cursor(CursorIcon::Default)
                    .on_disabled_hover_mission(
                        "Players can join Attack, Colonize, and Destroy missions.",
                    );
                response.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::WidgetType::Button,
                        response.enabled(),
                        "Invite players",
                    )
                });
                let tint = if response.hovered() && !response.is_pointer_button_down_on() {
                    Color32::WHITE
                } else {
                    Color32::from_gray(200)
                };
                ui.painter().image(
                    images.get("allied attack"),
                    icon_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    tint,
                );
                if state.allied_mission {
                    draw_joint_attack_player_summary(
                        ui,
                        egui::Rect::from_min_max(
                            egui::pos2(icon_rect.right() + 12.0, footer.players.top()),
                            footer.players.right_bottom(),
                        ),
                        active_invitation.as_ref(),
                        Some(&state.joint_attack_invitees),
                        session,
                        player.id,
                        images,
                    );
                }
            });
        }
        ui.scope(|ui| {

            let intercept_reason = interception::unavailable_reason(state, missions, map, player);
            let army_check = state.mission_info.army.has_army();
            let fuel_check = player.resources.deuterium
                >= state.mission_info.dispatch_fuel_consumption(map, player);
            let validation = crate::core::orders::validate_mission_with_patrols(
                player, map, origin, destination, &state.mission_info, missions,
            );
            let objective_check = validation.is_ok();
            let proposal_fleet_check =
                !state.allied_mission || proposal_fleet_valid(&state.mission_info, &origin_army);
            // A guest's revised fleet marks the owner pending, but sending the mission is the
            // owner's final acceptance of every currently accepted guest contribution. Only
            // the owner's own unsent edits require another proposal.
            let update_offer =
                state.allied_mission && (active_invitation.is_none() || !allied_synced);
            let shareable_objective = matches!(
                state.mission_info.objective,
                Icon::Colonize | Icon::Attack | Icon::Destroy
            );

            ui.scope_builder(UiBuilder::new().max_rect(footer.send), |ui| {
                ui.add_enabled_ui(
                    intercept_reason.is_none()
                        && army_check
                        && fuel_check
                        && objective_check
                        && proposal_fleet_check
                        && (active_invitation.is_none() || shareable_objective)
                        && (active_invitation.is_none() || !session.joint_attack_update_pending)
                        && (allied_synced || (update_offer && !session.joint_attack_update_pending)),
                    |ui| {
                        let response = ui
                            .add_custom_button(if update_offer { "Send proposal" } else { "Send mission" }, images)
                            .on_disabled_hover_ui(|ui| {
                                if let Some(reason) = intercept_reason {
                                    ui.small(reason);
                                } else if !army_check {
                                    ui.small("No ships selected for the mission.");
                                } else if !fuel_check {
                                    ui.small("Not enough fuel (deuterium) for the mission.");
                                } else if let Err(error) = &validation {
                                    ui.small(error.to_string());
                                } else if !proposal_fleet_check {
                                    ui.small("Select ships available at this origin for the allied mission.");
                                } else if !shareable_objective {
                                    ui.small("Only Colonize, Attack, and Destroy can be shared with invited players.");
                                } else if !allied_synced {
                                    ui.small("Invite players and wait for the current mission to be shared.");
                                }
                            });

                        if response.clicked()
                            || (response.enabled() && !editing_fleet
                                && keyboard.just_pressed(KeyCode::Enter))
                        {
                            proposal_sent = true;
                            if update_offer {
                                if allied_synced {
                                    if let Some(invitation) = &active_invitation {
                                        multiplayer_requests.write(MultiplayerRequest::RespondJointAttack {
                                            expected_revision: invitation.revision,
                                            attack_id: invitation.id,
                                            response: JointAttackResponse::Accepted,
                                            contribution: Some(JointAttackContribution {
                                                player_id: player.id,
                                                origin: state.mission_info.origin,
                                                army: state.mission_info.army.clone(),
                                                bombing: state.mission_info.bombing.clone(),
                                                combat_probes: state.mission_info.combat_probes,
                                                battle_plan: state.mission_info.battle_plan,
                                            }),
                                        });
                                    }
                                } else {
                                    sync_allied_mission(
                                        state, settings.turn, player, session,
                                        multiplayer_requests, active_invitation.as_ref(),
                                        Some(&origin_army),
                                    );
                                }
                                state.joint_attack_proposal_notice = true;
                            } else {
                            let mut mission = Mission::from_mission(
                                settings.turn,
                                player.id,
                                origin,
                                destination,
                                &state.mission_info,
                            );

                            let accepted = active_invitation.as_ref().map(|invitation| {
                                invitation
                                    .participants
                                    .iter()
                                    .filter(|participant| {
                                        participant.player_id == invitation.inviter
                                            || participant.response == JointAttackResponse::Accepted
                                    })
                                    .filter_map(|participant| participant.contribution.clone())
                                    .collect::<Vec<_>>()
                            });
                            if let (Some(invitation), Some(contributions)) = (
                                active_invitation.as_ref(),
                                accepted.filter(|items| items.len() > 1),
                            ) {
                                // The shared invitation also identifies its lead fleet so other
                                // participants can project a published launch before resolution.
                                mission.id = invitation.id;
                                let arrival_turn =
                                    contributions.iter().fold(settings.turn, |latest, item| {
                                        let route = Mission::new_with_id(
                                            invitation.id,
                                            settings.turn,
                                            item.player_id,
                                            map.get(item.origin),
                                            destination,
                                            invitation.objective,
                                            item.army.clone(),
                                            item.bombing.clone(),
                                            item.combat_probes,
                                            false,
                                            None,
                                        );
                                        latest
                                            .max(settings.turn.saturating_add(route.duration(map)))
                                    });
                                mission.joint_attack = Some(JointAttackMission {
                                    id: invitation.id,
                                    leader: player.id,
                                    arrival_turn,
                                    attackers: contributions
                                        .iter()
                                        .map(|item| (item.player_id, item.army.clone()))
                                        .collect(),
                                    combat_orders: contributions
                                        .iter()
                                        .map(|item| {
                                            (
                                                item.player_id,
                                                crate::core::missions::FleetCombatOrders {
                                                    bombing: item.bombing.clone(),
                                                    combat_probes: item.combat_probes,
                                                    battle_plan: item.battle_plan,
                                                },
                                            )
                                        })
                                        .collect(),
                                    survivors: std::collections::BTreeMap::new(),
                                    scouts: std::collections::BTreeMap::new(),
                                    origins: contributions
                                        .iter()
                                        .map(|item| (item.player_id, item.origin))
                                        .collect(),
                                });
                                send_mission.write(SendMissionMsg::joint(
                                    mission,
                                    JointMissionLaunch {
                                        attack_id: invitation.id,
                                        contributions,
                                    },
                                ));
                            } else {
                                send_mission.write(if let Some(invitation) = &active_invitation {
                                    SendMissionMsg::solo_after_cancel(mission, invitation.id)
                                } else {
                                    SendMissionMsg::new(mission)
                                });
                            }
                            // The accepted command emits its launch cue in send_mission.
                            set_ui_sound(ui.ctx(), None);
                            state.planet_selected = None;
                            state.mission = false;
                            state.mission_info = Mission::default();
                            state.mission_battle_plan_origin = None;
                            state.joint_attack_invitees.clear();
                            state.joint_attack_draft_id = None;
                            state.allied_mission = false;
                            }
                        }
                    },
                );
            });
            if let Some(cancel) = footer.secondary {
                let cancel_response = ui
                    .scope_builder(UiBuilder::new().max_rect(cancel), |ui| {
                        ui.add_enabled_ui(
                            active_invitation.is_none() || !session.joint_attack_update_pending,
                            |ui| ui.add_custom_button("Cancel mission", images),
                        )
                        .inner
                    })
                    .inner;
                if cancel_response.clicked() {
                    proposal_sent = true;
                    if let Some(invitation) = &active_invitation {
                        multiplayer_requests.write(MultiplayerRequest::CancelJointAttack {
                            attack_id: invitation.id,
                        });
                    }
                    state.joint_attack_draft_id = None;
                    state.joint_attack_owner_draft = None;
                    state.joint_attack_owner_shared_route = None;
                    state.joint_attack_owner_withdrawal = None;
                    state.joint_attack_invitees.clear();
                    state.joint_attack_invite_panel_rect = None;
                    state.allied_mission = false;
                    state.mission = false;
                    state.mission_info = Mission::default();
                    state.mission_battle_plan_origin = None;
                }
            }
            let fleet_icon = draw_mission_strength_badge(
                ui,
                &state.mission_info.army,
                images,
                footer.badges.center().y,
                footer.badges.right(),
            );
            if state.mission_info.objective != Icon::MissileStrike {
                draw_mission_plan_badge(
                    ui,
                    images,
                    fleet_icon,
                    &mut state.mission_info.battle_plan,
                    &state.mission_info.army,
                );
            }
        });
    }
    state.joint_attack_owner_draft =
        state.joint_attack_draft_id.map(|_| state.mission_info.clone());
    let response_open = state.joint_attack_open.is_some_and(|id| {
        session.joint_attacks.iter().any(|invitation| {
            invitation.id == id
                && invitation.inviter != player.id
                && !invitation.canceled
                && !invitation.launched
        })
    });
    if response_open {
        // An invitee's response modal has no invite action. Do not let the owner panel's
        // hover picker appear above that modal when both panels are in the same UI pass.
        state.joint_attack_invite_panel_rect = None;
    } else if let Some(icon_rect) = invite_icon_rect {
        // The mission window can be scaled, while pointer positions and the popup
        // area are in screen coordinates.
        let icon_rect = ui
            .ctx()
            .layer_transform_to_global(ui.layer_id())
            .unwrap_or(egui::emath::TSTransform::IDENTITY)
            .mul_rect(icon_rect);
        draw_joint_attack_invite_picker(
            ui.ctx(),
            state,
            session,
            player,
            icon_rect,
            ui.is_enabled(),
        );
    } else {
        state.joint_attack_invite_panel_rect = None;
    }
    if !proposal_sent
        && state.allied_mission
        && !allied_synced_for_owner(state, active_invitation.as_ref())
        && !session.joint_attack_update_pending
    {
        if let Some(invitation) = &active_invitation {
            if let Some(owner) = invitation
                .participants
                .first()
                .filter(|item| item.response == JointAttackResponse::Accepted)
            {
                let key = (invitation.id, invitation.revision);
                if state.joint_attack_owner_withdrawal != Some(key) {
                    multiplayer_requests.write(MultiplayerRequest::RespondJointAttack {
                        expected_revision: invitation.revision,
                        attack_id: invitation.id,
                        response: JointAttackResponse::Pending,
                        contribution: owner.contribution.clone(),
                    });
                    state.joint_attack_owner_withdrawal = Some(key);
                }
            }
        }
    }
}

/// Builds a planet label, optionally colored by the visible controller or owner affiliation.
fn mission_planet_affiliation_color(
    map: &Map,
    player: &Player,
    missions: &[Mission],
    session: &MultiplayerSession,
    planet: &Planet,
) -> Option<Color32> {
    visible_planet_affiliation(map, player, missions, planet)
        .map(|player_id| session.player_color(player_id).color().to_color32())
}

fn mission_planet_selector_label(
    ui: &Ui,
    map: &Map,
    player: &Player,
    missions: &[Mission],
    session: &MultiplayerSession,
    planet: &Planet,
    color_by_affiliation: bool,
) -> egui::text::LayoutJob {
    let mut label = egui::text::LayoutJob::default();
    let font_id = TextStyle::Button.resolve(ui.style());
    let name_color = if color_by_affiliation {
        mission_planet_affiliation_color(map, player, missions, session, planet)
            .unwrap_or_else(|| ui.visuals().text_color())
    } else {
        Color32::WHITE
    };
    label.append(
        &planet.name,
        0.0,
        egui::text::TextFormat {
            font_id,
            color: name_color,
            ..Default::default()
        },
    );
    label
}

fn mission_planet_selector_popup_style(style: &mut egui::Style) {
    // Keep the scrollbar beside the border while preserving the text and vertical padding.
    style.spacing.menu_margin.right = 2;
}

/// Shared planet selectors and fleet shortcut.
fn draw_mission_route(
    ui: &mut Ui,
    mission: &mut Mission,
    map: &Map,
    player: &Player,
    missions: &[Mission],
    session: &MultiplayerSession,
    available: &Army,
    units: &[Unit],
    images: &ImageIds,
    fixed_destination: bool,
    intercept_target: Option<MissionId>,
) -> [Option<Response>; 2] {
    let origin = map.get(mission.origin);
    let destination = map.get(mission.destination);
    let reinforcing = mission.reinforces_patrol(missions);
    let mut origin_response = None;
    let mut destination_response = None;
    let mut origin_selector_rect = egui::Rect::NOTHING;
    let destination_width = if mission.objective == Icon::Intercept {
        interception::TARGET_LABEL_WIDTH
    } else {
        100.0
    };
    egui::Grid::new("mission_origin_destination").spacing([30., 0.]).striped(false).show(
        ui,
        |ui| {
            let response = ui.cell(70., |ui| {
                ui.add_image(images.get(origin.image()), [60.; 2])
                    .interact(Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand)
            });

            origin_response = Some(response);

            ui.cell(100., |ui| {
                ui.vertical(|ui| {
                    if mission.objective == Icon::Intercept {
                        ui.set_width(ui.spacing().combo_width);
                    }
                    let selected_color = mission_planet_affiliation_color(
                        map,
                        player,
                        missions,
                        session,
                        map.get(mission.origin),
                    );
                    style_selection_boxes(ui, selected_color);
                    ui.add_space(15.);

                    let origins = map
                        .planets
                        .iter()
                        .filter(|planet| planet.can_launch_mission(player.id))
                        .sorted_by(|a, b| a.name.cmp(&b.name))
                        .collect::<Vec<_>>();

                    let selector = ComboBox::from_id_salt("origin")
                        .wrap_mode(if mission.objective == Icon::Intercept {
                            egui::TextWrapMode::Truncate
                        } else {
                            ui.wrap_mode()
                        })
                        .popup_style(mission_planet_selector_popup_style.into())
                        .height(60. * origins.len().max(5) as f32)
                        .selected_text(mission_planet_selector_label(
                            ui,
                            map,
                            player,
                            missions,
                            session,
                            map.get(mission.origin),
                            true,
                        ))
                        .show_ui(ui, |ui| {
                            let visuals = ui.visuals_mut();
                            visuals.selection.bg_fill = Color32::from_rgb(57, 66, 82);
                            visuals.selection.stroke = Stroke::new(
                                1.0,
                                selected_color.unwrap_or(Color32::from_rgb(190, 198, 210)),
                            );
                            for planet in origins {
                                let label = mission_planet_selector_label(
                                    ui, map, player, missions, session, planet, true,
                                );
                                if ui
                                    .selectable_value(&mut mission.origin, planet.id, label)
                                    .on_hover_cursor(CursorIcon::PointingHand)
                                    .changed()
                                {
                                    mission.battle_plan = planet.battle_plan_for(Some(player.id));
                                }
                            }
                        })
                        .response
                        .on_hover_cursor(CursorIcon::PointingHand);
                    origin_selector_rect = selector.rect;
                });
            });

            let (rect, mut response) =
                ui.cell(50., |ui| ui.allocate_exact_size([50.; 2].into(), Sense::click()));

            response = response.on_hover_cursor(CursorIcon::PointingHand).on_hover_mission(
                "Click to select all units on the origin planet. Right-click to unselect all.",
            );

            let image_rect = if response.hovered() && !response.is_pointer_button_down_on() {
                rect.expand(3.0)
            } else {
                rect
            };
            ui.painter().image(
                images.get(mission.image(player)),
                image_rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                player.color().color().to_color32(),
            );

            if response.clicked() {
                mission.army = units.iter().map(|unit| (*unit, available.amount(unit))).collect();
                set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
            } else if response.secondary_clicked() {
                mission.army.clear();
            }

            ui.add_enabled_ui(!fixed_destination, |ui| {
                ui.cell(destination_width, |ui| {
                    if mission.objective == Icon::Intercept {
                        interception::draw_target_label(
                            ui, intercept_target, missions, map, player, session,
                            origin_selector_rect.center().y,
                        );
                        return;
                    }
                    if mission.objective == Icon::Patrol {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(100.0, 70.0), Sense::hover());
                        // Match the origin's actual text center, including selector padding.
                        ui.put(
                            egui::Rect::from_center_size(
                                egui::pos2(rect.center().x, origin_selector_rect.center().y),
                                egui::vec2(rect.width(), origin_selector_rect.height()),
                            ),
                            egui::Label::new(
                                RichText::new(if reinforcing { "Reinforce" } else { "Deep space" })
                                    .text_style(TextStyle::Button),
                            )
                            .extend()
                            .halign(Align::Center),
                        )
                        .on_hover_mission(if reinforcing {
                            PATROL_REINFORCEMENT_TOOLTIP
                        } else {
                            "Choose a space destination, or select your active patrol to reinforce it."
                        });
                        return;
                    }
                    ui.vertical(|ui| {
                        let selected_color = mission_planet_affiliation_color(
                            map,
                            player,
                            missions,
                            session,
                            map.get(mission.destination),
                        );
                        style_selection_boxes(ui, selected_color);
                        ui.add_space(15.);
                        ComboBox::from_id_salt("destination")
                            .popup_style(mission_planet_selector_popup_style.into())
                            .selected_text(mission_planet_selector_label(
                                ui,
                                map,
                                player,
                                missions,
                                session,
                                map.get(mission.destination),
                                true,
                            ))
                            .show_ui(ui, |ui| {
                                let visuals = ui.visuals_mut();
                                visuals.selection.bg_fill = Color32::from_rgb(57, 66, 82);
                                visuals.selection.stroke = Stroke::new(
                                    1.0,
                                    selected_color.unwrap_or(Color32::from_rgb(190, 198, 210)),
                                );
                                for planet in map
                                    .planets
                                    .iter()
                                    .filter(|p| !p.is_destroyed)
                                    .filter(|p| p.id != mission.origin)
                                    .sorted_by(|a, b| a.name.cmp(&b.name))
                                {
                                    let label = mission_planet_selector_label(
                                        ui, map, player, missions, session, planet, true,
                                    );
                                    ui.selectable_value(&mut mission.destination, planet.id, label)
                                        .on_hover_cursor(CursorIcon::PointingHand);
                                }
                            })
                            .response
                            .on_hover_cursor(CursorIcon::PointingHand);
                    });
                });
            });
            let response = ui.cell(70., |ui| {
                if mission.objective == Icon::Intercept {
                    return ui.add_image(images.get("deep space"), [60.; 2])
                        .interact(Sense::click())
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_mission("Select an enemy mission on the map.");
                }
                let image = ui.add_image(
                    images.get(if mission.objective == Icon::Patrol {
                        "deep space".to_owned()
                    } else {
                        destination.image()
                    }),
                    [60.; 2],
                );
                if fixed_destination {
                    image.interact(Sense::hover())
                } else {
                    image.interact(Sense::click()).on_hover_cursor(CursorIcon::PointingHand)
                }
            });

            destination_response = Some(response);
        },
    );
    [origin_response, destination_response]
}

/// Fleet cards shared by mission drafts and joint-attack responses.
/// Returns whether a count is being edited, including the frame that confirms it.
pub(super) fn draw_mission_fleet_picker(
    ui: &mut Ui,
    selected: &mut Army,
    available: &Army,
    units: &[Unit],
    images: &ImageIds,
) -> bool {
    let mut editing = false;
    egui::Grid::new("units").striped(false).num_columns(2).spacing([25., 8.]).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 8.;

        for (i, unit) in units.iter().enumerate() {
            let n = available.amount(unit);
            let count = selected.entry(*unit).or_default();
            *count = (*count).min(n);

            ui.add_enabled_ui(n > 0, |ui| {
                ui.vertical_centered(|ui| {
                    ui.horizontal(|ui| {
                        ui.set_width(110.);

                        let response = ui
                            .add_image(images.get(unit.to_lowername()), [65., 65.])
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_hover_mission(unit.to_name())
                            .on_disabled_hover_mission(unit.to_name());

                        if response.clicked() {
                            *selected.entry(*unit).or_insert(0) = n;
                            set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
                        }

                        if response.secondary_clicked() {
                            *selected.entry(*unit).or_insert(0) = 0;
                        }

                        ui.add_text_on_image(
                            n.to_string(),
                            Color32::WHITE,
                            TextStyle::Body,
                            response.rect.left_bottom(),
                            Align2::LEFT_BOTTOM,
                        );

                        style_selection_boxes(ui, None);
                        ui.style_mut().drag_value_text_style = TextStyle::Body;
                        ui.spacing_mut().button_padding = egui::vec2(4.0, 6.0);
                        ui.spacing_mut().interact_size.x = 50.;
                        let value = selected.entry(*unit).or_insert(0);
                        let response = ui.add(egui::DragValue::new(value).speed(0.2).range(0..=n));
                        // Enter releases text focus before the Send mission shortcut is checked.
                        editing |=
                            response.has_focus() || response.lost_focus() || response.dragged();
                    });
                });
            });

            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    selected.retain(|_, count| *count > 0);
    editing
}

struct MissionFooterLayout {
    top: f32,
    players: egui::Rect,
    badges: egui::Rect,
    send: egui::Rect,
    secondary: Option<egui::Rect>,
}

/// Measure before painting: badges and controls must never wrap over one another.
fn mission_footer_layout(
    bounds: egui::Rect,
    players_width: f32,
    badges_width: f32,
    secondary: bool,
) -> MissionFooterLayout {
    let row_height = 50.0;
    let gap = 8.0;
    let button_gap = 16.0;
    let badge_gap = 20.0;
    let button_width = 180.0_f32.min(bounds.width());
    let button_count = if secondary {
        2.0
    } else {
        1.0
    };
    let buttons_width = button_count * button_width + (button_count - 1.0) * button_gap;
    let inline_width = players_width + badges_width + badge_gap + buttons_width + gap;
    let inline = inline_width <= bounds.width();
    // Bring both ends inward without forcing a footer that already fits onto another row.
    let horizontal_inset = ((bounds.width() - inline_width) * 0.25).clamp(0.0, 15.0);
    let bounds = bounds.shrink2(egui::vec2(horizontal_inset, 0.0));
    let send = egui::Rect::from_min_size(
        bounds.right_bottom() - egui::vec2(button_width, row_height),
        egui::vec2(button_width, row_height),
    );
    let secondary = secondary.then(|| {
        send.translate(if buttons_width <= bounds.width() {
            egui::vec2(-button_width - button_gap, 0.0)
        } else {
            egui::vec2(0.0, -row_height - gap)
        })
    });
    let first_button = secondary.unwrap_or(send);
    let badges = egui::Rect::from_min_size(
        if inline {
            egui::pos2(first_button.left() - badge_gap - badges_width, send.top())
        } else {
            egui::pos2(bounds.right() - badges_width, first_button.top() - gap - row_height)
        },
        egui::vec2(badges_width, row_height),
    );
    let players = egui::Rect::from_min_size(
        egui::pos2(
            bounds.left(),
            if inline {
                send.top()
            } else if players_width + gap + badges_width <= bounds.width() {
                badges.top()
            } else {
                badges.top() - gap - row_height
            },
        ),
        egui::vec2(players_width, row_height),
    );
    MissionFooterLayout {
        top: players.top().min(badges.top()),
        players,
        badges,
        send,
        secondary,
    }
}

/// Includes every order icon and label when deciding whether actions need another row.
fn mission_badges_width(ui: &Ui, army: &Army, plan: Option<BattlePlan>) -> f32 {
    let text_width = |text| {
        ui.painter()
            .layout_no_wrap(
                text,
                egui::FontId::proportional(MISSION_BADGE_FONT_SIZE),
                Color32::WHITE,
            )
            .size()
            .x
    };
    let strength_width = MISSION_BADGE_ICON_SIZE
        + MISSION_STRENGTH_TEXT_GAP
        + text_width(army.total_production().to_string());
    plan.map_or(strength_width, |plan| {
        strength_width
            + super::battle_plan_controls::PLAN_STRENGTH_GAP
            + super::battle_plan_controls::plan_badge_width(
                ui,
                MISSION_BADGE_ICON_SIZE,
                plan,
                army,
                MISSION_BADGE_FONT_SIZE,
            )
    })
}

/// Shows the selected fleet's production value beside the mission action.
fn draw_mission_strength_badge(
    ui: &mut Ui,
    selected: &Army,
    images: &ImageIds,
    button_center_y: f32,
    right: f32,
) -> egui::Rect {
    let value = selected.total_production().to_string();
    let font = egui::FontId::proportional(MISSION_BADGE_FONT_SIZE);
    let text_width =
        ui.painter().layout_no_wrap(value.clone(), font.clone(), Color32::WHITE).size().x;
    let icon_size = MISSION_BADGE_ICON_SIZE;
    let spacing = MISSION_STRENGTH_TEXT_GAP;
    let width = icon_size + spacing + text_width;
    let badge = egui::Rect::from_center_size(
        egui::pos2(right - width * 0.5, button_center_y),
        egui::vec2(width, 50.0),
    );
    let icon = egui::Rect::from_center_size(
        egui::pos2(badge.left() + icon_size * 0.5, button_center_y),
        egui::Vec2::splat(icon_size),
    );
    ui.painter().image(
        images.get("fleet"),
        icon,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.painter().text(
        egui::pos2(icon.right() + spacing, button_center_y),
        Align2::LEFT_CENTER,
        value,
        font,
        Color32::WHITE,
    );
    ui.interact(badge, ui.id().with("mission strength badge"), Sense::hover()).on_hover_ui(|ui| {
        ui.small("Fleet strength: total production points of the selected fleet.");
    });
    icon
}

/// Reserves all three order icons and the affinity before the fleet strength.
fn draw_mission_plan_badge(
    ui: &mut Ui,
    images: &ImageIds,
    fleet_icon: egui::Rect,
    plan: &mut BattlePlan,
    army: &Army,
) -> bool {
    let width = super::battle_plan_controls::plan_badge_width(
        ui,
        fleet_icon.width(),
        *plan,
        army,
        MISSION_BADGE_FONT_SIZE,
    );
    let rect = egui::Rect::from_min_size(
        fleet_icon.min - egui::vec2(width + super::battle_plan_controls::PLAN_STRENGTH_GAP, 0.0),
        fleet_icon.size(),
    );
    super::battle_plan_controls::draw_plan_button_at(
        ui,
        images,
        rect,
        plan,
        army,
        MISSION_BADGE_FONT_SIZE,
    )
}

/// Uses every current non-rejected fleet, replacing our published offer with the live editor.
fn joint_mission_preview(
    mission: &Mission,
    map: &Map,
    player_id: PlayerId,
    turn: usize,
    invitation: &JointAttackInvitation,
) -> Mission {
    let mut preview = Mission::new_with_id(
        invitation.id,
        turn,
        player_id,
        map.get(mission.origin),
        map.get(mission.destination),
        mission.objective,
        mission.army.clone(),
        mission.bombing.clone(),
        mission.combat_probes,
        false,
        None,
    );
    let duration = invitation
        .participants
        .iter()
        .filter(|participant| {
            participant.player_id != player_id
                && participant.response != JointAttackResponse::Rejected
        })
        .filter_map(|participant| participant.contribution.as_ref())
        .filter_map(|contribution| {
            let origin = map.try_get(contribution.origin)?;
            Some(
                Mission::new_with_id(
                    invitation.id,
                    turn,
                    contribution.player_id,
                    origin,
                    map.get(invitation.destination),
                    invitation.objective,
                    contribution.army.clone(),
                    contribution.bombing.clone(),
                    contribution.combat_probes,
                    false,
                    None,
                )
                .duration(map),
            )
        })
        .fold(preview.duration(map), usize::max);
    preview.joint_attack = Some(JointAttackMission {
        arrival_turn: turn.saturating_add(duration),
        ..default()
    });
    preview
}

/// Route facts and combat options use the same spacing, labels, and tooltips in both panels.
fn draw_mission_details(
    ui: &mut Ui,
    mission: &mut Mission,
    map: &Map,
    player: &Player,
    turn: usize,
    images: &ImageIds,
    invitation: Option<&JointAttackInvitation>,
) {
    let origin = map.get(mission.origin);
    let destination = map.get(mission.destination);
    let preview = invitation
        .map(|invitation| joint_mission_preview(mission, map, player.id, turn, invitation));
    let route = preview.as_ref().unwrap_or(mission);
    let speed = route.speed();
    let is_intercept = mission.objective == Icon::Intercept;
    let has_route = !is_intercept || mission.intercept.is_some();
    let distance = if is_intercept {
        mission
            .intercept
            .as_ref()
            .map(|intercept| origin.position.distance(intercept.point) / Planet::SIZE)
    } else {
        Some(mission.distance(map))
    };
    let duration = if has_route {
        route.duration(map)
    } else {
        0
    };
    let movement = if invitation.is_some() {
        route.next_turn_movement(map)
    } else {
        speed
    };
    ui.horizontal(|ui| {
        ui.small("🎯 Objective:");

        ui.spacing_mut().item_spacing.x = 4.;
        ui.add_image(images.get(mission.objective.asset_key()), [20.; 2]);
        ui.small(mission.objective.to_name());
    });

    ui.small(format!(
        "📏 Target distance: {}",
        distance.map_or_else(|| "---".to_owned(), |distance| format!("{distance:.1} AU")),
    )).on_hover_mission(if is_intercept {
        "Distance from the origin world to the interception point. Select a fleet that can reach a meeting point to calculate the route."
    } else {
        "AU means astronomical unit, the distance scale used on the galaxy map. \
        Target distance is the length of the route from the origin world to the \
        destination world."
    });
    let movement_tooltip = if invitation.is_some() {
        "Distance your fleet travels on its first turn. Each participating fleet adjusts its movement to reach the target on the shared arrival turn."
    } else {
        mission_movement_tooltip(speed == f32::MAX)
    };
    ui.small(format!(
        "🚀 Movement: {}",
        if speed == 0. || speed == f32::MAX {
            "---".to_string()
        } else if invitation.is_some() {
            format!("{movement:.1} AU")
        } else {
            format!("{speed} AU")
        }
    ))
    .on_hover_mission(movement_tooltip);
    let arrival_turn = mission_arrival_turn(turn, duration);
    let duration_response = ui.small(format!(
        "⏱ Duration: {}",
        if duration == 0 {
            "---".to_string()
        } else {
            format!(
                "+{} turn{} ({})",
                duration,
                if duration == 1 {
                    ""
                } else {
                    "s"
                },
                arrival_turn,
            )
        }
    ));
    if duration == 0 {
        duration_response
            .on_hover_mission("Select a valid fleet and route to calculate its arrival turn.");
    } else {
        duration_response.on_hover_mission(if let Some(intercept) = mission.intercept.as_ref().filter(|_| is_intercept) {
            format!("Your fleet reaches the interception point on turn {arrival_turn}. Contact is planned for turn {}, along the enemy's flight path before landing. If your fleet reaches the meeting point early, it waits there. A recall can cause a miss; surviving targets continue their movement.", intercept.encounter_turn)
        } else if invitation.is_some() {
            format!("All participating fleets arrive at turn {arrival_turn}. The slowest arrival sets the shared duration; each fleet adjusts its own movement.")
        } else {
            mission_arrival_tooltip(turn, duration)
        });
    }
    let fuel = mission.dispatch_fuel_consumption(map, player);
    let fuel_check = player.resources.deuterium >= fuel;
    let is_patrol = mission.objective == Icon::Patrol;
    let fuel_text = if !has_route {
        "⛽ Fuel consumption: ---".to_owned()
    } else if is_patrol {
        format!("⛽ Fuel consumption: {fuel} + {} / turn", mission.patrol_upkeep())
    } else {
        format!("⛽ Fuel consumption: {fuel}")
    };
    let fuel_response = if fuel_check {
        ui.small(fuel_text)
    } else {
        ui.colored_label(Color32::RED, RichText::new(fuel_text).small())
    };
    fuel_response.on_hover_mission(if !has_route {
        "Select a fleet that can reach an interception point to calculate its deuterium cost."
    } else if is_patrol {
        PATROL_FUEL_TOOLTIP
    } else {
        "Amount of deuterium it costs to send this mission."
    });
    if is_patrol {
        let probability = crate::core::patrols::detection_probability(
            1.0,
            crate::core::combat::report::combat_fleet_strength(&mission.army),
        );
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let icon = draw_patrol_detection_icon(ui);
            icon | ui.small(format!("Detection probability: {:.1}%", probability * 100.0))
        })
        .inner
        .on_hover_mission(PATROL_DETECTION_TOOLTIP);
        return;
    }
    draw_deep_cover_option(ui, mission, origin);

    if mission.objective == Icon::Raid {
        ui.small(format!(
            "📦 Cargo capacity: {}",
            crate::core::raids::cargo_capacity(&mission.army)
        ))
        .on_hover_mission("Per ship, production levels 1-5 carry 0, 10, 30, 70, and 150 resources respectively. Loot is credited when the fleet returns.");
    }

    if mission.objective == Icon::Destroy {
        let war_suns = mission.army.amount(&Unit::war_sun());
        let per_sun = f64::from(destination.destroy_probability_basis_points()) / 10_000.0;
        let combined = 1.0 - (1.0 - per_sun).powf(war_suns as f64);
        ui.small(format!(
            "💥 Chance of destruction: {:.1}% / {:.1}%",
            per_sun * 100.0,
            combined * 100.0,
        ))
        .on_hover_mission(
            "The first number is the probability of destroying the world per War Sun if \
            they shoot the first round of combat. The second number is the total chance for \
            all selected War Suns.",
        );
    }

    if matches!(mission.objective, Icon::Colonize | Icon::Attack | Icon::Raid | Icon::Destroy) {
        let probes = mission.army.amount(&Unit::probe());
        ui.add_enabled_ui(probes > 0, |ui| {
            ui.horizontal(|ui| {
                ui.small("⚔ Combat Probes:");
                ui.add(toggle(&mut mission.combat_probes));
            });
        })
        .response
        .on_hover_ui(|ui| {
            ui.set_width(300.);
            ui.small(
                "Normally, Probes leave combat after the first round and return \
            to the planet of origin. Enabling this option makes the Probes stay \
            during the whole combat, serving as extra fodder and having the \
            advantage that they stay with the rest of the fleet when victorious, \
            at risk of getting no enemy unit information when losing combat. \
            Probes always stay if the combat takes only one round.",
            );
        })
        .on_disabled_hover_mission("No Probes selected for this mission.");

        if probes == 0 {
            mission.combat_probes = false;
        }

        let bombing_fixed_by_inviter = invitation.is_some_and(|item| item.inviter != player.id);
        if let Some(invitation) = invitation.filter(|_| bombing_fixed_by_inviter) {
            // Bombing is a shared allied-attack order. Guests choose only their fleet and
            // probe behavior and battle plan; the mission owner controls every Bomber's target.
            mission.bombing.clone_from(&invitation.bombing);
        }
        let bombers = mission.army.amount(&Unit::Ship(Ship::Bomber));
        ui.add_enabled_ui(
            bombers > 0 && !destination.is_moon() && !bombing_fixed_by_inviter,
            |ui| {
                ui.horizontal(|ui| {
                    ui.small("💣 Bombing raid:");

                    style_selection_boxes(ui, None);
                    ui.style_mut().spacing.button_padding.y = 1.5;
                    if let Some(style) = ui.style_mut().text_styles.get_mut(&TextStyle::Button) {
                        style.size = 18.;
                    }

                    ComboBox::from_id_salt("bombing")
                        .width(125.)
                        .selected_text(mission.bombing.to_name())
                        .show_ui(ui, |ui| {
                            for item in BombingRaid::iter() {
                                ui.style_mut().spacing.button_padding.y = 1.5;
                                ui.style_mut().spacing.item_spacing.y = 5.;

                                ui.selectable_value(
                                    &mut mission.bombing,
                                    item.clone(),
                                    RichText::new(item.to_name()).small(),
                                )
                                .on_hover_cursor(CursorIcon::PointingHand)
                                .on_hover_mission(item.description());
                            }
                        })
                        .response
                        .on_hover_cursor(CursorIcon::PointingHand);
                });
            },
        )
        .response
        .on_hover_mission(
            "Command Bombers to bomb enemy buildings. Every round of combat, \
        every bomber has a 25% chance to decrease a target building's level by \
        one. The Planetary Shield must first be destroyed before bombing can \
        take place.",
        )
        .on_disabled_hover_mission(if bombing_fixed_by_inviter {
            "The player who created the allied attack fixed its bombing objective."
        } else if destination.is_moon() {
            "Moons cannot be bombed."
        } else {
            "No Bombers selected for this mission."
        });

        if !bombing_fixed_by_inviter && (bombers == 0 || destination.is_moon()) {
            mission.bombing = BombingRaid::None;
        }
    }
}

/// A monochrome radar sweep matches the detail rows without depending on font icon coverage.
fn draw_patrol_detection_icon(ui: &mut Ui) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::Vec2::splat(16.0), Sense::hover());
    let center = rect.center();
    let color = ui.visuals().text_color();
    let stroke = Stroke::new(1.0, color);
    for radius in [3.5, 7.0] {
        ui.painter().circle_stroke(center, radius, stroke);
    }
    ui.painter().line_segment([center, center + egui::vec2(5.0, -5.0)], stroke);
    ui.painter().circle_filled(center + egui::vec2(-3.5, -2.5), 1.5, color);
    response
}

/// Draws the active missions interface and emits any resulting local actions.
fn draw_active_missions(
    ui: &mut Ui,
    missions: Vec<&Mission>,
    recall_mission: &mut MessageWriter<RecallMissionMsg>,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    is_hovered: bool,
    images: &ImageIds,
    editable: bool,
) {
    if missions.is_empty() {
        ui.add_space(40.);
        ui.vertical_centered(|ui| {
            ui.label(format!("No {}.", state.mission_tab.to_lowername()));
        });
        return;
    }

    let patrol_friendship_available = session.active_game.as_ref().is_some_and(|game| {
        game.persisted.state.players.iter().filter(|player| !player.spectator).count() >= 3
    });

    // Sort by turns remaining ascending
    let missions = missions
        .iter()
        .sorted_by(|a, b| mission_display_turns(a, map).cmp(&mission_display_turns(b, map)));

    ui.add_space(30.);

    // The panel artwork includes a border; keep both the viewport and its scrollbar inside it.
    let mut frame = egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 0)).begin(ui);
    ScrollArea::vertical()
        .max_width(frame.content_ui.available_width())
        .auto_shrink([false, false])
        .max_height(frame.content_ui.available_height() - 50.)
        .show(&mut frame.content_ui, |ui| {
            let available_width = ui.available_width();
            let (route_column_width, leading_space) = mission_row_layout(available_width);
            let route_preview_width =
                (route_column_width - MISSION_ROUTE_FIXED_CONTENT_WIDTH).max(180.0);

            ui.horizontal(|ui| {
                ui.add_space(leading_space);

                let mut changed_hover = false;
                egui::Grid::new("active missions")
                    .spacing([MISSION_COLUMN_GAP, 0.])
                    .striped(false)
                    .show(ui, |ui| {
                        for mission in missions {
                            let (left_planet, right_planet, returning) =
                                mission_route_presentation(mission);
                            let origin = map.get(left_planet);
                            let destination = map.get(right_planet);
                            let foreign_patrol =
                                (mission.patrol.is_some() || mission.intercept.is_some()) && mission.owner != player.id;

                            if !foreign_patrol
                                && (mission.owner == player.id
                                    || !mission.objective.is_hidden()
                                    || mission.is_seen_by_radar(map, player).is_some())
                            {
                                let (resp1, resp2) = draw_mission_planet_link(
                                    ui,
                                    images.get(origin.image()),
                                    &origin.name,
                                    Sense::click(),
                                );
                                let resp1 = resp1.on_hover_cursor(CursorIcon::PointingHand);
                                let resp2 = resp2.on_hover_cursor(CursorIcon::PointingHand);

                                if mission.owner == player.id {
                                    let resp =
                                        draw_mission_log_badge(ui, images.get("logs"), resp1.rect);

                                    resp.on_hover_ui(|ui| {
                                        ui.set_min_width(350.);
                                        ui.small(format!(
                                            "Mission logs\n===========\n\n{}",
                                            mission.logs
                                        ));
                                    });
                                }

                                handle_mission_planet_link(
                                    &resp1,
                                    &resp2,
                                    origin,
                                    &mut changed_hover,
                                    state,
                                    map,
                                    player,
                                );
                            } else {
                                draw_mission_planet_link(
                                    ui,
                                    images.get("unknown"),
                                    "Unknown",
                                    Sense::hover(),
                                );
                            }

                            let (mut route_ui, response) =
                                mission_route_cell(ui, route_column_width);
                            let mut response = if mission.commanded_army(player.id).is_none() {
                                block_enemy_route_clicks(ui, response, mission.id)
                            } else {
                                response
                            };
                            let preview = route_ui.horizontal_centered(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.;

                                let objective = if mission.patrol.is_some()
                                    && mission.owner == player.id
                                {
                                    Icon::Patrol
                                } else {
                                    mission.displayed_objective(player.id)
                                };
                                draw_mission_route_icon(ui, images.get(objective.asset_key()))
                                    .on_hover_small(mission_objective_label(objective));

                                let has_plan = super::battle_plan_controls::mission_plan_visible(mission, player.id);
                                let turns = RichText::new(format!(
                                    "+{}",
                                    mission_display_turns(mission, map)
                                ))
                                .strong();
                                let mut preview_width = route_preview_width;
                                if mission.owner != player.id {
                                    // Enemy rows have no recall action or patrol access control.
                                    preview_width = route_column_width
                                        - MISSION_ROUTE_ICON_SIZE
                                        - ui.spacing().item_spacing.x;
                                    if !mission.is_patrolling() {
                                        let eta_width = egui::WidgetText::from(turns.clone())
                                            .into_galley(
                                                ui,
                                                Some(egui::TextWrapMode::Extend),
                                                f32::INFINITY,
                                                TextStyle::Body,
                                            )
                                            .size()
                                            .x;
                                        preview_width -= eta_width + ui.spacing().item_spacing.x;
                                    }
                                }
                                if has_plan {
                                    preview_width -= super::battle_plan_controls::mission_plan_button_width(ui)
                                        + ui.spacing().item_spacing.x;
                                    let mut plan = mission.battle_plan_for(Some(player.id));
                                    ui.add_enabled_ui(editable, |ui| {
                                        ui.push_id(("active battle plan", mission.id), |ui| {
                                            super::battle_plan::draw_mission_plan_button(
                                                ui,
                                                images, &mut plan, mission.commanded_army(player.id).unwrap_or(&mission.army),
                                            );
                                        });
                                    });
                                    if plan != mission.battle_plan_for(Some(player.id)) {
                                        state.mission_battle_plan_changes.push((mission.id, plan));
                                    }
                                }

                                let [red, green, blue] = session.player_color(mission.owner).rgb();
                                let preview = draw_route_preview(
                                    ui,
                                    preview_width,
                                    mission,
                                    map,
                                    player,
                                    Color32::from_rgb(red, green, blue),
                                    returning,
                                );

                                if mission.is_patrolling() {
                                    if let Some(patrol) = mission.patrol.as_ref().filter(|_| mission.owner == player.id) {
                                        if patrol_friendship_available {
                                            let access = draw_mission_route_icon(ui, images.get("protect"))
                                                .on_hover_cursor(CursorIcon::Default);
                                            let (_, changes) = super::player_access::draw_player_access_picker(
                                                &access, "Manage friendly players",
                                                "",
                                                session, player.id,
                                                &patrol.friendly_players,
                                                editable,
                                            );
                                            for (friendly_player, friendly) in changes {
                                                state.patrol_friendly_changes.push((mission.id, friendly_player, friendly));
                                                set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
                                            }
                                        } else {
                                            // Preserve the access icon's slot and the recall button's position.
                                            let (rect, _) = ui.allocate_exact_size(
                                                egui::Vec2::splat(MISSION_ROUTE_ICON_SIZE), Sense::hover(),
                                            );
                                            let infinity = egui::WidgetText::from(RichText::new("∞").strong().color(Color32::WHITE))
                                                .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, TextStyle::Body);
                                            ui.painter().galley(rect.center() - infinity.size() * 0.5, infinity, Color32::WHITE);
                                        }
                                    }
                                } else {
                                    ui.label(turns);
                                }

                                if mission_recall_available(mission, map, player.id) {
                                    ui.add_space(10.0);
                                    let recall = draw_recall_button(ui, images, editable)
                                        .on_hover_small(if editable {
                                            "Recall this mission. The fleet will turn around immediately and return to the planet of origin at no extra cost."
                                                .to_string()
                                        } else {
                                            "Continue your turn before changing mission orders."
                                                .to_string()
                                        });
                                    if recall.clicked() {
                                        recall_mission.write(RecallMissionMsg::new(mission.id));
                                    }
                                    // The action belongs to this fleet's hover preview too.
                                    response |= recall;
                                }
                                preview
                            }).inner;

                            // The list opens only the fleet preview; route facts belong to map hover.
                            if response.hovered() || preview.hovered() {
                                state.mission_hover = Some(mission.id);
                                state.mission_hover_from_ui = true;
                                changed_hover = true;
                            }
                            if handle_space_location_link(preview, mission.position, state).clicked() {
                                state.location_marker = Some(LocationMarker::for_mission(mission.id, mission.position));
                            }

                            if let Some(intercept) = mission.intercept.as_ref().filter(|_| mission.objective == Icon::Intercept || mission.owner != player.id) {
                                let (image, name) = draw_mission_planet_link(ui, images.get("deep space"), "Deep space", Sense::click());
                                let point = if mission.owner == player.id { intercept.point } else { mission.position };
                                handle_space_location_link(image | name, point, state);
                            } else if let Some(patrol) = &mission.patrol {
                                let can_focus_zone = !foreign_patrol || mission.is_patrolling();
                                let (image, name) = draw_mission_planet_link(
                                    ui,
                                    images.get("deep space"),
                                    "Deep space",
                                    if can_focus_zone { Sense::click() } else { Sense::hover() },
                                );
                                if image.hovered() || name.hovered() {
                                    state.mission_hover = Some(mission.id);
                                    state.mission_hover_from_ui = true;
                                    changed_hover = true;
                                }
                                if can_focus_zone {
                                    handle_space_location_link(image | name, patrol.center, state);
                                }
                            } else {
                                let (resp4, resp3) = draw_mission_planet_link(
                                    ui,
                                    images.get(destination.image()),
                                    &destination.name,
                                    Sense::click(),
                                );
                                let resp4 = resp4.on_hover_cursor(CursorIcon::PointingHand);
                                let resp3 = resp3.on_hover_cursor(CursorIcon::PointingHand);

                                handle_mission_planet_link(
                                    &resp3,
                                    &resp4,
                                    destination,
                                    &mut changed_hover,
                                    state,
                                    map,
                                    player,
                                );
                            }

                            ui.end_row();
                        }

                        // If not hovering anything, reset all hover selections
                        if is_hovered && !changed_hover {
                            state.planet_hover = None;
                            state.mission_planet_hover = None;
                            state.mission_hover = None;
                        }
                    });
            });
        });
    frame.end(ui);
}

/// Draws the mission reports interface and emits any resulting local actions.
fn mission_report_fleet_owner(report: &MissionReport, viewer: PlayerId) -> PlayerId {
    report
        .mission
        .joint_attack
        .as_ref()
        .filter(|attack| attack.attackers.contains_key(&viewer))
        .map_or(report.mission.owner, |_| viewer)
}

fn mission_report_fleet_color(
    report: &MissionReport,
    viewer: PlayerId,
    session: &MultiplayerSession,
) -> Color32 {
    if mission_report_sender_is_known(report, viewer) {
        session.player_color(mission_report_fleet_owner(report, viewer)).color().to_color32()
    } else {
        NEUTRAL_COMBAT_COLOR
    }
}

/// Resolves report selection by report identity rather than mission identity.
///
/// One traveling mission can produce multiple reports: an en-route encounter and its later
/// destination arrival. Those reports intentionally share a mission ID, but never a report ID.
fn selected_mission_report<'a>(
    reports: &[&'a MissionReport],
    selected: Option<ReportId>,
) -> Option<&'a MissionReport> {
    reports
        .iter()
        .copied()
        .find(|report| Some(report.id) == selected)
        .or_else(|| reports.last().copied())
}

fn mission_report_player_name(session: &MultiplayerSession, player_id: PlayerId) -> String {
    session
        .player_name(player_id)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Player {player_id}"))
}

fn mission_report_attacker_name(
    report: &MissionReport,
    viewer: PlayerId,
    session: &MultiplayerSession,
) -> String {
    if mission_report_sender_is_known(report, viewer) {
        mission_report_player_name(session, report.mission.owner)
    } else {
        "Unknown".to_owned()
    }
}

fn mission_report_attacker_color(
    report: &MissionReport,
    viewer: PlayerId,
    session: &MultiplayerSession,
) -> Color32 {
    if mission_report_sender_is_known(report, viewer) {
        session.player_color(report.mission.owner).color().to_color32()
    } else {
        NEUTRAL_COMBAT_COLOR
    }
}

fn mission_report_defender_name(report: &MissionReport, session: &MultiplayerSession) -> String {
    if let Some(name) = report.space_fauna_name() {
        name.to_owned()
    } else if let Some(name) = report.independent_population_name() {
        name
    } else if let Some(player_id) = report.planet.controlled.or(report.planet.owned) {
        mission_report_player_name(session, player_id)
    } else {
        "Unclaimed".to_owned()
    }
}

fn mission_report_defender_color(report: &MissionReport, session: &MultiplayerSession) -> Color32 {
    if report.is_space_fauna_encounter() || report.is_independent_population_encounter() {
        NEUTRAL_COMBAT_COLOR
    } else {
        report
            .planet
            .controlled
            .or(report.planet.owned)
            .map(|player_id| session.player_color(player_id).color().to_color32())
            .unwrap_or(NEUTRAL_COMBAT_COLOR)
    }
}

/// Draws a compact report endpoint with the same below-image caption used by active missions.
fn draw_mission_report_planet_link(
    ui: &mut Ui,
    image: egui::TextureId,
    corner_radius: egui::CornerRadius,
    planet_name: &str,
    sense: Sense,
) -> (Response, Response) {
    let (cell, _) = ui.allocate_exact_size(
        egui::vec2(MISSION_REPORT_ROUTE_PLANET_WIDTH, MISSION_REPORT_ROUTE_HEIGHT),
        Sense::hover(),
    );
    let (image_rect, name_rect) = mission_planet_rects(cell);
    let image_response = ui
        .place(
            image_rect,
            egui::Image::new(SizedTexture::new(
                image,
                egui::Vec2::splat(MISSION_PLANET_IMAGE_SIZE),
            ))
            .corner_radius(corner_radius),
        )
        .interact(sense);
    let name = egui::WidgetText::from(RichText::new(planet_name).text_style(TextStyle::Small))
        .into_galley(ui, Some(egui::TextWrapMode::Truncate), name_rect.width(), TextStyle::Small);
    let name_pos = name_rect.center() - name.size() * 0.5;
    ui.painter().galley(name_pos, name, ui.visuals().text_color());
    let name_response = ui.interact(name_rect, ui.next_auto_id(), sense);

    (image_response, name_response)
}

fn draw_mission_report_player(ui: &mut Ui, name: String, color: Color32) {
    let (cell, _) = ui.allocate_exact_size(
        egui::vec2(MISSION_REPORT_ROUTE_PLAYER_WIDTH, MISSION_REPORT_ROUTE_HEIGHT),
        Sense::hover(),
    );
    let (image_rect, _) = mission_planet_rects(cell);
    let galley = egui::WidgetText::from(RichText::new(&name).small().color(color)).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        cell.width(),
        TextStyle::Small,
    );
    let text_rect = egui::Rect::from_center_size(image_rect.center(), galley.size());
    if galley.elided {
        ui.interact(text_rect, ui.next_auto_id(), Sense::hover()).on_hover_small(name);
    }
    ui.painter().galley(text_rect.min, galley, color);
}

fn mission_report_route_leading_space(available_width: f32) -> f32 {
    ((available_width - MISSION_REPORT_ROUTE_WIDTH) * 0.5).max(0.0)
}

#[derive(Clone, Copy, Debug)]
struct MissionReportRouteRects {
    origin: egui::Rect,
    attacker: egui::Rect,
    center: egui::Rect,
    defender: egui::Rect,
    destination: egui::Rect,
}

fn mission_report_route_rects(row: egui::Rect) -> MissionReportRouteRects {
    let top = row.top();
    let height = MISSION_REPORT_ROUTE_HEIGHT;
    let mut left = row.left() + mission_report_route_leading_space(row.width());
    let mut next = |width: f32| {
        let rect = egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, height));
        left = rect.right() + MISSION_REPORT_ROUTE_GAP;
        rect
    };

    MissionReportRouteRects {
        origin: next(MISSION_REPORT_ROUTE_PLANET_WIDTH),
        attacker: next(MISSION_REPORT_ROUTE_PLAYER_WIDTH),
        center: next(MISSION_REPORT_ROUTE_CENTER_WIDTH),
        defender: next(MISSION_REPORT_ROUTE_PLAYER_WIDTH),
        destination: next(MISSION_REPORT_ROUTE_PLANET_WIDTH),
    }
}

fn draw_mission_reports(
    ui: &mut Ui,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    is_hovered: bool,
    images: &ImageIds,
) {
    let reports = player.reports.iter().filter(|r| !r.hidden).collect::<Vec<_>>();

    if reports.is_empty() {
        ui.add_space(40.);
        ui.vertical_centered(|ui| {
            ui.label(format!("No {}.", state.mission_tab.to_lowername()));
        });
        return;
    }

    // Capture the panel bounds before report columns can grow beyond their preferred size.
    let panel_rect = ui.max_rect();
    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.set_height(547.);

        ui.add_space(30.);

        ScrollArea::vertical().show(ui, |ui| {
            ui.set_width(150.);

            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 5.;

                // The scroll area inherits the horizontal parent layout. Apply top padding here
                // so the outside hover stroke clears the clip edge vertically.
                ui.add_space(MISSION_REPORT_LIST_TOP_PADDING);

                for report in reports.iter().rev() {
                    let (rect, mut response) =
                        ui.allocate_exact_size([160., 50.].into(), Sense::click());

                    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;

                            ui.add_space(7.);

                            ui.add_image(
                                images.get(report.mission.objective.asset_key()),
                                [25.; 2],
                            );

                            let size = mission_report_image_base_size(
                                state.mission_report == Some(report.id),
                            );
                            let mission_image = report.mission.image(player);
                            draw_mission_image(
                                ui,
                                images.get(mission_image),
                                mission_report_image_size(mission_image, size),
                                MISSION_REPORT_IMAGE_SLOT_SIZE,
                                mission_report_image_offset(mission_image),
                                mission_report_fleet_color(report, player.id, session),
                            );

                            ui.scope(|ui| {
                                ui.set_width(20.);
                                ui.small(report.turn.to_string());
                            });

                            let resp = add_combat_report_destination_image(
                                ui, report, state, images, 40.0,
                            );

                            if report.combat_report.is_some() {
                                let size = [20.; 2];
                                let pos = resp.rect.right_top() - egui::vec2(size[0], 0.);
                                ui.put(
                                    egui::Rect::from_min_size(pos, size.into()),
                                    egui::Image::new(SizedTexture::new(
                                        images.get(report.image(player)),
                                        size,
                                    )),
                                );
                            }
                        });
                    });

                    response = response.on_hover_cursor(CursorIcon::PointingHand);
                    if report.is_patrol_contact() {
                        response = response.on_hover_small("Friendly Fleet Detected");
                    }

                    if response.hovered() {
                        ui.painter().rect_stroke(
                            rect,
                            4.0,
                            Stroke::new(
                                MISSION_REPORT_HOVER_STROKE_WIDTH,
                                if response.is_pointer_button_down_on() {
                                    Color32::from_rgb(95, 131, 175)
                                } else {
                                    Color32::from_rgb(59, 66, 82)
                                },
                            ),
                            StrokeKind::Outside,
                        );
                    }

                    if response.clicked() {
                        set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
                        state.mission_report = Some(report.id);
                    }
                }
            });
        });

        ui.add_space(-10.);
        ui.separator();

        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 40.);

            let Some(report) = selected_mission_report(&reports, state.mission_report) else {
                return;
            };

            ui.horizontal(|ui| {
                let (route_row, _) = ui.allocate_exact_size(
                    egui::vec2(ui.available_width(), MISSION_REPORT_ROUTE_HEIGHT),
                    Sense::hover(),
                );
                let route_rects = mission_report_route_rects(route_row);
                let mut changed_hover = false;
                let origin = map.get(report.mission.origin);
                let destination = map.get(report.mission.destination);
                let sender_known = mission_report_sender_is_known(report, player.id);
                let origin_image = if sender_known {
                    origin.image()
                } else {
                    "unknown".to_owned()
                };
                let mut origin_ui = ui.new_child(
                    UiBuilder::new().id_salt("report origin").max_rect(route_rects.origin),
                );
                let (resp1, resp2) = draw_mission_report_planet_link(
                    &mut origin_ui,
                    images.get(origin_image),
                    egui::CornerRadius::ZERO,
                    if sender_known {
                        &origin.name
                    } else {
                        ""
                    },
                    if sender_known {
                        Sense::click()
                    } else {
                        Sense::hover()
                    },
                );
                if sender_known {
                    let resp1 = resp1.on_hover_cursor(CursorIcon::PointingHand);
                    let resp2 = resp2.on_hover_cursor(CursorIcon::PointingHand);
                    if report.mission.owner == player.id {
                        let logs = ui.add_icon_on_image(images.get("logs"), resp1.rect);
                        logs.on_hover_ui(|ui| {
                            ui.set_min_width(350.);
                            ui.small(format!(
                                "Mission logs\n===========\n\n{}",
                                report.mission.logs
                            ));
                        });
                    }
                    handle_mission_planet_link(
                        &resp1,
                        &resp2,
                        origin,
                        &mut changed_hover,
                        state,
                        map,
                        player,
                    );
                }

                let mut attacker_ui = ui.new_child(
                    UiBuilder::new().id_salt("report attacker").max_rect(route_rects.attacker),
                );
                draw_mission_report_player(
                    &mut attacker_ui,
                    mission_report_attacker_name(report, player.id, session),
                    mission_report_attacker_color(report, player.id, session),
                );

                let (route_image_rect, _) = mission_planet_rects(route_rects.center);
                let mut route_ui = ui.new_child(
                    UiBuilder::new()
                        .id_salt("report route")
                        .max_rect(egui::Rect::from_center_size(
                            route_image_rect.center(),
                            egui::vec2(MISSION_REPORT_ROUTE_CENTER_WIDTH, 50.0),
                        ))
                        .layout(
                            Layout::left_to_right(Align::Center).with_main_align(Align::Center),
                        ),
                );
                route_ui.spacing_mut().item_spacing.x = 4.0;

                route_ui
                    .add_image(images.get(report.mission.objective.asset_key()), [25.; 2])
                    .on_hover_small(report.mission.objective.to_name());

                let mission_image = report.mission.image(player);
                draw_mission_image(
                    &mut route_ui,
                    images.get(mission_image),
                    mission_report_image_size(mission_image, 50.0),
                    50.0,
                    mission_report_image_offset(mission_image),
                    mission_report_fleet_color(report, player.id, session),
                );

                route_ui
                    .small(report.turn.to_string())
                    .on_hover_small(format!("The mission arrived in turn {}.", report.turn));

                let mut defender_ui = ui.new_child(
                    UiBuilder::new().id_salt("report defender").max_rect(route_rects.defender),
                );
                draw_mission_report_player(
                    &mut defender_ui,
                    mission_report_defender_name(report, session),
                    mission_report_defender_color(report, session),
                );

                let mut destination_ui = ui.new_child(
                    UiBuilder::new()
                        .id_salt("report destination")
                        .max_rect(route_rects.destination),
                );
                let (resp4, resp3) = draw_mission_report_planet_link(
                    &mut destination_ui,
                    images.get(combat_selection_planet_image(report)),
                    encounter_image_corner_radius(report, MISSION_PLANET_IMAGE_SIZE),
                    mission_report_destination_name(report, destination),
                    Sense::click(),
                );
                let resp4 = resp4.on_hover_cursor(CursorIcon::PointingHand);
                let resp3 = resp3.on_hover_cursor(CursorIcon::PointingHand);

                if report.combat_report.is_some() {
                    ui.add_icon_on_image(images.get(report.image(player)), resp4.rect);
                }

                if report.is_space_encounter() {
                    handle_space_location_link(resp4 | resp3, report.planet.position, state);
                } else {
                    handle_mission_planet_link(
                        &resp3,
                        &resp4,
                        destination,
                        &mut changed_hover,
                        state,
                        map,
                        player,
                    );
                }

                // If not hovering anything, reset all hover selections
                if is_hovered && !changed_hover {
                    state.planet_hover = None;
                    state.mission_planet_hover = None;
                }
            });

            ui.add_space(-10.);
            ui.horizontal(|ui| {
                draw_mission_report_strength_bars(
                    ui,
                    report,
                    session,
                    player.id,
                    MISSION_REPORT_STRENGTH_BAR_WIDTH,
                );
            });
            ui.add_space(-10.);

            let mut divider_x = 0.0;
            let army_row = ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(140.);

                    let army = match report.mission.objective {
                        Icon::MissileStrike => vec![Unit::interplanetary_missile()],
                        Icon::Spy => vec![Unit::probe()],
                        _ => Unit::ships(),
                    };

                    draw_army_grid(ui, "attacker", &army, report, player, images);
                });

                ui.add_space(-13.);
                let (divider, _) = ui.allocate_exact_size(egui::vec2(6.0, 0.0), Sense::hover());
                divider_x = divider.center().x;
                ui.add_space(-10.);

                ui.vertical(|ui| {
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.;

                        let destination = map.get(report.mission.destination);

                        if report.is_space_fauna_encounter() {
                            draw_army_grid(
                                ui,
                                "defender_fauna",
                                &combat_report_fauna_units(report),
                                report,
                                player,
                                images,
                            );
                        } else if matches!(report.mission.objective, Icon::Patrol | Icon::Intercept)
                        {
                            draw_army_grid(
                                ui,
                                "intercepted_fleet",
                                &Unit::ships(),
                                report,
                                player,
                                images,
                            );
                        } else if !report.planet.army.has_army() {
                            ui.label(format!(
                                "Empty {}.",
                                if destination.is_moon() {
                                    "moon"
                                } else {
                                    "planet"
                                }
                            ));
                        } else if destination.is_moon() {
                            for (index, army) in
                                [Unit::ships(), Unit::buildings_for_world(true, false)]
                                    .into_iter()
                                    .enumerate()
                            {
                                draw_army_grid(
                                    ui,
                                    format!("defender_moon_{index}").as_str(),
                                    &army,
                                    report,
                                    player,
                                    images,
                                );
                            }
                        } else {
                            draw_army_grid(
                                ui,
                                "defender_ships",
                                &Unit::ships(),
                                report,
                                player,
                                images,
                            );
                            draw_army_grid(
                                ui,
                                "defender_defenses",
                                &Unit::defenses(),
                                report,
                                player,
                                images,
                            );
                            draw_mission_report_planet_intel(ui, report, player, images);
                        }
                    });

                    if (report.planet_destroyed || report.planet_colonized)
                        && report.can_see(&Side::Defender, player.id)
                    {
                        let (icon, label) = if report.planet_destroyed {
                            (Icon::Destroy, "Planet destroyed")
                        } else {
                            (Icon::Colonize, "Planet colonized")
                        };

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.add_space(10.);
                            ui.add_image(images.get(icon.to_lowername()), [15.0, 15.0]);
                            ui.small(label);
                        });
                    }
                });
            });
            let army_row = army_row.response.rect;

            // Bound the divider to the measured armies so it cannot stretch through the loot.
            ui.painter().vline(
                divider_x,
                army_row.y_range(),
                Stroke::new(0.75, ui.visuals().widgets.noninteractive.bg_stroke.color),
            );

            if report.is_patrol_contact() {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_image(images.get(Icon::Protect.asset_key()), [25.0; 2]);
                    ui.label(RichText::new("Friendly Fleet Detected").strong());
                });
            }

            let show_combat_details = report.combat_report.is_some()
                && report.can_see(&Side::Attacker, player.id)
                && report.can_see(&Side::Defender, player.id);
            let show_raid_loot =
                report.mission.objective == Icon::Raid && !report.is_space_fauna_encounter();
            let mut scouts_top = army_row.bottom() + 8.0;
            if show_raid_loot {
                let mut loot_ui = ui.new_child(
                    UiBuilder::new()
                        .id_salt("raid loot")
                        .max_rect(egui::Rect::from_min_size(
                            army_row.left_bottom() + egui::vec2(0.0, MISSION_REPORT_LOOT_TOP_GAP),
                            egui::vec2(ui.available_width(), CRAWLER_SALVAGE_ICON_SIZE[1]),
                        ))
                        .layout(Layout::top_down(Align::Min)),
                );
                draw_raid_loot(&mut loot_ui, report.raid_loot, images);
                scouts_top = loot_ui.min_rect().bottom() + 8.0;
            }
            if report.scout_probes > 0 && report.can_see(&Side::Attacker, player.id) {
                // Keep scout notes below the loot instead of pushing it into the details button.
                let mut scouts_ui = ui.new_child(
                    UiBuilder::new()
                        .id_salt("report scouts")
                        .max_rect(egui::Rect::from_min_size(
                            egui::pos2(army_row.left(), scouts_top),
                            egui::vec2(140.0, 18.0),
                        ))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                scouts_ui.spacing_mut().item_spacing.x = 4.0;
                let icon = scouts_ui.add_image(images.get(Icon::Spy.to_lowername()), [15.0; 2]);
                let label = scouts_ui.small(format!("Scouts: {}", report.scout_probes));
                (icon | label).on_hover_small_ext(
                    "Number of attacking Probes that left combat after the first round.",
                );
            }
            if show_combat_details {
                // Keep the details button anchored to the panel below the report content.
                let footer = egui::Rect::from_min_max(
                    egui::pos2(
                        ui.min_rect().left(),
                        panel_rect.bottom() - MISSION_REPORT_FOOTER_BOTTOM_INSET - 50.0,
                    ),
                    egui::pos2(
                        panel_rect.right() - MISSION_REPORT_FOOTER_RIGHT_INSET,
                        panel_rect.bottom() - MISSION_REPORT_FOOTER_BOTTOM_INSET,
                    ),
                );
                let mut button_ui = ui.new_child(
                    UiBuilder::new().max_rect(footer).layout(Layout::right_to_left(Align::Center)),
                );
                let button = button_ui.add_custom_button("Combat details", images);
                if button.clicked() {
                    state.combat_report = Some(report.id);
                    state.combat_report_round = 1;
                }
            }
        });
    });
}

const JOINT_ATTACK_TOAST_ACCENT: Color32 = Color32::from_rgb(112, 190, 255);

/// Keeps raid resources on one row with the top resource panel's border and named hovers.
fn draw_raid_loot(ui: &mut Ui, loot: Resources, images: &ImageIds) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = CRAWLER_SALVAGE_ICON_VALUE_GAP;
        for (index, resource) in CRAWLER_SALVAGE_RESOURCE_ORDER.into_iter().enumerate() {
            if index > 0 {
                ui.add_space(CRAWLER_SALVAGE_RESOURCE_GAP - CRAWLER_SALVAGE_ICON_VALUE_GAP);
            }
            let (rect, response) =
                ui.allocate_exact_size(CRAWLER_SALVAGE_ICON_SIZE.into(), Sense::hover());
            paint_bordered_resource_image(ui, images.get(resource.to_lowername()), rect, 2.0);
            response.on_hover_small(resource.to_name());
            ui.add(egui::Label::new(format_thousands(loot.get(&resource))).extend());
        }
    });
}

pub(super) fn joint_attack_toast_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            RichText::new(label).size(17.0).strong().color(ABANDON_CONFIRMATION_TEXT_COLOR),
        )
        .min_size(egui::vec2(0.0, MODAL_BUTTON_HEIGHT)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

pub(super) fn joint_attack_toast<R>(
    ui: &mut Ui,
    text: impl Into<String>,
    buttons: impl FnOnce(&mut Ui) -> R,
) -> R {
    let text = RichText::new(text.into()).small().color(JOINT_ATTACK_TOAST_ACCENT);
    let text_width = egui::WidgetText::from(text.clone())
        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, TextStyle::Small)
        .size()
        .x
        .ceil();
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(28, 36, 48, 235))
        .stroke(Stroke::new(1.0, JOINT_ATTACK_TOAST_ACCENT))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            // Fit the sentence on one line whenever the viewport has room for it.
            ui.set_width(text_width.min(ui.available_width()));
            // Wrapped labels can be narrower than the toast; anchor them to its left padding.
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.add(egui::Label::new(text).halign(Align::Min).wrap());
            });
            ui.add_space(3.0);
            style_modal_buttons(ui);
            buttons(ui)
        })
        .inner
}

/// Explains the first blocking condition on Accept, including when no fleet is selected.
fn joint_attack_accept_error(
    mission: &Mission,
    map: &Map,
    player: &Player,
    update_pending: bool,
) -> Option<&'static str> {
    let destination = map.get(mission.destination);
    if player.owns(destination) || player.controls(destination) {
        Some("You cannot attack your own planet.")
    } else if destination.blocks_hostile_action_by(player.id) {
        Some("Recall your protection fleet before joining this attack.")
    } else if !matches!(mission.objective, Icon::Colonize | Icon::Attack | Icon::Destroy)
        || !Icon::objectives(
            player.owns(destination),
            player.controls(destination),
            destination.allows_protection(player.id),
            destination.blocks_hostile_action_by(player.id),
        )
        .contains(&mission.objective)
        || (destination.is_moon() && mission.objective.on_planet_only())
    {
        Some("Choose a valid allied objective for this destination.")
    } else if !mission.army.has_army() {
        Some("No ships selected for the mission.")
    // The inviter supplies the objective-specific Colony Ship or War Sun. Supporters
    // may contribute any fleet made entirely of ships.
    } else if mission.army.iter().any(|(unit, count)| *count > 0 && !unit.is_ship()) {
        Some("Only ships can join an allied mission.")
    } else if mission.fuel_consumption(map) > player.resources.deuterium {
        Some("Not enough fuel (deuterium) for the mission.")
    } else if update_pending {
        Some("Updating your fleet. Please wait.")
    } else {
        None
    }
}

/// Width needed from the first player-name pixel through the status badge.
fn joint_attack_roster_content_width(
    ui: &Ui,
    invitation: Option<&JointAttackInvitation>,
    selected_invitees: Option<&std::collections::BTreeSet<PlayerId>>,
    session: &MultiplayerSession,
    viewer: PlayerId,
) -> f32 {
    let participants = joint_attack_roster_participants(invitation, selected_invitees, viewer);
    let name_font = egui::FontId::proportional(JOINT_ATTACK_ROSTER_NAME_SIZE);
    let status_font = egui::FontId::proportional(JOINT_ATTACK_ROSTER_STATUS_SIZE);
    let longest_name_width = participants
        .iter()
        .map(|participant| {
            let name = session
                .player_name(participant.player_id)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Player {}", participant.player_id));
            ui.painter().layout_no_wrap(name, name_font.clone(), Color32::WHITE).size().x
        })
        .fold(0.0_f32, f32::max);
    let badge_width = ["Accepted", "Pending", "Rejected"]
        .into_iter()
        .map(|status| {
            ui.painter()
                .layout_no_wrap(status.to_owned(), status_font.clone(), Color32::WHITE)
                .size()
                .x
                + 12.0
        })
        .fold(0.0_f32, f32::max);
    let strength_width = participants
        .iter()
        .map(|participant| {
            let strength =
                participant.contribution.as_ref().map_or(0, |item| fleet_strength(&item.army));
            ui.painter()
                .layout_no_wrap(strength.to_string(), name_font.clone(), Color32::WHITE)
                .size()
                .x
        })
        .fold(0.0_f32, f32::max);

    longest_name_width
        + JOINT_ATTACK_ROSTER_NAME_GAP
        + JOINT_ATTACK_ROSTER_FLEET_ICON_SIZE
        + JOINT_ATTACK_ROSTER_STRENGTH_GAP
        + strength_width
        + JOINT_ATTACK_ROSTER_BADGE_GAP
        + badge_width
}

fn joint_attack_roster_participants(
    invitation: Option<&JointAttackInvitation>,
    selected_invitees: Option<&std::collections::BTreeSet<PlayerId>>,
    viewer: PlayerId,
) -> Vec<JointAttackParticipant> {
    let mut others = invitation
        .into_iter()
        .flat_map(|invitation| &invitation.participants)
        .filter(|participant| participant.player_id != viewer)
        .cloned()
        .collect::<Vec<_>>();
    if let Some(selected_invitees) = selected_invitees {
        for player_id in selected_invitees {
            if !others.iter().any(|participant| participant.player_id == *player_id) {
                others.push(JointAttackParticipant {
                    player_id: *player_id,
                    response: JointAttackResponse::Pending,
                    contribution: None,
                });
            }
        }
    }
    others
}

/// Keep the footer compact; hover to inspect every invited fleet.
fn draw_joint_attack_player_summary(
    ui: &mut Ui,
    rect: egui::Rect,
    invitation: Option<&JointAttackInvitation>,
    selected_invitees: Option<&std::collections::BTreeSet<PlayerId>>,
    session: &MultiplayerSession,
    viewer: PlayerId,
    images: &ImageIds,
) {
    let others = joint_attack_roster_participants(invitation, selected_invitees, viewer);
    let response = ui.interact(rect, ui.id().with("joint attack players"), Sense::hover());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Label, ui.is_enabled(), "Joint attack players")
    });
    let response = response.on_hover_cursor(CursorIcon::Default);
    let pill = rect.shrink2(egui::vec2(0.0, 5.0));
    ui.scope(|ui| {
        style_selection_boxes(ui, None);
        let visuals = if response.hovered() {
            &ui.visuals().widgets.hovered
        } else {
            &ui.visuals().widgets.inactive
        };
        ui.painter().rect_filled(pill, visuals.corner_radius, visuals.bg_fill);
        ui.painter().rect_stroke(
            pill,
            visuals.corner_radius,
            visuals.bg_stroke,
            StrokeKind::Inside,
        );
    });
    ui.painter().text(
        rect.center() - egui::vec2(0.0, 6.0),
        egui::Align2::CENTER_CENTER,
        format!("Players ({})", others.len()),
        egui::FontId::proportional(13.0),
        ui.visuals().text_color(),
    );
    for (index, participant) in others.iter().enumerate() {
        let (_, color) = joint_attack_response_label(participant.response);
        ui.painter().circle_filled(
            rect.center()
                + egui::vec2((index as f32 - (others.len() - 1) as f32 * 0.5) * 12.0, 9.0),
            3.0,
            color,
        );
    }
    response.on_hover_ui(|ui| {
        let desired_width =
            joint_attack_roster_content_width(ui, invitation, selected_invitees, session, viewer);
        let width = desired_width.min(460.0).min(ui.ctx().content_rect().width() - 32.0);
        ui.set_width(width);
        let (roster, _) =
            ui.allocate_exact_size(egui::vec2(width, others.len() as f32 * 28.0), Sense::hover());
        paint_owner_joint_attack_roster(
            ui,
            roster.left(),
            roster.bottom(),
            roster.right(),
            invitation,
            selected_invitees,
            session,
            viewer,
            images,
        );
    });
}

fn joint_attack_response_label(response: JointAttackResponse) -> (&'static str, Color32) {
    match response {
        JointAttackResponse::Accepted => ("Accepted", Color32::from_rgb(121, 200, 158)),
        JointAttackResponse::Pending => ("Pending", Color32::from_rgb(229, 190, 107)),
        JointAttackResponse::Rejected => ("Rejected", Color32::from_rgb(225, 120, 120)),
    }
}

/// Aligned, larger roster shared by the owner and invitee popovers.
fn paint_owner_joint_attack_roster(
    ui: &mut Ui,
    name_x: f32,
    row_bottom: f32,
    right: f32,
    invitation: Option<&JointAttackInvitation>,
    selected_invitees: Option<&std::collections::BTreeSet<PlayerId>>,
    session: &MultiplayerSession,
    viewer: PlayerId,
    images: &ImageIds,
) {
    let others = joint_attack_roster_participants(invitation, selected_invitees, viewer);
    let painter = ui.painter();
    let name_font = egui::FontId::proportional(JOINT_ATTACK_ROSTER_NAME_SIZE);
    let status_font = egui::FontId::proportional(JOINT_ATTACK_ROSTER_STATUS_SIZE);
    let fleet_icon_size = JOINT_ATTACK_ROSTER_FLEET_ICON_SIZE;
    let name_gap = JOINT_ATTACK_ROSTER_NAME_GAP;
    let strength_gap = JOINT_ATTACK_ROSTER_STRENGTH_GAP;
    let badge_gap = JOINT_ATTACK_ROSTER_BADGE_GAP;
    let badge_width = ["Accepted", "Pending", "Rejected"]
        .into_iter()
        .map(|status| {
            painter.layout_no_wrap(status.to_owned(), status_font.clone(), Color32::WHITE).size().x
                + 12.0
        })
        .fold(0.0_f32, f32::max);
    let strength_width = others
        .iter()
        .map(|participant| {
            let strength =
                participant.contribution.as_ref().map_or(0, |item| fleet_strength(&item.army));
            painter.layout_no_wrap(strength.to_string(), name_font.clone(), Color32::WHITE).size().x
        })
        .fold(0.0_f32, f32::max);
    let longest_name_width = others
        .iter()
        .map(|participant| {
            let name = session
                .player_name(participant.player_id)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Player {}", participant.player_id));
            painter.layout_no_wrap(name, name_font.clone(), Color32::WHITE).size().x
        })
        .fold(0.0_f32, f32::max);
    let maximum_fleet_x =
        right - badge_width - badge_gap - strength_width - strength_gap - fleet_icon_size;
    let fleet_x = (name_x + longest_name_width + name_gap).min(maximum_fleet_x).max(name_x);
    let badge_x = fleet_x + fleet_icon_size + strength_gap + strength_width + badge_gap;
    let name_width = (fleet_x - name_gap - name_x).max(0.0);
    for (index, participant) in others.iter().enumerate() {
        let bottom = row_bottom - (others.len() - index - 1) as f32 * 28.0;
        let name = session
            .player_name(participant.player_id)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Player {}", participant.player_id));
        let strength =
            participant.contribution.as_ref().map_or(0, |item| fleet_strength(&item.army));
        let strength_label = strength.to_string();
        let name_color = session.player_color(participant.player_id).color().to_color32();
        let (status, color) = joint_attack_response_label(participant.response);
        let fleet_icon = egui::Rect::from_min_size(
            egui::pos2(fleet_x, bottom - 15.0),
            egui::Vec2::splat(fleet_icon_size),
        );
        let strength_x = fleet_icon.right() + strength_gap;
        let badge = egui::Rect::from_min_size(
            egui::pos2(badge_x, bottom - 21.0),
            egui::vec2(badge_width, 21.0),
        );
        painter
            .with_clip_rect(egui::Rect::from_min_max(
                egui::pos2(name_x, bottom - 24.0),
                egui::pos2(name_x + name_width, bottom),
            ))
            .text(
                egui::pos2(name_x, bottom),
                egui::Align2::LEFT_BOTTOM,
                name,
                name_font.clone(),
                name_color,
            );
        painter.image(
            images.get("fleet"),
            fleet_icon,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::WHITE,
        );
        painter.text(
            egui::pos2(strength_x, bottom),
            egui::Align2::LEFT_BOTTOM,
            strength_label,
            name_font.clone(),
            Color32::WHITE,
        );
        painter.rect_filled(badge, 4.0, color.gamma_multiply(0.12));
        painter.rect_stroke(
            badge,
            4.0,
            Stroke::new(1.0, color.gamma_multiply(0.65)),
            StrokeKind::Inside,
        );
        painter.text(
            badge.center(),
            egui::Align2::CENTER_CENTER,
            status,
            status_font.clone(),
            color,
        );
    }
}

/// Shows other players' published fleets in a compact roster.
#[cfg(test)]
fn draw_joint_attack_strengths(
    ui: &mut Ui,
    width: f32,
    invitation: &JointAttackInvitation,
    session: &MultiplayerSession,
    viewer: PlayerId,
    bottom_aligned: bool,
) {
    let participants = invitation
        .participants
        .iter()
        .filter(|participant| {
            participant.player_id != viewer && participant.response != JointAttackResponse::Rejected
        })
        .collect::<Vec<_>>();
    let height = if bottom_aligned {
        ui.available_height()
    } else {
        0.0
    };
    let layout = if bottom_aligned {
        Layout::bottom_up(Align::Min)
    } else {
        Layout::top_down(Align::Min)
    };
    ui.allocate_ui_with_layout(egui::vec2(width, height), layout, |ui| {
        ui.set_width(width);
        ui.spacing_mut().item_spacing.y = 2.0;
        let draw_row = |ui: &mut Ui, participant: &JointAttackParticipant| {
            let name = session
                .player_name(participant.player_id)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Player {}", participant.player_id));
            let strength =
                participant.contribution.as_ref().map_or(0, |item| fleet_strength(&item.army));
            let (status, color) = match participant.response {
                JointAttackResponse::Accepted => ("Accepted", Color32::from_rgb(121, 200, 158)),
                JointAttackResponse::Pending => ("Pending", Color32::from_rgb(229, 190, 107)),
                JointAttackResponse::Rejected => return,
            };
            let strength_text = format!(" ({strength})");
            let name_color = session.player_color(participant.player_id).color().to_color32();
            let font = egui::FontId::proportional(12.0);
            let status_font = egui::FontId::proportional(11.0);
            let status_width =
                ui.painter().layout_no_wrap(status.to_owned(), status_font, color).size().x + 12.0;
            let strength_width = ui
                .painter()
                .layout_no_wrap(strength_text.clone(), font.clone(), Color32::WHITE)
                .size()
                .x;
            let name_width = ui
                .painter()
                .layout_no_wrap(name.clone(), font, name_color)
                .size()
                .x
                .min((width - status_width - strength_width - 6.0).max(0.0));
            ui.allocate_ui_with_layout(
                egui::vec2(width, 18.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_sized(
                        [name_width, 17.0],
                        egui::Label::new(RichText::new(name.clone()).size(12.0).color(name_color))
                            .truncate(),
                    )
                    .on_hover_text(format!("{name}{strength_text}"));
                    ui.label(RichText::new(strength_text).size(12.0).color(Color32::WHITE));
                    ui.add_space(3.0);
                    egui::Frame::new()
                        .fill(color.gamma_multiply(0.12))
                        .stroke(Stroke::new(1.0, color.gamma_multiply(0.65)))
                        .corner_radius(4.0)
                        .inner_margin(egui::Margin::symmetric(5, 1))
                        .show(ui, |ui| {
                            ui.label(RichText::new(status).size(11.0).strong().color(color));
                        });
                },
            );
        };
        if bottom_aligned {
            for participant in participants.into_iter().rev() {
                draw_row(ui, participant);
            }
        } else {
            for participant in participants {
                draw_row(ui, participant);
            }
        }
    });
}

/// Keeps the compact mission panel and its launch footer inside the viewport.
pub(super) fn mission_panel_size(viewport: egui::Vec2) -> egui::Vec2 {
    egui::vec2(
        850.0_f32.min((viewport.x - 32.0).max(0.0)),
        MISSION_PANEL_HEIGHT.min((viewport.y - 32.0).max(0.0)),
    )
}

pub(super) fn mission_panel_scale(viewport: egui::Vec2) -> f32 {
    viewport_ui_scale(viewport)
        .min((viewport.x - 16.0).max(1.0) / 850.0)
        .min((viewport.y - 16.0).max(1.0) / MISSION_PANEL_HEIGHT)
        .max(0.001)
}

fn draw_joint_attack_response_panel(
    context: &egui::Context,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    requests: &mut MessageWriter<MultiplayerRequest>,
    images: &ImageIds,
    invitation: &JointAttackInvitation,
    _destination: &Planet,
    participant_response: JointAttackResponse,
) {
    let viewport = context.content_rect().size();
    let scale = mission_panel_scale(viewport);
    let size = mission_panel_size(viewport / scale);
    let modal_id = egui::Id::new(("joint attack response", invitation.id));
    let editable = !invitation.canceled && !invitation.launched;
    state.joint_attack_contribution.destination = invitation.destination;
    state.joint_attack_contribution.objective = invitation.objective;
    let response = scaled_modal(context, images, modal_id, size, scale, |ui, panel, _content| {
        ui.painter().text(
            egui::pos2(panel.center().x, panel.top() + 32.0),
            egui::Align2::CENTER_CENTER,
            "Joint Attack",
            egui::FontId::proportional(16.0),
            Color32::WHITE,
        );
        let destination = map.get(state.joint_attack_contribution.destination);
        let origin = map.get(state.joint_attack_contribution.origin);
        let available = mission_origin_army_after_allied_reservations(
            map,
            session,
            player.id,
            origin.id,
            Some(invitation.id),
        );
        state.joint_attack_contribution = Mission::from_mission(
            invitation.turn as usize,
            player.id,
            origin,
            destination,
            &state.joint_attack_contribution,
        );
        let badges_width = mission_badges_width(
            ui,
            &state.joint_attack_contribution.army,
            Some(state.joint_attack_contribution.battle_plan),
        );
        let footer = mission_footer_layout(
            panel.shrink(10.0),
            MISSION_INVITE_ICON_SIZE + 12.0 + MISSION_PLAYERS_SUMMARY_WIDTH,
            badges_width,
            true,
        );
        ui.scope_builder(UiBuilder::new().max_rect(panel), |ui| {
            // Reserve the tab row's height while leaving it visually empty.
            ui.add_space(17.0);
            ui.add_space(ui.text_style_height(&TextStyle::Body) + 18.0);
            ui.add_space(10.0);
            ScrollArea::horizontal()
                .id_salt(("joint attack route", invitation.id))
                .max_width(panel.width())
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.horizontal_top(|ui| {
                        ui.add_space(((panel.width() - 580.0) * 0.5).max(8.0));
                        ui.add_enabled_ui(editable, |ui| {
                            draw_mission_route(
                                ui,
                                &mut state.joint_attack_contribution,
                                map,
                                player,
                                &[],
                                session,
                                &available,
                                &Unit::ships(),
                                images,
                                true,
                                None,
                            );
                        });
                    });
                });
            ui.add_space(-10.0);
            ui.add(Separator::default().shrink(70.0));
            let destination = map.get(state.joint_attack_contribution.destination);
            let origin = map.get(state.joint_attack_contribution.origin);
            super::battle_plan::initialize_mission_plan(
                &mut state.joint_attack_contribution,
                origin,
                player.id,
                &mut state.joint_battle_plan_origin,
            );
            let available = mission_origin_army_after_allied_reservations(
                map,
                session,
                player.id,
                origin.id,
                Some(invitation.id),
            );
            state.joint_attack_contribution = Mission::from_mission(
                invitation.turn as usize,
                player.id,
                origin,
                destination,
                &state.joint_attack_contribution,
            );
            ScrollArea::both()
                .id_salt(("joint attack fleet and details", invitation.id))
                .max_width(panel.width())
                .max_height((footer.top - ui.cursor().top() - 20.0).max(0.0))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(panel.width());
                    ui.horizontal_top(|ui| {
                        ui.add_space(130.0);
                        ui.vertical(|ui| {
                            ui.set_width(280.0);
                            ui.add_enabled_ui(editable, |ui| {
                                draw_mission_fleet_picker(
                                    ui,
                                    &mut state.joint_attack_contribution.army,
                                    &available,
                                    &Unit::ships(),
                                    images,
                                );
                            });
                        });
                        ui.add_space(15.0);
                        ui.vertical(|ui| {
                            ui.set_width(330.0);
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = MISSION_OBJECTIVE_BUTTON_GAP;
                                for icon in Icon::objectives(
                                    player.owns(destination),
                                    player.controls(destination),
                                    destination.allows_protection(player.id),
                                    destination.blocks_hostile_action_by(player.id),
                                ) {
                                    ui.add_enabled_ui(false, |ui| {
                                        draw_mission_objective_button(
                                            ui,
                                            images.get(icon.asset_key()),
                                            icon,
                                            invitation.objective == icon,
                                        );
                                    });
                                }
                            });
                            ui.add_space(5.0);
                            ui.add_enabled_ui(editable, |ui| {
                                draw_mission_details(
                                    ui,
                                    &mut state.joint_attack_contribution,
                                    map,
                                    player,
                                    invitation.turn as usize,
                                    images,
                                    Some(invitation),
                                );
                            });
                        });
                    });
                });
        });
        let destination = map.get(state.joint_attack_contribution.destination);
        let error = if destination.owned == Some(invitation.inviter)
            || destination.controlled == Some(invitation.inviter)
        {
            Some("The mission owner cannot target their own planet.")
        } else {
            joint_attack_accept_error(
                &state.joint_attack_contribution,
                map,
                player,
                session.joint_attack_update_pending,
            )
        };
        let draft = JointAttackContribution {
            player_id: player.id,
            origin: state.joint_attack_contribution.origin,
            army: state.joint_attack_contribution.army.clone(),
            bombing: state.joint_attack_contribution.bombing.clone(),
            combat_probes: state.joint_attack_contribution.combat_probes,
            battle_plan: state.joint_attack_contribution.battle_plan,
        };
        let changed = invitation
            .participants
            .iter()
            .find(|item| item.player_id == player.id)
            .and_then(|item| item.contribution.as_ref())
            != Some(&draft);
        let mut action = None;
        let icon = egui::Rect::from_min_size(
            footer.players.left_center() - egui::vec2(0.0, MISSION_INVITE_ICON_SIZE * 0.5),
            egui::Vec2::splat(MISSION_INVITE_ICON_SIZE),
        );
        // This identifies the shared mission; only its owner can invite players.
        ui.painter().image(
            images.get("allied attack"),
            icon,
            egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
            Color32::from_gray(200),
        );
        draw_joint_attack_player_summary(
            ui,
            egui::Rect::from_min_max(
                egui::pos2(icon.right() + 12.0, footer.players.top()),
                footer.players.right_bottom(),
            ),
            Some(invitation),
            None,
            session,
            player.id,
            images,
        );
        let draw_button =
            |ui: &mut Ui, rect: egui::Rect, label: &str, enabled: bool, reason: Option<&str>| {
                ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
                    ui.add_enabled_ui(enabled, |ui| {
                        let mut response = ui.add_custom_button(label, images);
                        if let Some(reason) = reason {
                            response = response.on_disabled_hover_small(reason);
                        }
                        response
                    })
                    .inner
                })
                .inner
            };
        let fleet_icon = draw_mission_strength_badge(
            ui,
            &state.joint_attack_contribution.army,
            images,
            footer.badges.center().y,
            footer.badges.right(),
        );
        ui.add_enabled_ui(editable, |ui| {
            draw_mission_plan_badge(
                ui,
                images,
                fleet_icon,
                &mut state.joint_attack_contribution.battle_plan,
                &state.joint_attack_contribution.army,
            );
        });
        let send = draw_button(
            ui,
            footer.send,
            if changed {
                "Send proposal"
            } else {
                "Accept"
            },
            editable
                && (participant_response == JointAttackResponse::Pending || changed)
                && error.is_none(),
            error,
        );
        if send.clicked() {
            action = Some(JointAttackResponse::Accepted);
        }
        if draw_button(
            ui,
            footer.secondary.unwrap_or(footer.send),
            "Reject",
            editable && !session.joint_attack_update_pending,
            None,
        )
        .clicked()
        {
            action = Some(JointAttackResponse::Rejected);
        }
        (action, draft, changed)
    });

    let mut close = response.should_close();
    let (action, draft, changed) = response.inner;
    let published = invitation
        .participants
        .iter()
        .find(|item| item.player_id == player.id)
        .and_then(|item| item.contribution.as_ref());
    if let Some(action) = action {
        requests.write(MultiplayerRequest::RespondJointAttack {
            expected_revision: invitation.revision,
            attack_id: invitation.id,
            response: action,
            contribution: (action != JointAttackResponse::Rejected).then_some(draft),
        });
        if action == JointAttackResponse::Accepted && changed {
            state.joint_attack_proposal_notice = true;
        }
        close |= action == JointAttackResponse::Rejected;
    } else if editable
        && !session.joint_attack_update_pending
        && participant_response == JointAttackResponse::Accepted
        && changed
    {
        // Revoke the old accepted fleet once editing begins. Fleet adjustments remain local
        // until the player explicitly sends the replacement proposal.
        requests.write(MultiplayerRequest::RespondJointAttack {
            expected_revision: invitation.revision,
            attack_id: invitation.id,
            response: JointAttackResponse::Pending,
            contribution: published.cloned(),
        });
    }
    if close {
        state.joint_attack_open = None;
    }
}

/// Draws persistent actionable invitations in normal toast chrome and opens the response panel.
pub(super) fn draw_joint_attack_notifications(
    context: &egui::Context,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    requests: &mut MessageWriter<MultiplayerRequest>,
    messages: &mut MessageWriter<MessageMsg>,
    images: &ImageIds,
) {
    if !session.joint_attacks_enabled() {
        state.joint_attack_open = None;
        state.joint_attack_loaded = None;
        state.joint_attack_proposal_notice = false;
        return;
    }
    if std::mem::take(&mut state.joint_attack_proposal_notice) {
        messages.write(
            MessageMsg::info("Joint attack proposal sent.").with_duration(
                std::time::Duration::from_secs(JOINT_ATTACK_PROPOSAL_NOTICE_SECONDS),
            ),
        );
    }
    if !mission_panel_visible(state) || state.mission_tab != MissionTab::NewMission {
        if let Some(draft) = state.joint_attack_owner_draft.take() {
            // Keep the owner's unsent edits when the editor closes. Map navigation may have
            // changed mission_info, so restore it after checking the owner draft.
            let previous = std::mem::replace(&mut state.mission_info, draft);
            let invitation = state.joint_attack_draft_id.and_then(|id| {
                session
                    .joint_attacks
                    .iter()
                    .find(|item| item.id == id && !item.canceled && !item.launched)
            });
            if invitation.is_some() {
                let turn = session
                    .active_game
                    .as_ref()
                    .map_or(0, |game| game.persisted.state.turn as usize);
                sync_allied_mission(state, turn, player, session, requests, invitation, None);
            } else if !session.joint_attack_update_pending {
                state.joint_attack_draft_id = None;
                state.joint_attack_invitees.clear();
                state.allied_mission = false;
            }
            let draft = std::mem::replace(&mut state.mission_info, previous);
            state.joint_attack_owner_draft = state.joint_attack_draft_id.map(|_| draft);
        }
    }
    for invitation in &session.joint_attacks {
        if invitation.canceled && invitation.inviter != player.id {
            let participated = invitation
                .participants
                .iter()
                .any(|participant| participant.player_id == player.id);
            if participated
                && first_negotiation_notice(
                    context,
                    session,
                    player.id,
                    NegotiationNotice::MissionCanceled(invitation.id),
                )
            {
                let inviter_name = session
                    .player_name(invitation.inviter)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Player {}", invitation.inviter));
                let target_name = map
                    .try_get(invitation.destination)
                    .map(|planet| planet.name.as_str())
                    .unwrap_or("an unknown planet");
                messages.write(MessageMsg::info(format!(
                    "{inviter_name} canceled the allied attack on {target_name}."
                )));
            }
        }
        if invitation.inviter != player.id {
            continue;
        }
        let target_name = map
            .try_get(invitation.destination)
            .map(|planet| planet.name.as_str())
            .unwrap_or("an unknown planet");
        for rejected in invitation.participants.iter().filter(|participant| {
            !invitation.canceled && participant.response == JointAttackResponse::Rejected
        }) {
            if first_negotiation_notice(
                context,
                session,
                player.id,
                NegotiationNotice::MissionRejected(invitation.id, rejected.player_id),
            ) {
                let name = session
                    .player_name(rejected.player_id)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Player {}", rejected.player_id));
                messages.write(MessageMsg::info(format!(
                    "{name} rejected the joint attack on {target_name}."
                )));
            }
        }
    }

    show_notification_area(
        context,
        "joint_attack_notifications",
        true,
        context.content_rect().width(),
        |ui| {
            for invitation in &session.joint_attacks {
                if invitation.canceled || invitation.launched {
                    continue;
                }
                let Some(participant) = invitation
                    .participants
                    .iter()
                    .find(|participant| participant.player_id == player.id)
                else {
                    continue;
                };
                if state.joint_attack_open == Some(invitation.id)
                    || participant.response == JointAttackResponse::Rejected
                    || (participant.response == JointAttackResponse::Accepted
                        && mission_panel_visible(state))
                {
                    continue;
                }
                let inviter_name = session
                    .player_name(invitation.inviter)
                    .map(str::to_owned)
                    .unwrap_or_else(|| format!("Player {}", invitation.inviter));
                let target_name = map
                    .try_get(invitation.destination)
                    .map(|planet| planet.name.as_str())
                    .unwrap_or("an unknown planet");
                if invitation.inviter == player.id {
                    if mission_panel_visible(state) {
                        continue;
                    }
                    joint_attack_toast(
                        ui,
                        format!(
                            "Your allied {} mission on {target_name} is waiting.",
                            invitation.objective.to_lowername()
                        ),
                        |ui| {
                            if joint_attack_toast_button(ui, "Reopen mission", true).clicked() {
                                restore_joint_attack_owner_draft(state, invitation);
                                state.mission = true;
                                state.mission_tab = MissionTab::NewMission;
                                state.joint_attack_open = None;
                            }
                        },
                    );
                } else {
                    let article = if invitation.objective == Icon::Attack {
                        "an"
                    } else {
                        "a"
                    };
                    let (text, open_label) = match participant.response {
                        JointAttackResponse::Pending => (
                            format!(
                                "{inviter_name} invited you to join {article} {} mission on {target_name}.",
                                invitation.objective.to_lowername()
                            ),
                            "Open",
                        ),
                        JointAttackResponse::Accepted => (
                            format!(
                                "You joined {inviter_name}'s {} mission on {target_name}.",
                                invitation.objective.to_lowername()
                            ),
                            "Reopen",
                        ),
                        JointAttackResponse::Rejected => continue,
                    };
                    joint_attack_toast(ui, text, |ui| {
                        ui.with_layout(
                            Layout::right_to_left(Align::Center).with_main_wrap(true),
                            |ui| {
                                if joint_attack_toast_button(ui, open_label, true).clicked() {
                                    state.joint_attack_open = Some(invitation.id);
                                }
                                if joint_attack_toast_button(
                                    ui,
                                    "Reject",
                                    !session.joint_attack_update_pending,
                                )
                                .clicked()
                                {
                                    requests.write(MultiplayerRequest::RespondJointAttack {
                                        expected_revision: invitation.revision,
                                        attack_id: invitation.id,
                                        response: JointAttackResponse::Rejected,
                                        contribution: None,
                                    });
                                }
                            },
                        );
                    });
                }
            }
        },
    );

    let Some(open_id) = state.joint_attack_open else {
        return;
    };
    let Some(invitation) = session.joint_attacks.iter().find(|item| item.id == open_id).cloned()
    else {
        state.joint_attack_open = None;
        return;
    };
    if invitation.canceled || invitation.launched {
        state.joint_attack_open = None;
        return;
    }
    let Some(destination) = map.try_get(invitation.destination) else {
        state.joint_attack_open = None;
        return;
    };
    let participant =
        invitation.participants.iter().find(|participant| participant.player_id == player.id);
    let Some(participant) = participant else {
        state.joint_attack_open = None;
        return;
    };
    let participant_response = participant.response;
    if participant_response == JointAttackResponse::Rejected {
        state.joint_attack_open = None;
        return;
    }
    let restore_draft = state.joint_attack_loaded != Some(invitation.id);
    if restore_draft {
        state.joint_attack_loaded = Some(invitation.id);
        state.joint_attack_contribution = Mission {
            origin: player.home_planet,
            destination: invitation.destination,
            objective: invitation.objective,
            ..default()
        };
        state.joint_battle_plan_origin = None;
    }
    if restore_draft {
        if let Some(contribution) = participant.contribution.as_ref() {
            state.joint_attack_contribution.origin = contribution.origin;
            state.joint_attack_contribution.army = contribution.army.clone();
            state.joint_attack_contribution.bombing = contribution.bombing.clone();
            state.joint_attack_contribution.combat_probes = contribution.combat_probes;
            state.joint_attack_contribution.battle_plan = contribution.battle_plan;
            state.joint_battle_plan_origin = Some(contribution.origin);
        } else if !joint_origin_has_ships(
            map,
            session,
            player.id,
            state.joint_attack_contribution.origin,
            invitation.id,
        ) {
            // The home world is only a default. Open an invitation at a world that can
            // actually contribute ships when the player has not offered a fleet yet.
            if let Some(origin) = map.planets.iter().find(|planet| {
                planet.can_launch_mission(player.id)
                    && joint_origin_has_ships(map, session, player.id, planet.id, invitation.id)
            }) {
                state.joint_attack_contribution.origin = origin.id;
            }
        }
    }
    state.joint_attack_contribution.destination = invitation.destination;
    state.joint_attack_contribution.objective = invitation.objective;
    if !map
        .try_get(state.joint_attack_contribution.origin)
        .is_some_and(|origin| origin.can_launch_mission(player.id))
    {
        state.joint_attack_contribution.origin = player.home_planet;
        state.joint_attack_contribution.army.clear();
    }
    let origin = map.get(state.joint_attack_contribution.origin);
    super::battle_plan::initialize_mission_plan(
        &mut state.joint_attack_contribution,
        origin,
        player.id,
        &mut state.joint_battle_plan_origin,
    );
    draw_joint_attack_response_panel(
        context,
        state,
        map,
        player,
        session,
        requests,
        images,
        &invitation,
        destination,
        participant_response,
    );
}

/// Restores a shared owner draft without overwriting local edits awaiting publication.
fn restore_joint_attack_owner_draft(state: &mut UiState, invitation: &JointAttackInvitation) {
    if state.joint_attack_draft_id == Some(invitation.id)
        && state.joint_attack_owner_draft.is_some()
    {
        return;
    }
    state.joint_attack_draft_id = Some(invitation.id);
    state.joint_attack_owner_shared_route = Some((invitation.destination, invitation.objective));
    state.allied_mission = true;
    state.joint_attack_invitees =
        invitation.participants.iter().skip(1).map(|item| item.player_id).collect();
    if let Some(contribution) =
        invitation.participants.first().and_then(|item| item.contribution.as_ref())
    {
        state.mission_battle_plan_origin = Some(contribution.origin);
        state.joint_attack_owner_draft = Some(Mission {
            origin: contribution.origin,
            destination: invitation.destination,
            objective: invitation.objective,
            army: contribution.army.clone(),
            bombing: contribution.bombing.clone(),
            combat_probes: contribution.combat_probes,
            battle_plan: contribution.battle_plan,
            ..default()
        });
    }
}

/// Draws the mission interface and emits any resulting local actions.
pub(super) fn draw_mission(
    ui: &mut Ui,
    missions: &[Mission],
    send_mission: &mut MessageWriter<SendMissionMsg>,
    recall_mission: &mut MessageWriter<RecallMissionMsg>,
    settings: &Settings,
    state: &mut UiState,
    map: &mut Map,
    player: &mut Player,
    session: &MultiplayerSession,
    multiplayer_requests: &mut MessageWriter<MultiplayerRequest>,
    is_hovered: bool,
    keyboard: &ButtonInput<KeyCode>,
    images: &ImageIds,
    editable: bool,
) {
    if state.joint_attack_draft_id.is_none() {
        if let Some(invitation) = session.joint_attacks.iter().find(|item| {
            item.inviter == player.id
                && !item.canceled
                && !item.launched
                && !missions.iter().any(|mission| {
                    mission.joint_attack.as_ref().is_some_and(|attack| attack.id == item.id)
                })
        }) {
            restore_joint_attack_owner_draft(state, invitation);
            state.mission_tab = MissionTab::NewMission;
        }
    }
    // Rebuild this transient preview every Egui pass so it cannot outlive the hovered link.
    state.mission_planet_hover = None;

    ui.add_space(17.);
    draw_mission_tabs(ui, &mut state.mission_tab);

    match state.mission_tab {
        MissionTab::NewMission => {
            ui.add_enabled_ui(editable, |ui| {
                draw_new_mission(
                    ui,
                    send_mission,
                    missions,
                    settings,
                    state,
                    map,
                    player,
                    session,
                    multiplayer_requests,
                    is_hovered,
                    keyboard,
                    images,
                )
            });
        },
        MissionTab::ActiveMissions => draw_active_missions(
            ui,
            missions.iter().filter(|m| m.commanded_army(player.id).is_some()).collect(),
            recall_mission,
            state,
            map,
            player,
            session,
            is_hovered,
            images,
            editable,
        ),
        MissionTab::EnemyMissions => draw_active_missions(
            ui,
            missions.iter().filter(|m| m.commanded_army(player.id).is_none()).collect(),
            recall_mission,
            state,
            map,
            player,
            session,
            is_hovered,
            images,
            editable,
        ),
        MissionTab::MissionReports => {
            draw_mission_reports(ui, state, map, player, session, is_hovered, images)
        },
    }
}

#[cfg(test)]
#[path = "../../../../tests/core/ui_missions.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../../tests/core/ui_joint_missions.rs"]
mod joint_tests;

#[cfg(test)]
#[path = "../../../../tests/core/ui_patrols.rs"]
mod patrol_tests;
