//! Egui systems for the strategic HUD, shops, missions, and combat reports.

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
use crate::core::audio::{set_ui_sound, SoundEffect};
use crate::core::combat::report::{MissionReport, ReportId, RoundReport, Side};
use crate::core::combat::resolution::CombatUnit;
use crate::core::combat::stats::CombatStats;
use crate::core::constants::{
    BG2_COLOR, ENEMY_COLOR, OWN_COLOR, PROBES_PER_PRODUCTION_LEVEL, PS_SHIELD_PER_LEVEL,
    SHIELD_COLOR,
};
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::messages::MessageMsg;
use crate::core::missions::{BombingRaid, Mission, MissionId, Missions, SendMissionMsg};
use crate::core::orders::{purchase_limit, validate_mission};
use crate::core::player::{PlanetInfo, Player};
use crate::core::resources::ResourceName;
use crate::core::settings::Settings;
use crate::core::simulation::TurnCommand;
use crate::core::states::GameState;
use crate::core::ui::aesthetics::Aesthetics;
use crate::core::ui::dark::NordDark;
use crate::core::ui::utils::{toggle, CustomResponse, CustomUi, ImageIds};
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Description, Price, Unit};
use crate::multiplayer::client::{MultiplayerSession, PendingTurnCommands};
use crate::utils::{format_thousands, FmtNumb, NameFromEnum, SafeDiv, ToColor32};

mod missions;
use missions::draw_mission;
mod shop;
use shop::draw_shop;

#[derive(Component)]
/// Marker for entities owned by the in-game UI projection.
pub struct UiCmp;

#[derive(Clone, Debug, Default, PartialEq)]
/// Selected constructible-unit category in the local shop panel.
pub enum Shop {
    #[default]
    Buildings,
    Fleet,
    Defenses,
}

#[derive(EnumIter, Copy, Clone, Debug, Default, PartialEq)]
/// Selected section of the local mission panel.
pub enum MissionTab {
    #[default]
    NewMission,
    ActiveMissions,
    EnemyMissions,
    MissionReports,
}

#[derive(Resource, Default)]
/// Local-only panel, selection, hover, and report navigation state.
pub struct UiState {
    pub planet_hover: Option<PlanetId>,
    pub planet_selected: Option<PlanetId>,
    pub to_selected: bool,
    pub shop: Shop,
    pub lab: (ResourceName, ResourceName),
    pub lab_amount: usize,
    pub mission: bool,
    pub mission_tab: MissionTab,
    pub mission_info: Mission,
    pub jump_gate_history: bool,
    pub mission_hover: Option<MissionId>,
    /// UI hover expires each pass; map hover persists until a picking event changes it.
    pub(crate) mission_hover_from_ui: bool,
    pub mission_report: Option<MissionId>,
    pub combat_report: Option<ReportId>,
    pub combat_report_total: bool,
    pub combat_report_round: usize,
    pub combat_report_hover: Option<(Unit, Side)>,
    pub in_combat: Option<ReportId>,
    pub combat_round: usize,
    pub end_turn: bool,
}

/// Draws the panel interface and emits any resulting local actions.
fn draw_panel<R>(
    contexts: &mut EguiContexts,
    name: &str,
    image: &str,
    pos: (f32, f32),
    size: (f32, f32),
    images: &ImageIds,
    content: impl FnOnce(&mut Ui) -> R,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
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
        .show(context, |ui| {
            let response =
                ui.add(egui::Image::new(SizedTexture::new(images.get(image), ui.available_size())));

            ui.scope_builder(UiBuilder::new().max_rect(response.rect), content);
        });
}

/// Shows every opposing player's name beside the color used by their map cells.
fn draw_enemy_players_widget(
    context: &egui::Context,
    session: &MultiplayerSession,
    local_player: &Player,
) {
    let Some(game) = &session.active_game else {
        return;
    };
    let enemies = game
        .members
        .iter()
        .filter(|member| member.player_id != local_player.id)
        .collect::<Vec<_>>();
    if enemies.is_empty() {
        return;
    }

    egui::Area::new("stellarion_enemy_players".into())
        .fixed_pos(egui::pos2(18.0, 92.0))
        .movable(false)
        .constrain(true)
        .order(Order::Middle)
        .show(context, |ui| {
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(10, 16, 23, 218))
                .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(130, 170, 215, 95)))
                .corner_radius(6.0)
                .inner_margin(egui::Margin::symmetric(12, 9))
                .show(ui, |ui| {
                    let max_width = (context.content_rect().width() - 62.0).max(0.0);
                    ui.set_width(220.0_f32.min(max_width));
                    ui.spacing_mut().item_spacing = egui::vec2(6.0, 4.0);
                    ui.label(
                        RichText::new("ENEMY PLAYERS")
                            .size(11.0)
                            .strong()
                            .color(Color32::from_rgb(166, 188, 211)),
                    );
                    ui.add_space(5.0);
                    for member in enemies {
                        ui.horizontal(|ui| {
                            let color = session.player_color(member.player_id);
                            let [red, green, blue] = color.rgb();
                            let (rect, _) = ui
                                .allocate_exact_size(egui::vec2(18.0, 22.0), egui::Sense::hover());
                            ui.painter().circle_filled(
                                rect.center(),
                                6.0,
                                Color32::from_rgba_unmultiplied(
                                    red,
                                    green,
                                    blue,
                                    if member.connected {
                                        255
                                    } else {
                                        110
                                    },
                                ),
                            );
                            let status = (!member.connected).then(|| {
                                egui::WidgetText::from(
                                    RichText::new("DISCONNECTED")
                                        .size(9.0)
                                        .strong()
                                        .color(Color32::from_rgb(255, 112, 112)),
                                )
                                .into_galley(
                                    ui,
                                    Some(egui::TextWrapMode::Extend),
                                    f32::INFINITY,
                                    TextStyle::Body,
                                )
                            });
                            // Reserve the status width before truncating a long player name.
                            let status_width = status.as_ref().map_or(0.0, |galley| {
                                galley.size().x + 12.0 + 2.0 * ui.spacing().item_spacing.x
                            });
                            let name = egui::WidgetText::from(
                                RichText::new(&member.display_name).size(14.0).strong().color(
                                    if member.connected {
                                        ui.visuals().text_color()
                                    } else {
                                        Color32::from_rgb(174, 181, 190)
                                    },
                                ),
                            )
                            .into_galley(
                                ui,
                                Some(egui::TextWrapMode::Truncate),
                                (ui.available_width() - status_width).max(0.0),
                                TextStyle::Body,
                            );
                            ui.add(egui::Label::new(name));
                            if let Some(status) = status {
                                draw_disconnected_icon(ui);
                                ui.add(egui::Label::new(status));
                            }
                        });
                    }
                });
        });
}

/// Draws a small slashed Wi-Fi symbol without depending on a font's icon coverage.
fn draw_disconnected_icon(ui: &mut Ui) {
    let (rect, _) = ui.allocate_exact_size(egui::vec2(12.0, 14.0), Sense::hover());
    let center = rect.center();
    let color = Color32::from_rgb(255, 112, 112);
    let stroke = Stroke::new(1.2, color);
    for (width, top, bottom) in [(5.0, -5.0, -1.0), (3.0, -2.0, 1.0)] {
        ui.painter().add(egui::epaint::QuadraticBezierShape::from_points_stroke(
            [
                center + egui::vec2(-width, bottom),
                center + egui::vec2(0.0, top),
                center + egui::vec2(width, bottom),
            ],
            false,
            Color32::TRANSPARENT,
            stroke,
        ));
    }
    ui.painter().circle_filled(center + egui::vec2(0.0, 3.0), 1.0, color);
    ui.painter()
        .line_segment([center - egui::vec2(5.0, 5.0), center + egui::vec2(5.0, 5.0)], stroke);
}

/// Draws the army grid interface and emits any resulting local actions.
fn draw_army_grid(
    ui: &mut Ui,
    name: &str,
    army: &[Unit],
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

/// Draws the combat army grid interface and emits any resulting local actions.
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
                .filter_map(|cu| (cu.unit == unit).then_some(cu.repairs.len()))
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
                    .is_none_or(|(u, s)| (*s != side || *u == Unit::crawler()) || *u == unit),
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
                                .map(|cu| cu.repairs.iter().sum::<usize>() as f32)
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

                            ui.painter().rect_filled(bar, 0., BG2_COLOR.to_color32());

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

/// Draws the resources interface and emits any resulting local actions.
fn draw_resources(ui: &mut Ui, settings: &Settings, map: &Map, player: &Player, images: &ImageIds) {
    ui.add_space(10.);

    // Measure total horizontal width required
    let mut text = settings.turn.to_string();

    let (n_owned, n_max_owned) = player.planets_owned(map, settings);

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

/// Draws the planet overview interface and emits any resulting local actions.
fn draw_planet_overview(
    ui: &mut Ui,
    id: PlanetId,
    map: &mut Map,
    player: &mut Player,
    settings: &Settings,
    message: &mut MessageWriter<MessageMsg>,
    pending: &mut PendingTurnCommands,
    images: &ImageIds,
) {
    let (n_owned, n_max_owned) = player.planets_owned(map, settings);

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
        let owned = pending.is_editable() && player.owns(planet) && player.home_planet != planet.id;
        let controlled = pending.is_editable() && player.controls(planet) && !player.owns(planet);

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
                    let mission = Mission::from_mission(
                        settings.turn,
                        player.id,
                        planet,
                        planet,
                        &Mission::default(),
                    );

                    if !pending.push(TurnCommand::AbandonPlanet {
                        planet_id: planet.id,
                    }) {
                        message.write(MessageMsg::error(
                            "This turn already contains the maximum number of commands.",
                        ));
                        return;
                    }
                    planet.abandon();

                    // Inject hidden report to show last_info that the planet is abandoned
                    if planet.controlled.is_none() {
                        player.push_report(MissionReport {
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
                        if !pending.push(TurnCommand::ColonizePlanet {
                            planet_id: planet.id,
                        }) {
                            message.write(MessageMsg::error(
                                "This turn already contains the maximum number of commands.",
                            ));
                            return;
                        }
                        let colony_ships = planet.army.entry(Unit::colony_ship()).or_insert(1);
                        *colony_ships = colony_ships.saturating_sub(1);
                        planet.colonize(player.id);
                        // The map presentation announces the ownership change once, including
                        // direct colonization and colonies established by arriving missions.
                    }
                },
            );
        }
    }
}

/// Draws the overview interface and emits any resulting local actions.
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

/// Draws the report overview interface and emits any resulting local actions.
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

/// Draws the mission fleet hover interface and emits any resulting local actions.
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

/// Draws the combat report interface and emits any resulting local actions.
fn draw_combat_report(
    ui: &mut Ui,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    images: &ImageIds,
) {
    let Some(report_id) = state.combat_report else {
        return;
    };
    let Some((report, combat)) = player
        .reports
        .iter()
        .find(|report| report.id == report_id)
        .and_then(|report| report.combat_report.as_ref().map(|combat| (report, combat)))
        .filter(|(_, combat)| !combat.rounds.is_empty())
    else {
        state.combat_report = None;
        return;
    };
    state.combat_report_round = state.combat_report_round.clamp(1, combat.rounds.len());

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
            rr.planetary_shield = rr.planetary_shield.saturating_add(r.planetary_shield);
            rr.antiballistic_fired = rr.antiballistic_fired.saturating_add(r.antiballistic_fired);
            if rr.buildings.is_empty() {
                rr.buildings = r.buildings.clone()
            }
            rr
        });

        rr.destroy_probability =
            1. - combat.rounds.iter().fold(1., |acc, p| acc * (1. - p.destroy_probability));
        rr
    } else {
        combat.rounds[state.combat_report_round - 1].clone()
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
        let total_repaired = units.iter().map(|cu| cu.repairs.iter().sum::<usize>()).sum::<usize>();
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
                                .is_none_or(|(u, s)| *u == cu.unit && *s == Side::Attacker)
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
                            let hovered1 = if report.mission.objective != Icon::MissileStrike {
                                if round.defender.iter().any(|cu| cu.unit.is_ship()) {
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
                                }
                            } else {
                                false
                            };

                            let defenses: Vec<Unit> = round
                                .defender
                                .iter()
                                .filter_map(|cu| {
                                    (cu.unit.is_defense()
                                        && (report.mission.objective != Icon::MissileStrike
                                            || cu.unit != Unit::space_dock()))
                                    .then_some(cu.unit)
                                })
                                .collect();

                            let hovered2 = if !defenses.is_empty() {
                                draw_combat_army_grid(
                                    ui,
                                    "combat_defender2",
                                    state,
                                    &round,
                                    Unit::defenses()
                                        .into_iter()
                                        .filter(|u| defenses.contains(u))
                                        .collect(),
                                    Side::Defender,
                                    defend_c,
                                    images,
                                )
                            } else {
                                false
                            };

                            any_hovered = any_hovered || hovered1 || hovered2;

                            if report.planet.army.amount(&Unit::planetary_shield()) > 0
                                && report.mission.objective != Icon::MissileStrike
                            {
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
                                        .any(|(u, c)| u.is_economic_building() && *c > 0) =>
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
                                    state
                                        .combat_report_hover
                                        .as_ref()
                                        .is_none_or(|(u, s)| *u == cu.unit && *s == Side::Defender)
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

/// Draws the mission info hover interface and emits any resulting local actions.
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

/// Draws the combat selection interface and emits any resulting local actions.
fn draw_combat_selection(
    ui: &mut Ui,
    state: &mut UiState,
    map: &Map,
    player: &Player,
    session: &MultiplayerSession,
    settings: &mut Settings,
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

    ui.vertical_centered(|ui| {
        ui.label("Select a battle");
        ui.small(
            RichText::new("Choose a battle to view its combat.")
                .color(Color32::from_rgb(151, 167, 184)),
        );
    });

    ui.vertical_centered(|ui| {
        ui.add_space(10.);

        ScrollArea::vertical().id_salt("combat selection").show(ui, |ui| {
            ui.set_width((ui.available_width() - 24.).max(0.));

            ui.spacing_mut().item_spacing.y = 8.;

            for report in reports.iter().rev() {
                let destination = map.get(report.mission.destination);

                let (rect, response) =
                    ui.allocate_exact_size([ui.available_width(), 64.].into(), Sense::click());
                let response = response
                    .on_hover_cursor(CursorIcon::PointingHand)
                    .on_hover_text("View this combat");
                let hovered = response.hovered();
                let pressed = response.is_pointer_button_down_on();
                let player_color = session.player_color(report.mission.owner).color().to_color32();

                let fill = if pressed {
                    Color32::from_rgba_unmultiplied(34, 61, 84, 248)
                } else if hovered {
                    Color32::from_rgba_unmultiplied(24, 42, 58, 246)
                } else {
                    Color32::from_rgba_unmultiplied(13, 22, 32, 238)
                };
                let border = if hovered {
                    Color32::from_rgba_unmultiplied(103, 196, 238, 190)
                } else {
                    Color32::from_rgba_unmultiplied(132, 177, 213, 88)
                };

                ui.painter().rect(
                    rect,
                    egui::CornerRadius::same(7),
                    fill,
                    Stroke::new(1., border),
                    StrokeKind::Inside,
                );
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        rect.min + egui::vec2(1., 12.),
                        egui::pos2(rect.left() + 4., rect.bottom() - 12.),
                    ),
                    2.,
                    player_color,
                );

                let center_y = rect.center().y;
                let planet_rect = egui::Rect::from_center_size(
                    egui::pos2(rect.right() - 33., center_y),
                    egui::vec2(42., 42.),
                );
                let fleet_rect = egui::Rect::from_center_size(
                    egui::pos2(planet_rect.left() - 28., center_y),
                    egui::vec2(36., 36.),
                );
                let objective_rect = egui::Rect::from_center_size(
                    egui::pos2(fleet_rect.left() - 22., center_y),
                    egui::vec2(24., 24.),
                );
                let text_rect = egui::Rect::from_min_max(
                    egui::pos2(rect.left() + 17., rect.top() + 8.),
                    egui::pos2(objective_rect.left() - 10., rect.bottom() - 8.),
                );
                let uv = egui::Rect::from_min_max(egui::Pos2::ZERO, egui::Pos2::new(1., 1.));

                ui.painter().image(
                    images.get(report.mission.objective.to_lowername()),
                    objective_rect,
                    uv,
                    Color32::WHITE,
                );
                ui.painter().image(
                    images.get(report.mission.image(player)),
                    fleet_rect,
                    uv,
                    player_color,
                );
                ui.painter().image(
                    images.get(destination.image()),
                    planet_rect,
                    uv,
                    Color32::WHITE,
                );

                let text_painter = ui.painter().with_clip_rect(text_rect);
                text_painter.text(
                    egui::pos2(text_rect.left(), center_y),
                    Align2::LEFT_CENTER,
                    format!("Battle of {}", destination.name),
                    TextStyle::Body.resolve(ui.style()),
                    Color32::WHITE,
                );

                if response.clicked() {
                    state.in_combat = Some(report.id);
                    settings.combat_paused = false;
                    next_game_state.set(GameState::Combat);
                }
            }
        });
    });
}

/// Installs the original Fira/Nord theme once the primary egui context exists.
pub fn set_ui_style(mut contexts: EguiContexts, mut initialized: Local<bool>) {
    if *initialized {
        return;
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    context.set_global_style(NordDark.custom_style());
    context.add_font(FontInsert::new(
        "firasans",
        FontData::from_static(include_bytes!("../../../assets/fonts/FiraSans-Bold.ttf")),
        vec![InsertFontFamily {
            family: FontFamily::Proportional,
            priority: FontPriority::Highest,
        }],
    ));
    *initialized = true;
}

/// Adds ui images to the current UI or asset registry.
pub fn add_ui_images(
    mut contexts: EguiContexts,
    mut images: ResMut<ImageIds>,
    assets: Res<WorldAssets>,
) {
    for (k, v) in assets.images.iter() {
        let v = assets.ui_images.get(k).unwrap_or(v);
        let id = contexts.add_image(EguiTextureHandle::Strong(v.clone()));
        images.0.insert(k.clone(), id);
    }
}

/// Draws the ui interface and emits any resulting local actions.
pub fn draw_ui(
    mut contexts: EguiContexts,
    mut send_mission: MessageWriter<SendMissionMsg>,
    mut message: MessageWriter<MessageMsg>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    missions: Res<Missions>,
    mut state: ResMut<UiState>,
    mut settings: ResMut<Settings>,
    mut pending: ResMut<PendingTurnCommands>,
    session: Res<MultiplayerSession>,
    game_state: Res<State<GameState>>,
    mut next_game_state: ResMut<NextState<GameState>>,
    keyboard: Res<ButtonInput<KeyCode>>,
    images: Res<ImageIds>,
    window: Single<&Window>,
) {
    if game_state.get().is_modal_menu() {
        return;
    }

    let (width, height) = (window.width(), window.height());

    if *game_state.get() == GameState::Playing {
        if let Ok(context) = contexts.ctx_mut() {
            draw_enemy_players_widget(context, &session, &player);
        }
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
                |ui| {
                    draw_planet_overview(
                        ui,
                        id,
                        map,
                        player,
                        &settings,
                        &mut message,
                        &mut pending,
                        &images,
                    )
                },
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
        let Some(mission) = missions.get(mission_id) else {
            state.mission_hover = None;
            return;
        };

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

    // Keep the previous hover for drawing, but require the mission list to renew it.
    let mission_hover_from_ui = std::mem::take(&mut state.mission_hover_from_ui);

    if state.mission {
        let (window_w, window_h) = (850., 640.);

        let is_hovered = contexts.ctx_mut().is_ok_and(|ctx| ctx.is_pointer_over_egui());
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
                    pending.is_editable(),
                )
            },
        );
    } else if let Some(id) = state.planet_selected {
        if settings.show_menu && !player.spectator {
            // Hide shop if hovering another planet
            if state.planet_hover.is_none_or(|planet_id| planet_id == id) {
                let planet = map.get_mut(id);

                if player.owns(planet) || (planet.is_moon() && player.controls(planet)) {
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
                                &mut pending,
                                &images,
                            )
                        },
                    );
                }
            }
        }
    }

    if mission_hover_from_ui && !state.mission_hover_from_ui {
        state.mission_hover = None;
    }

    if state.combat_report.is_some() {
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
                    &session,
                    &mut settings,
                    &mut next_game_state,
                    &images,
                )
            },
        );
    }
}
