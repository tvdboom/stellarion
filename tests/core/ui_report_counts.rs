use super::*;

use crate::core::combat::report::{CombatReport, DefenderRetreat};
use crate::core::combat::resolution::ShotReport;
use crate::test_support::empty_report;

fn label_rect(shapes: &[egui::epaint::ClippedShape], text: &str, color: Color32) -> egui::Rect {
    shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(label) if label.galley.text() == text => {
                assert!(label
                    .galley
                    .job
                    .sections
                    .iter()
                    .all(|section| section.format.color == color));
                Some(label.galley.rect.translate(label.pos.to_vec2()))
            },
            _ => None,
        })
        .unwrap_or_else(|| panic!("missing {text} label"))
}

fn assert_counts(output: &egui::FullOutput, remaining: usize, lost: usize) -> egui::Rect {
    let icon = output
        .shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill_texture_id() == egui::TextureId::User(1) => {
                Some(rect.rect)
            },
            egui::Shape::Mesh(mesh) if mesh.texture_id == egui::TextureId::User(1) => {
                Some(mesh.calc_bounds())
            },
            _ => None,
        })
        .expect("unit icon");
    let remaining = label_rect(&output.shapes, &remaining.to_string(), Color32::WHITE);
    assert!(remaining.left() >= icon.left() && remaining.center().x < icon.center().x);
    assert!(remaining.center().y > icon.center().y && remaining.bottom() <= icon.bottom() + 2.0);
    if lost > 0 {
        let lost = label_rect(&output.shapes, &format!("-{lost}"), Color32::RED);
        assert!(lost.center().x > icon.center().x && lost.right() <= icon.right());
        assert!(lost.top() >= icon.top() - 2.0 && lost.center().y < icon.center().y);
        assert!(!remaining.intersects(lost));
    } else {
        assert!(!output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(label) if label.galley.text() == "-0")
        }));
    }
    icon
}

#[test]
fn mission_reports_show_remaining_and_losses_for_both_combat_sides_and_bombed_buildings() {
    for objective in [
        Icon::Attack,
        Icon::Colonize,
        Icon::Destroy,
        Icon::Raid,
        Icon::Spy,
        Icon::Sabotage,
        Icon::Expedition,
        Icon::Patrol,
        Icon::Intercept,
        Icon::MissileStrike,
    ] {
        for (unit, side) in [
            (Unit::Ship(Ship::LightFighter), Side::Attacker),
            (Unit::Ship(Ship::LightFighter), Side::Defender),
            (Unit::Defense(Defense::RocketLauncher), Side::Defender),
            (Unit::Building(Building::MetalMine), Side::Defender),
        ] {
            for (remaining, lost) in [(2, 8), (0, 10), (10, 0)] {
                let mut planet = Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
                planet.owned = Some(2);
                planet.controlled = Some(2);
                planet.army.insert(unit, 10);
                let mut report = empty_report(
                    Mission {
                        owner: 1,
                        objective,
                        army: Army::from([(unit, 10)]),
                        ..default()
                    },
                    planet,
                );
                report.surviving_attacker = Army::from([(unit, remaining)]);
                report.surviving_defender = Army::from([(unit, remaining)]).into();
                report.combat_report = Some(CombatReport::default());
                let player = Player::new(
                    if side == Side::Attacker {
                        1
                    } else {
                        2
                    },
                    0,
                );
                let images =
                    ImageIds(HashMap::from([(unit.to_lowername(), egui::TextureId::User(1))]));
                for (size, style) in [(40.0, TextStyle::Small), (65.0, TextStyle::Body)] {
                    let context = egui::Context::default();
                    let mut output = context.run_ui(egui::RawInput::default(), |ui| {
                        draw_mission_report_unit(
                            ui,
                            &unit,
                            &report,
                            &player,
                            side.clone(),
                            (size, style.clone()),
                            &images,
                        );
                    });
                    output.textures_delta.clear();
                    assert_counts(&output, remaining, lost);
                }
            }
        }
    }
}

#[test]
fn withdrawn_defenders_count_as_survivors_in_mission_reports() {
    let unit = Unit::Ship(Ship::LightFighter);
    let mut planet = Planet::new(1, "Target".into(), Vec2::X, false, 1.0);
    planet.controlled = Some(2);
    planet.army.insert(unit, 10);
    let mut report = empty_report(Mission::default(), planet);
    report.surviving_defender = Army::from([(unit, 1)]).into();
    report.combat_report = Some(CombatReport {
        defender_retreat: Some(DefenderRetreat {
            after_round: Some(0),
            home_planet: 2,
            ships: Army::from([(unit, 1)]),
            fleets: BTreeMap::new(),
        }),
        ..default()
    });
    let images = ImageIds(HashMap::from([(unit.to_lowername(), egui::TextureId::User(1))]));
    let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
        draw_mission_report_unit(
            ui,
            &unit,
            &report,
            &Player::new(2, 0),
            Side::Defender,
            (65.0, TextStyle::Body),
            &images,
        );
    });
    output.textures_delta.clear();
    assert_counts(&output, 2, 8);
}

fn combatant(id: u64, unit: Unit, target: Unit, kills: usize) -> CombatUnit {
    CombatUnit {
        fleet_id: 0,
        battle_plan: BattlePlan::default(),
        id,
        owner: None,
        unit,
        hull: unit.hull(),
        shield: unit.shield(),
        repairs: Vec::new(),
        shots: vec![
            ShotReport {
                unit: Some(target),
                killed: true,
                ..default()
            };
            kills
        ],
    }
}

#[test]
fn combat_details_show_round_and_total_survivors_without_hover_changing_losses() {
    let unit = Unit::Ship(Ship::LightFighter);
    let rounds = [(10, 3), (7, 5)].map(|(count, kills)| {
        let mut army = (0..count)
            .map(|id| {
                combatant(
                    id,
                    unit,
                    unit,
                    if id == 0 {
                        kills
                    } else {
                        0
                    },
                )
            })
            .collect::<Vec<_>>();
        army[0].shots.extend(vec![
            ShotReport {
                unit: Some(unit),
                missed: true,
                ..default()
            };
            100
        ]);
        RoundReport {
            attacker: army.clone(),
            defender: army,
            ..default()
        }
    });
    let images = ImageIds(HashMap::from([(unit.to_lowername(), egui::TextureId::User(1))]));
    for side in [Side::Attacker, Side::Defender] {
        for (selected, remaining, lost) in
            [(&rounds[..1], 7, 3), (&rounds[1..], 2, 5), (&rounds[..], 2, 8)]
        {
            for hover in [None, Some((Unit::Ship(Ship::Cruiser), side.opposite()))] {
                let filtered = hover.is_some();
                let mut state = UiState {
                    combat_report_hover: hover,
                    ..default()
                };
                let view = CombatRoundView::new(selected).unwrap();
                let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    draw_combat_army_grid(
                        ui,
                        "test army",
                        &mut state,
                        &view,
                        vec![unit],
                        side.clone(),
                        false,
                        SpaceDockMode::default(),
                        &images,
                    );
                });
                output.textures_delta.clear();
                let icon = assert_counts(&output, remaining, lost);
                if !filtered {
                    let loss_rect = label_rect(&output.shapes, &format!("-{lost}"), Color32::RED);
                    let shots = lost + 100 * selected.len();
                    let shots_rect =
                        label_rect(&output.shapes, &format!("💥{shots}"), Color32::WHITE);
                    assert!(
                        shots_rect.left() >= icon.left() && shots_rect.left() <= icon.left() + 4.0
                    );
                    assert!(shots_rect.top() >= icon.top() && shots_rect.top() <= icon.top() + 4.0);
                    assert!(shots_rect.center().x < icon.center().x);
                    assert!(!shots_rect.intersects(loss_rect));
                }
            }
        }
    }
}

#[test]
fn bombing_details_use_postcombat_levels_in_the_round_and_total_views() {
    let building = Unit::Building(Building::MetalMine);
    let bomber = Unit::Ship(Ship::Bomber);
    let images = ImageIds(HashMap::from([(building.to_lowername(), egui::TextureId::User(1))]));
    for remaining in [0, 7] {
        let rounds = [
            RoundReport {
                buildings: Army::from([(building, remaining + 3)]),
                ..default()
            },
            RoundReport {
                attacker: vec![combatant(1, bomber, building, 3)],
                buildings: Army::from([(building, remaining)]),
                ..default()
            },
        ];
        for selected in [&rounds[1..], &rounds[..]] {
            let view = CombatRoundView::new(selected).unwrap();
            let mut state = UiState::default();
            let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                draw_combat_army_grid(
                    ui,
                    "test buildings",
                    &mut state,
                    &view,
                    vec![building],
                    Side::Defender,
                    false,
                    SpaceDockMode::default(),
                    &images,
                );
            });
            output.textures_delta.clear();
            assert_counts(&output, remaining, 3);
        }
    }
}
