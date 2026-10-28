use super::*;
use bevy_egui::egui;

fn capture(mut paint: impl FnMut(&Painter)) -> Vec<Mesh> {
    let context = egui::Context::default();
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(640.0, 480.0))),
            ..Default::default()
        },
        |ui| paint(ui.painter()),
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
fn cinematic_lights_are_additive_and_feather_to_transparent_boundaries() {
    let center = pos2(200.0, 200.0);
    let meshes = capture(|painter| {
        glow(painter, center, 50.0, BLUE, 1.0);
        glow_line(painter, pos2(100.0, 100.0), pos2(300.0, 120.0), 3.0, GOLD, 0.8);
    });
    assert_eq!(meshes.len(), 2);
    for mesh in &meshes {
        assert!(mesh.is_valid());
        assert!(
            mesh.vertices.iter().all(|vertex| vertex.color.a() == 0),
            "Light may add radiance but must never dim the underlying ship artwork"
        );
        assert!(mesh.vertices.iter().any(|vertex| vertex.color.b() > 0));
    }
    let radial = &meshes[0];
    assert!(
        radial
            .vertices
            .iter()
            .filter(|vertex| vertex.pos.distance(center) > 49.0)
            .all(|vertex| vertex.color == Color32::TRANSPARENT),
        "The glow edge must be invisible instead of a hard circular disk"
    );
    assert!(capture(|painter| {
        glow(painter, center, 0.0, BLUE, 1.0);
        glow_line(painter, center, center, 2.0, BLUE, 1.0);
    })
    .is_empty());
}

#[test]
fn shield_contacts_feather_at_angular_ends_and_both_radial_edges() {
    let center = pos2(320.0, 240.0);
    let radius = 42.0;
    let meshes = capture(|painter| {
        glow_arc(painter, center, radius, -0.8, 1.6, 1.4, BLUE, 0.8);
    });
    assert_eq!(meshes.len(), 1);
    let mesh = &meshes[0];
    assert!(mesh.is_valid());
    assert!(mesh.vertices.iter().all(|vertex| vertex.color.a() == 0));
    assert!(mesh.vertices.iter().any(|vertex| vertex.color.b() > 0));
    for vertex in &mesh.vertices {
        let offset = vertex.pos - center;
        if offset.y.atan2(offset.x).abs() > 0.799 || (offset.length() - radius).abs() > 3.3 {
            assert_eq!(
                vertex.color,
                Color32::TRANSPARENT,
                "Shield flashes must not have hard crescent ends or opaque radial borders"
            );
        }
    }
}

#[test]
fn curved_projectile_wakes_remain_attached_and_bounded_when_seeking() {
    let flight = WeaponFlight {
        weapon: Weapon::Missile,
        origin: BevyVec3::new(80.0, 140.0, 0.0),
        destination: BevyVec3::new(520.0, 320.0, 0.0),
        size: 80.0,
        lane: 1.0,
    };
    let sample =
        |progress| capture(|painter| paint_projectile_wake(painter, flight, progress, 1.0));
    for progress in [0.001, 0.25, 0.7, 1.0] {
        let first = sample(progress);
        let _later = sample(1.0);
        let rewind = sample(progress);
        assert_eq!(first[0].vertices, rewind[0].vertices);
        assert!(first[0].is_valid());
        assert!(first[0].vertices.iter().all(|vertex| vertex.pos.is_finite()));
        let tip = flight.position(progress);
        assert!(
            first[0].vertices.iter().any(|vertex| {
                vertex.pos.distance(pos2(tip.x, tip.y)) < 0.001
                    && vertex.color != Color32::TRANSPARENT
            }),
            "A seek must not detach the luminous head from its projectile"
        );
        assert!(
            first[0].calc_bounds().width() <= 100.0,
            "A projectile wake must stay local instead of becoming a full-length beam"
        );
    }
}

#[test]
fn hull_impacts_scatter_in_the_incoming_direction_and_expire() {
    let center = pos2(320.0, 240.0);
    let sample = |age| {
        capture(|painter| {
            paint_impact_sparks(painter, center, vec2(-1.0, 0.0), age, 70.0, 31, 1.0);
        })
    };
    let sparks = sample(0.16);
    assert!(!sparks.is_empty());
    let reflected = sparks.iter().filter(|mesh| mesh.calc_bounds().center().x < center.x).count();
    assert!(
        reflected > sparks.len() / 2,
        "Most fragments should reflect away from the struck armor"
    );
    assert!(sample(-0.1).is_empty());
    assert!(sample(0.75).is_empty());
}

#[test]
fn wreck_fragments_outlive_the_flame_without_reintroducing_opaque_glow_disks() {
    let explosion_texture = TextureId::User(1);
    let shard_texture = TextureId::User(2);
    let images = ImageIds(
        [
            ("explosion".into(), explosion_texture),
            ("combat fx shard".into(), shard_texture),
            ("combat fx glow".into(), TextureId::User(3)),
            ("combat fx ring".into(), TextureId::User(4)),
        ]
        .into(),
    );
    let sample = |age| {
        capture(|painter| {
            explosion(
                painter,
                &images,
                pos2(320.0, 240.0),
                70.0,
                age,
                Unit::Ship(Ship::LightFighter),
            );
        })
    };
    assert!(sample(0.2).iter().all(|mesh| mesh.texture_id == explosion_texture));
    let debris = sample(1.8);
    assert!(!debris.is_empty());
    assert!(debris.iter().all(|mesh| mesh.texture_id == shard_texture));
    assert!(sample(3.6).is_empty());
}

#[test]
fn solid_energy_cores_keep_distinct_barrels_and_end_at_the_recorded_front() {
    let images = ImageIds::default();
    for weapon in [
        Weapon::Laser,
        Weapon::TwinLaser,
        Weapon::Repeater,
        Weapon::Railgun,
        Weapon::Broadside,
        Weapon::Plasma,
        Weapon::Siege,
    ] {
        let flight = WeaponFlight {
            weapon,
            origin: BevyVec3::new(80.0, 160.0, 0.0),
            destination: BevyVec3::new(580.0, 160.0, 0.0),
            size: 26.0,
            lane: 0.0,
        };
        let front = flight.sample(0.6).position.x;
        let meshes = capture(|painter| paint_weapon_body(painter, &images, flight, 0.6, 1.0));
        let cores: Vec<_> = meshes.iter().filter(|mesh| mesh.vertices.len() == 25).collect();
        assert_eq!(cores.len(), weapon.barrels(), "Every actual barrel needs its own solid core");
        for core in cores {
            assert!(core.is_valid());
            assert!(core.vertices.iter().all(|vertex| vertex.pos.is_finite()));
            assert!(
                core.calc_bounds().right() <= front + 0.001,
                "A brighter bolt must not advance beyond the actual projectile"
            );
            assert!(
                core.calc_bounds().height() >= 1.5,
                "Distant weapons need a readable continuous calibre"
            );
        }
    }
}

#[test]
fn kinetic_streaks_and_guided_exhaust_have_different_readable_silhouettes() {
    let flight = WeaponFlight {
        weapon: Weapon::Laser,
        origin: BevyVec3::new(40.0, 150.0, 0.0),
        destination: BevyVec3::new(540.0, 150.0, 0.0),
        size: 50.0,
        lane: 1.0,
    };
    let wake = |weapon| {
        capture(|painter| {
            paint_projectile_wake(
                painter,
                WeaponFlight {
                    weapon,
                    ..flight
                },
                0.7,
                1.0,
            );
        })
        .remove(0)
        .calc_bounds()
    };
    let laser = wake(Weapon::Laser);
    let rail = wake(Weapon::Railgun);
    let missile = wake(Weapon::Missile);
    assert!(rail.width() > laser.width() * 2.0, "Railguns should leave long, fast hot streaks");
    assert!(
        missile.height() > laser.height() * 1.5,
        "Guided exhaust should read as a curved expanding plume, not another laser bolt"
    );
}

#[test]
fn continuous_missile_smoke_survives_contact_then_expires_without_frame_history() {
    let flight = WeaponFlight {
        weapon: Weapon::Missile,
        origin: BevyVec3::new(80.0, 150.0, 0.0),
        destination: BevyVec3::new(540.0, 250.0, 0.0),
        size: 50.0,
        lane: 1.0,
    };
    let sample = |age| capture(|painter| paint_missile_smoke(painter, flight, age, 1.0));
    assert!(sample(-0.1).is_empty());
    assert!(sample(0.0).is_empty());
    let after_hit = sample(flight.weapon.flight() + 0.2);
    assert_eq!(
        after_hit.len(),
        1,
        "Smoke should form one continuous ribbon rather than bright dots"
    );
    assert!(after_hit[0].is_valid());
    assert!(sample(flight.weapon.flight() + 0.5).is_empty());
    assert_eq!(after_hit, sample(flight.weapon.flight() + 0.2));
}

#[test]
fn beam_afterglow_starts_at_full_contact_then_fades_monotonically() {
    let trail = WeaponTrail::Beam(
        BevyVec3::new(80.0, 160.0, 0.0),
        BevyVec3::new(480.0, 160.0, 0.0),
        2.0,
        Color::WHITE,
        0.22,
    );
    let sample = |age| {
        capture(|painter| {
            paint_trail(painter, &ImageIds::default(), trail, age, 5, 1.0);
        })
    };
    let brightness = |meshes: &[Mesh]| {
        meshes
            .iter()
            .flat_map(|mesh| &mesh.vertices)
            .map(|vertex| {
                u32::from(vertex.color.r())
                    + u32::from(vertex.color.g())
                    + u32::from(vertex.color.b())
            })
            .sum::<u32>()
    };
    assert!(
        brightness(&sample(0.0)) > brightness(&sample(0.05)),
        "A just-connected beam must not blink out before its afterglow appears"
    );
    assert!(brightness(&sample(0.05)) > brightness(&sample(0.15)));
    assert!(sample(0.22).is_empty());
}
