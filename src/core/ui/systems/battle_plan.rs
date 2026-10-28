//! Shared battle-plan controls for fleet dispatch, standing orders, and reports.

use super::*;
use crate::core::combat::plan::BattlePlan;

/// Inherits standing orders once per source change while preserving an edited or restored draft.
pub(super) fn initialize_mission_plan(
    mission: &mut Mission,
    origin: &Planet,
    owner: PlayerId,
    initialized_origin: &mut Option<PlanetId>,
) {
    if *initialized_origin != Some(origin.id) {
        mission.battle_plan = origin.battle_plan_for(Some(owner));
        *initialized_origin = Some(origin.id);
    }
}

pub(super) use super::battle_plan_controls::{draw_battle_plan_editor, draw_mission_plan_button};

/// Limits a standing order to the local commander's force, including protection fleets.
pub(super) fn commandable_garrison<'a>(planet: &'a Planet, player: &Player) -> Option<&'a Army> {
    if player.spectator || planet.is_destroyed {
        None
    } else if player.controls(planet) {
        Some(planet.army.controller())
    } else {
        planet.army.protector(player.id)
    }
}

pub(super) fn can_edit_garrison_plan(planet: &Planet, player: &Player) -> bool {
    commandable_garrison(planet, player).is_some()
}

pub(super) fn draw_garrison_plan_modal(
    context: &egui::Context,
    images: &ImageIds,
    planet: &Planet,
    player: &Player,
    enabled: bool,
) -> (bool, Option<BattlePlan>) {
    let Some(army) = commandable_garrison(planet, player) else {
        return (true, None);
    };
    let available = context.content_rect().size() / game_panel_scale(context.content_rect().size())
        - egui::vec2(32.0, 32.0);
    let size = egui::vec2(520.0_f32.min(available.x), 560.0_f32.min(available.y));
    let response = show_panel_modal(
        context,
        images,
        egui::Id::new(("garrison battle plan", planet.id)),
        size,
        |ui, rect, content| {
            let header = draw_modal_header(
                ui,
                rect,
                content,
                RichText::new("Standing Battle Plan").size(21.0).strong(),
                images.get("attack"),
            );
            let footer = egui::Rect::from_min_size(
                egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
                egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
            );
            let body = egui::Rect::from_min_max(
                egui::pos2(content.left(), (header.bottom() + 12.0).min(footer.top())),
                egui::pos2(content.right(), (footer.top() - 8.0).max(header.bottom() + 12.0)),
            );
            let mut changed = None;
            ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                ui.set_clip_rect(body);
                ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.small(format!("Your forces at {}", planet.name));
                    ui.small("Applies to this garrison and becomes the default for new missions.");
                    ui.add_space(8.0);
                    let mut plan = planet.battle_plan_for(Some(player.id));
                    ui.add_enabled_ui(enabled, |ui| {
                        if draw_battle_plan_editor(ui, images, &mut plan, army) {
                            changed = Some(plan);
                        }
                    });
                    if !enabled {
                        ui.small("Continue your turn before changing battle orders.");
                    }
                });
            });
            let mut close = false;
            ui.scope(|ui| {
                style_modal_buttons(ui);
                let button = egui::Rect::from_center_size(
                    footer.center(),
                    egui::vec2(110.0_f32.min(footer.width()), MODAL_BUTTON_HEIGHT),
                );
                close = draw_modal_button(ui, button, "Close", true).clicked();
            });
            (close, changed)
        },
    );
    let should_close = response.should_close();
    let (close, changed) = response.inner;
    (close || should_close, changed)
}

struct ReportBattlePlan {
    owner: Option<PlayerId>,
    plan: BattlePlan,
    army: Army,
}

/// Uses pre-battle snapshots so casualties and later standing-order edits cannot rewrite history.
fn report_battle_plans(report: &MissionReport, side: &Side) -> Vec<ReportBattlePlan> {
    report
        .battle_plan_fleets(side)
        .into_iter()
        .map(|(owner, _, plan, army)| ReportBattlePlan {
            owner,
            plan,
            army,
        })
        .collect()
}

pub(super) fn draw_report_battle_plans(
    ui: &mut Ui,
    report: &MissionReport,
    session: &MultiplayerSession,
) {
    ui.set_max_width(440.0);
    ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
        ui.small("Plans at the start of the battle");
        ui.add(egui::Label::new(RichText::new("Tactic advantages use the surviving force's suitability at the start of each round.").small()).wrap());
        for side in [Side::Attacker, Side::Defender] {
            let opponents = report_battle_plans(report, &side.opposite());
            for entry in report_battle_plans(report, &side) {
                ui.separator();
                let name = entry.owner.map_or_else(
                    || report.planet.name.clone(),
                    |owner| {
                        session
                            .player_name(owner)
                            .map(str::to_owned)
                            .unwrap_or_else(|| format!("Player {owner}"))
                    },
                );
                ui.small(format!(
                    "{} · {name}",
                    if side == Side::Attacker {
                        "Attacker"
                    } else {
                        "Defender"
                    }
                ));
                if entry.owner.is_none() {
                    ui.small("No commander orders; tactical counters do not apply.");
                    continue;
                }
                ui.small(format!(
                    "{} · {}% suitability",
                    entry.plan.tactic.name(),
                    entry.plan.suitability(&entry.army)
                ));
                ui.small(format!(
                    "Target: {} · Protection: {}",
                    entry.plan.target_priority.name(),
                    entry.plan.protection.name()
                ));
                for opponent in &opponents {
                    let Some(owner) = opponent.owner else {
                        ui.small("No tactical counter bonus against uncommanded forces.");
                        continue;
                    };
                    let opponent_name = session.player_name(owner).map(str::to_owned).unwrap_or_else(|| format!("Player {owner}"));
                    let relationship = if entry.plan.tactic.counters(opponent.plan.tactic) {
                        "Favorable; damage advantage scales with your suitability"
                    } else if opponent.plan.tactic.counters(entry.plan.tactic) {
                        "Unfavorable; enemy damage advantage scales with their suitability"
                    } else {
                        "Neutral; neither tactic gains a counter advantage"
                    };
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!(
                                "Against {opponent_name}'s {}: {relationship}.",
                                opponent.plan.tactic.name()
                            ))
                            .small(),
                        )
                        .wrap(),
                    );
                }
            }
        }
    });
}

#[cfg(test)]
#[path = "../../../../tests/core/ui_battle_plan.rs"]
mod tests;
