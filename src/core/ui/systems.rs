use crate::core::assets::WorldAssets;
use crate::core::camera::MainCamera;
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
use crate::core::units::missions::Mission;
use crate::core::units::ships::Ship;
use crate::core::units::{Description, Unit};
use crate::utils::NameFromEnum;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::{
    emath, Align, Align2, Color32, ComboBox, CursorIcon, FontData, FontFamily, Layout, RichText,
    Sense, Separator, TextStyle, TextureId, UiBuilder,
};
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

#[derive(Clone, Debug, Default)]
pub enum Shop {
    #[default]
    Buildings,
    Fleet,
    Defenses,
}

#[derive(Resource, Default)]
pub struct UiState {
    pub hovered_planet: Option<PlanetId>,
    pub selected_planet: Option<PlanetId>,
    pub to_selected: bool,
    pub shop: Shop,
    pub mission: bool,
    pub mission_info: Mission,
    pub end_turn: bool,
}

fn create_unit_hover(ui: &mut egui::Ui, unit: &Unit, msg: Option<String>, images: &ImageIds) {
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
    planet_q: Query<(&GlobalTransform, &PlanetCmp)>,
    camera_q: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut state: ResMut<UiState>,
    settings: Res<Settings>,
    images: Res<ImageIds>,
    window: Single<&Window>,
) {
    let (camera, camera_t) = camera_q.into_inner();
    let (width, height) = (window.width(), window.height());

    let all_units: [Vec<Unit>; 3] = [
        Building::iter().map(|b| Unit::Building(b)).collect(),
        Ship::iter().map(|s| Unit::Ship(s)).collect(),
        Defense::iter().map(|d| Unit::Defense(d)).collect(),
    ];

    egui::Window::new("resources")
        .frame(egui::Frame {
            fill: Color32::TRANSPARENT,
            ..default()
        })
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_pos((window.width() * 0.5 - 525., window.height() * 0.01))
        .default_size((1050., 70.))
        .show(contexts.ctx_mut().unwrap(), |ui| {
            let response = ui.add(egui::Image::new(SizedTexture::new(
                images.get("thin_panel"),
                ui.available_size(),
            )));

            ui.scope_builder(UiBuilder::new().max_rect(response.rect), |ui| {
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
            let (window_w, window_h) = (320., 630.);

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
                        ui.add_space(17.);

                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 7.;
                            ui.add_space(110.);
                            ui.add_image(images.get("overview"), [20., 20.]);
                            ui.small("Overview");
                        });
                    });

                    ui.add_space(10.);

                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

                        for units in all_units.iter() {
                            ui.add_space(20.);

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

    if state.mission {
        let (window_w, window_h) = (700., 450.);

        egui::Window::new("mission")
            .frame(egui::Frame {
                fill: Color32::TRANSPARENT,
                ..default()
            })
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .fixed_pos(((width - window_w) * 0.5, (height - window_h) * 0.5))
            .fixed_size((window_w, window_h))
            .show(contexts.ctx_mut().unwrap(), |ui| {
                let response = ui.add(egui::Image::new(SizedTexture::new(
                    images.get("panel"),
                    ui.available_size(),
                )));

                ui.scope_builder(UiBuilder::new().max_rect(response.rect), |ui| {
                    ui.add_space(50.);

                    ui.horizontal(|ui| {
                        ui.add_space(170.);

                        ui.vertical_centered(|ui| {
                            ComboBox::from_id_salt("origin")
                                .selected_text(&map.get(state.mission_info.origin).name)
                                .show_ui(ui, |ui| {
                                    for planet in
                                        map.planets.iter().filter(|p| p.owner == Some(player.id))
                                    {
                                        ui.selectable_value(
                                            &mut state.mission_info.origin,
                                            planet.id,
                                            &planet.name,
                                        );
                                    }
                                });
                        });

                        ui.add_space(20.);
                        ui.add_image(images.get("mission"), [50., 50.]);
                        ui.add_space(20.);

                        ui.vertical_centered(|ui| {
                            ComboBox::from_id_salt("destination")
                                .selected_text(&map.get(state.mission_info.destination).name)
                                .show_ui(ui, |ui| {
                                    for planet in &map.planets {
                                        ui.selectable_value(
                                            &mut state.mission_info.destination,
                                            planet.id,
                                            &planet.name,
                                        );
                                    }
                                });
                        });
                    });

                    ui.add(Separator::default().shrink(50.));

                    if state.mission_info.origin == state.mission_info.destination {
                        ui.add_space(20.);
                        ui.vertical_centered(|ui| {
                            ui.colored_label(
                                Color32::RED,
                                "The origin and destination planets must be different.",
                            );
                        });
                    } else {
                        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                            ui.add_space(10.);
                            ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                ui.add_space(50.);

                                let (rect, mut response) =
                                    ui.allocate_exact_size([180., 50.].into(), Sense::click());

                                response = response.on_hover_cursor(CursorIcon::PointingHand);

                                let image = if response.hovered()
                                    && !response.is_pointer_button_down_on()
                                {
                                    images.get("button hover")
                                } else {
                                    images.get("button")
                                };

                                ui.painter().image(
                                    image,
                                    rect,
                                    egui::Rect::from_min_max(
                                        egui::pos2(0., 0.),
                                        egui::pos2(1., 1.),
                                    ),
                                    Color32::WHITE,
                                );

                                ui.painter().text(
                                    rect.center(),
                                    Align2::CENTER_CENTER,
                                    "Send mission",
                                    TextStyle::Button.resolve(ui.style()),
                                    Color32::WHITE,
                                );

                                if response.clicked() {
                                    player.missions.push(state.mission_info.clone());
                                    state.selected_planet = None;
                                    state.mission = false;
                                }
                            });
                        });
                    }
                });
            });
    } else if let Some(id) = state.selected_planet {
        let planet = map.get_mut(id);

        if player.controls(&planet) {
            let (window_w, window_h) = (735., 340.);

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

                        let (production, idx) = match state.shop {
                            Shop::Buildings => (None, 0),
                            Shop::Fleet => (
                                Some((planet.fleet_production(), planet.max_fleet_production())),
                                1,
                            ),
                            Shop::Defenses => (
                                Some((
                                    planet.battery_production(),
                                    planet.max_battery_production(),
                                )),
                                2,
                            ),
                        };

                        ui.horizontal(|ui| {
                            ui.add_space(45.);
                            ui.add_image(
                                images.get(state.shop.to_lowername().as_str()),
                                [20., 20.],
                            );
                            ui.small(state.shop.to_name());

                            if let Some((current, max)) = production {
                                ui.with_layout(Layout::right_to_left(Align::Min), |ui| {
                                    ui.add_space(45.);
                                    ui.small(format!("Production: {}/{}", current, max));
                                });
                            }
                        });

                        ui.add_space(10.);

                        let units = &all_units[idx];
                        let (r1, r2) = units.split_at(min(5, units.len()));

                        for row in [r1, r2] {
                            ui.horizontal(|ui| {
                                ui.add_space(25.);

                                for unit in row {
                                    let count = planet.get(unit);
                                    let bought = planet.buy.iter().filter(|u| *u == unit).count();

                                    let resources_check = player.resources >= unit.price();
                                    let (level_check, building_check, production_check) = match unit
                                    {
                                        Unit::Building(_) => (
                                            true,
                                            count < Building::MAX_LEVEL,
                                            !planet.buy.contains(unit),
                                        ),
                                        Unit::Ship(s) => (
                                            s.level()
                                                <= planet.get(&Unit::Building(Building::Shipyard)),
                                            true,
                                            planet.fleet_production() + s.level()
                                                <= planet.max_fleet_production(),
                                        ),
                                        Unit::Defense(d) => (
                                            d.level()
                                                <= planet.get(&Unit::Building(Building::Factory)),
                                            true,
                                            planet.battery_production() + d.level()
                                                <= planet.max_battery_production()
                                                && d.is_missile()
                                                    .then_some(
                                                        planet.missile_capacity() + bought
                                                            < planet.max_missile_capacity(),
                                                    )
                                                    .unwrap_or(true),
                                        ),
                                    };

                                    ui.add_enabled_ui(
                                        resources_check
                                            && level_check
                                            && building_check
                                            && production_check,
                                        |ui| {
                                            ui.spacing_mut().button_padding.x = 2.;

                                            let mut response = ui.add_image_button(
                                                images.get(unit.to_lowername().as_str()),
                                                [130., 130.],
                                            );

                                            if ui.is_enabled() {
                                                response = response
                                                    .on_hover_cursor(CursorIcon::PointingHand);
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

                                            if matches!(unit, Unit::Building(Building::MissileSilo))
                                                && count > 0
                                            {
                                                painter.text(
                                                    rect.right_top() + egui::vec2(-7., 4.),
                                                    Align2::RIGHT_TOP,
                                                    format!(
                                                        "{}/{}",
                                                        planet.missile_capacity(),
                                                        planet.max_missile_capacity()
                                                    ),
                                                    TextStyle::Body.resolve(ui.style()),
                                                    Color32::WHITE,
                                                );
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
                                                    rect.left_bottom()
                                                        + egui::vec2(8. + offset_x, -12.),
                                                    Align2::LEFT_BOTTOM,
                                                    format!(" (+{})", bought),
                                                    TextStyle::Body.resolve(ui.style()),
                                                    Color32::WHITE,
                                                );
                                            }

                                            if settings.show_hover {
                                                response
                                                    .on_hover_ui(|ui| {
                                                        create_unit_hover(ui, unit, None, &images);
                                                    })
                                                    .on_disabled_hover_ui(|ui| {
                                                        create_unit_hover(
                                                            ui,
                                                            unit,
                                                            Some(if !resources_check {
                                                                "Not enough resources.".to_string()
                                                            } else if !building_check {
                                                                "Building already at maximum level."
                                                                    .to_string()
                                                            } else if !level_check {
                                                                format!(
                                                                    "Requires {} level {}.",
                                                                    match unit {
                                                                        Unit::Ship(_) =>
                                                                            Building::Shipyard
                                                                                .to_name(),
                                                                        Unit::Defense(d)
                                                                            if d.is_missile() =>
                                                                            Building::MissileSilo
                                                                                .to_name(),
                                                                        Unit::Defense(_) =>
                                                                            Building::Factory
                                                                                .to_name(),
                                                                        _ => unreachable!(),
                                                                    },
                                                                    unit.level()
                                                                )
                                                            } else {
                                                                "Production limit reached."
                                                                    .to_string()
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
                    });
                });
        }
    }
}
