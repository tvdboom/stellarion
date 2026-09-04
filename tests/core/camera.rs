use super::*;

#[test]
fn parallax_depth_combines_camera_motion_zoom_and_ambient_drift() {
    let layer = ParallaxCmp::new(0.5, 0.6, 1.0, Vec2::new(2.0, -1.0));
    let (position, scale) = parallax_state(&layer, Vec2::new(120.0, -40.0), 1.25, 3.0);

    assert_eq!(position, Vec2::new(66.0, -23.0));
    assert!((scale - 0.75).abs() < f32::EPSILON);
}
