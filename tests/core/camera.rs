use super::*;

#[test]
fn camera_bounds_keep_outermost_worlds_twenty_percent_inside_the_viewport() {
    let worlds = [Vec2::new(-700.0, 400.0), Vec2::new(700.0, -400.0)];
    let view_size = Vec2::new(1_000.0, 600.0);
    let bounds = camera_center_bounds(worlds, view_size).expect("worlds should produce bounds");

    assert_eq!(bounds.min, Vec2::new(-400.0, -220.0));
    assert_eq!(bounds.max, Vec2::new(400.0, 220.0));
    assert_eq!(700.0 - bounds.max.x, view_size.x * 0.3);
    assert_eq!(400.0 - bounds.max.y, view_size.y * 0.3);
}

#[test]
fn empty_and_small_galaxies_produce_stable_camera_bounds() {
    assert_eq!(bounds_from_points([]), None);
    assert_eq!(camera_center_bounds([], Vec2::splat(500.0)), None);

    let bounds = camera_center_bounds(
        [Vec2::new(-100.0, -50.0), Vec2::new(100.0, 50.0)],
        Vec2::new(1_000.0, 600.0),
    )
    .expect("worlds should produce bounds");
    assert_eq!(bounds.min, Vec2::ZERO);
    assert_eq!(bounds.max, Vec2::ZERO);
}

#[test]
fn map_drag_has_resistance_and_a_finite_overscroll_limit() {
    let bounds = Rect::from_corners(Vec2::splat(-400.0), Vec2::splat(400.0));
    let view_size = Vec2::splat(1_000.0);

    let near = rubber_band_position(Vec2::new(450.0, 0.0), bounds, view_size);
    let far = rubber_band_position(Vec2::new(40_000.0, 0.0), bounds, view_size);
    assert!(near.x > bounds.max.x);
    assert!(near.x < 450.0);
    assert_eq!(view_size.x * OVERSCROLL_SCREEN_FRACTION, 150.0);
    assert!(far.x < bounds.max.x + view_size.x * OVERSCROLL_SCREEN_FRACTION);
}

#[test]
fn released_camera_eases_back_to_its_hard_boundary() {
    let bounds = Rect::from_corners(Vec2::splat(-400.0), Vec2::splat(400.0));
    let outside = Vec2::new(480.0, -450.0);
    let settled = settle_position(outside, bounds, 1.0 / 60.0);

    assert!(settled.x < outside.x && settled.x > bounds.max.x);
    assert!(settled.y > outside.y && settled.y < bounds.min.y);
    assert_eq!(settle_position(bounds.max + Vec2::splat(0.05), bounds, 1.0), bounds.max);
}

#[test]
fn parallax_depth_combines_camera_motion_zoom_and_ambient_drift() {
    let layer = ParallaxCmp::new(0.5, 0.6, 1.0, Vec2::new(2.0, -1.0));
    let (position, scale) = parallax_state(&layer, Vec2::new(120.0, -40.0), 1.25, 3.0);

    assert_eq!(position, Vec2::new(66.0, -23.0));
    assert!((scale - 0.75).abs() < f32::EPSILON);
}
