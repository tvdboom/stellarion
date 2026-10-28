use super::*;
use crate::core::combat::report::{CombatReport, RoundReport};
use crate::core::combat::resolution::CombatUnit;
use crate::core::missions::Mission;

#[test]
fn later_nonpenetrating_contacts_do_not_remove_surface_scars() {
    let mut report = super::super::tests::bombing_report();
    let attackers = &mut report.combat_report.as_mut().unwrap().rounds[0].attacker;
    for id in 100..105 {
        let mut missed = attackers[2].clone();
        missed.id = id;
        attackers.push(missed);
    }
    let mut movie = CinematicPlayback::new(&report);
    let target =
        movie.timeline.shots.iter().find(|shot| shot.outcome.killed).unwrap().target.unwrap();
    assert!(movie.visuals[target].incoming_shots.len() > movie.visuals[target].surface_hits.len());
    assert_eq!(movie.visuals[target].surface_hits.len(), 2);
    movie.elapsed = movie.timeline.duration;
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    assert!(!super::super::tests::capture_planet_shapes(scene, |painter| movie
        .paint_surface_damage(painter, scene))
    .is_empty());
}

#[test]
fn surface_damage_requires_recorded_hits_and_is_fixed_seekable_and_absent_on_gas() {
    let mut movie = CinematicPlayback::new(&super::super::tests::bombing_report());
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let capture = |movie: &CinematicPlayback| {
        super::super::tests::capture_planet_shapes(scene, |painter| {
            movie.paint_surface_damage(painter, scene)
        })
    };
    let first = movie
        .timeline
        .shots
        .iter()
        .filter(|shot| shot.outcome.killed)
        .map(|shot| shot.impact_at)
        .min_by(f32::total_cmp)
        .unwrap();
    movie.elapsed = first - 0.01;
    assert!(capture(&movie).is_empty(), "Surface scars must not appear before a real hit");
    movie.elapsed = first + 0.5;
    let active = capture(&movie);
    assert!(!active.is_empty());
    movie.elapsed = first + 4.0;
    let settled = capture(&movie);
    assert!(!settled.is_empty(), "A successful strike leaves a persistent surface mark");
    assert_ne!(active, settled);
    movie.elapsed = first + 0.5;
    assert_eq!(capture(&movie), active);
    movie.gas_planet = true;
    assert!(capture(&movie).is_empty());
    movie.gas_planet = false;
    for shot in &mut movie.timeline.shots {
        shot.outcome.missed = true;
    }
    assert!(capture(&movie).is_empty(), "Misses cannot burn a recorded target");
}

#[test]
fn mixed_defense_footprints_fit_the_surface_without_overlapping_their_neighbors() {
    for count in [1, 7, 34, 150, 600] {
        let mut report = crate::test_support::empty_report(
            Mission::default(),
            Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
        );
        report.combat_report = Some(CombatReport {
            rounds: vec![RoundReport {
                defender: (0..count)
                    .map(|id| CombatUnit {
                        fleet_id: 0,
                        battle_plan: Default::default(),
                        id,
                        unit: Unit::Defense(match id % 3 {
                            0 => Defense::RocketLauncher,
                            1 => Defense::IonCannon,
                            _ => Defense::PlasmaTurret,
                        }),
                        owner: Some(2),
                        hull: 100,
                        shield: 0,
                        repairs: Vec::new(),
                        shots: Vec::new(),
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        });
        let movie = CinematicPlayback::new(&report);
        for size in [vec2(1440.0, 820.0), vec2(640.0, 360.0), vec2(450.0, 700.0)] {
            let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
            let poses: Vec<_> =
                (0..movie.visuals.len()).map(|index| movie.actor_pose(scene, index, 5.0)).collect();
            for (index, pose) in poses.iter().enumerate() {
                assert!(
                    pose.center.distance(scene.planet) + pose.size * 0.4 < scene.planet_radius,
                    "The complete pedestal must remain on the visible surface"
                );
                for other in &poses[..index] {
                    let separation = pose.center - other.center;
                    assert!(
                        separation.x.abs() >= (pose.size + other.size) * 0.43
                            || separation.y.abs() >= (pose.size + other.size) * 0.29,
                        "Mixed gun sizes must have separate readable silhouettes: {count}"
                    );
                }
            }
        }
    }
}

#[test]
fn support_vehicles_have_separate_homes_and_crawler_patrol_lanes() {
    // The three-unit case previously left the truck directly underneath the relocated gun.
    for count in [3, 11, 34, 150] {
        let mut report = crate::test_support::empty_report(
            Mission::default(),
            Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
        );
        report.combat_report = Some(CombatReport {
            rounds: vec![RoundReport {
                defender: (0..count)
                    .map(|id| CombatUnit {
                        fleet_id: 0,
                        battle_plan: Default::default(),
                        id,
                        unit: Unit::Defense(match id % 3 {
                            0 => Defense::Crawler,
                            1 => Defense::RepairTruck,
                            _ => Defense::RocketLauncher,
                        }),
                        owner: Some(2),
                        hull: 100,
                        shield: 0,
                        repairs: Vec::new(),
                        shots: Vec::new(),
                    })
                    .collect(),
                ..Default::default()
            }],
            ..Default::default()
        });
        let movie = CinematicPlayback::new(&report);
        for size in [vec2(1440.0, 820.0), vec2(640.0, 360.0), vec2(450.0, 700.0)] {
            let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
            for frame in 0..=40 {
                let poses: Vec<_> = (0..movie.visuals.len())
                    .map(|index| movie.actor_pose(scene, index, frame as f32))
                    .collect();
                for (index, pose) in poses.iter().enumerate() {
                    assert!(
                        pose.center.distance(scene.planet) + pose.size * 0.4 < scene.planet_radius
                    );
                    for other in &poses[..index] {
                        let separation = pose.center - other.center;
                        assert!(
                            separation.x.abs() >= (pose.size + other.size) * 0.43
                                || separation.y.abs() >= (pose.size + other.size) * 0.29,
                            "Support vehicles must retain clear parking and patrol lanes: {count}, frame {frame}"
                        );
                    }
                }
            }
        }
    }
}

#[test]
fn environmental_meshes_have_finite_geometry_and_transparent_outer_edges() {
    let context = bevy_egui::egui::Context::default();
    let rect = Rect::from_min_size(Pos2::ZERO, vec2(640.0, 480.0));
    let mut output = context.run_ui(
        bevy_egui::egui::RawInput {
            screen_rect: Some(rect),
            ..Default::default()
        },
        |ui| {
            paint_atmosphere(ui.painter(), rect.center(), 100.0, BLUE);
            soft_ellipse(ui.painter(), rect.center(), vec2(60.0, 15.0), Color32::BLACK, 0.5);
            paint_vignette(ui.painter(), rect);
        },
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
    assert_eq!(meshes.len(), 3);
    for mesh in meshes {
        assert!(mesh.is_valid());
        assert!(mesh.vertices.iter().all(|vertex| vertex.pos.is_finite()));
        assert!(mesh.vertices.iter().any(|vertex| vertex.color.a() == 0));
    }
}
