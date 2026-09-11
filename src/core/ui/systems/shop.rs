//! Shop panels for the game interface.

use super::*;
use crate::core::map::planet::ShieldOverloadState;
use crate::core::orders::conversion_output;
use crate::core::units::buildings::FleetWithdrawal;

/// Formats the resource output confirmed by a laboratory conversion.
pub(super) fn conversion_success_message(gain: usize, resource: ResourceName) -> MessageMsg {
    MessageMsg::info(format!("Gained {} {}.", format_thousands(gain), resource.to_name()))
}

/// Warns only when a proposed unit changes a balanced grid into an energy shortage.
pub(super) fn energy_shortage_warning(
    energy: EnergyGrid,
    unit: Unit,
    solar_band: Option<SolarBand>,
) -> Option<MessageMsg> {
    let after = energy.with_unit(unit, solar_band, 1);
    (after.balance() < energy.balance() && energy.balance() >= 0 && after.balance() < 0)
        .then(|| MessageMsg::warning("Next turn: Energy shortage."))
}

/// Returns the capacity summary relevant to the active shop category.
pub(super) fn shop_capacity_summary(
    shop: Shop,
    planet: &Planet,
) -> Option<(&'static str, usize, usize)> {
    match (shop, planet.is_moon()) {
        (Shop::Buildings, true) => Some(("Fields", planet.fields_consumed(), planet.max_fields())),
        (Shop::Fleet, false) => {
            Some(("Production", planet.fleet_production(), planet.max_fleet_production()))
        },
        (Shop::Defenses, false) => {
            Some(("Production", planet.battery_production(), planet.max_battery_production()))
        },
        (Shop::Buildings | Shop::Orbitals | Shop::Fleet | Shop::Defenses, _) => None,
    }
}

/// Draws one compact image tile used by building-specific controls.
fn image_tile_button(ui: &mut Ui, image: egui::TextureId, selected: bool) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(74.0, 50.0), Sense::click());
    let response = response.on_hover_cursor(CursorIcon::PointingHand);
    let border = if selected {
        Color32::from_rgb(116, 211, 245)
    } else if response.hovered() {
        Color32::from_rgba_unmultiplied(117, 158, 190, 190)
    } else {
        Color32::from_rgba_unmultiplied(100, 128, 151, 105)
    };

    let tint = if selected {
        Color32::WHITE
    } else if response.hovered() {
        Color32::from_rgb(210, 218, 225)
    } else {
        Color32::from_rgb(148, 158, 168)
    };
    ui.painter().image(
        image,
        // Focus art is 3:2, so this inset preserves its aspect ratio exactly.
        rect.shrink(1.0),
        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
        tint,
    );

    ui.painter().rect_stroke(
        rect,
        egui::CornerRadius::same(6),
        Stroke::new(
            if selected {
                2.0
            } else {
                1.0
            },
            border,
        ),
        StrokeKind::Inside,
    );
    response
}

/// Draws one Terraformer or Laboratory resource tile.
fn resource_tile_button(
    ui: &mut Ui,
    images: &ImageIds,
    focus: Option<ResourceName>,
    selected: bool,
) -> Response {
    let image = focus
        .map_or_else(|| images.get("no focus"), |resource| images.get(resource.to_lowername()));
    image_tile_button(ui, image, selected)
}

/// Draws the large explanation shared by compact unit-stat rows.
pub(super) fn draw_stat_hover(ui: &mut Ui, stat: &CombatStats, images: &ImageIds) {
    ui.set_width(500.);
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.add_image(images.get(stat.to_lowername()), [130., 90.]);
        });
        ui.vertical(|ui| {
            ui.label(stat.to_name());
            ui.separator();
            ui.small(stat.description());
        });
    });
}

/// Draws one full-width unit stat row.
fn draw_unit_stat(ui: &mut Ui, unit: &Unit, stat: CombatStats, images: &ImageIds) -> Response {
    ui.separator();
    ui.add_space(12.);
    let response = ui.horizontal(|ui| {
        ui.set_width(180.);
        ui.style_mut().interaction.selectable_labels = true;
        ui.add_image(images.get(stat.to_lowername()), [70., 45.]);
        ui.label(unit.get_stat(&stat)).on_hover_cursor(CursorIcon::Default);
    });
    ui.add_space(12.);
    response.response.on_hover_ui(|ui| draw_stat_hover(ui, &stat, images))
}

/// Draws the building-and-orbital Spy intelligence requirement.
pub(super) fn draw_intelligence_stat(ui: &mut Ui, unit: &Unit, images: &ImageIds) -> Response {
    draw_unit_stat(ui, unit, CombatStats::Intelligence, images)
}

/// Draws an orbital's Shipyard production requirement.
pub(super) fn draw_production_stat(ui: &mut Ui, unit: &Unit, images: &ImageIds) -> Response {
    draw_unit_stat(ui, unit, CombatStats::Production, images)
}

/// Draws the production and intelligence boxes in orbital hover-panel order.
pub(super) fn draw_orbital_stats(
    ui: &mut Ui,
    unit: &Unit,
    images: &ImageIds,
) -> (Response, Response) {
    let production = draw_production_stat(ui, unit, images);
    let intelligence = draw_intelligence_stat(ui, unit, images);
    (production, intelligence)
}

/// Draws the unit hover interface and emits any resulting local actions.
fn draw_unit_hover(
    ui: &mut Ui,
    unit: &Unit,
    count: usize,
    state: &mut UiState,
    player: &mut Player,
    planet: &mut Planet,
    solar_band: Option<SolarBand>,
    pending: &mut PendingTurnCommands,
    message: &mut MessageWriter<MessageMsg>,
    msg: Option<String>,
    images: &ImageIds,
) {
    ui.horizontal(|ui| {
        ui.set_width(700.);

        ui.vertical(|ui| {
            ui.add_image(images.get(unit.to_lowername()), [200.; 2]);
        });
        ui.vertical(|ui| {
            ui.label(unit.to_name());

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.;

                for resource in ResourceName::iter() {
                    let price = unit.price().get(&resource);
                    ui.add_image(images.get(resource.to_lowername()), [50., 35.]);
                    ui.label(price.to_string());
                    ui.add_space(30.);
                }

                let energy = EnergyGrid::for_unit(*unit, solar_band);
                if unit.is_building() || energy != EnergyGrid::default() {
                    ui.add_image(images.get("energy"), [50., 35.]);
                    ui.label(if energy.supply > 0 {
                        format!("+{}", energy.supply)
                    } else {
                        energy.demand.to_string()
                    });
                }
            });

            ui.separator();

            if let Some(msg) = msg {
                ui.colored_label(Color32::RED, RichText::new(msg).small());
            }

            ui.small(unit.description());

            ui.add_space(10.);

            ui.spacing_mut().item_spacing.y = 0.;

            if !unit.is_building() {
                ui.separator();
            }

            if !unit.is_building() {
                for (i, row) in CombatStats::iter()
                    .filter(|c| {
                        !(matches!(c, CombatStats::RapidFire | CombatStats::Intelligence)
                            || *c == CombatStats::Production && unit.is_orbital())
                    })
                    .collect::<Vec<CombatStats>>()
                    .chunks(3)
                    .enumerate()
                {
                    if i == 0 || row.iter().any(|s| unit.get_stat(s) != "---") {
                        egui::Grid::new(ui.auto_id_with(format!("row_{:?}", row[0])))
                            .spacing([20., 0.])
                            .striped(false)
                            .show(ui, |ui| {
                                for stat in row {
                                    ui.horizontal(|ui| {
                                        ui.set_width(150.);
                                        ui.style_mut().interaction.selectable_labels = true;

                                        ui.add_image(images.get(stat.to_lowername()), [70., 45.]);
                                        ui.label(unit.get_stat(stat))
                                            .on_hover_cursor(CursorIcon::Default);
                                    })
                                    .response
                                    .on_hover_ui(|ui| draw_stat_hover(ui, stat, images));
                                }
                            });
                    }

                    ui.spacing_mut().item_spacing.y = 10.;
                }
            }

            if unit.is_orbital() {
                let _ = draw_orbital_stats(ui, unit, images);
            } else if unit.is_building() {
                let _ = draw_intelligence_stat(ui, unit, images);
            }

            if *unit == Unit::Building(Building::Laboratory) && count > 0 {
                let (from, to) = &mut state.lab;

                if from == to {
                    *to = from.next(None);
                }

                ui.separator();

                ui.add_space(12.);
                ui.small("Convert resources");
                ui.add_space(9.);

                ui.horizontal(|ui| {
                    let response = resource_tile_button(ui, images, Some(*from), false)
                        .on_hover_small_ext("Click to cycle over resources.");

                    if response.clicked() {
                        *from = from.next(Some(*to));
                    } else if response.secondary_clicked() {
                        *from = from.prev(Some(*to));
                    }

                    ui.style_mut().drag_value_text_style = TextStyle::Body;
                    ui.spacing_mut().interact_size.x = 60.;
                    ui.spacing_mut().button_padding = egui::Vec2::new(6., 6.);
                    ui.add(
                        egui::DragValue::new(&mut state.lab_amount)
                            .speed(100)
                            .range(0..=player.resources.get(from)),
                    );
                    let gain = conversion_output(state.lab_amount, count);

                    let (rect, mut response) =
                        ui.allocate_exact_size([32.; 2].into(), Sense::click());

                    let image = if response.hovered() && !response.is_pointer_button_down_on() {
                        images.get("convert hover")
                    } else {
                        images.get("convert")
                    };

                    ui.add_image_painter(image, rect);

                    response = response
                        .on_hover_cursor(CursorIcon::PointingHand)
                        .on_hover_small_ext(format!(
                            "Convert {} {} into {} {}.",
                            state.lab_amount,
                            from.to_name(),
                            gain,
                            to.to_name()
                        ));

                    if response.clicked()
                        && state.lab_amount > 0
                        && state.lab_amount <= player.resources.get(from)
                        && pending.push(TurnCommand::ConvertResources {
                            planet_id: planet.id,
                            from: *from,
                            to: *to,
                            amount: state.lab_amount,
                        })
                    {
                        // Confirm the control immediately; the informational toast adds its
                        // separate notification cue when it reports the gained resource.
                        set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
                        let source = player.resources.get_mut(from);
                        *source = source.saturating_sub(state.lab_amount);
                        let destination = player.resources.get_mut(to);
                        *destination = destination.saturating_add(gain);
                        message.write(conversion_success_message(gain, *to));
                    }

                    ui.label(gain.to_string());

                    let response = resource_tile_button(ui, images, Some(*to), false)
                        .on_hover_small_ext("Click to cycle over resources.");

                    if response.clicked() {
                        *to = to.next(Some(*from));
                    } else if response.secondary_clicked() {
                        *to = to.prev(Some(*from));
                    }
                });
            } else if *unit == Unit::Building(Building::Terraformer)
                && (count > 0 || planet.buy.contains(unit))
            {
                ui.separator();
                ui.add_space(12.);
                ui.small("Resource focus");
                ui.add_space(9.);
                ui.horizontal(|ui| {
                    for focus in [
                        None,
                        Some(ResourceName::Metal),
                        Some(ResourceName::Crystal),
                        Some(ResourceName::Deuterium),
                    ] {
                        let selected = planet.terraformer_focus == focus;
                        let response = resource_tile_button(ui, images, focus, selected);
                        if !selected
                            && response.clicked()
                            && pending.push(TurnCommand::SetTerraformerFocus {
                                planet_id: planet.id,
                                resource: focus,
                            })
                        {
                            planet.terraformer_focus = focus;
                            set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
                        }
                    }
                });
            } else if *unit == Unit::Building(Building::ColonialAdministration)
                && count > 0
                && player.owns(planet)
                && player.home_planet != planet.id
            {
                ui.separator();
                ui.add_space(12.);
                draw_fleet_withdrawal(ui, planet, pending, images);
            } else if *unit == Unit::Building(Building::PlanetaryShield) && count > 0 {
                ui.separator();
                ui.add_space(12.);
                draw_planetary_shield_overload(ui, planet, pending, count);
            } else if *unit == Unit::Building(Building::CommandRelay)
                && (count > 0 || planet.buy.contains(unit))
            {
                ui.separator();
                ui.add_space(12.);
                draw_command_relay_toggle(ui, planet, pending);
            }

            if !unit.rapid_fire().is_empty() {
                ui.separator();
                ui.small(CombatStats::RapidFire.to_name())
                    .on_hover_ui(|ui| draw_stat_hover(ui, &CombatStats::RapidFire, images));

                egui::Grid::new("rapid_fire").spacing([10., 10.]).striped(false).show(ui, |ui| {
                    let mut counter = 0;
                    for rf_unit in Unit::all().iter().flatten() {
                        if let Some(rf) = unit.rapid_fire().get(rf_unit) {
                            ui.horizontal(|ui| {
                                ui.set_width(115.);
                                ui.spacing_mut().item_spacing.x = 8.;

                                ui.add_image(images.get(rf_unit.to_lowername()), [45., 45.]);
                                ui.small(format!("{}%", rf));
                            })
                            .response
                            .on_hover_text(RichText::new(rf_unit.to_name()).small());

                            counter += 1;
                            if counter % 4 == 0 {
                                ui.end_row();
                            }
                        }
                    }
                });
            }
        });
    });
}

/// Draws one toggle whose compact label and switch share the same action.
fn labeled_toggle(ui: &mut Ui, label: &str, active: &mut bool) -> Response {
    ui.horizontal(|ui| {
        let label = ui
            .add(egui::Label::new(RichText::new(label).small()).sense(Sense::click()))
            .on_hover_cursor(CursorIcon::PointingHand);
        if label.clicked() {
            *active = !*active;
        }
        ui.add(toggle(active));
    })
    .response
}

/// Draws the activation toggle exposed by a completed Command Relay's hover panel.
pub(super) fn draw_command_relay_toggle(
    ui: &mut Ui,
    planet: &mut Planet,
    pending: &mut PendingTurnCommands,
) {
    let mut active = planet.command_relay_active;
    labeled_toggle(
        ui,
        if active {
            "Relay active:"
        } else {
            "Relay inactive:"
        },
        &mut active,
    )
    .on_hover_small(
        "When active, the Relay diverts undersized enemy Spy missions before combat, returning \
        their Probes safely with a false report of an empty planet. When inactive, Spy missions \
        gather intelligence normally.",
    );
    if active != planet.command_relay_active
        && pending.push(TurnCommand::SetCommandRelay {
            planet_id: planet.id,
            active,
        })
    {
        planet.command_relay_active = active;
        set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
    }
}

/// Draws the overload toggle exposed by a completed Planetary Shield's hover panel.
pub(super) fn draw_planetary_shield_overload(
    ui: &mut Ui,
    planet: &mut Planet,
    pending: &mut PendingTurnCommands,
    level: usize,
) {
    let cooling_down = planet.shield_overload == ShieldOverloadState::Cooldown;
    ui.add_enabled_ui(pending.can_accept_commands() && !cooling_down, |ui| {
        let mut active = planet.shield_overload.is_overloaded();
        labeled_toggle(
            ui,
            match planet.shield_overload {
                ShieldOverloadState::Ready => "Overload shield:",
                ShieldOverloadState::Overloaded => "Shield overloaded:",
                ShieldOverloadState::Cooldown => "Shield cooling down:",
            },
            &mut active,
        )
        .on_hover_small(format!(
            "Overload for the next turn: +{}% shield strength and +{} Energy demand. After \
                use, the shield must cool down for one turn.",
            level.saturating_mul(crate::core::constants::PS_OVERLOAD_BONUS_PERCENT_PER_LEVEL),
            crate::core::constants::PS_OVERLOAD_ENERGY_COST,
        ));
        if active != planet.shield_overload.is_overloaded()
            && pending.push(TurnCommand::SetPlanetaryShieldOverload {
                planet_id: planet.id,
                active,
            })
        {
            planet.shield_overload = if active {
                ShieldOverloadState::Overloaded
            } else {
                ShieldOverloadState::Ready
            };
            set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
        }
    });
}

/// Draws one withdrawal stance with the same tile treatment as Terraformer focus.
fn fleet_withdrawal_button(
    ui: &mut Ui,
    images: &ImageIds,
    withdrawal: FleetWithdrawal,
    selected: bool,
) -> Response {
    let image = match withdrawal {
        FleetWithdrawal::Off => "no focus",
        FleetWithdrawal::Losses75 => "withdrawal 75",
        FleetWithdrawal::Losses50 => "withdrawal 50",
        FleetWithdrawal::Losses25 => "withdrawal 25",
        FleetWithdrawal::Immediate => "withdrawal immediate",
    };
    let response = image_tile_button(ui, images.get(image), selected);
    response.widget_info(|| {
        egui::WidgetInfo::selected(
            egui::WidgetType::Button,
            ui.is_enabled(),
            selected,
            withdrawal.label(),
        )
    });
    response.on_hover_small(match withdrawal {
        FleetWithdrawal::Off => "Turn off withdrawal.",
        FleetWithdrawal::Losses75 => "Withdraw after losing 75% of fleet strength.",
        FleetWithdrawal::Losses50 => "Withdraw after losing 50% of fleet strength.",
        FleetWithdrawal::Losses25 => "Withdraw after losing 25% of fleet strength.",
        FleetWithdrawal::Immediate => "Immediate withdrawal.",
    })
}

/// Draws the withdrawal orders unlocked by a completed Colonial Administration.
pub(super) fn draw_fleet_withdrawal(
    ui: &mut Ui,
    planet: &mut Planet,
    pending: &mut PendingTurnCommands,
    images: &ImageIds,
) {
    let administration = Unit::Building(Building::ColonialAdministration);
    let level = planet.army.amount(&administration);
    if level == 0 {
        return;
    }

    ui.add_enabled_ui(pending.can_accept_commands(), |ui| {
        ui.spacing_mut().item_spacing = egui::vec2(8.0, 4.0);
        ui.small("Fleet withdrawal");
        ui.add_space(6.0);
        ui.horizontal_wrapped(|ui| {
            for withdrawal in FleetWithdrawal::ALL {
                if level < withdrawal.minimum_level() {
                    continue;
                }

                let selected = planet.fleet_withdrawal == withdrawal;
                let response = fleet_withdrawal_button(ui, images, withdrawal, selected);
                if response.clicked()
                    && !selected
                    && pending.push(TurnCommand::SetFleetWithdrawal {
                        planet_id: planet.id,
                        withdrawal,
                    })
                {
                    planet.fleet_withdrawal = withdrawal;
                    set_ui_sound(ui.ctx(), Some(SoundEffect::Button));
                }
            }
        });
    });
}

/// Draws the shop interface and emits any resulting local actions.
pub(super) fn draw_shop(
    ui: &mut Ui,
    state: &mut UiState,
    settings: &Settings,
    player: &mut Player,
    planet: &mut Planet,
    solar_band: Option<SolarBand>,
    mut next_turn_energy: EnergyGrid,
    senate_level_limit: usize,
    pending: &mut PendingTurnCommands,
    message: &mut MessageWriter<MessageMsg>,
    images: &ImageIds,
) {
    ui.spacing_mut().item_spacing = emath::Vec2::new(4., 4.);

    ui.add_space(4.);

    if planet.is_moon() && matches!(state.shop, Shop::Orbitals | Shop::Defenses) {
        state.shop = Shop::default();
    }

    ui.horizontal(|ui| {
        ui.add_space(20.);
        if ui
            .add_sized([26., 24.], egui::Button::new(RichText::new("‹").size(19.)).frame(false))
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text("Previous shop category")
            .clicked()
        {
            state.shop = state.shop.previous(planet.is_moon());
        }

        ui.add_image(images.get(state.shop.to_lowername()), [20., 20.]);
        ui.small(state.shop.to_name());

        if ui
            .add_sized([26., 24.], egui::Button::new(RichText::new("›").size(19.)).frame(false))
            .on_hover_cursor(CursorIcon::PointingHand)
            .on_hover_text("Next shop category")
            .clicked()
        {
            state.shop = state.shop.next(planet.is_moon());
        }

        if let Some((label, current, max)) = shop_capacity_summary(state.shop, planet) {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.add_space(45.);
                ui.small(format!("{label}: {current}/{max}"));
            });
        }
    });

    let units = match state.shop {
        Shop::Buildings => {
            Unit::buildings_for_world(planet.is_moon(), planet.id == player.home_planet)
        },
        Shop::Orbitals => Unit::orbitals(),
        Shop::Fleet => Unit::ships(),
        Shop::Defenses => {
            Unit::defenses().into_iter().filter(|unit| *unit != Unit::space_dock()).collect()
        },
    };

    ui.add_space(10.);

    for row in units.chunks(5) {
        ui.horizontal(|ui| {
            ui.add_space(25.);

            for unit in row {
                let count = planet.army.amount(unit);
                let bought = planet.buy.iter().filter(|u| *u == unit).count();

                let purchase = purchase_limit(player, planet, *unit, senate_level_limit);
                let limit = purchase.as_ref().copied().unwrap_or(0);
                ui.add_enabled_ui(limit > 0 && pending.can_accept_commands(), |ui| {
                    ui.spacing_mut().button_padding = egui::Vec2::splat(2.);

                    let mut response =
                        ui.add_image_button(images.get(unit.to_lowername()), [130., 130.]);

                    if ui.is_enabled() {
                        response = response.on_hover_cursor(CursorIcon::PointingHand);
                    }

                    if response.clicked() || response.secondary_clicked() {
                        set_ui_sound(ui.ctx(), None);
                    }

                    if response.clicked()
                        && pending.push(TurnCommand::BuyUnits {
                            planet_id: planet.id,
                            unit: *unit,
                            count: 1,
                        })
                    {
                        player.resources -= unit.price();
                        planet.buy.push(*unit);
                        if let Some(warning) =
                            energy_shortage_warning(next_turn_energy, *unit, solar_band)
                        {
                            message.write(warning);
                        }
                        next_turn_energy = next_turn_energy.with_unit(*unit, solar_band, 1);
                        set_ui_sound(ui.ctx(), Some(SoundEffect::purchase(*unit)));
                    }

                    if !unit.is_building()
                        && *unit != Unit::space_dock()
                        && response.secondary_clicked()
                    {
                        // Buy 5 new units (or maximum possible)
                        let n = limit.min(5);

                        if n > 0
                            && pending.push(TurnCommand::BuyUnits {
                                planet_id: planet.id,
                                unit: *unit,
                                count: n,
                            })
                        {
                            player.resources -= unit.price() * n;
                            planet.buy.extend(vec![*unit; n]);
                            set_ui_sound(ui.ctx(), Some(SoundEffect::purchase(*unit)));
                        }
                    }

                    if count > 0 {
                        let text = match unit {
                            Unit::Building(Building::MissileSilo) => Some(format!(
                                "{}/{}",
                                planet.missile_capacity(),
                                planet.max_missile_capacity()
                            )),
                            Unit::Building(Building::JumpGate) => {
                                Some(format!("{}/{}", planet.jump_gate, planet.max_jump_capacity()))
                            },
                            Unit::Building(Building::Laboratory) => {
                                Some(format!("1:{}", 1. + 0.5 * (5 - count) as f32))
                            },
                            _ => None,
                        };

                        if let Some(text) = text {
                            ui.add_text_on_image(
                                text,
                                Color32::WHITE,
                                TextStyle::Body,
                                response.rect.right_top() - egui::Vec2::new(3., -3.),
                                Align2::RIGHT_TOP,
                            );
                        }
                    }

                    let rect = ui.add_text_on_image(
                        count.to_string(),
                        Color32::WHITE,
                        TextStyle::Heading,
                        response.rect.left_bottom(),
                        Align2::LEFT_BOTTOM,
                    );

                    if bought > 0 {
                        ui.add_text_on_image(
                            format!(" (+{})", bought),
                            Color32::WHITE,
                            TextStyle::Body,
                            rect.right_bottom() - egui::Vec2::new(6., 7.),
                            Align2::LEFT_BOTTOM,
                        );
                    }

                    if settings.show_hover {
                        response
                            .on_hover_ui(|ui| {
                                draw_unit_hover(
                                    ui, unit, count, state, player, planet, solar_band, pending,
                                    message, None, images,
                                );
                            })
                            .on_disabled_hover_ui(|ui| {
                                draw_unit_hover(
                                    ui,
                                    unit,
                                    count,
                                    state,
                                    player,
                                    planet,
                                    solar_band,
                                    pending,
                                    message,
                                    purchase.as_ref().err().map(ToString::to_string),
                                    images,
                                );
                            });
                    }
                });
            }
        });
    }
}
