use super::*;

#[test]
fn camera_hull_excludes_empty_bounding_box_corners() {
    let hull = planet_hull([Vec2::new(0.0, 0.0), Vec2::new(100.0, 0.0), Vec2::new(0.0, 100.0)]);

    assert_eq!(clamp_to_planet_hull(Vec2::new(25.0, 25.0), &hull), Some(Vec2::new(25.0, 25.0)));
    assert_eq!(clamp_to_planet_hull(Vec2::new(100.0, 100.0), &hull), Some(Vec2::splat(50.0)));
}

#[test]
fn empty_and_small_galaxies_produce_stable_camera_hulls() {
    assert_eq!(clamp_to_planet_hull(Vec2::ZERO, &[]), None);
    assert_eq!(clamp_to_planet_hull(Vec2::ZERO, &[Vec2::X]), Some(Vec2::X));
    assert_eq!(
        clamp_to_planet_hull(Vec2::new(50.0, 100.0), &[Vec2::ZERO, Vec2::new(100.0, 0.0)]),
        Some(Vec2::new(50.0, 0.0))
    );
}

#[test]
fn map_drag_reaches_twenty_percent_of_the_full_screen_beyond_the_hull() {
    let map = Map {
        rect: Rect::from_corners(Vec2::splat(-800.0), Vec2::splat(800.0)),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![
            crate::core::map::planet::Planet::new(
                0,
                "Lower bound".to_string(),
                Vec2::new(-400.0, 0.0),
                false,
                1.0,
            ),
            crate::core::map::planet::Planet::new(
                1,
                "Upper bound".to_string(),
                Vec2::new(400.0, 0.0),
                false,
                1.0,
            ),
        ],
    };

    let near = drag_camera_position(Vec2::ZERO, Vec2::new(450.0, 0.0), Vec2::splat(1_000.0), &map);
    let far = drag_camera_position(
        Vec2::ZERO,
        Vec2::new(40_000.0, -40_000.0),
        Vec2::splat(1_000.0),
        &map,
    );

    assert_eq!(near, Vec2::new(450.0, 0.0));
    assert_eq!(far, Vec2::new(600.0, -200.0));
}

#[test]
fn released_camera_bounces_back_to_the_midpoint_boundary() {
    let outside = Vec2::new(560.0, -540.0);
    let target = Vec2::new(400.0, -400.0);
    let settled = settle_position(outside, target, 1.0 / 60.0);

    assert!(settled.x < outside.x && settled.x > target.x);
    assert!(settled.y > outside.y && settled.y < target.y);
    assert_eq!(settle_position(target + Vec2::splat(0.05), target, 1.0), target);
}

#[test]
fn parallax_depth_combines_camera_motion_zoom_and_ambient_drift() {
    let layer = ParallaxCmp::new(0.5, 0.6, 1.0, Vec2::new(2.0, -1.0));
    let (position, scale) = parallax_state(&layer, Vec2::new(120.0, -40.0), 1.25, 3.0);

    assert_eq!(position, Vec2::new(66.0, -23.0));
    assert!((scale - 0.75).abs() < f32::EPSILON);
}

#[test]
fn camera_focus_zoom_reaches_the_widest_allowed_scale() {
    let mut scale = MIN_ZOOM;
    let mut complete = false;
    for _ in 0..256 {
        (scale, complete) = advance_focus_zoom(scale, MAX_ZOOM);
        if complete {
            break;
        }
    }

    assert!(complete);
    assert_eq!(scale, MAX_ZOOM);
}
