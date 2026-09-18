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
