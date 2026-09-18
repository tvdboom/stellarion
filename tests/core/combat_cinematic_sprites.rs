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
                assert_eq!(sample.len(), 1, "Hull must not be chopped into aiming pieces");
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
