use bevy::prelude::*;
use bevy_egui::egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::{
    emath, Align, Align2, Color32, ComboBox, CursorIcon, FontData, FontFamily, Layout, Response,
    RichText, ScrollArea, Sense, Separator, TextStyle, Ui, UiBuilder,
};
use bevy_egui::{egui, EguiContexts, EguiTextureHandle};
use itertools::Itertools;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::core::assets::WorldAssets;
use crate::core::camera::MainCamera;
use crate::core::combat::CombatStats;
use crate::core::map::icon::Icon;
use crate::core::map::map::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::systems::PlanetCmp;
use crate::core::missions::{Mission, MissionId, Missions, SendMissionMsg};
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::ui::aesthetics::Aesthetics;
use crate::core::ui::dark::NordDark;
use crate::core::ui::utils::{toggle, CustomUi, ImageIds};
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Combat, Description, Price, Unit};
use crate::utils::NameFromEnum;

#[derive(Component)]
pub struct UiCmp;

#[derive(Clone, Debug, Default)]
pub enum Shop {
    #[default]
    Buildings,
    Fleet,
    Defenses,
}

#[derive(EnumIter, Copy, Clone, Debug, Default, PartialEq)]
pub enum MissionTab {
    #[default]
    NewMission,
    ActiveMissions,
    IncomingAttacks,
}
#[derive(Resource, Default)]
pub struct UiState {
    pub planet_hover: Option<PlanetId>,
    pub planet_selected: Option<PlanetId>,
    pub to_selected: bool,
    pub shop: Shop,
    pub mission: bool,
    pub mission_tab: MissionTab,
    pub mission_info: Mission,
    pub mission_hover: Option<MissionId>,
    pub end_turn: bool,
}

fn draw_panel<R>(
    contexts: &mut EguiContexts,
    name: &str,
    image: &str,
    pos: (f32, f32),
    size: (f32, f32),
    images: &ImageIds,
    content: impl FnOnce(&mut Ui) -> R,
) {
    egui::Window::new(name)
        .frame(egui::Frame {
            fill: Color32::TRANSPARENT,
            ..default()
        })
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_pos(pos)
        .fixed_size(size)
        .show(contexts.ctx_mut().unwrap(), |ui| {
            let response =
                ui.add(egui::Image::new(SizedTexture::new(images.get(image), ui.available_size())));

            ui.scope_builder(UiBuilder::new().max_rect(response.rect), content);
        });
}

fn draw_resources(ui: &mut Ui, settings: &Settings, map: &Map, player: &Player, images: &ImageIds) {
    ui.add_space(10.);

    ui.horizontal_centered(|ui| {
        ui.add_space(80.);

        let response = ui
            .scope(|ui| {
                ui.add_image(images.get("turn"), [65., 40.]);
                ui.heading(settings.turn.to_string());
            })
            .response;

        if settings.show_hover {
            response.on_hover_ui(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.add_image(images.get("turn"), [130., 90.]);
                    });
                    ui.vertical(|ui| {
                        ui.label("Turn");
                        ui.separator();
                        ui.small("Current turn in the game.");
                    });
                });
            });
        }

        ui.add_space(120.);

        for resource in ResourceName::iter() {
            let response = ui
                .scope(|ui| {
                    ui.add_image(images.get(resource.to_lowername()), [65., 40.]);
                    ui.heading(player.resources.get(&resource).to_string());
                    ui.add_space(35.);
                })
                .response;

            if settings.show_hover {
                response.on_hover_ui(|ui| {
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.add_image(images.get(resource.to_lowername()), [130., 90.]);
                        });
                        ui.vertical(|ui| {
                            ui.label(resource.to_name());
                            ui.separator();
                            ui.scope(|ui| {
                                ui.style_mut().interaction.selectable_labels = true;
                                ui.small(format!(
                                    "Production: +{}",
                                    player.resource_production(&map.planets).get(&resource)
                                ))
                                .on_hover_cursor(CursorIcon::Default)
                                .on_hover_text_at_pointer(
                                    RichText::new(
                                        map.planets
                                            .iter()
                                            .filter_map(|p| {
                                                player.owns(p).then_some((
                                                    p.name.clone(),
                                                    p.resource_production().get(&resource),
                                                ))
                                            })
                                            .sorted_by(|a, b| b.1.cmp(&a.1))
                                            .map(|(n, c)| format!("{}: {}", n, c))
                                            .join("\n"),
                                    )
                                    .small(),
                                );
                            });
                            ui.small(resource.description());
                        });
                    });
                });
            }
        }
    });
}

fn draw_overview(ui: &mut Ui, planet: &Planet, units: &[Vec<Unit>; 3], images: &ImageIds) {
    ui.add_space(17.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 7.;
        ui.add_space(80.);
        ui.add_image(images.get("overview"), [20., 20.]);
        ui.small(format!("Overview: {}", &planet.name));
    });

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

        for units in units.iter() {
            ui.add_space(20.);

            ui.vertical(|ui| {
                for unit in units {
                    ui.horizontal(|ui| {
                        ui.add_image(images.get(unit.to_lowername()), [50., 50.]);
                        ui.label(planet.get(&unit).to_string());
                    })
                    .response
                    .on_hover_ui(|ui| {
                        ui.small(unit.to_name());
                    });
                }
            });
        }
    });
}

fn draw_fleet(ui: &mut Ui, planet: &Planet, units: &[Vec<Unit>; 3], images: &ImageIds) {
    ui.add_space(17.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 7.;
        ui.add_space(30.);
        ui.add_image(images.get("fleet"), [25., 25.]);
        ui.small("Fleet");
    });

    ui.add_space(10.);

    ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);
    for unit in units[1].iter() {
        ui.horizontal(|ui| {
            ui.add_space(30.);
            ui.add_image(images.get(unit.to_lowername()), [50., 50.]);
            ui.label(planet.get(unit).to_string());
        });
    }
}

fn draw_new_mission(
    ui: &mut Ui,
    send_mission: &mut MessageWriter<SendMissionMsg>,
    settings: &Settings,
    state: &mut UiState,
    map: &mut Map,
    player: &mut Player,
    units: &[Vec<Unit>; 3],
    is_hovered: bool,
    images: &ImageIds,
) {
    let origin = map.get(state.mission_info.origin);
    let destination = map.get(state.mission_info.destination);

    state.mission_info.owner = player.id;

    // Start a bit outside the origin planet to be able to see the image
    let direction = (-origin.position + destination.position).normalize();
    state.mission_info.position = origin.position + direction * Planet::SIZE;

    if origin.controlled == destination.controlled {
        state.mission_info.objective = Icon::Deploy;
    } else if state.mission_info.objective == Icon::Deploy {
        state.mission_info.objective = Icon::default();
    }

    if !state.mission_info.objective.condition(origin) {
        state.mission_info.objective =
            Icon::iter().find(|i| i.is_mission() && i.condition(origin)).unwrap_or_default();
    }

    let army = if state.mission_info.objective == Icon::MissileStrike {
        &vec![Unit::Defense(Defense::InterplanetaryMissile)]
    } else {
        &units[1]
    };

    ui.horizontal_top(|ui| {
        ui.add_space(60.);

        let action = |r1: Response, id, h: &mut bool, state: &mut UiState| {
            if r1.clicked() {
                state.planet_selected = Some(id);
                state.to_selected = true;
                state.mission_info.origin = id;
                state.mission = false;
            } else if r1.secondary_clicked() {
                state.mission_tab = MissionTab::NewMission;
                state.mission_info.origin = state
                    .planet_selected
                    .filter(|&p| player.owns(map.get(p)))
                    .unwrap_or(player.home_planet);
                state.mission_info.destination = id;
            } else if r1.hovered() {
                state.planet_hover = Some(id);
                *h = true;
            }
        };

        let mut changed_hover = false;
        egui::Grid::new("mission_origin_destination").spacing([30., 0.]).striped(false).show(
            ui, |ui| {
                let response = ui.cell(70., |ui| {
                    ui.add_image(images.get(format!("planet{}", origin.image)), [70., 70.])
                        .interact(Sense::click())
                        .on_hover_cursor(CursorIcon::PointingHand)
                });

                action(response, origin.id, &mut changed_hover, state);

                ui.cell(100., |ui| {
                    ui.vertical(|ui| {
                        ui.add_space(15.);
                        ComboBox::from_id_salt("origin")
                            .height(50.)
                            .selected_text(&map.get(state.mission_info.origin).name)
                            .show_ui(ui, |ui| {
                                for planet in map.planets.iter().filter(|p| player.controls(p)).sorted_by(|a, b| a.name.cmp(&b.name)) {
                                    ui.selectable_value(
                                        &mut state.mission_info.origin,
                                        planet.id,
                                        &planet.name,
                                    );
                                }
                            })
                            .response
                            .on_hover_cursor(CursorIcon::PointingHand);
                    });
                });

                let (rect, mut response) = ui.cell(50., |ui| {
                    ui.allocate_exact_size([50., 50.].into(), Sense::click())
                });

                response = response
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_ui(|ui| {
                        ui.small("Click to select all units on the origin planet. Right-click to unselect all.");
                    });

                let image = if response.hovered() && !response.is_pointer_button_down_on() {
                    images.get(format!("{} hover", state.mission_info.image(player)))
                } else {
                    images.get(state.mission_info.image(player))
                };

                ui.add_image_painter(image, rect);

                if response.clicked() {
                    state.mission_info.army = army.iter().map(|u| (u.clone(), origin.get(u))).collect();
                } else if response.secondary_clicked() {
                    state.mission_info.army.clear();
                }

                ui.cell(100., |ui| {
                    ui.vertical(|ui| {
                        ui.add_space(15.);
                        ComboBox::from_id_salt("destination")
                            .selected_text(&map.get(state.mission_info.destination).name)
                            .show_ui(ui, |ui| {
                                for planet in map.planets.iter().sorted_by(|a, b| a.name.cmp(&b.name)) {
                                    ui.selectable_value(
                                        &mut state.mission_info.destination,
                                        planet.id,
                                        &planet.name,
                                    );
                                }
                            })
                            .response
                            .on_hover_cursor(CursorIcon::PointingHand);
                    });
                });

                let response = ui.cell(70., |ui| {
                    ui.add_image(images.get(format!("planet{}", destination.image)), [70., 70.])
                        .interact(Sense::hover())
                        .on_hover_cursor(CursorIcon::PointingHand)
                });

                action(response, destination.id, &mut changed_hover, state);
            });

        // If not hovering anything, reset hover selection
        if is_hovered && !changed_hover {
            state.planet_hover = None;
        }
    });

    ui.add_space(-10.);
    ui.add(Separator::default().shrink(50.));

    if state.mission_info.origin == state.mission_info.destination {
        ui.add_space(20.);
        ui.vertical_centered(|ui| {
            ui.colored_label(Color32::RED, "The origin and destination planets must be different.");
        });
    } else {
        ui.horizontal(|ui| {
            ui.add_space(70.);

            ui.vertical(|ui| {
                ui.set_width(260.);

                egui::Grid::new("units").striped(false).num_columns(2).spacing([25., 8.]).show(
                    ui,
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 8.;

                        for (i, unit) in army.iter().enumerate() {
                            let n = origin.get(unit);

                            ui.add_enabled_ui(n > 0, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.set_width(110.);

                                        let response = ui
                                            .add_image(
                                                images.get(unit.to_lowername()),
                                                [55., 55.],
                                            )
                                            .interact(Sense::click())
                                            .on_hover_cursor(CursorIcon::PointingHand)
                                            .on_hover_ui(|ui| {
                                                ui.small(unit.to_name());
                                            })
                                            .on_disabled_hover_ui(|ui| {
                                                ui.small(unit.to_name());
                                            });

                                        if response.clicked() {
                                            *state.mission_info.army.entry(*unit).or_insert(0) = n;
                                        }

                                        if response.secondary_clicked() {
                                            *state.mission_info.army.entry(*unit).or_insert(0) = 0;
                                        }

                                        let rect = response.rect;
                                        let painter = ui.painter();

                                        painter.text(
                                            rect.left_bottom() + egui::vec2(4., -4.),
                                            Align2::LEFT_BOTTOM,
                                            n.to_string(),
                                            TextStyle::Button.resolve(ui.style()),
                                            Color32::WHITE,
                                        );

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

            ui.add_space(10.);

            ui.vertical(|ui| {
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

                    for icon in Icon::objectives(player.controls(destination)) {
                        ui.add_enabled_ui(icon.condition(origin), |ui| {
                            let button = ui
                                .add(
                                    egui::Button::image(SizedTexture::new(
                                        images.get(icon.to_lowername()),
                                        [40., 40.],
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
                                        state
                                            .mission_info
                                            .army
                                            .remove(&Unit::Defense(Defense::InterplanetaryMissile));
                                    },
                                }

                                state.mission_info.objective = icon;
                            }
                        });
                    }
                });

                ui.add_space(5.);

                let speed = state.mission_info.speed();
                let distance = state.mission_info.distance(map);
                let duration = state.mission_info.duration(map);
                let fuel = state.mission_info.fuel_consumption(map);

                ui.horizontal(|ui| {
                    ui.small("Objective:");

                    ui.spacing_mut().item_spacing.x = 4.;
                    ui.add_image(
                        images.get(state.mission_info.objective.to_lowername()),
                        [20., 20.],
                    );
                    ui.small(state.mission_info.objective.to_name());
                });

                ui.small(format!("Distance: {distance:.1} AU"));
                ui.small(format!(
                    "Speed: {}",
                    if speed == 0. || speed == f32::MAX {
                        "---".to_string()
                    } else {
                        format!("{speed} AU/turn")
                    }
                ));
                ui.small(format!(
                    "Duration: {}",
                    if duration == 0 {
                        "---".to_string()
                    } else {
                        format!(
                            "+{} turn{} ({})",
                            duration,
                            if duration == 1 { "" } else { "s" },
                            settings.turn + duration,
                        )
                    }
                ));
                ui.small(format!("Fuel consumption: {fuel}"));

                let mut has_gate = false;
                if state.mission_info.objective == Icon::Deploy {
                    if player.owns(origin)
                        && player.owns(destination)
                        && origin.get(&Unit::Building(Building::JumpGate)) > 0
                        && destination.get(&Unit::Building(Building::JumpGate)) > 0 {
                        has_gate = true;

                        let jump_cost = state.mission_info.jump_cost();
                        let can_jump = origin.jump_gate + jump_cost <= origin.max_jump_capacity();
                        if !can_jump {
                            state.mission_info.jump_gate = false;
                        }

                        ui.add_enabled_ui(can_jump, |ui| {
                            ui.horizontal(|ui| {
                                ui.small(format!("Jump Gate ({}/{}):", jump_cost, origin.max_jump_capacity() - origin.jump_gate));
                                ui.add(toggle(&mut state.mission_info.jump_gate));
                            });
                        })
                            .response
                            .on_hover_ui(|ui| {
                                ui.small(
                                    "Whether to send this mission through the Jump Gate. \
                                    Missions through the Jump Gate always take 1 turn and cost \
                                    no fuel."
                                );
                            })
                            .on_disabled_hover_ui(|ui| {
                                ui.small(
                                    "The selected fleet has a higher jump cost than the Gate \
                                    can transfer this turn."
                                );
                            });
                    }
                } else {
                    state.mission_info.jump_gate = false;
                }

                ui.add_space(if has_gate { 5. } else { 45. });

                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                    ui.add_space(40.);

                    let army_check = state.mission_info.army.values().sum::<usize>() > 0;
                    let fuel_check = player.resources.get(&ResourceName::Deuterium) >= fuel;
                    let objective_check = match state.mission_info.objective {
                        Icon::Deploy => state.mission_info.army.iter().any(|(u, c)| u.is_ship() && *c > 0),
                        Icon::Colonize => state.mission_info.get(&Unit::Ship(Ship::ColonyShip)) > 0,
                        Icon::Attack => state
                            .mission_info
                            .army
                            .iter()
                            .any(|(u, n)| *n > 0 && matches!(u, Unit::Ship(s) if s.is_combat())),
                        Icon::Spy => {
                            state.mission_info.get(&Unit::Ship(Ship::Probe))
                                == state.mission_info.total()
                        },
                        Icon::MissileStrike => {
                            state.mission_info.get(&Unit::Defense(Defense::InterplanetaryMissile))
                                == state.mission_info.total()
                        },
                        Icon::Destroy => state.mission_info.get(&Unit::Ship(Ship::WarSun)) > 0,
                        _ => unreachable!(),
                    };

                    ui.add_enabled_ui(army_check && fuel_check && objective_check, |ui| {
                        let (rect, mut response) =
                            ui.allocate_exact_size([180., 50.].into(), Sense::click());

                        response = response
                            .on_hover_cursor(CursorIcon::PointingHand)
                            .on_disabled_hover_ui(|ui| {
                                if !army_check {
                                    ui.small("No ships selected for the mission.");
                                } else if !fuel_check {
                                    ui.small("Not enough fuel (deuterium) for the mission.");
                                } else {
                                    ui.small("The ship requirements for the mission objective is not met.");
                                }
                            });

                        let image = if response.hovered() && !response.is_pointer_button_down_on() {
                            images.get("button hover")
                        } else {
                            images.get("button")
                        };

                        ui.add_image_painter(image, rect);

                        ui.painter().text(
                            rect.center(),
                            Align2::CENTER_CENTER,
                            "Send mission",
                            TextStyle::Button.resolve(ui.style()),
                            Color32::WHITE,
                        );

                        if response.clicked() {
                            send_mission
                                .write(SendMissionMsg::new(Mission::from(&state.mission_info)));
                            state.planet_selected = None;
                            state.mission = false;
                            state.mission_info = Mission::default();
                        }
                    });
                });
            });
        });
    }
}

fn draw_active_missions(
    ui: &mut Ui,
    missions: Vec<&Mission>,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    is_hovered: bool,
    images: &ImageIds,
) {
    if missions.len() == 0 {
        ui.add_space(20.);
        ui.vertical_centered(|ui| {
            ui.label(format!("No {}.", state.mission_tab.to_lowername()));
        });
        return;
    }

    // Sort by turns remaining ascending
    let missions = missions
        .iter()
        .sorted_by(|a, b| a.turns_to_destination(map).cmp(&b.turns_to_destination(map)));

    ui.add_space(20.);

    ScrollArea::vertical().show(ui, |ui| {
        ui.set_width(ui.available_width() - 45.);
        ui.set_height(ui.available_height() - 100.);

        ui.horizontal(|ui| {
            ui.add_space(90.);

            let action = |r1: Response, r2: Response, id, h: &mut bool, state: &mut UiState| {
                if r1.clicked() || r2.clicked() {
                    state.planet_selected = Some(id);
                    state.to_selected = true;
                    state.mission_info.origin = id;
                    state.mission = false;
                } else if r1.secondary_clicked() || r2.secondary_clicked() {
                    state.mission_tab = MissionTab::NewMission;
                    state.mission_info.origin = state
                        .planet_selected
                        .filter(|&p| player.owns(map.get(p)))
                        .unwrap_or(player.home_planet);
                    state.mission_info.destination = id;
                } else if r1.hovered() || r2.hovered() {
                    state.planet_hover = Some(id);
                    *h = true;
                }
            };

            let mut changed_hover = false;
            egui::Grid::new("active missions").spacing([20., 0.]).striped(false).show(ui, |ui| {
                for mission in missions {
                    let origin = map.get(mission.origin);
                    let destination = map.get(mission.destination);

                    let resp1 = ui.cell(70., |ui| {
                        ui.add_image(images.get(format!("planet{}", origin.image)), [70., 70.])
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand)
                    });

                    let resp2 = ui.cell(100., |ui| {
                        ui.small(&origin.name)
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand)
                    });

                    action(resp1, resp2, origin.id, &mut changed_hover, state);

                    let response = ui.cell(100., |ui| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 4.;

                            ui.add_image(
                                images.get(if mission.owner == player.id {
                                    mission.objective.to_lowername()
                                } else {
                                    Icon::Attacked.to_lowername()
                                }),
                                [25., 25.],
                            );

                            ui.add_image(
                                if state.mission_hover == Some(mission.id) {
                                    images.get(format!("{} hover", mission.image(player)))
                                } else {
                                    images.get(mission.image(player))
                                },
                                [50., 50.],
                            );

                            ui.small(format!("+{}", mission.turns_to_destination(map)));
                        })
                        .response
                        .interact(Sense::hover())
                    });

                    if response.hovered() {
                        state.mission_hover = Some(mission.id);
                        changed_hover = true;
                    }

                    let resp3 = ui.cell(100., |ui| {
                        ui.small(&destination.name)
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand)
                    });

                    let resp4 = ui.cell(70., |ui| {
                        ui.add_image(images.get(format!("planet{}", destination.image)), [70., 70.])
                            .interact(Sense::click())
                            .on_hover_cursor(CursorIcon::PointingHand)
                    });

                    action(resp3, resp4, destination.id, &mut changed_hover, state);

                    ui.end_row();
                }

                // If not hovering anything, reset all hover selections
                if is_hovered && !changed_hover {
                    state.planet_hover = None;
                    state.mission_hover = None;
                }
            });
        });
    });
}

fn draw_mission(
    ui: &mut Ui,
    missions: &Vec<Mission>,
    send_mission: &mut MessageWriter<SendMissionMsg>,
    settings: &Settings,
    state: &mut UiState,
    map: &mut Map,
    player: &mut Player,
    units: &[Vec<Unit>; 3],
    is_hovered: bool,
    images: &ImageIds,
) {
    ui.add_space(12.);
    ui.horizontal(|ui| {
        ui.style_mut().spacing.button_padding = egui::vec2(6., 0.);

        ui.add_space(40.);
        for tab in MissionTab::iter() {
            ui.selectable_value(&mut state.mission_tab, tab, tab.to_title());
        }
    });

    match state.mission_tab {
        MissionTab::NewMission => draw_new_mission(
            ui,
            send_mission,
            settings,
            state,
            map,
            player,
            units,
            is_hovered,
            images,
        ),
        MissionTab::ActiveMissions => draw_active_missions(
            ui,
            missions.iter().filter(|m| m.owner == player.id).collect(),
            state,
            map,
            player,
            is_hovered,
            images,
        ),
        MissionTab::IncomingAttacks => draw_active_missions(
            ui,
            missions.iter().filter(|m| m.owner != player.id).collect(),
            state,
            map,
            player,
            is_hovered,
            images,
        ),
    }
}

fn draw_mission_fleet_hover(
    ui: &mut Ui,
    mission: &Mission,
    player: &Player,
    units: &[Vec<Unit>; 3],
    images: &ImageIds,
) {
    let army = if mission.objective == Icon::MissileStrike {
        &vec![Unit::Defense(Defense::InterplanetaryMissile)]
    } else {
        &units[1]
    };

    ui.add_space(17.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 7.;
        ui.add_space(20.);
        ui.add_image(images.get(mission.image(player)), [25., 25.]);
        ui.small("Mission");
    });

    ui.add_space(10.);

    ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);
    for unit in army.iter() {
        ui.horizontal(|ui| {
            ui.add_space(30.);
            ui.add_image(images.get(unit.to_lowername()), [50., 50.]);
            ui.label(mission.get(unit).to_string());
        });
    }
}

fn draw_mission_info_hover(
    ui: &mut Ui,
    mission: &Mission,
    settings: &Settings,
    map: &Map,
    player: &Player,
    images: &ImageIds,
) {
    let origin = map.get(mission.origin);
    let destination = map.get(mission.destination);

    ui.add_space(40.);

    ui.spacing_mut().item_spacing.y = 10.;

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.small("Origin:");

        ui.spacing_mut().item_spacing.x = 4.;
        ui.add_image(images.get(format!("planet{}", origin.image)), [25., 25.]);
        ui.small(origin.name.to_name());
    });

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.small("Destination:");

        ui.spacing_mut().item_spacing.x = 4.;
        ui.add_image(images.get(format!("planet{}", destination.image)), [25., 25.]);
        ui.small(destination.name.to_name());
    });

    ui.add(Separator::default().shrink(20.));

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.small("Objective:");

        ui.spacing_mut().item_spacing.x = 4.;
        let objective = if mission.owner == player.id {
            mission.objective
        } else {
            Icon::Attacked
        };
        ui.add_image(images.get(objective.to_lowername()), [20., 20.]);
        ui.small(objective.to_name());
    });

    ui.add(Separator::default().shrink(20.));

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.vertical(|ui| {
            ui.small(format!("Distance: {:.1} AU", mission.distance(map)));

            let speed = mission.speed();
            ui.small(format!(
                "Speed: {}",
                if speed == f32::MAX {
                    "---".to_string()
                } else {
                    format!("{speed} AU/turn")
                }
            ));

            let duration = mission.duration(map);
            ui.small(format!(
                "Duration: +{} turn{} ({})",
                duration,
                if duration == 1 {
                    ""
                } else {
                    "s"
                },
                settings.turn + duration
            ));
        });
    });
}

fn draw_unit_hover(
    ui: &mut Ui,
    unit: &Unit,
    units: &[Vec<Unit>; 3],
    msg: Option<String>,
    images: &ImageIds,
) {
    ui.horizontal(|ui| {
        ui.set_width(700.);

        ui.vertical(|ui| {
            ui.add_image(images.get(unit.to_lowername()), [200., 200.]);
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
                    if i == 0 || row.iter().any(|s| unit.get(s) != "---") {
                        egui::Grid::new(ui.auto_id_with(format!("row_{:?}", row[0])))
                            .spacing([20., 0.])
                            .striped(false)
                            .show(ui, |ui| {
                                for stat in row {
                                    ui.horizontal(|ui| {
                                        ui.set_width(150.);
                                        ui.style_mut().interaction.selectable_labels = true;

                                        ui.add_image(images.get(stat.to_lowername()), [70., 45.]);
                                        ui.label(unit.get(&stat))
                                            .on_hover_cursor(CursorIcon::Default);
                                    })
                                    .response
                                    .on_hover_ui(|ui| stat_hover(ui, stat));
                                }
                            });
                    }

                    ui.spacing_mut().item_spacing.y = 10.;
                }
            }

            if !unit.rapid_fire().is_empty() {
                ui.separator();
                ui.small(CombatStats::RapidFire.to_name())
                    .on_hover_ui(|ui| stat_hover(ui, &CombatStats::RapidFire));

                egui::Grid::new("rapid_fire").spacing([10., 10.]).striped(false).show(ui, |ui| {
                    let mut counter = 0;
                    for rf_unit in units.iter().take(2).flatten() {
                        if let Some(rf) = unit.rapid_fire().get(&rf_unit) {
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

fn draw_shop(
    ui: &mut Ui,
    state: &mut UiState,
    settings: &Settings,
    player: &mut Player,
    planet: &mut Planet,
    units: &[Vec<Unit>; 3],
    images: &ImageIds,
) {
    ui.spacing_mut().item_spacing = emath::Vec2::new(4., 4.);

    ui.add_space(4.);

    let (production, idx) = match state.shop {
        Shop::Buildings => (None, 0),
        Shop::Fleet => (Some((planet.fleet_production(), planet.max_fleet_production())), 1),
        Shop::Defenses => (Some((planet.battery_production(), planet.max_battery_production())), 2),
    };

    ui.horizontal(|ui| {
        ui.add_space(45.);
        ui.add_image(images.get(state.shop.to_lowername()), [20., 20.]);
        ui.small(state.shop.to_name());

        if let Some((current, max)) = production {
            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                ui.add_space(45.);
                ui.small(format!("Production: {}/{}", current, max));
            });
        }
    });

    ui.add_space(10.);

    for row in units[idx].chunks(5) {
        ui.horizontal(|ui| {
            ui.add_space(25.);

            for unit in row {
                let count = planet.get(unit);
                let bought = planet.buy.iter().filter(|u| *u == unit).count();

                let resources_check = player.resources >= unit.price();
                let (level_check, building_check, production_check) = match unit {
                    Unit::Building(_) => {
                        (true, count < Building::MAX_LEVEL, !planet.buy.contains(unit))
                    },
                    Unit::Ship(s) => (
                        s.production() <= planet.get(&Unit::Building(Building::Shipyard)),
                        true,
                        planet.fleet_production() + s.production() <= planet.max_fleet_production(),
                    ),
                    Unit::Defense(d) if d.is_missile() => (
                        d.production() <= planet.get(&Unit::Building(Building::MissileSilo)),
                        true,
                        planet.battery_production() + d.production()
                            <= planet.max_battery_production()
                            && planet.missile_capacity() + bought < planet.max_missile_capacity(),
                    ),
                    Unit::Defense(d) => (
                        d.production() <= planet.get(&Unit::Building(Building::Factory)),
                        true,
                        planet.battery_production() + d.production()
                            <= planet.max_battery_production(),
                    ),
                };

                ui.add_enabled_ui(
                    resources_check && level_check && building_check && production_check,
                    |ui| {
                        ui.spacing_mut().button_padding = egui::Vec2::splat(2.);

                        let mut response =
                            ui.add_image_button(images.get(unit.to_lowername()), [130., 130.]);

                        if ui.is_enabled() {
                            response = response.on_hover_cursor(CursorIcon::PointingHand);
                        }

                        if response.clicked() {
                            player.resources -= unit.price();
                            planet.buy.push(unit.clone());
                        }

                        if !unit.is_building()
                            && response.secondary_clicked()
                            && player.resources >= unit.price() * 5usize
                        {
                            player.resources -= unit.price() * 5usize;
                            planet.buy.extend([unit.clone(); 5]);
                        }

                        let rect = response.rect;
                        let painter = ui.painter();

                        if count > 0 {
                            let text = match unit {
                                Unit::Building(Building::MissileSilo) => Some(format!(
                                    "{}/{}",
                                    planet.missile_capacity(),
                                    planet.max_missile_capacity()
                                )),
                                Unit::Building(Building::JumpGate) => Some(format!(
                                    "{}/{}",
                                    planet.jump_gate,
                                    planet.max_jump_capacity()
                                )),
                                _ => None,
                            };

                            if let Some(text) = text {
                                painter.text(
                                    rect.right_top() + egui::vec2(-7., 4.),
                                    Align2::RIGHT_TOP,
                                    text,
                                    TextStyle::Body.resolve(ui.style()),
                                    Color32::WHITE,
                                );
                            }
                        }

                        painter.text(
                            rect.left_bottom() + egui::vec2(7., -4.),
                            Align2::LEFT_BOTTOM,
                            count.to_string(),
                            TextStyle::Heading.resolve(ui.style()),
                            Color32::WHITE,
                        );

                        if bought > 0 {
                            let offset_x = ui
                                .painter()
                                .layout_no_wrap(
                                    count.to_string(),
                                    TextStyle::Heading.resolve(ui.style()),
                                    Color32::WHITE,
                                )
                                .size()
                                .x;

                            painter.text(
                                rect.left_bottom() + egui::vec2(8. + offset_x, -12.),
                                Align2::LEFT_BOTTOM,
                                format!(" (+{})", bought),
                                TextStyle::Body.resolve(ui.style()),
                                Color32::WHITE,
                            );
                        }

                        if settings.show_hover {
                            response
                                .on_hover_ui(|ui| {
                                    draw_unit_hover(ui, unit, units, None, &images);
                                })
                                .on_disabled_hover_ui(|ui| {
                                    draw_unit_hover(
                                        ui,
                                        unit,
                                        units,
                                        Some(if !resources_check {
                                            "Not enough resources.".to_string()
                                        } else if !building_check {
                                            "Building already at maximum level.".to_string()
                                        } else if !level_check {
                                            format!(
                                                "Requires {} level {}.",
                                                match unit {
                                                    Unit::Ship(_) => Building::Shipyard.to_name(),
                                                    Unit::Defense(d) if d.is_missile() =>
                                                        Building::MissileSilo.to_name(),
                                                    Unit::Defense(_) => Building::Factory.to_name(),
                                                    _ => unreachable!(),
                                                },
                                                unit.production()
                                            )
                                        } else {
                                            "Production limit reached.".to_string()
                                        }),
                                        &images,
                                    );
                                });
                        }
                    },
                );
            }
        });
    }
}

pub fn set_ui_style(mut contexts: EguiContexts) {
    let context = contexts.ctx_mut().unwrap();
    context.set_style(NordDark.custom_style());

    context.add_font(FontInsert::new(
        "firasans",
        FontData::from_static(include_bytes!("../../../assets/fonts/FiraSans-Bold.ttf")),
        vec![InsertFontFamily {
            family: FontFamily::Proportional,
            priority: FontPriority::Highest,
        }],
    ));
}

pub fn add_ui_images(
    mut contexts: EguiContexts,
    mut images: ResMut<ImageIds>,
    assets: Local<WorldAssets>,
) {
    for (k, v) in assets.images.iter() {
        let id = contexts.add_image(EguiTextureHandle::Strong(v.clone()));
        images.0.insert(k, id);
    }
}

pub fn draw_ui(
    mut contexts: EguiContexts,
    planet_q: Query<(&Transform, &PlanetCmp)>,
    camera_q: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut send_mission: MessageWriter<SendMissionMsg>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    missions: Res<Missions>,
    mut state: ResMut<UiState>,
    settings: Res<Settings>,
    images: Res<ImageIds>,
    window: Single<&Window>,
) {
    let (camera, camera_t) = camera_q.into_inner();
    let (width, height) = (window.width(), window.height());

    let units: [Vec<Unit>; 3] = [
        Building::iter().map(|b| Unit::Building(b)).collect(),
        Ship::iter().map(|s| Unit::Ship(s)).collect(),
        Defense::iter().map(|d| Unit::Defense(d)).collect(),
    ];

    draw_panel(
        &mut contexts,
        "resources",
        "thin panel",
        (window.width() * 0.5 - 525., window.height() * 0.01),
        (1050., 70.),
        &images,
        |ui| draw_resources(ui, &settings, &map, &player, &images),
    );

    // Store whether the next panel should be shown on the right side or not
    let right_side = if let Some(id) = state.planet_hover.or(state.planet_selected) {
        let (planet, planet_pos) = planet_q
            .iter()
            .find_map(|(t, p)| {
                (p.id == id).then_some((
                    map.get(id),
                    camera.world_to_viewport(camera_t, t.translation).unwrap(),
                ))
            })
            .unwrap();

        let right_side = planet_pos.x < width * 0.5;

        if player.owns(planet) {
            let (window_w, window_h) = (330., 630.);

            draw_panel(
                &mut contexts,
                "overview",
                "panel",
                (
                    if right_side {
                        width * 0.998 - window_w
                    } else {
                        width * 0.002
                    },
                    height * 0.5 - window_h * 0.5,
                ),
                (window_w, window_h),
                &images,
                |ui| draw_overview(ui, planet, &units, &images),
            );

            !right_side
        } else if player.controls(planet) && planet.has_fleet() {
            let (window_w, window_h) = (140., 630.);

            draw_panel(
                &mut contexts,
                "planet fleet",
                "panel",
                (
                    if right_side {
                        width * 0.998 - window_w
                    } else {
                        width * 0.002
                    },
                    height * 0.5 - window_h * 0.5,
                ),
                (window_w, window_h),
                &images,
                |ui| draw_fleet(ui, planet, &units, &images),
            );

            !right_side
        } else {
            right_side
        }
    } else {
        true
    };

    if let Some(mission_id) = state.mission_hover {
        let mission = missions.get(mission_id);

        let (window_w, window_h) = (140., 630.);

        draw_panel(
            &mut contexts,
            "mission hover fleet",
            "panel",
            (
                if right_side {
                    width * 0.998 - window_w
                } else {
                    width * 0.002
                },
                height * 0.5 - window_h * 0.5,
            ),
            (window_w, window_h),
            &images,
            |ui| draw_mission_fleet_hover(ui, mission, &player, &units, &images),
        );

        let (window_w2, window_h2) = (270., 280.);

        draw_panel(
            &mut contexts,
            "mission hover info",
            "panel",
            (
                if right_side {
                    width * 0.998 - window_w - window_w2 - 1.
                } else {
                    width * 0.002 + window_w + 1.
                },
                height * 0.5 - window_h * 0.5 + 27.,
            ),
            (window_w2, window_h2),
            &images,
            |ui| draw_mission_info_hover(ui, mission, &settings, &map, &player, &images),
        );
    }

    if state.mission {
        state.end_turn = false;

        let (window_w, window_h) = (700., 540.);

        let is_hovered = contexts.ctx().unwrap().is_pointer_over_area();
        draw_panel(
            &mut contexts,
            "mission",
            "panel",
            ((width - window_w) * 0.5, (height - window_h) * 0.5),
            (window_w, window_h),
            &images,
            |ui| {
                draw_mission(
                    ui,
                    &missions.0,
                    &mut send_mission,
                    &settings,
                    &mut state,
                    &mut map,
                    &mut player,
                    &units,
                    is_hovered,
                    &images,
                )
            },
        );
    } else if let Some(id) = state.planet_selected {
        if settings.show_menu {
            state.end_turn = false;

            // Hide shop if hovering another planet
            if !state.planet_hover.is_some_and(|planet_id| planet_id != id) {
                let planet = map.get_mut(id);

                if player.owns(&planet) {
                    let (window_w, window_h) = (735., 340.);

                    draw_panel(
                        &mut contexts,
                        "shop",
                        "panel",
                        (width * 0.5 - window_w * 0.5, height * 0.995 - window_h),
                        (window_w, window_h),
                        &images,
                        |ui| {
                            draw_shop(
                                ui,
                                &mut state,
                                &settings,
                                &mut player,
                                planet,
                                &units,
                                &images,
                            )
                        },
                    );
                }
            }
        }
    }
}
