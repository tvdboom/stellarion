//! Trading Post notifications and bilateral resource-negotiation panels.

use super::*;
use crate::core::messages::show_notification_area;
use crate::core::messages::MessageAction;
use crate::core::trading::{
    trading_post_capacity, trading_posts_are_adjacent, visible_trading_post_owner, ResourceLoan,
};
use crate::core::ui::utils::sized_image_tile_button;
use crate::multiplayer::model::{TradeInvitation, TradeParticipant, TradeResponse};

const TRADE_ACCENT: Color32 = Color32::from_rgb(112, 190, 255);
const TRADE_SUMMARY_FONT_SIZE: f32 = 15.0;
const TRADE_PANEL_TOP_BAR_FRACTION: f32 = 0.075;
const TRADE_PANEL_VERTICAL_OFFSET: f32 = -11.0;

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

const RESOURCE_ROW_HORIZONTAL_MARGIN: i8 = 9;
const RESOURCE_HUB_ROW_WIDTH: f32 = 428.0;

/// Reserves equal resource columns independently of the amount widget's width.
fn resource_row(
    ui: &mut Ui,
    max_width: f32,
    max_image_width: f32,
    max_amount_width: f32,
    mut contents: impl FnMut(&mut Ui, ResourceName, egui::Vec2, f32),
) {
    egui::Frame::NONE
        .inner_margin(egui::Margin::symmetric(RESOURCE_ROW_HORIZONTAL_MARGIN, 0))
        .show(ui, |ui| {
            let row_width = ui.available_width().min(max_width);
            ui.vertical_centered(|ui| {
                ui.set_width(row_width);
                let resource_gap = 10.0;
                ui.spacing_mut().item_spacing.x = resource_gap;
                let cell_width = (row_width - resource_gap * 2.0) / 3.0;
                let image_width = (cell_width * 0.5).min(max_image_width);
                // Preserve the art's 3:2 ratio inside the tile's one-point border.
                let image_size = egui::vec2(image_width, (image_width - 2.0) / 1.5 + 2.0);
                let amount_width = (cell_width - image_width - 6.0).clamp(1.0, max_amount_width);
                ui.horizontal(|ui| {
                    for resource in ResourceName::iter() {
                        ui.allocate_ui_with_layout(
                            egui::vec2(cell_width, image_size.y.max(34.0)),
                            Layout::left_to_right(Align::Center),
                            |ui| {
                                ui.set_min_width(cell_width);
                                ui.spacing_mut().item_spacing.x = 6.0;
                                contents(ui, resource, image_size, amount_width);
                            },
                        );
                    }
                });
            });
        });
}

/// Keeps every trade resource framed the same way, including read-only offers.
fn resource_tile_button(
    ui: &mut Ui,
    image: egui::TextureId,
    size: egui::Vec2,
    enabled: bool,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, mut response) = ui.allocate_exact_size(size, sense);
    if enabled {
        response = response.on_hover_cursor(CursorIcon::PointingHand);
    }
    paint_bordered_resource_image(ui, image, rect, 2.0);
    response
}

fn resource_controls(
    ui: &mut Ui,
    resources: &mut Resources,
    available: Resources,
    enabled: bool,
    images: &ImageIds,
) {
    resource_row(ui, RESOURCE_HUB_ROW_WIDTH, 62.0, 64.0, |ui, resource, image_size, width| {
        style_selection_boxes(ui, None);
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
        let response =
            resource_tile_button(ui, images.get(resource.to_lowername()), image_size, enabled);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::WidgetType::Button,
                enabled && ui.is_enabled(),
                *amount > 0,
                resource.to_name(),
            )
        });
        if enabled && response.clicked() {
            *amount = maximum;
        } else if enabled && response.secondary_clicked() {
            *amount = 0;
        }
        ui.add_enabled(enabled, egui::DragValue::new(amount).range(0..=maximum).speed(10))
            .on_hover_text(RichText::new(resource.to_name()).size(15.0));
    });
}

fn trade_panel_header(
    ui: &mut Ui,
    panel: egui::Rect,
    content: egui::Rect,
    title: &str,
) -> egui::Rect {
    // The top ornament is part of the stretched panel artwork, so its height scales with it.
    let bar_height = panel.height() * TRADE_PANEL_TOP_BAR_FRACTION;
    let header = egui::Rect::from_min_size(
        egui::pos2(content.left(), panel.top()),
        egui::vec2(content.width(), bar_height),
    );
    ui.scope_builder(UiBuilder::new().max_rect(header.translate(egui::vec2(0.0, 2.0))), |ui| {
        ui.centered_and_justified(|ui| {
            ui.label(
                RichText::new(title)
                    .size(21.0_f32.min(bar_height - 4.0))
                    .strong()
                    .color(ABANDON_CONFIRMATION_TEXT_COLOR),
            );
        });
    });
    header
}

fn offer_heading(
    ui: &mut Ui,
    label: RichText,
    response: Option<TradeResponse>,
    proposer: bool,
    finalized: bool,
    text_inset: f32,
) {
    let (status, color) = match response {
        Some(TradeResponse::Pending) => ("Pending", Color32::from_rgb(229, 190, 107)),
        Some(TradeResponse::Accepted) if finalized => {
            ("Accepted", Color32::from_rgb(121, 200, 158))
        },
        Some(TradeResponse::Accepted) if proposer => ("Proposed", TRADE_ACCENT),
        Some(TradeResponse::Accepted) => ("Confirmed", TRADE_ACCENT),
        Some(TradeResponse::Rejected) => ("Rejected", Color32::from_rgb(232, 126, 133)),
        None => ("Draft", ABANDON_CONFIRMATION_TEXT_COLOR),
    };
    ui.horizontal(|ui| {
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 10.0;
            draw_status_badge(ui, status, color);
            ui.allocate_ui_with_layout(
                egui::vec2(ui.available_width(), 24.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.add_space(text_inset);
                    ui.add(egui::Label::new(label.size(18.0).strong()).wrap());
                },
            );
        });
    });
    ui.add_space(8.0);
}

fn draw_bundle(ui: &mut Ui, resources: Resources, images: &ImageIds) {
    resource_row(ui, 342.0, 52.0, 30.0, |ui, resource, image_size, width| {
        resource_tile_button(ui, images.get(resource.to_lowername()), image_size, false);
        ui.add_sized(
            egui::vec2(width, 34.0),
            egui::Label::new(RichText::new(resources.get(&resource).to_string()).small().strong()),
        )
        .on_hover_text(RichText::new(resource.to_name()).size(15.0));
    });
    ui.add_space(8.0);
    ui.label(
        RichText::new(format!("{} total", resources.total()))
            .size(TRADE_SUMMARY_FONT_SIZE)
            .color(Color32::GRAY),
    );
}

fn resource_hub_text_inset(available_width: f32) -> f32 {
    let margin = f32::from(RESOURCE_ROW_HORIZONTAL_MARGIN);
    margin + ((available_width - margin * 2.0 - RESOURCE_HUB_ROW_WIDTH).max(0.0) * 0.5)
}

fn resource_hub_text_label(ui: &mut Ui, inset: f32, text: RichText) {
    ui.horizontal(|ui| {
        ui.add_space(inset);
        ui.label(text);
    });
}

fn draw_resource_hub_bundle(ui: &mut Ui, resources: Resources, images: &ImageIds, text_inset: f32) {
    resource_row(ui, RESOURCE_HUB_ROW_WIDTH, 62.0, 64.0, |ui, resource, image_size, width| {
        resource_tile_button(ui, images.get(resource.to_lowername()), image_size, false);
        ui.add_sized(
            egui::vec2(width, 34.0),
            egui::Label::new(RichText::new(resources.get(&resource).to_string()).small().strong()),
        )
        .on_hover_text(RichText::new(resource.to_name()).size(15.0));
    });
    ui.add_space(8.0);
    resource_hub_text_label(
        ui,
        text_inset,
        RichText::new(format!("{} total", resources.total()))
            .size(TRADE_SUMMARY_FONT_SIZE)
            .color(Color32::GRAY),
    );
}

fn trade_footer_buttons(
    ui: &mut Ui,
    footer: egui::Rect,
    invitation: bool,
    editable: bool,
    valid: bool,
    changed: bool,
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
        close = draw_modal_button(ui, button_rect(), "Close", true).clicked();
        if !finished {
            if invitation && draw_modal_button(ui, button_rect(), "Reject", editable).clicked() {
                action = Some(TradeResponse::Rejected);
            }
            let label = if invitation && changed {
                "Send offer"
            } else if invitation {
                "Accept"
            } else {
                "Send offer"
            };
            if draw_modal_button(ui, button_rect(), label, editable && valid).clicked() {
                action = Some(TradeResponse::Accepted);
            }
        }
    });
    (close, action)
}

fn resource_hub_term_button(
    ui: &mut Ui,
    images: &ImageIds,
    term: ResourceLoanTerm,
    selected: bool,
    enabled: bool,
    width: f32,
    principal: Resources,
) -> Response {
    let (image, label, turns, premium) = match term {
        ResourceLoanTerm::Short => ("loan short", "Short-term", 1, 10),
        ResourceLoanTerm::Medium => ("loan medium", "Mid-term", 2, 30),
        ResourceLoanTerm::Long => ("loan long", "Long-term", 3, 50),
    };
    let contents = ui
        .add_enabled_ui(enabled, |ui| {
            ui.allocate_ui_with_layout(
                egui::vec2(width, 72.0),
                Layout::top_down(Align::Center),
                |ui| {
                    let image = sized_image_tile_button(
                        ui,
                        images.get(image),
                        selected,
                        egui::vec2(70.0, 47.0),
                    );
                    ui.add_space(1.0);
                    image.union(
                        ui.add(
                            egui::Label::new(RichText::new(label).small().strong())
                                .wrap_mode(egui::TextWrapMode::Extend),
                        ),
                    )
                },
            )
            .inner
        })
        .inner;
    let hover_region =
        ui.interact(contents.rect, ui.id().with(("resource hub term", label)), Sense::hover());
    let response = contents.union(hover_region);
    let repayment = term.repayment(principal);
    let hover = |ui: &mut Ui| {
        ui.set_max_width(360.0);
        ui.small(format!(
            "Repay after {turns} turn{} with a {premium}% premium on each borrowed resource. \
            The complete repayment is deducted automatically after production on the due turn.",
            if turns == 1 {
                ""
            } else {
                "s"
            }
        ));
        ui.add_space(4.0);
        ui.label(RichText::new("Repayment bundle").small().strong().color(TRADE_ACCENT));
        draw_bundle(ui, repayment, images);
        ui.add_space(4.0);
        ui.small(
            "If it cannot be paid in full, it remains due and retries next turn. Until \
            every overdue repayment is cleared, none of your Resource Markets can open a new \
            loan.",
        );
    };
    response
        .on_hover_ui(hover)
        .on_disabled_hover_ui(hover)
        .on_hover_cursor(CursorIcon::PointingHand)
}

fn pending_resource_loan(
    pending: &PendingTurnCommands,
    player_id: PlayerId,
    selected_planet: PlanetId,
    turn: u64,
) -> Option<ResourceLoan> {
    pending.commands.iter().chain(&pending.queued_commands).find_map(|command| match command {
        TurnCommand::BorrowResources {
            planet_id,
            resources,
            term,
        } if *planet_id == selected_planet => Some(ResourceLoan {
            player_id,
            planet_id: *planet_id,
            issued_turn: turn,
            due_turn: turn.saturating_add(term.turns()),
            principal: *resources,
            term: *term,
        }),
        _ => None,
    })
}

fn pending_early_resource_loan_repayment(
    pending: &PendingTurnCommands,
    planet_id: PlanetId,
) -> bool {
    pending.commands.iter().chain(&pending.queued_commands).any(|command| {
        matches!(
            command,
            TurnCommand::RepayResourceLoanEarly {
                planet_id: pending_planet,
            } if *pending_planet == planet_id
        )
    })
}

fn resource_loan_repayment_heading(
    loan: &ResourceLoan,
    turn: u64,
    early_repayment_pending: bool,
) -> String {
    if early_repayment_pending || loan.due_turn == turn {
        "Repayment due this turn".to_owned()
    } else if loan.due_turn < turn {
        let overdue = turn - loan.due_turn;
        format!(
            "Repayment overdue by {overdue} turn{}",
            if overdue == 1 {
                ""
            } else {
                "s"
            }
        )
    } else {
        let remaining = loan.due_turn - turn;
        if remaining == 1 {
            "Repayment due in 1 turn".to_owned()
        } else {
            format!("Repayment due in {remaining} turns")
        }
    }
}

fn draw_resource_hub_panel(
    context: &egui::Context,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    pending: &mut PendingTurnCommands,
    messages: &mut MessageWriter<MessageMsg>,
    images: &ImageIds,
) {
    let Some(planet_id) = state.trading_post_open else {
        return;
    };
    let canonical = session.active_game.as_ref().map(|game| &game.persisted.state);
    let turn = canonical.map_or(pending.turn.max(1), |model| model.turn);
    // `map` is the projected draft. A testing boost can create a completed Trading Post before
    // the turn is saved, and the Resource Hub must use the same infrastructure the player sees.
    let capacity =
        map.try_get(planet_id).map_or(0, |planet| trading_post_capacity(planet, player.id));
    let active_loan = canonical
        .and_then(|model| {
            model
                .resource_loans
                .iter()
                .find(|loan| loan.player_id == player.id && loan.planet_id == planet_id)
        })
        .cloned();
    let borrowing_blocked =
        canonical.is_some_and(|model| model.resource_hub_borrowing_blocked(player.id));
    let drafted_loan = pending_resource_loan(pending, player.id, planet_id, turn);
    let early_repayment_pending = pending_early_resource_loan_repayment(pending, planet_id);
    let displayed_loan = active_loan.as_ref().or(drafted_loan.as_ref());
    let preferred_height =
        if displayed_loan.is_some_and(|loan| !early_repayment_pending && loan.due_turn < turn) {
            480.0
        } else if displayed_loan.is_some() {
            440.0
        } else {
            470.0
        };
    let panel_size = egui::vec2(520.0, preferred_height).min(
        context.content_rect().size() / game_panel_scale(context.content_rect().size())
            - egui::vec2(32.0, 62.0),
    );
    let response = show_panel_modal_with_offset(
        context,
        images,
        egui::Id::new("resource hub panel"),
        panel_size,
        egui::vec2(0.0, TRADE_PANEL_VERTICAL_OFFSET),
        |ui, panel, content| {
            let header = trade_panel_header(ui, panel, content, "Resource Market");
            let footer = egui::Rect::from_min_size(
                egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
                egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
            );
            let body = egui::Rect::from_min_max(
                egui::pos2(content.left(), header.bottom() + 22.0),
                egui::pos2(content.right(), footer.top() - 12.0),
            );
            let mut borrow = false;
            let mut repay_early = false;
            ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                ui.set_clip_rect(body);
                let resource_text_inset = resource_hub_text_inset(ui.available_width());
                if let Some(loan) = displayed_loan {
                    let repayment = loan.repayment();
                    let drafted = active_loan.is_none();
                    resource_hub_text_label(
                        ui,
                        resource_text_inset,
                        RichText::new("Borrowed resources")
                            .size(18.0)
                            .strong()
                            .color(TRADE_ACCENT),
                    );
                    ui.add_space(4.0);
                    draw_resource_hub_bundle(
                        ui,
                        loan.principal,
                        images,
                        resource_text_inset,
                    );
                    ui.add_space(10.0);
                    resource_hub_text_label(
                        ui,
                        resource_text_inset,
                        RichText::new(resource_loan_repayment_heading(
                            loan,
                            turn,
                            early_repayment_pending,
                        ))
                        .size(18.0)
                        .strong()
                        .color(TRADE_ACCENT),
                    );
                    ui.add_space(4.0);
                    draw_resource_hub_bundle(ui, repayment, images, resource_text_inset);
                    if !early_repayment_pending && loan.due_turn < turn {
                        ui.add_space(10.0);
                        resource_hub_text_label(
                            ui,
                            resource_text_inset,
                            RichText::new(
                                "Repayment will retry automatically this turn, and new Resource Market loans remain blocked.",
                            )
                            .size(16.0),
                        );
                    }
                    if !drafted && loan.due_turn <= turn && !player.resources.contains(repayment) {
                        resource_hub_text_label(
                            ui,
                            resource_text_inset,
                            RichText::new(
                                "Insufficient resources: repayment will retry next turn.",
                            )
                            .color(Color32::from_rgb(229, 190, 107)),
                        );
                    }
                } else if borrowing_blocked {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(
                                "New Resource Market loans are unavailable while your empire has an overdue repayment.",
                            )
                            .size(17.0),
                        );
                    });
                } else if capacity == 0 {
                    ui.vertical_centered(|ui| {
                        ui.label(
                            RichText::new(
                                "This Trading Post is not available in the saved turn yet. Complete it and advance the turn before borrowing.",
                            )
                            .size(17.0),
                        );
                    });
                } else {
                    resource_hub_text_label(
                        ui,
                        resource_text_inset,
                        RichText::new("Choose a resource bundle")
                            .size(18.0)
                            .strong()
                            .color(TRADE_ACCENT),
                    );
                    ui.add_space(8.0);
                    resource_controls(
                        ui,
                        &mut state.resource_hub_resources,
                        Resources::new(capacity, capacity, capacity),
                        pending.can_accept_commands(),
                        images,
                    );
                    let total = state.resource_hub_resources.total();
                    ui.add_space(7.0);
                    resource_hub_text_label(
                        ui,
                        resource_text_inset,
                        RichText::new(format!("{total} / {capacity} selected"))
                            .size(TRADE_SUMMARY_FONT_SIZE)
                            .color(if total <= capacity {
                                Color32::GRAY
                            } else {
                                Color32::LIGHT_RED
                            }),
                    );
                    ui.add_space(13.0);
                    resource_hub_text_label(
                        ui,
                        resource_text_inset,
                        RichText::new("Choose a repayment term")
                            .size(18.0)
                            .strong()
                            .color(TRADE_ACCENT),
                    );
                    ui.add_space(7.0);
                    ui.vertical_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 38.0;
                        ui.set_width(328.0);
                        ui.columns(3, |columns| {
                            for (column, term) in columns.iter_mut().zip([
                                ResourceLoanTerm::Short,
                                ResourceLoanTerm::Medium,
                                ResourceLoanTerm::Long,
                            ]) {
                                if resource_hub_term_button(
                                    column,
                                    images,
                                    term,
                                    state.resource_hub_term == term,
                                    pending.can_accept_commands(),
                                    70.0,
                                    state.resource_hub_resources,
                                )
                                .clicked()
                                {
                                    state.resource_hub_term = term;
                                }
                            }
                        });
                    });
                }
            });
            let total = state.resource_hub_resources.total();
            let valid = displayed_loan.is_none()
                && !borrowing_blocked
                && capacity > 0
                && total > 0
                && total <= capacity
                && pending.can_accept_commands();
            let mut close = false;
            ui.scope(|ui| {
                style_modal_buttons(ui);
                let gap = 10.0;
                let width = 124.0_f32.min((footer.width() - gap) * 0.5);
                let active_repayment = active_loan.as_ref().filter(|_| !early_repayment_pending);
                let show_borrow = displayed_loan.is_none() && capacity > 0;
                let has_second_button = active_repayment.is_some() || show_borrow;
                let left = if has_second_button {
                    footer.center().x - width - gap * 0.5
                } else {
                    footer.center().x - width * 0.5
                };
                let first = egui::Rect::from_min_size(
                    egui::pos2(left, footer.center().y - MODAL_BUTTON_HEIGHT * 0.5),
                    egui::vec2(width, MODAL_BUTTON_HEIGHT),
                );
                close = draw_modal_button(ui, first, "Close", true).clicked();
                if let Some(loan) = active_repayment {
                    let repayment = loan.repayment();
                    repay_early = draw_modal_button(
                        ui,
                        first.translate(egui::vec2(width + gap, 0.0)),
                        "Repay early",
                        pending.can_accept_commands() && player.resources.contains(repayment),
                    )
                    .on_disabled_hover_text("The complete repayment bundle is not available.")
                    .clicked();
                } else if show_borrow {
                    borrow = draw_modal_button(
                        ui,
                        first.translate(egui::vec2(width + gap, 0.0)),
                        "Borrow",
                        valid,
                    )
                    .clicked();
                }
            });
            (close, borrow, repay_early)
        },
    );
    let (mut close, borrow, repay_early) = response.inner;
    close |= response.should_close();
    if borrow {
        let resources = state.resource_hub_resources;
        let term = state.resource_hub_term;
        if pending.push(TurnCommand::BorrowResources {
            planet_id,
            resources,
            term,
        }) {
            messages.write(MessageMsg::info(format!(
                "Successfully borrowed {} resources.",
                resources.total(),
            )));
            close = true;
        } else {
            messages.write(MessageMsg::info(COMMAND_LIMIT_REACHED_MESSAGE));
        }
    }
    if repay_early {
        if pending.push(TurnCommand::RepayResourceLoanEarly {
            planet_id,
        }) {
            messages.write(MessageMsg::info(
                "Full Resource Market repayment added to this turn's draft.",
            ));
            close = true;
        } else {
            messages.write(MessageMsg::info(COMMAND_LIMIT_REACHED_MESSAGE));
        }
    }
    if close {
        state.trading_post_open = None;
        state.resource_hub_resources = Resources::default();
    }
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
    let projected_route = state.trading_post_open.and_then(|enemy_planet| {
        route_from_enemy_post(map, player, enemy_planet)
            .map(|(own_planet, enemy_player)| (own_planet, enemy_planet, enemy_player))
    });
    let saved_route = session.active_game.as_ref().and_then(|game| {
        state.trading_post_open.and_then(|enemy_planet| {
            route_from_enemy_post(&game.persisted.state.map, player, enemy_planet)
                .map(|(own_planet, enemy_player)| (own_planet, enemy_planet, enemy_player))
        })
    });
    // The backend commits a completed post from the visible draft before creating this private
    // negotiation, keeping the canonical route consistent for the other participant.
    let projected_post = projected_route.is_some() && saved_route.is_none();
    let submitted_route = projected_route.is_some_and(|(_, _, enemy_player)| {
        session.active_game.as_ref().is_some_and(|game| {
            game.submitted_players.contains(&player.id)
                || game.submitted_players.contains(&enemy_player)
        })
    });
    let new_route = projected_route.filter(|_| !submitted_route);

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
            egui::vec2(560.0, 280.0).min(
                context.content_rect().size() / game_panel_scale(context.content_rect().size())
                    - egui::vec2(32.0, 32.0),
            ),
            |ui, panel, content| {
                let header = trade_panel_header(ui, panel, content, "Trading Post");
                let body = egui::Rect::from_min_max(
                    egui::pos2(content.left(), header.bottom() + 20.0),
                    egui::pos2(
                        content.right(),
                        (content.bottom() - MODAL_BUTTON_HEIGHT - 12.0).max(header.bottom() + 20.0),
                    ),
                );
                ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                    ui.set_clip_rect(body);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Label::new(RichText::new(
                            if submitted_route {
                                "One of the players has already ended this turn. New trades can begin next turn."
                            } else {
                                "Both players need completed Trading Posts, and at least one post must reach the other to trade."
                            },
                        ).size(17.0)).wrap());
                    });
                });
                let footer = egui::Rect::from_min_size(
                    egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
                    egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
                );
                trade_footer_buttons(ui, footer, false, false, false, false, true).0
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
    let response = show_panel_modal_with_offset(
        context,
        images,
        egui::Id::new("trading post panel"),
        egui::vec2(520.0, 422.0).min(
            context.content_rect().size() / game_panel_scale(context.content_rect().size())
                - egui::vec2(32.0, 86.0),
        ),
        egui::vec2(0.0, TRADE_PANEL_VERTICAL_OFFSET),
        |ui, panel, content| {
            let header = trade_panel_header(ui, panel, content, "Trading Post");
            let footer = egui::Rect::from_min_size(
                egui::pos2(content.left(), content.bottom() - MODAL_BUTTON_HEIGHT),
                egui::vec2(content.width(), MODAL_BUTTON_HEIGHT),
            );
            let body = egui::Rect::from_min_max(
                egui::pos2(content.left(), header.bottom() + 30.0),
                egui::pos2(content.right(), (footer.top() - 12.0).max(header.bottom() + 30.0)),
            );
            ui.scope_builder(UiBuilder::new().max_rect(body), |ui| {
                ui.set_clip_rect(body);
                ScrollArea::vertical()
                    .id_salt("trade offers")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.y = 5.0;
                        let resource_text_inset = resource_hub_text_inset(ui.available_width());

                        // Both players see the sender first, including before the draft is sent.
                        let own_first = invitation.is_none_or(|trade| trade.proposer == player.id);
                        let order = if own_first {
                            [player.id, other_player]
                        } else {
                            [other_player, player.id]
                        };
                        for (index, participant_id) in order.into_iter().enumerate() {
                            if index > 0 {
                                ui.add_space(16.0);
                                ui.separator();
                                ui.add_space(16.0);
                            }
                            let own = participant_id == player.id;
                            let participant =
                                invitation.and_then(|trade| trade.participant(participant_id));
                            let status = participant
                                .map(|participant| {
                                    if own
                                        && !finalized
                                        && !canceled
                                        && participant.resources != state.trade_resources
                                    {
                                        TradeResponse::Pending
                                    } else {
                                        participant.response
                                    }
                                })
                                .or_else(|| (!own).then_some(TradeResponse::Pending));
                            offer_heading(
                                ui,
                                RichText::new(player_name(session, participant_id)).color(
                                    session.player_color(participant_id).color().to_color32(),
                                ),
                                status,
                                invitation.is_some_and(|trade| trade.proposer == participant_id),
                                finalized,
                                resource_text_inset,
                            );
                            if own {
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
                                ui.add_space(8.0);
                                resource_hub_text_label(
                                    ui,
                                    resource_text_inset,
                                    RichText::new(format!("{total} / {capacity} selected"))
                                        .size(TRADE_SUMMARY_FONT_SIZE)
                                        .color(color),
                                );
                            } else {
                                draw_resource_hub_bundle(
                                    ui,
                                    participant
                                        .map_or_else(Resources::default, |other| other.resources),
                                    images,
                                    resource_text_inset,
                                );
                            }
                        }
                        // Keep the final summary reachable when a short viewport needs scrolling.
                        ui.add_space(8.0);
                    });
            });
            let total = state.trade_resources.total();
            let other_has_offer = invitation.is_none_or(|trade| {
                other_participant(trade, player.id)
                    .is_some_and(|participant| !participant.resources.is_empty())
            });
            let valid = total > 0
                && total <= capacity
                && player.resources.contains(state.trade_resources)
                && other_has_offer;
            let changed = invitation.is_some_and(|trade| {
                trade
                    .participant(player.id)
                    .is_some_and(|own| own.resources != state.trade_resources)
            });
            let needs_confirmation = invitation.is_none_or(|trade| {
                trade
                    .participant(player.id)
                    .is_some_and(|own| own.response != TradeResponse::Accepted || changed)
            });
            trade_footer_buttons(
                ui,
                footer,
                invitation.is_some(),
                editable && !session.trade_update_pending,
                valid && needs_confirmation,
                changed,
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
                first_negotiation_notice(
                    context,
                    session,
                    player.id,
                    NegotiationNotice::TradeClosed(trade.id),
                );
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
            requests.write(MultiplayerRequest::CreateTrade {
                invitation: TradeInvitation {
                    id,
                    revision: 0,
                    turn: game.persisted.state.turn,
                    proposer: player.id,
                    canceled: false,
                    finalized: false,
                    participants,
                },
                projected_post,
            });
            state.trade_open = Some(id);
            state.trade_draft_id = Some(id);
        }
        close = action == TradeResponse::Rejected;
    } else if let Some(trade) = invitation.filter(|_| editable && !session.trade_update_pending) {
        // Withdraw an accepted offer once editing begins. Keep all resource adjustments local
        // until Send offer; the old accepted amounts must not finalize in the meantime.
        if let Some(own) = trade.participant(player.id).filter(|own| {
            own.response == TradeResponse::Accepted && own.resources != state.trade_resources
        }) {
            requests.write(MultiplayerRequest::RespondTrade {
                trade_id: trade.id,
                expected_revision: trade.revision,
                resources: own.resources,
                response: TradeResponse::Pending,
            });
        }
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
    pending: &mut PendingTurnCommands,
    requests: &mut MessageWriter<MultiplayerRequest>,
    messages: &mut MessageWriter<MessageMsg>,
    images: &ImageIds,
) {
    if state.trade_open.is_none()
        && state.trade_draft_id.is_some_and(|id| {
            session.trades.iter().any(|trade| trade.id == id && (trade.finalized || trade.canceled))
        })
    {
        state.trade_draft_id = None;
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
        if first_negotiation_notice(
            context,
            session,
            player.id,
            NegotiationNotice::TradeClosed(invitation.id),
        ) {
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
                    MessageMsg::info(format!("Trade with {other_name} successful."))
                        .with_action(MessageAction::OpenTrade(invitation.id))
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
                || invitation.canceled
                || invitation.finalized
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

    let own_hub_open = state.trading_post_open.is_some_and(|planet_id| {
        map.try_get(planet_id).is_some_and(|planet| {
            visible_trading_post_owner(map, player.id, planet) == Some(player.id)
        })
    });
    if own_hub_open {
        draw_resource_hub_panel(context, state, map, player, session, pending, messages, images);
    } else if state.trade_open.is_some() || state.trading_post_open.is_some() {
        draw_trade_panel(context, state, map, player, session, requests, messages, images);
    }
}
