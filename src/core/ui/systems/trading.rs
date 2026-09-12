//! Trading Post notifications and bilateral resource-negotiation panels.

use super::*;
use crate::core::trading::{trading_post_capacity, trading_posts_are_adjacent};
use crate::multiplayer::model::{TradeInvitation, TradeParticipant, TradeResponse};

const TRADE_ACCENT: Color32 = Color32::from_rgb(238, 179, 82);

fn trade_button(ui: &mut Ui, label: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(label).small().strong().color(TRADE_ACCENT))
            .min_size(egui::vec2(76.0, 28.0))
            .fill(Color32::from_rgb(37, 31, 22))
            .stroke(Stroke::new(1.0, Color32::from_rgb(151, 110, 49)))
            .corner_radius(4.0),
    )
    .on_hover_cursor(CursorIcon::PointingHand)
}

fn trade_toast(ui: &mut Ui, text: impl Into<String>, buttons: impl FnOnce(&mut Ui)) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(38, 34, 27, 240))
        .stroke(Stroke::new(1.0, TRADE_ACCENT))
        .corner_radius(5.0)
        .inner_margin(egui::Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.set_max_width(350.0);
            ui.add(
                egui::Label::new(RichText::new(text.into()).small().color(TRADE_ACCENT))
                    .halign(Align::Min)
                    .wrap(),
            );
            ui.add_space(3.0);
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

fn invitation_for_route<'a>(
    session: &'a MultiplayerSession,
    turn: u64,
    player_id: PlayerId,
    other_player: PlayerId,
) -> Option<&'a TradeInvitation> {
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

fn resource_controls(ui: &mut Ui, resources: &mut Resources, available: Resources, enabled: bool) {
    egui::Grid::new("trade resource controls").num_columns(2).spacing([12.0, 7.0]).show(ui, |ui| {
        for resource in ResourceName::iter() {
            ui.label(resource.to_name());
            let amount = resources.get_mut(&resource);
            *amount = (*amount).min(available.get(&resource));
            ui.add_enabled(
                enabled,
                egui::DragValue::new(amount).range(0..=available.get(&resource)).speed(10),
            );
            ui.end_row();
        }
    });
}

fn draw_bundle(ui: &mut Ui, label: &str, resources: Resources) {
    ui.label(RichText::new(label).strong());
    ui.small(format!(
        "Metal {}   Crystal {}   Deuterium {}   ({} total)",
        resources.metal,
        resources.crystal,
        resources.deuterium,
        resources.total()
    ));
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

    if let Some((_, _, enemy_player)) = new_route {
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
        return;
    }

    let draft_id = invitation.map_or(0, |trade| trade.id);
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
    let editable = !finalized && !canceled && !session.trade_update_pending;
    let other_name = player_name(session, other_player);
    let response = show_panel_modal(
        context,
        images,
        egui::Id::new("trading post panel"),
        egui::vec2(560.0, 430.0),
        |ui, panel, content| {
            let body = draw_modal_header(
                ui,
                panel,
                content,
                RichText::new("Trading Post").strong(),
                images.get("trading post"),
            );
            let mut close = false;
            let mut action = None;
            ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                ui.vertical_centered(|ui| {
                    let own_name = map.try_get(own_planet).map_or("your post", |p| p.name.as_str());
                    let foreign_name =
                        map.try_get(other_planet).map_or("foreign post", |p| p.name.as_str());
                    ui.label(format!("{own_name} ↔ {foreign_name} · {other_name}"));
                    ui.small(format!("Your outgoing limit: {capacity} resources per turn"));
                });
                ui.add_space(10.0);

                if let Some(trade) = invitation {
                    if let Some(other) = other_participant(trade, player.id) {
                        draw_bundle(ui, &format!("{other_name} offers"), other.resources);
                        ui.small(format!("Status: {:?}", other.response));
                    }
                    ui.separator();
                }

                ui.label(RichText::new("You offer").strong());
                resource_controls(ui, &mut state.trade_resources, player.resources, editable);
                let total = state.trade_resources.total();
                let valid = total > 0
                    && total <= capacity
                    && player.resources.contains(state.trade_resources);
                let color = if total <= capacity { Color32::GRAY } else { Color32::RED };
                ui.colored_label(color, format!("{total} / {capacity} selected"));
                ui.add_space(8.0);

                if finalized {
                    ui.colored_label(
                        TRADE_ACCENT,
                        "Trade accepted. Your offer is reserved now; incoming resources arrive when the turn resolves.",
                    );
                } else if canceled {
                    ui.colored_label(Color32::LIGHT_RED, "This trade was rejected.");
                } else if let Some(trade) = invitation {
                    let own = trade.participant(player.id);
                    let status = own.map_or(TradeResponse::Pending, |entry| entry.response);
                    ui.small(match status {
                        TradeResponse::Accepted => "You accepted these amounts. Changes require confirmation again.",
                        TradeResponse::Pending => "Review both offers, then accept the latest amounts.",
                        TradeResponse::Rejected => "You rejected this trade.",
                    });
                } else {
                    ui.small("The other player can set their return offer after you send this proposal.");
                }

                ui.with_layout(Layout::bottom_up(Align::Center), |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Close").clicked() {
                            close = true;
                        }
                        if !finalized && !canceled {
                            if invitation.is_some()
                                && ui
                                    .add_enabled(editable, egui::Button::new("Reject"))
                                    .clicked()
                            {
                                action = Some(TradeResponse::Rejected);
                            }
                            let label = if invitation.is_some() { "Accept" } else { "Send offer" };
                            if ui
                                .add_enabled(editable && valid, egui::Button::new(label))
                                .clicked()
                            {
                                action = Some(TradeResponse::Accepted);
                            }
                        }
                    });
                });
            });
            (close, action)
        },
    );

    let (mut close, action) = response.inner;
    close |= response.should_close();
    if let Some(action) = action {
        if let Some(trade) = invitation {
            requests.write(MultiplayerRequest::RespondTrade {
                trade_id: trade.id,
                resources: state.trade_resources,
                response: action,
            });
            if action == TradeResponse::Rejected {
                messages
                    .write(MessageMsg::info(format!("You rejected the trade with {other_name}.")));
            }
        } else if let Some(game) = session.active_game.as_ref() {
            let id = rand::random::<u64>().max(1);
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
                turn: game.persisted.state.turn,
                proposer: player.id,
                canceled: false,
                finalized: false,
                participants,
            }));
            state.trade_open = Some(id);
        }
        close = true;
    }
    if close {
        state.trading_post_open = None;
        state.trade_open = None;
        state.trade_draft_id = None;
    }
}

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
    let notification_top = 70.0_f32.max(resource_bar_bottom(context.content_rect().size()) + 112.0);
    egui::Area::new("trade_notifications".into())
        .anchor(Align2::RIGHT_TOP, egui::vec2(-12.0, notification_top))
        .order(Order::Tooltip)
        .interactable(true)
        .layout(Layout::top_down(Align::Max))
        .show(context, |ui| {
            ui.set_max_width(380.0_f32.min((context.content_rect().width() - 50.0).max(0.0)));
            ui.spacing_mut().item_spacing.y = 6.0;
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
                if invitation.finalized {
                    trade_toast(
                        ui,
                        format!("Your trade with {other_name} is confirmed for this turn."),
                        |ui| {
                            ui.horizontal(|ui| {
                                if trade_button(ui, "Review", true).clicked() {
                                    state.trade_open = Some(invitation.id);
                                }
                                if trade_button(ui, "Dismiss", true).clicked() {
                                    state.trade_notices_dismissed.insert(invitation.id);
                                }
                            });
                        },
                    );
                } else if invitation.canceled {
                    trade_toast(ui, format!("The trade with {other_name} was rejected."), |ui| {
                        if trade_button(ui, "Dismiss", true).clicked() {
                            state.trade_notices_dismissed.insert(invitation.id);
                        }
                    });
                } else if own.response == TradeResponse::Pending {
                    let text = if invitation.proposer == other.player_id {
                        format!("{other_name} proposed a Trading Post exchange.")
                    } else {
                        format!("{other_name} updated their offer. Confirm the latest trade.")
                    };
                    trade_toast(ui, text, |ui| {
                        ui.horizontal(|ui| {
                            if trade_button(ui, "Open", true).clicked() {
                                state.trade_open = Some(invitation.id);
                            }
                            if trade_button(ui, "Reject", !session.trade_update_pending).clicked() {
                                requests.write(MultiplayerRequest::RespondTrade {
                                    trade_id: invitation.id,
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
