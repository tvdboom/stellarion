//! Shop panels for the game interface.

use super::*;

pub(super) fn conversion_success_message(gain: usize, resource: ResourceName) -> MessageMsg {
    MessageMsg::info(format!("Gained {} {}.", format_thousands(gain), resource.to_name()))
}

pub(super) fn energy_shortage_warning(
    energy: EnergyGrid,
    unit: Unit,
    solar_band: Option<SolarBand>,
) -> Option<MessageMsg> {
    let after = energy.with_unit(unit, solar_band, 1);
    (after.balance() < energy.balance() && energy.balance() >= 0 && after.balance() < 0).then(
        || {
            MessageMsg::warning(format!(
                "This purchase leaves next turn's Energy at {}. Resource production and Planetary \
            Shields will operate at {}% efficiency unless you generate more Energy.",
                after.balance(),
                after.efficiency_percent()
            ))
        },
    )
}

pub(super) fn shop_unit_description(unit: Unit, senate_level_limit: usize) -> String {
    if unit == Unit::Building(Building::Senate) {
        let suffix = if senate_level_limit == 1 {
            ""
        } else {
            "s"
        };
        return format!(
            "{} Each level adds +1 planet slot. This match permits {} Senate level{}, for up to +{} planet slot{}.",
            unit.description(),
            senate_level_limit,
            suffix,
            senate_level_limit,
            suffix,
        );
    }
    unit.description().to_string()
}

/// Draws the unit hover interface and emits any resulting local actions.
fn draw_unit_hover(
    ui: &mut Ui,
    unit: &Unit,
    count: usize,
    state: &mut UiState,
    player: &mut Player,
    planet_id: PlanetId,
    solar_band: Option<SolarBand>,
    senate_level_limit: usize,
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

            ui.small(shop_unit_description(*unit, senate_level_limit));

            ui.add_space(10.);

            ui.spacing_mut().item_spacing.y = 0.;

            if !unit.is_building() {
                ui.separator();
            }

            let stat_hover = |ui: &mut Ui, stat: &CombatStats| {
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
            };

            if !unit.is_building() {
                for (i, row) in CombatStats::iter()
                    .filter(|c| *c != CombatStats::RapidFire)
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
                                    .on_hover_ui(|ui| stat_hover(ui, stat));
                                }
                            });
                    }

                    ui.spacing_mut().item_spacing.y = 10.;
                }
            } else if *unit == Unit::Building(Building::Laboratory) && count > 0 {
                let (from, to) = &mut state.lab;

                if from == to {
                    *to = from.next(None);
                }

                ui.separator();

                ui.add_space(20.);

                ui.horizontal(|ui| {
                    let response = ui
                        .add_image(images.get(from.to_lowername()), [65., 43.])
                        .interact(Sense::click())
                        .on_hover_small_ext("Click to cycle over resources.");

                    if response.clicked() {
                        *from = from.next(None);
                    } else if response.secondary_clicked() {
                        *from = from.prev(None);
                    }

                    let gain = (state.lab_amount as f32 / (1. + 0.5 * (5 - count) as f32)) as usize;

                    ui.style_mut().drag_value_text_style = TextStyle::Body;
                    ui.spacing_mut().interact_size.x = 60.;
                    ui.spacing_mut().button_padding = egui::Vec2::new(6., 6.);
                    ui.add(
                        egui::DragValue::new(&mut state.lab_amount)
                            .speed(100)
                            .range(0..=player.resources.get(from)),
                    );

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
                        && pending.push(TurnCommand::ConvertResources {
                            planet_id,
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

                    let response = ui
                        .add_image(images.get(to.to_lowername()), [65., 43.])
                        .interact(Sense::click())
                        .on_hover_small_ext("Click to cycle over resources.");

                    if response.clicked() {
                        *to = to.next(Some(*from));
                    } else if response.secondary_clicked() {
                        *to = to.prev(Some(*from));
                    }
                });
            }

            if !unit.rapid_fire().is_empty() {
                ui.separator();
                ui.small(CombatStats::RapidFire.to_name())
                    .on_hover_ui(|ui| stat_hover(ui, &CombatStats::RapidFire));

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

        let (current, max) = match state.shop {
            Shop::Buildings => (planet.fields_consumed(), planet.max_fields()),
            Shop::Orbitals => (0, 0),
            Shop::Fleet => (planet.fleet_production(), planet.max_fleet_production()),
            Shop::Defenses => (planet.battery_production(), planet.max_battery_production()),
        };

        if matches!(state.shop, Shop::Fleet | Shop::Defenses)
            || (state.shop == Shop::Buildings && planet.is_moon())
        {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.add_space(45.);
                ui.small(format!(
                    "{}: {}/{}",
                    if planet.is_moon() {
                        "Fields"
                    } else {
                        "Production"
                    },
                    current,
                    max
                ));
            });
        }
    });

    let units = match state.shop {
        Shop::Buildings => Unit::buildings()
            .into_iter()
            .filter(|unit| unit.valid_on(planet.is_moon()))
            .collect::<Vec<_>>(),
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
                                    ui,
                                    unit,
                                    count,
                                    state,
                                    player,
                                    planet.id,
                                    solar_band,
                                    senate_level_limit,
                                    pending,
                                    message,
                                    None,
                                    images,
                                );
                            })
                            .on_disabled_hover_ui(|ui| {
                                draw_unit_hover(
                                    ui,
                                    unit,
                                    count,
                                    state,
                                    player,
                                    planet.id,
                                    solar_band,
                                    senate_level_limit,
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
