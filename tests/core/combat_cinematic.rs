use super::*;
use crate::core::combat::report::{CombatReport, DefenderRetreat, RoundReport};
use crate::core::combat::resolution::CombatUnit;
use crate::core::map::planet::Planet;
use crate::core::missions::Mission;

fn replay() -> CinematicPlayback {
    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
    );
    let record = |id, unit| CombatUnit {
        id,
        unit,
        owner: None,
        hull: 100,
        shield: 10,
        repairs: Vec::new(),
        shots: Vec::new(),
    };
    report.combat_report = Some(CombatReport {
        rounds: vec![RoundReport {
            attacker: (0..24).map(|id| record(id, Unit::Ship(Ship::LightFighter))).collect(),
            defender: (24..58)
                .map(|id| record(id, Unit::Defense(Defense::RocketLauncher)))
                .collect(),
            ..Default::default()
        }],
        ..Default::default()
    });
    CinematicPlayback::new(&report)
}

#[test]
fn pause_freezes_camera_and_actors_and_speed_scales_the_same_clock() {
    let mut movie = replay();
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    movie.advance(0.25, 2.0, false);
    assert_eq!(movie.elapsed, 0.5);
    let before = movie.actor_pose(scene, 0, movie.elapsed);
    movie.advance(100.0, 4.0, true);
    let after = movie.actor_pose(scene, 0, movie.elapsed);
    assert_eq!(movie.elapsed, 0.5);
    assert_eq!(before.center, after.center);
    assert_eq!(before.angle, after.angle);
    movie.advance(f32::NAN, 1.0, false);
    movie.advance(1.0, f32::INFINITY, false);
    assert_eq!(movie.elapsed, 0.5);
    movie.advance(10_000.0, 1.0, false);
    assert!(movie.is_finished());
    movie.elapsed = 0.0;
    assert_eq!(movie.elapsed, 0.0);
}

#[test]
fn arrivals_curve_bank_independently_and_join_continuous_maneuvers() {
    let movie = replay();
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let end = movie.timeline.entrance_duration;
    assert!(end >= 4.0, "Ships need time for a visible approach before weapons fire");
    let mut banks = Vec::new();
    for index in 0..24 {
        let start = movie.actor_pose(scene, index, 0.0);
        let middle = movie.actor_pose(scene, index, end * 0.5);
        let arrived = movie.actor_pose(scene, index, end);
        assert!(start.center.x + start.size < scene.rect.left());
        assert!(middle.center.x > start.center.x && arrived.center.x > middle.center.x);
        let chord = arrived.center - start.center;
        let offset = middle.center - start.center;
        let distance_from_line = (chord.x * offset.y - chord.y * offset.x).abs() / chord.length();
        assert!(distance_from_line > 12.0, "Arrival {index} collapsed to a straight translation");
        banks.push(middle.angle);
        assert!(
            (middle.angle - start.angle).abs().max((arrived.angle - middle.angle).abs()) > 0.02,
            "The sprite must steer visibly during approach"
        );
        for tick in 0..200 {
            let pose = movie.actor_pose(scene, index, tick as f32 * 0.1);
            assert!(pose.angle.abs() < 0.27, "Banking must retain the isometric view");
        }
        let step = 0.01;
        let before = movie.actor_pose(scene, index, end - step);
        let after = movie.actor_pose(scene, index, end + step);
        let incoming = (arrived.center - before.center) / step;
        let outgoing = (after.center - arrived.center) / step;
        assert!(
            (incoming - outgoing).length() < 2.0,
            "Arrival must blend into the same ongoing flight without a stop or jump"
        );
        assert!((after.angle - before.angle).abs() < 0.01);
        let later = movie.actor_pose(scene, index, end + 2.0);
        assert!(later.center.distance(arrived.center) > 4.0);
    }
    assert!(
        banks.iter().copied().fold(f32::NEG_INFINITY, f32::max)
            - banks.iter().copied().fold(f32::INFINITY, f32::min)
            > 0.1,
        "A fleet must not all bank in lockstep"
    );
}

#[test]
fn background_preserves_map_art_proportions_and_color_without_an_oversized_nebula() {
    use bevy_egui::egui;
    let mut movie = replay();
    let map_texture = TextureId::User(501);
    let nebula_texture = TextureId::User(502);
    let images = ImageIds([("bg".into(), map_texture), ("nebula".into(), nebula_texture)].into());
    for size in [vec2(1440.0, 820.0), vec2(640.0, 480.0), vec2(450.0, 700.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
        movie.elapsed = 22.0;
        let context = egui::Context::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(scene.rect),
                ..Default::default()
            },
            |ui| movie.paint_space(ui.painter(), scene, &images),
        );
        output.textures_delta.clear();
        let meshes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .collect();
        assert!(!meshes.iter().any(|mesh| mesh.texture_id == nebula_texture));
        let map = meshes.iter().find(|mesh| mesh.texture_id == map_texture).unwrap();
        let bounds = map.calc_bounds();
        assert!(bounds.contains_rect(scene.rect));
        assert!((bounds.width() / bounds.height() - 1.5).abs() < 0.001);
        assert!(map.vertices.iter().all(|vertex| vertex.color == Color32::WHITE));
    }
}

#[test]
fn dense_surface_formations_keep_each_turret_on_the_globe() {
    for count in [1, 2, 7, 34, 150, 600] {
        let positions: Vec<_> = (0..count).map(|i| formation_home(2, i, count)).collect();
        for (index, position) in positions.iter().enumerate() {
            assert!(position.length() < 0.95, "Turret {index}/{count} floats off the planet");
            assert!(positions[..index].iter().all(|other| *other != *position));
        }
    }
    let movie = replay();
    assert_eq!(movie.visuals.len(), 58);
    assert_eq!(movie.draw_order.len(), movie.timeline.actors.len());
}

#[test]
fn shield_intersection_stops_a_surface_shot_on_the_near_limb() {
    let center = pos2(100.0, 100.0);
    let hit = sphere_entry(pos2(0.0, 100.0), center, center, 40.0).unwrap();
    assert!((hit.x - 60.0).abs() < 0.001);
    assert!((hit.distance(center) - 40.0).abs() < 0.001);
    assert!(sphere_entry(pos2(0.0, 0.0), pos2(10.0, 0.0), center, 40.0).is_none());
    assert!(sphere_entry(center, center, center, 40.0).is_none());
}

#[test]
fn resizing_preserves_ship_scale_and_ground_attachment() {
    let movie = replay();
    for size in [vec2(1440.0, 820.0), vec2(640.0, 360.0), vec2(450.0, 700.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
        for (index, visual) in movie.visuals.iter().enumerate() {
            let pose = movie.actor_pose(scene, index, 4.0);
            assert!(pose.size > 0.0 && pose.center.is_finite());
            if visual.ground {
                assert!(pose.center.distance(scene.planet) < scene.planet_radius);
            }
        }
    }
    assert!(unit_size(Unit::Ship(Ship::WarSun)) > unit_size(Unit::Ship(Ship::LightFighter)) * 3.0);
}

#[test]
fn immediate_withdrawal_starts_visible_then_flies_out_without_exploding() {
    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
    );
    report.combat_report = Some(CombatReport {
        defender_retreat: Some(DefenderRetreat {
            after_round: None,
            home_planet: 0,
            ships: [(Unit::Ship(Ship::LightFighter), 3)].into(),
        }),
        ..Default::default()
    });
    let movie = CinematicPlayback::new(&report);
    for size in [vec2(1440.0, 820.0), vec2(640.0, 480.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
        for (index, actor) in movie.timeline.actors.iter().enumerate() {
            let departure = actor.retreat_at.unwrap();
            let before = movie.actor_pose(scene, index, departure - 0.1);
            let departing = movie.actor_pose(scene, index, departure + 0.6);
            let after = movie.actor_pose(scene, index, departure + 1.65);
            assert!(scene.rect.contains(before.center), "A withdrawing ship must appear on screen");
            assert!(departing.center.x > before.center.x);
            assert!(after.center.x - after.size > scene.rect.right());
            assert!(actor.death_at.is_none());
        }
    }
}

#[test]
fn map_shield_filaments_animate_with_playback_and_disappear_on_recorded_collapse() {
    use crate::core::combat::resolution::ShotReport;
    use bevy_egui::egui;

    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
    );
    report.combat_report = Some(CombatReport {
        rounds: vec![RoundReport {
            attacker: vec![CombatUnit {
                id: 1,
                unit: Unit::Ship(Ship::Cruiser),
                owner: None,
                hull: 100,
                shield: 0,
                repairs: Vec::new(),
                shots: vec![ShotReport {
                    unit: Some(Unit::planetary_shield()),
                    planetary_shield_damage: 100,
                    ..Default::default()
                }],
            }],
            ..Default::default()
        }],
        ..Default::default()
    });
    let mut movie = CinematicPlayback::new(&report);
    let texture = TextureId::User(987);
    let images = ImageIds([("planetary shield marker".into(), texture)].into());
    let sample = |movie: &CinematicPlayback| {
        let context = egui::Context::default();
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
        let mut output = context.run_ui(egui::RawInput::default(), |ui| {
            movie.paint_planet(ui.painter(), scene, &images);
        });
        output.textures_delta.clear();
        output
            .shapes
            .into_iter()
            .filter_map(|shape| match shape.shape {
                Shape::Mesh(mesh) if mesh.texture_id == texture => Some(mesh),
                _ => None,
            })
            .flat_map(|mesh| {
                mesh.vertices.iter().map(|vertex| (vertex.pos, vertex.color)).collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    };
    movie.elapsed = 0.6;
    let initial = sample(&movie);
    assert_eq!(initial.len(), 8, "Both filament layers must use the existing map artwork");
    movie.advance(5.0, 4.0, true);
    assert_eq!(sample(&movie), initial, "Pause must freeze rotation and opacity together");
    movie.advance(0.25, 2.0, false);
    assert_ne!(sample(&movie), initial, "The shield must visibly flow and pulse over time");
    movie.elapsed = movie.timeline.shots[0].impact_at;
    assert_eq!(movie.timeline.planetary_shield_at(movie.elapsed), 0);
    assert!(sample(&movie).is_empty(), "A depleted shield must not leave a decorative field");
}

#[test]
fn cinematic_weapon_meshes_preserve_distinct_barrels_masks_and_shared_colors() {
    let context = bevy_egui::egui::Context::default();
    let images = ImageIds(
        [
            ("combat fx beam".to_string(), TextureId::User(41)),
            ("combat fx missile".to_string(), TextureId::User(42)),
        ]
        .into(),
    );
    for (weapon, expected_meshes, mask) in [
        (Weapon::Laser, 2, TextureId::User(41)),
        (Weapon::TwinLaser, 5, TextureId::User(41)),
        (Weapon::Repeater, 7, TextureId::User(41)),
        (Weapon::Broadside, 5, TextureId::User(41)),
        (Weapon::Missile, 2, TextureId::User(42)),
        (Weapon::Bomb, 2, TextureId::User(42)),
    ] {
        context.begin_pass(bevy_egui::egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
            ..Default::default()
        });
        let painter = context.layer_painter(bevy_egui::egui::LayerId::background());
        paint_weapon_body(
            &painter,
            &images,
            WeaponFlight {
                weapon,
                origin: BevyVec3::new(100.0, 200.0, 0.0),
                destination: BevyVec3::new(500.0, 200.0, 0.0),
                size: 100.0,
                lane: 1.0,
            },
            0.5,
            1.0,
        );
        let mut output = context.end_pass();
        output.textures_delta.clear();
        let meshes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| {
                if let Shape::Mesh(mesh) = &shape.shape {
                    Some(mesh)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(meshes.len(), expected_meshes, "{weapon:?} must retain its actual barrels");
        assert_eq!(meshes[0].texture_id, mask);
        assert!(meshes[0]
            .vertices
            .iter()
            .all(|vertex| vertex.color == weapon_color(weapon.color().with_alpha(0.6))));
    }
}

fn bombing_replay() -> CinematicPlayback {
    use crate::core::combat::resolution::ShotReport;
    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
    );
    let buildings: Vec<_> =
        Unit::resource_buildings().into_iter().chain(Unit::industrial_buildings()).collect();
    for unit in &buildings {
        report.planet.army.insert(*unit, 3);
    }
    report.combat_report = Some(CombatReport {
        rounds: vec![RoundReport {
            attacker: (0..3)
                .map(|index| CombatUnit {
                    id: index,
                    unit: Unit::Ship(Ship::Bomber),
                    owner: None,
                    hull: 100,
                    shield: 0,
                    repairs: vec![],
                    shots: vec![ShotReport {
                        unit: Some(buildings[0]),
                        killed: index < 2,
                        missed: index == 2,
                        ..Default::default()
                    }],
                })
                .collect(),
            defender: (4..10)
                .map(|id| CombatUnit {
                    id,
                    unit: Unit::Defense(Defense::RocketLauncher),
                    owner: None,
                    hull: 100,
                    shield: 0,
                    repairs: vec![],
                    shots: vec![],
                })
                .collect(),
            ..Default::default()
        }],
        ..Default::default()
    });
    CinematicPlayback::new(&report)
}

#[test]
fn six_buildings_fit_above_exit_and_bombs_land_on_the_recorded_roof() {
    use crate::core::map::utils::{MAIN_BUTTON_RIGHT, MAIN_BUTTON_WIDTH};
    let movie = bombing_replay();
    for size in [vec2(1440.0, 900.0), vec2(640.0, 480.0), vec2(640.0, 360.0), vec2(450.0, 700.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
        let exit = Rect::from_min_size(
            pos2(
                size.x - MAIN_BUTTON_RIGHT - MAIN_BUTTON_WIDTH,
                size.y - MAIN_BUTTON_BOTTOM - MAIN_BUTTON_HEIGHT,
            ),
            vec2(MAIN_BUTTON_WIDTH, MAIN_BUTTON_HEIGHT),
        );
        for (index, actor) in movie
            .timeline
            .actors
            .iter()
            .enumerate()
            .filter(|(_, actor)| actor.initial_levels.is_some())
        {
            let pose = movie.actor_pose(scene, index, movie.timeline.entrance_duration);
            let footprint = Rect::from_center_size(pose.center, Vec2::splat(pose.size));
            assert!(
                scene.rect.contains_rect(footprint),
                "{} must remain visible at {size:?}",
                actor.unit.to_lowername()
            );
            assert!(
                !exit.intersects(footprint),
                "{} overlaps Exit at {size:?}",
                actor.unit.to_lowername()
            );
            assert!(
                pose.center.y + pose.size * 0.43 + (11.0 * scene.scale).max(10.0) < exit.top(),
                "Level tags must also clear the fixed exit control at {size:?}"
            );
            assert!(pose.center.distance(scene.planet) < scene.planet_radius);
        }
        for (index, shot) in movie.timeline.shots.iter().enumerate() {
            let (_, end, _) = movie.shot_geometry(scene, index, shot);
            let target = movie.actor_pose(scene, shot.target.unwrap(), shot.impact_at);
            if shot.outcome.missed {
                assert!(end.distance(target.center) > target.size * 0.7);
            } else {
                assert!(
                    end.distance(target.center) < 0.001,
                    "A successful bomb must hit its actual building"
                );
            }
        }
    }
}

#[test]
fn loss_captions_are_readable_separated_seekable_and_absent_for_misses() {
    use bevy_egui::egui;
    let mut movie = bombing_replay();
    let images = ImageIds::default();
    for size in [vec2(1440.0, 820.0), vec2(640.0, 480.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
        let sample = |movie: &CinematicPlayback| {
            let context = egui::Context::default();
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(scene.rect),
                    ..Default::default()
                },
                |ui| {
                    movie.paint_level_losses(ui.painter(), scene, &images);
                },
            );
            output.textures_delta.clear();
            output
                .shapes
                .into_iter()
                .filter_map(|shape| match shape.shape {
                    Shape::Text(text)
                        if text.galley.text() == "-1 level"
                            && text.override_text_color.is_none() =>
                    {
                        Some((text.pos, text.galley.size()))
                    },
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        movie.elapsed = movie.timeline.level_losses[1].impact_at + 0.05;
        let captions = sample(&movie);
        assert_eq!(captions.len(), 4, "Exactly two actual losses, each with a shadow");
        assert!(captions.iter().all(|(_, size)| size.y >= 11.0));
        assert!(
            (captions[0].0.y - captions[2].0.y).abs() >= captions[0].1.y,
            "Consecutive losses on one building must remain separately readable"
        );
        movie.advance(10.0, 4.0, true);
        assert_eq!(sample(&movie), captions, "Pause must freeze all caption movement");
        movie.elapsed = 0.0;
        assert!(sample(&movie).is_empty());
        movie.elapsed =
            movie.timeline.shots.iter().map(|shot| shot.impact_at).fold(0.0_f32, f32::max) + 2.3;
        assert!(sample(&movie).is_empty(), "Misses cannot introduce additional captions");
    }
}

#[test]
fn combined_ray_has_one_discharge_and_planet_breakup_uses_the_actual_artwork() {
    use bevy_egui::egui;
    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
    );
    report.planet_destroyed = true;
    report.combat_report = Some(CombatReport {
        rounds: vec![RoundReport {
            attacker: (0..3)
                .map(|id| CombatUnit {
                    id,
                    unit: Unit::war_sun(),
                    owner: None,
                    hull: 100,
                    shield: 0,
                    repairs: vec![],
                    shots: vec![],
                })
                .collect(),
            destroy_probability: 0.5,
            ..Default::default()
        }],
        ..Default::default()
    });
    let mut movie = CinematicPlayback::new(&report);
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let planet_texture = TextureId::User(90);
    let beam_texture = TextureId::User(91);
    let images = ImageIds(
        [(movie.planet_image.clone(), planet_texture), ("combat fx beam".into(), beam_texture)]
            .into(),
    );
    let sample = |movie: &CinematicPlayback| {
        let context = egui::Context::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(scene.rect),
                ..Default::default()
            },
            |ui| {
                movie.paint_planet(ui.painter(), scene, &images);
                movie.paint_planet_attacks(ui.painter(), scene, &images);
            },
        );
        output.textures_delta.clear();
        output
            .shapes
            .into_iter()
            .filter_map(|shape| match shape.shape {
                Shape::Mesh(mesh) => Some(mesh),
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    let attack = &movie.timeline.planet_attacks[0];
    let focus = movie.planet_attack_focus(scene, attack);
    assert!(focus.distance(scene.planet) > scene.planet_radius * 1.3);
    let discharge_at = attack.discharge_at;
    let end_at = attack.end_at;
    movie.elapsed = discharge_at - 0.1;
    assert_eq!(
        sample(&movie).iter().filter(|mesh| mesh.texture_id == beam_texture).count(),
        6,
        "All three War Suns have two feeder-ray layers, without a premature discharge"
    );
    movie.elapsed = discharge_at + 0.3;
    assert_eq!(
        sample(&movie).iter().filter(|mesh| mesh.texture_id == beam_texture).count(),
        9,
        "Three feeder rays and exactly one three-layer outgoing beam"
    );
    movie.elapsed = end_at + 0.4;
    let fragments = sample(&movie);
    assert_eq!(fragments.iter().filter(|mesh| mesh.texture_id == planet_texture).count(), 36);
    assert_eq!(fragments.iter().filter(|mesh| mesh.texture_id == beam_texture).count(), 0);
    movie.elapsed = end_at + 3.9;
    assert!(
        !sample(&movie).iter().any(|mesh| mesh.texture_id == planet_texture),
        "A destroyed world must leave empty space"
    );
}
