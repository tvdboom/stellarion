use crate::core::assets::WorldAssets;
use crate::core::camera::MainCamera;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::player::Player;
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::ui::aesthetics::Aesthetics;
use crate::core::ui::dark::NordDark;
use crate::core::ui::utils::CustomUi;
use crate::core::units::buildings::BuildingName;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::Description;
use crate::utils::NameFromEnum;
use bevy::prelude::*;
use bevy_egui::egui;
use bevy_egui::egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::{emath, FontData, FontFamily, TextureId, UiBuilder};
use bevy_egui::EguiContexts;
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
pub enum Unit {
    #[default]
    Building,
    Fleet,
    Defense,
}

#[derive(Resource, Default)]
pub struct UiState {
    pub hovered_planet: Option<PlanetId>,
    pub selected_planet: Option<PlanetId>,
    pub unit: Unit,
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
    planet_q: Query<(&GlobalTransform, &Planet)>,
    camera_q: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    player: Res<Player>,
    state: Res<UiState>,
    settings: Res<Settings>,
    images: Res<ImageIds>,
    window: Single<&Window>,
) {
    let (camera, camera_t) = camera_q.into_inner();

    egui::Window::new("resources")
        .frame(egui::Frame {
            fill: egui::Color32::TRANSPARENT,
            ..default()
        })
        .collapsible(false)
        .resizable(false)
        .title_bar(false)
        .fixed_pos((window.width() * 0.5 - 600., window.height() * 0.04))
        .fixed_size((1200., 70.))
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
                            ui.label("Turn");
                            ui.small("Current turn in the game");
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
                                ui.label(resource.to_name());
                                ui.small(resource.description());
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
                    p,
                    camera.world_to_viewport(camera_t, t.compute_transform().translation).unwrap(),
                ))
            })
            .unwrap();

        let (width, height) = (window.width(), window.height());
        let (window_w, window_h) = (350., 630.);

        egui::Window::new("overview")
            .frame(egui::Frame {
                fill: egui::Color32::TRANSPARENT,
                ..default()
            })
            .collapsible(false)
            .resizable(false)
            .title_bar(false)
            .fixed_pos((
                if planet_pos.x < width * 0.5 {
                    width * 0.99 - window_w
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

                    ui.add_space(20.);

                    ui.vertical(|ui| {
                        for building in BuildingName::iter() {
                            ui.horizontal(|ui| {
                                ui.add_image(
                                    images.get(building.to_lowername().as_str()),
                                    [50., 50.],
                                );
                                ui.label(
                                    planet
                                        .buildings
                                        .iter()
                                        .find(|b| b.name == building)
                                        .map(|b| b.level)
                                        .unwrap_or(0)
                                        .to_string(),
                                );
                            })
                            .response
                            .on_hover_ui(|ui| {
                                ui.small(building.to_name());
                            });
                        }
                    });

                    ui.add_space(20.);

                    ui.vertical(|ui| {
                        for ship in Ship::iter() {
                            ui.horizontal(|ui| {
                                ui.add_image(images.get(ship.to_lowername().as_str()), [50., 50.]);
                                ui.label(
                                    player
                                        .fleets
                                        .get(&id)
                                        .map(|f| f.get(&ship))
                                        .unwrap_or(0)
                                        .to_string(),
                                );
                            })
                            .response
                            .on_hover_ui(|ui| {
                                ui.small(ship.to_name());
                            });
                        }
                    });

                    ui.add_space(20.);

                    ui.vertical(|ui| {
                        for defense in Defense::iter() {
                            ui.horizontal(|ui| {
                                ui.add_image(
                                    images.get(defense.to_lowername().as_str()),
                                    [50., 50.],
                                );
                                ui.label(
                                    player
                                        .defenses
                                        .get(&id)
                                        .map(|f| f.get(&defense))
                                        .unwrap_or(0)
                                        .to_string(),
                                );
                            })
                            .response
                            .on_hover_ui(|ui| {
                                ui.small(defense.to_name());
                            });
                        }
                    });
                });
            });
    }
}
