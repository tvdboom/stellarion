//! Objective-style fleet insignia and hover-based battle-plan selectors.

use super::*;
use crate::core::combat::plan::{BattlePlan, CombatTactic, ProtectionPosture, TargetPriority};

const CHOICE_TITLE_FONT_SIZE: f32 = 18.0;
const PLAN_ICON_GAP: f32 = 6.0;
const TACTIC_AFFINITY_TEXT_GAP: f32 = 5.0;
const MISSION_PLAN_ICON_SIZE: f32 = 25.0;
const MISSION_PLAN_FONT_SIZE: f32 = 20.0;
/// Separates the plan's percentage from the fleet strength in both headers.
pub(super) const PLAN_STRENGTH_GAP: f32 = 24.0;

fn tactic_key(tactic: CombatTactic) -> &'static str {
    match tactic {
        CombatTactic::Swarm => "tactic swarm",
        CombatTactic::Skirmish => "tactic skirmish",
        CombatTactic::Breakthrough => "tactic breakthrough",
        CombatTactic::Siege => "tactic siege",
        CombatTactic::Interception => "tactic interception",
        CombatTactic::Bastion => "tactic bastion",
    }
}
fn target_key(priority: TargetPriority) -> &'static str {
    priority.image_key()
}
fn protection_key(posture: ProtectionPosture) -> &'static str {
    posture.image_key()
}
fn paint_option_icon(ui: &Ui, images: &ImageIds, key: &str, rect: egui::Rect) {
    ui.painter().image(
        images.get(key),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        if ui.is_enabled() {
            Color32::WHITE
        } else {
            Color32::from_gray(100)
        },
    );
}
pub(super) fn affinity_color(fit: u8) -> Color32 {
    if fit >= 67 {
        Color32::from_rgb(142, 210, 177)
    } else if fit >= 34 {
        Color32::from_rgb(236, 194, 119)
    } else {
        Color32::from_rgb(232, 151, 142)
    }
}

fn selection_card(
    ui: &mut Ui,
    images: &ImageIds,
    width: f32,
    label: &str,
    name: &str,
    icon: &str,
    automatic_tactic: Option<&mut bool>,
) -> Response {
    let stacked = width < 160.0;
    let height = if stacked {
        112.0
    } else {
        84.0
    };
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    let visuals = ui.style().interact(&response);
    let menu_open = egui::Popup::is_id_open(ui.ctx(), egui::Popup::default_response_id(&response));
    let (fill, stroke) = if response.is_pointer_button_down_on() || menu_open {
        (Color32::from_rgb(58, 88, 105), Stroke::new(1.5, Color32::from_rgb(155, 225, 245)))
    } else if response.hovered() {
        (Color32::from_rgb(44, 71, 88), Stroke::new(1.0, Color32::from_rgb(116, 211, 245)))
    } else {
        (visuals.bg_fill, visuals.bg_stroke)
    };
    ui.painter().rect(rect, 5.0, fill, stroke, StrokeKind::Inside);
    let available_text = (width
        - if stacked {
            12.0
        } else {
            80.0
        })
    .max(20.0);
    let fit_text = |text: &str, max_size: f32, min_size: f32, color, available: f32| {
        let mut size = max_size;
        loop {
            let galley = ui.painter().layout_no_wrap(
                text.to_owned(),
                egui::FontId::proportional(size),
                color,
            );
            if galley.size().x <= available {
                return galley;
            }
            if size <= min_size {
                return ui.painter().layout(
                    text.to_owned(),
                    egui::FontId::proportional(size),
                    color,
                    available,
                );
            }
            size -= 0.5;
        }
    };
    let heading = fit_text(
        label,
        12.0,
        8.0,
        ui.visuals().weak_text_color(),
        (available_text - 16.0).max(20.0),
    );
    let heading_center = if stacked {
        13.0
    } else {
        18.0
    };
    let heading_top = rect.top() + heading_center - heading.size().y * 0.5;
    paint_option_icon(
        ui,
        images,
        icon,
        if stacked {
            egui::Rect::from_center_size(
                rect.center_top() + egui::vec2(0.0, 48.0),
                egui::Vec2::splat(44.0),
            )
        } else {
            egui::Rect::from_min_size(
                egui::pos2(rect.left() + 8.0, heading_top),
                egui::Vec2::splat(52.0),
            )
        },
    );
    ui.painter().galley(
        egui::pos2(
            if stacked {
                rect.center().x - heading.size().x * 0.5
            } else {
                rect.left() + 70.0
            },
            heading_top,
        ),
        heading,
        ui.visuals().weak_text_color(),
    );
    let galley = fit_text(name, 14.0, 8.0, ui.visuals().text_color(), available_text);
    let name_center = if stacked {
        82.0
    } else {
        38.0
    };
    let text_pos = egui::pos2(
        if stacked {
            rect.center().x - galley.size().x * 0.5
        } else {
            rect.left() + 70.0
        },
        rect.top() + name_center - galley.size().y * 0.5,
    );
    ui.painter().galley(text_pos, galley, ui.visuals().text_color());
    if let Some(automatic_tactic) = automatic_tactic {
        let auto_font_size = 12.0;
        let auto_gap = 8.0;
        let auto_width = ui
            .painter()
            .layout_no_wrap(
                "Auto".to_owned(),
                egui::FontId::proportional(auto_font_size),
                Color32::WHITE,
            )
            .size()
            .x;
        let toggle_height = if stacked {
            14.0
        } else {
            16.0
        };
        let toggle_width = auto_width + auto_gap + 2.0 * toggle_height;
        let toggle_rect = egui::Rect::from_min_size(
            if stacked {
                rect.center_top() + egui::vec2(-toggle_width * 0.5, 94.0)
            } else {
                rect.min + egui::vec2(70.0, 54.0)
            },
            egui::vec2(toggle_width, toggle_height),
        );
        // The card already reserved this space; keep the switch from advancing the card row.
        let mut toggle_ui = ui.new_child(
            UiBuilder::new().max_rect(toggle_rect).layout(Layout::left_to_right(Align::Center)),
        );
        toggle_ui.spacing_mut().item_spacing.x = auto_gap;
        toggle_ui.spacing_mut().interact_size.y = toggle_height;
        let inactive = &mut toggle_ui.visuals_mut().widgets.inactive;
        inactive.bg_fill = Color32::from_rgb(23, 32, 42);
        inactive.bg_stroke = Stroke::new(1.0, Color32::from_rgb(85, 109, 130));
        let label = toggle_ui
            .add(egui::Label::new(RichText::new("Auto").size(auto_font_size)).sense(Sense::click()))
            .on_hover_cursor(CursorIcon::PointingHand);
        if label.clicked() {
            *automatic_tactic = !*automatic_tactic;
            set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
        }
        (label | toggle_ui.add(toggle(automatic_tactic))).on_hover_ui(|ui| {
            ui.label(RichText::new("Auto-select highest affinity").size(16.0));
        });
    }
    let arrow = rect.right_top() + egui::vec2(-12.0, heading_center);
    ui.painter().add(egui::Shape::line(
        vec![
            arrow + egui::vec2(-4.0, -2.0),
            arrow + egui::vec2(0.0, 2.0),
            arrow + egui::vec2(4.0, -2.0),
        ],
        Stroke::new(1.5, ui.visuals().text_color()),
    ));
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::Button,
            ui.is_enabled(),
            format!("{label}: {name}"),
        )
    });
    response.on_hover_cursor(CursorIcon::PointingHand)
}

/// Keeps all three choice lists consistent, including feedback while the pointer is held down.
fn paint_choice_frame(ui: &Ui, response: &Response, selected: bool) {
    let pressed = response.is_pointer_button_down_on();
    let fill = if pressed {
        Color32::from_rgb(58, 88, 105)
    } else if response.hovered() {
        Color32::from_rgb(44, 71, 88)
    } else if selected {
        Color32::from_rgb(31, 64, 78)
    } else {
        Color32::from_rgb(23, 32, 42)
    };
    ui.painter().rect(
        response.rect,
        4.0,
        fill,
        Stroke::new(
            1.0,
            if selected || response.hovered() || pressed {
                Color32::from_rgb(116, 211, 245)
            } else {
                Color32::from_rgb(55, 75, 90)
            },
        ),
        StrokeKind::Inside,
    );
}

fn tactic_choice(
    ui: &mut Ui,
    images: &ImageIds,
    tactic: CombatTactic,
    plan: &BattlePlan,
    army: &Army,
) -> Response {
    ui.spacing_mut().item_spacing.y = 4.0;
    let fit = BattlePlan {
        automatic_tactic: false,
        tactic,
        ..*plan
    }
    .suitability(army);
    let selected = tactic == plan.tactic;
    let text_color = ui.visuals().text_color();
    let title = ui.painter().layout_no_wrap(
        tactic.name().to_owned(),
        egui::FontId::proportional(CHOICE_TITLE_FONT_SIZE),
        text_color,
    );
    // Preserve the title-to-matchup spacing and bottom padding when aligning the title top.
    let matchup_top = 8.0 + title.size().y * 0.5 + 17.0;
    let height = matchup_top + 30.0 + 24.0 + 8.0;
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), height), Sense::click());
    paint_choice_frame(ui, &response, selected);
    paint_option_icon(
        ui,
        images,
        tactic_key(tactic),
        egui::Rect::from_min_size(rect.min + egui::vec2(8.0, 8.0), egui::vec2(52.0, 52.0)),
    );
    ui.painter().text(
        rect.min + egui::vec2(34.0, 78.0),
        Align2::CENTER_CENTER,
        format!("{fit}%"),
        egui::FontId::proportional(16.0),
        affinity_color(fit),
    );
    ui.painter().galley(rect.min + egui::vec2(80.0, 8.0), title, text_color);
    let column = (rect.width() - 84.0) * 0.5;
    for favorable in [true, false] {
        let x = 80.0
            + if favorable {
                0.0
            } else {
                column
            };
        for (row, other) in CombatTactic::ALL
            .into_iter()
            .filter(|other| {
                if favorable {
                    tactic.counters(*other)
                } else {
                    other.counters(tactic)
                }
            })
            .enumerate()
        {
            let y = matchup_top + row as f32 * 30.0;
            let icon_rect =
                egui::Rect::from_min_size(rect.min + egui::vec2(x, y), egui::vec2(24.0, 24.0));
            paint_option_icon(ui, images, tactic_key(other), icon_rect);
            ui.interact(icon_rect, response.id.with(other.name()), Sense::hover())
                .on_hover_small(other.name());
            // Countered fleets receive no direct penalty: red is the opponent's maximum
            // counter bonus, whose actual value depends on their unknown composition.
            let text = if favorable {
                format!("+{:.1}%", f32::from(fit) * 0.15)
            } else {
                "-15%".to_owned()
            };
            let color = if favorable {
                Color32::from_rgb(142, 210, 177)
            } else {
                Color32::from_rgb(232, 151, 142)
            };
            ui.painter().text(
                rect.min + egui::vec2(x + 30.0, y + 12.0),
                Align2::LEFT_CENTER,
                text,
                egui::FontId::proportional(12.0),
                color,
            );
        }
    }
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tactic.name())
    });
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn described_option(
    ui: &mut Ui,
    images: &ImageIds,
    icon: &str,
    name: &str,
    description: &str,
    selected: bool,
) -> Response {
    ui.spacing_mut().item_spacing.y = 4.0;
    let width = ui.available_width();
    let text_width = (width - 84.0).max(40.0);
    let text_color = ui.visuals().text_color();
    let title = ui.painter().layout(
        name.to_owned(),
        egui::FontId::proportional(CHOICE_TITLE_FONT_SIZE),
        text_color,
        text_width,
    );
    let body = ui.painter().layout(
        description.to_owned(),
        egui::FontId::proportional(12.0),
        text_color,
        text_width,
    );
    let title_height = title.size().y;
    let height = (title_height + 6.0 + body.size().y).max(52.0) + 20.0;
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, height), Sense::click());
    paint_choice_frame(ui, &response, selected);
    paint_option_icon(
        ui,
        images,
        icon,
        egui::Rect::from_min_size(rect.min + egui::vec2(10.0, 10.0), egui::Vec2::splat(52.0)),
    );
    let title_pos = rect.min + egui::vec2(74.0, 10.0);
    ui.painter().galley(title_pos, title, text_color);
    ui.painter().galley(title_pos + egui::vec2(0.0, title_height + 6.0), body, text_color);
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), name));
    response.on_hover_cursor(CursorIcon::PointingHand)
}

fn choice_popup(
    response: &Response,
    width: f32,
    max_height: f32,
    scrollbar_at_edge: bool,
    contents: impl FnOnce(&mut Ui),
) {
    let mut frame = egui::Frame::popup(&response.ctx.global_style());
    // The shorter Protection list keeps equal padding even when no scrollbar is needed.
    let width = if scrollbar_at_edge {
        let right_margin = frame.inner_margin.right;
        frame.inner_margin.right = 0;
        width + f32::from(right_margin)
    } else {
        width
    };
    egui::Popup::menu(response).frame(frame).show(|ui| {
        ui.set_width(width);
        ui.spacing_mut().scroll.floating = false;
        ui.spacing_mut().scroll.bar_inner_margin = 2.0;
        let content_margin = egui::Margin {
            right: 0,
            ..ui.spacing().scroll.content_margin
        };
        ScrollArea::vertical()
            .max_height(max_height)
            .content_margin(content_margin)
            .show(ui, contents);
    });
}

/// Four priority choices use the full card width with equal popup padding and no scrollbar.
fn priority_popup(response: &Response, width: f32, images: &ImageIds, plan: &mut BattlePlan) {
    egui::Popup::menu(response).show(|ui| {
        ui.set_width(width);
        for priority in TargetPriority::ALL {
            if described_option(
                ui,
                images,
                target_key(priority),
                priority.name(),
                priority.description(),
                plan.target_priority == priority,
            )
            .clicked()
            {
                plan.target_priority = priority;
                ui.close();
            }
        }
    });
}

/// Three cards select battle plans, with automatic tactic selection on the tactic card.
pub(super) fn draw_battle_plan_editor(
    ui: &mut Ui,
    images: &ImageIds,
    plan: &mut BattlePlan,
    army: &Army,
) -> bool {
    let previous = *plan;
    *plan = plan.resolved(army);
    ui.spacing_mut().item_spacing.y = 8.0;
    let width = (ui.ctx().content_rect().width() - 36.0).clamp(160.0, 370.0);
    let max_height = (ui.ctx().content_rect().height() - 70.0).clamp(120.0, 640.0);
    let card_gap = 8.0;
    let card_width = ((ui.available_width() - 2.0 * card_gap) / 3.0).max(40.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = card_gap;
        let response = selection_card(
            ui,
            images,
            card_width,
            "Priority",
            plan.target_priority.name(),
            target_key(plan.target_priority),
            None,
        );
        priority_popup(&response, width, images, plan);
        let response = selection_card(
            ui,
            images,
            card_width,
            "Protection",
            plan.protection.name(),
            protection_key(plan.protection),
            None,
        );
        choice_popup(&response, width, max_height, false, |ui| {
            for posture in ProtectionPosture::ALL {
                if described_option(
                    ui,
                    images,
                    protection_key(posture),
                    posture.name(),
                    posture.description(),
                    plan.protection == posture,
                )
                .clicked()
                {
                    plan.protection = posture;
                    ui.close();
                }
            }
        });
        let response = selection_card(
            ui,
            images,
            card_width,
            "Tactic",
            plan.tactic.name(),
            tactic_key(plan.tactic),
            Some(&mut plan.automatic_tactic),
        );
        *plan = plan.resolved(army);
        choice_popup(&response, width.min(300.0), max_height, true, |ui| {
            for tactic in CombatTactic::ALL {
                if tactic_choice(ui, images, tactic, plan, army).clicked() {
                    plan.tactic = tactic;
                    plan.automatic_tactic = false;
                    ui.close();
                }
            }
        });
    });
    if !ui.is_enabled() {
        *plan = previous;
        return false;
    }
    *plan != previous
}

#[derive(Clone)]
struct PlanHover {
    rect: egui::Rect,
    last_hover: f64,
    child_open: bool,
}

/// Reserves the full percentage width so active-mission routes stay aligned as affinity changes.
pub(super) fn mission_plan_button_width(ui: &Ui) -> f32 {
    let text_width = ui
        .painter()
        .layout_no_wrap(
            "100%".to_owned(),
            egui::FontId::proportional(MISSION_PLAN_FONT_SIZE),
            Color32::WHITE,
        )
        .size()
        .x;
    3.0 * MISSION_PLAN_ICON_SIZE + 2.0 * PLAN_ICON_GAP + TACTIC_AFFINITY_TEXT_GAP + text_width
}

/// Opens on hover and stays available while the player uses a nested choice list.
pub(super) fn draw_mission_plan_button(
    ui: &mut Ui,
    images: &ImageIds,
    plan: &mut BattlePlan,
    army: &Army,
) -> Response {
    let width = mission_plan_button_width(ui);
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(width, MISSION_PLAN_ICON_SIZE), Sense::hover());
    let first_icon = egui::Rect::from_min_size(rect.min, egui::Vec2::splat(MISSION_PLAN_ICON_SIZE));
    paint_plan_badge(ui, images, first_icon, *plan, army, MISSION_PLAN_FONT_SIZE);
    draw_plan_hover(ui, images, response, plan, army)
}

/// Measures all three order icons and the tactic's affinity before placing a header badge.
pub(super) fn plan_badge_width(
    ui: &Ui,
    icon_size: f32,
    plan: BattlePlan,
    army: &Army,
    font_size: f32,
) -> f32 {
    2.0 * (icon_size + PLAN_ICON_GAP) + tactic_badge_width(ui, icon_size, plan, army, font_size)
}

/// Measures the compact tactic-only badge used in the world overview.
pub(super) fn tactic_badge_width(
    ui: &Ui,
    icon_size: f32,
    plan: BattlePlan,
    army: &Army,
    font_size: f32,
) -> f32 {
    let fit = plan.resolved(army).suitability(army);
    let text_width = ui
        .painter()
        .layout_no_wrap(
            format!("{fit}%"),
            egui::FontId::proportional(font_size),
            affinity_color(fit),
        )
        .size()
        .x;
    icon_size + TACTIC_AFFINITY_TEXT_GAP + text_width
}

/// Paints priority, protection, and tactic, followed by the tactic's affinity.
fn paint_plan_badge(
    ui: &Ui,
    images: &ImageIds,
    first_icon: egui::Rect,
    plan: BattlePlan,
    army: &Army,
    font_size: f32,
) -> egui::Rect {
    let plan = plan.resolved(army);
    let mut icon = first_icon;
    for (index, key) in
        [target_key(plan.target_priority), protection_key(plan.protection), tactic_key(plan.tactic)]
            .into_iter()
            .enumerate()
    {
        icon = first_icon
            .translate(egui::vec2(index as f32 * (first_icon.width() + PLAN_ICON_GAP), 0.0));
        paint_option_icon(ui, images, key, icon);
    }
    first_icon.union(paint_tactic_affinity(ui, icon, plan, army, font_size))
}

/// Opens the shared editor from any order icon or the tactic's affinity percentage.
pub(super) fn draw_plan_button_at(
    ui: &mut Ui,
    images: &ImageIds,
    rect: egui::Rect,
    plan: &mut BattlePlan,
    army: &Army,
    font_size: f32,
) -> bool {
    let badge = paint_plan_badge(ui, images, rect, *plan, army, font_size);
    draw_badge_hover_at(ui, images, badge, plan, army)
}

/// Shows only the tactic and affinity while retaining the full battle-plan editor on hover.
pub(super) fn draw_tactic_button_at(
    ui: &mut Ui,
    images: &ImageIds,
    rect: egui::Rect,
    plan: &mut BattlePlan,
    army: &Army,
    font_size: f32,
) -> bool {
    paint_option_icon(ui, images, tactic_key(plan.resolved(army).tactic), rect);
    let badge = rect.union(paint_tactic_affinity(ui, rect, *plan, army, font_size));
    draw_badge_hover_at(ui, images, badge, plan, army)
}

fn draw_badge_hover_at(
    ui: &mut Ui,
    images: &ImageIds,
    badge: egui::Rect,
    plan: &mut BattlePlan,
    army: &Army,
) -> bool {
    let response = ui.interact(badge, ui.id().with("garrison battle plan"), Sense::hover());
    let response = draw_plan_hover(ui, images, response, plan, army);
    ui.ctx().data_mut(|data| {
        data.get_temp::<PlanHover>(response.id.with("battle plan hover")).is_some()
    })
}

fn draw_plan_hover(
    ui: &mut Ui,
    images: &ImageIds,
    response: Response,
    plan: &mut BattlePlan,
    army: &Army,
) -> Response {
    *plan = plan.resolved(army);
    let response = response.on_hover_cursor(CursorIcon::Default);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Image, ui.is_enabled(), "Battle plan")
    });
    let id = response.id.with("battle plan hover");
    let now = ui.input(|input| input.time);
    let previous = ui.ctx().data_mut(|data| data.get_temp::<PlanHover>(id));
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let hovering = response.hovered()
        || previous.as_ref().is_some_and(|state| {
            pointer.is_some_and(|point| state.rect.expand(8.0).contains(point))
                || (state.child_open && egui::Popup::is_any_open(ui.ctx()))
        });
    let last_hover = if hovering {
        now
    } else {
        previous.as_ref().map_or(-1.0, |state| state.last_hover)
    };
    let open = response.enabled() && (hovering || now - last_hover < 0.25);
    let frame = egui::Frame::popup(&ui.ctx().global_style()).inner_margin(8);
    if let Some(popup) =
        egui::Popup::from_response(&response).id(id).open(open).frame(frame).show(|ui| {
            ui.set_width((ui.ctx().content_rect().width() - 36.0).clamp(240.0, 560.0));
            draw_battle_plan_editor(ui, images, plan, army);
        })
    {
        let child_open = egui::Popup::is_any_open(ui.ctx());
        ui.ctx().data_mut(|data| {
            data.insert_temp(
                id,
                PlanHover {
                    rect: popup.response.rect,
                    last_hover,
                    child_open,
                },
            )
        });
    } else {
        ui.ctx().data_mut(|data| data.remove::<PlanHover>(id));
    }
    response
}

pub(super) fn mission_plan_visible(mission: &Mission, viewer: PlayerId) -> bool {
    mission.objective != Icon::MissileStrike && mission.commanded_army(viewer).is_some()
}

/// Paints the affinity and returns its measured bounds for the header's hover target.
pub(super) fn paint_tactic_affinity(
    ui: &Ui,
    rect: egui::Rect,
    plan: BattlePlan,
    army: &Army,
    font_size: f32,
) -> egui::Rect {
    let fit = plan.resolved(army).suitability(army);
    ui.painter().text(
        rect.right_center() + egui::vec2(TACTIC_AFFINITY_TEXT_GAP, 0.0),
        Align2::LEFT_CENTER,
        format!("{fit}%"),
        egui::FontId::proportional(font_size),
        affinity_color(fit),
    )
}

pub(super) fn draw_mission_plan_summary(
    ui: &mut Ui,
    images: &ImageIds,
    plan: BattlePlan,
    army: &Army,
) {
    let plan = plan.resolved(army);
    let fit = plan.suitability(army);
    for (key, name) in [
        (target_key(plan.target_priority), plan.target_priority.name()),
        (protection_key(plan.protection), plan.protection.name()),
    ] {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            ui.add_image(images.get(key), [24.0; 2]);
            ui.small(name);
        });
    }
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.add_image(images.get(tactic_key(plan.tactic)), [24.0; 2]);
        ui.small(plan.tactic.name());
        ui.label(RichText::new(format!("({fit}%)")).small().color(affinity_color(fit)));
    });
}

#[cfg(test)]
#[path = "../../../../tests/core/ui_battle_plan_controls.rs"]
mod tests;
