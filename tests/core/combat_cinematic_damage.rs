use super::*;
use crate::core::combat::report::{CombatReport, RoundReport};
use crate::core::combat::resolution::CombatUnit;
use crate::core::missions::Mission;
use crate::core::units::Combat;

fn motion(mass: f32, mirror: bool) -> WreckMotion {
    WreckMotion {
        origin: ActorPose {
            center: pos2(320.0, 240.0),
            size: 90.0,
            angle: 0.0,
            mirror,
        },
        dimensions: vec2(90.0, 60.0),
        velocity: vec2(18.0, -5.0),
        mass,
        phase: 0.4,
    }
}

#[test]
fn wrecks_retain_inertia_while_capital_sections_tumble_more_slowly() {
    let light = motion(0.0, false);
    let heavy = motion(1.0, false);
    assert!(light.lifetime() < heavy.lifetime());
    assert!(heavy.lifetime() <= 2.7, "Visible hulls must clear before the closing banner");
    for piece in 0..3 {
        let moving = heavy.fragment(piece, 0.75).unwrap();
        let stationary = WreckMotion {
            velocity: Vec2::ZERO,
            ..heavy
        }
        .fragment(piece, 0.75)
        .unwrap();
        assert!(
            (moving.center - stationary.center - heavy.velocity * 0.75).length() < 0.001,
            "A destroyed hull must retain its actual pre-death momentum"
        );
        assert!(moving.angle.abs() < light.fragment(piece, 0.75).unwrap().angle.abs());
        assert!(
            moving.opacity < 1.0,
            "Dead hull sections must already be charred, not alive duplicates"
        );
        assert!(heavy.fragment(piece, -0.01).is_none());
        assert!(heavy.fragment(piece, f32::NAN).is_none());
        assert!(heavy.fragment(piece, heavy.lifetime()).is_none());
    }
}

#[test]
fn torn_hull_meshes_crop_one_atlas_cell_and_preserve_the_original_mirror() {
    let uv = Rect::from_min_max(Pos2::ZERO, pos2(0.25, 0.5));
    for piece in 0..3 {
        let original = motion(0.0, false);
        let mirrored = motion(0.0, true);
        let left = fragment_mesh(
            TextureId::User(1),
            uv,
            original,
            piece,
            original.fragment(piece, 0.0).unwrap(),
        );
        let right = fragment_mesh(
            TextureId::User(1),
            uv,
            mirrored,
            piece,
            mirrored.fragment(piece, 0.0).unwrap(),
        );
        assert!(left.is_valid() && right.is_valid());
        assert!(
            left.vertices.len() > 5,
            "Broken sections must have torn seams rather than square cuts"
        );
        for (left, right) in left.vertices.iter().zip(&right.vertices) {
            assert!(uv.contains(left.uv));
            assert_eq!(left.uv, right.uv);
            assert!((left.pos.x + right.pos.x - original.origin.center.x * 2.0).abs() < 0.001);
            assert_eq!(left.pos.y, right.pos.y);
        }
    }
}

fn destruction_movie() -> CinematicPlayback {
    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
    );
    report.combat_report = Some(CombatReport {
        rounds: vec![RoundReport {
            attacker: vec![CombatUnit {
                fleet_id: 0,
                battle_plan: Default::default(),
                id: 41,
                unit: Unit::Ship(Ship::Battleship),
                owner: Some(1),
                hull: 0,
                shield: 0,
                repairs: Vec::new(),
                shots: Vec::new(),
            }],
            ..Default::default()
        }],
        ..Default::default()
    });
    let mut movie = CinematicPlayback::new(&report);
    movie.timeline.actors[0].death_at = Some(6.0);
    movie
}

fn capture(movie: &CinematicPlayback, scene: Scene, images: &ImageIds) -> Vec<Mesh> {
    let context = bevy_egui::egui::Context::default();
    let mut output = context.run_ui(
        bevy_egui::egui::RawInput {
            screen_rect: Some(scene.rect),
            ..Default::default()
        },
        |ui| movie.paint_actor_destruction(ui.painter(), scene, images, 0, 6.0),
    );
    output.textures_delta.clear();
    output
        .shapes
        .into_iter()
        .filter_map(|shape| match shape.shape {
            Shape::Mesh(mesh) => Some((*mesh).clone()),
            _ => None,
        })
        .collect()
}

#[test]
fn destruction_rewinds_exactly_and_never_uses_an_uncropped_firing_atlas() {
    let mut movie = destruction_movie();
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let hull = TextureId::User(11);
    let images = ImageIds(
        [
            ("firing battleship".into(), hull),
            ("explosion".into(), TextureId::User(12)),
            ("combat fx glow".into(), TextureId::User(13)),
            ("combat fx shard".into(), TextureId::User(14)),
        ]
        .into(),
    );
    movie.elapsed = 5.9;
    assert!(capture(&movie, scene, &images).is_empty());
    movie.elapsed = 6.7;
    let before = capture(&movie, scene, &images);
    let pieces: Vec<_> = before.iter().filter(|mesh| mesh.texture_id == hull).collect();
    assert_eq!(pieces.len(), 3, "The dead ship should consist solely of three separating sections");
    assert!(pieces
        .iter()
        .all(|mesh| mesh.vertices.iter().all(
            |vertex| (0.0..=0.25).contains(&vertex.uv.x) && (0.0..=0.5).contains(&vertex.uv.y)
        )));
    movie.elapsed = 9.6;
    assert!(capture(&movie, scene, &images).is_empty());
    movie.elapsed = 6.7;
    assert_eq!(
        capture(&movie, scene, &images),
        before,
        "Debris, smoke and embers must reproduce exactly when the clock rewinds"
    );
    movie.advance(0.1, 4.0, true);
    assert_eq!(capture(&movie, scene, &images), before);
}

fn damaged_movie(unit: Unit, repair: bool) -> (CinematicPlayback, usize) {
    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), bevy::math::Vec2::ZERO, false, 1.0),
    );
    let damage = unit.hull() * 3 / 4;
    report.combat_report = Some(CombatReport {
        rounds: vec![RoundReport {
            attacker: vec![CombatUnit {
                fleet_id: 0,
                battle_plan: Default::default(),
                id: 7,
                unit: Unit::Ship(Ship::Cruiser),
                owner: Some(1),
                hull: Unit::Ship(Ship::Cruiser).hull(),
                shield: 0,
                repairs: Vec::new(),
                shots: vec![super::super::super::resolution::ShotReport {
                    target_id: Some(41),
                    unit: Some(unit),
                    hull_damage: damage,
                    shield_damage: unit.shield(),
                    ..Default::default()
                }],
            }],
            defender: vec![CombatUnit {
                fleet_id: 0,
                battle_plan: Default::default(),
                id: 41,
                unit,
                owner: Some(2),
                hull: if repair {
                    unit.hull()
                } else {
                    unit.hull() - damage
                },
                shield: 0,
                repairs: if repair {
                    vec![damage]
                } else {
                    Vec::new()
                },
                shots: Vec::new(),
            }],
            ..Default::default()
        }],
        ..Default::default()
    });
    let movie = CinematicPlayback::new(&report);
    let index = movie.timeline.actors.iter().position(|actor| actor.id == Some(41)).unwrap();
    (movie, index)
}

#[test]
fn damage_vapor_begins_after_actual_hull_damage_and_rewinds_exactly() {
    let (mut movie, index) = damaged_movie(Unit::Ship(Ship::Battleship), false);
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let impact = movie.timeline.shots[0].impact_at;
    assert!(
        movie.damage_plume(scene, index, impact - 0.01, impact + 0.5).is_none(),
        "A later hit cannot retroactively create a smoke trail"
    );
    assert!(movie.damage_plume(scene, index, impact + 0.5, impact).is_none());
    let plume = movie.damage_plume(scene, index, impact + 0.01, impact + 0.8).unwrap();
    assert!(plume.center.is_finite() && plume.radius > 0.0 && plume.opacity > 0.0);
    assert!(movie.damage_plume(scene, index, impact + 0.01, impact + 1.7).is_none());
    movie.elapsed = movie.timeline.duration;
    movie.elapsed = impact + 0.8;
    assert_eq!(movie.damage_plume(scene, index, impact + 0.01, movie.elapsed), Some(plume));
}

#[test]
fn repairs_stop_new_venting_but_allow_emitted_vapor_to_disperse() {
    let (movie, index) = damaged_movie(Unit::Defense(Defense::HeavyLaser), true);
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)));
    let healed = movie.timeline.repairs[0].end_at;
    assert!(movie.damage_strength(index, healed - 0.01) > 0.0);
    assert_eq!(movie.damage_strength(index, healed), 0.0);
    assert!(movie.damage_plume(scene, index, healed - 0.01, healed + 0.4).is_some());
    assert!(movie.damage_plume(scene, index, healed + 0.01, healed + 0.4).is_none());
}
