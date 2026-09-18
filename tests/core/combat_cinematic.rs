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
        let distance_from_line = (1..10)
            .map(|step| {
                let offset =
                    movie.actor_pose(scene, index, end * step as f32 / 10.0).center - start.center;
                (chord.x * offset.y - chord.y * offset.x).abs() / chord.length()
            })
            .fold(0.0_f32, f32::max);
        assert!(distance_from_line > 12.0, "Arrival {index} collapsed to a straight translation");
        banks.push(middle.angle);
        assert!(
            (middle.angle - start.angle).abs().max((arrived.angle - middle.angle).abs()) > 0.02,
            "The sprite must steer visibly during approach"
        );
        for tick in 0..200 {
            let time = tick as f32 * 0.1;
            let pose = movie.actor_pose(scene, index, time);
            let velocity = movie.actor_pose(scene, index, time + 0.025).center
                - movie.actor_pose(scene, index, time - 0.025).center;
            let bow = rotate(Vec2::angled(movie.visuals[index].art_heading), pose.angle);
            if velocity.length_sq() > 0.000_001 {
                assert!(bow.dot(velocity.normalized()) > 0.99, "Ships must turn into their course");
            }
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
fn capital_ships_hold_steady_courses_and_all_hulls_fly_bow_first() {
    use strum::IntoEnumIterator;

    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let mut fighter_travel = 0.0;
    let mut capital_travel = 0.0;
    for ship in Ship::iter() {
        for side in [Side::Attacker, Side::Defender] {
            let mut movie = replay();
            movie.show_planet = false;
            movie.timeline.actors[0].unit = Unit::Ship(ship);
            movie.timeline.actors[0].side = side.clone();
            movie.visuals[0].art_heading = sprite_heading(Unit::Ship(ship));
            movie.visuals[0].size = unit_size(Unit::Ship(ship));
            let mut travelled = 0.0;
            for tick in 0..600 {
                let time = 5.0 + tick as f32 * 0.05;
                let pose = movie.actor_pose(scene, 0, time);
                let next = movie.actor_pose(scene, 0, time + 0.05);
                let velocity = next.center - pose.center;
                let direction = if pose.mirror {
                    -1.0
                } else {
                    1.0
                };
                let bow = rotate(
                    Vec2::angled(movie.visuals[0].art_heading) * vec2(direction, 1.0),
                    pose.angle,
                );
                assert!(velocity.length() > 0.01, "{ship:?} must keep moving");
                assert!(bow.dot(velocity.normalized()) > 0.98, "{ship:?} flew backwards");
                if matches!(ship, Ship::WarSun | Ship::Dreadnought | Ship::Battleship) {
                    let turn = next.angle - pose.angle;
                    assert!(
                        turn.sin().atan2(turn.cos()).abs() < 0.03,
                        "Heavy hulls must turn slowly"
                    );
                }
                travelled += velocity.length();
            }
            if side == Side::Attacker {
                if ship == Ship::LightFighter {
                    fighter_travel = travelled;
                } else if ship == Ship::WarSun {
                    capital_travel = travelled;
                }
            }
            movie.timeline.actors[0].retreat_at = Some(8.0);
            for tick in 1..32 {
                let time = 8.0 + tick as f32 * 0.05;
                let pose = movie.actor_pose(scene, 0, time);
                let velocity = movie.actor_pose(scene, 0, time + 0.01).center - pose.center;
                let bow = rotate(
                    Vec2::angled(movie.visuals[0].art_heading)
                        * vec2(
                            if pose.mirror {
                                -1.0
                            } else {
                                1.0
                            },
                            1.0,
                        ),
                    pose.angle,
                );
                assert!(
                    bow.dot(velocity.normalized()) > 0.95,
                    "{ship:?} withdrawal at {time} must be bow-first"
                );
            }
        }
    }
    assert!(capital_travel < fighter_travel * 0.25, "Capital ships should not dart like escorts");
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
    CinematicPlayback::new(&bombing_report())
}

fn bombing_report() -> MissionReport {
    use crate::core::combat::resolution::ShotReport;
    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
    );
    report.mission.bombing = BombingRaid::Economic;
    report.planet.kind = PlanetKind::Dry;
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
    report
}

#[test]
fn relevant_buildings_fit_above_exit_and_bombs_land_on_the_recorded_roof() {
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
        for (index, _) in
            movie.visuals.iter().enumerate().filter(|(_, visual)| visual.ground && visual.visible)
        {
            let pose = movie.actor_pose(scene, index, movie.timeline.entrance_duration);
            assert!(
                pose.center.distance(scene.planet) + pose.size * 0.25 < scene.planet_radius,
                "The actual defensive terrace must sit on the globe, including its base"
            );
        }
        for (index, actor) in movie.timeline.actors.iter().enumerate().filter(|(index, actor)| {
            actor.initial_levels.is_some() && movie.visuals[*index].visible
        }) {
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
fn raid_visibility_keeps_defenses_and_docks_hides_other_orbitals_and_respects_joint_orders() {
    use crate::core::missions::{FleetCombatOrders, JointAttackMission};
    use crate::core::units::orbitals;
    let mut report = bombing_report();
    for orbital in orbitals::ALL {
        report.planet.army.insert(orbital, 2);
    }
    let round = &mut report.combat_report.as_mut().unwrap().rounds[0];
    let mut dock = round.defender[0].clone();
    dock.id = 99;
    dock.unit = Unit::space_dock();
    round.defender.push(dock);
    for (raid, expected) in
        [(BombingRaid::None, 0), (BombingRaid::Economic, 3), (BombingRaid::Industrial, 3)]
    {
        report.mission.bombing = raid.clone();
        let movie = CinematicPlayback::new(&report);
        let visible: Vec<_> =
            movie.draw_order.iter().map(|index| &movie.timeline.actors[*index]).collect();
        assert_eq!(visible.iter().filter(|actor| actor.initial_levels.is_some()).count(), expected);
        assert!(!visible
            .iter()
            .any(|actor| actor.unit.is_orbital() && actor.unit != Unit::space_dock()));
        assert_eq!(visible.iter().filter(|actor| actor.unit == Unit::space_dock()).count(), 1);
        assert_eq!(visible.iter().filter(|actor| actor.unit.is_turret()).count(), 6);
        for actor in visible.iter().filter(|actor| actor.initial_levels.is_some()) {
            assert_eq!(actor.unit.is_economic_building(), raid == BombingRaid::Economic);
        }
        assert!(
            movie.timeline.actors.iter().any(|actor| actor.unit == Unit::space_dock()),
            "Hiding scenery must not rewrite recorded participants"
        );
    }
    report.mission.joint_attack = Some(JointAttackMission {
        combat_orders: [
            (
                1,
                FleetCombatOrders {
                    bombing: BombingRaid::Economic,
                    ..Default::default()
                },
            ),
            (
                2,
                FleetCombatOrders {
                    bombing: BombingRaid::Industrial,
                    ..Default::default()
                },
            ),
        ]
        .into(),
        ..Default::default()
    });
    let movie = CinematicPlayback::new(&report);
    assert_eq!(
        movie
            .draw_order
            .iter()
            .filter(|index| movie.timeline.actors[**index].initial_levels.is_some())
            .count(),
        6
    );
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(640.0, 480.0)));
    for &index in &movie.draw_order {
        if movie.timeline.actors[index].initial_levels.is_some() {
            let pose = movie.actor_pose(scene, index, 5.0);
            assert!(
                pose.center.y + pose.size * 0.5
                    < scene.rect.bottom() - MAIN_BUTTON_BOTTOM - MAIN_BUTTON_HEIGHT
            );
        }
    }
}

#[test]
fn space_dock_hovers_above_the_planet_with_clearance_for_its_entire_sprite() {
    let mut report = bombing_report();
    let round = &mut report.combat_report.as_mut().unwrap().rounds[0];
    let mut dock = round.defender[0].clone();
    dock.id = 99;
    dock.unit = Unit::space_dock();
    round.defender.push(dock);
    let movie = CinematicPlayback::new(&report);
    let index =
        movie.timeline.actors.iter().position(|actor| actor.unit == Unit::space_dock()).unwrap();
    assert!(movie.actor_visible(index));
    assert!(!movie.visuals[index].ground);
    for viewport in
        [vec2(1440.0, 900.0), vec2(640.0, 480.0), vec2(640.0, 360.0), vec2(450.0, 700.0)]
    {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, viewport));
        let first = movie.actor_pose(scene, index, 0.0);
        for tick in 0..100 {
            let pose = movie.actor_pose(scene, index, tick as f32 * 0.4);
            assert!(pose.center.y < scene.planet.y);
            assert!(
                pose.center.distance(scene.planet) - pose.size * 0.5 > scene.planet_radius * 1.065,
                "Dock must orbit outside the field, including its hull"
            );
            assert!(
                scene
                    .rect
                    .contains_rect(Rect::from_center_size(pose.center, Vec2::splat(pose.size))),
                "Dock must remain visible at {viewport:?}"
            );
            assert!((pose.size - first.size).abs() < 0.001);
        }
        assert!(
            movie.actor_pose(scene, index, 8.0).center.distance(first.center) > scene.scale * 2.0,
            "The dock should visibly drift in orbit"
        );
    }
}

#[test]
fn surface_counterfire_uses_raised_guns_and_hits_the_recorded_attacking_ship() {
    use crate::core::combat::resolution::ShotReport;
    use bevy_egui::egui;
    let mut report = bombing_report();
    let round = &mut report.combat_report.as_mut().unwrap().rounds[0];
    round.attacker.iter_mut().for_each(|actor| actor.shots.clear());
    let units = [
        Defense::RocketLauncher,
        Defense::LightLaser,
        Defense::HeavyLaser,
        Defense::GaussCannon,
        Defense::IonCannon,
        Defense::PlasmaTurret,
    ];
    for (actor, unit) in round.defender.iter_mut().zip(units) {
        actor.unit = Unit::Defense(unit);
        actor.shots = vec![ShotReport {
            target_id: Some(0),
            unit: Some(Unit::Ship(Ship::Bomber)),
            hull_damage: 1,
            ..Default::default()
        }];
    }
    let mut movie = CinematicPlayback::new(&report);
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let images = ImageIds(
        [
            ("combat fx beam".into(), TextureId::User(91)),
            ("combat fx missile".into(), TextureId::User(92)),
            ("combat fx glow".into(), TextureId::User(93)),
        ]
        .into(),
    );
    assert_eq!(movie.timeline.shots.len(), units.len());
    for index in 0..movie.timeline.shots.len() {
        let shot = &movie.timeline.shots[index];
        assert!(movie.actor_visible(shot.source));
        assert_eq!(movie.timeline.actors[shot.source].side, Side::Defender);
        let target = shot.target.unwrap();
        assert_eq!(movie.timeline.actors[target].id, Some(0));
        assert_eq!(movie.timeline.actors[target].side, Side::Attacker);
        let (start, end, _) = movie.shot_geometry(scene, index, shot);
        let gun = movie.actor_pose(scene, shot.source, shot.launch_at);
        assert!(start.y < gun.center.y - gun.size * 0.2);
        assert!(start.x < gun.center.x);
        assert!(end.distance(movie.actor_pose(scene, target, shot.impact_at).center) < 0.001);
        movie.elapsed = (shot.launch_at + shot.impact_at) * 0.5;
        let context = egui::Context::default();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(scene.rect),
                ..Default::default()
            },
            |ui| {
                movie.paint_shot(ui.painter(), scene, &images, index, shot);
            },
        );
        output.textures_delta.clear();
        assert!(
            output.shapes.iter().any(
                |shape| matches!(&shape.shape, Shape::Mesh(mesh) if !mesh.vertices.is_empty())
            ),
            "Every recorded turret shot must produce visible weapon artwork"
        );
    }
}

#[test]
fn gas_platforms_hover_and_buildings_have_no_permanent_level_labels() {
    use bevy_egui::egui;
    let mut report = bombing_report();
    report.planet.kind = PlanetKind::Gas;
    let mut movie = CinematicPlayback::new(&report);
    movie.elapsed = movie.timeline.entrance_duration;
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let context = egui::Context::default();
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(scene.rect),
            ..Default::default()
        },
        |ui| {
            for &index in &movie.draw_order {
                let actor = &movie.timeline.actors[index];
                if actor.initial_levels.is_some() {
                    assert!(movie.visuals[index].texture.starts_with("cinematic gas "));
                    assert_ne!(
                        movie.actor_pose(scene, index, 5.0).center,
                        movie.actor_pose(scene, index, 6.0).center
                    );
                    movie.paint_actor(ui.painter(), scene, &ImageIds::default(), index);
                }
            }
        },
    );
    output.textures_delta.clear();
    assert!(
        !output.shapes.iter().any(|shape| matches!(&shape.shape, Shape::Text(_))),
        "Only recorded level losses get a temporary caption"
    );
}

#[test]
fn every_planet_kind_uses_two_stable_large_globes() {
    use strum::IntoEnumIterator;
    let mut planet = bombing_report().planet;
    for kind in PlanetKind::iter() {
        planet.kind = kind;
        planet.id = 12;
        let first = cinematic_planet_image(&planet);
        assert_eq!(first, cinematic_planet_image(&planet));
        planet.id = 13;
        let second = cinematic_planet_image(&planet);
        assert_ne!(first, second);
        for name in [first, second] {
            assert!(crate::core::assets::CINEMATIC_PLANET_IMAGE_NAMES.contains(&name.as_str()));
        }
    }
}

#[test]
fn combat_maneuvers_cover_diagonal_passes_and_bombers_approach_the_surface() {
    let movie = replay();
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    for index in 0..24 {
        let samples: Vec<_> = (0..80)
            .map(|step| movie.actor_pose(scene, index, 5.0 + step as f32 * 0.25).center)
            .collect();
        let bounds = Rect::from_points(&samples);
        assert!(
            bounds.width() > 250.0 && bounds.height() > 140.0,
            "Fighters should make broad attack passes, not hover over their formation slot"
        );
    }
    let bomber = bombing_replay();
    assert!(bomber.visuals[0].bombing_target.is_some());
    let nearest = (0..120)
        .map(|step| {
            bomber.actor_pose(scene, 0, 5.0 + step as f32 * 0.25).center.distance(scene.planet)
        })
        .fold(f32::INFINITY, f32::min);
    assert!(
        nearest < scene.planet_radius * 1.9,
        "Bomber passes must reach the planet's approach corridor"
    );
}

#[test]
fn bomber_runs_cannot_cross_a_live_planetary_shield() {
    let mut report = bombing_report();
    report.planet.army.insert(Unit::planetary_shield(), 3);
    report.combat_report.as_mut().unwrap().rounds[0].planetary_shield = 999;
    let shot = &mut report.combat_report.as_mut().unwrap().rounds[0].attacker[0].shots[0];
    shot.planetary_shield_damage = 1;
    shot.killed = false;
    let movie = CinematicPlayback::new(&report);
    for size in [vec2(1440.0, 900.0), vec2(640.0, 480.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
        for tick in 0..200 {
            let time = 4.8 + tick as f32 * 0.1;
            assert!(movie.timeline.planetary_shield_at(time) > 0);
            for index in 0..3 {
                let pose = movie.actor_pose(scene, index, time);
                assert!(
                    pose.center.distance(scene.planet) - pose.size * 0.23
                        > scene.planet_radius * 1.065
                );
            }
        }
        let (index, shot) = movie
            .timeline
            .shots
            .iter()
            .enumerate()
            .find(|(_, shot)| shot.outcome.planetary_shield_damage > 0)
            .unwrap();
        let (_, impact, _) = movie.shot_geometry(scene, index, shot);
        assert!(
            (impact.distance(scene.planet) - scene.planet_radius * 1.065).abs() < 0.01,
            "Recorded shield damage must strike the field, never a roof behind it"
        );
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

fn planet_strike_replay(destroyed: bool) -> CinematicPlayback {
    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
    );
    report.planet_destroyed = destroyed;
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
    CinematicPlayback::new(&report)
}

fn planet_strike_images(movie: &CinematicPlayback) -> ImageIds {
    ImageIds(
        [
            (movie.planet_image.clone(), TextureId::User(90)),
            ("combat fx beam".into(), TextureId::User(91)),
            ("explosion".into(), TextureId::User(92)),
            ("combat fx shard".into(), TextureId::User(93)),
            ("combat fx glow".into(), TextureId::User(94)),
            ("combat fx ring".into(), TextureId::User(95)),
        ]
        .into(),
    )
}

fn planet_strike_shapes(movie: &CinematicPlayback, scene: Scene, images: &ImageIds) -> Vec<Shape> {
    capture_planet_shapes(scene, |painter| {
        movie.paint_planet(painter, scene, images);
        movie.paint_planet_attacks(painter, scene, images);
    })
}

fn capture_planet_shapes(scene: Scene, mut paint: impl FnMut(&Painter)) -> Vec<Shape> {
    use bevy_egui::egui;
    let context = egui::Context::default();
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(scene.rect),
            ..Default::default()
        },
        |ui| paint(ui.painter()),
    );
    output.textures_delta.clear();
    output.shapes.into_iter().map(|shape| shape.shape).collect()
}

fn meshes_with_texture(shapes: &[Shape], texture: TextureId) -> Vec<&Mesh> {
    shapes
        .iter()
        .filter_map(|shape| match shape {
            Shape::Mesh(mesh) if mesh.texture_id == texture => Some(mesh.as_ref()),
            _ => None,
        })
        .collect()
}

#[test]
fn combined_ray_has_one_discharge_regardless_of_war_sun_count() {
    let mut movie = planet_strike_replay(true);
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let images = planet_strike_images(&movie);
    let beam_texture = images.0["combat fx beam"];
    let attack = &movie.timeline.planet_attacks[0];
    let focus = movie.planet_attack_focus(scene, attack);
    assert!(focus.distance(scene.planet) > scene.planet_radius * 1.3);
    let discharge_at = attack.discharge_at;
    let end_at = attack.end_at;
    movie.elapsed = discharge_at - 0.1;
    assert_eq!(
        meshes_with_texture(&planet_strike_shapes(&movie, scene, &images), beam_texture).len(),
        6,
        "All three War Suns have two feeder-ray layers, without a premature discharge"
    );
    movie.elapsed = discharge_at + 0.3;
    assert_eq!(
        meshes_with_texture(&planet_strike_shapes(&movie, scene, &images), beam_texture).len(),
        9,
        "Three feeder rays and exactly one three-layer outgoing beam"
    );
    movie.elapsed = end_at + 0.4;
    assert!(
        meshes_with_texture(&planet_strike_shapes(&movie, scene, &images), beam_texture).is_empty()
    );
}

#[test]
fn destroyed_planet_uses_overlapping_shared_blasts_and_small_shards_then_clears() {
    let mut movie = planet_strike_replay(true);
    let images = planet_strike_images(&movie);
    let planet_texture = images.0[&movie.planet_image];
    let blast_texture = images.0["explosion"];
    let shard_texture = images.0["combat fx shard"];
    let end_at = movie.timeline.planet_attacks[0].end_at;
    for size in [vec2(1440.0, 820.0), vec2(640.0, 480.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
        movie.elapsed = end_at + 0.7;
        let shapes = planet_strike_shapes(&movie, scene, &images);
        let blasts = meshes_with_texture(&shapes, blast_texture);
        assert!(blasts.len() > 3, "A planetary blast must spread through overlapping wreck clouds");
        let bounds =
            blasts.iter().fold(Rect::NOTHING, |bounds, mesh| bounds.union(mesh.calc_bounds()));
        assert!(
            bounds.contains_rect(Rect::from_center_size(
                scene.planet,
                Vec2::splat(scene.planet_radius * 2.0),
            )),
            "The shared explosions must cover the full globe"
        );
        for blast in &blasts {
            let uv = Rect::from_points(
                &blast.vertices.iter().map(|vertex| vertex.uv).collect::<Vec<_>>(),
            );
            assert!((uv.width() - 1.0 / 8.0).abs() < 0.0001);
            assert!(
                (uv.height() - 1.0 / 6.0).abs() < 0.0001,
                "Planet destruction must sample the normal battle explosion atlas"
            );
        }
        let planet = meshes_with_texture(&shapes, planet_texture);
        assert!(
            planet.len() <= 1,
            "The globe must fade as one image instead of flying away in wedges"
        );

        movie.elapsed = end_at + 1.25;
        let shapes = planet_strike_shapes(&movie, scene, &images);
        assert!(meshes_with_texture(&shapes, planet_texture).is_empty());
        assert!(
            !meshes_with_texture(&shapes, blast_texture).is_empty(),
            "Successive blasts must retain the ordinary explosion animation after the first flash"
        );

        movie.elapsed = end_at + 2.5;
        let shapes = planet_strike_shapes(&movie, scene, &images);
        let shards = meshes_with_texture(&shapes, shard_texture);
        assert!(shards.len() >= 18, "A destroyed planet should leave a cloud of small debris");
        assert!(
            shards
                .iter()
                .all(|mesh| mesh.calc_bounds().size().length() < scene.planet_radius * 0.12),
            "Debris must remain small instead of resembling large slices of the globe"
        );
        assert!(meshes_with_texture(&shapes, planet_texture).is_empty());
        movie.elapsed = end_at + 3.9;
        assert!(
            planet_strike_shapes(&movie, scene, &images).is_empty(),
            "A destroyed world must leave empty space after its effects finish"
        );
    }
}

#[test]
fn failed_planet_strike_leaves_an_intact_globe_and_fading_surface_ripples() {
    let mut movie = planet_strike_replay(false);
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let images = planet_strike_images(&movie);
    let discharge_at = movie.timeline.planet_attacks[0].discharge_at;
    let end_at = movie.timeline.planet_attacks[0].end_at;
    // Inspect the strike separately from the quiet planet's decorative atmosphere arcs.
    movie.elapsed = end_at + 1.4;
    let ripple_shapes = |movie: &CinematicPlayback| {
        capture_planet_shapes(scene, |painter| {
            movie.paint_planet_attacks(painter, scene, &images);
        })
    };
    assert!(!ripple_shapes(&movie).iter().any(|shape| matches!(shape, Shape::Path(_))));
    for time in [discharge_at + 0.35, end_at + 0.4] {
        movie.elapsed = time;
        let shapes = planet_strike_shapes(&movie, scene, &images);
        let globe = meshes_with_texture(&shapes, images.0[&movie.planet_image]);
        assert_eq!(globe.len(), 1);
        assert!(globe[0].vertices.iter().all(|vertex| vertex.color == Color32::WHITE));
        assert!(meshes_with_texture(&shapes, images.0["explosion"]).is_empty());
        assert!(meshes_with_texture(&shapes, images.0["combat fx shard"]).is_empty());
        let ripples = ripple_shapes(&movie);
        let paths: Vec<_> = ripples
            .iter()
            .filter_map(|shape| match shape {
                Shape::Path(path) => Some(path),
                _ => None,
            })
            .collect();
        assert!(!paths.is_empty(), "A failed discharge must visibly ripple over the globe");
        for path in paths {
            assert!(
                path.points
                    .iter()
                    .all(|point| point.distance(scene.planet) <= scene.planet_radius * 1.03),
                "Surface ripples must curve around the globe instead of expanding into space"
            );
        }
    }
    movie.elapsed = end_at + 1.4;
    assert_eq!(
        meshes_with_texture(
            &planet_strike_shapes(&movie, scene, &images),
            images.0[&movie.planet_image]
        )
        .len(),
        1,
        "A failed attack must never remove the planet"
    );
}

#[test]
fn planetary_blasts_and_surface_ripples_freeze_on_pause_and_reproduce_after_seeking() {
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    for destroyed in [false, true] {
        let mut movie = planet_strike_replay(destroyed);
        let images = planet_strike_images(&movie);
        let attack = &movie.timeline.planet_attacks[0];
        for time in [attack.discharge_at + 0.35, attack.end_at + 0.7, attack.end_at + 2.5] {
            movie.elapsed = time;
            let before = planet_strike_shapes(&movie, scene, &images);
            movie.advance(100.0, 4.0, true);
            assert_eq!(
                planet_strike_shapes(&movie, scene, &images),
                before,
                "Pausing must freeze every stage of the planetary effect"
            );
            movie.elapsed = 0.0;
            assert!(meshes_with_texture(
                &planet_strike_shapes(&movie, scene, &images),
                images.0["explosion"]
            )
            .is_empty());
            movie.elapsed = time;
            assert_eq!(planet_strike_shapes(&movie, scene, &images), before,
                "Seeking back to a timestamp must restore identical atlas frames, ripples and debris");
        }
    }
}
