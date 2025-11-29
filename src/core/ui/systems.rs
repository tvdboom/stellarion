use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use bevy_egui::egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use bevy_egui::egui::load::SizedTexture;
use bevy_egui::egui::{
    emath, Align, Align2, Color32, ComboBox, CursorIcon, FontData, FontFamily, Layout, Order,
    Response, RichText, ScrollArea, Sense, Separator, Slider, Stroke, StrokeKind, TextStyle, Ui,
    UiBuilder,
};
use bevy_egui::{egui, EguiContexts, EguiTextureHandle};
use itertools::Itertools;
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::core::assets::WorldAssets;
use crate::core::combat::combat::CombatUnit;
use crate::core::combat::report::{MissionReport, ReportId, RoundReport, Side};
use crate::core::combat::stats::CombatStats;
use crate::core::constants::{
    ENEMY_COLOR, OWN_COLOR, PROBES_PER_PRODUCTION_LEVEL, PS_SHIELD_PER_LEVEL, SHIELD_COLOR,
};
use crate::core::map::icon::Icon;
use crate::core::map::map::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::messages::MessageMsg;
use crate::core::missions::{BombingRaid, Mission, MissionId, Missions, SendMissionMsg};
use crate::core::player::{PlanetInfo, Player};
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::ui::aesthetics::Aesthetics;
use crate::core::ui::dark::NordDark;
use crate::core::ui::utils::{toggle, CustomResponse, CustomUi, ImageIds};
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Description, Price, Unit};
use crate::utils::{format_thousands, FmtNumb, NameFromEnum, SafeDiv, ToColor32};

#[derive(Component)]
pub struct UiCmp;

#[derive(Clone, Debug, Default, PartialEq)]
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
    EnemyMissions,
    MissionReports,
}

#[derive(Resource, Default)]
pub struct UiState {
    pub planet_hover: Option<PlanetId>,
    pub planet_selected: Option<PlanetId>,
    pub to_selected: bool,
    pub shop: Shop,
    pub lab: (ResourceName, ResourceName),
    pub lab_amount: usize,
    pub phalanx_hover: Option<PlanetId>,
    pub radar_hover: Option<PlanetId>,
    pub mission: bool,
    pub mission_tab: MissionTab,
    pub mission_info: Mission,
    pub jump_gate_history: bool,
    pub mission_hover: Option<MissionId>,
    pub mission_report: Option<MissionId>,
    pub combat_report: Option<ReportId>,
    pub combat_report_total: bool,
    pub combat_report_round: usize,
    pub combat_report_hover: Option<(Unit, Side)>,
    pub in_combat: Option<ReportId>,
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
        .order(if name == "combat report" {
            Order::Foreground
        } else {
            Order::Middle
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

fn draw_army_grid(
    ui: &mut Ui,
    name: &str,
    army: &Vec<Unit>,
    report: &MissionReport,
    player: &Player,
    images: &ImageIds,
) {
    let side = if name == "attacker" {
        Side::Attacker
    } else {
        Side::Defender
    };

    egui::Grid::new(name).striped(false).num_columns(2).spacing([8., 8.]).show(ui, |ui| {
        let can_see = report.can_see(&side, player.id);

        for (i, unit) in army.iter().enumerate() {
            let (survived, total) = if side == Side::Attacker {
                (report.surviving_attacker.amount(unit), report.mission.army.amount(unit))
            } else {
                (report.surviving_defender.amount(unit), report.planet.army.amount(unit))
            };
            let lost = total - survived;

            let text = if can_see {
                if lost > 0 {
                    format!("{lost}/{total}")
                } else {
                    total.to_string()
                }
            } else if report.mission.owner == player.id
                && side == Side::Defender
                && report.scout_probes > (unit.production() - 1) * PROBES_PER_PRODUCTION_LEVEL
            {
                // Even if attacker lost combat, he can see enemy starting units with scouts
                total.to_string()
            } else {
                "?".to_string()
            };

            ui.add_enabled_ui(text != "0", |ui| {
                let response = ui
                    .add_image(images.get(unit.to_lowername()), [65., 65.])
                    .on_hover_small_ext(unit.to_name())
                    .on_disabled_hover_small_ext(unit.to_name());

                ui.add_text_on_image(
                    text,
                    if can_see && lost > 0 {
                        Color32::RED
                    } else {
                        Color32::WHITE
                    },
                    TextStyle::Body,
                    response.rect.left_bottom(),
                    Align2::LEFT_BOTTOM,
                );
            });

            if i % 2 == 1 {
                ui.end_row();
            }
        }
    });
}

fn draw_combat_army_grid(
    ui: &mut Ui,
    name: &str,
    state: &mut UiState,
    round: &RoundReport,
    units: Vec<Unit>,
    side: Side,
    color: Color,
    images: &ImageIds,
) -> bool {
    let (own, mut enemy) = match side {
        Side::Attacker => (&round.attacker, round.defender.clone()),
        Side::Defender => (&round.defender, round.attacker.clone()),
    };

    if let Some((u, s)) = &state.combat_report_hover {
        if *s != side {
            enemy = enemy.into_iter().filter(|cu| cu.unit == *u).collect::<Vec<_>>();
        }
    }

    let total_ps = round.buildings.amount(&Unit::planetary_shield()) * PS_SHIELD_PER_LEVEL;

    let n_columns = if name.contains("building") {
        1
    } else {
        2
    };

    let mut any_hovered = false;
    egui::Grid::new(name).striped(false).num_columns(n_columns).spacing([8., 25.]).show(ui, |ui| {
        for (i, (unit, count)) in units
            .into_iter()
            .filter_map(|u| {
                if u.is_building() {
                    Some((u, round.buildings.amount(&u)))
                } else {
                    let mut seen = HashSet::new();
                    let n = own.iter().filter(|cu| cu.unit == u && seen.insert(cu.id)).count();
                    (n > 0).then_some((u, n))
                }
            })
            .enumerate()
        {
            let n_repaired = own
                .iter()
                .filter_map(|cu| (cu.unit == unit).then_some(cu.n_repaired))
                .sum::<usize>();
            let shots = enemy
                .iter()
                .flat_map(|u| &u.shots)
                .filter(|s| s.unit == Some(unit))
                .collect::<Vec<_>>();
            let n_shots = shots.len();
            let lost = match unit {
                Unit::Defense(Defense::InterplanetaryMissile) => count,
                Unit::Defense(Defense::AntiballisticMissile) => round.antiballistic_fired,
                _ => shots.iter().filter(|s| s.killed).count(),
            };

            let hovering_crawler =
                matches!(state.combat_report_hover, Some((Unit::Defense(Defense::Crawler), _)));

            ui.add_enabled_ui(
                state
                    .combat_report_hover
                    .as_ref()
                    .map_or(true, |(u, s)| (*s != side || *u == Unit::crawler()) || *u == unit),
                |ui| {
                    let response = ui
                        .add_image(images.get(unit.to_lowername()), [70.; 2])
                        .on_hover_small_ext(unit.to_name());

                    if response.hovered() && !unit.is_building() {
                        any_hovered = true;
                        state.combat_report_hover = Some((unit, side.clone()));
                    }

                    let text = if hovering_crawler && side == Side::Defender {
                        if n_repaired > 0 {
                            Some(format!("❤{n_repaired}"))
                        } else {
                            None
                        }
                    } else if n_shots > 0 {
                        Some(format!("💥{n_shots}"))
                    } else {
                        None
                    };

                    if let Some(text) = text {
                        ui.add_text_on_image(
                            text,
                            Color32::WHITE,
                            TextStyle::Small,
                            response.rect.right_top() - egui::Vec2::new(2., -3.),
                            Align2::RIGHT_TOP,
                        );
                    }

                    ui.add_text_on_image(
                        if lost > 0 {
                            format!("{lost}/{count}")
                        } else {
                            count.to_string()
                        },
                        if lost > 0 {
                            Color32::RED
                        } else {
                            Color32::WHITE
                        },
                        TextStyle::Body,
                        response.rect.left_bottom(),
                        Align2::LEFT_BOTTOM,
                    );

                    let all_cu: Vec<_> = own.iter().filter(|cu| cu.unit == unit).collect();
                    let (hull, shield) = if hovering_crawler && side == Side::Defender {
                        (
                            all_cu
                                .iter()
                                .map(|cu| cu.repaired as f32)
                                .sum::<f32>()
                                .safe_div((count * unit.hull()) as f32),
                            0.,
                        )
                    } else if unit.is_building() {
                        if unit == Unit::planetary_shield() {
                            let mut ps = round.planetary_shield as f32;
                            if let Some((hu, hs)) = &state.combat_report_hover {
                                if *hs != side {
                                    ps = enemy
                                        .iter()
                                        .filter(|cu| cu.unit == *hu)
                                        .flat_map(|cu| cu.shots.iter())
                                        .filter(|s| s.unit.is_some_and(|u| u == unit))
                                        .fold(0., |s_acc, s| {
                                            s_acc + s.planetary_shield_damage as f32
                                        });
                                }
                            }

                            (f32::NAN, ps / total_ps as f32)
                        } else {
                            (f32::NAN, f32::NAN)
                        }
                    } else {
                        let mut shield = all_cu
                            .iter()
                            .map(|cu| {
                                if lost == count {
                                    0.
                                } else {
                                    cu.shield as f32
                                }
                            })
                            .sum::<f32>()
                            .safe_div((all_cu.len() * unit.shield()) as f32);

                        let mut hull = all_cu
                            .iter()
                            .fold(HashMap::<_, f32>::new(), |mut map, cu| {
                                let val = if lost == count {
                                    0.
                                } else {
                                    cu.hull as f32
                                };
                                map.entry(cu.id).and_modify(|m| *m = (*m).min(val)).or_insert(val);
                                map
                            })
                            .values()
                            .sum::<f32>()
                            .safe_div((count * unit.hull()) as f32);

                        if let Some((hu, hs)) = &state.combat_report_hover {
                            if *hs != side {
                                let (s_sum, h_sum) = enemy
                                    .iter()
                                    .filter(|cu| cu.unit == *hu)
                                    .flat_map(|cu| cu.shots.iter())
                                    .filter(|s| s.unit.is_some_and(|u| u == unit))
                                    .fold((0., 0.), |(s_acc, h_acc), s| {
                                        (
                                            s_acc + s.shield_damage as f32,
                                            h_acc + s.hull_damage as f32,
                                        )
                                    });

                                // Total shield when hover is not well-defined -> clamp to range for now
                                shield = s_sum.safe_div((count * unit.shield()) as f32).min(1.);
                                hull = h_sum.safe_div((count * unit.hull()) as f32);
                            }
                        }

                        (hull, shield)
                    };

                    for (i, (value, color)) in [shield, hull]
                        .into_iter()
                        .zip([SHIELD_COLOR.to_color32(), color.to_color32()])
                        .enumerate()
                    {
                        if !value.is_nan() {
                            let bar = egui::Rect::from_min_max(
                                egui::pos2(
                                    response.rect.left(),
                                    response.rect.bottom() + i as f32 * 10.,
                                ),
                                egui::pos2(
                                    response.rect.right(),
                                    response.rect.bottom() + (i + 1) as f32 * 10.,
                                ),
                            );

                            ui.painter().rect_filled(bar, 0., Color32::from_gray(40));

                            let filled = egui::Rect::from_min_max(
                                bar.min,
                                egui::pos2(bar.min.x + bar.width() * value, bar.max.y),
                            );

                            ui.painter().rect_filled(filled, 0., color);
                        }
                    }
                },
            );

            if n_columns == 1 || i % 2 == 1 {
                ui.end_row();
            }
        }
    });

    any_hovered
}

fn draw_resources(ui: &mut Ui, settings: &Settings, map: &Map, player: &Player, images: &ImageIds) {
    ui.add_space(10.);

    // Measure total horizontal width required
    let mut text = settings.turn.to_string();

    let (n_owned, n_max_owned) = player.planets_owned(&map, &settings);

    text += &n_owned.to_string();
    text += &n_max_owned.to_string();
    for r in ResourceName::iter() {
        text += &player.resources.get(&r).to_string();
    }

    let size_x = ui
        .painter()
        .layout_no_wrap(text, TextStyle::Heading.resolve(ui.style()), Color32::WHITE)
        .size()
        .x
        + 80.
        + 35. * 3.
        + 65. * 5.
        + ui.spacing().item_spacing.x * 12.5;

    ui.horizontal_centered(|ui| {
        ui.add_space((ui.available_width() - size_x) * 0.5);

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

        ui.add_space(35.);

        let response = ui
            .scope(|ui| {
                ui.add_image(images.get("owned"), [65., 40.]);
                ui.heading(format!("{n_owned}/{n_max_owned}"));
            })
            .response;

        if settings.show_hover {
            response.on_hover_ui(|ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.add_image(images.get("owned"), [130., 90.]);
                    });
                    ui.vertical(|ui| {
                        ui.label("Planets colonized / Max. colonizable");
                        ui.separator();
                        ui.small(
                            "The current number of planets colonized (owned) and the maximum \
                            number of planets than can be colonized this game. A spots is only \
                            if an owned planet is abandoned, conquered or destroyed.",
                        );
                    });
                });
            });
        }

        ui.add_space(80.);

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
                                        map.planets()
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

fn draw_planet_overview(
    ui: &mut Ui,
    id: PlanetId,
    map: &mut Map,
    player: &mut Player,
    settings: &Settings,
    message: &mut MessageWriter<MessageMsg>,
    images: &ImageIds,
) {
    let (n_owned, n_max_owned) = player.planets_owned(&map, &settings);

    let planet = map.get_mut(id);

    ui.add_space(19.);

    let size = ui.available_size() - egui::vec2(15., 5.);
    let (rect, _) = ui.allocate_exact_size(size, Sense::click());

    let image = egui::Image::new(SizedTexture::new(images.get(planet.kind.to_lowername()), size));
    image.paint_at(ui, rect.translate(egui::vec2(8., 0.)));

    // Now overlay elements on top
    ui.scope_builder(UiBuilder::new().max_rect(rect.shrink(5.)), |ui| {
        ui.vertical_centered(|ui| {
            ui.heading(&planet.name);
        });

        ui.add_space(10.);

        ui.with_layout(Layout::top_down(Align::RIGHT), |ui| {
            ui.spacing_mut().item_spacing.y = 6.;
            ui.small(format!(
                "🌎 Planet Kind: {}",
                if !planet.is_moon() {
                    planet.kind.to_name()
                } else {
                    "Moon".to_string()
                }
            ))
            .on_hover_small(planet.kind.description());
            ui.small(format!(
                "📐 Diameter: {}km ({:.0}%)",
                format_thousands(planet.diameter),
                planet.destroy_probability() * 100.,
            ))
            .on_hover_small(
                "Smaller planets are easier to destroy than larger ones, since it's easier \
                to reach their core with a Death Ray, the weapon used by War Suns. The percentage \
                indicates the initial probability a War Sun has of destroying this planet after a \
                combat round.",
            );
            ui.small(format!(
                "{} Temperature: {}°C to {}°C",
                planet.kind.temperature_emoji(),
                planet.temperature.0,
                planet.temperature.1
            ));
            ui.small(format!(
                "🗺 Coordinates: ({}, {})",
                planet.position.x.round(),
                planet.position.y.round()
            ))
            .on_hover_small_ext("Position of the planet relative to the system's center.");
        });
    });

    if !planet.is_moon() {
        let owned = player.owns(planet) && player.home_planet != planet.id;
        let controlled = player.controls(planet) && !player.owns(planet);

        let size = egui::vec2(40., 40.);
        let pos = rect.left_bottom() - egui::vec2(-20., size.y + 7.);
        let rect = egui::Rect::from_min_size(pos, size);

        if owned {
            ui.add_enabled_ui(planet.buy.is_empty(), |ui| {
                let mut response = ui
                    .interact(rect, ui.id(), Sense::click())
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_small_ext(
                        "Abandon this planet. The buildings on the planet remain. \
                        Defenses on the planet are destroyed.",
                    )
                    .on_disabled_hover_small_ext(
                        "A planet can't be abandoned when there are units being built.",
                    );

                if response.enabled() {
                    response = response.on_hover_cursor(CursorIcon::PointingHand);
                }

                ui.add_image_painter(images.get("abandon"), rect);

                if response.clicked() {
                    planet.abandon();

                    // Inject hidden report to show last_info that the planet is abandoned
                    if planet.controlled == None {
                        let mission = Mission::from_mission(
                            settings.turn,
                            player.id,
                            planet,
                            planet,
                            &Mission::default(),
                        );

                        player.reports.push(MissionReport {
                            id: rand::random(),
                            turn: settings.turn,
                            mission,
                            planet: planet.clone(),
                            scout_probes: 0,
                            surviving_attacker: Army::new(),
                            surviving_defender: Army::new(),
                            planet_colonized: false,
                            planet_destroyed: false,
                            destination_owned: None,
                            destination_controlled: None,
                            combat_report: None,
                            hidden: true,
                        });
                    }

                    message.write(MessageMsg::info(format!("Planet {} abandoned.", planet.name)));
                }
            });
        } else if controlled {
            ui.add_enabled_ui(
                planet.army.amount(&Unit::colony_ship()) > 0 && n_owned < n_max_owned,
                |ui| {
                    let mut response = ui
                        .interact(rect, ui.id(), Sense::click())
                        .on_hover_small_ext("Colonize this planet.")
                        .on_disabled_hover_small_ext(if n_owned >= n_max_owned {
                            "Maximum number of colonized planets reached."
                        } else {
                            "A Colony Ship is required on this planet to colonize it."
                        });

                    if response.enabled() {
                        response = response.on_hover_cursor(CursorIcon::PointingHand);
                    }

                    ui.add_image_painter(images.get("colonize"), rect);

                    if response.clicked() {
                        *planet.army.entry(Unit::colony_ship()).or_insert(1) -= 1;
                        planet.colonize(player.id);
                        message
                            .write(MessageMsg::info(format!("Planet {} colonized.", planet.name)));
                    }
                },
            );
        }
    }
}

fn draw_overview(ui: &mut Ui, planet: &Planet, images: &ImageIds) {
    ui.add_space(17.);

    ui.horizontal(|ui| {
        let text = &planet.name;
        let size_x = ui
            .painter()
            .layout_no_wrap(text.clone(), TextStyle::Small.resolve(ui.style()), Color32::WHITE)
            .size()
            .x;

        ui.spacing_mut().item_spacing.x = 7.;
        ui.add_space((ui.available_width() - size_x - 27.) * 0.5);
        ui.add_image(images.get("overview"), [20.; 2]);
        ui.small(text);
    });

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

        ui.add_space(10.);

        for units in Unit::all_valid(planet.is_moon()) {
            ui.add_space(5.);

            ui.vertical(|ui| {
                for unit in units {
                    let n = planet.army.amount(&unit);

                    ui.add_enabled_ui(n > 0, |ui| {
                        let response = ui.add_image(images.get(unit.to_lowername()), [50.; 2]);
                        ui.add_text_on_image(
                            n.to_string(),
                            Color32::WHITE,
                            TextStyle::Body,
                            response.rect.left_bottom(),
                            Align2::LEFT_BOTTOM,
                        );
                    })
                    .response
                    .on_hover_small(unit.to_name())
                    .on_disabled_hover_small(unit.to_name());
                }
            });
        }
    });
}

fn draw_report_overview(ui: &mut Ui, planet: &Planet, info: &PlanetInfo, images: &ImageIds) {
    ui.add_space(17.);

    ui.horizontal(|ui| {
        let text = format!("{} ({})", planet.name, info.turn);
        let size_x = ui
            .painter()
            .layout_no_wrap(text.clone(), TextStyle::Small.resolve(ui.style()), Color32::WHITE)
            .size()
            .x;

        ui.add_space((ui.available_width() - size_x) * 0.5);
        ui.small(text);
    })
    .response
    .on_hover_small(format!("Intelligence from turn {}.", info.turn));

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

        ui.add_space(10.);
        for units in Unit::all_valid(planet.is_moon()) {
            ui.add_space(5.);

            ui.vertical(|ui| {
                for unit in units {
                    let text = if let Some(n) = info.army.get(&unit) {
                        n.to_string()
                    } else {
                        "?".to_string()
                    };

                    ui.add_enabled_ui(text != "0", |ui| {
                        let response = ui.add_image(images.get(unit.to_lowername()), [50.; 2]);
                        ui.add_text_on_image(
                            text,
                            Color32::WHITE,
                            TextStyle::Body,
                            response.rect.left_bottom(),
                            Align2::LEFT_BOTTOM,
                        );
                    })
                    .response
                    .on_hover_small(unit.to_name())
                    .on_disabled_hover_small(unit.to_name());
                }
            });
        }
    });
}

fn draw_mission_fleet_hover(
    ui: &mut Ui,
    mission: &Mission,
    map: &Map,
    player: &Player,
    images: &ImageIds,
) {
    let army = match mission.objective {
        Icon::MissileStrike => vec![Unit::interplanetary_missile()],
        Icon::Spy => vec![Unit::probe()],
        _ => Unit::ships(),
    };

    ui.add_space(17.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.;
        ui.add_space(10.);
        ui.add_image(images.get(mission.image(player)), [25.; 2]);
        ui.small("Mission");
    });

    ui.add_space(10.);

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing = emath::Vec2::new(7., 4.);

        ui.add_space(32.);

        ui.vertical(|ui| {
            for unit in army.iter() {
                let n = mission.army.amount(unit);

                ui.add_enabled_ui(n > 0, |ui| {
                    let response = ui.add_image(images.get(unit.to_lowername()), [50.; 2]);
                    ui.add_text_on_image(
                        if mission.owner != player.id
                            && !player.spectator
                            && mission
                                .is_seen_by_phalanx(map, player)
                                .map(|lvl| unit.production() > lvl)
                                .unwrap_or(true)
                            && mission
                                .is_seen_by_radar(map, player)
                                .map(|lvl| unit.production() > lvl)
                                .unwrap_or(true)
                        {
                            "?".to_string()
                        } else {
                            n.to_string()
                        },
                        Color32::WHITE,
                        TextStyle::Body,
                        response.rect.left_bottom(),
                        Align2::LEFT_BOTTOM,
                    );
                })
                .response
                .on_hover_small(unit.to_name())
                .on_disabled_hover_small(unit.to_name());
            }
        });
    });
}

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

    let (n_owned, n_max_owned) = player.planets_owned(&map, &settings);

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
                    && (!destination.is_moon() || !state.mission_info.objective.on_planet_only())
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

                let image = if response.hovered() && !response.is_pointer_button_down_on() {
                    images.get(format!("{} hover", state.mission_info.image(player)))
                } else {
                    images.get(state.mission_info.image(player))
                };

                ui.add_image_painter(image, rect);

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
                            ui.style_mut().text_styles.get_mut(&TextStyle::Button).unwrap().size =
                                18.;

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

                    if bombers == 0 {
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
            let objective_check = match state.mission_info.objective {
                Icon::Deploy => state.mission_info.army.iter().any(|(u, c)| u.is_ship() && *c > 0),
                Icon::Colonize => state.mission_info.army.amount(&Unit::colony_ship()) > 0,
                Icon::Attack => {
                    state.mission_info.army.iter().any(|(u, n)| *n > 0 && u.is_combat_ship())
                },
                Icon::Spy => {
                    state.mission_info.army.amount(&Unit::probe()) == state.mission_info.total()
                },
                Icon::MissileStrike => {
                    state.mission_info.army.amount(&Unit::interplanetary_missile())
                        == state.mission_info.total()
                },
                Icon::Destroy => state.mission_info.army.amount(&Unit::Ship(Ship::WarSun)) > 0,
                _ => unreachable!(),
            };

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
                        state.planet_selected = None;
                        state.mission = false;
                        state.mission_info = Mission::default();
                    }
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

                                    ui.add_image(
                                        if state.mission_hover == Some(mission.id) {
                                            images.get(format!("{} hover", mission.image(player)))
                                        } else {
                                            images.get(mission.image(player))
                                        },
                                        [50.; 2],
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

fn draw_mission_reports(
    ui: &mut Ui,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    is_hovered: bool,
    images: &ImageIds,
) {
    let reports = player.reports.iter().filter(|r| !r.hidden).collect::<Vec<_>>();

    if reports.len() == 0 {
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

                            ui.add_image(
                                if state.mission_report == Some(report.mission.id)
                                    || (response.hovered() && !response.is_pointer_button_down_on())
                                {
                                    images.get(format!("{} hover", report.mission.image(player)))
                                } else {
                                    images.get(report.mission.image(player))
                                },
                                [50.; 2],
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

            let report = player
                .reports
                .iter()
                .find(|r| state.mission_report == Some(r.mission.id))
                .unwrap_or(player.reports.last().unwrap());

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

                            ui.add_image(
                                images.get(format!("{} hover", report.mission.image(player))),
                                [50.; 2],
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

                let (a_color, d_color) = if report.mission.owner == player.id {
                    (OWN_COLOR, ENEMY_COLOR)
                } else {
                    (ENEMY_COLOR, OWN_COLOR)
                };

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
                        let units = Unit::all_valid(destination.is_moon());

                        for (i, army) in [units.get(1), units.get(2), units.get(0)]
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
                        {
                            if ui.add_custom_button("Combat details", images).clicked() {
                                state.combat_report = Some(report.id);
                                state.combat_report_round = 1;
                            }
                        }
                    });
                });
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
    is_hovered: bool,
    keyboard: &ButtonInput<KeyCode>,
    images: &ImageIds,
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
        MissionTab::NewMission => draw_new_mission(
            ui,
            send_mission,
            settings,
            state,
            map,
            player,
            is_hovered,
            keyboard,
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
        MissionTab::EnemyMissions => draw_active_missions(
            ui,
            missions.iter().filter(|m| m.owner != player.id).collect(),
            state,
            map,
            player,
            is_hovered,
            images,
        ),
        MissionTab::MissionReports => {
            draw_mission_reports(ui, state, map, player, is_hovered, images)
        },
    }
}

fn draw_combat_report(
    ui: &mut Ui,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    images: &ImageIds,
) {
    let report = player.reports.iter().find(|r| r.id == state.combat_report.unwrap()).unwrap();
    let combat = report.combat_report.as_ref().unwrap();

    let origin = map.get(report.mission.origin);
    let destination = map.get(report.mission.destination);

    ui.add_space(5.);

    ui.horizontal(|ui| {
        ui.set_height(55.);
        ui.spacing_mut().item_spacing.x = 8.;

        ui.add_space(70.);

        ui.add_image(images.get(origin.image()), [35., 35.]);
        ui.add_space(5.);
        ui.small(&origin.name);

        ui.add_space(25.);

        ui.add_image(images.get(report.mission.objective.to_lowername()), [25.; 2]);
        ui.add_image(images.get(report.mission.image(player)), [50.; 2]);
        ui.small(report.turn.to_string());

        ui.add_space(25.);

        ui.small(&destination.name);
        ui.add_space(5.);
        let resp = ui.add_image(images.get(destination.image()), [35., 35.]);

        let size = [15., 15.];
        let pos = resp.rect.right_top() - egui::vec2(size[0], 0.);
        ui.put(
            egui::Rect::from_min_size(pos, size.into()),
            egui::Image::new(SizedTexture::new(images.get(report.image(player)), size)),
        );

        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(70.);

            ui.add_enabled_ui(!state.combat_report_total, |ui| {
                ui.add(
                    Slider::new(&mut state.combat_report_round, 1..=combat.rounds.len())
                        .step_by(1f64)
                        .show_value(false),
                )
                .on_hover_cursor(CursorIcon::PointingHand)
                .on_hover_small("Combat round for which to show the details.");

                ui.add_space(10.);

                ui.small(format!("Round: {}/{}", state.combat_report_round, combat.rounds.len()));
            });

            ui.add_space(30.);

            ui.add(toggle(&mut state.combat_report_total)).on_hover_small(
                "If enabled, the panel shows the total statistics over the whole combat \
                (sum over all rounds). If disabled, it shows the statistics per round.",
            );

            ui.add_space(10.);

            ui.small("Total:")
        });
    });

    let round = if state.combat_report_total {
        let mut rr = combat.rounds.iter().fold(RoundReport::default(), |mut rr, r| {
            rr.attacker.extend(r.attacker.clone());
            rr.defender.extend(r.defender.clone());
            rr.planetary_shield += r.planetary_shield;
            rr.antiballistic_fired += r.antiballistic_fired;
            if rr.buildings.is_empty() {
                rr.buildings = r.buildings.clone()
            }
            rr
        });

        rr.destroy_probability =
            1. - combat.rounds.iter().fold(1., |acc, p| acc * (1. - p.destroy_probability));
        rr
    } else {
        combat.rounds.get(state.combat_report_round - 1).unwrap().clone()
    };

    let draw_stats = |ui: &mut Ui, units: Vec<&CombatUnit>, side: Side| {
        let shots = units.iter().flat_map(|u| &u.shots).collect::<Vec<_>>();

        let shield_damage = shots.iter().map(|a| a.shield_damage).sum::<usize>();
        let hull_damage = shots.iter().map(|a| a.hull_damage).sum::<usize>();
        let ps_damage = shots.iter().map(|a| a.planetary_shield_damage).sum::<usize>();

        let u_shots = shots
            .iter()
            .filter(|s| matches!(s.unit, Some(u) if !u.is_building()))
            .collect::<Vec<_>>();
        let m_shots = shots
            .iter()
            .filter(|s| s.unit == Some(Unit::interplanetary_missile()))
            .collect::<Vec<_>>();
        let b_shots = shots
            .iter()
            .filter(
                |s| matches!(s.unit, Some(u) if u.is_building() && u != Unit::planetary_shield()),
            )
            .collect::<Vec<_>>();
        let shots_missed = u_shots.iter().filter(|s| s.missed).count();
        let total_repaired = units.iter().map(|cu| cu.repaired).sum::<usize>();
        let missiles_hit = m_shots.iter().filter(|s| s.killed).count();
        let bombs_hit = b_shots.iter().filter(|s| s.killed).count();

        let rapid_fire = shots.iter().filter(|a| a.rapid_fire).count();
        let enemies_killed = shots.iter().filter(|a| a.killed).count();

        let draw_row = |ui: &mut Ui, icon: &str, val: String, hover: &str| {
            ui.vertical_centered(|ui| {
                ui.label(icon).on_hover_small(hover);
            });
            ui.label(if units.is_empty() {
                "--".to_string()
            } else {
                val
            })
            .on_hover_small(hover);
            ui.end_row();
        };

        egui::Grid::new("stats_grid").striped(false).num_columns(2).spacing([2., 6.]).show(
            ui,
            |ui| {
                draw_row(ui, "🛡", shield_damage.fmt(), "Damage dealt to shields.");
                draw_row(ui, "🔰", hull_damage.fmt(), "Damage dealt to hulls.");
                if side == Side::Attacker {
                    draw_row(ui, "🌐", ps_damage.fmt(), "Damage dealt to the planetary shield.");
                }
                draw_row(
                    ui,
                    "⚔",
                    (shield_damage + hull_damage + ps_damage).fmt(),
                    "Total damage dealt.",
                );
                if side == Side::Defender {
                    draw_row(
                        ui,
                        "❤",
                        total_repaired.to_string(),
                        "Total hull points repaired by Crawlers.",
                    );
                }
                draw_row(
                    ui,
                    "❌",
                    format!("{:.0}%", (shots_missed as f32).safe_div(u_shots.len() as f32) * 100.),
                    "Percentage of shots that missed a target. A shot misses when it \
                    fires on a unit that was already destroyed that round.",
                );
                draw_row(
                    ui,
                    "🔥",
                    format!("{:.0}%", (rapid_fire as f32).safe_div(u_shots.len() as f32) * 100.),
                    "Percentage of shots that gained rapid fire.",
                );
                if report.mission.objective == Icon::MissileStrike && side == Side::Defender {
                    draw_row(
                        ui,
                        "🚀",
                        format!(
                            "{:.0}%",
                            (missiles_hit as f32).safe_div(m_shots.len() as f32) * 100.
                        ),
                        "Percentage of Antiballistic Missiles that intercepted an \
                        incoming Interplanetary Missile.",
                    );
                }
                if report.mission.bombing != BombingRaid::None && side == Side::Attacker {
                    draw_row(
                        ui,
                        "💣",
                        format!("{:.0}%", (bombs_hit as f32).safe_div(b_shots.len() as f32) * 100.),
                        "Percentage of bombs that hit enemy buildings.",
                    );
                }
                draw_row(ui, "💀", enemies_killed.fmt(), "Number of enemy units destroyed.");
                if report.mission.objective == Icon::Destroy && side == Side::Attacker {
                    draw_row(
                        ui,
                        "☠",
                        format!("{:.0}%", round.destroy_probability * 100.),
                        "Probability of successfully destroying the planet.",
                    );
                }
            },
        );
    };

    let mut any_hovered = false;

    let (attacker_w, defender_w) = (ui.available_width() * 0.3, ui.available_width() * 0.6);

    let (attack_c, defend_c) = if report.mission.owner == player.id {
        (OWN_COLOR, ENEMY_COLOR)
    } else {
        (ENEMY_COLOR, OWN_COLOR)
    };

    ui.horizontal(|ui| {
        ui.add_space(40.);

        ui.visuals_mut().widgets.noninteractive.bg_stroke.width = 6.;

        ui.vertical(|ui| {
            ui.set_width(attacker_w);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.label("Attacker");
            });
            ui.visuals_mut().widgets.noninteractive.bg_stroke.color = attack_c.to_color32();
            ui.separator();
        });
        ui.vertical(|ui| {
            ui.set_width(defender_w);
            ui.label("Defender");
            ui.visuals_mut().widgets.noninteractive.bg_stroke.color = defend_c.to_color32();
            ui.separator();
        });
    });

    ui.horizontal(|ui| {
        ui.add_space(40.);

        ui.vertical(|ui| {
            ui.set_width(attacker_w);

            ui.add_space(10.);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(135.);

                    let units = round
                        .attacker
                        .iter()
                        .filter(|cu| {
                            state
                                .combat_report_hover
                                .as_ref()
                                .map_or(true, |(u, s)| *u == cu.unit && *s == Side::Attacker)
                        })
                        .collect::<Vec<_>>();

                    draw_stats(ui, units, Side::Attacker);
                });

                ui.vertical(|ui| {
                    ui.set_width(ui.available_width() - 12.);

                    let hovered = draw_combat_army_grid(
                        ui,
                        "combat_attacker",
                        state,
                        &round,
                        if report.mission.objective == Icon::MissileStrike {
                            vec![Unit::interplanetary_missile()]
                        } else {
                            Unit::ships()
                        },
                        Side::Attacker,
                        attack_c,
                        images,
                    );
                    any_hovered = any_hovered || hovered;
                });

                ui.set_height(470.);
                ui.separator();
            });
        });

        ui.vertical(|ui| {
            ui.set_width(defender_w);

            ui.add_space(10.);

            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(520.);

                    if round.defender.is_empty() {
                        ui.label("No defending units.");
                    } else {
                        ui.horizontal_top(|ui| {
                            let hovered1 = if round.defender.iter().any(|cu| cu.unit.is_ship()) {
                                draw_combat_army_grid(
                                    ui,
                                    "combat_defender1",
                                    state,
                                    &round,
                                    Unit::ships(),
                                    Side::Defender,
                                    defend_c,
                                    images,
                                )
                            } else {
                                false
                            };

                            let hovered2 = if round.defender.iter().any(|cu| cu.unit.is_defense()) {
                                draw_combat_army_grid(
                                    ui,
                                    "combat_defender2",
                                    state,
                                    &round,
                                    Unit::defenses(),
                                    Side::Defender,
                                    defend_c,
                                    images,
                                )
                            } else {
                                false
                            };

                            any_hovered = any_hovered || hovered1 || hovered2;

                            if report.planet.army.amount(&Unit::planetary_shield()) > 0 {
                                draw_combat_army_grid(
                                    ui,
                                    "combat_buildings1",
                                    state,
                                    &round,
                                    vec![Unit::planetary_shield()],
                                    Side::Defender,
                                    defend_c,
                                    images,
                                );
                            }

                            let units = match report.mission.bombing {
                                BombingRaid::Economic
                                    if report
                                        .planet
                                        .army
                                        .iter()
                                        .any(|(u, c)| u.is_resource_building() && *c > 0) =>
                                {
                                    Unit::resource_buildings()
                                },
                                BombingRaid::Industrial
                                    if report
                                        .planet
                                        .army
                                        .iter()
                                        .any(|(u, c)| u.is_industrial_building() && *c > 0) =>
                                {
                                    Unit::industrial_buildings()
                                },
                                _ => vec![],
                            };

                            if !units.is_empty() {
                                draw_combat_army_grid(
                                    ui,
                                    "combat_buildings2",
                                    state,
                                    &round,
                                    units,
                                    Side::Defender,
                                    defend_c,
                                    images,
                                );
                            }
                        });
                    }
                });

                ui.horizontal_top(|ui| {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        ui.vertical(|ui| {
                            let units = round
                                .defender
                                .iter()
                                .filter(|cu| {
                                    state.combat_report_hover.as_ref().map_or(true, |(u, s)| {
                                        *u == cu.unit && *s == Side::Defender
                                    })
                                })
                                .collect::<Vec<_>>();

                            draw_stats(ui, units, Side::Defender);
                        });
                    });
                });
            });
        });
    });

    if !any_hovered {
        state.combat_report_hover = None;
    }

    ui.with_layout(Layout::bottom_up(Align::Max), |ui| {
        ui.add_space(50.);
        ui.horizontal(|ui| {
            ui.add_space(40.);
            if ui.add_custom_button("Close details", images).clicked() {
                state.combat_report = None;
            }

            ui.add_space(310.);

            ui.small("Hover over a unit to show the statistics for that unit only.");
        });
    });
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
        ui.add_image(images.get(origin.image()), [25.; 2]);
        ui.small(origin.name.to_name());
    });

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.small("Destination:");

        ui.spacing_mut().item_spacing.x = 4.;
        ui.add_image(images.get(destination.image()), [25.; 2]);
        ui.small(destination.name.to_name());
    });

    ui.add(Separator::default().shrink(20.));

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.small("🎯 Objective:");

        ui.spacing_mut().item_spacing.x = 4.;
        let objective = if mission.owner == player.id {
            mission.objective
        } else {
            Icon::Attacked
        };
        ui.add_image(images.get(objective.to_lowername()), [20.; 2]);
        ui.small(objective.to_name());
    });

    ui.add(Separator::default().shrink(20.));

    ui.horizontal(|ui| {
        ui.add_space(25.);
        ui.vertical(|ui| {
            ui.small(format!("📏 Distance: {:.1} AU", mission.distance(map)));

            let speed = mission.speed();
            ui.small(format!(
                "🚀 Speed: {}",
                if speed == f32::MAX {
                    "---".to_string()
                } else {
                    format!("{speed} AU/turn")
                }
            ));

            let duration = mission.duration(map);
            ui.small(format!(
                "⏱ Duration: +{} turn{} ({})",
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
    count: usize,
    state: &mut UiState,
    player: &mut Player,
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
                                        ui.label(unit.get_stat(&stat))
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
                    *to = ResourceName::iter().find(|r| r != from).unwrap();
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
                            .range(0..=player.resources.get(&from)),
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

                    if response.clicked() {
                        *player.resources.get_mut(from) -= state.lab_amount;
                        *player.resources.get_mut(to) += gain;
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

fn draw_shop(
    ui: &mut Ui,
    state: &mut UiState,
    settings: &Settings,
    player: &mut Player,
    planet: &mut Planet,
    images: &ImageIds,
) {
    ui.spacing_mut().item_spacing = emath::Vec2::new(4., 4.);

    ui.add_space(4.);

    if planet.is_moon() && state.shop == Shop::Defenses {
        state.shop = Shop::default();
    }

    let (current, max, idx) = match state.shop {
        Shop::Buildings => (planet.fields_consumed(), planet.max_fields(), 0),
        Shop::Fleet => (planet.fleet_production(), planet.max_fleet_production(), 1),
        Shop::Defenses => (planet.battery_production(), planet.max_battery_production(), 2),
    };

    ui.horizontal(|ui| {
        ui.add_space(45.);
        ui.add_image(images.get(state.shop.to_lowername()), [20., 20.]);
        ui.small(state.shop.to_name());

        if state.shop != Shop::Buildings || planet.is_moon() {
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

    ui.add_space(10.);

    for row in Unit::all_valid(planet.is_moon())[idx].chunks(5) {
        ui.horizontal(|ui| {
            ui.add_space(25.);

            for unit in row {
                let count = planet.army.amount(unit);
                let bought = planet.buy.iter().filter(|u| *u == unit).count();

                let resources_check = player.resources >= unit.price();
                let (level_check, building_check, production_check) = match unit {
                    Unit::Building(_) => (
                        true,
                        count < Building::MAX_LEVEL,
                        !planet.buy.contains(unit)
                            && (!planet.is_moon()
                                || !unit.consumes_field()
                                || planet.fields_consumed() < planet.max_fields()),
                    ),
                    Unit::Ship(s) => (
                        s.production() <= planet.army.amount(&Unit::Building(Building::Shipyard)),
                        true,
                        planet.fleet_production() + s.production() <= planet.max_fleet_production(),
                    ),
                    Unit::Defense(d) if d.is_missile() => (
                        d.production()
                            <= planet.army.amount(&Unit::Building(Building::MissileSilo)),
                        true,
                        planet.battery_production() + d.production()
                            <= planet.max_battery_production()
                            && planet.missile_capacity() + bought < planet.max_missile_capacity(),
                    ),
                    Unit::Defense(d) => (
                        d.production() <= planet.army.amount(&Unit::Building(Building::Factory)),
                        true,
                        planet.battery_production() + d.production()
                            <= planet.max_battery_production()
                            && (*d != Defense::SpaceDock || count == 0),
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

                        // Check whether it's hovered (independent of enabled state)
                        let hovered = ui.input(|i| {
                            i.pointer
                                .hover_pos()
                                .map(|pos| response.rect.contains(pos))
                                .unwrap_or(false)
                        });

                        if *unit == Unit::Building(Building::SensorPhalanx) {
                            state.phalanx_hover = hovered.then_some(planet.id);
                        } else if *unit == Unit::Building(Building::OrbitalRadar) {
                            state.radar_hover = hovered.then_some(planet.id);
                        }

                        if response.clicked() {
                            player.resources -= unit.price();
                            planet.buy.push(unit.clone());
                        }

                        if !unit.is_building()
                            && *unit != Unit::space_dock()
                            && response.secondary_clicked()
                        {
                            // Buy 5 new units (or maximum possible)
                            let n = match unit {
                                Unit::Ship(s) => {
                                    let max_n_p = (planet.max_fleet_production()
                                        - planet.fleet_production())
                                        / s.production();
                                    let max_n_r = (player.resources / s.price()).min();
                                    5.min(max_n_p).min(max_n_r)
                                },
                                Unit::Defense(d) => {
                                    let max_n_p = (planet.max_battery_production()
                                        - planet.battery_production())
                                        / d.production();
                                    let max_n_r = (player.resources / d.price()).min();
                                    let max_n_m = if d.is_missile() {
                                        planet.max_missile_capacity() - planet.missile_capacity()
                                    } else {
                                        5
                                    };
                                    5.min(max_n_p).min(max_n_r).min(max_n_m)
                                },
                                _ => unreachable!(),
                            };

                            player.resources -= unit.price() * n;
                            planet.buy.extend(vec![unit.clone(); n]);
                        }

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
                                    draw_unit_hover(ui, unit, count, state, player, None, &images);
                                })
                                .on_disabled_hover_ui(|ui| {
                                    draw_unit_hover(
                                        ui,
                                        unit,
                                        count,
                                        state,
                                        player,
                                        Some(if !resources_check {
                                            "Not enough resources.".to_string()
                                        } else if !building_check {
                                            "Building at maximum level.".to_string()
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

fn draw_combat_selection(
    ui: &mut Ui,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    settings: &Settings,
    next_game_state: &mut NextState<GameState>,
    images: &ImageIds,
) {
    let reports = player
        .reports
        .iter()
        .filter(|r| {
            r.turn == settings.turn
                && !r.hidden
                && r.combat_report.is_some()
                && r.can_see(&Side::Defender, player.id)
        })
        .collect::<Vec<_>>();

    ui.add_space(5.);

    ui.vertical_centered(|ui| ui.label("Select a battle"));

    ui.vertical_centered(|ui| {
        ui.add_space(5.);

        ScrollArea::vertical().show(ui, |ui| {
            ui.set_width(ui.available_width() - 30.);

            ui.spacing_mut().item_spacing.y = 5.;

            for report in reports.iter().rev() {
                let destination = map.get(report.mission.destination);

                let (rect, mut response) =
                    ui.allocate_exact_size([ui.available_width(), 50.].into(), Sense::click());

                ui.scope_builder(UiBuilder::new().max_rect(rect), |ui| {
                    ui.horizontal_centered(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.;

                        let text = format!("Battle of {}", destination.name);
                        let size_x = ui
                            .painter()
                            .layout_no_wrap(
                                text.clone(),
                                TextStyle::Small.resolve(ui.style()),
                                Color32::WHITE,
                            )
                            .size()
                            .x
                            + 150.;

                        ui.add_space((ui.available_width() - size_x) * 0.5);

                        ui.small(text);

                        ui.add_space(20.);

                        ui.add_image(images.get(report.mission.objective.to_lowername()), [25.; 2]);

                        ui.add_image(
                            if state.mission_report == Some(report.mission.id)
                                || (response.hovered() && !response.is_pointer_button_down_on())
                            {
                                images.get(format!("{} hover", report.mission.image(player)))
                            } else {
                                images.get(report.mission.image(player))
                            },
                            [50.; 2],
                        );

                        ui.add_image(images.get(destination.image()), [40.; 2]);
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
                    state.in_combat = Some(report.id);
                    next_game_state.set(GameState::Combat);
                }
            }
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
        let id = contexts.add_image(EguiTextureHandle::Strong(v.clone()));
        images.0.insert(k, id);
    }
}

pub fn draw_ui(
    mut contexts: EguiContexts,
    mut send_mission: MessageWriter<SendMissionMsg>,
    mut message: MessageWriter<MessageMsg>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    missions: Res<Missions>,
    mut state: ResMut<UiState>,
    settings: Res<Settings>,
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    images: Res<ImageIds>,
    window: Single<&Window>,
) {
    let (width, height) = (window.width(), window.height());

    if matches!(game_state.get(), GameState::Playing | GameState::GameMenu) {
        draw_panel(
            &mut contexts,
            "resources",
            "thin panel",
            (window.width() * 0.5 - 625., window.height() * 0.01),
            (1250., 70.),
            &images,
            |ui| draw_resources(ui, &settings, &map, &player, &images),
        );
    }

    // Store whether the next panel should be shown on the right side or not
    let right_side = if let Some(id) = state.planet_hover.or(state.planet_selected) {
        let right_side = state.planet_selected.is_some()
            || window.cursor_position().map(|pos| pos.x < width * 0.5).unwrap_or_default();

        let planet = map.get(id);

        let (window_w, window_h) = if planet.is_moon() {
            (145., 630.)
        } else {
            (205., 630.)
        };

        let mut draw_planet_info = |contexts, id, map, player, extension| {
            let (window_w2, window_h2) = (518., 216.);

            draw_panel(
                contexts,
                "planet overview",
                "panel",
                (
                    if right_side {
                        width * 0.998
                            - window_w2
                            - if extension {
                                window_w
                            } else {
                                0.
                            }
                    } else {
                        width * 0.002
                            + if extension {
                                window_w
                            } else {
                                0.
                            }
                    },
                    height * 0.5 - window_h * 0.5 + 27.,
                ),
                (window_w2, window_h2),
                &images,
                |ui| draw_planet_overview(ui, id, map, player, &settings, &mut message, &images),
            );
        };

        // Check whether there is a report on this planet
        let info = player.last_info(planet, &missions.0);

        if player.controls(planet) || player.spectator {
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
                |ui| draw_overview(ui, planet, &images),
            );

            draw_planet_info(&mut contexts, id, &mut map, &mut player, true);
            !right_side
        } else if let Some(info) = info {
            // Don't use has_army since no units is also valid information
            if !planet.is_destroyed && !info.army.is_empty() {
                draw_panel(
                    &mut contexts,
                    "report overview",
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
                    |ui| draw_report_overview(ui, planet, &info, &images),
                );

                draw_planet_info(&mut contexts, id, &mut map, &mut player, true);
                !right_side
            } else if !planet.is_destroyed {
                draw_planet_info(&mut contexts, id, &mut map, &mut player, false);
                !right_side
            } else {
                right_side
            }
        } else if !planet.is_destroyed {
            draw_planet_info(&mut contexts, id, &mut map, &mut player, false);
            !right_side
        } else {
            right_side
        }
    } else {
        true
    };

    if let Some(mission_id) = state.mission_hover {
        let mission = missions.get(mission_id);

        let (window_w, window_h) = (110., 630.);

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
            |ui| draw_mission_fleet_hover(ui, mission, &map, &player, &images),
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

        let (window_w, window_h) = (850., 640.);

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
                    is_hovered,
                    &keyboard,
                    &images,
                )
            },
        );
    } else if let Some(id) = state.planet_selected {
        if settings.show_menu && !player.spectator {
            state.end_turn = false;

            // Hide shop if hovering another planet
            if !state.planet_hover.is_some_and(|planet_id| planet_id != id) {
                let planet = map.get_mut(id);

                if player.owns(&planet) || (planet.is_moon() && player.controls(&planet)) {
                    let (window_w, window_h) = (735., 340.);

                    draw_panel(
                        &mut contexts,
                        "shop",
                        "panel",
                        (width * 0.5 - window_w * 0.5, height * 0.995 - window_h),
                        (window_w, window_h),
                        &images,
                        |ui| draw_shop(ui, &mut state, &settings, &mut player, planet, &images),
                    );
                }
            }
        }
    }

    if state.combat_report.is_some() {
        state.end_turn = false;

        let (window_w, window_h) = (1070., 700.);

        draw_panel(
            &mut contexts,
            "combat report",
            "panel",
            (width * 0.5 - window_w * 0.5, height * 0.9 - window_h),
            (window_w, window_h),
            &images,
            |ui| draw_combat_report(ui, &mut state, &map, &player, &images),
        );
    }

    if *game_state.get() == GameState::CombatMenu {
        let (window_w, window_h) = (380., 420.);

        draw_panel(
            &mut contexts,
            "combat list",
            "panel",
            ((width - window_w) * 0.5, (height - window_h) * 0.5),
            (window_w, window_h),
            &images,
            |ui| {
                draw_combat_selection(
                    ui,
                    &mut state,
                    &map,
                    &player,
                    &settings,
                    &mut next_game_state,
                    &images,
                )
            },
        );
    }
}
