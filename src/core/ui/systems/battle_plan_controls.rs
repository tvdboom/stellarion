//! Objective-style fleet insignia and hover-based battle-plan selectors.

use super::*;
use crate::core::combat::plan::{BattlePlan, CombatTactic, ProtectionPosture, TargetPriority};

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
    match priority {
        TargetPriority::Standard => "target standard",
        TargetPriority::SmallShips => "target small ships",
        TargetPriority::MediumShips => "target medium ships",
        TargetPriority::CapitalShips => "target capital ships",
        TargetPriority::Defenses => "target defenses",
    }
}
fn protection_key(posture: ProtectionPosture) -> &'static str {
    match posture {
        ProtectionPosture::Standard => "protection standard",
        ProtectionPosture::ScreenCapitals => "protection screen capitals",
        ProtectionPosture::HeavyCover => "protection heavy cover",
        ProtectionPosture::ProtectBombers => "protection protect bombers",
    }
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
fn affinity_color(fit: u8) -> Color32 {
    if fit >= 67 {
        Color32::from_rgb(142, 210, 177)
    } else if fit >= 34 {
        Color32::from_rgb(236, 194, 119)
    } else {
        Color32::from_rgb(232, 151, 142)
    }
}

pub(super) fn paint_battle_plan_icon(ui: &Ui, images: &ImageIds, rect: egui::Rect) {
    ui.painter().image(
        images.get("battle plan"),
        rect,
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        if ui.is_enabled() {
            Color32::WHITE
        } else {
            Color32::from_gray(100)
        },
    );
}

fn selection_card(
    ui: &mut Ui,
    images: &ImageIds,
    width: f32,
    label: &str,
    name: &str,
    icon: &str,
    detail: &str,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, 142.0), Sense::click());
    let visuals = ui.style().interact(&response);
    ui.painter().rect(rect, 5.0, visuals.bg_fill, visuals.bg_stroke, StrokeKind::Inside);
    paint_option_icon(
        ui,
        images,
        icon,
        egui::Rect::from_center_size(
            rect.center_top() + egui::vec2(0.0, 57.0),
            egui::vec2(52.0, 52.0),
        ),
    );
    let available_text = (width - 12.0).max(20.0);
    let fit_text = |text: &str, max_size: f32, min_size: f32, color| {
        let mut size = max_size;
        loop {
            let galley = ui.painter().layout_no_wrap(
                text.to_owned(),
                egui::FontId::proportional(size),
                color,
            );
            if galley.size().x <= available_text || size <= min_size {
                return galley;
            }
            size -= 0.5;
        }
    };
    let heading = fit_text(label, 10.0, 8.0, ui.visuals().weak_text_color());
    ui.painter().galley(
        rect.center_top() + egui::vec2(-heading.size().x * 0.5, 12.0 - heading.size().y * 0.5),
        heading,
        ui.visuals().weak_text_color(),
    );
    let galley = fit_text(name, 14.0, 9.0, ui.visuals().text_color());
    let text_pos =
        rect.center_top() + egui::vec2(-galley.size().x * 0.5, 102.0 - galley.size().y * 0.5);
    ui.painter().galley(text_pos, galley, ui.visuals().text_color());
    ui.painter().text(
        rect.center_top() + egui::vec2(0.0, 125.0),
        Align2::CENTER_CENTER,
        detail,
        egui::FontId::proportional(11.0),
        Color32::from_rgb(142, 194, 213),
    );
    let arrow = rect.right_bottom() - egui::vec2(12.0, 12.0);
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
    let (rect, response) =
        ui.allocate_exact_size(egui::vec2(ui.available_width(), 94.0), Sense::click());
    let fill = if response.hovered() {
        Color32::from_rgb(44, 71, 88)
    } else if selected {
        Color32::from_rgb(31, 64, 78)
    } else {
        Color32::from_rgb(23, 32, 42)
    };
    ui.painter().rect(
        rect,
        4.0,
        fill,
        Stroke::new(
            1.0,
            if selected || response.hovered() {
                Color32::from_rgb(116, 211, 245)
            } else {
                Color32::from_rgb(55, 75, 90)
            },
        ),
        StrokeKind::Inside,
    );
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
        egui::FontId::proportional(13.0),
        affinity_color(fit),
    );
    ui.painter().text(
        rect.min + egui::vec2(80.0, 14.0),
        Align2::LEFT_CENTER,
        tactic.name(),
        egui::FontId::proportional(16.0),
        ui.visuals().text_color(),
    );
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
            let y = 30.0 + row as f32 * 30.0;
            paint_option_icon(
                ui,
                images,
                tactic_key(other),
                egui::Rect::from_min_size(rect.min + egui::vec2(x, y), egui::vec2(24.0, 24.0)),
            );
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
    let frame = egui::Frame::new()
        .inner_margin(10.0)
        .corner_radius(4.0)
        .fill(if selected {
            Color32::from_rgb(31, 64, 78)
        } else {
            Color32::from_rgb(23, 32, 42)
        })
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.horizontal_top(|ui| {
                ui.add_image(images.get(icon), [52.0; 2]);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(RichText::new(name).size(16.0).strong());
                    ui.add(egui::Label::new(RichText::new(description).size(12.0)).wrap());
                });
            });
        });
    ui.interact(frame.response.rect, ui.id().with(name), Sense::click())
        .on_hover_cursor(CursorIcon::PointingHand)
}

/// Three large cards open lists with complete descriptions and real composition fit.
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
    let card_width = ((ui.available_width() - 2.0 * ui.spacing().item_spacing.x) / 3.0).max(40.0);
    ui.horizontal_top(|ui| {
        let response = selection_card(
            ui,
            images,
            card_width,
            "Tactic",
            plan.tactic.name(),
            tactic_key(plan.tactic),
            &format!("{}% fit", plan.suitability(army)),
        );
        egui::Popup::menu(&response).show(|ui| {
            ui.set_width(width);
            if ui.checkbox(&mut plan.automatic_tactic, "Auto-select highest affinity").changed() {
                *plan = plan.resolved(army);
            }
            ScrollArea::vertical().max_height(max_height).show(ui, |ui| {
                for tactic in CombatTactic::ALL {
                    if tactic_choice(ui, images, tactic, plan, army).clicked() {
                        plan.tactic = tactic;
                        plan.automatic_tactic = false;
                        ui.close();
                    }
                }
            });
        });
        let response = selection_card(
            ui,
            images,
            card_width,
            "Priority",
            plan.target_priority.name(),
            target_key(plan.target_priority),
            "",
        );
        egui::Popup::menu(&response).show(|ui| {
            ui.set_width(width);
            ScrollArea::vertical().max_height(max_height).show(ui, |ui| {
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
        });
        let response = selection_card(
            ui,
            images,
            card_width,
            "Protection",
            plan.protection.name(),
            protection_key(plan.protection),
            "",
        );
        egui::Popup::menu(&response).show(|ui| {
            ui.set_width(width);
            ScrollArea::vertical().max_height(max_height).show(ui, |ui| {
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

/// Opens on hover and stays available while the player uses a nested choice list.
pub(super) fn draw_mission_plan_button(
    ui: &mut Ui,
    images: &ImageIds,
    plan: &mut BattlePlan,
    army: &Army,
) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(25.0, 25.0), Sense::click());
    draw_plan_hover(ui, images, response, rect, plan, army, false)
}

/// Places the shared editor in a compact header slot without changing the header's layout.
pub(super) fn draw_plan_button_at(
    ui: &mut Ui,
    images: &ImageIds,
    rect: egui::Rect,
    plan: &mut BattlePlan,
    army: &Army,
) -> bool {
    let response = ui.interact(rect, ui.id().with("garrison battle plan"), Sense::click());
    let response = draw_plan_hover(ui, images, response, rect, plan, army, true);
    ui.ctx().data_mut(|data| {
        data.get_temp::<PlanHover>(response.id.with("battle plan hover")).is_some()
    })
}

fn draw_plan_hover(
    ui: &mut Ui,
    images: &ImageIds,
    response: Response,
    rect: egui::Rect,
    plan: &mut BattlePlan,
    army: &Army,
    show_tactic: bool,
) -> Response {
    *plan = plan.resolved(army);
    if show_tactic {
        paint_option_icon(ui, images, tactic_key(plan.tactic), rect);
    } else {
        paint_battle_plan_icon(ui, images, rect);
    }
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), "Battle plan")
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
    if let Some(popup) = egui::Popup::from_response(&response).id(id).open(open).show(|ui| {
        ui.set_width((ui.ctx().content_rect().width() - 36.0).clamp(240.0, 640.0));
        draw_battle_plan_editor(ui, images, plan, army);
    }) {
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

pub(super) fn paint_tactic_affinity(ui: &Ui, rect: egui::Rect, plan: BattlePlan, army: &Army) {
    let fit = plan.resolved(army).suitability(army);
    ui.painter().text(
        rect.right_center() + egui::vec2(5.0, 0.0),
        Align2::LEFT_CENTER,
        format!("{fit}%"),
        egui::FontId::proportional(14.0),
        affinity_color(fit),
    );
}

pub(super) fn draw_mission_plan_summary(
    ui: &mut Ui,
    images: &ImageIds,
    plan: BattlePlan,
    army: &Army,
) {
    let plan = plan.resolved(army);
    let fit = plan.suitability(army);
    ui.horizontal(|ui| {
        ui.add_image(images.get(tactic_key(plan.tactic)), [24.0; 2]);
        ui.small(plan.tactic.name());
        ui.label(RichText::new(format!("({fit}%)")).small().color(affinity_color(fit)));
    });
    for (key, name) in [
        (target_key(plan.target_priority), plan.target_priority.name()),
        (protection_key(plan.protection), plan.protection.name()),
    ] {
        ui.horizontal(|ui| {
            ui.add_image(images.get(key), [24.0; 2]);
            ui.small(name);
        });
    }
}

#[cfg(test)]
#[path = "../../../../tests/core/ui_battle_plan_controls.rs"]
mod tests;
