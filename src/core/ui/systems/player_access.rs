//! Compact hover selectors for world protection and a patrol's rules of engagement.

use std::collections::BTreeSet;

use super::*;
use crate::core::ui::utils::HOVER_TOOLTIP_TEXT_SIZE;

#[derive(Clone)]
struct AccessHover {
    rect: egui::Rect,
    pass: u64,
    last_hover: f64,
}

/// Keeps the adjacent picker open while crossing its gap or clicking several player rows.
pub(super) fn draw_player_access_picker(
    response: &Response,
    title: &str,
    explanation: &str,
    session: &MultiplayerSession,
    local_player: PlayerId,
    selected: &BTreeSet<PlayerId>,
    editable: bool,
) -> (bool, Vec<(PlayerId, bool)>) {
    let context = &response.ctx;
    let id = response.id.with("player access picker");
    let pass = context.cumulative_pass_nr();
    let now = context.input(|input| input.time);
    let previous = context.data_mut(|data| data.get_temp::<AccessHover>(id));
    let previous = previous.filter(|previous| previous.pass.saturating_add(1) >= pass);
    let hovering = response.hovered()
        || previous.as_ref().is_some_and(|previous| {
            context
                .pointer_hover_pos()
                .is_some_and(|point| previous.rect.expand(6.0).contains(point))
        });
    let last_hover = if hovering {
        now
    } else {
        previous.as_ref().map_or(-1.0, |previous| previous.last_hover)
    };
    let open = hovering || (previous.is_some() && now - last_hover < 0.2);
    let width = (context.content_rect().width() - 24.0).clamp(120.0, 260.0);
    let height = (context.content_rect().height() - 110.0).max(34.0);
    let mut changes = Vec::new();
    let frame = egui::Frame::new()
        .fill(Color32::from_rgb(15, 22, 29))
        .stroke(Stroke::new(1.0, Color32::from_rgb(65, 87, 106)))
        .corner_radius(7.0)
        .inner_margin(egui::Margin::symmetric(10, 9));
    let popup = egui::Popup::from_response(response)
        .id(id)
        .open(open)
        .align(egui::emath::RectAlign::RIGHT_END)
        .align_alternatives(&[
            egui::emath::RectAlign::LEFT_END,
            egui::emath::RectAlign::RIGHT_START,
            egui::emath::RectAlign::LEFT_START,
        ])
        .gap(2.0)
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .frame(frame)
        .show(|ui| {
            ui.set_width(width - 20.0);
            ui.label(RichText::new(title).strong().size(HOVER_TOOLTIP_TEXT_SIZE));
            if !explanation.is_empty() {
                ui.small(explanation);
            }
            ui.add_space(6.0);
            ScrollArea::vertical().id_salt(id).max_height(height).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 5.0;
                let members = session.active_game.iter().flat_map(|game| {
                    game.members.iter().filter(|member| {
                        member.player_id != local_player
                            && game
                                .persisted
                                .state
                                .player(member.player_id)
                                .is_ok_and(|player| !player.spectator)
                    })
                });
                let mut count = 0;
                for member in members {
                    count += 1;
                    let allowed = selected.contains(&member.player_id);
                    let response = modal_player_row(
                        ui,
                        ui.available_width(),
                        34.0,
                        &member.display_name,
                        session.player_color(member.player_id).color().to_color32(),
                        allowed,
                        editable,
                    );
                    if response.clicked() {
                        changes.push((member.player_id, !allowed));
                    }
                }
                if count == 0 {
                    ui.small("No players available.");
                }
            });
        });
    if let Some(popup) = popup {
        context.data_mut(|data| {
            data.insert_temp(
                id,
                AccessHover {
                    rect: popup.response.rect,
                    pass,
                    last_hover,
                },
            )
        });
        (true, changes)
    } else {
        context.data_mut(|data| data.remove::<AccessHover>(id));
        (false, changes)
    }
}

#[cfg(test)]
#[path = "../../../../tests/core/ui_player_access.rs"]
mod tests;
