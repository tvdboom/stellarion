//! Missions panels for the game interface.

use super::*;
use crate::core::missions::MissionRouteStyle;

/// Draws the same compact route language used by hovered missions on the strategic map.
fn draw_route_preview(ui: &mut Ui, mission: &Mission, player: &Player, color: Color32) -> Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(58.0, 28.0), Sense::hover());
    let painter = ui.painter().with_clip_rect(rect);
    let left = rect.left() + 4.0;
    let right = rect.right() - 4.0;
    let width = (right - left).max(1.0);
    let center_y = rect.center().y;
    let time = ui.input(|input| input.time) as f32;
    let speed = 22.0 * mission.route_speed_factor() as f32;
    let phase = time * speed;

    painter.line_segment(
        [egui::pos2(left, center_y), egui::pos2(right, center_y)],
        Stroke::new(1.0, Color32::from_rgba_unmultiplied(143, 158, 174, 52)),
    );

    match mission.route_style(player) {
        MissionRouteStyle::Standard => {
            let spacing = 15.0;
            for index in 0..5 {
                let x = left + (index as f32 * spacing + phase).rem_euclid(width + spacing) - 5.0;
                let alpha = if x < left || x > right {
                    0
                } else {
                    220
                };
                let marker =
                    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), alpha);
                painter.line_segment(
                    [egui::pos2(x - 3.0, center_y - 4.0), egui::pos2(x + 2.0, center_y)],
                    Stroke::new(1.7, marker),
                );
                painter.line_segment(
                    [egui::pos2(x + 2.0, center_y), egui::pos2(x - 3.0, center_y + 4.0)],
                    Stroke::new(1.7, marker),
                );
            }
        },
        MissionRouteStyle::JumpGate => {
            let spacing = 18.0;
            for index in 0..4 {
                let progress = (index as f32 * spacing + phase).rem_euclid(width + spacing);
                let x = left + progress - 5.0;
                if !(left..=right).contains(&x) {
                    continue;
                }
                let pulse = ((time * 3.2 + index as f32 * 1.1).sin() * 0.5 + 0.5).max(0.0);
                painter.circle_stroke(
                    egui::pos2(x, center_y),
                    2.5 + 2.2 * pulse,
                    Stroke::new(
                        1.4,
                        Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 210),
                    ),
                );
            }
        },
        MissionRouteStyle::MissileStrike => {
            let spacing = 22.0;
            for index in 0..4 {
                let x = left + (index as f32 * spacing + phase).rem_euclid(width + spacing) - 7.0;
                if !(left..=right).contains(&x) {
                    continue;
                }
                let marker = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 235);
                painter.line_segment(
                    [egui::pos2(x - 10.0, center_y), egui::pos2(x - 2.5, center_y)],
                    Stroke::new(2.0, marker.gamma_multiply(0.55)),
                );
                painter.circle_filled(egui::pos2(x, center_y), 2.7, marker);
            }
        },
    }

    response
}

/// Draws a faction-tinted mission sprite without separate hover-color assets.
fn draw_mission_image(ui: &mut Ui, image: egui::TextureId, size: f32, color: Color32) -> Response {
    ui.add(
        egui::Image::new(SizedTexture::new(image, egui::vec2(size, size)))
            .fit_to_exact_size(egui::vec2(size, size))
            .tint(color),
    )
}

/// Draws the new mission interface and emits any resulting local actions.
fn draw_new_mission(
    ui: &mut Ui,
    send_mission: &mut MessageWriter<SendMissionMsg>,
    settings: &Settings,
    state: &mut UiState,
    map: &mut Map,
    player: &mut Player,
    is_hovered: bool,
    keyboard: &ButtonInput<KeyCode>,
    images: &ImageIds,
) {
    let origin = map.get(state.mission_info.origin);
    let destination = map.get(state.mission_info.destination);

    let (n_owned, n_max_owned) = player.planets_owned(map, settings);

    // Block selection of any unit when in spectator mode to be unable to send missions
    if player.spectator {
        state.mission_info.army = Army::new();
    }

    // Recalculate position (in case origin changed)
    state.mission_info =
        Mission::from_mission(settings.turn, player.id, origin, destination, &state.mission_info);

    if state.mission_info.objective == Icon::Colonize && n_owned >= n_max_owned {
        state.mission_info.objective = Icon::Deploy;
    }

    if origin.controlled == destination.controlled {
        // Check for ownership since you can colonize a controlled planet
        if destination.owned == Some(player.id) || state.mission_info.objective != Icon::Colonize {
            state.mission_info.objective = Icon::Deploy;
        }
    } else if state.mission_info.objective == Icon::Deploy {
        state.mission_info.objective = Icon::default();
    }

    if !state.mission_info.objective.condition(origin)
        || (destination.is_moon() && state.mission_info.objective.on_planet_only())
    {
        state.mission_info.objective = Icon::iter()
            .find(|i| {
                i.is_mission()
                    && i.condition(origin)
                    && (!destination.is_moon() || !i.on_planet_only())
            })
            .unwrap_or_default();
    }

    let army = match state.mission_info.objective {
        Icon::MissileStrike => vec![Unit::interplanetary_missile()],
        Icon::Spy => vec![Unit::probe()],
        _ => Unit::ships(),
    };

    let speed = state.mission_info.speed();
    let distance = state.mission_info.distance(map);
    let duration = state.mission_info.duration(map);
    let fuel = state.mission_info.fuel_consumption(map);

    ui.add_space(10.);

    ui.horizontal_top(|ui| {
        ui.add_space(135.);

        let action = |r: Response, planet: &Planet, h: &mut bool, state: &mut UiState| {
            if r.clicked() {
                state.planet_selected = Some(planet.id);
                state.to_selected = true;
                state.mission = false;
                if player.owns(planet) {
                    state.mission_info.origin = planet.id;
                }
            } else if r.secondary_clicked() && !planet.is_destroyed {
                state.mission_tab = MissionTab::NewMission;
                state.mission_info.destination = planet.id;
            } else if r.hovered() {
                state.planet_hover = Some(planet.id);
                *h = true;
            }
        };

        let mut changed_hover = false;
        egui::Grid::new("mission_origin_destination").spacing([30., 0.]).striped(false).show(
            ui,
            |ui| {
                let response = ui.cell(70., |ui| {
                    ui.add_image(images.get(origin.image()), [60.; 2])
                        .interact(Sense::click())
                        .on_hover_cursor(CursorIcon::PointingHand)
                });

                action(response, origin, &mut changed_hover, state);

                ui.cell(100., |ui| {
                    ui.vertical(|ui| {
                        ui.add_space(15.);

                        let controlled = map
                            .planets
                            .iter()
                            .filter(|p| player.controls(p))
                            .sorted_by(|a, b| a.name.cmp(&b.name))
                            .collect::<Vec<_>>();

                        ComboBox::from_id_salt("origin")
                            .height(60. * controlled.len().max(5) as f32)
                            .selected_text(&map.get(state.mission_info.origin).name)
                            .show_ui(ui, |ui| {
                                for planet in controlled {
                                    ui.selectable_value(
                                        &mut state.mission_info.origin,
                                        planet.id,
                                        &planet.name,
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand);
                                }
                            })
                            .response
                            .on_hover_cursor(CursorIcon::PointingHand);
                    });
                });

                let (rect, mut response) =
                    ui.cell(50., |ui| ui.allocate_exact_size([50.; 2].into(), Sense::click()));

                response = response.on_hover_cursor(CursorIcon::PointingHand).on_hover_small(
                    "Click to select all units on the origin planet. Right-click to unselect all.",
                );

                let image_rect = if response.hovered() && !response.is_pointer_button_down_on() {
                    rect.expand(3.0)
                } else {
                    rect
                };
                ui.painter().image(
                    images.get(state.mission_info.image(player)),
                    image_rect,
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    player.color().color().to_color32(),
                );

                if response.clicked() {
                    state.mission_info.army =
                        army.iter().map(|u| (*u, origin.army.amount(u))).collect();
                } else if response.secondary_clicked() {
                    state.mission_info.army.clear();
                }

                ui.cell(100., |ui| {
                    ui.vertical(|ui| {
                        ui.add_space(15.);
                        ComboBox::from_id_salt("destination")
                            .selected_text(&map.get(state.mission_info.destination).name)
                            .show_ui(ui, |ui| {
                                for planet in map
                                    .planets
                                    .iter()
                                    .filter(|p| !p.is_destroyed)
                                    .sorted_by(|a, b| a.name.cmp(&b.name))
                                {
                                    ui.selectable_value(
                                        &mut state.mission_info.destination,
                                        planet.id,
                                        &planet.name,
                                    )
                                    .on_hover_cursor(CursorIcon::PointingHand);
                                }
                            })
                            .response
                            .on_hover_cursor(CursorIcon::PointingHand);
                    });
                });

                let response = ui.cell(70., |ui| {
                    ui.add_image(images.get(destination.image()), [60.; 2])
                        .interact(Sense::click())
                        .on_hover_cursor(CursorIcon::PointingHand)
                });

                action(response, destination, &mut changed_hover, state);
            },
        );

        // If not hovering anything, reset hover selection
        if is_hovered && !changed_hover {
            state.planet_hover = None;
        }
    });

    ui.add_space(-10.);
    ui.add(Separator::default().shrink(70.));

    if state.mission_info.origin == state.mission_info.destination {
        ui.add_space(30.);
        ui.vertical_centered(|ui| {
            ui.colored_label(Color32::RED, "The origin and destination planets must be different.");
        });
    } else {
        ui.horizontal(|ui| {
            ui.add_space(130.);

            ui.vertical(|ui| {
                ui.set_width(280.);

                egui::Grid::new("units").striped(false).num_columns(2).spacing([25., 8.]).show(
                    ui,
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 8.;

                        for (i, unit) in army.iter().enumerate() {
                            let n = origin.army.amount(unit);

                            ui.add_enabled_ui(n > 0, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.set_width(110.);

                                        let response = ui
                                            .add_image(images.get(unit.to_lowername()), [65., 65.])
                                            .interact(Sense::click())
                                            .on_hover_cursor(CursorIcon::PointingHand)
                                            .on_hover_small(unit.to_name())
                                            .on_disabled_hover_small(unit.to_name());

                                        if response.clicked() {
                                            *state.mission_info.army.entry(*unit).or_insert(0) = n;
                                        }

                                        if response.secondary_clicked() {
                                            *state.mission_info.army.entry(*unit).or_insert(0) = 0;
                                        }

                                        ui.add_text_on_image(
                                            n.to_string(),
                                            Color32::WHITE,
                                            TextStyle::Body,
                                            response.rect.left_bottom(),
                                            Align2::LEFT_BOTTOM,
                                        );

                                        ui.style_mut().drag_value_text_style = TextStyle::Body;
                                        ui.spacing_mut().interact_size.x = 50.;
                                        let value =
                                            state.mission_info.army.entry(*unit).or_insert(0);
                                        ui.add(egui::DragValue::new(value).speed(0.2).range(0..=n));
                                    });
                                });
                            });

                            if i % 2 == 1 {
                                ui.end_row();
                            }
                        }
                    },
                );
            });

            ui.add_space(15.);

            ui.vertical(|ui| {
                ui.add_space(20.);

                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.;
                    ui.spacing_mut().button_padding = egui::Vec2::splat(2.);

                    let on_hover = |ui: &mut Ui, icon: &Icon, msg: bool| {
                        ui.horizontal(|ui| {
                            ui.vertical(|ui| {
                                ui.add_image(
                                    images.get(format!("{} cover", icon.to_lowername())),
                                    [150., 150.],
                                );
                            });
                            ui.vertical(|ui| {
                                ui.label(icon.to_name());
                                ui.separator();

                                if msg {
                                    ui.colored_label(
                                        Color32::RED,
                                        RichText::new(icon.requirement()).small(),
                                    );
                                }

                                ui.small(icon.description());
                            });
                        });
                    };

                    for icon in
                        Icon::objectives(player.owns(destination), player.controls(destination))
                    {
                        ui.add_enabled_ui(
                            icon.condition(origin)
                                && !(destination.is_moon() && icon.on_planet_only())
                                && !(icon == Icon::Colonize && n_owned >= n_max_owned),
                            |ui| {
                                let button = ui
                                    .add(
                                        egui::Button::image(SizedTexture::new(
                                            images.get(icon.to_lowername()),
                                            [40.; 2],
                                        ))
                                        .corner_radius(5.),
                                    )
                                    .on_hover_ui(|ui| on_hover(ui, &icon, false))
                                    .on_disabled_hover_ui(|ui| on_hover(ui, &icon, true))
                                    .on_hover_cursor(CursorIcon::PointingHand);

                                if button.clicked() {
                                    match icon {
                                        Icon::Spy => state
                                            .mission_info
                                            .army
                                            .retain(|u, _| matches!(u, Unit::Ship(Ship::Probe))),
                                        Icon::MissileStrike => {
                                            state.mission_info.army.retain(|u, _| {
                                                matches!(
                                                    u,
                                                    Unit::Defense(Defense::InterplanetaryMissile)
                                                )
                                            })
                                        },
                                        _ => {
                                            state.mission_info.army.remove(&Unit::Defense(
                                                Defense::InterplanetaryMissile,
                                            ));
                                        },
                                    }

                                    state.mission_info.objective = icon;
                                }
                            },
                        );
                    }
                });

                ui.add_space(5.);

                ui.horizontal(|ui| {
                    ui.small("🎯 Objective:");

                    ui.spacing_mut().item_spacing.x = 4.;
                    ui.add_image(images.get(state.mission_info.objective.to_lowername()), [20.; 2]);
                    ui.small(state.mission_info.objective.to_name());
                });

                ui.small(format!("📏 Distance: {distance:.1} AU"));
                ui.small(format!(
                    "🚀 Speed: {}",
                    if speed == 0. || speed == f32::MAX {
                        "---".to_string()
                    } else {
                        format!("{speed} AU/turn")
                    }
                ));
                ui.small(format!(
                    "⏱ Duration: {}",
                    if duration == 0 {
                        "---".to_string()
                    } else {
                        format!(
                            "+{} turn{} ({})",
                            duration,
                            if duration == 1 {
                                ""
                            } else {
                                "s"
                            },
                            settings.turn + duration,
                        )
                    }
                ));
                ui.small(format!("⛽ Fuel consumption: {fuel}"))
                    .on_hover_small("Amount of deuterium it costs to send this mission.");

                if matches!(
                    state.mission_info.objective,
                    Icon::Colonize | Icon::Attack | Icon::Destroy
                ) {
                    let probes = state.mission_info.army.amount(&Unit::probe());
                    ui.add_enabled_ui(probes > 0, |ui| {
                        ui.horizontal(|ui| {
                            ui.small("⚔ Combat Probes:");
                            ui.add(toggle(&mut state.mission_info.combat_probes));
                        });
                    })
                    .response
                    .on_hover_ui(|ui| {
                        ui.set_width(300.);
                        ui.small(
                            "Normally, Probes leave combat after the first round and return \
                            to the planet of origin. Enabling this option makes the Probes stay \
                            during the whole combat, serving as extra fodder and having the \
                            advantage that they stay with the rest of the fleet when victorious, \
                            at risk of getting no enemy unit information when losing combat. \
                            Probes always stay if the combat takes only one round.",
                        );
                    })
                    .on_disabled_hover_small("No Probes selected for this mission.");

                    if probes == 0 {
                        state.mission_info.combat_probes = false;
                    }

                    let bombers = state.mission_info.army.amount(&Unit::Ship(Ship::Bomber));
                    ui.add_enabled_ui(bombers > 0 && !destination.is_moon(), |ui| {
                        ui.horizontal(|ui| {
                            ui.small("💣 Bombing raid:");

                            ui.style_mut().spacing.button_padding.y = 1.5;
                            if let Some(style) =
                                ui.style_mut().text_styles.get_mut(&TextStyle::Button)
                            {
                                style.size = 18.;
                            }

                            ComboBox::from_id_salt("bombing")
                                .width(125.)
                                .selected_text(state.mission_info.bombing.to_name())
                                .show_ui(ui, |ui| {
                                    for item in BombingRaid::iter() {
                                        ui.style_mut().spacing.button_padding.y = 1.5;
                                        ui.style_mut().spacing.item_spacing.y = 5.;

                                        ui.selectable_value(
                                            &mut state.mission_info.bombing,
                                            item.clone(),
                                            RichText::new(item.to_name()).small(),
                                        )
                                        .on_hover_cursor(CursorIcon::PointingHand)
                                        .on_hover_small(item.description());
                                    }
                                })
                                .response
                                .on_hover_cursor(CursorIcon::PointingHand);
                        });
                    })
                    .response
                    .on_hover_small(
                        "Command Bombers to bomb enemy buildings. Every round of combat, \
                        every bomber has a 10% chance to decrease a target building's level by \
                        one. The Planetary Shield must first be destroyed before bombing can \
                        take place.",
                    )
                    .on_disabled_hover_small(if destination.is_moon() {
                        "Moons cannot be bombed."
                    } else {
                        "No Bombers selected for this mission."
                    });

                    if bombers == 0 || destination.is_moon() {
                        state.mission_info.bombing = BombingRaid::None;
                    }
                }

                if state.mission_info.objective == Icon::Deploy {
                    if player.owns(origin)
                        && player.owns(destination)
                        && origin.army.amount(&Unit::Building(Building::JumpGate)) > 0
                        && destination.army.amount(&Unit::Building(Building::JumpGate)) > 0
                    {
                        let jump_cost = state.mission_info.jump_cost();
                        let can_jump = origin.jump_gate + jump_cost <= origin.max_jump_capacity();

                        if !can_jump {
                            state.mission_info.jump_gate = false;
                        } else if state.mission_info.jump_gate != state.jump_gate_history {
                            state.mission_info.jump_gate = state.jump_gate_history;
                        }

                        ui.horizontal(|ui| {
                            ui.small(format!(
                                "🌀 Jump Gate ({}/{}):",
                                jump_cost,
                                origin.max_jump_capacity() - origin.jump_gate
                            ));
                            if ui.add(toggle(&mut state.mission_info.jump_gate)).clicked() {
                                state.jump_gate_history = !state.jump_gate_history;
                            }
                        })
                        .response
                        .on_hover_small(
                            "Whether to send this mission through the Jump Gate. Missions \
                                through the Jump Gate always take 1 turn and cost no fuel. The \
                                armies total jump cost can't surpass the Gate's limit.",
                        );
                    }
                } else {
                    state.mission_info.jump_gate = false;
                }
            });
        });

        ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
            ui.add_space(60.);

            let army_check = state.mission_info.army.has_army();
            let fuel_check = player.resources.get(&ResourceName::Deuterium) >= fuel;
            let objective_check =
                validate_mission(player, origin, destination, &state.mission_info).is_ok();

            ui.horizontal(|ui| {
                ui.add_space(40.);

                ui.add_enabled_ui(army_check && fuel_check && objective_check, |ui| {
                    let response =
                        ui.add_custom_button("Send mission", images).on_disabled_hover_ui(|ui| {
                            if !army_check {
                                ui.small("No ships selected for the mission.");
                            } else if !fuel_check {
                                ui.small("Not enough fuel (deuterium) for the mission.");
                            } else {
                                ui.small(
                                    "The ship requirements for the mission objective is not met.",
                                );
                            }
                        });

                    if response.clicked()
                        || (response.enabled() && keyboard.just_pressed(KeyCode::Enter))
                    {
                        let mission = Mission::from_mission(
                            settings.turn,
                            player.id,
                            origin,
                            destination,
                            &state.mission_info,
                        );

                        send_mission.write(SendMissionMsg::new(mission));
                        // The accepted command emits its launch cue in send_mission.
                        set_ui_sound(ui.ctx(), None);
                        state.planet_selected = None;
                        state.mission = false;
                        state.mission_info = Mission::default();
                    }
                });
            });
        });
    }
}

/// Draws the active missions interface and emits any resulting local actions.
fn draw_active_missions(
    ui: &mut Ui,
    missions: Vec<&Mission>,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    is_hovered: bool,
    images: &ImageIds,
) {
    if missions.is_empty() {
        ui.add_space(40.);
        ui.vertical_centered(|ui| {
            ui.label(format!("No {}.", state.mission_tab.to_lowername()));
        });
        return;
    }

    // Sort by turns remaining ascending
    let missions = missions
        .iter()
        .sorted_by(|a, b| a.turns_to_destination(map).cmp(&b.turns_to_destination(map)));

    ui.add_space(30.);

    ScrollArea::vertical()
        .max_width(ui.available_width() - 45.)
        .max_height(ui.available_height() - 50.)
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.add_space(165.);

                let action = |r1: Response,
                              r2: Response,
                              planet: &Planet,
                              h: &mut bool,
                              state: &mut UiState| {
                    if r1.clicked() || r2.clicked() {
                        state.planet_selected = Some(planet.id);
                        state.to_selected = true;
                        state.mission = false;
                        if player.owns(planet) {
                            state.mission_info.origin = planet.id;
                        }
                    } else if (r1.secondary_clicked() || r2.secondary_clicked())
                        && !planet.is_destroyed
                    {
                        state.mission_tab = MissionTab::NewMission;
                        state.mission_info.origin = state
                            .planet_selected
                            .filter(|&p| player.owns(map.get(p)))
                            .unwrap_or(player.home_planet);
                        state.mission_info.destination = planet.id;
                    } else if r1.hovered() || r2.hovered() {
                        state.planet_hover = Some(planet.id);
                        *h = true;
                    }
                };

                let mut changed_hover = false;
                egui::Grid::new("active missions").spacing([20., 0.]).striped(false).show(
                    ui,
                    |ui| {
                        for mission in missions {
                            let origin = map.get(mission.origin);
                            let destination = map.get(mission.destination);

                            if mission.owner == player.id
                                || !mission.objective.is_hidden()
                                || mission.is_seen_by_radar(map, player).is_some()
                            {
                                let resp1 = ui.cell(70., |ui| {
                                    let resp1 = ui
                                        .add_image(images.get(origin.image()), [60.; 2])
                                        .interact(Sense::click())
                                        .on_hover_cursor(CursorIcon::PointingHand);

                                    if mission.owner == player.id {
                                        let resp =
                                            ui.add_icon_on_image(images.get("logs"), resp1.rect);

                                        resp.on_hover_ui(|ui| {
                                            ui.set_min_width(350.);
                                            ui.small(format!(
                                                "Mission logs\n===========\n\n{}",
                                                mission.logs
                                            ));
                                        });
                                    }

                                    resp1
                                });

                                let resp2 = ui.cell(100., |ui| {
                                    ui.small(&origin.name)
                                        .interact(Sense::click())
                                        .on_hover_cursor(CursorIcon::PointingHand)
                                });

                                action(resp1, resp2, origin, &mut changed_hover, state);
                            } else {
                                ui.cell(70., |ui| {
                                    ui.add_image(images.get("unknown"), [60.; 2]);
                                });
                                ui.cell(100., |ui| ui.small("Unknown"));
                            }

                            let response = ui.cell(100., |ui| {
                                ui.horizontal(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.;

                                    ui.add_image(
                                        images.get(if mission.owner == player.id {
                                            mission.objective.to_lowername()
                                        } else {
                                            Icon::Attacked.to_lowername()
                                        }),
                                        [25.; 2],
                                    );

                                    let [red, green, blue] =
                                        session.player_color(mission.owner).rgb();
                                    draw_route_preview(
                                        ui,
                                        mission,
                                        player,
                                        Color32::from_rgb(red, green, blue),
                                    );

                                    ui.small(format!("+{}", mission.turns_to_destination(map)));
                                })
                                .response
                                .interact(Sense::hover())
                            });

                            if response.hovered() {
                                state.mission_hover = Some(mission.id);
                                state.mission_hover_from_ui = true;
                                changed_hover = true;
                            }

                            let resp3 = ui.cell(100., |ui| {
                                ui.small(&destination.name)
                                    .interact(Sense::click())
                                    .on_hover_cursor(CursorIcon::PointingHand)
                            });

                            let resp4 = ui.cell(70., |ui| {
                                ui.add_image(images.get(destination.image()), [60.; 2])
                                    .interact(Sense::click())
                                    .on_hover_cursor(CursorIcon::PointingHand)
                            });

                            action(resp3, resp4, destination, &mut changed_hover, state);

                            ui.end_row();
                        }

                        // If not hovering anything, reset all hover selections
                        if is_hovered && !changed_hover {
                            state.planet_hover = None;
                            state.mission_hover = None;
                        }
                    },
                );
            });
        });
}

/// Draws the mission reports interface and emits any resulting local actions.
fn draw_mission_reports(
    ui: &mut Ui,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    is_hovered: bool,
    images: &ImageIds,
) {
    let reports = player.reports.iter().filter(|r| !r.hidden).collect::<Vec<_>>();

    if reports.is_empty() {
        ui.add_space(40.);
        ui.vertical_centered(|ui| {
            ui.label(format!("No {}.", state.mission_tab.to_lowername()));
        });
        return;
    }

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.set_height(547.);

        ui.add_space(30.);

        ScrollArea::vertical().show(ui, |ui| {
            ui.set_width(150.);

            ui.vertical_centered(|ui| {
                ui.spacing_mut().item_spacing.y = 5.;

                for report in reports.iter().rev() {
                    let destination = map.get(report.mission.destination);

                    let (rect, mut response) =
                        ui.allocate_exact_size([160., 50.].into(), Sense::click());

                    ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
                        ui.horizontal_centered(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;

                            ui.add_space(7.);

                            ui.add_image(
                                images.get(report.mission.objective.to_lowername()),
                                [25.; 2],
                            );

                            let [red, green, blue] =
                                session.player_color(report.mission.owner).rgb();
                            let size = if state.mission_report == Some(report.mission.id)
                                || (response.hovered() && !response.is_pointer_button_down_on())
                            {
                                52.0
                            } else {
                                48.0
                            };
                            draw_mission_image(
                                ui,
                                images.get(report.mission.image(player)),
                                size,
                                Color32::from_rgb(red, green, blue),
                            );

                            ui.scope(|ui| {
                                ui.set_width(20.);
                                ui.small(report.turn.to_string());
                            });

                            let resp = ui.add_image(images.get(destination.image()), [40.; 2]);

                            if report.combat_report.is_some() {
                                let size = [20.; 2];
                                let pos = resp.rect.right_top() - egui::vec2(size[0], 0.);
                                ui.put(
                                    egui::Rect::from_min_size(pos, size.into()),
                                    egui::Image::new(SizedTexture::new(
                                        images.get(report.image(player)),
                                        size,
                                    )),
                                );
                            }
                        });
                    });

                    response = response.on_hover_cursor(CursorIcon::PointingHand);

                    if response.hovered() {
                        ui.painter().rect_stroke(
                            rect,
                            4.0,
                            Stroke::new(
                                1.5,
                                if response.is_pointer_button_down_on() {
                                    Color32::from_rgb(95, 131, 175)
                                } else {
                                    Color32::from_rgb(59, 66, 82)
                                },
                            ),
                            StrokeKind::Outside,
                        );
                    }

                    if response.clicked() {
                        state.mission_report = Some(report.mission.id);
                    }
                }
            });
        });

        ui.add_space(-10.);
        ui.separator();

        ui.vertical(|ui| {
            ui.set_width(ui.available_width() - 40.);

            let Some(report) = player
                .reports
                .iter()
                .find(|r| state.mission_report == Some(r.mission.id))
                .or_else(|| reports.last().copied())
            else {
                return;
            };

            ui.horizontal(|ui| {
                let action = |r1: Response,
                              r2: Response,
                              planet: &Planet,
                              h: &mut bool,
                              state: &mut UiState| {
                    if r1.clicked() || r2.clicked() {
                        state.planet_selected = Some(planet.id);
                        state.to_selected = true;
                        state.mission = false;
                        if player.owns(planet) {
                            state.mission_info.origin = planet.id;
                        }
                    } else if (r1.secondary_clicked() || r2.secondary_clicked())
                        && !planet.is_destroyed
                    {
                        state.mission_tab = MissionTab::NewMission;
                        state.mission_info.origin = state
                            .planet_selected
                            .filter(|&p| player.owns(map.get(p)))
                            .unwrap_or(player.home_planet);
                        state.mission_info.destination = planet.id;
                    } else if r1.hovered() || r2.hovered() {
                        state.planet_hover = Some(planet.id);
                        *h = true;
                    }
                };

                ui.add_space(55.);

                let mut changed_hover = false;
                egui::Grid::new("active report").spacing([10., 0.]).striped(false).show(ui, |ui| {
                    let origin = map.get(report.mission.origin);
                    let destination = map.get(report.mission.destination);

                    if report.mission.owner == player.id || !report.mission.objective.is_hidden() {
                        let resp1 = ui.cell(70., |ui| {
                            let resp1 = ui
                                .add_image(images.get(origin.image()), [60.; 2])
                                .interact(Sense::click())
                                .on_hover_cursor(CursorIcon::PointingHand);

                            if report.mission.owner == player.id {
                                let resp = ui.add_icon_on_image(images.get("logs"), resp1.rect);

                                resp.on_hover_ui(|ui| {
                                    ui.set_min_width(350.);
                                    ui.small(format!(
                                        "Mission logs\n===========\n\n{}",
                                        report.mission.logs
                                    ));
                                });
                            }

                            resp1
                        });

                        let resp2 = ui.cell(100., |ui| {
                            ui.small(&origin.name)
                                .interact(Sense::click())
                                .on_hover_cursor(CursorIcon::PointingHand)
                        });

                        action(resp1, resp2, origin, &mut changed_hover, state);
                    } else {
                        ui.cell(70., |ui| {
                            ui.add_image(images.get("unknown"), [60.; 2]);
                        });
                        ui.cell(100., |ui| ui.small("Unknown"));
                    }

                    ui.cell(100., |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;

                            ui.add_image(
                                images.get(report.mission.objective.to_lowername()),
                                [25.; 2],
                            )
                            .on_hover_small(report.mission.objective.to_name());

                            let [red, green, blue] =
                                session.player_color(report.mission.owner).rgb();
                            draw_mission_image(
                                ui,
                                images.get(report.mission.image(player)),
                                50.0,
                                Color32::from_rgb(red, green, blue),
                            );

                            ui.small(report.turn.to_string()).on_hover_small(format!(
                                "The mission arrived in turn {}.",
                                report.turn
                            ));
                        });
                    });

                    let resp3 = ui.cell(100., |ui| {
                        ui.small(&destination.name)
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand)
                    });

                    let resp4 = ui.cell(70., |ui| {
                        ui.add_image(images.get(destination.image()), [60.; 2])
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand)
                    });

                    if report.combat_report.is_some() {
                        ui.add_icon_on_image(images.get(report.image(player)), resp4.rect);
                    }

                    action(resp3, resp4, destination, &mut changed_hover, state);

                    // If not hovering anything, reset all hover selections
                    if is_hovered && !changed_hover {
                        state.planet_hover = None;
                    }
                });
            });

            ui.add_space(-10.);
            ui.horizontal(|ui| {
                ui.visuals_mut().widgets.noninteractive.bg_stroke.width = 6.;

                let a_color = session.player_color(report.mission.owner).color();
                let d_color = report
                    .planet
                    .controlled
                    .or(report.planet.owned)
                    .map_or(Color::srgb_u8(150, 158, 170), |defender| {
                        session.player_color(defender).color()
                    });

                ui.vertical(|ui| {
                    ui.set_width(140.);
                    ui.visuals_mut().widgets.noninteractive.bg_stroke.color = a_color.to_color32();
                    ui.separator();
                });
                ui.vertical(|ui| {
                    ui.set_width(ui.available_width());
                    ui.visuals_mut().widgets.noninteractive.bg_stroke.color = d_color.to_color32();
                    ui.separator();
                });
            });
            ui.add_space(-10.);

            ui.horizontal(|ui| {
                ui.set_height(357.);

                ui.vertical(|ui| {
                    ui.set_width(140.);

                    let army = match report.mission.objective {
                        Icon::MissileStrike => vec![Unit::interplanetary_missile()],
                        Icon::Spy => vec![Unit::probe()],
                        _ => Unit::ships(),
                    };

                    draw_army_grid(ui, "attacker", &army, report, player, images);

                    if report.scout_probes > 0 && report.can_see(&Side::Attacker, player.id) {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;
                            ui.add_image(images.get(Icon::Spy.to_lowername()), [15., 15.]);
                            ui.small(format!("Scouts: {}", report.scout_probes));
                        })
                        .response
                        .on_hover_small_ext(
                            "Number of attacking Probes that left combat after the first round.",
                        );
                    }
                });

                ui.add_space(-13.);
                ui.separator();
                ui.add_space(-10.);

                ui.vertical(|ui| {
                    ui.set_height(450.);

                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.;

                        let destination = map.get(report.mission.destination);

                        if !report.planet.army.has_army() {
                            ui.label(format!(
                                "Empty {}.",
                                if destination.is_moon() {
                                    "moon"
                                } else {
                                    "planet"
                                }
                            ));
                        } else {
                            let units = Unit::all_valid(destination.is_moon());
                            for (i, army) in [units.get(1), units.get(2), units.first()]
                                .into_iter()
                                .flatten()
                                .enumerate()
                            {
                                draw_army_grid(
                                    ui,
                                    format!("defender_{i}").as_str(),
                                    army,
                                    report,
                                    player,
                                    images,
                                );
                            }
                        }
                    });

                    if (report.planet_destroyed || report.planet_colonized)
                        && report.can_see(&Side::Defender, player.id)
                    {
                        let (icon, label) = if report.planet_destroyed {
                            (Icon::Destroy, "Planet destroyed")
                        } else {
                            (Icon::Colonize, "Planet colonized")
                        };

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.0;
                            ui.add_space(10.);
                            ui.add_image(images.get(icon.to_lowername()), [15.0, 15.0]);
                            ui.small(label);
                        });
                    }

                    ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
                        if report.combat_report.is_some()
                            && report.can_see(&Side::Attacker, player.id)
                            && report.can_see(&Side::Defender, player.id)
                            && ui.add_custom_button("Combat details", images).clicked()
                        {
                            state.combat_report = Some(report.id);
                            state.combat_report_round = 1;
                        }
                    });
                });
            });
        });
    });
}

/// Draws the mission interface and emits any resulting local actions.
pub(super) fn draw_mission(
    ui: &mut Ui,
    missions: &[Mission],
    send_mission: &mut MessageWriter<SendMissionMsg>,
    settings: &Settings,
    state: &mut UiState,
    map: &mut Map,
    player: &mut Player,
    session: &MultiplayerSession,
    is_hovered: bool,
    keyboard: &ButtonInput<KeyCode>,
    images: &ImageIds,
    editable: bool,
) {
    ui.add_space(17.);
    ui.horizontal(|ui| {
        ui.style_mut().spacing.button_padding = egui::vec2(6., 0.);

        ui.add_space(105.);
        for tab in MissionTab::iter() {
            ui.selectable_value(&mut state.mission_tab, tab, tab.to_title());
        }
    });

    match state.mission_tab {
        MissionTab::NewMission => {
            ui.add_enabled_ui(editable, |ui| {
                draw_new_mission(
                    ui,
                    send_mission,
                    settings,
                    state,
                    map,
                    player,
                    is_hovered,
                    keyboard,
                    images,
                )
            });
        },
        MissionTab::ActiveMissions => draw_active_missions(
            ui,
            missions.iter().filter(|m| m.owner == player.id).collect(),
            state,
            map,
            player,
            session,
            is_hovered,
            images,
        ),
        MissionTab::EnemyMissions => draw_active_missions(
            ui,
            missions.iter().filter(|m| m.owner != player.id).collect(),
            state,
            map,
            player,
            session,
            is_hovered,
            images,
        ),
        MissionTab::MissionReports => {
            draw_mission_reports(ui, state, map, player, session, is_hovered, images)
        },
    }
}
