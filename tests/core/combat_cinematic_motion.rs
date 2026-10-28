use super::*;

#[test]
fn approach_has_no_delayed_thrust_or_heading_snap() {
    let movie = tests::replay();
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let step = 0.005;
    for index in 0..24 {
        // The short visibility entrance must meet the long approach without a sudden
        // change in thrust or helm, before the first long-range salvo is released.
        let time = 1.35;
        let before = movie.actor_pose(scene, index, time - step);
        let at = movie.actor_pose(scene, index, time);
        let after = movie.actor_pose(scene, index, time + step);
        let incoming = (at.center - before.center) / step;
        let outgoing = (after.center - at.center) / step;
        assert!((incoming - outgoing).length() < 8.0, "Arrival {index} snapped its thrust");
        assert!((before.angle - after.angle).abs() < 0.02, "Arrival {index} snapped its helm");
    }
}

#[test]
fn opening_fire_begins_in_visible_outer_lanes_during_a_slow_approach() {
    let mut movie = tests::replay();
    movie.show_planet = false;
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let opening = super::super::cinematic_timeline::FLEET_FIRE_START;
    assert!(movie.timeline.entrance_duration >= 8.0);
    for index in 0..24 {
        let first = movie.actor_course_pose(scene, index, opening);
        let later = movie.actor_course_pose(scene, index, opening + 4.0);
        assert!(scene.rect.contains(first.center), "Ship {index} must be visible before it fires");
        assert!(first.center.x < scene.rect.width() * 0.40, "Opening range collapsed too early");
        assert!(later.center.x > first.center.x + 12.0, "Ships should keep closing under fire");
        let step = movie.actor_course_pose(scene, index, opening + 0.1);
        assert!(
            step.center.distance(first.center) < 10.0,
            "Opening closure should feel deliberate"
        );
    }
}

#[test]
fn mixed_fleets_enter_without_a_visible_sprint_and_fit_at_the_first_salvo() {
    let mut movie = tests::replay();
    let classes = [
        Ship::LightFighter,
        Ship::HeavyFighter,
        Ship::Destroyer,
        Ship::Cruiser,
        Ship::Battleship,
        Ship::Dreadnought,
        Ship::WarSun,
    ];
    for index in 0..24 {
        let unit = Unit::Ship(classes[index % classes.len()]);
        movie.timeline.actors[index].unit = unit;
        movie.timeline.actors[index].side = if index < 12 {
            Side::Attacker
        } else {
            Side::Defender
        };
        movie.visuals[index].size = unit_size(unit) * 0.70;
        movie.visuals[index].aspect = sprite_aspect(unit);
        movie.visuals[index].art_heading = sprite_heading(unit);
        movie.visuals[index].firing_sheet = firing_sheet(unit);
    }
    formation::layout_fleets(&mut movie.visuals, &movie.timeline.actors);
    movie.timeline.actors[24].unit = Unit::space_dock();
    movie.visuals[24].ground = false;
    movie.visuals[24].size = unit_size(Unit::space_dock()) * 0.70;
    movie.visuals[24].home = Vec2::ZERO;
    movie.orbital_obstacles = vec![24];
    let opening = super::super::cinematic_timeline::FLEET_FIRE_START;
    for viewport in [vec2(1440.0, 900.0), vec2(640.0, 480.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, viewport));
        for index in 0..24 {
            let pose = movie.actor_pose(scene, index, opening);
            let aspect = movie.visuals[index].aspect;
            let dimensions = vec2(pose.size * aspect.min(1.0), pose.size / aspect.max(1.0));
            for corner in [vec2(-0.5, -0.5), vec2(0.5, -0.5), vec2(0.5, 0.5), vec2(-0.5, 0.5)] {
                let point = pose.center + rotate(corner * dimensions, pose.angle);
                assert!(
                    scene.rect.contains(point),
                    "Ship {index} clips at first fire in {viewport:?}: {point:?}"
                );
            }
            let opening_speed =
                movie.actor_course_pose(scene, index, opening + 0.025).center.distance(pose.center)
                    / 0.025;
            for step in 0..66 {
                let time = step as f32 * 0.025;
                let first = movie.actor_course_pose(scene, index, time);
                let next = movie.actor_course_pose(scene, index, time + 0.025);
                if scene.rect.expand(first.size * 0.5).contains(first.center) {
                    let speed = first.center.distance(next.center) / 0.025;
                    assert!(
                        speed <= 320.0 * scene.scale,
                        "Ship {index} visibly sprints at {time}: {speed}"
                    );
                    assert!(
                        speed <= (opening_speed * 5.0).max(240.0 * scene.scale),
                        "Entry speed {speed} dwarfs opening cruise {opening_speed}"
                    );
                }
            }
        }
    }
}

#[test]
fn long_replays_keep_fleets_framed_without_reversing_their_cruise() {
    let mut movie = tests::replay();
    movie.show_planet = false;
    for size in [vec2(1440.0, 820.0), vec2(640.0, 480.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, size));
        for index in 0..24 {
            for time in [35.0, 60.0, 120.0, 300.0] {
                let pose = movie.actor_pose(scene, index, time);
                let next = movie.actor_pose(scene, index, time + 0.5);
                assert!(scene.rect.contains(pose.center), "Long cruise left the battle canvas");
                assert!(next.center.x > pose.center.x, "A forward pass reversed its course");
                assert!(!pose.mirror, "A forward pass unexpectedly flipped its hull");
                assert!(pose.angle.abs() <= 0.65);
            }
        }
    }
}

#[test]
fn planet_avoidance_blends_velocity_before_entering_the_clearance_radius() {
    let mut movie = tests::replay();
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    movie.visuals[0].home = vec2(0.45, 0.50);
    let clearance = scene.planet_radius * 1.11 + movie.visuals[0].size * scene.scale * 0.56;
    movie.show_planet = false;
    let distance = |time| movie.actor_pose(scene, 0, time).center.distance(scene.planet);
    let crossing = (1..400)
        .map(|tick| tick as f32 * 0.1)
        .find(|time| distance(*time - 0.1) > clearance && distance(*time) <= clearance)
        .expect("fixture must fly through the planet's protected flight corridor");
    let mut before = crossing - 0.1;
    let mut after = crossing;
    for _ in 0..16 {
        let middle = (before + after) * 0.5;
        if distance(middle) > clearance {
            before = middle;
        } else {
            after = middle;
        }
    }
    let time = (before + after) * 0.5;
    movie.show_planet = true;
    let step = 0.005;
    let previous = movie.actor_pose(scene, 0, time - step).center;
    let position = movie.actor_pose(scene, 0, time).center;
    let next = movie.actor_pose(scene, 0, time + step).center;
    let incoming = (position - previous) / step;
    let outgoing = (next - position) / step;
    assert!(position.distance(scene.planet) > clearance);
    assert!((incoming - outgoing).length() < 2.0, "Shield clearance abruptly changed velocity");
}

fn impulse_movie(ship: Ship) -> CinematicPlayback {
    let mut movie = tests::replay();
    movie.show_planet = false;
    movie.timeline.actors[0].unit = Unit::Ship(ship);
    movie.timeline.actors[0].max_hull = 1000;
    movie.visuals[0].size = unit_size(Unit::Ship(ship));
    movie
}

fn recorded_contact(
    movie: &mut CinematicPlayback,
    impact_at: f32,
    hull: usize,
    shield: usize,
) -> usize {
    let index = movie.timeline.shots.len();
    movie.timeline.shots.push(CinematicShot {
        source: 24,
        target: Some(0),
        launch_at: impact_at - 0.5,
        impact_at,
        outcome: crate::core::combat::resolution::ShotReport {
            target_id: movie.timeline.actors[0].id,
            unit: Some(movie.timeline.actors[0].unit),
            hull_damage: hull,
            shield_damage: shield,
            ..Default::default()
        },
    });
    movie.visuals[0].incoming_shots.push(index);
    index
}

#[test]
fn penetrating_impulses_begin_at_contact_without_anticipating_future_hits() {
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let mut movie = impulse_movie(Ship::LightFighter);
    let before: Vec<_> = [5.80, 5.98, 5.999, 6.0]
        .into_iter()
        .map(|time| (time, movie.actor_pose(scene, 0, time)))
        .collect();
    recorded_contact(&mut movie, 6.0, 500, 0);
    for (time, expected) in before {
        let pose = movie.actor_pose(scene, 0, time);
        assert_eq!(pose.center, expected.center, "Hull moved before the recorded impact");
        assert_eq!(pose.angle, expected.angle, "Look-ahead steering anticipated a hit");
        assert_eq!(pose.mirror, expected.mirror);
    }
    let kicked = movie.actor_pose(scene, 0, 6.12);
    let course = movie.actor_course_pose(scene, 0, 6.12);
    assert!(kicked.center.distance(course.center) > 4.0, "A penetrating hit should be visible");
    assert_eq!(
        kicked.mirror,
        movie.actor_pose(scene, 0, 5.99).mirror,
        "A hit must not flip the hull"
    );
}

#[test]
fn capitals_take_smaller_slower_impulses_and_both_classes_recover_their_course() {
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let mut peaks = Vec::new();
    for ship in [Ship::LightFighter, Ship::WarSun] {
        let mut movie = impulse_movie(ship);
        recorded_contact(&mut movie, 6.0, 500, 0);
        let peak = (0..100)
            .map(|step| {
                let age = step as f32 * 0.025;
                (age, movie.hull_impact_response(scene, 0, 6.0 + age).displacement.length())
            })
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .unwrap();
        peaks.push(peak);
        let recovered = movie.actor_flight_pose(scene, 0, 8.1);
        assert_eq!(recovered.center, movie.actor_course_pose(scene, 0, 8.1).center);
        assert_eq!(movie.hull_impact_response(scene, 0, 8.1).bank, 0.0);
    }
    assert!(
        peaks[0].1 > peaks[1].1 * 3.0,
        "Capital mass must visibly resist the same relative hit"
    );
    assert!(peaks[1].0 > peaks[0].0, "Heavy hulls should respond more slowly");
}

#[test]
fn shields_misses_and_ground_units_do_not_receive_hull_impulses() {
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let mut movie = impulse_movie(Ship::LightFighter);
    let index = recorded_contact(&mut movie, 6.0, 0, 1000);
    assert_eq!(movie.hull_impact_response(scene, 0, 6.2).displacement, Vec2::ZERO);
    movie.timeline.shots[index].outcome.hull_damage = 1000;
    movie.timeline.shots[index].outcome.missed = true;
    assert_eq!(movie.hull_impact_response(scene, 0, 6.2).displacement, Vec2::ZERO);
    movie.timeline.shots[index].outcome.missed = false;
    for unit in [Unit::space_dock(), Unit::repair_truck(), Unit::Defense(Defense::PlasmaTurret)] {
        movie.timeline.actors[0].unit = unit;
        assert_eq!(movie.hull_impact_response(scene, 0, 6.2).displacement, Vec2::ZERO);
        assert_eq!(movie.hull_impact_response(scene, 0, 6.2).bank, 0.0);
    }
}

#[test]
fn barrages_have_bounded_seekable_impulses_and_projectiles_hit_the_reacted_hull() {
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    let mut movie = impulse_movie(Ship::LightFighter);
    for sequence in 0..80 {
        recorded_contact(&mut movie, 6.0 + sequence as f32 * 0.002, 500, 0);
    }
    let response = movie.hull_impact_response(scene, 0, 6.3);
    assert!(response.displacement.length() < 24.0);
    assert!(response.bank.abs() < 0.12);
    let pose = movie.actor_pose(scene, 0, 6.3);
    movie.elapsed = 500.0;
    let revisited = movie.actor_pose(scene, 0, 6.3);
    assert_eq!(pose.center, revisited.center);
    assert_eq!(pose.angle, revisited.angle);
    let hit = recorded_contact(&mut movie, 6.3, 1, 0);
    let (_, endpoint, _) = movie.shot_geometry(scene, hit, &movie.timeline.shots[hit]);
    assert_eq!(
        endpoint, pose.center,
        "A later shot must use the displaced target's exact contact pose"
    );
}

#[test]
fn station_clearance_preserves_height_and_relaxes_only_after_the_wreck_hold() {
    let mut movie = tests::replay();
    let station = 24;
    movie.timeline.actors[station].unit = Unit::space_dock();
    movie.visuals[station].ground = false;
    movie.visuals[station].size = unit_size(Unit::space_dock());
    movie.visuals[station].home = Vec2::ZERO;
    movie.orbital_obstacles = vec![station];
    for dimensions in [vec2(1440.0, 900.0), vec2(640.0, 480.0)] {
        let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, dimensions));
        let dock = movie.actor_cruise_pose(scene, station, 6.0);
        for size in [66.0, 196.0] {
            let size = size * scene.scale;
            let radius = dock.size * 0.49 + size * 0.56 + 10.0 * scene.scale;
            for row in -12..=12 {
                let original = dock.center + vec2(radius * 0.4, row as f32 * radius / 10.0);
                let clear = movie.station_berth(scene, 0, 6.0, original, size);
                assert_eq!(
                    clear.y, original.y,
                    "Station avoidance must not push upper ships offscreen"
                );
                assert!(
                    clear.distance(dock.center) >= radius - 0.01,
                    "A complete hull must clear the station"
                );
                assert!(clear.is_finite());
            }
        }
        movie.timeline.actors[station].death_at = Some(2.4);
        let original = dock.center;
        let held = movie.station_berth(scene, 0, 6.0, original, 66.0 * scene.scale);
        let early = movie.station_berth(scene, 0, 6.001, original, 66.0 * scene.scale);
        assert!(held.distance(early) < 0.1, "The wreck corridor must release without a jump");
        assert_eq!(movie.station_berth(scene, 0, 8.0, original, 66.0 * scene.scale), original);
        movie.timeline.actors[station].death_at = None;
    }
}
