//! Missions panels for the game interface.

use super::*;
use crate::core::identity::PlayerId;
use crate::core::messages::show_notification_area;
use crate::core::missions::MissionRouteStyle;

/// Shows the launch-only Energy charge while the draft uses a Jump Gate.
pub(super) fn draw_jump_energy_cost(ui: &mut Ui, mission: &Mission, images: &ImageIds) {
    if mission.jump_gate {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.add_image(images.get("energy"), [30., 21.]);
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
const MISSION_PREVIEW_HELIX_WAVELENGTH: f32 = 60.0;
const MISSION_PREVIEW_HELIX_ANGULAR_SPEED: f32 = 2.0;
const MISSION_RECALL_BUTTON_SIZE: f32 = 32.0;
const MISSION_INVITE_ICON_SIZE: f32 = 40.0;
const MISSION_ROUTE_FIXED_CONTENT_WIDTH: f32 = 122.0;
const MISSION_LOG_BADGE_SIZE: f32 = 20.0;
const MISSION_COLUMN_GAP: f32 = 24.0;
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
const MISSION_REPORT_PLANET_INTEL_WIDTH: f32 = 138.0;
const MISSION_REPORT_INTEL_COLUMN_GAP: f32 = 4.0;
const MISSION_REPORT_INTEL_IMAGE_SIZE: f32 = (MISSION_REPORT_PLANET_INTEL_WIDTH
    - MISSION_REPORT_INTEL_COLUMN_GAP * (MISSION_REPORT_INTEL_COLUMNS - 1) as f32)
    / MISSION_REPORT_INTEL_COLUMNS as f32;
const MISSION_REPORT_INTEL_COLUMNS: usize = 3;
const MISSION_REPORT_INTEL_GROUP_GAP: f32 = 7.0;
const FLEET_MOVEMENT_TOOLTIP: &str = "Distance the fleet will travel next turn. Fleets accelerate \
    each travel turn, so this is not a fixed per-turn speed.";
const JUMP_GATE_MOVEMENT_TOOLTIP: &str =
    "A Jump Gate bypasses normal fleet movement and delivers the fleet in one turn.";

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
    ui.add_enabled_ui(available, |ui| {
        ui.horizontal(|ui| {
            ui.small("Deep Cover:");
            ui.add(toggle(&mut mission.deep_cover));
        });
    })
    .response
    .on_hover_small(format!(
        "Costs {} extra deuterium per Probe. If the origin's completed Command Relay level is \
        higher than the destination's when the mission arrives, all Probes scan and return \
        without combat or alerting the defender. Otherwise, the Probes face normal Spy combat.",
        crate::core::constants::DEEP_COVER_DEUTERIUM_PER_PROBE,
    ))
    .on_disabled_hover_small("Build a Command Relay at the origin to enable Deep Cover.");
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
fn fleet_strength(army: &Army) -> usize {
    army.iter().fold(0_usize, |strength, (unit, count)| {
        if unit.is_ship() {
            strength.saturating_add(count.saturating_mul(unit.production()))
        } else {
            strength
        }
    })
}

/// Returns whether the local player should be offered the active-mission recall action.
fn mission_recall_available(mission: &Mission, player_id: PlayerId) -> bool {
    mission.owner == player_id
        && mission.joint_attack.is_none()
        && !mission.is_returning()
        && mission.objective.is_recallable()
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
    let phase = elapsed * MISSION_PREVIEW_HELIX_ANGULAR_SPEED * direction;
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

/// Draws the same compact route language used by hovered missions on the strategic map.
fn draw_route_preview(
    ui: &mut Ui,
    width: f32,
    mission: &Mission,
    player: &Player,
    color: Color32,
    returning: bool,
) -> Response {
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, MISSION_ROUTE_PREVIEW_HEIGHT), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    // Keep the helix and chevrons inside the preview lane.
    let left = rect.left() + 12.0;
    let right = rect.right() - 8.0;
    let center_y = rect.center().y;
    let time = ui.input(|input| input.time) as f32;
    let speed = mission.route_animation_speed() as f32;
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
        egui::vec2(MISSION_PLANET_COLUMN_WIDTH, MISSION_PLANET_NAME_HEIGHT),
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

/// Consumes pointer clicks over an enemy route without presenting it as a control.
///
/// The mission panel sits above the pickable strategic map. A click-only interaction keeps the
/// route artwork from passing clicks through to a planet behind the panel, while the default
/// cursor communicates that the objective, arrows, and arrival time have no action of their own.
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
    let name = egui::WidgetText::from(RichText::new(name).text_style(TextStyle::Small))
        .into_galley(ui, Some(egui::TextWrapMode::Truncate), name_rect.width(), TextStyle::Small);
    let name_pos = name_rect.center() - name.size() * 0.5;
    ui.painter().galley(name_pos, name, ui.visuals().text_color());
    let name_response = ui.interact(name_rect, ui.next_auto_id(), sense);

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
        state.planet_hover = None;
        state.mission_planet_hover = None;
        state.planet_selected = Some(planet.id);
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

/// Synchronizes confirmed invitations while the owner continues editing their mission.
fn sync_allied_mission(
    context: &egui::Context,
    state: &mut UiState,
    turn: usize,
    player: &Player,
    session: &MultiplayerSession,
    requests: &mut MessageWriter<MultiplayerRequest>,
    invitation: Option<&JointAttackInvitation>,
) -> bool {
    let eligible =
        matches!(state.mission_info.objective, Icon::Colonize | Icon::Attack | Icon::Destroy);
    if !session.has_active_game() {
        return true;
    }
    if !eligible {
        state.allied_mission = false;
        state.joint_attack_invite_selection = None;
    }
    if !state.allied_mission || state.joint_attack_invitees.is_empty() {
        if let Some(invitation) = invitation {
            if !session.joint_attack_update_pending {
                requests.write(MultiplayerRequest::CancelJointAttack {
                    attack_id: invitation.id,
                });
                state.joint_attack_draft_id = None;
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
    };
    let synced = invitation.is_some_and(|invitation| {
        invitation.destination == state.mission_info.destination
            && invitation.objective == state.mission_info.objective
            && invitation.participants.first().and_then(|item| item.contribution.as_ref())
                == Some(&contribution)
            && invitation
                .participants
                .iter()
                .skip(1)
                .map(|item| item.player_id)
                .eq(state.joint_attack_invitees.iter().copied())
    });
    if !synced
        && !session.joint_attack_update_pending
        && !context.input(|input| input.pointer.any_down())
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

/// Stages player choices in the protection-style modal and commits only on confirmation.
fn draw_joint_attack_invite_modal(
    context: &egui::Context,
    state: &mut UiState,
    session: &MultiplayerSession,
    player: &Player,
    destination: &Planet,
    images: &ImageIds,
    editable: bool,
) {
    let Some(mut selected) = state.joint_attack_invite_selection.take() else {
        return;
    };
    let Some(game) = &session.active_game else {
        return;
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
    selected.retain(|id| members.iter().any(|member| member.player_id == *id));
    let available = (context.content_rect().size() - egui::vec2(32.0, 32.0)).max(egui::Vec2::ZERO);
    let size = egui::vec2(
        560.0_f32.min(available.x),
        (270.0 + members.len() as f32 * 49.0).min(available.y),
    );
    let enabled = editable && !session.joint_attack_update_pending;
    let response = show_panel_modal(
        context,
        images,
        egui::Id::new("joint attack invite players"),
        size,
        |ui, rect, content| {
            let header = draw_modal_header(
                ui,
                rect,
                content,
                RichText::new("Invite players")
                    .size(21.0)
                    .strong()
                    .color(ABANDON_CONFIRMATION_TEXT_COLOR),
                images.get("allied attack"),
            );
            let footer = egui::Rect::from_min_size(
                egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
                egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
            );
            let intro = egui::Rect::from_min_max(
                egui::pos2(content.left(), header.bottom() + 12.0),
                egui::pos2(content.right(), (header.bottom() + 70.0).min(footer.top())),
            );
            ui.scope_builder(UiBuilder::new().max_rect(intro), |ui| {
                ui.set_clip_rect(intro);
                ui.vertical_centered(|ui| {
                    ui.small(format!("Invite players to join the attack on {}.", destination.name));
                });
            });
            let list_top = (intro.bottom() + 12.0).min(footer.top());
            let list = egui::Rect::from_min_max(
                egui::pos2(content.left(), list_top),
                egui::pos2(content.right(), (footer.top() - 12.0).max(list_top)),
            );
            ui.scope_builder(UiBuilder::new().max_rect(list), |ui| {
                ui.set_clip_rect(list);
                ScrollArea::vertical()
                    .id_salt("joint attack invite players list")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 7.0;
                        if members.is_empty() {
                            ui.small("No players are available to invite.");
                        }
                        for member in &members {
                            let available_width = ui.available_width();
                            let name_width = ui
                                .painter()
                                .layout_no_wrap(
                                    member.display_name.clone(),
                                    egui::FontId::proportional(17.0),
                                    Color32::WHITE,
                                )
                                .size()
                                .x;
                            let row_width = (name_width + 64.0)
                                .max(PROTECTION_PLAYER_ROW_MIN_WIDTH)
                                .min(available_width);
                            let invited = selected.contains(&member.player_id);
                            let already_invited =
                                state.joint_attack_invitees.contains(&member.player_id);
                            let response = ui
                                .horizontal(|ui| {
                                    ui.add_space(
                                        ((ui.available_width() - row_width) * 0.5).max(0.0),
                                    );
                                    modal_player_row(
                                        ui,
                                        row_width,
                                        42.0,
                                        &member.display_name,
                                        session.player_color(member.player_id).color().to_color32(),
                                        invited,
                                        enabled && !already_invited,
                                    )
                                })
                                .inner;
                            if response.clicked() {
                                if invited {
                                    selected.remove(&member.player_id);
                                } else {
                                    selected.insert(member.player_id);
                                }
                            }
                        }
                    });
            });
            ui.scope_builder(UiBuilder::new().max_rect(footer), |ui| {
                style_modal_buttons(ui);
                let button_width = 120.0_f32.min((footer.width() - 12.0).max(0.0) * 0.5);
                let buttons_width = button_width * 2.0 + 12.0;
                ui.horizontal(|ui| {
                    ui.add_space(((footer.width() - buttons_width) * 0.5).max(0.0));
                    ui.spacing_mut().item_spacing.x = 12.0;
                    let button = |text| {
                        egui::Button::new(
                            RichText::new(text)
                                .size(17.0)
                                .strong()
                                .color(ABANDON_CONFIRMATION_TEXT_COLOR),
                        )
                        .min_size(egui::vec2(button_width, MODAL_BUTTON_HEIGHT))
                    };
                    let cancel = ui
                        .add(button("Cancel"))
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .clicked();
                    let confirm = ui
                        .add_enabled(
                            enabled
                                && (!selected.is_empty()
                                    || !state.joint_attack_invitees.is_empty()),
                            button("Confirm"),
                        )
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_disabled_hover_small(if session.joint_attack_update_pending {
                            "Wait for the current invitation update."
                        } else {
                            "Select at least one player to invite."
                        })
                        .clicked();
                    (cancel, confirm)
                })
                .inner
            })
            .inner
        },
    );
    let (cancel, confirm) = response.inner;
    if confirm {
        state.allied_mission = !selected.is_empty();
        state.joint_attack_invitees = selected;
    } else if !cancel && !response.should_close() {
        state.joint_attack_invite_selection = Some(selected);
    }
}

/// Draws the new mission interface and emits any resulting local actions.
fn draw_new_mission(
    ui: &mut Ui,
    send_mission: &mut MessageWriter<SendMissionMsg>,
    _missions: &[Mission],
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
    let panel_rect = ui.max_rect().intersect(ui.ctx().content_rect());
    if state.joint_attack_draft_id.is_some() {
        if let Some(draft) = &state.joint_attack_owner_draft {
            state.mission_info = draft.clone();
        }
    }
    let active_invitation = state
        .joint_attack_draft_id
        .and_then(|id| {
            session.joint_attacks.iter().find(|invitation| {
                invitation.id == id && !invitation.canceled && !invitation.launched
            })
        })
        .cloned();
    if state.joint_attack_draft_id.is_some()
        && active_invitation.is_none()
        && !session.joint_attack_update_pending
    {
        state.joint_attack_draft_id = None;
        state.joint_attack_invitees.clear();
        state.allied_mission = false;
    }
    if !map
        .try_get(state.mission_info.origin)
        .is_some_and(|planet| planet.can_launch_mission(player.id))
    {
        state.mission_info.origin = player.home_planet;
    }
    let origin = map.get(state.mission_info.origin);
    let destination = map.get(state.mission_info.destination);
    let mut origin_army = origin.mission_origin_army(player.id).cloned().unwrap_or_default();
    for reserved in session
        .joint_attacks
        .iter()
        .filter(|invitation| {
            !invitation.canceled && !invitation.launched && invitation.inviter != player.id
        })
        .flat_map(|invitation| &invitation.participants)
        .filter(|participant| {
            participant.player_id == player.id
                && participant.response == JointAttackResponse::Accepted
        })
        .filter_map(|participant| participant.contribution.as_ref())
        .filter(|contribution| contribution.origin == origin.id)
    {
        for (unit, count) in &reserved.army {
            if let Some(available) = origin_army.get_mut(unit) {
                *available = available.saturating_sub(*count);
            }
        }
    }
    origin_army.retain(|_, count| *count > 0);

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

    let objectives = Icon::objectives(
        player.owns(destination),
        player.controls(destination),
        destination.allows_protection(player.id),
        destination.is_protected_by(player.id),
    );
    if !objectives.contains(&state.mission_info.objective) {
        state.mission_info.objective = objectives.first().copied().unwrap_or_default();
    }

    // Keep an invited Protect draft selected even when its origin has no ships left.
    // Opening a gate shortcut must not silently turn it into a hostile mission.
    if (!state.mission_info.objective.condition_for_army(&origin_army)
        && state.mission_info.objective != Icon::Protect)
        || (destination.is_moon() && state.mission_info.objective.on_planet_only())
    {
        state.mission_info.objective = objectives
            .iter()
            .copied()
            .find(|i| {
                i.condition_for_army(&origin_army)
                    && (!destination.is_moon() || !i.on_planet_only())
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
                for (response, planet) in draw_mission_route(
                    ui,
                    &mut state.mission_info,
                    map,
                    player,
                    &origin_army,
                    &army,
                    images,
                    false,
                )
                .into_iter()
                .flatten()
                .zip([origin, destination])
                {
                    action(response, planet, &mut changed_hover, state);
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

    {
        let body_width = panel_rect.width();
        let invite_width = if session.has_active_game() {
            MISSION_INVITE_ICON_SIZE + 12.0
        } else {
            0.0
        };
        let button_count = 1 + usize::from(active_invitation.is_some());
        let compact_footer = button_count as f32 * 180.0
            + button_count.saturating_sub(1) as f32 * ui.spacing().item_spacing.x
            > body_width - 20.0 - invite_width;
        let footer_height = if compact_footer {
            100.0 + ui.spacing().item_spacing.y
        } else {
            50.0
        };
        let footer_rect = egui::Rect::from_min_max(
            egui::pos2(panel_rect.left() + 10.0, panel_rect.bottom() - footer_height - 10.0),
            panel_rect.right_bottom() - egui::vec2(10.0, 10.0),
        );
        // Let the extra destruction row overflow vertically without adding a scrollbar.
        let body_scroll = if state.mission_info.objective == Icon::Destroy {
            ScrollArea::horizontal().vertical_scroll_offset(0.0)
        } else {
            ScrollArea::both()
        };
        body_scroll
            .id_salt("mission fleet and details")
            .max_width(body_width)
            .max_height((footer_rect.top() - ui.cursor().top() - 20.0).max(0.0))
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_width(body_width);
                ui.horizontal_top(|ui| {
                    ui.add_space(130.);

                    ui.vertical(|ui| {
                        ui.set_width(280.);

                        draw_mission_fleet_picker(
                            ui,
                            &mut state.mission_info.army,
                            &origin_army,
                            &army,
                            images,
                        );
                        if let Some(invitation) = &active_invitation {
                            ui.separator();
                            draw_joint_attack_strengths(
                                ui,
                                invitation,
                                session,
                                Some((player.id, &state.mission_info.army)),
                            );
                        }
                    });

                    ui.add_space(15.);

                    ui.vertical(|ui| {
                        ui.set_width(330.);
                        ui.add_space(20.);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.;
                            ui.spacing_mut().button_padding = egui::Vec2::splat(2.);

                            let on_hover = |ui: &mut Ui, icon: &Icon, msg: bool| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.add_image(
                                            images.get(format!("{} cover", icon.asset_key())),
                                            [150., 150.],
                                        );
                                    });
                                    ui.vertical(|ui| {
                                        ui.label(icon.to_name());
                                        ui.separator();

                                        if msg {
                                            ui.colored_label(
                                                Color32::RED,
                                                RichText::new(icon.requirement()).small(),
                                            );
                                        }

                                        ui.small(icon.description());
                                    });
                                });
                            };

                            for icon in Icon::objectives(
                                player.owns(destination),
                                player.controls(destination),
                                destination.allows_protection(player.id),
                                destination.is_protected_by(player.id),
                            ) {
                                ui.add_enabled_ui(
                                    icon.condition_for_army(&origin_army)
                                        && !(destination.is_moon() && icon.on_planet_only())
                                        && !(icon == Icon::Colonize && n_owned >= n_max_owned),
                                    |ui| {
                                        let button = ui
                                            .add(
                                                egui::Button::image(SizedTexture::new(
                                                    images.get(icon.asset_key()),
                                                    [40.; 2],
                                                ))
                                                .corner_radius(5.),
                                            )
                                            .on_hover_ui(|ui| on_hover(ui, &icon, false))
                                            .on_disabled_hover_ui(|ui| on_hover(ui, &icon, true))
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

                                            state.mission_info.objective = icon;
                                        }
                                    },
                                );
                            }
                        });

                        ui.add_space(5.);

                        draw_mission_details(
                            ui,
                            &mut state.mission_info,
                            map,
                            player,
                            settings.turn,
                            images,
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
                                .on_hover_small(
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
            ui.ctx(),
            state,
            settings.turn,
            player,
            session,
            multiplayer_requests,
            active_invitation.as_ref(),
        );
        if session.has_active_game() {
            let eligible = matches!(
                state.mission_info.objective,
                Icon::Colonize | Icon::Attack | Icon::Destroy,
            );
            ui.add_enabled_ui(eligible && !session.joint_attack_update_pending, |ui| {
                let icon_rect = egui::Rect::from_min_size(
                    footer_rect.left_bottom() + egui::vec2(6.0, -MISSION_INVITE_ICON_SIZE - 11.0),
                    egui::Vec2::splat(MISSION_INVITE_ICON_SIZE),
                );
                let response = ui
                    .interact(icon_rect, ui.id().with("invite players"), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_small("Invite players to an allied attack mission.")
                    .on_disabled_hover_small(if !eligible {
                        "Players can join Attack, Colonize, and Destroy missions."
                    } else {
                        "Wait for the current invitation update."
                    });
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
                if response.clicked() {
                    state.joint_attack_invite_selection = Some(state.joint_attack_invitees.clone());
                }
            });
        }
        let actions_rect = egui::Rect::from_min_max(
            footer_rect.left_top() + egui::vec2(invite_width, 0.0),
            footer_rect.right_bottom(),
        );
        ui.scope_builder(UiBuilder::new().max_rect(actions_rect).layout(Layout::top_down(Align::Max)), |ui| {

            let army_check = state.mission_info.army.has_army();
            let fuel_check = player.resources.deuterium >= state.mission_info.fuel_consumption(map);
            let validation =
                validate_mission(player, map, origin, destination, &state.mission_info);
            let objective_check = validation.is_ok();
            let invitee_accepted = state.joint_attack_draft_id.is_none()
                || active_invitation.as_ref().is_some_and(|invitation| {
                    invitation.participants.iter().any(|participant| {
                        participant.player_id != player.id
                            && participant.response == JointAttackResponse::Accepted
                    })
                });

            ui.with_layout(Layout::right_to_left(Align::Center).with_main_wrap(true), |ui| {

                ui.add_enabled_ui(
                    army_check && fuel_check && objective_check && invitee_accepted && allied_synced
                        && state.joint_attack_invite_selection.is_none(),
                    |ui| {
                        let response = ui
                            .add_custom_button("Send mission", images)
                            .on_disabled_hover_ui(|ui| {
                                if !army_check {
                                    ui.small("No ships selected for the mission.");
                                } else if !fuel_check {
                                    ui.small("Not enough fuel (deuterium) for the mission.");
                                } else if let Err(error) = &validation {
                                    ui.small(error.to_string());
                                } else if !allied_synced {
                                    ui.small("Invite players and wait for the current mission to be shared.");
                                } else if !invitee_accepted {
                                    ui.small("At least one invited player must accept first.");
                                }
                            });

                        if response.clicked()
                            || (response.enabled() && keyboard.just_pressed(KeyCode::Enter))
                        {
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
                                        participant.response == JointAttackResponse::Accepted
                                    })
                                    .filter_map(|participant| participant.contribution.clone())
                                    .collect::<Vec<_>>()
                            });
                            if let (Some(invitation), Some(contributions)) = (
                                active_invitation.as_ref(),
                                accepted.filter(|items| items.len() > 1),
                            ) {
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
                                send_mission.write(SendMissionMsg::new(mission));
                            }
                            // The accepted command emits its launch cue in send_mission.
                            set_ui_sound(ui.ctx(), None);
                            state.planet_selected = None;
                            state.mission = false;
                            state.mission_info = Mission::default();
                            state.joint_attack_invitees.clear();
                            state.joint_attack_draft_id = None;
                            state.allied_mission = false;
                        }
                    },
                );
                if let Some(invitation) = &active_invitation {
                    if ui.add_enabled_ui(!session.joint_attack_update_pending, |ui| {
                        ui.add_custom_button("Cancel mission", images)
                    }).inner.clicked() {
                        multiplayer_requests.write(MultiplayerRequest::CancelJointAttack {
                            attack_id: invitation.id,
                        });
                        state.joint_attack_draft_id = None;
                        state.joint_attack_invitees.clear();
                        state.allied_mission = false;
                        state.mission = false;
                        state.mission_info = Mission::default();
                    }
                }
            });
        });
    }
    state.joint_attack_owner_draft =
        state.joint_attack_draft_id.map(|_| state.mission_info.clone());
    draw_joint_attack_invite_modal(
        ui.ctx(),
        state,
        session,
        player,
        destination,
        images,
        ui.is_enabled(),
    );
}

/// Shared planet selectors and fleet shortcut; invitations keep their destination fixed.
fn draw_mission_route(
    ui: &mut Ui,
    mission: &mut Mission,
    map: &Map,
    player: &Player,
    available: &Army,
    units: &[Unit],
    images: &ImageIds,
    fixed_destination: bool,
) -> [Option<Response>; 2] {
    let origin = map.get(mission.origin);
    let destination = map.get(mission.destination);
    let mut origin_response = None;
    let mut destination_response = None;
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
                    style_selection_boxes(ui);
                    ui.add_space(15.);

                    let origins = map
                        .planets
                        .iter()
                        .filter(|planet| planet.can_launch_mission(player.id))
                        .sorted_by(|a, b| a.name.cmp(&b.name))
                        .collect::<Vec<_>>();

                    ComboBox::from_id_salt("origin")
                        .height(60. * origins.len().max(5) as f32)
                        .selected_text(&map.get(mission.origin).name)
                        .show_ui(ui, |ui| {
                            for planet in origins {
                                ui.selectable_value(&mut mission.origin, planet.id, &planet.name)
                                    .on_hover_cursor(CursorIcon::PointingHand);
                            }
                        })
                        .response
                        .on_hover_cursor(CursorIcon::PointingHand);
                });
            });

            let (rect, mut response) =
                ui.cell(50., |ui| ui.allocate_exact_size([50.; 2].into(), Sense::click()));

            response = response.on_hover_cursor(CursorIcon::PointingHand).on_hover_small(
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
            } else if response.secondary_clicked() {
                mission.army.clear();
            }

            ui.add_enabled_ui(!fixed_destination, |ui| {
                ui.cell(100., |ui| {
                    ui.vertical(|ui| {
                        style_selection_boxes(ui);
                        ui.add_space(15.);
                        ComboBox::from_id_salt("destination")
                            .selected_text(&map.get(mission.destination).name)
                            .show_ui(ui, |ui| {
                                for planet in map
                                    .planets
                                    .iter()
                                    .filter(|p| !p.is_destroyed)
                                    .sorted_by(|a, b| a.name.cmp(&b.name))
                                {
                                    ui.selectable_value(
                                        &mut mission.destination,
                                        planet.id,
                                        &planet.name,
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand);
                                }
                            })
                            .response
                            .on_hover_cursor(CursorIcon::PointingHand);
                    });
                });
            });
            let response = ui.cell(70., |ui| {
                ui.add_image(images.get(destination.image()), [60.; 2])
                    .interact(Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand)
            });

            destination_response = Some(response);
        },
    );
    [origin_response, destination_response]
}

/// Fleet cards shared by mission drafts and joint-attack responses.
fn draw_mission_fleet_picker(
    ui: &mut Ui,
    selected: &mut Army,
    available: &Army,
    units: &[Unit],
    images: &ImageIds,
) {
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
                            .on_hover_small(unit.to_name())
                            .on_disabled_hover_small(unit.to_name());

                        if response.clicked() {
                            *selected.entry(*unit).or_insert(0) = n;
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

                        style_selection_boxes(ui);
                        ui.style_mut().drag_value_text_style = TextStyle::Body;
                        ui.spacing_mut().button_padding = egui::vec2(4.0, 6.0);
                        ui.spacing_mut().interact_size.x = 50.;
                        let value = selected.entry(*unit).or_insert(0);
                        ui.add(egui::DragValue::new(value).speed(0.2).range(0..=n));
                    });
                });
            });

            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
    selected.retain(|_, count| *count > 0);
}

/// Route facts and combat options use the same spacing, labels, and tooltips in both panels.
fn draw_mission_details(
    ui: &mut Ui,
    mission: &mut Mission,
    map: &Map,
    player: &Player,
    turn: usize,
    images: &ImageIds,
) {
    let origin = map.get(mission.origin);
    let destination = map.get(mission.destination);
    let speed = mission.speed();
    let distance = mission.distance(map);
    let duration = mission.duration(map);
    ui.horizontal(|ui| {
        ui.small("🎯 Objective:");

        ui.spacing_mut().item_spacing.x = 4.;
        ui.add_image(images.get(mission.objective.asset_key()), [20.; 2]);
        ui.small(mission.objective.to_name());
    });

    ui.small(format!("📏 Target distance: {distance:.1} AU")).on_hover_small(
        "AU means astronomical unit, the distance scale used on the galaxy map. \
        Target distance is the length of the route from the origin world to the \
        destination world.",
    );
    let movement_tooltip = mission_movement_tooltip(speed == f32::MAX);
    ui.small(format!(
        "🚀 First-turn movement: {}",
        if speed == 0. || speed == f32::MAX {
            "---".to_string()
        } else {
            format!("{speed} AU")
        }
    ))
    .on_hover_small(movement_tooltip);
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
            .on_hover_small("Select a valid fleet and route to calculate its arrival turn.");
    } else {
        duration_response.on_hover_small(mission_arrival_tooltip(turn, duration));
    }
    draw_deep_cover_option(ui, mission, origin);
    let fuel = mission.fuel_consumption(map);
    let fuel_check = player.resources.deuterium >= fuel;
    let fuel_text = format!("⛽ Fuel consumption: {fuel}");
    let fuel_response = if fuel_check {
        ui.small(fuel_text)
    } else {
        ui.colored_label(Color32::RED, RichText::new(fuel_text).small())
    };
    fuel_response.on_hover_small("Amount of deuterium it costs to send this mission.");

    if mission.objective == Icon::Destroy {
        let war_suns = mission.army.amount(&Unit::war_sun());
        let per_sun = f64::from(destination.destroy_probability_basis_points()) / 10_000.0;
        let combined = 1.0 - (1.0 - per_sun).powf(war_suns as f64);
        ui.small(format!(
            "💥 Chance of destruction: {:.1}% / {:.1}%",
            per_sun * 100.0,
            combined * 100.0,
        ))
        .on_hover_small(
            "The first number is the chance per War Sun. \
            The second number is the total chance for all selected War Suns.",
        );
    }

    if matches!(mission.objective, Icon::Colonize | Icon::Attack | Icon::Destroy) {
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
        .on_disabled_hover_small("No Probes selected for this mission.");

        if probes == 0 {
            mission.combat_probes = false;
        }

        let bombers = mission.army.amount(&Unit::Ship(Ship::Bomber));
        ui.add_enabled_ui(bombers > 0 && !destination.is_moon(), |ui| {
            ui.horizontal(|ui| {
                ui.small("💣 Bombing raid:");

                style_selection_boxes(ui);
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
                            .on_hover_small(item.description());
                        }
                    })
                    .response
                    .on_hover_cursor(CursorIcon::PointingHand);
            });
        })
        .response
        .on_hover_small(
            "Command Bombers to bomb enemy buildings. Every round of combat, \
        every bomber has a 25% chance to decrease a target building's level by \
        one. The Planetary Shield must first be destroyed before bombing can \
        take place.",
        )
        .on_disabled_hover_small(if destination.is_moon() {
            "Moons cannot be bombed."
        } else {
            "No Bombers selected for this mission."
        });

        if bombers == 0 || destination.is_moon() {
            mission.bombing = BombingRaid::None;
        }
    }
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

                            if mission.owner == player.id
                                || !mission.objective.is_hidden()
                                || mission.is_seen_by_radar(map, player).is_some()
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

                            let (mut route_ui, mut response) =
                                mission_route_cell(ui, route_column_width);
                            route_ui.horizontal_centered(|ui| {
                                ui.spacing_mut().item_spacing.x = 8.;

                                ui.add_image(
                                    images.get(mission.displayed_objective(player.id).asset_key()),
                                    [25.; 2],
                                );

                                let [red, green, blue] = session.player_color(mission.owner).rgb();
                                draw_route_preview(
                                    ui,
                                    route_preview_width,
                                    mission,
                                    player,
                                    Color32::from_rgb(red, green, blue),
                                    returning,
                                );

                                ui.label(
                                    RichText::new(format!(
                                        "+{}",
                                        mission_display_turns(mission, map)
                                    ))
                                    .strong(),
                                );

                                if mission_recall_available(mission, player.id) {
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
                            });

                            let response = if mission.owner != player.id {
                                block_enemy_route_clicks(ui, response, mission.id)
                            } else {
                                response
                            };

                            if response.hovered() {
                                // Browsing animated routes must never enqueue generic UI audio.
                                set_ui_sound(ui.ctx(), None);
                                state.mission_hover = Some(mission.id);
                                state.mission_hover_from_ui = true;
                                changed_hover = true;
                            }

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
                    let destination = map.get(report.mission.destination);

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

                            let [red, green, blue] =
                                session.player_color(report.mission.owner).rgb();
                            let size = mission_report_image_base_size(
                                state.mission_report == Some(report.mission.id),
                            );
                            let mission_image = report.mission.image(player);
                            draw_mission_image(
                                ui,
                                images.get(mission_image),
                                mission_report_image_size(mission_image, size),
                                MISSION_REPORT_IMAGE_SLOT_SIZE,
                                mission_report_image_offset(mission_image),
                                Color32::from_rgb(red, green, blue),
                            );

                            ui.scope(|ui| {
                                ui.set_width(20.);
                                ui.small(report.turn.to_string());
                            });

                            let resp = ui.add_image(images.get(destination.image()), [40.; 2]);

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
                        state.mission_report = Some(report.mission.id);
                    }
                }
            });
        });

        ui.add_space(-10.);
        ui.separator();

        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 40.);

            let Some(report) = player
                .reports
                .iter()
                .find(|r| state.mission_report == Some(r.mission.id))
                .or_else(|| reports.last().copied())
            else {
                return;
            };

            ui.horizontal(|ui| {
                ui.add_space(55.);

                let mut changed_hover = false;
                egui::Grid::new("active report").spacing([10., 0.]).striped(false).show(ui, |ui| {
                    let origin = map.get(report.mission.origin);
                    let destination = map.get(report.mission.destination);

                    if report.mission.owner == player.id || !report.mission.objective.is_hidden() {
                        let resp1 = ui.cell(70., |ui| {
                            let resp1 = ui
                                .add_image(images.get(origin.image()), [60.; 2])
                                .interact(Sense::click())
                                .on_hover_cursor(CursorIcon::PointingHand);

                            if report.mission.owner == player.id {
                                let resp = ui.add_icon_on_image(images.get("logs"), resp1.rect);

                                resp.on_hover_ui(|ui| {
                                    ui.set_min_width(350.);
                                    ui.small(format!(
                                        "Mission logs\n===========\n\n{}",
                                        report.mission.logs
                                    ));
                                });
                            }

                            resp1
                        });

                        let resp2 = ui.cell(100., |ui| {
                            ui.small(&origin.name)
                                .interact(Sense::click())
                                .on_hover_cursor(CursorIcon::PointingHand)
                        });

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
                        ui.cell(70., |ui| {
                            ui.add_image(images.get("unknown"), [60.; 2]);
                        });
                        ui.cell(100., |ui| ui.small("Unknown"));
                    }

                    ui.cell(100., |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;

                            ui.add_image(
                                images.get(report.mission.objective.asset_key()),
                                [25.; 2],
                            )
                            .on_hover_small(report.mission.objective.to_name());

                            let [red, green, blue] =
                                session.player_color(report.mission.owner).rgb();
                            let mission_image = report.mission.image(player);
                            draw_mission_image(
                                ui,
                                images.get(mission_image),
                                mission_report_image_size(mission_image, 50.0),
                                50.0,
                                mission_report_image_offset(mission_image),
                                Color32::from_rgb(red, green, blue),
                            );

                            ui.small(report.turn.to_string()).on_hover_small(format!(
                                "The mission arrived in turn {}.",
                                report.turn
                            ));
                        });
                    });

                    let resp3 = ui.cell(100., |ui| {
                        ui.small(&destination.name)
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand)
                    });

                    let resp4 = ui.cell(70., |ui| {
                        ui.add_image(images.get(destination.image()), [60.; 2])
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand)
                    });

                    if report.combat_report.is_some() {
                        ui.add_icon_on_image(images.get(report.image(player)), resp4.rect);
                    }

                    handle_mission_planet_link(
                        &resp3,
                        &resp4,
                        destination,
                        &mut changed_hover,
                        state,
                        map,
                        player,
                    );

                    // If not hovering anything, reset all hover selections
                    if is_hovered && !changed_hover {
                        state.planet_hover = None;
                        state.mission_planet_hover = None;
                    }
                });
            });

            ui.add_space(-10.);
            ui.horizontal(|ui| {
                ui.visuals_mut().widgets.noninteractive.bg_stroke.width = 6.;

                let a_color = session.player_color(report.mission.owner).color();
                let d_color = report
                    .planet
                    .controlled
                    .or(report.planet.owned)
                    .map_or(Color::srgb_u8(150, 158, 170), |defender| {
                        session.player_color(defender).color()
                    });

                ui.vertical(|ui| {
                    ui.set_width(140.);
                    ui.visuals_mut().widgets.noninteractive.bg_stroke.color = a_color.to_color32();
                    ui.separator();
                });
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.visuals_mut().widgets.noninteractive.bg_stroke.color = d_color.to_color32();
                    ui.separator();
                });
            });
            ui.add_space(-10.);

            ui.horizontal(|ui| {
                ui.set_height(357.);

                ui.vertical(|ui| {
                    ui.set_width(140.);

                    let army = match report.mission.objective {
                        Icon::MissileStrike => vec![Unit::interplanetary_missile()],
                        Icon::Spy => vec![Unit::probe()],
                        _ => Unit::ships(),
                    };

                    draw_army_grid(ui, "attacker", &army, report, player, images);

                    if report.scout_probes > 0 && report.can_see(&Side::Attacker, player.id) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;
                            ui.add_image(images.get(Icon::Spy.to_lowername()), [15., 15.]);
                            ui.small(format!("Scouts: {}", report.scout_probes));
                        })
                        .response
                        .on_hover_small_ext(
                            "Number of attacking Probes that left combat after the first round.",
                        );
                    }
                });

                ui.add_space(-13.);
                ui.separator();
                ui.add_space(-10.);

                ui.vertical(|ui| {
                    ui.set_height(450.);

                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.;

                        let destination = map.get(report.mission.destination);

                        if !report.planet.army.has_army() {
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

                    ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                        if report.combat_report.is_some()
                            && report.can_see(&Side::Attacker, player.id)
                            && report.can_see(&Side::Defender, player.id)
                            && ui.add_custom_button("Combat details", images).clicked()
                        {
                            state.combat_report = Some(report.id);
                            state.combat_report_round = 1;
                        }
                    });
                });
            });
        });
    });
}

const JOINT_ATTACK_TOAST_ACCENT: Color32 = Color32::from_rgb(112, 190, 255);

fn joint_attack_toast_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            RichText::new(label).size(17.0).strong().color(ABANDON_CONFIRMATION_TEXT_COLOR),
        )
        .min_size(egui::vec2(110.0, MODAL_BUTTON_HEIGHT)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

fn joint_attack_toast<R>(
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
            ui.add(egui::Label::new(text).halign(Align::Min).wrap());
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
    } else if destination.is_protected_by(player.id) {
        Some("Recall your protection fleet before joining this attack.")
    } else if !mission.army.has_army() {
        Some("No ships selected for the mission.")
    } else if mission.fuel_consumption(map) > player.resources.deuterium {
        Some("Not enough fuel (deuterium) for the mission.")
    } else if update_pending {
        Some("Updating your fleet. Please wait.")
    } else {
        None
    }
}

/// Shows live drafts and committed fleets to every participant, excluding rejected players.
fn draw_joint_attack_strengths(
    ui: &mut Ui,
    invitation: &JointAttackInvitation,
    session: &MultiplayerSession,
    local: Option<(PlayerId, &Army)>,
) {
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        ui.small(RichText::new("Fleet contributions").strong());
        for participant in invitation
            .participants
            .iter()
            .filter(|participant| participant.response != JointAttackResponse::Rejected)
        {
            let name = session
                .player_name(participant.player_id)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("Player {}", participant.player_id));
            let army = local
                .filter(|(id, _)| *id == participant.player_id)
                .map(|(_, army)| army)
                .or_else(|| participant.contribution.as_ref().map(|item| &item.army));
            let strength = army.map_or(0, fleet_strength);
            let status = if participant.response == JointAttackResponse::Accepted {
                "Accepted"
            } else {
                "Choosing"
            };
            ui.small(
                RichText::new(format!("{name} · Strength {strength} · {status}"))
                    .color(session.player_color(participant.player_id).color().to_color32()),
            );
        }
    });
}

/// Shows the owner's current published choices, including the exact fleet behind its strength.
fn draw_joint_attack_owner_plan(
    ui: &mut Ui,
    invitation: &JointAttackInvitation,
    map: &Map,
    session: &MultiplayerSession,
    images: &ImageIds,
) {
    let Some(owner) = invitation.participants.first().and_then(|item| item.contribution.as_ref())
    else {
        return;
    };
    ui.scope(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        let name = session.player_name(invitation.inviter).unwrap_or("Leader");
        ui.small(
            RichText::new(format!("{name}'s mission"))
                .strong()
                .color(session.player_color(invitation.inviter).color().to_color32()),
        );
        let world_name = |id| map.try_get(id).map_or("Unknown world", |world| world.name.as_str());
        ui.small(format!("Origin: {}", world_name(owner.origin)));
        ui.small(format!("Target: {}", world_name(invitation.destination)));
        ui.small(format!("Objective: {}", invitation.objective.to_name()));
        ui.small(format!("Bombing raid: {}", owner.bombing.to_name()));
        ui.small(format!(
            "Combat Probes: {}",
            if owner.combat_probes {
                "On"
            } else {
                "Off"
            }
        ));
        ui.horizontal_wrapped(|ui| {
            for (unit, count) in owner.army.iter().filter(|(_, count)| **count > 0) {
                ui.horizontal(|ui| {
                    ui.add_image(images.get(unit.to_lowername()), [28.0; 2]);
                    ui.small(count.to_string());
                })
                .response
                .on_hover_small(format!("{}: {count}", unit.to_name()));
            }
        });
        ui.small("Mission changes reset all acceptances.");
    });
}

/// Allied drafts reserve room for the live roster and still fit smaller viewports.
pub(super) fn mission_panel_size(viewport: egui::Vec2, participants: usize) -> egui::Vec2 {
    let height = if participants == 0 {
        640.0
    } else {
        704.0 + 24.0 * participants as f32
    };
    egui::vec2(
        850.0_f32.min((viewport.x - 32.0).max(0.0)),
        height.min((viewport.y - 32.0).max(0.0)),
    )
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
    destination: &Planet,
    participant_response: JointAttackResponse,
) {
    let size = mission_panel_size(context.content_rect().size(), invitation.participants.len());
    let modal_id = egui::Id::new(("joint attack response", invitation.id));
    let editable = participant_response == JointAttackResponse::Pending;
    let response = show_panel_modal(context, images, modal_id, size, |ui, panel, content| {
        let header = egui::Rect::from_min_size(content.min, egui::vec2(content.width(), 40.0));
        ui.scope_builder(UiBuilder::new().max_rect(header), |ui| {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Joint Attack")
                        .size(21.0)
                        .strong()
                        .color(ABANDON_CONFIRMATION_TEXT_COLOR),
                );
            });
        });
        let close_rect = egui::Rect::from_min_size(
            egui::pos2(content.right() - 28.0, content.top()),
            egui::vec2(28.0, 28.0),
        );
        let close_clicked = ui.put(close_rect, egui::Button::new("×")).clicked();
        let footer = egui::Rect::from_min_max(
            egui::pos2(content.left(), content.bottom() - 50.0),
            content.max,
        );
        let body = egui::Rect::from_min_max(
            egui::pos2(content.left(), header.bottom() + 10.0),
            egui::pos2(content.right(), footer.top() - 12.0),
        );
        ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
            ui.set_clip_rect(body);
            ScrollArea::both()
                .id_salt(("joint attack body", invitation.id))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_width(body.width());
                    let origin = map.get(state.joint_attack_contribution.origin);
                    state.joint_attack_contribution.destination = destination.id;
                    state.joint_attack_contribution.objective = invitation.objective;
                    let available =
                        origin.mission_origin_army(player.id).cloned().unwrap_or_default();
                    ui.add_enabled_ui(editable, |ui| {
                        ui.horizontal_top(|ui| {
                            ui.add_space((panel.left() + 135.0 - body.left()).max(0.0));
                            draw_mission_route(
                                ui,
                                &mut state.joint_attack_contribution,
                                map,
                                player,
                                &available,
                                &Unit::ships(),
                                images,
                                true,
                            );
                        });
                    });
                    ui.add_space(-10.0);
                    ui.add(Separator::default().shrink(36.0));
                    let origin = map.get(state.joint_attack_contribution.origin);
                    let available =
                        origin.mission_origin_army(player.id).cloned().unwrap_or_default();
                    state.joint_attack_contribution = Mission::from_mission(
                        invitation.turn as usize,
                        player.id,
                        origin,
                        destination,
                        &state.joint_attack_contribution,
                    );
                    let mut columns = |ui: &mut Ui| {
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
                            ui.separator();
                            draw_joint_attack_strengths(
                                ui,
                                invitation,
                                session,
                                Some((player.id, &state.joint_attack_contribution.army)),
                            );
                        });
                        ui.add_space(15.0);
                        ui.vertical(|ui| {
                            ui.set_width(330.0);
                            ui.add_space(20.0);
                            ui.add_enabled_ui(editable, |ui| {
                                draw_mission_details(
                                    ui,
                                    &mut state.joint_attack_contribution,
                                    map,
                                    player,
                                    invitation.turn as usize,
                                    images,
                                );
                            });
                            ui.separator();
                            draw_joint_attack_owner_plan(ui, invitation, map, session, images);
                        });
                    };
                    if body.width() < 680.0 {
                        ui.vertical(&mut columns);
                    } else {
                        ui.horizontal_top(|ui| {
                            ui.add_space((panel.left() + 130.0 - body.left()).max(0.0));
                            columns(ui);
                        });
                    }
                });
        });
        let error = joint_attack_accept_error(
            &state.joint_attack_contribution,
            map,
            player,
            session.joint_attack_update_pending,
        );
        let mut close = close_clicked;
        let mut action = None;
        ui.scope_builder(UiBuilder::new().max_rect(footer), |ui| {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if editable {
                    if ui
                        .add_enabled_ui(error.is_none(), |ui| {
                            ui.add_custom_button("Accept", images)
                        })
                        .inner
                        .on_disabled_hover_small(error.unwrap_or_default())
                        .clicked()
                    {
                        action = Some(JointAttackResponse::Accepted);
                    }
                    if ui
                        .add_enabled_ui(!session.joint_attack_update_pending, |ui| {
                            ui.add_custom_button("Reject", images)
                        })
                        .inner
                        .clicked()
                    {
                        action = Some(JointAttackResponse::Rejected);
                    }
                } else {
                    if ui
                        .add_enabled_ui(!session.joint_attack_update_pending, |ui| {
                            ui.add_custom_button("Undo accept", images)
                        })
                        .inner
                        .clicked()
                    {
                        action = Some(JointAttackResponse::Pending);
                    }
                    if ui.add_custom_button("Close", images).clicked() {
                        close = true;
                    }
                }
            });
        });
        (close, action)
    });

    let (mut close, action) = response.inner;
    close |= response.should_close();
    let draft = JointAttackContribution {
        player_id: player.id,
        origin: state.joint_attack_contribution.origin,
        army: state.joint_attack_contribution.army.clone(),
        bombing: state.joint_attack_contribution.bombing.clone(),
        combat_probes: state.joint_attack_contribution.combat_probes,
    };
    let published = invitation
        .participants
        .iter()
        .find(|item| item.player_id == player.id)
        .and_then(|item| item.contribution.as_ref());
    // Publish the latest draft after input settles, coalescing drag edits while a request runs.
    let publish_draft = editable
        && !session.joint_attack_update_pending
        && !context.input(|input| input.pointer.any_down())
        && published != Some(&draft);
    if let Some(action) = action {
        requests.write(MultiplayerRequest::RespondJointAttack {
            expected_revision: invitation.revision,
            attack_id: invitation.id,
            response: action,
            contribution: (action != JointAttackResponse::Rejected).then_some(draft),
        });
        close = action == JointAttackResponse::Rejected;
    } else if publish_draft {
        requests.write(MultiplayerRequest::RespondJointAttack {
            expected_revision: invitation.revision,
            attack_id: invitation.id,
            response: JointAttackResponse::Pending,
            contribution: Some(draft),
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
    if !mission_panel_visible(state) || state.mission_tab != MissionTab::NewMission {
        if let Some(draft) = state.joint_attack_owner_draft.take() {
            // Keep publishing coalesced owner edits even after the editor is closed. Map
            // navigation may have changed mission_info, so restore it after using the owner draft.
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
                sync_allied_mission(context, state, turn, player, session, requests, invitation);
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
            if participated && state.joint_attack_cancellations_notified.insert(invitation.id) {
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
            if state.joint_attack_rejections_notified.insert((invitation.id, rejected.player_id)) {
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
                    if mission_panel_visible(state)
                        && state.mission_tab == MissionTab::NewMission
                        && state.joint_attack_draft_id == Some(invitation.id)
                    {
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
                } else if participant.response == JointAttackResponse::Pending {
                    let article = if invitation.objective == Icon::Attack {
                        "an"
                    } else {
                        "a"
                    };
                    joint_attack_toast(
                        ui,
                        format!(
                            "{inviter_name} invited you to join {article} {} mission on {target_name}.",
                            invitation.objective.to_lowername()
                        ),
                        |ui| {
                            ui.horizontal(|ui| {
                                if joint_attack_toast_button(ui, "Open", true).clicked() {
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
                            });
                        },
                    );
                } else if participant.response == JointAttackResponse::Accepted {
                    joint_attack_toast(
                        ui,
                        format!(
                            "You joined {inviter_name}'s {} mission on {target_name}.",
                            invitation.objective.to_lowername()
                        ),
                        |ui| {
                            if joint_attack_toast_button(ui, "Review", true).clicked() {
                                state.joint_attack_open = Some(invitation.id);
                            }
                        },
                    );
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
            ..default()
        };
    }
    if restore_draft || participant_response == JointAttackResponse::Accepted {
        if let Some(contribution) = participant.contribution.as_ref() {
            state.joint_attack_contribution.origin = contribution.origin;
            state.joint_attack_contribution.army = contribution.army.clone();
            state.joint_attack_contribution.bombing = contribution.bombing.clone();
            state.joint_attack_contribution.combat_probes = contribution.combat_probes;
        }
    }
    if !map
        .try_get(state.joint_attack_contribution.origin)
        .is_some_and(|origin| origin.can_launch_mission(player.id))
    {
        state.joint_attack_contribution.origin = player.home_planet;
        state.joint_attack_contribution.army.clear();
    }
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
    state.allied_mission = true;
    state.joint_attack_invitees =
        invitation.participants.iter().skip(1).map(|item| item.player_id).collect();
    if let Some(contribution) =
        invitation.participants.first().and_then(|item| item.contribution.as_ref())
    {
        state.joint_attack_owner_draft = Some(Mission {
            origin: contribution.origin,
            destination: invitation.destination,
            objective: invitation.objective,
            army: contribution.army.clone(),
            bombing: contribution.bombing.clone(),
            combat_probes: contribution.combat_probes,
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
            missions.iter().filter(|m| m.owner == player.id).collect(),
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
            missions.iter().filter(|m| m.owner != player.id).collect(),
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
