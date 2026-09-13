//! Trading Post notifications and bilateral resource-negotiation panels.

use super::*;
use crate::core::messages::show_notification_area;
use crate::core::trading::{
    trading_post_capacity, trading_posts_are_adjacent, visible_trading_post_owner,
};
use crate::multiplayer::model::{TradeInvitation, TradeParticipant, TradeResponse};

const TRADE_ACCENT: Color32 = Color32::from_rgb(112, 190, 255);

fn trade_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(
            RichText::new(label).small().strong().color(ABANDON_CONFIRMATION_TEXT_COLOR),
        )
        .min_size(egui::vec2(76.0, 28.0)),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

fn trade_toast(ui: &mut Ui, text: impl Into<String>, buttons: impl FnOnce(&mut Ui)) {
    let text = RichText::new(text.into()).small().color(TRADE_ACCENT);
    let text_width = egui::WidgetText::from(text.clone())
        .into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, TextStyle::Small)
        .size()
        .x
        .ceil();
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(28, 36, 48, 235))
        .stroke(Stroke::new(1.0, TRADE_ACCENT))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_width(text_width.min(ui.available_width()));
            ui.add(egui::Label::new(text).halign(Align::Min).wrap());
            ui.add_space(3.0);
            style_modal_buttons(ui);
            buttons(ui);
        });
}

fn player_name(session: &MultiplayerSession, player_id: PlayerId) -> String {
    session
        .player_name(player_id)
        .map(str::to_owned)
        .unwrap_or_else(|| format!("Player {player_id}"))
}

fn other_participant(
    invitation: &TradeInvitation,
    player_id: PlayerId,
) -> Option<&TradeParticipant> {
    invitation.participants.iter().find(|participant| participant.player_id != player_id)
}

fn invitation_for_route(
    session: &MultiplayerSession,
    turn: u64,
    player_id: PlayerId,
    other_player: PlayerId,
) -> Option<&TradeInvitation> {
    session.trades.iter().find(|invitation| {
        invitation.turn == turn
            && invitation.participant(player_id).is_some()
            && invitation.participant(other_player).is_some()
    })
}

fn route_from_enemy_post(
    map: &Map,
    player: &Player,
    enemy_planet_id: PlanetId,
) -> Option<(PlanetId, PlayerId)> {
    let enemy = map.try_get(enemy_planet_id)?;
    let enemy_player = enemy.owned?;
    (enemy_player != player.id).then_some(())?;
    map.planets
        .iter()
        .filter(|planet| planet.owned == Some(player.id))
        .filter(|planet| {
            trading_posts_are_adjacent(map, player.id, planet.id, enemy_player, enemy_planet_id)
        })
        .max_by_key(|planet| trading_post_capacity(planet, player.id))
        .map(|planet| (planet.id, enemy_player))
}

/// Reserves equal resource columns independently of the amount widget's width.
fn resource_row(
    ui: &mut Ui,
    images: &ImageIds,
    mut amount: impl FnMut(&mut Ui, ResourceName, f32),
) {
    // Retain the former frame's horizontal inset so the resource images stay in place.
    egui::Frame::NONE.inner_margin(egui::Margin::symmetric(9, 0)).show(ui, |ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        let cell_width = (ui.available_width() - 12.0) / 3.0;
        let image_width = (cell_width * 0.35).min(48.0);
        let amount_width = (cell_width - image_width - 6.0).clamp(1.0, 64.0);
        ui.horizontal(|ui| {
            for resource in ResourceName::iter() {
                ui.allocate_ui_with_layout(
                    egui::vec2(cell_width, 34.0),
                    Layout::left_to_right(Align::Center),
                    |ui| {
                        ui.set_min_width(cell_width);
                        ui.add_image(
                            images.get(resource.to_lowername()),
                            [image_width, image_width / 1.5],
                        )
                        .on_hover_text(resource.to_name());
                        amount(ui, resource, amount_width);
                    },
                );
            }
        });
    });
}

fn resource_controls(
    ui: &mut Ui,
    resources: &mut Resources,
    available: Resources,
    enabled: bool,
    images: &ImageIds,
) {
    resource_row(ui, images, |ui, resource, width| {
        style_selection_boxes(ui);
        ui.style_mut().drag_value_text_style = TextStyle::Small;
        ui.spacing_mut().button_padding = egui::vec2(4.0, 6.0);
        ui.spacing_mut().interact_size = egui::vec2(width, 34.0);
        let amount = resources.get_mut(&resource);
        if enabled {
            *amount = (*amount).min(available.get(&resource));
        }
        let maximum = if enabled {
            available.get(&resource)
        } else {
            *amount
        };
        ui.add_enabled(enabled, egui::DragValue::new(amount).range(0..=maximum).speed(10))
            .on_hover_text(resource.to_name());
    });
}

fn trade_panel_header(
    ui: &mut Ui,
    panel: egui::Rect,
    content: egui::Rect,
    images: &ImageIds,
) -> egui::Rect {
    let inset = egui::vec2(0.0, 10.0);
    draw_modal_header(
        ui,
        panel.translate(inset),
        content.translate(inset),
        RichText::new("Trading Post").size(21.0).strong().color(ABANDON_CONFIRMATION_TEXT_COLOR),
        images.get("trading post"),
    )
}

fn offer_heading(ui: &mut Ui, label: RichText, response: Option<TradeResponse>) {
    let (status, color) = match response {
        Some(TradeResponse::Pending) => ("Pending", Color32::from_rgb(229, 190, 107)),
        Some(TradeResponse::Accepted) => ("Accepted", Color32::from_rgb(121, 200, 158)),
        Some(TradeResponse::Rejected) => ("Rejected", Color32::from_rgb(232, 126, 133)),
        None => ("Draft", ABANDON_CONFIRMATION_TEXT_COLOR),
    };
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing.x = 10.0;
        ui.label(label.size(18.0).strong());
        egui::Frame::new()
            .fill(color.gamma_multiply(0.12))
            .stroke(Stroke::new(1.0, color.gamma_multiply(0.65)))
            .corner_radius(5.0)
            .inner_margin(egui::Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.label(RichText::new(status).size(13.0).strong().color(color));
            });
    });
}

fn draw_bundle(ui: &mut Ui, resources: Resources, images: &ImageIds) {
    resource_row(ui, images, |ui, resource, width| {
        ui.add_sized(
            egui::vec2(width, 34.0),
            egui::Label::new(RichText::new(resources.get(&resource).to_string()).small().strong()),
        )
        .on_hover_text(resource.to_name());
    });
    ui.small(format!("{} total", resources.total()));
}

fn trade_modal_button(ui: &mut Ui, rect: egui::Rect, label: &str, enabled: bool) -> Response {
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

fn trade_footer_buttons(
    ui: &mut Ui,
    footer: egui::Rect,
    invitation: bool,
    editable: bool,
    valid: bool,
    finished: bool,
) -> (bool, Option<TradeResponse>) {
    let count = if finished {
        1
    } else if invitation {
        3
    } else {
        2
    };
    let gap = 10.0_f32.min(footer.width() * 0.025);
    let width =
        112.0_f32.min(((footer.width() - gap * (count - 1) as f32) / count as f32).max(1.0));
    let row_width = width * count as f32 + gap * (count - 1) as f32;
    let mut left = footer.center().x - row_width * 0.5;
    let mut button_rect = || {
        let rect = egui::Rect::from_min_size(
            egui::pos2(left, footer.center().y - MODAL_BUTTON_HEIGHT * 0.5),
            egui::vec2(width, MODAL_BUTTON_HEIGHT),
        );
        left += width + gap;
        rect
    };
    let mut close = false;
    let mut action = None;
    ui.scope(|ui| {
        style_modal_buttons(ui);
        close = trade_modal_button(ui, button_rect(), "Close", true).clicked();
        if !finished {
            if invitation && trade_modal_button(ui, button_rect(), "Reject", editable).clicked() {
                action = Some(TradeResponse::Rejected);
            }
            let label = if invitation {
                "Accept"
            } else {
                "Send offer"
            };
            if trade_modal_button(ui, button_rect(), label, editable && valid).clicked() {
                action = Some(TradeResponse::Accepted);
            }
        }
    });
    (close, action)
}

fn draw_trade_panel(
    context: &egui::Context,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    requests: &mut MessageWriter<MultiplayerRequest>,
    messages: &mut MessageWriter<MessageMsg>,
    images: &ImageIds,
) {
    let new_route = state.trading_post_open.and_then(|enemy_planet| {
        route_from_enemy_post(map, player, enemy_planet)
            .map(|(own_planet, enemy_player)| (own_planet, enemy_planet, enemy_player))
    });

    // A negotiation belongs to the player pair for this turn, even when a different
    // visible post cannot establish a new route. Accepted trades remain reviewable.
    if let Some(enemy_player) = state
        .trading_post_open
        .and_then(|id| map.try_get(id))
        .and_then(|planet| visible_trading_post_owner(map, player.id, planet))
        .filter(|owner| *owner != player.id)
    {
        if let Some(existing) = invitation_for_route(
            session,
            session.active_game.as_ref().map_or(0, |game| game.persisted.state.turn),
            player.id,
            enemy_player,
        ) {
            state.trade_open = Some(existing.id);
            state.trading_post_open = None;
        }
    }

    let invitation =
        state.trade_open.and_then(|id| session.trades.iter().find(|trade| trade.id == id));
    if invitation.is_none() && new_route.is_none() {
        state.trade_open = None;
        let Some(_) = state.trading_post_open.and_then(|id| map.try_get(id)).filter(|planet| {
            visible_trading_post_owner(map, player.id, planet)
                .is_some_and(|owner| owner != player.id)
        }) else {
            state.trading_post_open = None;
            state.trade_draft_id = None;
            return;
        };
        let response = show_panel_modal(
            context,
            images,
            egui::Id::new("trading post panel"),
            egui::vec2(560.0, 280.0).min(context.content_rect().size() - egui::vec2(32.0, 32.0)),
            |ui, panel, content| {
                let header = trade_panel_header(ui, panel, content, images);
                let body = egui::Rect::from_min_max(
                    egui::pos2(content.left(), header.bottom() + 8.0),
                    egui::pos2(
                        content.right(),
                        (content.bottom() - MODAL_BUTTON_HEIGHT - 12.0).max(header.bottom() + 8.0),
                    ),
                );
                ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                    ui.set_clip_rect(body);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Label::new(RichText::new(
                            "You need a completed Trading Post of your own within range of this post to trade. Both posts must reach each other.",
                        ).size(17.0)).wrap());
                    });
                });
                let footer = egui::Rect::from_min_size(
                    egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
                    egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
                );
                trade_footer_buttons(ui, footer, false, false, false, true).0
            },
        );
        if response.inner || response.should_close() {
            state.trading_post_open = None;
            state.trade_draft_id = None;
        }
        return;
    }

    let draft_id = invitation.map_or(state.trade_open.unwrap_or(0), |trade| trade.id);
    if state.trade_draft_id != Some(draft_id) {
        state.trade_resources = invitation
            .and_then(|trade| trade.participant(player.id))
            .map_or_else(Resources::default, |participant| participant.resources);
        state.trade_draft_id = Some(draft_id);
    }

    let (own_planet, other_planet, other_player) = if let Some(trade) = invitation {
        let Some(own) = trade.participant(player.id) else {
            state.trade_open = None;
            return;
        };
        let Some(other) = other_participant(trade, player.id) else {
            state.trade_open = None;
            return;
        };
        (own.planet_id, other.planet_id, other.player_id)
    } else if let Some(route) = new_route {
        route
    } else {
        return;
    };

    let capacity =
        map.try_get(own_planet).map_or(0, |planet| trading_post_capacity(planet, player.id));
    let finalized = invitation.is_some_and(|trade| trade.finalized);
    let canceled = invitation.is_some_and(|trade| trade.canceled);
    let editable = !finalized && !canceled;
    let other_name = player_name(session, other_player);
    let response = show_panel_modal(
        context,
        images,
        egui::Id::new("trading post panel"),
        egui::vec2(560.0, 540.0).min(context.content_rect().size() - egui::vec2(32.0, 32.0)),
        |ui, panel, content| {
            let header = trade_panel_header(ui, panel, content, images);
            let footer = egui::Rect::from_min_size(
                egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
                egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
            );
            let body = egui::Rect::from_min_max(
                egui::pos2(content.left(), header.bottom() + 8.0),
                egui::pos2(content.right(), (footer.top() - 12.0).max(header.bottom() + 8.0)),
            );
            ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                ui.set_clip_rect(body);
                ui.spacing_mut().item_spacing.y = 6.0;

                if let Some(trade) = invitation {
                    if let Some(other) = other_participant(trade, player.id) {
                        offer_heading(
                            ui,
                            RichText::new(format!("{other_name} offers"))
                                .color(session.player_color(other_player).color().to_color32()),
                            Some(if canceled {
                                TradeResponse::Rejected
                            } else {
                                other.response
                            }),
                        );
                        draw_bundle(ui, other.resources, images);
                    }
                    ui.separator();
                }

                let own_response =
                    invitation.and_then(|trade| trade.participant(player.id)).map(|own| {
                        if canceled {
                            TradeResponse::Rejected
                        } else if !finalized && own.resources != state.trade_resources {
                            TradeResponse::Pending
                        } else {
                            own.response
                        }
                    });
                offer_heading(ui, RichText::new("You offer"), own_response);
                resource_controls(
                    ui,
                    &mut state.trade_resources,
                    player.resources,
                    editable,
                    images,
                );
                let total = state.trade_resources.total();
                let color = if total <= capacity {
                    Color32::GRAY
                } else {
                    Color32::LIGHT_RED
                };
                ui.label(
                    RichText::new(format!("{total} / {capacity} selected")).size(15.0).color(color),
                );
            });
            let total = state.trade_resources.total();
            let valid =
                total > 0 && total <= capacity && player.resources.contains(state.trade_resources);
            trade_footer_buttons(
                ui,
                footer,
                invitation.is_some(),
                editable && !session.trade_update_pending,
                valid,
                finalized || canceled,
            )
        },
    );

    let (mut close, action) = response.inner;
    close |= response.should_close();
    if let Some(action) = action {
        if let Some(trade) = invitation {
            requests.write(MultiplayerRequest::RespondTrade {
                trade_id: trade.id,
                expected_revision: trade.revision,
                resources: state.trade_resources,
                response: action,
            });
            if action == TradeResponse::Rejected {
                state.trade_notices_dismissed.insert(trade.id);
                messages
                    .write(MessageMsg::info(format!("You rejected the trade with {other_name}.")));
            }
        } else if let Some(game) = session.active_game.as_ref() {
            let id = (rand::random::<u64>() & i64::MAX as u64).max(1);
            let mut participants = [
                TradeParticipant {
                    player_id: player.id,
                    planet_id: own_planet,
                    resources: state.trade_resources,
                    response: TradeResponse::Accepted,
                },
                TradeParticipant {
                    player_id: other_player,
                    planet_id: other_planet,
                    resources: Resources::default(),
                    response: TradeResponse::Pending,
                },
            ];
            participants.sort_by_key(|participant| participant.player_id);
            requests.write(MultiplayerRequest::CreateTrade(TradeInvitation {
                id,
                revision: 0,
                turn: game.persisted.state.turn,
                proposer: player.id,
                canceled: false,
                finalized: false,
                participants,
            }));
            state.trade_open = Some(id);
            state.trade_draft_id = Some(id);
        }
        close = action == TradeResponse::Rejected;
    } else if let Some(trade) = invitation.filter(|trade| {
        editable
            && !session.trade_update_pending
            && trade
                .participant(player.id)
                .is_some_and(|own| own.resources != state.trade_resources)
    }) {
        // Publish every edit, including dragging and clearing an amount. Edits made while a
        // request runs stay local and are published as soon as its acknowledgement arrives.
        requests.write(MultiplayerRequest::RespondTrade {
            trade_id: trade.id,
            expected_revision: trade.revision,
            resources: state.trade_resources,
            response: TradeResponse::Pending,
        });
    }
    if close {
        state.trading_post_open = None;
        state.trade_open = None;
        if state.trade_draft_id == Some(0) || action == Some(TradeResponse::Rejected) {
            state.trade_draft_id = None;
        }
    }
}

#[cfg(test)]
#[path = "../../../../tests/core/ui_trading.rs"]
mod tests;

/// Draws persistent trade invitations and the panel opened from a Trading Post marker or notice.
pub(super) fn draw_trade_notifications(
    context: &egui::Context,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    requests: &mut MessageWriter<MultiplayerRequest>,
    messages: &mut MessageWriter<MessageMsg>,
    images: &ImageIds,
) {
    // Closing the panel must not drop resource edits made during an in-flight update.
    if state.trade_open.is_none() {
        if let Some(trade) =
            state.trade_draft_id.and_then(|id| session.trades.iter().find(|trade| trade.id == id))
        {
            let own = trade.participant(player.id);
            if trade.finalized
                || trade.canceled
                || own.is_none()
                || (!session.trade_update_pending
                    && own.is_some_and(|own| own.resources == state.trade_resources))
            {
                state.trade_draft_id = None;
            } else if !session.trade_update_pending {
                requests.write(MultiplayerRequest::RespondTrade {
                    trade_id: trade.id,
                    expected_revision: trade.revision,
                    resources: state.trade_resources,
                    response: TradeResponse::Pending,
                });
            }
        }
    }
    for invitation in &session.trades {
        if (!invitation.finalized && !invitation.canceled)
            || invitation.participant(player.id).is_none()
        {
            continue;
        }
        let Some(other) = other_participant(invitation, player.id) else {
            continue;
        };
        if state.trade_notices_dismissed.insert(invitation.id) {
            let other_name = player_name(session, other.player_id);
            if invitation.finalized {
                // Close only this negotiation once both offers are confirmed by the backend.
                if state.trade_open == Some(invitation.id) {
                    state.trade_open = None;
                    state.trading_post_open = None;
                }
                if state.trade_draft_id == Some(invitation.id) {
                    state.trade_draft_id = None;
                }
                messages.write(
                    MessageMsg::info(format!(
                        "Your trade with {other_name} is confirmed for this turn."
                    ))
                    .with_duration(std::time::Duration::from_secs(2)),
                );
            } else {
                messages
                    .write(MessageMsg::info(format!("The trade with {other_name} was rejected.")));
            }
        }
    }

    show_notification_area(context, "trade_notifications", true, f32::INFINITY, |ui| {
        for invitation in &session.trades {
            let Some(own) = invitation.participant(player.id) else {
                continue;
            };
            if state.trade_open == Some(invitation.id)
                || state.trade_notices_dismissed.contains(&invitation.id)
            {
                continue;
            }
            let Some(other) = other_participant(invitation, player.id) else {
                continue;
            };
            let other_name = player_name(session, other.player_id);
            if own.response == TradeResponse::Accepted {
                trade_toast(
                    ui,
                    format!("Your trade with {other_name} is waiting for acceptance."),
                    |ui| {
                        if trade_button(ui, "Reopen trade", true).clicked() {
                            state.trade_open = Some(invitation.id);
                        }
                    },
                );
            } else if own.response == TradeResponse::Pending {
                let text = if invitation.proposer == other.player_id {
                    format!("{other_name} proposed a resource trade.")
                } else {
                    format!("Your trade with {other_name} needs confirmation.")
                };
                trade_toast(ui, text, |ui| {
                    ui.horizontal(|ui| {
                        if trade_button(ui, "Open", true).clicked() {
                            state.trade_open = Some(invitation.id);
                        }
                        if trade_button(ui, "Reject", !session.trade_update_pending).clicked() {
                            requests.write(MultiplayerRequest::RespondTrade {
                                trade_id: invitation.id,
                                expected_revision: invitation.revision,
                                resources: own.resources,
                                response: TradeResponse::Rejected,
                            });
                        }
                    });
                });
            }
        }
    });

    if state.trade_open.is_some() || state.trading_post_open.is_some() {
        draw_trade_panel(context, state, map, player, session, requests, messages, images);
    }
}
