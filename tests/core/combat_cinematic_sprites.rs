use super::super::tests::{bombing_report, planet_strike_replay};
use super::*;
use bevy_egui::egui;

fn meshes(paint: impl Fn(&Painter)) -> Vec<Mesh> {
    let context = egui::Context::default();
    let mut result = context.run_ui(egui::RawInput::default(), |ui| paint(ui.painter()));
    result.textures_delta.clear();
    result
        .shapes
        .into_iter()
        .filter_map(|shape| match shape.shape {
            Shape::Mesh(mesh) => Some((*mesh).clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn aiming_keeps_every_turret_joint_closed_and_foundation_fixed() {
    let mut movie = CinematicPlayback::new(&bombing_report());
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let index = movie.timeline.shots[0].source;
    let launch = movie.timeline.shots[0].launch_at;
    for unit in [
        Defense::RocketLauncher,
        Defense::LightLaser,
        Defense::HeavyLaser,
        Defense::GaussCannon,
        Defense::IonCannon,
        Defense::PlasmaTurret,
    ] {
        let sheet = firing_sheet(Unit::Defense(unit)).unwrap();
        for mirror in [false, true] {
            let pose = ActorPose {
                center: pos2(500.0, 450.0),
                size: 240.0,
                angle: 0.0,
                mirror,
            };
            let mut foundation = None;
            for age in [-0.3, -0.1, 0.01, 0.10, 0.20, 0.3, 0.4, 0.52, 0.8] {
                movie.elapsed = launch + age;
                let sample = meshes(|painter| {
                    movie.paint_firing_sprite(
                        painter,
                        scene,
                        index,
                        pose,
                        sheet,
                        TextureId::User(10),
                    )
                });
                let mesh = &sample[0];
                for (base, illuminated) in mesh.vertices.iter().zip(&sample[1].vertices) {
                    assert_eq!(
                        base.pos, illuminated.pos,
                        "Surface lighting detached from an aimed gun"
                    );
                    assert_eq!(base.uv, illuminated.uv);
                }
                // Adjacent bands have separate UVs but MUST share their geometric seam.
                for band in 0..8 {
                    for column in 0..5 {
                        assert_eq!(
                            mesh.vertices[band * 10 + 5 + column].pos,
                            mesh.vertices[(band + 1) * 10 + column].pos,
                            "{unit:?}: aiming opened a gap in the pedestal"
                        );
                    }
                }
                let base = &mesh.vertices[80..];
                if let Some(ref expected) = foundation {
                    assert_eq!(base, expected);
                } else {
                    foundation = Some(base.to_vec());
                }
            }
        }
    }
}

#[test]
fn ships_and_stations_keep_complete_silhouettes_and_attached_muzzles() {
    let mut movie = CinematicPlayback::new(&bombing_report());
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let index = movie.timeline.shots[0].source;
    let launch = movie.timeline.shots[0].launch_at;
    for unit in [
        Unit::Ship(Ship::HeavyFighter),
        Unit::war_sun(),
        Unit::space_dock(),
        Unit::Defense(Defense::GaussCannon),
    ] {
        movie.timeline.actors[index].unit = unit;
        let sheet = firing_sheet(unit).unwrap();
        movie.visuals[index].firing_sheet = Some(sheet);
        for side in [Side::Attacker, Side::Defender] {
            movie.timeline.actors[index].side = side;
            for age in [-0.1, 0.01, 0.10, 0.3, 0.8] {
                movie.elapsed = launch + age;
                let pose = movie.actor_pose(scene, index, movie.elapsed);
                let sample = meshes(|painter| {
                    movie.paint_firing_sprite(
                        painter,
                        scene,
                        index,
                        pose,
                        sheet,
                        TextureId::User(10),
                    )
                });
                assert_eq!(
                    sample.len(),
                    2,
                    "A complete hull and its masked lighting share the same silhouette"
                );
                assert_eq!(sample[0].texture_id, sample[1].texture_id);
                assert!(
                    sample[1].vertices.iter().all(|vertex| vertex.color.a() == 0),
                    "Surface light must add radiance without covering the hull texture"
                );
                let hull = &sample[0];
                if !sheet.ground {
                    assert_eq!(hull.vertices.len(), 4);
                }
                let mouth = sheet.muzzle(movie.firing_frame(index, movie.elapsed));
                let actual = if sheet.ground {
                    hull.vertices[0].pos
                        + (hull.vertices[4].pos - hull.vertices[0].pos) * mouth.x
                        + (hull.vertices[5].pos - hull.vertices[0].pos)
                            * (mouth.y / sheet.weapon.bottom())
                } else {
                    let x = if pose.mirror {
                        1.0 - mouth.x
                    } else {
                        mouth.x
                    };
                    hull.vertices[0].pos
                        + (hull.vertices[1].pos - hull.vertices[0].pos) * x
                        + (hull.vertices[3].pos - hull.vertices[0].pos) * mouth.y
                };
                let target = movie.turret_target(scene, index, movie.elapsed);
                assert!(
                    actual.distance(movie.actor_muzzle(scene, index, movie.elapsed, target))
                        < 0.001
                );
            }
        }
    }
}

#[test]
fn war_sun_keeps_its_complete_shop_silhouette_through_every_firing_phase() {
    let mut movie = CinematicPlayback::new(&bombing_report());
    let index = movie.timeline.shots[0].source;
    movie.timeline.actors[index].unit = Unit::war_sun();
    let sheet = firing_sheet(Unit::war_sun()).unwrap();
    movie.visuals[index].firing_sheet = Some(sheet);
    let launch = movie.timeline.shots[0].launch_at;
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    assert_eq!(sheet.texture, "cinematic war sun");
    for age in [-0.3, -0.1, 0.01, 0.10, 0.20, 0.3, 0.4, 0.52, 0.8] {
        movie.elapsed = launch + age;
        let pose = movie.actor_pose(scene, index, movie.elapsed);
        let rendered = meshes(|painter| {
            movie.paint_firing_sprite(painter, scene, index, pose, sheet, TextureId::User(10))
        });
        let uv: Vec<_> = rendered[0].vertices.iter().map(|vertex| vertex.uv).collect();
        assert_eq!(
            uv,
            vec![pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)],
            "A firing phase must not crop the ring or swap it for a differently shaped frame"
        );
        assert!(sheet.muzzle(movie.firing_frame(index, movie.elapsed)).x > 0.90);
    }
    assert!(
        engine_ports(Ship::WarSun).iter().all(|&(x, _)| x < 0.1),
        "Forward weapon emitters must not be used as engine nozzles"
    );
}

#[test]
fn war_suns_move_bow_first_toward_the_focus_throughout_the_death_beam() {
    let movie = planet_strike_replay(true);
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let attack = &movie.timeline.planet_attacks[0];
    let focus = movie.planet_attack_focus(scene, attack);
    for &index in &attack.sources {
        for tick in 1..30 {
            let time = attack.start_at + (attack.end_at - attack.start_at) * tick as f32 / 30.0;
            let pose = movie.actor_pose(scene, index, time);
            let velocity = movie.actor_pose(scene, index, time + 0.01).center - pose.center;
            let bow = rotate(vec2(1.0, 0.0), pose.angle);
            assert!(velocity.length() > 0.01, "The firing hull froze");
            assert!(bow.dot(velocity.normalized()) > 0.99, "The firing hull slid backwards");
            assert!(
                bow.dot((focus - pose.center).normalized()) > 0.999,
                "The War Sun did not line up its axial cannon"
            );
            let muzzle = movie.actor_muzzle(scene, index, time, focus);
            assert!(
                (muzzle - pose.center).dot(bow) > pose.size * 0.12,
                "Death ray fired from the rear or center"
            );
        }
    }
}

#[test]
fn firing_cycle_has_safe_idle_boundaries_and_replay_is_seekable() {
    for time in [f32::NAN, f32::INFINITY, -100.0, 100.0] {
        assert_eq!(release_frame(time), 0);
    }
    let movie = CinematicPlayback::new(&bombing_report());
    let shot = &movie.timeline.shots[0];
    let samples: Vec<_> = (0..100)
        .map(|tick| {
            let time = shot.launch_at - 0.5 + tick as f32 * 0.015;
            (time, movie.firing_frame(shot.source, time))
        })
        .collect();
    for &(time, expected) in samples.iter().rev() {
        assert_eq!(movie.firing_frame(shot.source, time), expected);
    }
}

#[test]
fn planet_focus_stays_beside_the_world_even_when_the_fleet_is_far_north() {
    let mut movie = planet_strike_replay(true);
    for home in [vec2(0.15, -0.8), vec2(0.25, 0.10), vec2(0.8, 0.0), vec2(-0.4, 0.9)] {
        for visual in &mut movie.visuals {
            visual.home = home;
        }
        for size in [vec2(1440.0, 900.0), vec2(640.0, 360.0)] {
            let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
            let attack = &movie.timeline.planet_attacks[0];
            let focus = movie.planet_attack_focus(scene, attack);
            assert!(focus.x < scene.planet.x - scene.planet_radius);
            assert!((focus.y - scene.planet.y).abs() < scene.planet_radius * 0.1);
            for &source in &attack.sources {
                for tick in 0..=40 {
                    let time =
                        attack.start_at + (attack.end_at - attack.start_at) * tick as f32 / 40.0;
                    let pose = movie.actor_pose(scene, source, time);
                    let muzzle = movie.actor_muzzle(scene, source, time, focus);
                    let barrel = Vec2::angled(pose.angle);
                    assert!(barrel.dot((focus - muzzle).normalized()) > 0.9999,
                        "The actual cannon must aim at the convergence point throughout charging and firing");
                    assert!(pose.angle.abs() < 0.4);
                }
            }
        }
    }
}

#[test]
fn ordinary_ship_targeting_preserves_course_and_keeps_muzzles_attached() {
    use strum::IntoEnumIterator;

    for size in [vec2(1440.0, 900.0), vec2(640.0, 360.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
        for ship in Ship::iter().filter(|ship| firing_sheet(Unit::Ship(*ship)).is_some()) {
            for side in [Side::Attacker, Side::Defender] {
                let mut movie = CinematicPlayback::new(&bombing_report());
                movie.show_planet = false;
                let index = movie.timeline.shots[0].source;
                let target = movie.timeline.shots[0].target.unwrap();
                let launch = movie.timeline.shots[0].launch_at;
                movie.timeline.actors[index].unit = Unit::Ship(ship);
                movie.timeline.actors[index].side = side.clone();
                movie.visuals[index].firing_sheet = firing_sheet(Unit::Ship(ship));
                movie.visuals[index].bombing_target = None;
                movie.visuals[index].home = vec2(0.4, 0.4);
                let sign = if side == Side::Attacker {
                    1.0
                } else {
                    -1.0
                };
                for elevation in [-0.60_f32, 0.0, 0.60, 1.3] {
                    let center = movie.actor_flight_pose(scene, index, launch).center;
                    let offset =
                        vec2(sign * elevation.cos(), elevation.sin()) * 500.0 * scene.scale;
                    movie.visuals[target].home =
                        (center + offset - scene.planet) / scene.planet_radius;
                    for age in [0.0, 0.01, 0.1, 0.3] {
                        let time = launch + age;
                        let launches = std::mem::take(&mut movie.visuals[index].firing_times);
                        let course = movie.actor_pose(scene, index, time);
                        movie.visuals[index].firing_times = launches;
                        let pose = movie.actor_pose(scene, index, time);
                        let aim = movie.turret_target(scene, index, time);
                        let muzzle = movie.actor_muzzle(scene, index, time, aim);
                        assert_eq!(pose.mirror, course.mirror);
                        assert!((pose.angle - course.angle).abs() <= ship_aim_limit(Unit::Ship(ship)) + 0.000_001,
                            "{ship:?} on {side:?} tilted its complete hull toward elevation {elevation}");
                        let sheet = movie.visuals[index].firing_sheet.unwrap();
                        let dimensions = movie.hull_dimensions(
                            scene,
                            index,
                            pose,
                            time,
                            sheet.dimensions(pose.size),
                        );
                        let mouth = sheet.muzzle(movie.firing_frame(index, time));
                        let attached = engine_attachment(pose, dimensions, (mouth.x, mouth.y));
                        assert!(
                            muzzle.distance(attached) < 0.001,
                            "Off-axis fire must still leave the rendered cannon muzzle"
                        );
                        assert_eq!(movie.actor_pose(scene, index, time).angle, pose.angle);
                    }
                }
            }
        }
    }
}

#[test]
fn rapidly_alternating_high_and_low_targets_do_not_wrench_ship_attitude() {
    let mut movie = CinematicPlayback::new(&bombing_report());
    let index = movie.timeline.shots[0].source;
    let targets: Vec<_> = movie
        .visuals
        .iter()
        .enumerate()
        .filter_map(|(index, visual)| visual.ground.then_some(index))
        .take(3)
        .collect();
    for (slot, &target) in targets.iter().enumerate() {
        movie.visuals[target].home = vec2(
            -0.6,
            if slot == 1 {
                4.0
            } else {
                -4.0
            },
        );
        let shot = &mut movie.timeline.shots[slot];
        shot.source = index;
        shot.target = Some(target);
        shot.launch_at = 5.0 + slot as f32 * 0.16;
        shot.impact_at = shot.launch_at + 0.5;
    }
    movie.visuals[index].firing_times = vec![5.0, 5.16, 5.32];
    movie.visuals[index].firing_shots = vec![0, 1, 2];
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    for ship in [Ship::LightFighter, Ship::Battleship, Ship::Dreadnought] {
        movie.timeline.actors[index].unit = Unit::Ship(ship);
        let sample = |time| {
            let mut pose = ActorPose {
                center: pos2(250.0, 400.0),
                size: 100.0,
                angle: 0.07,
                mirror: false,
            };
            movie.aim_ship_cannon(scene, index, time, &mut pose);
            pose.angle
        };
        let mut previous = sample(3.0);
        for tick in 1..=1200 {
            let time = 3.0 + tick as f32 * 0.005;
            let angle = sample(time);
            assert!((angle - 0.07).abs() <= ship_aim_limit(Unit::Ship(ship)) + 0.000_001);
            assert!(
                (angle - previous).abs() / 0.005 < 0.25,
                "{ship:?} snapped between targets at {time}: {previous} -> {angle}"
            );
            assert_eq!(angle, sample(time), "Attitude must remain frame-independent");
            previous = angle;
        }
        assert_eq!(sample(8.0), 0.07, "The ship should return to its travel attitude");
    }
}

#[test]
fn ships_recover_their_flight_heading_between_salvos_without_rapid_fire_nodding() {
    for ship in [Ship::LightFighter, Ship::Battleship, Ship::WarSun] {
        let unit = Unit::Ship(ship);
        for launch in [5.0, 10.0] {
            for offset in [-0.15, 0.0, 0.30] {
                assert_eq!(salvo_aim_weight(&[5.0, 10.0], launch + offset, unit), 1.0);
            }
        }
        assert_eq!(salvo_aim_weight(&[5.0, 10.0], 7.0, unit), 0.0);
        for tick in 0..=100 {
            let time = 5.0 + tick as f32 * 0.006;
            assert_eq!(salvo_aim_weight(&[5.0, 5.2, 5.4, 5.6], time, unit), 1.0);
        }
        for tick in 0..2500 {
            let time = 3.5 + tick as f32 * 0.002;
            let change =
                salvo_aim_weight(&[5.0], time + 0.002, unit) - salvo_aim_weight(&[5.0], time, unit);
            assert!(change.abs() < 0.006, "Salvo preparation or recovery snapped");
        }
    }
    assert!(salvo_aim_weight(&[5.0], 4.0, Unit::war_sun()) > 0.0);
    assert_eq!(salvo_aim_weight(&[5.0], 4.0, Unit::Ship(Ship::LightFighter)), 0.0);

    let mut movie = CinematicPlayback::new(&bombing_report());
    let index = movie.timeline.shots[0].source;
    movie.visuals[index].firing_times = vec![5.0, 10.0];
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let mut pose = ActorPose {
        center: pos2(250.0, 250.0),
        size: 100.0,
        angle: 0.2,
        mirror: false,
    };
    movie.aim_ship_cannon(scene, index, 7.0, &mut pose);
    assert_eq!(pose.angle, 0.2, "Idle cannon alignment must leave the flight heading alone");
}

#[test]
fn closely_spaced_salvos_aim_at_each_recorded_target_at_release() {
    let mut movie = CinematicPlayback::new(&bombing_report());
    let index = movie.timeline.shots[0].source;
    let targets: Vec<_> = movie
        .visuals
        .iter()
        .enumerate()
        .filter_map(|(i, visual)| visual.ground.then_some(i))
        .take(3)
        .collect();
    let launches = [5.0, 5.2, 5.4];
    movie.visuals[index].firing_times = launches.to_vec();
    movie.visuals[index].firing_shots = vec![0, 1, 2];
    for slot in 0..3 {
        let shot = &mut movie.timeline.shots[slot];
        shot.source = index;
        shot.target = Some(targets[slot]);
        shot.launch_at = launches[slot];
        shot.impact_at = launches[slot] + 0.8;
    }
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    for (slot, &launch) in launches.iter().enumerate() {
        let expected = movie.actor_flight_pose(scene, targets[slot], launch + 0.8).center;
        let actual = movie.turret_target(scene, index, launch);
        assert!(actual.distance(expected) < 0.001, "Look-ahead skipped the current target");
        let before = movie.turret_target(scene, index, launch - 0.001);
        let after = movie.turret_target(scene, index, launch + 0.001);
        assert!(before.distance(after) < 0.1, "Changing targets must remain continuous at release");
    }
}

#[test]
fn cinematic_hulls_do_not_add_decorative_owner_bands() {
    let mut report = bombing_report();
    let round = &mut report.combat_report.as_mut().unwrap().rounds[0];
    for (army, owners) in [(&mut round.attacker, [1, 3]), (&mut round.defender, [2, 4])] {
        for (index, record) in army.iter_mut().enumerate() {
            record.owner = Some(owners[index % 2]);
        }
    }
    let mut movie = CinematicPlayback::new(&report);
    // Deliberately differ from player-slot defaults: lobby-selected colors are authoritative.
    let palette = |owner: PlayerId| {
        let [r, g, b] = PlayerColor::new((owner as u8 + 1) % 6).unwrap().rgb();
        Color32::from_rgb(r, g, b)
    };
    movie.set_owner_colors(palette);
    for (index, actor) in movie.timeline.actors.iter().enumerate() {
        let Some(_owner) = actor.owner else {
            continue;
        };
        let sheet = movie.visuals[index].firing_sheet.unwrap();
        for frame in 0..8 {
            for mirror in [false, true] {
                let pose = ActorPose {
                    center: pos2(500.0, 450.0),
                    size: 150.0,
                    angle: 0.5,
                    mirror,
                };
                let sample = meshes(|painter| {
                    paint_owner_hull(
                        painter,
                        TextureId::User(10),
                        pose.center,
                        sheet.dimensions(pose.size),
                        pose.angle,
                        mirror,
                        frame_uv(frame, FULL_UV),
                        movie.visuals[index].owner_color,
                    );
                });
                let mesh = &sample[0];
                assert_eq!(sample.len(), 1);
                assert_eq!(mesh.vertices.len(), 4, "Lighting cannot split armor into bands");
                assert_eq!(mesh.vertices[0].color, mesh.vertices[1].color);
                assert_eq!(mesh.vertices[2].color, mesh.vertices[3].color);
                assert_eq!(mesh.vertices[0].color, Color32::WHITE);
                assert_eq!(mesh.vertices[2].color, Color32::from_rgb(210, 225, 242));
                let uv = frame_uv(frame, FULL_UV);
                for vertex in &mesh.vertices {
                    assert!(uv.contains(vertex.uv), "Paint leaked into another firing frame");
                    let point = (vertex.uv - uv.min) / uv.size();
                    let reflection = vec2(
                        if mirror {
                            -1.0
                        } else {
                            1.0
                        },
                        1.0,
                    );
                    let expected = pose.center
                        + rotate(
                            (point - Vec2::splat(0.5)) * sheet.dimensions(pose.size) * reflection,
                            pose.angle,
                        );
                    assert!(vertex.pos.distance(expected) < 0.001);
                }
            }
        }
    }
    movie.timeline.actors[0].owner = None;
    movie.set_owner_colors(palette);
    assert!(
        movie.visuals[0].owner_color.is_none(),
        "Neutral units cannot inherit a side's player color"
    );
}

#[test]
fn engine_plumes_attach_to_mirrored_hulls_and_fade_without_hard_edges() {
    use strum::IntoEnumIterator;

    for ship in Ship::iter() {
        let dimensions = firing_sheet(Unit::Ship(ship))
            .map_or(vec2(100.0, 70.0), |sheet| sheet.dimensions(100.0));
        for mirror in [false, true] {
            let pose = ActorPose {
                center: pos2(200.0, 200.0),
                size: 100.0,
                angle: 0.4,
                mirror,
            };
            let axis = rotate(
                vec2(
                    if mirror {
                        -1.0
                    } else {
                        1.0
                    },
                    0.0,
                ),
                pose.angle,
            );
            for &port in engine_ports(ship) {
                let origin = engine_attachment(pose, dimensions, port);
                assert!((origin - pose.center).dot(axis) < 0.0, "Engine must remain at the stern");
                let mesh = engine_plume_mesh(origin, axis, 30.0, 4.0, Vec2::ZERO, BLUE, 0.8);
                assert_eq!(mesh.vertices[2].pos, origin, "Nozzle cannot detach from hull");
                assert!(mesh
                    .vertices
                    .iter()
                    .all(|vertex| (vertex.pos - origin).dot(axis) <= 0.001));
                for row in mesh.vertices.chunks_exact(5) {
                    assert_eq!(row[0].color.a(), 0);
                    assert_eq!(row[4].color.a(), 0);
                }
                assert!(mesh.vertices[40..].iter().all(|vertex| vertex.color.a() == 0));
                assert!(mesh.indices.iter().all(|&index| (index as usize) < mesh.vertices.len()));
            }
        }
    }
}

#[test]
fn propulsion_uses_replay_time_and_excludes_ground_equipment() {
    let mut movie = CinematicPlayback::new(&bombing_report());
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let index = movie.timeline.shots[0].source;
    let capture = |movie: &CinematicPlayback| {
        meshes(|painter| {
            movie.paint_ship_propulsion(
                painter,
                scene,
                index,
                movie.actor_pose(scene, index, movie.elapsed),
            )
        })
    };
    movie.elapsed = 1.5;
    let before = capture(&movie);
    assert!(!before.is_empty());
    movie.elapsed = 7.0;
    assert_ne!(before, capture(&movie));
    movie.elapsed = 1.5;
    assert_eq!(before, capture(&movie), "Seeking must reproduce the same plume geometry");
    movie.timeline.actors[index].unit = Unit::space_dock();
    assert!(capture(&movie).is_empty(), "Stations must not inherit ship engines");
}

#[test]
fn hull_illumination_preserves_atlas_masks_and_localizes_surface_light() {
    let pose = ActorPose {
        center: pos2(200.0, 150.0),
        size: 100.0,
        angle: 0.3,
        mirror: true,
    };
    let light = HullLighting {
        center: pose.center,
        size: pose.size,
        upper_surface: vec2(0.0, -1.0),
        roll: 0.0,
        planet: None,
        flashes: [
            Some(HullLight {
                position: pose.center + vec2(30.0, 0.0),
                radius: 60.0,
                color: GOLD,
                strength: 0.8,
            }),
            None,
        ],
        engine: None,
    };
    assert!(
        light.color_at(pose.center + vec2(30.0, 0.0)).r()
            > light.color_at(pose.center - vec2(30.0, 0.0)).r()
    );
    for frame in 0..8 {
        let uv = frame_uv(frame, FULL_UV);
        let mesh = lit_hull_mesh(TextureId::User(23), pose, vec2(100.0, 70.0), uv, &light);
        assert_eq!(mesh.texture_id, TextureId::User(23));
        assert!(mesh.vertices.iter().all(|vertex| uv.contains(vertex.uv) && vertex.color.a() == 0));
        for vertex in mesh.vertices {
            let fraction = (vertex.uv - uv.min) / uv.size();
            let expected = pose.center
                + rotate(
                    (vec2(1.0 - fraction.x, fraction.y) - Vec2::splat(0.5)) * vec2(100.0, 70.0),
                    pose.angle,
                );
            assert!(vertex.pos.distance(expected) < 0.001);
        }
    }
}

#[test]
fn surface_flashes_follow_recorded_launches_and_real_contacts_only() {
    let mut movie = CinematicPlayback::new(&bombing_report());
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let source = movie.timeline.shots[0].source;
    let target = movie.timeline.shots[0].target.unwrap();
    let launch = movie.timeline.shots[0].launch_at;
    let impact = movie.timeline.shots[0].impact_at;
    let sample = |movie: &CinematicPlayback, index| {
        movie.hull_lighting(scene, index, movie.actor_pose(scene, index, movie.elapsed))
    };
    movie.elapsed = launch + 0.01;
    assert!(sample(&movie, source).flashes[0].is_some());
    movie.elapsed = launch + 0.21;
    assert!(sample(&movie, source).flashes[0].is_none());

    movie.timeline.shots[0].outcome.hull_damage = 20;
    movie.elapsed = impact - 0.01;
    assert!(sample(&movie, target).flashes[1].is_none());
    movie.elapsed = impact + 0.01;
    assert!(sample(&movie, target).flashes[1].is_some());
    movie.elapsed = impact + 0.37;
    assert!(sample(&movie, target).flashes[1].is_none());
    movie.elapsed = impact + 0.01;
    movie.timeline.shots[0].outcome.hull_damage = 0;
    movie.timeline.shots[0].outcome.planetary_shield_damage = 20;
    assert!(
        sample(&movie, target).flashes[1].is_none(),
        "A contact at the planetary shield cannot illuminate a protected hull"
    );
}

#[test]
fn maneuvering_jets_require_course_changes_and_scale_with_the_viewport() {
    assert_eq!(maneuver_thrust(vec2(40.0, 0.0), Vec2::ZERO, 1.0), (0.0, 0.0));
    assert_eq!(maneuver_thrust(vec2(40.0, 0.0), vec2(25.0, 0.0), 1.0), (0.0, 0.0));
    let turning = maneuver_thrust(vec2(40.0, 0.0), vec2(0.0, 50.0), 1.0);
    assert!(turning.0 > 0.0 && turning.1 == 0.0);
    assert_eq!(turning, maneuver_thrust(vec2(20.0, 0.0), vec2(0.0, 25.0), 0.5));
    let opposite = maneuver_thrust(vec2(-40.0, 0.0), vec2(0.0, 50.0), 1.0);
    assert_eq!(turning.0, -opposite.0);
    let braking = maneuver_thrust(vec2(40.0, 0.0), vec2(-100.0, 0.0), 1.0);
    assert!(braking.0 == 0.0 && braking.1 > 0.0);
}

#[test]
fn perspective_banking_is_mass_sensitive_bounded_and_releases_at_firing_alignment() {
    let straight = vec2(10.0, 0.0);
    let turning = vec2(9.0, 3.0);
    let fighter = Unit::Ship(Ship::LightFighter);
    assert_eq!(perspective_roll(straight, straight, 0.4, fighter), 0.0);
    let light = perspective_roll(straight, turning, 0.4, fighter);
    let heavy = perspective_roll(straight, turning, 0.4, Unit::war_sun());
    assert!(light > heavy && heavy > 0.0);
    assert_eq!(light, -perspective_roll(straight, vec2(9.0, -3.0), 0.4, fighter));
    assert_eq!(light, perspective_roll(straight * 0.5, turning * 0.5, 0.4, fighter));
    assert!(perspective_roll(straight, -straight, 0.4, fighter).abs() <= 1.0);

    let movie = CinematicPlayback::new(&bombing_report());
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let shot = &movie.timeline.shots[0];
    for time in [shot.launch_at - 0.15, shot.launch_at, shot.launch_at + 0.30] {
        let pose = movie.actor_pose(scene, shot.source, time);
        assert_eq!(movie.hull_roll(scene, shot.source, time), 0.0);
        assert_eq!(
            movie.hull_dimensions(scene, shot.source, pose, time, vec2(150.0, 100.0)),
            vec2(150.0, 100.0)
        );
    }
    for index in 0..movie.timeline.actors.len() {
        for tick in 0..60 {
            let time = tick as f32 * 0.15;
            let pose = movie.actor_pose(scene, index, time);
            let dimensions = movie.hull_dimensions(scene, index, pose, time, vec2(150.0, 100.0));
            assert_eq!(dimensions.x, 150.0);
            assert!((92.0..=100.0).contains(&dimensions.y));
        }
    }
}

#[test]
fn banked_muzzles_and_nozzles_remain_attached_to_the_rendered_texture() {
    let mut movie = CinematicPlayback::new(&bombing_report());
    let index = movie.timeline.shots[0].source;
    movie.timeline.actors[index].unit = Unit::Ship(Ship::LightFighter);
    movie.visuals[index].firing_sheet = firing_sheet(Unit::Ship(Ship::LightFighter));
    movie.visuals[index].firing_times.clear();
    let sheet = movie.visuals[index].firing_sheet.unwrap();
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let mut maximum_bank = 0.0_f32;
    for tick in 1..18 {
        movie.elapsed = tick as f32 * 0.2;
        let pose = movie.actor_pose(scene, index, movie.elapsed);
        maximum_bank = maximum_bank.max(movie.hull_roll(scene, index, movie.elapsed).abs());
        let samples = meshes(|painter| {
            movie.paint_firing_sprite(painter, scene, index, pose, sheet, TextureId::User(10))
        });
        let hull = &samples[0];
        let textured_point = |point: Pos2| {
            let x = if pose.mirror {
                1.0 - point.x
            } else {
                point.x
            };
            hull.vertices[0].pos
                + (hull.vertices[1].pos - hull.vertices[0].pos) * x
                + (hull.vertices[3].pos - hull.vertices[0].pos) * point.y
        };
        let muzzle = movie.actor_muzzle(scene, index, movie.elapsed, scene.planet);
        assert!(muzzle.distance(textured_point(sheet.muzzle(0))) < 0.001);
        for &(x, y) in engine_ports(Ship::LightFighter) {
            let attachment = movie.hull_attachment(
                scene,
                index,
                pose,
                movie.elapsed,
                (vec2(x, y) - Vec2::splat(0.5)) * sheet.dimensions(pose.size),
            );
            assert!(attachment.distance(textured_point(pos2(x, y))) < 0.001);
        }
        for vertex in &samples[1].vertices {
            let uv = frame_uv(0, FULL_UV);
            let point = (vertex.uv - uv.min) / uv.size();
            assert!(
                vertex.pos.distance(textured_point(point.to_pos2())) < 0.001,
                "Banked lighting drifted away from the underlying hull"
            );
        }
    }
    assert!(maximum_bank > 0.001, "A curved flight should show some perspective banking");
}

#[test]
fn propulsion_does_not_react_to_a_hull_impact_before_contact() {
    let mut movie = CinematicPlayback::new(&bombing_report());
    let index = movie.timeline.shots[0].source;
    let incoming = 1;
    let impact = 10.0;
    movie.timeline.shots[incoming].target = Some(index);
    movie.timeline.shots[incoming].impact_at = impact;
    movie.timeline.shots[incoming].outcome.killed = false;
    movie.timeline.shots[incoming].outcome.missed = false;
    movie.timeline.shots[incoming].outcome.shield_damage = 0;
    movie.timeline.shots[incoming].outcome.planetary_shield_damage = 0;
    movie.visuals[index].incoming_shots = vec![incoming];
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let capture = |movie: &CinematicPlayback| {
        meshes(|painter| {
            movie.paint_ship_propulsion(
                painter,
                scene,
                index,
                movie.actor_pose(scene, index, movie.elapsed),
            );
        })
    };
    for time in [impact - 0.10, impact - 0.04, impact - 0.001] {
        movie.elapsed = time;
        movie.timeline.shots[incoming].outcome.hull_damage = 0;
        let calm = capture(&movie);
        movie.timeline.shots[incoming].outcome.hull_damage = 40;
        assert_eq!(calm, capture(&movie), "Exhaust anticipated a hull impact at {time}");
    }
    movie.elapsed = impact + 0.12;
    movie.timeline.shots[incoming].outcome.hull_damage = 0;
    let calm = capture(&movie);
    movie.timeline.shots[incoming].outcome.hull_damage = 40;
    assert_ne!(
        calm,
        capture(&movie),
        "Engines should respond once the recorded impact moves the hull"
    );
}
