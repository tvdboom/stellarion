use crate::core::assets::WorldAssets;
use crate::core::camera::MainCamera;
use crate::core::constants::SILO_CAPACITY_FACTOR;
use crate::core::map::map::Map;
use crate::core::map::planet::PlanetId;
use crate::core::map::systems::PlanetCmp;
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::ui::aesthetics::Aesthetics;
use crate::core::ui::dark::NordDark;
use crate::core::ui::utils::CustomUi;
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Description, Unit};
use crate::utils::NameFromEnum;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::{emath, Align, Align2, Color32, FontData, FontFamily, Layout, RichText, TextStyle, TextureId, UiBuilder};
use bevy_egui::EguiContexts;
use std::cmp::min;
use std::collections::HashMap;
use strum::IntoEnumIterator;

#[derive(Component)]
pub struct UiCmp;

#[derive(Resource, Default)]
pub struct ImageIds(pub HashMap<&'static str, TextureId>);

impl ImageIds {
    pub fn get(&self, key: &str) -> TextureId {
        *self.0.get(key).expect(format!("No image found with name: {}", key).as_str())
    }
}

#[derive(Clone, Default)]
pub enum Shop {
    #[default]
    Buildings,
    Ships,
    Defenses,
}

#[derive(Resource, Default)]
pub struct UiState {
    pub hovered_planet: Option<PlanetId>,
    pub selected_planet: Option<PlanetId>,
    pub shop: Shop,
    pub end_turn: bool,
}

fn create_unit_hover(ui: &mut egui::Ui, unit: &Unit, msg: Option<&str>, images: &ImageIds) {
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            ui.add_image(images.get(unit.to_lowername().as_str()), [200., 200.]);
        });
        ui.vertical(|ui| {
            ui.label(unit.to_name());
            ui.separator();

            if let Some(msg) = msg {
                ui.colored_label(Color32::RED, RichText::new(msg).small());
            }

            ui.small(unit.description());

            ui.add_space(10.);

            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 6.;
                for resource in ResourceName::iter() {
                    let price = unit.price().get(&resource);
                    ui.add_image(images.get(resource.to_lowername().as_str()), [35., 25.]);
                    ui.label(price.to_string());
                    ui.add_space(30.);
                }
            });
        });
    });
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
        let id = contexts.add_image(v.clone_weak());
        images.0.insert(k, id);
    }
}

pub fn draw_ui(
    mut contexts: EguiContexts,
    mut planet_q: Query<(&GlobalTransform, &PlanetCmp)>,
    camera_q: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    state: Res<UiState>,
    settings: Res<Settings>,
    images: Res<ImageIds>,
    window: Single<&Window>,
) {
    let (camera, camera_t) = camera_q.into_inner();

    let all_units: [Vec<Unit>; 3] = [
        Building::iter().map(|b| Unit::Building(b)).collect(),
        Ship::iter().map(|s| Unit::Ship(s)).collect(),
        Defense::iter().map(|d| Unit::Defense(d)).collect(),
    ];

    egui::Window::new("resources")
        .frame(egui::Frame {
            fill: egui::Color32::TRANSPARENT,
            ..default()
        })
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_pos((window.width() * 0.5 - 600., window.height() * 0.04))
        .default_size((1200., 70.))
        .show(contexts.ctx_mut().unwrap(), |ui| {
            let response = ui.add(egui::Image::new(SizedTexture::new(
                images.get("thin_panel"),
                ui.available_size(),
            )));

            ui.scope_builder(UiBuilder::new().max_rect(response.rect), |ui| {
                ui.add_space(10.);

                ui.horizontal_centered(|ui| {
                    ui.add_space(150.);

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
                                ui.add_image(
                                    images.get(resource.to_lowername().as_str()),
                                    [65., 40.],
                                );
                                ui.heading(player.resources.get(&resource).to_string());
                                ui.add_space(35.);
                            })
                            .response;

                        if settings.show_hover {
                            response.on_hover_ui(|ui| {
                                ui.horizontal(|ui| {
                                    ui.vertical(|ui| {
                                        ui.add_image(
                                            images.get(resource.to_lowername().as_str()),
                                            [130., 90.],
                                        );
                                    });
                                    ui.vertical(|ui| {
                                        ui.label(resource.to_name());
                                        ui.separator();
                                        ui.small(format!(
                                            "Production: +{}",
                                            player.resource_production(&map.planets).get(&resource)
                                        ));
                                        ui.small(resource.description());
                                    });
                                });
                            });
                        }
                    }
                });
            });
        });

    if let Some(id) = state.hovered_planet.or(state.selected_planet) {
        let (planet, planet_pos) = planet_q
            .iter()
            .find_map(|(t, p)| {
                (p.id == id).then_some((
                    map.get(id),
                    camera.world_to_viewport(camera_t, t.compute_transform().translation).unwrap(),
                ))
            })
            .unwrap();

        if player.controls(planet) {
            let (width, height) = (window.width(), window.height());
            let (window_w, window_h) = (330., 630.);

            egui::Window::new("overview")
                .frame(egui::Frame {
                    fill: Color32::TRANSPARENT,
                    ..default()
                })
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .fixed_pos((
                    if planet_pos.x < width * 0.5 {
                        width * 0.998 - window_w
                    } else {
                        width * 0.01
                    },
                    height * 0.2,
                ))
                .fixed_size((window_w, window_h))
                .show(contexts.ctx_mut().unwrap(), |ui| {
                    let response = ui.add(egui::Image::new(SizedTexture::new(
                        images.get("panel"),
                        ui.available_size(),
                    )));

                    ui.scope_builder(UiBuilder::new().max_rect(response.rect), |ui| {
                        ui.add_space(15.);
                        ui.vertical_centered(|ui| {
                            ui.label("Overview");
                        });
                    });

                    ui.add_space(5.);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = emath::Vec2::new(6., 4.);

                        for units in all_units.iter() {
                            ui.add_space(30.);

                            ui.vertical(|ui| {
                                for unit in units {
                                    ui.horizontal(|ui| {
                                        ui.add_image(
                                            images.get(unit.to_lowername().as_str()),
                                            [50., 50.],
                                        );
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
                });
        }
    }

    if let Some(id) = state.selected_planet {
        let planet = map.get_mut(id);

        if player.controls(&planet) {
            let (width, height) = (window.width(), window.height());
            let (window_w, window_h) = (730., 340.);

            egui::Window::new("shop")
                .frame(egui::Frame {
                    fill: Color32::TRANSPARENT,
                    ..default()
                })
                .collapsible(false)
                .resizable(false)
                .title_bar(false)
                .fixed_pos((width * 0.5 - window_w * 0.5, height * 0.995 - window_h))
                .fixed_size((window_w, window_h))
                .show(contexts.ctx_mut().unwrap(), |ui| {
                    let response = ui.add(egui::Image::new(SizedTexture::new(
                        images.get("panel"),
                        ui.available_size(),
                    )));

                    ui.scope_builder(UiBuilder::new().max_rect(response.rect), |ui| {
                        ui.spacing_mut().item_spacing = emath::Vec2::new(4., 4.);

                        ui.add_space(4.);

                        let units = match state.shop {
                            Shop::Buildings => {
                                ui.add_space(35.);
                                &all_units[0]
                            },
                            Shop::Ships | Shop::Defenses => {
                                let (current, max) = match state.shop {
                                    Shop::Ships => {
                                        (planet.fleet_production(), planet.max_fleet_production())
                                    },
                                    Shop::Defenses => (
                                        planet.battery_production(),
                                        planet.max_battery_production(),
                                    ),
                                    _ => unreachable!(),
                                };
                                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                    ui.add_space(45.);
                                    ui.small(format!("Production: {}/{}", current, max));
                                });

                                ui.add_space(9.);

                                &all_units[match state.shop {
                                    Shop::Ships => 1,
                                    Shop::Defenses => 2,
                                    _ => unreachable!(),
                                }]
                            },
                        };

                        let (r1, r2) = units.split_at(min(5, units.len()));

                        for row in [r1, r2] {
                            ui.horizontal(|ui| {
                                ui.add_space(25.);

                                for unit in row {
                                    let resources_check = player.resources >= unit.price();
                                    let (level_check, production_check) = match unit {
                                        Unit::Building(_) => (
                                            true,
                                            !planet.buy.contains(unit)
                                                && planet.get(unit) < Building::MAX_LEVEL,
                                        ),
                                        Unit::Ship(s) => (
                                            s.level()
                                                <= planet.get(&Unit::Building(Building::Shipyard)),
                                            planet.fleet_production() + s.level()
                                                <= planet.max_fleet_production(),
                                        ),
                                        Unit::Defense(d) => (
                                            d.level()
                                                <= planet.get(&Unit::Building(Building::Factory)),
                                            planet.battery_production() + d.level()
                                                <= planet.max_battery_production() && d.is_missile().then_some(planet.missile_capacity() < planet.max_missile_capacity()).unwrap_or(true)
                                        ),
                                    };

                                    ui.add_enabled_ui(
                                        resources_check && level_check && production_check,
                                        |ui| {
                                            ui.spacing_mut().button_padding.x = 2.;

                                            let response = ui.add_image_button(
                                                images.get(unit.to_lowername().as_str()),
                                                [130., 130.],
                                            );

                                            if response.clicked() {
                                                player.resources -= unit.price();
                                                planet.buy.push(unit.clone());
                                            }

                                            let rect = response.rect;
                                            let painter = ui.painter();

                                            if matches!(unit, Unit::Building(Building::MissileSilo))
                                            {
                                                painter.text(
                                                    rect.left_bottom() + egui::vec2(7., -4.),
                                                    Align2::LEFT_BOTTOM,
                                                    format!(
                                                        "{}/{}",
                                                        planet.get(&Unit::Defense(
                                                            Defense::InterplanetaryMissile
                                                        )) + planet.get(&Unit::Defense(
                                                            Defense::AntiballisticMissile
                                                        )),
                                                        unit.level() * SILO_CAPACITY_FACTOR
                                                    ),
                                                    TextStyle::Body.resolve(ui.style()),
                                                    Color32::WHITE,
                                                );
                                            }

                                            painter.text(
                                                rect.right_bottom() + egui::vec2(-7., -4.),
                                                Align2::RIGHT_BOTTOM,
                                                planet.get(unit).to_string(),
                                                TextStyle::Heading.resolve(ui.style()),
                                                Color32::WHITE,
                                            );

                                            if settings.show_hover {
                                                response
                                                    .on_hover_ui(|ui| {
                                                        create_unit_hover(ui, unit, None, &images);
                                                    })
                                                    .on_disabled_hover_ui(|ui| {
                                                        create_unit_hover(ui, unit, Some(if !resources_check {
                                                            "Not enough resources."
                                                        } else if !level_check {
                                                            "Building level too low to produce this unit."
                                                        } else {
                                                            "Production limit reached."
                                                        }), &images);
                                                    });
                                            }
                                        },
                                    );
                                }
                            });
                        }
                    });
                });
        }
    }
}
