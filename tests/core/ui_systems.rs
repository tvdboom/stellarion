use super::*;

#[test]
fn combat_details_hide_the_underlying_mission_panel_until_closed() {
    let mut state = UiState {
        mission: true,
        mission_tab: MissionTab::MissionReports,
        mission_report: Some(42),
        combat_report: Some(7),
        ..default()
    };

    assert!(!mission_panel_visible(&state));
    assert!(state.mission, "the report remains available behind the details view");

    state.combat_report = None;
    assert!(mission_panel_visible(&state));
}

#[test]
fn combat_defender_heading_uses_each_participants_game_color() {
    let context = egui::Context::default();
    let controller_color = Color32::from_rgb(88, 112, 255);
    let protector_color = Color32::from_rgb(42, 214, 156);
    let participants = [
        ("Practice P1".to_string(), controller_color),
        ("Practice P3".to_string(), protector_color),
    ];
    let mut output = context.run_ui(Default::default(), |ui| {
        draw_colored_combat_heading(ui, "Defender", controller_color, &participants);
    });
    output.textures_delta.clear();

    assert_eq!(text_color(&output.shapes, "Defender · "), controller_color);
    assert_eq!(text_color(&output.shapes, "Practice P1"), controller_color);
    assert_eq!(text_color(&output.shapes, "Practice P3"), protector_color);
    assert_eq!(text_color(&output.shapes, " + "), controller_color);
}

fn click_text(context: &egui::Context, text: &str, mut draw: impl FnMut(&mut egui::Ui)) {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(600.0, 180.0));
    let mut frame = |events| {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                events,
                ..default()
            },
            |context| {
                egui::CentralPanel::default().show(context, |ui| draw(ui));
            },
        );
        output.textures_delta.clear();
        output
    };

    frame(Vec::new());
    let output = frame(Vec::new());
    let position = text_rect(&output.shapes, text).center();
    frame(vec![
        egui::Event::PointerMoved(position),
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: true,
            modifiers: default(),
        },
    ]);
    frame(vec![egui::Event::PointerButton {
        pos: position,
        button: egui::PointerButton::Primary,
        pressed: false,
        modifiers: default(),
    }]);
}

#[test]
fn temperature_tooltips_explain_climate_and_stellar_zone_energy() {
    let mut planet = Planet::new(1, "Climate world".into(), Vec2::ZERO, false, 1.0);

    for (kind, band, climate, energy) in [
        (PlanetKind::Dry, SolarBand::Inner, "High temperatures", 3),
        (PlanetKind::Water, SolarBand::Temperate, "Moderate temperatures", 2),
        (PlanetKind::Ice, SolarBand::Outer, "Frigid temperatures", 1),
    ] {
        planet.kind = kind;
        let tooltip = planet_temperature_tooltip(&planet, Some(band));
        assert!(tooltip.starts_with(climate), "{tooltip}");
        assert!(!tooltip.contains('\n'));
        assert!(
            tooltip.ends_with(&format!("Solar Satellites produce {energy} Energy per level here."))
        );
    }

    planet.kind = PlanetKind::Gray;
    planet.temperature = (-240, -80);
    let tooltip = planet_temperature_tooltip(&planet, None);
    assert!(tooltip.starts_with("Frigid temperatures"));
    assert!(!tooltip.contains("Solar Satellites"));
    assert_eq!(tooltip, "Frigid temperatures persist because this moon has almost no atmosphere.");
}

#[test]
fn planetary_shield_label_toggles_overload_but_not_cooldown() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut planet = Planet::new(1, "Shield world".into(), Vec2::ZERO, false, 1.0);
    planet.army.insert(Unit::planetary_shield(), 5);
    let mut pending = PendingTurnCommands::default();

    let mut output = context.run_ui(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            shop::draw_planetary_shield_overload(ui, &mut planet, &mut pending, 5);
        });
    });
    output.textures_delta.clear();
    assert!(has_text(&output.shapes, "Overload shield:"));

    click_text(&context, "Overload shield:", |ui| {
        shop::draw_planetary_shield_overload(ui, &mut planet, &mut pending, 5);
    });
    assert!(planet.shield_overload.is_overloaded());
    assert!(matches!(
        pending.commands.as_slice(),
        [TurnCommand::SetPlanetaryShieldOverload {
            planet_id: 1,
            active: true,
        }]
    ));

    planet.shield_overload = crate::core::map::planet::ShieldOverloadState::Cooldown;
    pending.commands.clear();
    let mut output = context.run_ui(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            shop::draw_planetary_shield_overload(ui, &mut planet, &mut pending, 5);
        });
    });
    output.textures_delta.clear();
    assert!(has_text(&output.shapes, "Shield cooling down:"));
    click_text(&context, "Shield cooling down:", |ui| {
        shop::draw_planetary_shield_overload(ui, &mut planet, &mut pending, 5);
    });
    assert_eq!(planet.shield_overload, crate::core::map::planet::ShieldOverloadState::Cooldown);
    assert!(pending.commands.is_empty());
}

#[test]
fn command_relay_label_toggles_the_relay_without_changing_its_text() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut planet = Planet::new(1, "Relay world".into(), Vec2::ZERO, false, 1.0);
    let mut pending = PendingTurnCommands::default();

    click_text(&context, "Relay active:", |ui| {
        shop::draw_command_relay_toggle(ui, &mut planet, &mut pending);
    });

    assert!(!planet.command_relay_active);
    assert!(matches!(
        pending.commands.as_slice(),
        [TurnCommand::SetCommandRelay {
            planet_id: 1,
            active: false,
        }]
    ));
}

#[test]
fn colonial_withdrawal_selector_fits_small_panels_at_every_level() {
    for width in [280.0, 480.0] {
        for level in [1, 5] {
            let context = egui::Context::default();
            context.set_global_style(NordDark.custom_style());
            let images = ImageIds(HashMap::from([
                ("no focus".into(), egui::TextureId::User(1)),
                ("withdrawal 75".into(), egui::TextureId::User(2)),
                ("withdrawal 50".into(), egui::TextureId::User(3)),
                ("withdrawal 25".into(), egui::TextureId::User(4)),
                ("withdrawal immediate".into(), egui::TextureId::User(5)),
            ]));
            let mut planet = Planet::new(1, "Colony".into(), Vec2::ZERO, false, 1.0);
            planet.army.insert(Unit::Building(Building::ColonialAdministration), level);
            let mut pending = PendingTurnCommands::default();
            let mut bounds = egui::Rect::NOTHING;
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 220.0),
                    )),
                    ..default()
                },
                |context| {
                    egui::CentralPanel::default().show(context, |ui| {
                        bounds = ui
                            .scope(|ui| {
                                shop::draw_fleet_withdrawal(ui, &mut planet, &mut pending, &images);
                            })
                            .response
                            .rect;
                    });
                },
            );
            output.textures_delta.clear();
            assert!(
                bounds.right() <= width,
                "withdrawal selector overflowed its {width}-wide panel: {bounds:?}"
            );
            assert!(has_text(&output.shapes, "Fleet withdrawal"));
            assert!(!["Off", "75%", "50%", "25%", "Immediate"]
                .iter()
                .any(|label| has_text(&output.shapes, label)));
            for (texture, minimum_level) in [(1, 0), (2, 1), (3, 2), (4, 3), (5, 4)] {
                assert_eq!(
                    image_rect(&output.shapes, egui::TextureId::User(texture)).is_some(),
                    level >= minimum_level,
                    "withdrawal image {texture} had the wrong level availability"
                );
            }
            if width >= 480.0 {
                assert!(
                    bounds.height() < 85.0,
                    "level {level} withdrawal buttons wrapped in a {width}-wide panel: {bounds:?}"
                );
                let baseline =
                    image_rect(&output.shapes, egui::TextureId::User(1)).unwrap().center().y;
                for texture in 2..=5 {
                    if let Some(rect) = image_rect(&output.shapes, egui::TextureId::User(texture)) {
                        assert!(
                            (rect.center().y - baseline).abs() < 1.0,
                            "withdrawal image {texture} was not aligned with the off image"
                        );
                    }
                }
            }
            assert!(pending.commands.is_empty());
        }
    }
}

#[test]
fn colonial_withdrawal_image_tiles_explain_each_stance_on_hover() {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 220.0));
    for (texture, tooltip) in [
        (1, "Turn off withdrawal."),
        (2, "Withdraw after losing 75% of fleet strength."),
        (3, "Withdraw after losing 50% of fleet strength."),
        (4, "Withdraw after losing 25% of fleet strength."),
        (5, "Immediate withdrawal."),
    ] {
        let context = egui::Context::default();
        let mut style = NordDark.custom_style();
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
        context.set_global_style(style);
        let images = ImageIds(HashMap::from([
            ("no focus".into(), egui::TextureId::User(1)),
            ("withdrawal 75".into(), egui::TextureId::User(2)),
            ("withdrawal 50".into(), egui::TextureId::User(3)),
            ("withdrawal 25".into(), egui::TextureId::User(4)),
            ("withdrawal immediate".into(), egui::TextureId::User(5)),
        ]));
        let mut planet = Planet::new(1, "Colony".into(), Vec2::ZERO, false, 1.0);
        planet.army.insert(Unit::Building(Building::ColonialAdministration), 5);
        let mut pending = PendingTurnCommands::default();
        let input = |events| egui::RawInput {
            screen_rect: Some(viewport),
            events,
            ..default()
        };

        let mut warmup = context.run_ui(input(Vec::new()), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                shop::draw_fleet_withdrawal(ui, &mut planet, &mut pending, &images);
            });
        });
        warmup.textures_delta.clear();
        let target = image_rect(&warmup.shapes, egui::TextureId::User(texture)).unwrap();

        let mut hover_start =
            context.run_ui(input(vec![egui::Event::PointerMoved(target.center())]), |context| {
                egui::CentralPanel::default().show(context, |ui| {
                    shop::draw_fleet_withdrawal(ui, &mut planet, &mut pending, &images);
                });
            });
        hover_start.textures_delta.clear();
        let mut output = context.run_ui(input(Vec::new()), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                shop::draw_fleet_withdrawal(ui, &mut planet, &mut pending, &images);
            });
        });
        output.textures_delta.clear();

        assert!(has_text(&output.shapes, tooltip), "missing tooltip for texture {texture}");
    }
}

#[test]
fn shop_navigation_places_orbitals_between_buildings_and_fleet_and_skips_them_on_moons() {
    assert_eq!(Shop::Buildings.next(false), Shop::Orbitals);
    assert_eq!(Shop::Orbitals.next(false), Shop::Fleet);
    assert_eq!(Shop::Fleet.previous(false), Shop::Orbitals);
    assert_eq!(Shop::Orbitals.previous(false), Shop::Buildings);
    assert_eq!(Icon::Orbitals.shop(), Some(Shop::Orbitals));
    assert_eq!(Shop::Buildings.next(true), Shop::Fleet);
    assert_eq!(Shop::Fleet.previous(true), Shop::Buildings);
}

#[test]
fn moon_shop_shows_fields_only_for_buildings() {
    let moon = Planet::new(1, "Moon".into(), Vec2::ZERO, true, 1.0);

    assert_eq!(
        shop::shop_capacity_summary(Shop::Buildings, &moon),
        Some(("Fields", moon.fields_consumed(), moon.max_fields()))
    );
    assert_eq!(shop::shop_capacity_summary(Shop::Fleet, &moon), None);
}

#[test]
fn planet_unit_hover_panel_has_room_for_all_four_categories() {
    assert_eq!(Unit::all_for_world(false, true).len(), 4);
    assert_eq!(Unit::all_for_world(false, false).len(), 4);
    const {
        assert!(PLANET_UNITS_PANEL_WIDTH >= 270.0);
        assert!(PLANET_UNITS_PANEL_WIDTH > MOON_UNITS_PANEL_WIDTH);
    }
}

#[test]
fn planet_buildings_follow_the_gameplay_order_in_the_shop_and_planet_hover() {
    let common = vec![
        Unit::Building(Building::MetalMine),
        Unit::Building(Building::CrystalMine),
        Unit::Building(Building::DeuteriumSynthesizer),
        Unit::Building(Building::Reactor),
        Unit::Building(Building::Terraformer),
        Unit::Building(Building::Shipyard),
        Unit::Building(Building::Factory),
        Unit::Building(Building::MissileSilo),
        Unit::Building(Building::PlanetaryShield),
    ];
    let mut home = common.clone();
    home.push(Unit::Building(Building::Senate));
    let mut colony = common;
    colony.push(Unit::Building(Building::ColonialAdministration));

    assert_eq!(Unit::buildings_for_world(false, true), home);
    assert_eq!(Unit::buildings_for_world(false, false), colony);
    assert_eq!(Unit::all_for_world(false, true)[0], home);
    assert_eq!(Unit::all_for_world(false, false)[0], colony);
    assert_eq!(Unit::buildings_for_world(false, true).len(), 10);
    assert_eq!(Unit::buildings_for_world(false, false).len(), 10);
    assert_eq!(
        Unit::buildings_for_world(true, false),
        vec![
            Unit::Building(Building::LunarBase),
            Unit::Building(Building::TidalGenerator),
            Unit::Building(Building::Shipyard),
            Unit::Building(Building::Laboratory),
            Unit::Building(Building::OrbitalRadar),
        ]
    );
}

#[test]
fn colonial_withdrawal_controls_require_a_completed_first_level() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut planet = Planet::new(1, "Colony".into(), Vec2::ZERO, false, 1.0);
    planet.buy.push(Unit::Building(Building::ColonialAdministration));
    let mut pending = PendingTurnCommands::default();
    let images = ImageIds::default();
    let mut output = context.run_ui(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            shop::draw_fleet_withdrawal(ui, &mut planet, &mut pending, &images);
        });
    });
    output.textures_delta.clear();

    assert!(!has_text(&output.shapes, "Fleet withdrawal:"));
    assert!(pending.commands.is_empty());
}

#[test]
fn complete_building_catalog_keeps_both_world_specific_government_buildings() {
    let expected = vec![
        Unit::Building(Building::MetalMine),
        Unit::Building(Building::CrystalMine),
        Unit::Building(Building::DeuteriumSynthesizer),
        Unit::Building(Building::Reactor),
        Unit::Building(Building::Terraformer),
        Unit::Building(Building::Shipyard),
        Unit::Building(Building::Factory),
        Unit::Building(Building::MissileSilo),
        Unit::Building(Building::PlanetaryShield),
        Unit::Building(Building::Senate),
        Unit::Building(Building::ColonialAdministration),
    ];
    let catalog =
        Unit::buildings().into_iter().filter(|unit| unit.valid_on(false)).collect::<Vec<_>>();

    assert_eq!(catalog, expected);
    assert_eq!(Unit::all_valid(false)[0], expected);
}
use crate::core::units::ships::Ship;

#[test]
fn laboratory_conversion_uses_a_sounding_information_toast_with_the_gained_resource() {
    let notification = shop::conversion_success_message(12_345, ResourceName::Metal);

    assert_eq!(notification.message, "Gained 12.345 Metal.");
    assert_eq!(notification.level, crate::core::messages::MessageLevel::Info);
    assert!(!notification.silent);
}

#[test]
fn combat_selection_uses_prebattle_planet_artwork() {
    let mut destination = Planet::new(0, "Cindra".into(), Vec2::ZERO, false, 1.0);
    let original_image = destination.image();
    let report = MissionReport {
        id: 1,
        turn: 1,
        mission: Mission {
            destination: destination.id,
            ..default()
        },
        planet: destination.clone(),
        scout_probes: 0,
        surviving_attacker: Army::new(),
        surviving_defender: Army::new().into(),
        planet_colonized: false,
        planet_destroyed: true,
        destination_owned: None,
        destination_controlled: None,
        combat_report: None,
        hidden: false,
    };
    destination.destroy();

    assert_eq!(combat_selection_planet_image(&report), original_image);
    assert_ne!(combat_selection_planet_image(&report), destination.image());
}

#[test]
fn combat_details_put_the_space_dock_above_the_planetary_shield() {
    let mut planet = Planet::new(0, "Darian".into(), Vec2::ZERO, false, 1.0);
    planet.army.insert(Unit::space_dock(), 1);
    planet.army.insert(Unit::planetary_shield(), 5);
    let report = MissionReport {
        id: 1,
        turn: 1,
        mission: Mission {
            destination: planet.id,
            objective: Icon::Attack,
            ..default()
        },
        planet,
        scout_probes: 0,
        surviving_attacker: Army::new(),
        surviving_defender: Army::new().into(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: None,
        destination_controlled: None,
        combat_report: None,
        hidden: false,
    };
    let round = RoundReport {
        defender: vec![CombatUnit {
            id: 7,
            owner: None,
            unit: Unit::space_dock(),
            hull: Unit::space_dock().hull(),
            shield: Unit::space_dock().shield(),
            repairs: vec![],
            shots: vec![],
        }],
        buildings: Army::from([(Unit::planetary_shield(), 5)]),
        ..default()
    };

    assert_eq!(
        combat_defender_structure_column(&report, &round),
        vec![Unit::space_dock(), Unit::planetary_shield()]
    );
}

#[test]
fn crawler_salvage_summary_shows_each_recovered_resource_only_to_the_defender() {
    let context = egui::Context::default();
    let mut planet = Planet::new(1, "Salvage".into(), Vec2::ZERO, false, 1.0);
    planet.owned = Some(2);
    planet.controlled = Some(2);
    planet.army = Army::from([
        (Unit::crawler(), 15),
        (Unit::Defense(Defense::RocketLauncher), 10),
        (Unit::Defense(Defense::PlasmaTurret), 2),
    ])
    .into();
    let report = MissionReport {
        id: 1,
        turn: 1,
        mission: Mission {
            owner: 1,
            destination: planet.id,
            objective: Icon::Attack,
            ..default()
        },
        planet,
        scout_probes: 0,
        surviving_attacker: Army::new(),
        surviving_defender: Army::from([
            (Unit::crawler(), 5),
            (Unit::Defense(Defense::RocketLauncher), 4),
            (Unit::Defense(Defense::PlasmaTurret), 1),
        ])
        .into(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: Some(2),
        destination_controlled: Some(2),
        combat_report: Some(Default::default()),
        hidden: false,
    };
    let images = ImageIds(HashMap::from([
        ("metal".to_string(), egui::TextureId::User(1)),
        ("crystal".to_string(), egui::TextureId::User(2)),
        ("deuterium".to_string(), egui::TextureId::User(3)),
    ]));
    assert_eq!(
        CRAWLER_SALVAGE_RESOURCE_ORDER,
        [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium]
    );
    assert_eq!(CRAWLER_SALVAGE_ICON_SIZE, [48.0, 30.0]);
    const { assert!(CRAWLER_SALVAGE_RESOURCE_GAP > CRAWLER_SALVAGE_ICON_VALUE_GAP) };

    let input = || egui::RawInput {
        screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 100.0))),
        ..default()
    };
    let mut body_font_size = 0.0;
    let mut defender_output = context.run_ui(input(), |ui| {
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 50.0),
            Layout::left_to_right(Align::Center),
            |ui| {
                body_font_size = TextStyle::Body.resolve(ui.style()).size;
                assert!(draw_crawler_salvage(ui, &report, &Player::new(2, 0), &images).is_some());
            },
        );
    });
    defender_output.textures_delta.clear();
    assert!(has_text(&defender_output.shapes, "Recovered:"));
    assert_eq!(text_font_size(&defender_output.shapes, "Recovered:"), body_font_size);
    for (resource, amount) in
        [(ResourceName::Metal, "29"), (ResourceName::Crystal, "6"), (ResourceName::Deuterium, "5")]
    {
        assert!(has_text(&defender_output.shapes, amount));
        assert_eq!(text_font_size(&defender_output.shapes, amount), body_font_size);
        assert!(!has_text(&defender_output.shapes, &resource.to_lowername()));
    }
    let label_center_y = text_rect(&defender_output.shapes, "Recovered:").center().y;
    let amount_center_y = text_rect(&defender_output.shapes, "29").center().y;
    assert!(
        (label_center_y - amount_center_y).abs() <= 1.0,
        "the recovery label and resource amounts should be vertically centered together: \
         label={label_center_y}, amount={amount_center_y}"
    );
    let mut attacker_output = context.run_ui(input(), |ui| {
        assert!(draw_crawler_salvage(ui, &report, &Player::new(1, 0), &images).is_none());
    });
    attacker_output.textures_delta.clear();
    assert!(!has_text(&attacker_output.shapes, "Recovered:"));
}

#[test]
fn crawler_salvage_is_shown_only_for_totals_or_the_last_round() {
    assert!(combat_report_shows_salvage(true, 1, 6));
    assert!(combat_report_shows_salvage(false, 6, 6));
    for round in 1..6 {
        assert!(!combat_report_shows_salvage(false, round, 6));
    }
}

#[test]
fn world_shortcuts_follow_acquisition_order_with_home_first() {
    let model = crate::core::simulation::GameModel::new([12; 32], Default::default()).unwrap();
    let mut player = model.players[0].clone();
    let mut worlds = model
        .map
        .planets
        .iter()
        .filter(|planet| planet.id != player.home_planet)
        .take(3)
        .cloned()
        .collect::<Vec<_>>();
    worlds[0].name = "Zulu".into();
    worlds[1].name = "Alpha".into();
    worlds[2].name = "Beta".into();
    for world in &worlds {
        player.record_world_acquisition(world.id);
    }
    let expected = player.world_acquisition_order.clone();
    worlds.push(model.map.get(player.home_planet).clone());
    worlds.reverse();
    worlds.sort_by_key(|planet| world_shortcut_order(planet, &player));
    assert_eq!(worlds.iter().map(|planet| planet.id).collect::<Vec<_>>(), expected);
}

#[test]
fn enemy_counts_use_visible_intelligence_not_hidden_ownership() {
    let mut model = crate::core::simulation::GameModel::new([12; 32], Default::default()).unwrap();
    let player = model.players[0].clone();
    let enemy_home = model.players[1].home_planet;
    let hidden =
        model.map.planets.iter().find(|p| !p.is_moon() && p.controlled.is_none()).unwrap().id;
    model.map.get_mut(hidden).controlled = Some(2);
    let moon = model.map.moons()[0].id;
    model.map.get_mut(moon).controlled = Some(2);
    assert_eq!(known_planet_counts(&model.map, &player, &[]).get(&2), None);
    let visible = Mission::new_with_id(
        1,
        1,
        2,
        model.map.get(enemy_home),
        model.map.get(player.home_planet),
        Icon::Attack,
        Army::from([(Unit::probe(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    assert_eq!(
        known_planet_counts(&model.map, &player, std::slice::from_ref(&visible)).get(&2),
        Some(&1)
    );
    // Unknown changes cannot update the count; it remains last-known intelligence.
    model.map.get_mut(enemy_home).controlled = None;
    assert_eq!(
        known_planet_counts(&model.map, &player, std::slice::from_ref(&visible)).get(&2),
        Some(&1)
    );
    model.map.get_mut(enemy_home).controlled = Some(player.id);
    assert_eq!(
        known_planet_counts(&model.map, &player, std::slice::from_ref(&visible)).get(&2),
        None
    );
    model.map.get_mut(enemy_home).controlled = Some(2);
    model.map.get_mut(enemy_home).is_destroyed = true;
    assert_eq!(known_planet_counts(&model.map, &player, &[visible]).get(&2), None);
}

#[test]
fn public_strategic_structures_reveal_and_extend_known_enemy_owner_counts() {
    let mut model = crate::core::simulation::GameModel::new([13; 32], Default::default()).unwrap();
    let player = model.players[0].clone();
    let enemy_id = model.players[1].id;
    let enemy_home = model.players[1].home_planet;
    let hidden = model
        .map
        .planets
        .iter()
        .find(|planet| !planet.is_moon() && planet.controlled.is_none())
        .unwrap()
        .id;
    model.map.get_mut(hidden).owned = Some(enemy_id);
    model.map.get_mut(hidden).controlled = Some(player.id);

    assert_eq!(known_planet_counts(&model.map, &player, &[]).get(&enemy_id), None);
    model.map.get_mut(hidden).army.insert(Unit::space_dock(), 1);
    assert_eq!(known_planet_counts(&model.map, &player, &[]).get(&enemy_id), Some(&1));
    assert_eq!(
        known_planet_counts(&model.map, &player, &[]).get(&player.id),
        Some(&1),
        "the public marker reveals the owner, not a different current controller"
    );

    model.map.get_mut(hidden).army.remove(&Unit::space_dock());
    model.map.get_mut(hidden).army.insert(Unit::Building(Building::OrbitalRailgun), 1);
    assert_eq!(
        known_planet_counts(&model.map, &player, &[]).get(&enemy_id),
        Some(&1),
        "an Orbital Railgun publishes the same ownership intelligence"
    );

    let known_enemy_mission = Mission::new_with_id(
        1,
        1,
        enemy_id,
        model.map.get(enemy_home),
        model.map.get(player.home_planet),
        Icon::Attack,
        Army::from([(Unit::probe(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    assert_eq!(
        known_planet_counts(&model.map, &player, &[known_enemy_mission]).get(&enemy_id),
        Some(&2),
        "a public strategic structure must add to intelligence the player already had"
    );
}

#[test]
fn spy_reports_reveal_buildings_and_orbitals_one_intelligence_tier_at_a_time() {
    let mut origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    origin.owned = Some(1);
    origin.controlled = Some(1);
    let mut target = Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
    target.owned = Some(2);
    target.controlled = Some(2);

    let tiers = [
        (Building::MetalMine, 1),
        (Building::CrystalMine, 1),
        (Building::DeuteriumSynthesizer, 1),
        (Building::Reactor, 2),
        (Building::Terraformer, 2),
        (Building::Shipyard, 3),
        (Building::Factory, 3),
        (Building::MissileSilo, 3),
        (Building::PlanetaryShield, 4),
        (Building::Senate, 5),
        (Building::ColonialAdministration, 5),
        (Building::SolarSatellite, 1),
        (Building::CommandRelay, 2),
        (Building::SensorPhalanx, 3),
        (Building::JumpGate, 4),
    ];
    target.army = tiers
        .iter()
        .map(|(building, _)| (Unit::Building(*building), 1))
        .chain([(Unit::Building(Building::OrbitalRailgun), 1), (Unit::space_dock(), 1)])
        .collect();

    for (returning_probes, visible_tier) in [(5, 1), (6, 2), (11, 3), (16, 4), (21, 5)] {
        let mission = Mission::new_with_id(
            1,
            1,
            1,
            &origin,
            &target,
            Icon::Spy,
            Army::from([(Unit::probe(), returning_probes)]),
            BombingRaid::None,
            false,
            false,
            None,
        );
        let mut player = Player::new(1, origin.id);
        player.push_report(MissionReport {
            id: 1,
            turn: 2,
            mission,
            planet: target.clone(),
            scout_probes: returning_probes,
            surviving_attacker: Army::from([(Unit::probe(), returning_probes)]),
            surviving_defender: target.army.clone(),
            planet_colonized: false,
            planet_destroyed: false,
            destination_owned: target.owned,
            destination_controlled: target.controlled,
            combat_report: None,
            hidden: false,
        });

        let known = player.last_info(&target, &[]).expect("Spy report should create intelligence");
        for (building, tier) in tiers {
            assert_eq!(
                known.army.amount(&Unit::Building(building)),
                usize::from(tier <= visible_tier),
                "{building:?} visibility with {returning_probes} returning Probes"
            );
        }
        for public in [Unit::Building(Building::OrbitalRailgun), Unit::space_dock()] {
            assert_eq!(
                known.army.amount(&public),
                1,
                "{public:?} should remain public with {returning_probes} returning Probes"
            );
        }
    }
}

#[test]
fn building_intelligence_stat_uses_its_icon_value_and_explanation() {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 220.0));
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let intelligence = egui::TextureId::User(1);
    let images = ImageIds(HashMap::from([("intelligence".into(), intelligence)]));
    let unit = Unit::Building(Building::SensorPhalanx);
    let input = egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };

    let mut target = egui::Rect::NOTHING;
    let mut output = context.run_ui(input, |context| {
        egui::CentralPanel::default().show(context, |ui| {
            target = shop::draw_intelligence_stat(ui, &unit, &images).rect;
            shop::draw_stat_hover(ui, &CombatStats::Intelligence, &images);
        });
    });
    output.textures_delta.clear();
    assert!(has_text(&output.shapes, "2"));
    assert_eq!(images.get("intelligence"), intelligence);
    assert!(target.width() >= 180.0 && target.height() >= 45.0);
    assert!(has_text(&output.shapes, "Intelligence"));
    assert!(has_text(
        &output.shapes,
        "The minimum intelligence level required for an enemy Spy mission to see this structure."
    ));
    assert_eq!(
        Unit::Building(Building::OrbitalRailgun).get_stat(&CombatStats::Intelligence),
        "---"
    );
    assert_eq!(Unit::space_dock().get_stat(&CombatStats::Intelligence), "---");
}

#[test]
fn orbital_hover_stats_put_production_before_intelligence_in_one_row() {
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(700.0, 300.0));
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let production = egui::TextureId::User(1);
    let intelligence = egui::TextureId::User(2);
    let images = ImageIds(HashMap::from([
        ("production".into(), production),
        ("intelligence".into(), intelligence),
    ]));
    let unit = Unit::Building(Building::OrbitalRailgun);
    let mut production_box = egui::Rect::NOTHING;
    let mut intelligence_box = egui::Rect::NOTHING;

    let input = egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };
    let mut output = context.run_ui(input, |ui| {
        let (production, intelligence) = shop::draw_orbital_stats(ui, &unit, &images);
        production_box = production.rect;
        intelligence_box = intelligence.rect;
    });
    output.textures_delta.clear();

    assert!(has_text(&output.shapes, "5"));
    assert!(has_text(&output.shapes, "---"));
    assert_eq!(images.get("production"), production);
    assert_eq!(images.get("intelligence"), intelligence);
    assert!(production_box.right() < intelligence_box.left());
    assert!((production_box.top() - intelligence_box.top()).abs() < 1.0);
}

#[test]
fn intelligence_stat_leaves_space_before_the_next_separator() {
    let context = egui::Context::default();
    let images = ImageIds(HashMap::from([("intelligence".into(), egui::TextureId::User(1))]));
    let unit = Unit::Building(Building::SensorPhalanx);
    let mut stat = egui::Rect::NOTHING;
    let mut separator = egui::Rect::NOTHING;

    let mut output = context.run_ui(Default::default(), |ui| {
        stat = shop::draw_intelligence_stat(ui, &unit, &images).rect;
        separator = ui.separator().rect;
    });
    output.textures_delta.clear();

    assert!(separator.top() - stat.bottom() >= 12.0);
}

#[test]
fn enemy_progress_fits_beside_long_names_and_disconnected_status() {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::simulation::{GameModel, MatchStatus, PersistedGame};
    use crate::multiplayer::model::{GameMembership, GameRecord};
    let mut model = GameModel::new([21; 32], Default::default()).unwrap();
    let player = model.players[0].clone();
    model
        .map
        .planets
        .iter_mut()
        .find(|planet| !planet.is_moon() && planet.controlled.is_none())
        .unwrap()
        .controlled = Some(player.id);
    let id = GameId::new("enemy-progress");
    for connected in [true, false] {
        for width in [320.0, 480.0, 1280.0] {
            let mut session = MultiplayerSession::default();
            session.active_game = Some(GameRecord {
                id: id.clone(),
                code: GameCode::new("ABCDEF"),
                revision: 0,
                saved_at: 0,
                max_players: 2,
                status: MatchStatus::Active,
                persisted: PersistedGame::new(model.clone()),
                submitted_players: vec![],
                members: vec![
                    GameMembership {
                        game_id: id.clone(),
                        player_id: player.id,
                        user_id: UserId::new("local"),
                        display_name: "Local player".into(),
                        is_creator: true,
                        identity_version: 1,
                        connected: true,
                    },
                    GameMembership {
                        game_id: id.clone(),
                        player_id: 2,
                        user_id: UserId::new("enemy"),
                        display_name: "An exceptionally long enemy player name".into(),
                        is_creator: false,
                        identity_version: 1,
                        connected,
                    },
                ],
            });
            let context = egui::Context::default();
            let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(width, 360.0));
            let mut shapes = vec![];
            for _ in 0..3 {
                let mut output = context.run_ui(
                    egui::RawInput {
                        screen_rect: Some(viewport),
                        ..default()
                    },
                    |context| {
                        draw_players_widget(context, &session, &player, &model.map, &[]);
                    },
                );
                output.textures_delta.clear();
                shapes = output.shapes;
            }
            let progress = text_rect(&shapes, "?/15");
            assert!(viewport.contains_rect(progress), "width={width}, connected={connected}");
            assert!(has_text(&shapes, "PLAYERS"));
            assert!(has_text(&shapes, "2/15"));
            let local_name = text_rect(&shapes, "Local player");
            let name = text_rect(&shapes, "An exceptionally long enemy player name");
            assert!(local_name.top() < name.top());
            assert!(name.right() <= progress.left());
            if !connected {
                let status = text_rect(&shapes, "DISCONNECTED");
                assert!(progress.right() <= status.left());
                assert!(viewport.contains_rect(status));
            }
        }
    }
}

#[test]
fn eliminated_player_shows_zero_progress_and_a_struck_name() {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::simulation::{GameModel, GameRules, MatchStatus, PersistedGame};
    use crate::multiplayer::model::{GameMembership, GameRecord};

    let mut model = GameModel::new(
        [24; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    let local_player = model.players[0].clone();
    let eliminated = model.players[1].id;
    model.players[1].spectator = true;
    let target = model.planets_to_win();
    let game_id = GameId::new("eliminated-player-panel");
    let members = [(local_player.id, "Local player"), (eliminated, "Fallen empire"), (3, "Rival")]
        .into_iter()
        .map(|(player_id, display_name)| GameMembership {
            game_id: game_id.clone(),
            player_id,
            user_id: UserId::new(format!("user-{player_id}")),
            display_name: display_name.into(),
            is_creator: player_id == local_player.id,
            identity_version: 1,
            connected: true,
        })
        .collect();
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: game_id,
        code: GameCode::new("ABCDEF"),
        revision: 1,
        saved_at: 0,
        max_players: 3,
        status: MatchStatus::Active,
        persisted: PersistedGame::new(model.clone()),
        submitted_players: Vec::new(),
        members,
    });
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 420.0));
    let mut shapes = Vec::new();
    for _ in 0..3 {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..default()
            },
            |context| {
                draw_players_widget(context, &session, &local_player, &model.map, &[]);
            },
        );
        output.textures_delta.clear();
        shapes = output.shapes;
    }

    assert!(has_text(&shapes, &format!("0/{target}")));
    let name = text_rect(&shapes, "Fallen empire");
    assert!(
        shapes.iter().any(|shape| shape_has_line_through(&shape.shape, name)),
        "the eliminated player's name should be crossed through its middle"
    );
}

#[test]
fn players_panel_matches_owned_worlds_width_and_left_inset() {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::simulation::{GameModel, MatchStatus, PersistedGame};
    use crate::multiplayer::model::{GameMembership, GameRecord};

    let model = GameModel::new([22; 32], Default::default()).unwrap();
    let player = model.players[0].clone();
    let id = GameId::new("matching-hud-panels");
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: id.clone(),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: 2,
        status: MatchStatus::Active,
        persisted: PersistedGame::new(model.clone()),
        submitted_players: vec![],
        members: vec![
            GameMembership {
                game_id: id.clone(),
                player_id: player.id,
                user_id: UserId::new("local"),
                display_name: "Local player".into(),
                is_creator: true,
                identity_version: 1,
                connected: true,
            },
            GameMembership {
                game_id: id,
                player_id: 2,
                user_id: UserId::new("enemy"),
                display_name: "Enemy".into(),
                is_creator: false,
                identity_version: 1,
                connected: true,
            },
        ],
    });
    session.local_practice = true;

    for viewport_size in [egui::vec2(1_280.0, 720.0), egui::vec2(1_920.0, 1_080.0)] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, viewport_size);
        let mut state = UiState::default();
        let mut settings = Settings::default();
        let images = ImageIds::default();
        let mut owned_panel = egui::Rect::NOTHING;
        let mut players_panel = egui::Rect::NOTHING;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..default()
            },
            |context| {
                owned_panel = draw_owned_worlds_widget(
                    context,
                    &model.map,
                    &player,
                    &session,
                    &mut state,
                    &mut settings,
                    &images,
                );
                players_panel = draw_players_widget(context, &session, &player, &model.map, &[]);
            },
        );
        output.textures_delta.clear();

        assert_eq!(players_panel.left(), owned_panel.left());
        assert_eq!(players_panel.width(), owned_panel.width());

        let mut player_output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..default()
            },
            |context| {
                draw_players_widget(context, &session, &player, &model.map, &[]);
            },
        );
        player_output.textures_delta.clear();
        assert!(
            player_output.shapes.iter().all(|shape| !shape_has_line_segment(&shape.shape)),
            "an opponent name should not be underlined before hover"
        );
        let enemy_name = text_rect(&player_output.shapes, "Enemy");
        let mut hover_output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                events: vec![egui::Event::PointerMoved(enemy_name.center())],
                ..default()
            },
            |context| {
                draw_players_widget(context, &session, &player, &model.map, &[]);
            },
        );
        hover_output.textures_delta.clear();
        assert!(
            hover_output.shapes.iter().any(|shape| shape_has_line_segment(&shape.shape)),
            "hovering a switchable player should paint a player-colored underline"
        );
        assert!(!has_text(&hover_output.shapes, "Control this empire and edit its turn draft."));
    }
}

fn text_rect(shapes: &[egui::epaint::ClippedShape], text: &str) -> egui::Rect {
    shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(label) if label.galley.job.text == text => {
                Some(label.galley.rect.translate(label.pos.to_vec2()))
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing `{text}` shortcut label"))
}

fn has_text(shapes: &[egui::epaint::ClippedShape], text: &str) -> bool {
    shapes.iter().any(|shape| match &shape.shape {
        egui::Shape::Text(label) => label.galley.job.text == text,
        _ => false,
    })
}

fn count_text(shapes: &[egui::epaint::ClippedShape], text: &str) -> usize {
    shapes
        .iter()
        .filter(|shape| {
            matches!(&shape.shape, egui::Shape::Text(label) if label.galley.job.text == text)
        })
        .count()
}

#[test]
fn protection_access_tooltip_lists_allowed_players_in_their_game_colors() {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::simulation::{GameModel, GameRules, MatchStatus, PersistedGame};
    use crate::multiplayer::model::{GameMembership, GameRecord};

    let mut model = GameModel::new(
        [23; 32],
        GameRules {
            player_count: 4,
            ..default()
        },
    )
    .unwrap();
    let protected = model.players[0].home_planet;
    let planet_name = model.map.get(protected).name.clone();
    model.map.get_mut(protected).protection_permissions.extend([2, 3, 4]);
    let id = GameId::new("protection-tooltip");
    let local_membership = GameMembership {
        game_id: id.clone(),
        player_id: 1,
        user_id: UserId::new("local"),
        display_name: "Local player".into(),
        is_creator: true,
        identity_version: 1,
        connected: true,
    };
    let mut session = MultiplayerSession::default();
    session.membership = Some(local_membership.clone());
    session.active_game = Some(GameRecord {
        id: id.clone(),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: 4,
        status: MatchStatus::Active,
        persisted: PersistedGame::new(model),
        submitted_players: vec![],
        members: vec![
            local_membership,
            GameMembership {
                game_id: id.clone(),
                player_id: 2,
                user_id: UserId::new("protector"),
                display_name: "Allowed player".into(),
                is_creator: false,
                identity_version: 1,
                connected: true,
            },
            GameMembership {
                game_id: id.clone(),
                player_id: 3,
                user_id: UserId::new("opponent-3"),
                display_name: "Opponent 3".into(),
                is_creator: false,
                identity_version: 1,
                connected: true,
            },
            GameMembership {
                game_id: id,
                player_id: 4,
                user_id: UserId::new("opponent-4"),
                display_name: "Opponent 4".into(),
                is_creator: false,
                identity_version: 1,
                connected: true,
            },
        ],
    });
    let expected_colors =
        [2, 3, 4].map(|player_id| session.player_color(player_id).color().to_color32());
    let context = egui::Context::default();
    let mut output = context.run_ui(Default::default(), |ui| {
        let planet = session.active_game.as_ref().unwrap().persisted.state.map.get(protected);
        draw_protection_access_tooltip(ui, planet, &session);
    });
    output.textures_delta.clear();

    assert!(has_text(&output.shapes, "Players currently allowed:"));
    for (name, color) in
        ["Allowed player", "Opponent 3", "Opponent 4"].into_iter().zip(expected_colors)
    {
        assert_eq!(text_color(&output.shapes, name), color);
    }
    assert_eq!(
        text_font_size(&output.shapes, "Allowed player"),
        text_font_size(&output.shapes, "Players currently allowed:")
    );
    assert_eq!(count_text(&output.shapes, ", "), 2);
    assert!(
        (text_rect(&output.shapes, "Players currently allowed:").center().y
            - text_rect(&output.shapes, "Allowed player").center().y)
            .abs()
            < 1.0
    );
    assert!(!has_text(&output.shapes, "None"));

    let panel_texture = egui::TextureId::User(90);
    let protect_texture = egui::TextureId::User(91);
    let description_text =
        format!("Choose who may send fleets to {planet_name}. Changes apply immediately.");
    let images = ImageIds(HashMap::from([
        ("panel".to_string(), panel_texture),
        ("protect".to_string(), protect_texture),
    ]));
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(560.0, 430.0));
    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    for _ in 0..2 {
        context.begin_pass(input());
        let planet = session.active_game.as_ref().unwrap().persisted.state.map.get(protected);
        assert_eq!(
            draw_protection_access_modal(&context, &images, planet, &session),
            (false, vec![])
        );
        output = context.end_pass();
        output.textures_delta.clear();
    }
    let panel = image_rect(&output.shapes, panel_texture).expect("missing protection panel");
    let icon = image_rect(&output.shapes, protect_texture).expect("missing protection icon");
    let title_text = "Protection Access";
    let title = text_rect(&output.shapes, title_text);
    assert!(viewport.contains_rect(panel));
    assert!(panel.contains_rect(icon));
    assert!(icon.right() > panel.center().x && icon.top() < panel.center().y);
    assert!((icon.top() - panel.top() - MODAL_ICON_TOP_INSET).abs() < 1.0);
    assert!((panel.right() - icon.right() - MODAL_ICON_RIGHT_INSET).abs() < 1.0);
    assert!((title.center().x - panel.center().x).abs() < 1.0);
    let description = text_rect(&output.shapes, &description_text);
    assert!(description.top() - title.bottom() >= 12.0);
    assert_eq!(text_color(&output.shapes, title_text), ABANDON_CONFIRMATION_TEXT_COLOR);
    for text in [title_text, "Allowed player", "Opponent 3", "Opponent 4", "Close"] {
        assert!(panel.contains_rect(text_rect(&output.shapes, text)), "`{text}` escaped the panel");
    }
    assert_eq!(text_color(&output.shapes, "Allowed player"), expected_colors[0]);
    let mut painted_rects = Vec::new();
    for shape in &output.shapes {
        collect_rects(&shape.shape, &mut painted_rects);
    }
    let player_rows = painted_rects
        .iter()
        .filter(|rect| {
            (rect.height() - 42.0).abs() < 1.0
                && rect.width() >= PROTECTION_PLAYER_ROW_MIN_WIDTH - 1.0
                && rect.width() <= PROTECTION_PLAYER_ROW_MAX_WIDTH + 1.0
        })
        .collect::<Vec<_>>();
    assert_eq!(player_rows.len(), 3);
    assert!(player_rows.iter().all(|row| row.width() < panel.width() * 0.5));
    assert!(player_rows.iter().all(|row| (row.center().x - panel.center().x).abs() < 1.0));
    let first_row_top = player_rows
        .iter()
        .map(|row| row.top())
        .min_by(f32::total_cmp)
        .expect("missing first protection row");
    assert!(first_row_top - description.bottom() >= 18.0);
    let close = painted_rects
        .iter()
        .find(|rect| {
            (rect.width() - 110.0).abs() < 1.0 && (rect.height() - MODAL_BUTTON_HEIGHT).abs() < 1.0
        })
        .expect("missing contained Close button");
    assert!(panel.contains_rect(*close));

    session
        .active_game
        .as_mut()
        .unwrap()
        .persisted
        .state
        .map
        .get_mut(protected)
        .protection_permissions
        .clear();
    let mut output = egui::Context::default().run_ui(Default::default(), |ui| {
        let planet = session.active_game.as_ref().unwrap().persisted.state.map.get(protected);
        draw_protection_access_tooltip(ui, planet, &session);
    });
    output.textures_delta.clear();
    assert!(!has_text(&output.shapes, "Players currently allowed:"));
    assert!(!has_text(&output.shapes, "None"));
}

#[test]
fn protecting_unit_counts_exclude_the_white_controller_total() {
    let fighter = Unit::Ship(Ship::LightFighter);
    let mut planet = Planet::new(9, "Protected world".into(), Vec2::ZERO, false, 1.0);
    planet.controlled = Some(1);
    planet.army.insert(fighter, 6);
    planet.army.dock_protector(2, Army::from([(fighter, 3)]));
    planet.army.dock_protector(3, Army::from([(fighter, 4)]));

    let protection = protecting_unit_counts(&planet, &fighter);
    assert_eq!(protection, vec![(2, 3), (3, 4)]);
    assert_eq!(protection.iter().map(|(_, count)| count).sum::<usize>(), 7);
    assert_eq!(planet.army.combined_amount(&fighter), 13);
}

#[test]
fn overview_unit_counts_render_protection_smaller_and_in_player_colors() {
    let context = egui::Context::default();
    let session = MultiplayerSession::default();
    let protection = [(2, 3), (3, 4)];
    let mut image_rect = egui::Rect::NOTHING;
    let mut output = context.run_ui(Default::default(), |ui| {
        image_rect = ui.allocate_exact_size(egui::vec2(50.0, 50.0), Sense::hover()).0;
        draw_overview_unit_counts(ui, image_rect, 6, &protection, &session);
    });
    output.textures_delta.clear();

    assert_eq!(text_color(&output.shapes, "6"), Color32::WHITE);
    assert_eq!(text_color(&output.shapes, "3"), session.player_color(2).color().to_color32());
    assert_eq!(text_color(&output.shapes, "4"), session.player_color(3).color().to_color32());
    assert!(text_font_size(&output.shapes, "3") < text_font_size(&output.shapes, "6"));
    for text in ["6", "3", "4"] {
        assert!(image_rect.intersects(text_rect(&output.shapes, text)));
    }
}

fn text_color(shapes: &[egui::epaint::ClippedShape], text: &str) -> Color32 {
    shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(label) if label.galley.job.text == text => {
                label.galley.job.sections.first().map(|section| section.format.color)
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing `{text}` text color"))
}

fn text_font_size(shapes: &[egui::epaint::ClippedShape], text: &str) -> f32 {
    shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(label) if label.galley.job.text == text => {
                label.galley.job.sections.first().map(|section| section.format.font_id.size)
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing `{text}` font size"))
}

fn collect_rects(shape: &egui::Shape, rects: &mut Vec<egui::Rect>) {
    match shape {
        egui::Shape::Rect(candidate) => rects.push(candidate.rect),
        egui::Shape::Vec(shapes) => {
            for shape in shapes {
                collect_rects(shape, rects);
            }
        },
        _ => {},
    }
}

fn shape_has_fill(shape: &egui::Shape, fill: Color32) -> bool {
    match shape {
        egui::Shape::Rect(candidate) => candidate.fill == fill,
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| shape_has_fill(shape, fill)),
        _ => false,
    }
}

fn shape_has_line_segment(shape: &egui::Shape) -> bool {
    match shape {
        egui::Shape::LineSegment {
            ..
        } => true,
        egui::Shape::Vec(shapes) => shapes.iter().any(shape_has_line_segment),
        _ => false,
    }
}

fn shape_has_line_through(shape: &egui::Shape, rect: egui::Rect) -> bool {
    match shape {
        egui::Shape::LineSegment {
            points,
            ..
        } => {
            (points[0].y - points[1].y).abs() < 0.5
                && (points[0].y - rect.center().y).abs() < 1.0
                && points[0].x.min(points[1].x) <= rect.left() + 1.0
                && points[0].x.max(points[1].x) >= rect.right() - 1.0
        },
        egui::Shape::Vec(shapes) => shapes.iter().any(|shape| shape_has_line_through(shape, rect)),
        _ => false,
    }
}

fn image_rect(
    shapes: &[egui::epaint::ClippedShape],
    texture_id: egui::TextureId,
) -> Option<egui::Rect> {
    image_rects(shapes, texture_id).into_iter().next()
}

fn image_rects(
    shapes: &[egui::epaint::ClippedShape],
    texture_id: egui::TextureId,
) -> Vec<egui::Rect> {
    fn collect(shape: &egui::Shape, texture_id: egui::TextureId, rects: &mut Vec<egui::Rect>) {
        match shape {
            egui::Shape::Mesh(mesh) if mesh.texture_id == texture_id => {
                let min = mesh
                    .vertices
                    .iter()
                    .fold(egui::pos2(f32::INFINITY, f32::INFINITY), |min, v| min.min(v.pos));
                let max = mesh
                    .vertices
                    .iter()
                    .fold(egui::pos2(f32::NEG_INFINITY, f32::NEG_INFINITY), |max, v| {
                        max.max(v.pos)
                    });
                rects.push(egui::Rect::from_min_max(min, max));
            },
            egui::Shape::Vec(shapes) => {
                for shape in shapes {
                    collect(shape, texture_id, rects);
                }
            },
            _ => {},
        }
    }

    let mut rects = Vec::new();
    for shape in shapes {
        collect(&shape.shape, texture_id, &mut rects);
    }
    rects
}

fn image_tint(
    shapes: &[egui::epaint::ClippedShape],
    texture_id: egui::TextureId,
) -> Option<Color32> {
    shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Mesh(mesh) if mesh.texture_id == texture_id => {
            mesh.vertices.first().map(|vertex| vertex.color)
        },
        _ => None,
    })
}

#[test]
fn world_shortcut_selects_fleet_silhouettes_from_stationed_ships() {
    let mut planet = Planet::new(1, "Masduk".to_string(), Vec2::ZERO, false, 1.0);
    planet.army.insert(Unit::Building(Building::Shipyard), 1);
    planet.army.insert(Unit::Defense(Defense::RocketLauncher), 2);
    assert_eq!(world_shortcut_fleet_image(planet.army.controller()), None);

    planet.army.insert(Unit::probe(), 3);
    assert_eq!(world_shortcut_fleet_image(planet.army.controller()), Some("mission spy"));

    planet.army.insert(Unit::Ship(Ship::LightFighter), 1);
    assert_eq!(world_shortcut_fleet_image(planet.army.controller()), Some("mission"));

    planet.army.insert(Unit::war_sun(), 1);
    assert_eq!(world_shortcut_fleet_image(planet.army.controller()), Some("mission destroy"));
}

#[test]
fn world_shortcut_centers_the_name_and_only_shows_a_fleet_icon_for_a_fleet() {
    let context = egui::Context::default();
    let mut style = NordDark.custom_style();
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    context.set_global_style(style);
    let mut planet = Planet::new(1, "Masduk".to_string(), Vec2::ZERO, false, 1.0);
    planet.army.insert(Unit::Ship(Ship::LightFighter), 1);
    let planet_texture = egui::TextureId::User(1);
    let mission_texture = egui::TextureId::User(2);
    let fleet_color = Color32::from_rgb(244, 197, 66);
    let images = ImageIds(HashMap::from([
        (planet.image(), planet_texture),
        ("mission".to_string(), mission_texture),
    ]));

    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(260.0, 100.0),
            )),
            events: vec![egui::Event::PointerMoved(egui::pos2(50.0, 25.0))],
            ..default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                draw_world_shortcut(
                    ui,
                    &planet,
                    false,
                    fleet_color,
                    &MultiplayerSession::default(),
                    &images,
                    1.0,
                );
            });
        },
    );
    output.textures_delta.clear();

    let name = text_rect(&output.shapes, "Masduk");
    let planet_icon = image_rect(&output.shapes, planet_texture).expect("missing planet icon");
    let fleet_icon = image_rect(&output.shapes, mission_texture).expect("missing fleet icon");
    assert!((name.center().y - planet_icon.center().y).abs() < 1.0);
    assert!((fleet_icon.center().y - planet_icon.center().y).abs() < 1.0);
    assert!(name.right() < fleet_icon.left());
    assert_eq!(fleet_icon.size(), egui::Vec2::splat(20.0));
    assert_eq!(image_tint(&output.shapes, mission_texture), Some(fleet_color));
    assert!(
        !has_text(&output.shapes, "FLEET") && !has_text(&output.shapes, "NO FLEET"),
        "world shortcut still painted fleet status text"
    );
    for tooltip in
        ["Center the map and open this planet", "Center the map on this controlled world"]
    {
        assert!(!has_text(&output.shapes, tooltip), "world shortcut still painted `{tooltip}`");
    }

    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let planet = Planet::new(2, "Galix".to_string(), Vec2::ZERO, false, 1.0);
    let mut output = context.run_ui(egui::RawInput::default(), |context| {
        egui::CentralPanel::default().show(context, |ui| {
            draw_world_shortcut(
                ui,
                &planet,
                false,
                fleet_color,
                &MultiplayerSession::default(),
                &images,
                1.0,
            );
        });
    });
    output.textures_delta.clear();

    assert!(image_rect(&output.shapes, mission_texture).is_none());
    assert!(!has_text(&output.shapes, "NO FLEET"));
}

#[test]
fn world_shortcut_shows_a_protecting_fleet_in_its_players_selected_color() {
    use crate::core::identity::{GameCode, GameId};
    use crate::core::player::PlayerColor;
    use crate::core::simulation::{GameModel, MatchStatus, PersistedGame};
    use crate::multiplayer::model::GameRecord;

    let mut model = GameModel::new([31; 32], Default::default()).unwrap();
    let protector_color = PlayerColor::new(4).unwrap();
    model.players[1].color = protector_color;
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: GameId::new("world-shortcut-protection"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: 2,
        status: MatchStatus::Active,
        persisted: PersistedGame::new(model),
        submitted_players: Vec::new(),
        members: Vec::new(),
    });

    let mut planet = Planet::new(1, "Masduk".to_string(), Vec2::ZERO, false, 1.0);
    planet.army.insert(Unit::Ship(Ship::LightFighter), 1);
    planet.army.dock_protector(2, Army::from([(Unit::probe(), 3)]));
    let planet_texture = egui::TextureId::User(1);
    let fleet_texture = egui::TextureId::User(2);
    let protecting_fleet_texture = egui::TextureId::User(3);
    let controller_color = Color32::from_rgb(102, 128, 255);
    let images = ImageIds(HashMap::from([
        (planet.image(), planet_texture),
        ("mission".to_string(), fleet_texture),
        ("mission spy".to_string(), protecting_fleet_texture),
    ]));
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(260.0, 100.0),
            )),
            ..default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                draw_world_shortcut(ui, &planet, false, controller_color, &session, &images, 1.0);
            });
        },
    );
    output.textures_delta.clear();

    let fleet = image_rect(&output.shapes, fleet_texture).expect("missing controller fleet");
    let protecting_fleet =
        image_rect(&output.shapes, protecting_fleet_texture).expect("missing protecting fleet");
    assert!(fleet.right() < protecting_fleet.left());
    assert_eq!(image_tint(&output.shapes, fleet_texture), Some(controller_color));
    assert_eq!(
        image_tint(&output.shapes, protecting_fleet_texture),
        Some(protector_color.color().to_color32())
    );
}

#[test]
fn home_shortcut_crown_fits_with_long_names_and_fleets_at_small_scales() {
    for scale in [0.72, 1.0] {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut planet =
            Planet::new(1, "An exceptionally long home planet name".into(), Vec2::ZERO, false, 1.0);
        planet.army.insert(Unit::Ship(Ship::LightFighter), 1);
        let planet_texture = egui::TextureId::User(1);
        let fleet_texture = egui::TextureId::User(2);
        let images = ImageIds(HashMap::from([
            (planet.image(), planet_texture),
            ("mission".into(), fleet_texture),
        ]));
        let mut row = egui::Rect::NOTHING;
        let mut output = context.run_ui(egui::RawInput::default(), |context| {
            egui::CentralPanel::default().show(context, |ui| {
                ui.set_width(OWNED_WORLDS_WIDTH * scale);
                let top = ui.cursor().min;
                row = egui::Rect::from_min_size(
                    top,
                    egui::vec2(ui.available_width(), WORLD_SHORTCUT_HEIGHT * scale),
                );
                draw_world_shortcut(
                    ui,
                    &planet,
                    true,
                    Color32::WHITE,
                    &MultiplayerSession::default(),
                    &images,
                    scale,
                );
            });
        });
        output.textures_delta.clear();
        let name = text_rect(&output.shapes, &planet.name);
        assert!(!has_text(&output.shapes, "HOME"));
        let home = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh)
                    if mesh.vertices.iter().all(|v| v.color == HOME_PLANET_COLOR.to_color32()) =>
                {
                    Some(mesh.calc_bounds())
                },
                _ => None,
            })
            .expect("home crown");
        let fleet = image_rect(&output.shapes, fleet_texture).unwrap();
        assert!(row.contains_rect(home));
        assert!(row.contains_rect(name));
        assert!(row.contains_rect(fleet));
        assert!(home.right() < name.left());
        assert!((home.center().y - name.center().y).abs() < 1.0);
        assert!(name.right() < fleet.left());
    }
}

#[test]
fn world_groups_use_separate_headings_with_prominent_counts() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(300.0, 160.0),
            )),
            ..default()
        },
        |context| {
            egui::CentralPanel::default().show(context, |ui| {
                ui.set_width(OWNED_WORLDS_WIDTH);
                draw_world_group_header(ui, "OWNED PLANETS", 7, 1.0);
                draw_world_group_header(ui, "CONTROLLED PLANETS AND MOONS", 3, 1.0);
            });
        },
    );
    output.textures_delta.clear();

    for label in ["OWNED PLANETS", "7", "CONTROLLED PLANETS AND MOONS", "3"] {
        text_rect(&output.shapes, label);
    }
    let own_heading = text_rect(&output.shapes, "OWNED PLANETS");
    let own_count = text_rect(&output.shapes, "7");
    let controlled_heading = text_rect(&output.shapes, "CONTROLLED PLANETS AND MOONS");
    let controlled_count = text_rect(&output.shapes, "3");
    assert!(own_count.height() > own_heading.height());
    assert!(controlled_count.height() > controlled_heading.height());
    assert!(controlled_count.left() - controlled_heading.right() >= 8.0);
    assert!(
        output.shapes.iter().all(|shape| match &shape.shape {
            egui::Shape::Text(label) => label.galley.job.text != "YOUR WORLDS",
            _ => true,
        }),
        "the removed aggregate heading was still painted"
    );
}

#[test]
fn owned_worlds_panel_grows_with_the_number_of_planets() {
    fn panel_height(planet_count: usize) -> f32 {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let planets = (0..planet_count)
            .map(|id| {
                let mut planet = Planet::new(id, format!("Planet {id}"), Vec2::ZERO, false, 1.0);
                planet.owned = Some(1);
                planet
            })
            .collect::<Vec<_>>();
        let images = ImageIds(
            planets.iter().map(|planet| (planet.image(), egui::TextureId::User(1))).collect(),
        );
        let map = Map {
            rect: Rect::default(),
            solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
            planets,
        };
        let player = Player::new(1, 0);
        let mut state = UiState::default();
        let mut settings = Settings::default();
        let mut height = 0.0;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1_280.0, 2_000.0),
                )),
                ..default()
            },
            |context| {
                height = draw_owned_worlds_widget(
                    context,
                    &map,
                    &player,
                    &MultiplayerSession::default(),
                    &mut state,
                    &mut settings,
                    &images,
                )
                .height();
            },
        );
        output.textures_delta.clear();
        height
    }

    let one_planet = panel_height(1);
    let four_planets = panel_height(4);

    assert!(
        four_planets >= one_planet + 3.0 * (WORLD_SHORTCUT_HEIGHT + WORLD_LIST_ITEM_SPACING),
        "one planet: {one_planet}, four planets: {four_planets}"
    );
}

#[test]
fn owned_worlds_panel_uses_the_compact_screen_edge_inset_below_the_resource_panel() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: Vec::new(),
    };
    let player = Player::new(1, 0);
    let mut state = UiState::default();
    let mut settings = Settings::default();
    let images = ImageIds::default();
    let mut panel = egui::Rect::NOTHING;
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1_280.0, 720.0),
            )),
            ..default()
        },
        |context| {
            panel = draw_owned_worlds_widget(
                context,
                &map,
                &player,
                &MultiplayerSession::default(),
                &mut state,
                &mut settings,
                &images,
            );
        },
    );
    output.textures_delta.clear();

    assert_eq!(panel.min, egui::pos2(OWNED_WORLDS_LEFT, OWNED_WORLDS_TOP));
    assert_eq!(panel.width(), OWNED_WORLDS_WIDTH + 20.0);
}

#[test]
fn strategic_hud_panels_scale_with_viewports() {
    fn panel_metrics(viewport: egui::Vec2) -> (egui::Rect, egui::Rect, egui::Vec2, egui::Vec2) {
        let context = egui::Context::default();
        context.set_global_style(NordDark.custom_style());
        let mut planet = Planet::new(0, "Masduk".to_string(), Vec2::ZERO, false, 1.0);
        planet.owned = Some(1);
        let planet_texture = egui::TextureId::User(6);
        let turn_texture = egui::TextureId::User(1);
        let images = ImageIds(HashMap::from([
            ("turn".to_string(), turn_texture),
            ("owned".to_string(), egui::TextureId::User(2)),
            ("metal".to_string(), egui::TextureId::User(3)),
            ("crystal".to_string(), egui::TextureId::User(4)),
            ("deuterium".to_string(), egui::TextureId::User(5)),
            (planet.image(), planet_texture),
        ]));
        let map = Map {
            rect: Rect::default(),
            solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
            planets: vec![planet],
        };
        let player = Player::new(1, 0);
        let mut state = UiState::default();
        let mut settings = Settings::default();
        let input = || egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
            ..default()
        };

        let mut warmup = context.run_ui(input(), |context| {
            draw_owned_worlds_widget(
                context,
                &map,
                &player,
                &MultiplayerSession::default(),
                &mut state,
                &mut settings,
                &images,
            );
            draw_resources_widget(context, &settings, &map, &player, &images, 0);
        });
        warmup.textures_delta.clear();

        let mut worlds = egui::Rect::NOTHING;
        let mut resources = egui::Rect::NOTHING;
        let mut output = context.run_ui(input(), |context| {
            worlds = draw_owned_worlds_widget(
                context,
                &map,
                &player,
                &MultiplayerSession::default(),
                &mut state,
                &mut settings,
                &images,
            );
            resources = draw_resources_widget(context, &settings, &map, &player, &images, 0);
        });
        output.textures_delta.clear();

        let turn_icon = image_rect(&output.shapes, turn_texture).expect("missing turn icon").size();
        let planet_icon =
            image_rect(&output.shapes, planet_texture).expect("missing planet icon").size();
        (worlds, resources, turn_icon, planet_icon)
    }

    let baseline = panel_metrics(egui::vec2(HUD_REFERENCE_WIDTH, HUD_REFERENCE_HEIGHT));
    let small =
        panel_metrics(egui::vec2(HUD_REFERENCE_WIDTH * 0.9, HUD_REFERENCE_HEIGHT * HUD_MIN_SCALE));
    let large = panel_metrics(egui::vec2(HUD_REFERENCE_WIDTH * 2.0, HUD_REFERENCE_HEIGHT * 2.0));

    assert_eq!(strategic_hud_scale(egui::vec2(800.0, 600.0)), HUD_MIN_SCALE);
    assert_eq!(
        strategic_hud_scale(egui::vec2(HUD_REFERENCE_WIDTH * 2.0, HUD_REFERENCE_HEIGHT * 2.0,)),
        HUD_MAX_SCALE
    );
    assert!(
        (large.0.width() / baseline.0.width() - HUD_MAX_SCALE).abs() < 0.02,
        "world panel did not scale proportionally: baseline={:?}, large={:?}",
        baseline.0,
        large.0
    );
    assert!(
        large.0.height() / baseline.0.height() > 1.5,
        "world panel did not scale proportionally: baseline={:?}, large={:?}",
        baseline.0,
        large.0
    );
    assert!(
        (large.1.width() / baseline.1.width() - HUD_MAX_SCALE).abs() < 0.02,
        "resource panel did not scale proportionally: baseline={:?}, large={:?}",
        baseline.1,
        large.1
    );
    assert_eq!(baseline.2, egui::vec2(64.0, 40.0));
    assert!((small.2 - baseline.2 * HUD_MIN_SCALE).length() < 0.01);
    assert!((large.2 - baseline.2 * HUD_MAX_SCALE).length() < 0.01);
    assert_eq!(baseline.3, egui::Vec2::splat(30.0));
    assert_eq!(small.3, baseline.3);
    assert_eq!(large.3, baseline.3 * HUD_MAX_SCALE);
    assert_eq!(small.0, baseline.0);
    assert!(small.1.height() < baseline.1.height());
    let expected_world_position = baseline.0.min * HUD_MAX_SCALE;
    assert!((large.0.min.x - expected_world_position.x).abs() < 1.0);
    assert!((large.0.min.y - expected_world_position.y).abs() < 1.0);
    assert!((large.1.top() - RESOURCE_BAR_TOP * HUD_MAX_SCALE).abs() < 1.0);
}

#[test]
fn resource_bar_uses_the_shared_hud_frame_without_an_image_texture() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let unused_texture = egui::TextureId::User(99);
    let images = ImageIds(HashMap::from([
        ("turn".to_string(), egui::TextureId::User(1)),
        ("owned".to_string(), egui::TextureId::User(2)),
        ("metal".to_string(), egui::TextureId::User(3)),
        ("crystal".to_string(), egui::TextureId::User(4)),
        ("deuterium".to_string(), egui::TextureId::User(5)),
        ("thin panel".to_string(), unused_texture),
    ]));
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: Vec::new(),
    };
    let player = Player::default();
    let settings = Settings::default();
    let viewport = egui::Rect::from_min_size(
        egui::Pos2::ZERO,
        egui::vec2(HUD_REFERENCE_WIDTH, HUD_REFERENCE_HEIGHT),
    );
    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };

    let mut warmup = context.run_ui(input(), |context| {
        draw_resources_widget(context, &settings, &map, &player, &images, 0);
    });
    warmup.textures_delta.clear();

    let mut panel = egui::Rect::NOTHING;
    let mut output = context.run_ui(input(), |context| {
        panel = draw_resources_widget(context, &settings, &map, &player, &images, 0);
    });
    output.textures_delta.clear();

    assert_eq!(panel.top(), RESOURCE_BAR_TOP);
    assert!(
        (panel.bottom() - resource_bar_bottom(viewport.size())).abs() < 1.0,
        "resource bar bottom calculation drifted from its rendered panel: {panel:?}"
    );
    assert!(panel.left() >= RESOURCE_BAR_SIDE_INSET);
    assert!(panel.right() <= viewport.right() - RESOURCE_BAR_SIDE_INSET);
    assert!(
        (panel.center().x - viewport.center().x).abs() < 1.0,
        "resource bar was not centered: {panel:?} in {viewport:?}"
    );
    assert!(
        (780.0..1_000.0).contains(&panel.width()),
        "resource bar did not use the intended larger content width: {panel:?}"
    );
    assert_eq!(hud_panel_frame().fill, HUD_PANEL_FILL);
    assert_eq!(hud_panel_frame().stroke.color, HUD_PANEL_STROKE);
    for label in ["TURN", "PLANETS", "METAL", "CRYSTAL", "DEUTERIUM", "ENERGY"] {
        text_rect(&output.shapes, label);
    }
    let turn_image = image_rect(&output.shapes, images.get("turn")).expect("missing turn image");
    assert_eq!(turn_image.size(), egui::vec2(64.0, 40.0));
    assert!((turn_image.center().y - panel.center().y).abs() < 1.0);
    let turn_label = text_rect(&output.shapes, "TURN");
    let turn_value = text_rect(&output.shapes, "1");
    let turn_text_center = (turn_label.top() + turn_value.bottom()) * 0.5;
    assert!(
        (turn_text_center - panel.center().y - RESOURCE_SUMMARY_TEXT_VERTICAL_OFFSET).abs() < 1.0,
        "turn text was not optically centered: label={turn_label:?}, value={turn_value:?}, panel={panel:?}"
    );
    let value_bottom_padding = panel.bottom() - text_rect(&output.shapes, "1500").bottom();
    assert!(
        value_bottom_padding < 10.0,
        "resource value retained too much bottom padding: {value_bottom_padding}"
    );
    let planets_to_metal =
        image_rect(&output.shapes, images.get("metal")).expect("missing metal image").left()
            - text_rect(&output.shapes, "PLANETS").right();
    let metal_to_crystal =
        image_rect(&output.shapes, images.get("crystal")).expect("missing crystal image").left()
            - text_rect(&output.shapes, "1500").right();
    assert!(
        (planets_to_metal - metal_to_crystal).abs() < 1.0,
        "resource blocks used unequal gaps: planets→metal={planets_to_metal}, metal→crystal={metal_to_crystal}"
    );
    assert_eq!(text_font_size(&output.shapes, "1500"), 28.0);
    assert!(text_rect(&output.shapes, "1500").height() > 28.0);
    assert!(
        image_rect(&output.shapes, unused_texture).is_none(),
        "the removed thin-panel texture was still painted"
    );
    assert!(
        output.shapes.iter().all(|shape| !shape_has_line_segment(&shape.shape)),
        "the resource section divider was still painted"
    );
}

#[test]
fn energy_summary_shows_signed_balance_and_colors_only_shortages_red() {
    for (energy, expected, color) in [
        (
            EnergyGrid {
                supply: 12,
                demand: 7,
            },
            "+5",
            Color32::WHITE,
        ),
        (
            EnergyGrid {
                supply: 7,
                demand: 12,
            },
            "-5",
            Color32::RED,
        ),
        (
            EnergyGrid {
                supply: 7,
                demand: 7,
            },
            "0",
            Color32::WHITE,
        ),
    ] {
        assert_eq!(energy_balance_text(energy), expected);
        assert_eq!(energy_balance_color(energy), color);

        let context = egui::Context::default();
        let mut output = context.run_ui(Default::default(), |ui| {
            draw_resource_summary_with_value_color(
                ui,
                egui::TextureId::Managed(0),
                "ENERGY",
                expected,
                color,
                false,
                1.0,
            );
        });
        output.textures_delta.clear();
        assert_eq!(text_color(&output.shapes, expected), color);
    }
}

#[test]
fn queued_construction_immediately_changes_the_top_bar_energy_balance() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let images = ImageIds(HashMap::from([
        ("turn".to_string(), egui::TextureId::User(1)),
        ("owned".to_string(), egui::TextureId::User(2)),
        ("metal".to_string(), egui::TextureId::User(3)),
        ("crystal".to_string(), egui::TextureId::User(4)),
        ("deuterium".to_string(), egui::TextureId::User(5)),
        ("energy".to_string(), egui::TextureId::User(6)),
    ]));
    let mut model = crate::core::simulation::GameModel::new([47; 32], Default::default()).unwrap();
    let home_planet = model.players[0].home_planet;
    model.map.get_mut(home_planet).buy.push(Unit::Building(Building::MetalMine));
    let player = &model.players[0];

    let mut output = context.run_ui(Default::default(), |ui| {
        draw_resources(ui, &Settings::default(), &model.map, player, &images, false, 1.0, 0);
    });
    output.textures_delta.clear();

    assert_eq!(player.energy_grid(&model.map).balance(), 0);
    assert_eq!(EnergyGrid::for_player_next_turn(player.id, &model.map).balance(), -1);
    assert!(has_text(&output.shapes, "-1"));
    assert_eq!(text_color(&output.shapes, "-1"), Color32::RED);
}

#[test]
fn committed_railguns_immediately_reduce_top_bar_energy_until_the_draft_resets() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let images = ImageIds(HashMap::from([
        ("turn".to_string(), egui::TextureId::User(1)),
        ("owned".to_string(), egui::TextureId::User(2)),
        ("metal".to_string(), egui::TextureId::User(3)),
        ("crystal".to_string(), egui::TextureId::User(4)),
        ("deuterium".to_string(), egui::TextureId::User(5)),
        ("energy".to_string(), egui::TextureId::User(6)),
    ]));
    let mut model = crate::core::simulation::GameModel::new([49; 32], Default::default()).unwrap();
    let player_id = model.players[0].id;
    let first = model.players[0].home_planet;
    let worlds = model
        .map
        .planets
        .iter()
        .filter(|planet| !planet.is_moon() && planet.owned.is_none())
        .map(|planet| planet.id)
        .take(2)
        .collect::<Vec<_>>();
    let second = worlds[0];
    let target = worlds[1];
    model.map.get_mut(second).colonize(player_id);
    let target_position = model.map.get(target).position;
    for (index, origin) in [first, second].into_iter().enumerate() {
        let planet = model.map.get_mut(origin);
        planet.position = target_position + Vec2::X * Planet::SIZE * (index as f32 + 1.0);
        planet.army.insert(Unit::Building(Building::OrbitalRailgun), 1);
    }

    let mut pending = PendingTurnCommands::default();
    pending.reset(model.turn);
    assert!(pending.push(TurnCommand::FireOrbitalRailguns {
        target
    }));
    let action_demand = pending_railgun_energy_demand(&model.map, player_id, &pending);
    assert_eq!(action_demand, 10);
    let expected =
        energy_balance_text(projected_energy(&model.map, &model.players[0], action_demand));

    let mut output = context.run_ui(Default::default(), |ui| {
        draw_resources(
            ui,
            &Settings::default(),
            &model.map,
            &model.players[0],
            &images,
            false,
            1.0,
            action_demand,
        );
    });
    output.textures_delta.clear();
    assert!(has_text(&output.shapes, &expected));

    pending.reset(model.turn.saturating_add(1));
    assert_eq!(pending_railgun_energy_demand(&model.map, player_id, &pending), 0);
}

#[test]
fn production_hover_breakdowns_follow_acquisition_order_and_only_show_world_names() {
    let mut model = crate::core::simulation::GameModel::new([46; 32], Default::default()).unwrap();
    let mut player = model.players[0].clone();
    for world in &mut model.map.planets {
        world.owned = None;
        world.controlled = None;
        world.army.clear();
    }
    let home_id = player.home_planet;
    let planet_id = model.map.planets().into_iter().find(|planet| planet.id != home_id).unwrap().id;
    let moon_id = model.map.moons().first().unwrap().id;

    let home = model.map.get_mut(home_id);
    home.name = "Home".into();
    home.colonize(player.id);
    home.army.insert(Unit::Building(Building::Reactor), 1);
    home.army.insert(Unit::Building(Building::MetalMine), 1);
    let planet = model.map.get_mut(planet_id);
    planet.name = "Colony".into();
    planet.colonize(player.id);
    planet.army.insert(Unit::Building(Building::MetalMine), 2);
    let moon = model.map.get_mut(moon_id);
    moon.name = "Darian".into();
    moon.control(player.id);
    moon.army.insert(Unit::Building(Building::TidalGenerator), 1);
    moon.army.insert(Unit::Building(Building::Laboratory), 1);
    player.world_acquisition_order = vec![home_id, moon_id, planet_id];

    let energy = energy_world_breakdown(&model.map, &player);
    assert_eq!(
        energy.iter().map(|(name, _)| name.as_str()).collect::<Vec<_>>(),
        ["Home", "Darian", "Colony"]
    );
    assert!(energy.iter().all(|(name, _)| !name.contains("(Moon)")));

    let metal = resource_world_breakdown(&model.map, &player, ResourceName::Metal, 0);
    assert_eq!(
        metal.iter().map(|world| world.name.as_str()).collect::<Vec<_>>(),
        ["Home", "Colony"]
    );
}

#[test]
fn energy_tooltip_shows_only_the_next_turn_net_balance() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let texture = context.load_texture(
        "energy tooltip test",
        egui::ColorImage::filled([1, 1], Color32::WHITE),
        default(),
    );
    let images = ImageIds(HashMap::from([("energy".to_string(), texture.id())]));
    let mut model = crate::core::simulation::GameModel::new([48; 32], Default::default()).unwrap();
    let home_planet = model.players[0].home_planet;
    model.map.get_mut(home_planet).buy.push(Unit::Building(Building::MetalMine));
    let player = &model.players[0];

    let mut output = context.run_ui(Default::default(), |ui| {
        draw_energy_tooltip(ui, &model.map, player, &images, 0);
    });
    output.textures_delta.clear();

    let production = text_rect(&output.shapes, "Production: -1");
    let description = text_rect(&output.shapes, ENERGY_DESCRIPTION);
    assert!(!has_text(&output.shapes, "Production: 3/4"));
    assert!(!has_text(&output.shapes, "Efficiency:"));
    assert!(!has_text(&output.shapes, "Production next turn"));
    assert!(has_text(&output.shapes, ENERGY_DESCRIPTION));
    assert!(description.top() > production.bottom());
    assert_eq!(ENERGY_DESCRIPTION, "Energy powers and maintains buildings across your empire.");
}

#[test]
fn hovering_energy_production_lists_planets_before_railgun_fire() {
    let mut planet = Planet::new(0, "Power Grid".into(), Vec2::ZERO, false, 1.0);
    planet.owned = Some(0);
    planet.army.insert(Unit::Building(Building::Reactor), 1);
    planet.army.insert(Unit::Building(Building::MetalMine), 1);
    planet.buy.push(Unit::Building(Building::Reactor));
    let mut unpowered = Planet::new(1, "Unpowered".into(), Vec2::ZERO, false, 1.0);
    unpowered.owned = Some(0);
    unpowered.army.insert(Unit::Building(Building::MetalMine), 3);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![planet, unpowered],
    };
    let player = Player::new(0, 0);
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_000.0, 300.0));

    let context = egui::Context::default();
    let mut target = egui::Rect::NOTHING;
    let input = |events| egui::RawInput {
        screen_rect: Some(viewport),
        events,
        ..default()
    };
    let mut warmup = context.run_ui(input(Vec::new()), |ui| {
        target = draw_energy_production_row(ui, &map, &player, 15).rect;
    });
    warmup.textures_delta.clear();
    let mut output =
        context.run_ui(input(vec![egui::Event::PointerMoved(target.center())]), |ui| {
            draw_energy_production_row(ui, &map, &player, 15);
        });
    output.textures_delta.clear();

    assert!(has_text(&output.shapes, "Production: -13"));
    assert!(!has_text(&output.shapes, "Production: 6/4"));
    assert!(!has_text(&output.shapes, "Production next turn"));
    assert!(has_text(&output.shapes, "Power Grid: +5"));
    assert_eq!(text_color(&output.shapes, "Power Grid: +5"), Color32::WHITE);
    assert_eq!(text_color(&output.shapes, "Unpowered: -3"), Color32::WHITE);
    let last_planet = text_rect(&output.shapes, "Unpowered: -3");
    let railgun_fire = text_rect(&output.shapes, "Railgun fire: -15");
    assert!(railgun_fire.top() > last_planet.bottom());
    assert!(!has_text(&output.shapes, "Committed Railgun fire"));
    assert!(!has_text(&output.shapes, "Production per planet"));
    assert!(!has_text(&output.shapes, "Efficiency:"));
}

#[test]
fn buying_a_building_that_crosses_below_zero_warns_about_next_turn() {
    let mine = Unit::Building(Building::MetalMine);
    let warning = shop::energy_shortage_warning(
        EnergyGrid {
            supply: 3,
            demand: 3,
        },
        mine,
        None,
    )
    .unwrap();
    assert_eq!(warning.message, "Next turn: Energy shortage.");
    assert_eq!(warning.level, crate::core::messages::MessageLevel::Warning);

    assert!(shop::energy_shortage_warning(
        EnergyGrid {
            supply: 1,
            demand: 0,
        },
        Unit::space_dock(),
        None,
    )
    .is_some());

    assert!(shop::energy_shortage_warning(
        EnergyGrid {
            supply: 3,
            demand: 4,
        },
        mine,
        None,
    )
    .is_none());
    assert!(shop::energy_shortage_warning(
        EnergyGrid {
            supply: 3,
            demand: 3,
        },
        Unit::Ship(Ship::LightFighter),
        None,
    )
    .is_none());
}

#[test]
fn resource_summaries_have_one_disjoint_hover_target_without_highlight_chrome() {
    let context = egui::Context::default();
    let mut style = NordDark.custom_style();
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    context.set_global_style(style);
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(500.0, 120.0));
    let draw = |ui: &mut Ui, targets: &mut Vec<egui::Rect>| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = egui::Vec2::ZERO;
            let first =
                draw_resource_summary(ui, egui::TextureId::User(1), "METAL", "1500", false, 1.0);
            targets.push(first.rect);
            first.on_hover_ui(|ui| {
                ui.label("FIRST RESOURCE TOOLTIP");
            });
            draw_resource_gap(ui, resource_bar_gap(false, 1.0), 1.0);
            let second =
                draw_resource_summary(ui, egui::TextureId::User(2), "CRYSTAL", "1200", false, 1.0);
            targets.push(second.rect);
            second.on_hover_ui(|ui| {
                ui.label("SECOND RESOURCE TOOLTIP");
            });
        });
    };
    let input = |events| egui::RawInput {
        screen_rect: Some(viewport),
        events,
        ..default()
    };

    let mut targets = Vec::new();
    let mut warmup = context.run_ui(input(Vec::new()), |ui| draw(ui, &mut targets));
    warmup.textures_delta.clear();
    let pointer = targets[0].center();
    targets.clear();

    let mut hover_start = context
        .run_ui(input(vec![egui::Event::PointerMoved(pointer)]), |ui| draw(ui, &mut targets));
    hover_start.textures_delta.clear();
    targets.clear();
    let mut output = context.run_ui(input(Vec::new()), |ui| draw(ui, &mut targets));
    output.textures_delta.clear();

    assert!(targets[0].right() < targets[1].left());
    assert!(has_text(&output.shapes, "FIRST RESOURCE TOOLTIP"));
    assert!(!has_text(&output.shapes, "SECOND RESOURCE TOOLTIP"));
    let removed_hover_fill = Color32::from_rgba_unmultiplied(130, 170, 215, 18);
    assert!(
        output.shapes.iter().all(|shape| !shape_has_fill(&shape.shape, removed_hover_fill)),
        "resource hover still painted a visible container"
    );
}

#[test]
fn resource_tooltip_shows_queued_production_as_production() {
    let context = egui::Context::default();
    let mut style = NordDark.custom_style();
    style.interaction.tooltip_delay = 0.0;
    style.interaction.show_tooltips_only_when_still = false;
    context.set_global_style(style);
    let texture = context.load_texture(
        "resource tooltip test metal",
        egui::ColorImage::filled([1, 1], Color32::WHITE),
        default(),
    );
    let metal_texture = texture.id();
    let images = ImageIds(HashMap::from([("metal".to_string(), metal_texture)]));
    let mut planet = Planet::new(0, "Foundry".into(), Vec2::ZERO, false, 1.0);
    planet.owned = Some(0);
    planet.resources = crate::core::resources::Resources::new(10, 0, 0);
    planet.army.insert(Unit::Building(Building::MetalMine), 1);
    planet.buy.push(Unit::Building(Building::MetalMine));
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![planet],
    };
    let player = Player::new(0, 0);
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_000.0, 300.0));
    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };
    let mut tooltip_image = egui::Rect::NOTHING;
    let mut warmup = context.run_ui(input(), |ui| {
        tooltip_image = draw_resource_tooltip(ui, ResourceName::Metal, &map, &player, &images, 0);
    });
    warmup.textures_delta.clear();
    let mut output = context.run_ui(input(), |ui| {
        tooltip_image = draw_resource_tooltip(ui, ResourceName::Metal, &map, &player, &images, 0);
    });
    output.textures_delta.clear();

    assert!(has_text(&output.shapes, "Metal"));
    assert!(has_text(&output.shapes, "Production: +16"));
    assert!(!has_text(&output.shapes, "Production next turn"));
    assert!(!has_text(&output.shapes, "Production: +9"));
    assert_eq!(text_color(&output.shapes, "(-20%)"), Color32::RED);
    assert_eq!(tooltip_image.size(), egui::vec2(130.0, 90.0));
}

#[test]
fn hovering_production_expands_the_next_turn_planet_breakdown() {
    let mut planet = Planet::new(0, "Focused".into(), Vec2::ZERO, false, 1.0);
    planet.owned = Some(0);
    planet.resources = crate::core::resources::Resources::new(10, 0, 0);
    planet.army.insert(Unit::Building(Building::MetalMine), 1);
    planet.army.insert(Unit::Building(Building::Reactor), 2);
    planet.army.insert(Unit::Building(Building::Terraformer), 1);
    planet.buy.push(Unit::Building(Building::Terraformer));
    planet.terraformer_focus = Some(ResourceName::Metal);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![planet],
    };
    let player = Player::new(0, 0);
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_000.0, 300.0));

    let context = egui::Context::default();
    let mut target = egui::Rect::NOTHING;
    let input = |events| egui::RawInput {
        screen_rect: Some(viewport),
        events,
        ..default()
    };
    let mut warmup = context.run_ui(input(Vec::new()), |ui| {
        target = draw_resource_production_row(ui, &map, &player, ResourceName::Metal, 0).rect;
    });
    warmup.textures_delta.clear();
    let mut output =
        context.run_ui(input(vec![egui::Event::PointerMoved(target.center())]), |ui| {
            draw_resource_production_row(ui, &map, &player, ResourceName::Metal, 0);
        });
    output.textures_delta.clear();

    assert!(has_text(&output.shapes, "Focused: +12"));
    assert!(!has_text(&output.shapes, "Production per planet"));
    assert!(!has_text(&output.shapes, "Production next turn per planet"));
    assert!(!has_text(&output.shapes, "Efficiency:"));
    assert!(!has_text(&output.shapes, "Terraformer"));
    assert!(!has_text(&output.shapes, "(100%)"));
    assert_eq!(text_color(&output.shapes, "(+20%)"), HEALTH_COLOR.to_color32());
}

#[test]
fn resource_breakdown_colors_each_planets_terraformer_modifier() {
    let mut focused = Planet::new(0, "Focused".into(), Vec2::ZERO, false, 1.0);
    focused.owned = Some(0);
    focused.resources = crate::core::resources::Resources::new(10, 10, 0);
    focused.army.insert(Unit::Building(Building::MetalMine), 1);
    focused.army.insert(Unit::Building(Building::CrystalMine), 1);
    focused.army.insert(Unit::Building(Building::Reactor), 2);
    focused.army.insert(Unit::Building(Building::Terraformer), 1);
    focused.buy.push(Unit::Building(Building::Terraformer));
    focused.terraformer_focus = Some(ResourceName::Metal);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![focused],
    };
    let player = Player::new(0, 0);

    let metal = resource_world_breakdown(&map, &player, ResourceName::Metal, 0);
    assert_eq!(metal[0].amount, 12);
    assert_eq!(metal[0].terraformer_modifier_percent, 20);
    let crystal = resource_world_breakdown(&map, &player, ResourceName::Crystal, 0);
    assert_eq!(crystal[0].amount, 8);
    assert_eq!(crystal[0].terraformer_modifier_percent, -20);

    let context = egui::Context::default();
    let mut output = context.run_ui(Default::default(), |ui| {
        draw_resource_world_breakdown(ui, &map, &player, ResourceName::Metal, 0);
        draw_resource_world_breakdown(ui, &map, &player, ResourceName::Crystal, 0);
    });
    output.textures_delta.clear();
    assert!(!has_text(&output.shapes, "Terraformer"));
    assert_eq!(text_color(&output.shapes, "(+20%)"), HEALTH_COLOR.to_color32());
    assert_eq!(text_color(&output.shapes, "(-20%)"), Color32::RED);
}

#[test]
fn resource_breakdown_hides_terraformers_without_an_active_modifier() {
    let mut planet = Planet::new(0, "Quiet World".into(), Vec2::ZERO, false, 1.0);
    planet.owned = Some(0);
    planet.resources = crate::core::resources::Resources::new(10, 0, 0);
    planet.army.insert(Unit::Building(Building::MetalMine), 1);
    planet.army.insert(Unit::Building(Building::Terraformer), 5);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![planet],
    };
    let player = Player::new(0, 0);

    let breakdown = resource_world_breakdown(&map, &player, ResourceName::Metal, 0);
    assert_eq!(breakdown[0].terraformer_modifier_percent, 0);
    assert_eq!(breakdown[0].amount, 4);

    let context = egui::Context::default();
    let mut output = context.run_ui(Default::default(), |ui| {
        draw_resource_world_breakdown(ui, &map, &player, ResourceName::Metal, 0);
    });
    output.textures_delta.clear();
    assert!(has_text(&output.shapes, "Quiet World: +4"));
    assert!(!has_text(&output.shapes, "Terraformer"));
    assert!(!has_text(&output.shapes, "(0%)"));
}

#[test]
fn controlled_world_shortcut_keeps_the_world_selected_as_a_mission_origin() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let mut planet = Planet::new(7, "Forward Base".to_string(), Vec2::ZERO, false, 1.0);
    planet.controlled = Some(1);
    let images = ImageIds(HashMap::from([(planet.image(), egui::TextureId::User(1))]));
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![planet],
    };
    let player = Player::new(1, 0);
    let mut state = UiState {
        mission: true,
        combat_report: Some(3),
        focus_planet: Some(2),
        ..default()
    };
    let mut settings = Settings {
        show_menu: false,
        ..default()
    };
    let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1_280.0, 720.0));
    {
        let mut frame = |events| {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(screen),
                    events,
                    ..default()
                },
                |context| {
                    draw_owned_worlds_widget(
                        context,
                        &map,
                        &player,
                        &MultiplayerSession::default(),
                        &mut state,
                        &mut settings,
                        &images,
                    );
                },
            );
            output.textures_delta.clear();
            output
        };

        frame(Vec::new());
        let output = frame(Vec::new());
        let position = text_rect(&output.shapes, "Forward Base").center();
        frame(vec![
            egui::Event::PointerMoved(position),
            egui::Event::PointerButton {
                pos: position,
                button: egui::PointerButton::Primary,
                pressed: true,
                modifiers: default(),
            },
        ]);
        frame(vec![egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed: false,
            modifiers: default(),
        }]);
    }

    assert_eq!(state.planet_selected, Some(7));
    assert_eq!(state.mission_info.origin, 7);
    assert_eq!(state.focus_planet, None);
    assert!(state.to_selected);
    assert!(!state.mission);
    assert_eq!(state.combat_report, None);
    assert!(settings.show_menu);
}

#[test]
fn mission_planet_hover_uses_a_units_only_panel_without_replacing_click_selection() {
    let state = UiState {
        mission_planet_hover: Some(3),
        planet_hover: Some(2),
        planet_selected: Some(1),
        ..default()
    };

    assert_eq!(visible_planet_panel(&state), Some((3, PlanetPanelMode::UnitsOnly)));
    assert_eq!(state.planet_selected, Some(1));
}

#[test]
fn planet_details_follow_hover_without_pinning_the_selected_world() {
    let mut state = UiState {
        planet_selected: Some(1),
        ..default()
    };
    assert_eq!(visible_planet_panel(&state), None);
    for id in [1, 2] {
        state.planet_hover = Some(id);
        assert_eq!(visible_planet_panel(&state), Some((id, PlanetPanelMode::Full)));
        state.planet_hover = None;
        assert_eq!(visible_planet_panel(&state), None);
        assert_eq!(state.planet_selected, Some(1));
    }
}

#[test]
fn planet_hover_switches_sides_even_with_a_selected_mission_origin() {
    for selected in [None, Some(1), Some(2)] {
        for units_only in [false, true] {
            let state = UiState {
                planet_selected: selected,
                planet_hover: Some(2),
                mission_planet_hover: units_only.then_some(3),
                ..default()
            };
            for (cursor_x, right_side) in [(100.0, true), (611.5, false), (670.0, false)] {
                let target = planet_hover_panel_target(&state, Some(cursor_x), 1223.0).unwrap();
                assert_eq!(target.right_side, right_side);
                assert_eq!(
                    target.id,
                    if units_only {
                        3
                    } else {
                        2
                    }
                );
                assert_eq!(
                    target.mode,
                    if units_only {
                        PlanetPanelMode::UnitsOnly
                    } else {
                        PlanetPanelMode::Full
                    }
                );
            }
        }
    }
}

#[test]
fn planet_panel_slide_restarts_for_each_world_and_eases_from_its_map_edge() {
    let left_target = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: false,
    };
    let right_target = PlanetPanelSlideTarget {
        id: 2,
        mode: PlanetPanelMode::Full,
        right_side: true,
    };
    let mut slide = PlanetPanelSlide::default();

    assert_eq!(slide.update(Some(left_target), 0.0), Some((left_target, 0.0)));
    let (_, halfway) = slide.update(Some(left_target), PLANET_PANEL_SLIDE_DURATION * 0.5).unwrap();
    assert_eq!(halfway, 0.5);
    assert_eq!(planet_panel_slide_offset(halfway, false, 600.0), -75.0);
    assert_eq!(planet_panel_slide_offset(halfway, true, 600.0), 75.0);
    assert_eq!(
        slide.update(Some(left_target), PLANET_PANEL_SLIDE_DURATION),
        Some((left_target, 1.0))
    );

    assert_eq!(
        slide.update(Some(right_target), PLANET_PANEL_SLIDE_DURATION),
        Some((right_target, 0.0))
    );
    slide.hide();
    assert_eq!(
        slide.update(Some(right_target), PLANET_PANEL_SLIDE_DURATION),
        Some((right_target, 0.0))
    );
}

#[test]
fn planet_panel_rewinds_its_entrance_when_hidden() {
    let target = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: false,
    };
    let mut slide = PlanetPanelSlide::default();

    slide.update(Some(target), 0.0);
    slide.update(Some(target), PLANET_PANEL_TOTAL_DURATION);
    assert!(!slide.is_animating());

    let (_, progress) = slide.update(None, PLANET_PANEL_SLIDE_DURATION * 0.5).unwrap();
    assert_eq!(progress, 1.0);
    assert!(slide.detail_progress(PLANET_DETAIL_LINE_COUNT - 1) < 1.0);
    assert!(slide.is_animating());

    let (_, progress) =
        slide.update(None, PLANET_PANEL_TOTAL_DURATION - PLANET_PANEL_SLIDE_DURATION).unwrap();
    assert!((progress - 0.5).abs() < f32::EPSILON);
    assert!((planet_panel_slide_offset(progress, false, 600.0) + 75.0).abs() < 0.001);

    assert_eq!(slide.update(None, PLANET_PANEL_SLIDE_DURATION * 0.5), None);
    assert!(!slide.is_animating());
}

#[test]
fn planet_detail_lines_start_after_the_panel_and_follow_from_top_to_bottom() {
    let target = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: true,
    };
    let mut slide = PlanetPanelSlide::default();

    slide.update(Some(target), 0.0);
    slide.update(Some(target), PLANET_PANEL_SLIDE_DURATION);
    assert_eq!(slide.detail_progress(0), 0.0);
    assert_eq!(slide.detail_progress(1), 0.0);

    slide.update(Some(target), PLANET_DETAIL_LINE_STAGGER * 0.5);
    assert!(slide.detail_progress(0) > 0.0);
    assert_eq!(slide.detail_progress(1), 0.0);

    slide.update(Some(target), PLANET_DETAIL_LINE_STAGGER);
    assert!(slide.detail_progress(0) > slide.detail_progress(1));
    assert!(slide.detail_progress(1) > 0.0);
    assert_eq!(slide.detail_progress(2), 0.0);

    slide.update(Some(target), PLANET_PANEL_TOTAL_DURATION);
    for line in 0..PLANET_DETAIL_LINE_COUNT {
        assert_eq!(slide.detail_progress(line), 1.0);
    }
    assert!(!slide.is_animating());
}

#[test]
fn planet_panel_survives_pointer_transfer_and_stays_open_while_hovered() {
    let target = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: true,
    };
    let panel_rect = egui::Rect::from_min_size(egui::pos2(700.0, 100.0), egui::vec2(500.0, 600.0));
    let mut hold = PlanetPanelHoverHold::default();

    assert_eq!(hold.update(Some(target), Some(egui::pos2(200.0, 300.0)), 0.0), Some(target));
    hold.set_panel_rects([Some(panel_rect), None]);

    assert_eq!(
        hold.update(None, Some(egui::pos2(500.0, 300.0)), PLANET_PANEL_HOVER_HOLD_DURATION * 0.75,),
        Some(target),
    );
    assert_eq!(
        hold.update(None, Some(panel_rect.center()), PLANET_PANEL_HOVER_HOLD_DURATION,),
        Some(target),
    );
    assert_eq!(
        hold.update(None, Some(egui::pos2(500.0, 300.0)), PLANET_PANEL_HOVER_HOLD_DURATION + 0.01,),
        None,
    );
}

#[test]
fn full_planet_panel_keeps_its_side_while_crossing_the_screen_midpoint() {
    let right = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: true,
    };
    let recomputed_left = PlanetPanelSlideTarget {
        right_side: false,
        ..right
    };
    let mut hold = PlanetPanelHoverHold::default();

    assert_eq!(hold.update(Some(right), Some(egui::pos2(500.0, 300.0)), 0.0), Some(right));
    assert_eq!(
        hold.update(Some(recomputed_left), Some(egui::pos2(700.0, 300.0)), 0.1),
        Some(right)
    );
}

#[test]
fn quick_planet_crossings_do_not_steal_an_open_hover_panel() {
    let open = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: true,
    };
    let crossed = PlanetPanelSlideTarget {
        id: 2,
        mode: PlanetPanelMode::Full,
        right_side: false,
    };
    let panel_rect = egui::Rect::from_min_size(egui::pos2(700.0, 100.0), egui::vec2(500.0, 600.0));
    let mut hold = PlanetPanelHoverHold::default();

    assert_eq!(hold.update(Some(open), Some(egui::pos2(300.0, 300.0)), 0.0), Some(open));
    hold.set_panel_rects([Some(panel_rect), None]);

    assert_eq!(
        hold.update(
            Some(crossed),
            Some(egui::pos2(550.0, 300.0)),
            PLANET_PANEL_HOVER_SWITCH_DELAY * 0.5,
        ),
        Some(open),
    );
    assert_eq!(hold.update(Some(crossed), Some(panel_rect.center()), 0.0), Some(open),);
}

#[test]
fn resting_on_another_planet_deliberately_switches_the_hover_panel() {
    let open = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: true,
    };
    let next = PlanetPanelSlideTarget {
        id: 2,
        mode: PlanetPanelMode::Full,
        right_side: false,
    };
    let mut hold = PlanetPanelHoverHold::default();

    assert_eq!(hold.update(Some(open), None, 0.0), Some(open));
    assert_eq!(hold.update(Some(next), None, 0.0), Some(open));
    assert_eq!(hold.update(Some(next), None, PLANET_PANEL_HOVER_SWITCH_DELAY), Some(next),);
}

#[test]
fn mission_planet_preview_does_not_latch_the_full_map_panel() {
    let full = PlanetPanelSlideTarget {
        id: 1,
        mode: PlanetPanelMode::Full,
        right_side: true,
    };
    let units_only = PlanetPanelSlideTarget {
        id: 2,
        mode: PlanetPanelMode::UnitsOnly,
        right_side: false,
    };
    let mut hold = PlanetPanelHoverHold::default();

    assert_eq!(hold.update(Some(full), None, 0.0), Some(full));
    assert_eq!(hold.update(Some(units_only), None, 0.0), Some(units_only));
    assert_eq!(hold.update(None, None, 0.0), None);
}

#[test]
fn mission_hover_panels_stay_opposite_the_pointer() {
    const VIEWPORT_WIDTH: f32 = 1_200.0;

    for pointer_x in [100.0, 1_100.0] {
        let (fleet_x, info_x) = mission_hover_panel_x_positions(Some(pointer_x), VIEWPORT_WIDTH);
        let fleet = egui::Rect::from_min_size(
            egui::pos2(fleet_x, 0.0),
            egui::vec2(MISSION_HOVER_FLEET_WIDTH, 630.0),
        );
        let info = egui::Rect::from_min_size(
            egui::pos2(info_x, 0.0),
            egui::vec2(MISSION_HOVER_INFO_WIDTH, 280.0),
        );

        assert!(!fleet.x_range().contains(pointer_x));
        assert!(!info.x_range().contains(pointer_x));
        assert_eq!(fleet_x > VIEWPORT_WIDTH * 0.5, pointer_x < VIEWPORT_WIDTH * 0.5);
    }
}

#[test]
fn mission_list_hover_hides_route_details_but_map_hover_keeps_them() {
    assert!(!mission_hover_shows_info_panel(true));
    assert!(mission_hover_shows_info_panel(false));
}

#[test]
fn abandon_confirmation_is_centered_and_reuses_the_planet_panel_texture() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let panel_texture = egui::TextureId::User(9);
    let abandon_texture = egui::TextureId::User(10);
    let images = ImageIds(HashMap::from([
        ("panel".to_string(), panel_texture),
        ("abandon".to_string(), abandon_texture),
    ]));
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 260.0));
    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };
    context.begin_pass(input());
    assert_eq!(draw_abandon_confirmation(&context, &images, "Masduk"), None);
    let mut warmup = context.end_pass();
    warmup.textures_delta.clear();

    context.begin_pass(input());
    assert_eq!(draw_abandon_confirmation(&context, &images, "Masduk"), None);
    let content_center = context.content_rect().center();
    let mut output = context.end_pass();
    output.textures_delta.clear();

    let panel = image_rect(&output.shapes, panel_texture).expect("missing modal panel image");
    assert!(
        (panel.center().x - content_center.x).abs() < 1.0,
        "panel {panel:?}, content center {content_center:?}"
    );
    assert!(
        (panel.center().y - content_center.y).abs() < 1.0,
        "panel {panel:?}, content center {content_center:?}"
    );
    assert!(viewport.contains_rect(panel));
    assert_eq!(panel.size(), egui::vec2(388.0, 228.0));
    let icon = image_rect(&output.shapes, abandon_texture).expect("missing abandon icon");
    assert!(panel.contains_rect(icon));
    assert!(icon.right() > panel.center().x && icon.top() < panel.center().y);
    assert!((icon.top() - panel.top() - MODAL_ICON_TOP_INSET).abs() < 1.0);
    assert!((panel.right() - icon.right() - MODAL_ICON_RIGHT_INSET).abs() < 1.0);
    let title = text_rect(&output.shapes, "ABANDON PLANET");
    assert!((title.center().x - panel.center().x).abs() < 1.0);
    assert_eq!(text_color(&output.shapes, "ABANDON PLANET"), ABANDON_CONFIRMATION_TEXT_COLOR);
    for text in ["ABANDON PLANET", "Are you sure you want to abandon planet Masduk?", "Yes", "No"] {
        let label = text_rect(&output.shapes, text);
        assert!(panel.contains_rect(label), "modal text `{text}` was outside {panel:?}: {label:?}");
    }
    assert_eq!(
        text_color(&output.shapes, "Are you sure you want to abandon planet Masduk?",),
        Color32::WHITE
    );

    let mut painted_rects = Vec::new();
    for shape in &output.shapes {
        collect_rects(&shape.shape, &mut painted_rects);
    }
    let buttons = painted_rects
        .iter()
        .filter(|button| {
            (button.width() - 96.0).abs() < 1.0
                && (button.height() - MODAL_BUTTON_HEIGHT).abs() < 1.0
        })
        .collect::<Vec<_>>();
    assert_eq!(
        buttons.len(),
        2,
        "expected two styled confirmation buttons; painted rectangles: {painted_rects:?}"
    );
    assert!(buttons.iter().all(|button| panel.contains_rect(**button)));
    let button_row = buttons[0].union(*buttons[1]);
    assert!((button_row.center().x - panel.center().x).abs() < 1.0);

    let removed = "The buildings on Masduk will remain, but its defenses will be destroyed.";
    assert!(!has_text(&output.shapes, removed), "removed modal text `{removed}` was still shown");
}

#[test]
fn colonize_confirmation_matches_the_abandon_confirmation_pattern() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let panel_texture = egui::TextureId::User(11);
    let colonize_texture = egui::TextureId::User(12);
    let images = ImageIds(HashMap::from([
        ("panel".to_string(), panel_texture),
        ("colonize".to_string(), colonize_texture),
    ]));
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(420.0, 260.0));
    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };

    context.begin_pass(input());
    assert_eq!(draw_colonize_confirmation(&context, &images, "Falix"), None);
    let mut warmup = context.end_pass();
    warmup.textures_delta.clear();

    context.begin_pass(input());
    assert_eq!(draw_colonize_confirmation(&context, &images, "Falix"), None);
    let mut output = context.end_pass();
    output.textures_delta.clear();

    let panel = image_rect(&output.shapes, panel_texture).expect("missing modal panel image");
    let icon = image_rect(&output.shapes, colonize_texture).expect("missing colonize icon");
    assert!(viewport.contains_rect(panel));
    assert!(panel.contains_rect(icon));
    assert!((icon.top() - panel.top() - MODAL_ICON_TOP_INSET).abs() < 1.0);
    assert!((panel.right() - icon.right() - MODAL_ICON_RIGHT_INSET).abs() < 1.0);
    for text in ["COLONIZE PLANET", "Are you sure you want to colonize planet Falix?", "Yes", "No"]
    {
        assert!(
            panel.contains_rect(text_rect(&output.shapes, text)),
            "modal text `{text}` escaped the panel"
        );
    }
}

#[test]
fn railgun_confirmation_centers_its_costs_and_keeps_all_content_inside_the_panel() {
    let context = egui::Context::default();
    context.set_global_style(NordDark.custom_style());
    let panel_texture = egui::TextureId::User(9);
    let deuterium_texture = context.load_texture(
        "railgun confirmation deuterium",
        egui::ColorImage::filled([1, 1], Color32::WHITE),
        default(),
    );
    let energy_texture = context.load_texture(
        "railgun confirmation energy",
        egui::ColorImage::filled([1, 1], Color32::WHITE),
        default(),
    );
    let railgun_texture = egui::TextureId::User(10);
    let images = ImageIds(HashMap::from([
        ("panel".to_string(), panel_texture),
        ("deuterium".to_string(), deuterium_texture.id()),
        ("energy".to_string(), energy_texture.id()),
        ("railgun strike".to_string(), railgun_texture),
    ]));
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 340.0));
    let input = || egui::RawInput {
        screen_rect: Some(viewport),
        ..default()
    };

    context.begin_pass(input());
    assert_eq!(
        draw_railgun_confirmation(&context, &images, "Ulmar", 1, 1_000, 5, 2_500, true),
        None
    );
    let mut warmup = context.end_pass();
    warmup.textures_delta.clear();

    context.begin_pass(input());
    assert_eq!(
        draw_railgun_confirmation(&context, &images, "Ulmar", 1, 1_000, 5, 2_500, true),
        None
    );
    let mut output = context.end_pass();
    output.textures_delta.clear();

    let panel = image_rect(&output.shapes, panel_texture).expect("missing modal panel image");
    let deuterium =
        image_rect(&output.shapes, deuterium_texture.id()).expect("missing deuterium icon");
    let energy = image_rect(&output.shapes, energy_texture.id()).expect("missing energy icon");
    let railgun = image_rect(&output.shapes, railgun_texture).expect("missing railgun icon");
    let deuterium_amount = text_rect(&output.shapes, "1.000");
    let energy_amount = text_rect(&output.shapes, "5");
    let cost_row = deuterium.union(deuterium_amount).union(energy).union(energy_amount);
    let title = text_rect(&output.shapes, "ORBITAL RAILGUN STRIKE");
    let heading = text_rect(&output.shapes, "Fire every available Railgun at Ulmar?");
    let railgun_details = text_rect(&output.shapes, "Orbital Railguns firing: 1");
    let chance_details = text_rect(&output.shapes, "Destruction chance: 25%");

    assert!(viewport.contains_rect(panel));
    assert!(panel.contains_rect(railgun));
    assert!(railgun.right() > panel.center().x && railgun.top() < panel.center().y);
    assert!((title.center().x - panel.center().x).abs() < 1.0);
    assert_eq!(
        text_color(&output.shapes, "ORBITAL RAILGUN STRIKE"),
        ABANDON_CONFIRMATION_TEXT_COLOR
    );
    assert!((cost_row.center().x - panel.center().x).abs() < 1.0);
    assert!(cost_row.top() - heading.bottom() >= 10.0);
    assert!(railgun_details.top() - cost_row.bottom() >= 13.0);
    assert!(chance_details.top() - railgun_details.bottom() >= 4.0);
    assert_eq!(text_color(&output.shapes, "1.000"), Color32::WHITE);
    assert_eq!(text_color(&output.shapes, "5"), Color32::WHITE);
    assert_eq!(text_font_size(&output.shapes, "1.000"), 20.0);
    assert_eq!(text_font_size(&output.shapes, "5"), 20.0);
    assert!(!has_text(&output.shapes, "Cost"));

    for text in [
        "ORBITAL RAILGUN STRIKE",
        "Fire every available Railgun at Ulmar?",
        "1.000",
        "5",
        "Orbital Railguns firing: 1",
        "Destruction chance: 25%",
        "Yes",
        "No",
    ] {
        let label = text_rect(&output.shapes, text);
        assert!(panel.contains_rect(label), "modal text `{text}` was outside {panel:?}: {label:?}");
    }
    assert!(panel.contains_rect(deuterium));
    assert!(panel.contains_rect(energy));

    let mut painted_rects = Vec::new();
    for shape in &output.shapes {
        collect_rects(&shape.shape, &mut painted_rects);
    }
    let buttons = painted_rects
        .iter()
        .filter(|button| {
            (button.width() - 96.0).abs() < 1.0
                && (button.height() - MODAL_BUTTON_HEIGHT).abs() < 1.0
        })
        .collect::<Vec<_>>();
    assert_eq!(buttons.len(), 2, "expected two styled confirmation buttons");
    let button_row = buttons[0].union(*buttons[1]);
    assert!((button_row.center().x - panel.center().x).abs() < 1.0);
    assert!(panel.contains_rect(button_row));
}
