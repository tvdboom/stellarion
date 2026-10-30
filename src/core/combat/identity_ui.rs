//! Shared hover details for the compact commander panels in both combat views.

use bevy_egui::egui::{self, Color32, RichText};

use super::plan::{BattlePlan, CombatTactic};
use crate::core::ui::utils::ImageIds;

#[derive(Clone)]
pub(super) struct CombatIdentityPlan {
    pub plan: BattlePlan,
    pub affinity: u8,
    /// Matchups against the opposing commanders present at this round's start.
    pub damage_bonuses: Vec<(CombatTactic, u16)>,
}

pub(super) fn show_identity_tooltip(
    ui: &mut egui::Ui,
    panel: egui::Rect,
    id: egui::Id,
    plans: &[CombatIdentityPlan],
    images: &ImageIds,
) {
    if plans.is_empty() {
        return;
    }
    let response = ui.interact(panel, id, egui::Sense::hover());
    let card_rect_id = id.with("hover card rect");
    let pointer = ui.input(|input| input.pointer.hover_pos());
    let card_hovered = ui.ctx().data(|data| {
        data.get_temp::<egui::Rect>(card_rect_id)
            .is_some_and(|rect| pointer.is_some_and(|pos| rect.contains(pos)))
    });
    // Normal tooltip timing dismisses tooltips on wheel input, before checking their contents.
    // Keep the scrollable card open over either surface, including while the wheel is moving.
    let open =
        response.hovered() || card_hovered || egui::Tooltip::should_show_tooltip(&response, true);
    let width = 300.0_f32.min((ui.ctx().content_rect().width() - 48.0).max(120.0));
    let frame = identity_tooltip_frame(ui.style());
    let mut tooltip = egui::Tooltip::for_widget(&response).width(width).gap(8.0);
    tooltip.popup = tooltip.popup.open(open).frame(frame);
    if let Some(shown) = tooltip.show(|ui| draw_identity_details(ui, plans, images)) {
        ui.ctx().data_mut(|data| data.insert_temp(card_rect_id, shown.response.rect));
    } else {
        ui.ctx().data_mut(|data| data.remove::<egui::Rect>(card_rect_id));
    }
}

pub(super) fn identity_tooltip_frame(style: &egui::Style) -> egui::Frame {
    egui::Frame::popup(style)
        .fill(Color32::from_rgb(10, 21, 33))
        .stroke(egui::Stroke::new(1.0, Color32::from_rgb(63, 91, 113)))
        .corner_radius(5)
        .inner_margin(14)
}

pub(super) fn draw_identity_details(
    ui: &mut egui::Ui,
    plans: &[CombatIdentityPlan],
    images: &ImageIds,
) {
    let muted = Color32::from_rgb(151, 174, 194);
    let text = Color32::from_rgb(225, 235, 245);
    ui.spacing_mut().item_spacing = egui::vec2(10.0, 6.0);
    let max_height = (ui.ctx().content_rect().height() - 130.0).clamp(100.0, 400.0);
    egui::ScrollArea::vertical().max_height(max_height).show(ui, |ui| {
        for (index, entry) in plans.iter().enumerate() {
            if index > 0 {
                ui.add_space(6.0);
                ui.separator();
            }
            for (category, key, name, affinity) in [
                (
                    "Tactic",
                    entry.plan.tactic.image_key(),
                    entry.plan.tactic.name(),
                    Some(entry.affinity),
                ),
                (
                    "Target priority",
                    entry.plan.target_priority.image_key(),
                    entry.plan.target_priority.name(),
                    None,
                ),
                (
                    "Protection",
                    entry.plan.protection.image_key(),
                    entry.plan.protection.name(),
                    None,
                ),
            ] {
                ui.horizontal(|ui| {
                    ui.add(egui::Image::new((images.get(key), egui::vec2(32.0, 32.0))));
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 1.0;
                        ui.label(RichText::new(category).size(10.0).color(muted));
                        ui.label(RichText::new(name).size(14.0).color(text));
                    });
                    if let Some(affinity) = affinity {
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.with_layout(egui::Layout::top_down(egui::Align::Max), |ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                ui.label(
                                    RichText::new(format!("{affinity}% affinity"))
                                        .size(11.0)
                                        .color(Color32::from_rgb(116, 211, 245)),
                                );
                                if entry.damage_bonuses.is_empty() {
                                    ui.label(
                                        RichText::new(damage_bonus_label(0))
                                            .size(11.0)
                                            .color(muted),
                                    );
                                }
                                for (tactic, bonus) in &entry.damage_bonuses {
                                    let label = damage_bonus_label(*bonus);
                                    let label = if entry.damage_bonuses.len() > 1 {
                                        format!("{label} vs {}", tactic.name())
                                    } else {
                                        label
                                    };
                                    ui.label(RichText::new(label).size(11.0).color(
                                        if *bonus > 0 {
                                            Color32::from_rgb(133, 218, 164)
                                        } else {
                                            muted
                                        },
                                    ));
                                }
                            });
                        });
                    }
                });
            }
        }
    });
}

fn damage_bonus_label(basis_points: u16) -> String {
    let percentage = format!("{:.2}", f64::from(basis_points) / 100.0);
    format!("+{}% damage", percentage.trim_end_matches('0').trim_end_matches('.'))
}
