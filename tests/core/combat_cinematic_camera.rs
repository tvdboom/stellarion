use super::*;

fn viewport() -> Rect {
    Rect::from_min_size(egui::pos2(37.0, 23.0), vec2(800.0, 600.0))
}

fn assert_near(actual: Pos2, expected: Pos2) {
    assert!(actual.distance(expected) < 0.001, "{actual:?} differs from {expected:?}");
}

#[test]
fn default_camera_preserves_the_original_canvas_at_any_viewport_origin() {
    let camera = CinematicCamera::default();
    for viewport in [
        viewport(),
        Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0)),
        Rect::from_min_size(egui::pos2(400.0, 300.0), vec2(450.0, 700.0)),
    ] {
        let transform = camera.transform(viewport);
        for point in [viewport.min, viewport.max, viewport.center()] {
            assert_near(transform * point, point);
        }
    }
}

#[test]
fn zoom_preserves_the_world_point_under_the_pointer_until_bounds_require_recentering() {
    let viewport = viewport();
    let mut camera = CinematicCamera::default();
    let pointer = viewport.min + viewport.size() * vec2(0.7, 0.3);
    let anchor = camera.transform(viewport).inverse() * pointer;
    camera.zoom_at(viewport, pointer, 4.0);
    assert!(camera.zoom > 1.0);
    assert_near(camera.transform(viewport) * anchor, pointer);
    camera.pan(viewport, vec2(45.0, -20.0));
    let pointer = viewport.min + viewport.size() * vec2(0.4, 0.6);
    let anchor = camera.transform(viewport).inverse() * pointer;
    camera.zoom_at(viewport, pointer, -2.0);
    assert_near(camera.transform(viewport) * anchor, pointer);
}

#[test]
fn zoom_limits_bound_extreme_wheel_input_and_center_the_widest_view() {
    let viewport = viewport();
    let mut camera = CinematicCamera::default();
    camera.zoom_at(viewport, viewport.center(), 100_000.0);
    assert_eq!(camera.zoom, MAX_ZOOM);
    camera.pan(viewport, vec2(100_000.0, -100_000.0));
    let shown = camera.transform(viewport).inverse().mul_rect(viewport);
    let allowed = viewport.expand2(viewport.size() * 0.25);
    assert!(
        allowed
            .expand2(viewport.size() * (0.20 / camera.zoom) + Vec2::splat(0.01))
            .contains_rect(shown),
        "overscroll must retain a bounded battle canvas"
    );
    for _ in 0..120 {
        camera.bound_center(viewport, false, 1.0 / 60.0);
    }
    assert!(allowed
        .expand(0.01)
        .contains_rect(camera.transform(viewport).inverse().mul_rect(viewport)));
    camera.zoom_at(viewport, viewport.max, -100_000.0);
    assert_eq!(camera.zoom, MIN_ZOOM);
    for _ in 0..120 {
        camera.bound_center(viewport, false, 1.0 / 60.0);
    }
    assert_eq!(camera.center, Vec2::splat(0.5));
    camera.pan(viewport, vec2(-100_000.0, 100_000.0));
    assert_ne!(camera.center, Vec2::splat(0.5), "The widest view can still stretch at the edges");
    for _ in 0..120 {
        camera.bound_center(viewport, false, 1.0 / 60.0);
    }
    assert_eq!(camera.center, Vec2::splat(0.5));
    assert_near(camera.transform(viewport) * viewport.center(), viewport.center());
}

#[test]
fn edges_stretch_by_twenty_percent_and_return_with_the_maps_frame_independent_easing() {
    let viewport = viewport();
    for zoom in [MIN_ZOOM, 1.0, 2.0, MAX_ZOOM] {
        let mut camera = CinematicCamera {
            zoom,
            ..Default::default()
        };
        camera.pan(viewport, vec2(-100_000.0, 100_000.0));
        let travel = (0.75 - 0.5 / zoom).max(0.0);
        let target = vec2(0.5 + travel, 0.5 - travel);
        let stretch = (camera.center - target) * viewport.size() * zoom;
        assert!((stretch - viewport.size() * vec2(0.2, -0.2)).length() < 0.001);
        let held = camera.center;
        for _ in 0..10 {
            camera.bound_center(viewport, true, 0.1);
        }
        assert_eq!(camera.center, held, "Holding the drag must retain the stretch");
        let mut faster = camera.clone();
        for _ in 0..12 {
            camera.bound_center(viewport, false, 1.0 / 60.0);
        }
        for _ in 0..24 {
            faster.bound_center(viewport, false, 1.0 / 120.0);
        }
        assert!((camera.center - faster.center).length() < 0.000_01);
        assert!((camera.center - target).length() < (held - target).length() * 0.07);
        for _ in 0..120 {
            camera.bound_center(viewport, false, 1.0 / 60.0);
        }
        assert!((camera.center - target).length() < 0.000_001);
    }
}

#[test]
fn star_depths_follow_pan_and_zoom_at_distinct_rates() {
    let viewport = viewport();
    let mut camera = CinematicCamera::default();
    let drag = vec2(80.0, -36.0);
    camera.pan(viewport, drag);
    let point = viewport.center();
    for depth in [0.34, 0.57, 0.8] {
        assert_near(camera.parallax_transform(viewport, depth) * point, point + drag * depth);
    }
    camera.zoom_at(viewport, point, 5.0);
    let far = camera.parallax_transform(viewport, 0.34);
    let middle = camera.parallax_transform(viewport, 0.57);
    let near = camera.parallax_transform(viewport, 0.8);
    assert!(1.0 < far.scaling && far.scaling < middle.scaling);
    assert!(middle.scaling < near.scaling && near.scaling < camera.zoom);
}

#[test]
fn dragging_moves_scene_by_the_pointer_distance_at_every_zoom() {
    let viewport = viewport();
    for zoom in [1.0, 2.0, MAX_ZOOM] {
        let mut camera = CinematicCamera {
            zoom,
            ..Default::default()
        };
        let point = viewport.min + viewport.size() * vec2(0.4, 0.65);
        let before = camera.transform(viewport) * point;
        let drag = vec2(48.0, -24.0);
        camera.pan(viewport, drag);
        assert_near(camera.transform(viewport) * point, before + drag);
        camera.pan(viewport, -drag);
        assert_near(camera.transform(viewport) * point, before);
    }
}

#[test]
fn resized_viewports_preserve_relative_framing_and_zoom() {
    let original = viewport();
    let resized = Rect::from_min_size(egui::pos2(120.0, 70.0), vec2(450.0, 850.0));
    let mut camera = CinematicCamera::default();
    camera.zoom_at(original, original.center(), 6.0);
    camera.pan(original, vec2(70.0, -40.0));
    let zoom = camera.zoom;
    for fraction in [vec2(0.2, 0.1), vec2(0.7, 0.4), Vec2::splat(0.5)] {
        let original_screen =
            camera.transform(original) * (original.min + original.size() * fraction);
        let resized_screen = camera.transform(resized) * (resized.min + resized.size() * fraction);
        let original_fraction = (original_screen - original.min) / original.size();
        let resized_fraction = (resized_screen - resized.min) / resized.size();
        assert!((original_fraction - resized_fraction).length() < 0.0001);
    }
    assert_eq!(camera.zoom, zoom);
    let before = camera.transform(resized) * resized.center();
    let drag = vec2(-20.0, 35.0);
    camera.pan(resized, drag);
    assert_near(camera.transform(resized) * resized.center(), before + drag);
}
