use super::*;
use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo};
use bevy::window::WindowResolution;
use bevy_egui::{EguiContext, EguiUserTextures, PrimaryEguiContext};

fn marker_scene(
    zoom: f32,
    dpi: f32,
    ui_scale: f32,
    marker: LocationMarker,
) -> (App, egui::Context, Entity) {
    let mut app = App::new();
    app.init_resource::<EguiUserTextures>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .insert_resource(UiState {
            location_marker: Some(marker),
            ..default()
        })
        .add_systems(Update, draw_location_marker);
    let mut egui_context = EguiContext::default();
    let context = egui_context.get_mut().clone();
    context.set_pixels_per_point(dpi * ui_scale);
    app.world_mut().spawn((egui_context, PrimaryEguiContext));
    app.world_mut().spawn(Window {
        resolution: WindowResolution::new((800.0 * dpi) as u32, (600.0 * dpi) as u32)
            .with_scale_factor_override(dpi),
        ..default()
    });
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = zoom;
    projection.update(800.0, 600.0);
    let camera = app
        .world_mut()
        .spawn((
            MainCamera,
            Camera {
                computed: ComputedCameraValues {
                    clip_from_view: projection.get_clip_from_view(),
                    target_info: Some(RenderTargetInfo {
                        physical_size: UVec2::new((800.0 * dpi) as u32, (600.0 * dpi) as u32),
                        scale_factor: dpi,
                    }),
                    ..default()
                },
                ..default()
            },
            GlobalTransform::from_translation(Vec3::new(80.0, -10.0, 1.0)),
        ))
        .id();
    (app, context, camera)
}

#[test]
fn location_echoes_wait_for_arrival_then_expand_fade_and_expire_without_capturing_input() {
    let context = egui::Context::default();
    let center = egui::pos2(320.0, 240.0);
    let mut marker = LocationMarker::new(Vec2::new(900.0, -700.0));
    let mut previous_ring = None;
    for (time, waiting) in
        [(0.0, true), (10.0, true), (20.0, false), (20.6, false), (20.9, false), (22.1, false)]
    {
        let mut alive = false;
        let mut output = context.run_ui(
            egui::RawInput {
                time: Some(time),
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                events: vec![egui::Event::PointerMoved(center)],
                ..default()
            },
            |ui| alive = marker.draw(ui.ctx(), center, waiting),
        );
        output.textures_delta.clear();
        let circles = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Circle(circle) => Some(circle),
                _ => None,
            })
            .collect::<Vec<_>>();
        if waiting || time > 22.0 {
            assert!(circles.is_empty());
        } else {
            assert!(circles.iter().all(|circle| circle.center == center));
            assert!(circles.iter().any(|circle| circle.radius == 2.0));
            if time > 20.5 {
                let rings =
                    circles.iter().filter(|circle| circle.stroke.width == 1.5).collect::<Vec<_>>();
                assert_eq!(rings.len(), 3);
                let outer = rings[0];
                if let Some((radius, alpha)) = previous_ring {
                    assert!(outer.radius > radius);
                    assert!(outer.stroke.color.a() < alpha);
                }
                previous_ring = Some((outer.radius, outer.stroke.color.a()));
            }
        }
        assert_eq!(alive, time < 22.0);
        assert!(!context.is_pointer_over_egui());
    }
    assert_eq!(marker.position, Vec2::new(900.0, -700.0));
}

#[test]
fn marker_tracks_world_coordinates_at_each_zoom_and_display_scale_and_cleans_itself_up() {
    for (zoom, dpi, ui_scale) in
        [(0.5, 1.0, 1.0), (1.8, 1.0, 1.0), (0.5, 2.0, 1.0), (1.8, 2.0, 1.5)]
    {
        let (mut app, context, camera) =
            marker_scene(zoom, dpi, ui_scale, LocationMarker::new(Vec2::new(100.0, 50.0)));
        for (time, camera_position) in
            [(0.0, Vec2::new(80.0, -10.0)), (0.6, Vec2::ZERO), (2.1, Vec2::ZERO)]
        {
            *app.world_mut().get_mut::<GlobalTransform>(camera).unwrap() =
                GlobalTransform::from_translation(camera_position.extend(1.0));
            let mut output = context.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0) / ui_scale,
                    )),
                    ..default()
                },
                |_| app.update(),
            );
            output.textures_delta.clear();
            if time > 2.0 {
                assert!(app.world().resource::<UiState>().location_marker.is_none());
                continue;
            }
            let expected = egui::pos2(
                (400.0 + (100.0 - camera_position.x) / zoom) / ui_scale,
                (300.0 - (50.0 - camera_position.y) / zoom) / ui_scale,
            );
            let center_dot = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Circle(circle) if circle.radius == 2.0 => Some(circle),
                    _ => None,
                })
                .unwrap();
            assert!(
                center_dot.center.distance(expected) < 0.01,
                "zoom {zoom}, dpi {dpi}, UI scale {ui_scale}"
            );
        }
    }
}

#[test]
fn fleet_rings_follow_the_displayed_icon_and_surround_its_image_at_every_display_scale() {
    let stored_position = Vec2::new(100.0, 50.0);
    for (zoom, dpi, ui_scale) in
        [(0.5, 1.0, 1.0), (1.8, 1.0, 1.0), (0.5, 2.0, 1.0), (1.8, 2.0, 1.5)]
    {
        let (mut app, context, _) =
            marker_scene(zoom, dpi, ui_scale, LocationMarker::for_mission(7, stored_position));
        // Another fleet at the patrol center must not attract the selected fleet's rings.
        app.world_mut().spawn((
            MissionCmp::new(8),
            GlobalTransform::from_translation(stored_position.extend(0.0)),
            Sprite::from_color(Color::WHITE, Vec2::splat(100.0)),
            Visibility::Inherited,
        ));
        let fleet = app
            .world_mut()
            .spawn((
                MissionCmp::new(7),
                GlobalTransform::default(),
                Sprite::default(),
                Visibility::Inherited,
            ))
            .id();
        for (time, position, waiting) in [
            (0.0, Vec2::new(170.0, -40.0), true),
            (10.0, Vec2::new(170.0, -40.0), false),
            (10.6, Vec2::new(230.0, 30.0), false),
            (12.1, Vec2::new(230.0, 30.0), false),
        ] {
            let size = if time < 10.5 {
                Vec2::new(40.0, 60.0)
            } else {
                Vec2::new(80.0, 30.0)
            };
            *app.world_mut().get_mut::<GlobalTransform>(fleet).unwrap() = GlobalTransform::from(
                Transform::from_translation(position.extend(0.0))
                    .with_rotation(Quat::from_rotation_z(0.65))
                    .with_scale(Vec3::splat(1.2)),
            );
            app.world_mut().get_mut::<Sprite>(fleet).unwrap().custom_size = Some(size);
            {
                let mut state = app.world_mut().resource_mut::<UiState>();
                state.to_selected = waiting;
                state.focus_position = Some(stored_position);
            }
            let mut output = context.run_ui(
                egui::RawInput {
                    time: Some(time),
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0) / ui_scale,
                    )),
                    ..default()
                },
                |_| app.update(),
            );
            output.textures_delta.clear();
            assert!(
                !output.shapes.iter().any(|shape| matches!(shape.shape, egui::Shape::Circle(_))),
                "fleet highlights must not draw an egui dot or rings over the image"
            );
            let mut query = app.world_mut().query_filtered::<(
                &Transform,
                &GlobalTransform,
                &Mesh2d,
                &MeshMaterial2d<ColorMaterial>,
                &Visibility,
                &Pickable,
            ), With<FleetMarkerRing>>();
            let rings = query
                .iter(app.world())
                .filter(|(_, _, _, _, visibility, _)| **visibility != Visibility::Hidden)
                .collect::<Vec<_>>();
            if waiting || time > 12.0 {
                assert!(rings.is_empty());
            } else {
                let image_radius = size.length() * 0.5 * 1.2;
                assert!(!rings.is_empty());
                for (transform, global, mesh, material, _, pickable) in rings {
                    assert!(transform.translation.truncate().distance(position) < 0.01);
                    assert_eq!(
                        global.translation(),
                        transform.translation,
                        "rendered rings must follow the image without a frame of delay"
                    );
                    assert!(
                        transform.translation.z < 0.0,
                        "rings must render below the fleet image"
                    );
                    assert!(
                        transform.scale.x >= image_radius + 6.0 * zoom * ui_scale - 0.05,
                        "zoom {zoom}, dpi {dpi}, UI scale {ui_scale}, time {time}"
                    );
                    assert_eq!(transform.scale.x, transform.scale.y);
                    let material =
                        app.world().resource::<Assets<ColorMaterial>>().get(&material.0).unwrap();
                    assert_eq!(material.alpha_mode, bevy::sprite_render::AlphaMode2d::Blend);
                    assert!(material.color.alpha() > 0.0);
                    let meshes = app.world().resource::<Assets<Mesh>>();
                    let mesh = meshes.get(&mesh.0).unwrap();
                    let bevy::mesh::VertexAttributeValues::Float32x3(vertices) =
                        mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap()
                    else {
                        panic!("ring positions must be 3D vertices");
                    };
                    assert!(
                        vertices.iter().all(|vertex| Vec3::from(*vertex).length() > 0.96),
                        "the annulus must leave its center empty"
                    );
                    assert!(!pickable.is_hoverable && !pickable.should_block_lower);
                }
                assert!(!context.is_pointer_over_egui());
            }
            let state = app.world().resource::<UiState>();
            if time > 12.0 {
                assert!(state.location_marker.is_none());
            } else {
                assert_eq!(state.location_marker.as_ref().unwrap().position, stored_position);
            }
            assert_eq!(state.focus_position, Some(stored_position));
        }
    }
}

#[test]
fn fleet_highlight_ends_when_the_selected_image_is_hidden_or_removed() {
    for despawn in [false, true] {
        let (mut app, context, _) =
            marker_scene(1.0, 1.0, 1.0, LocationMarker::for_mission(7, Vec2::ZERO));
        app.world_mut().spawn((
            MissionCmp::new(8),
            GlobalTransform::default(),
            Sprite::default(),
            Visibility::Inherited,
        ));
        let fleet = app
            .world_mut()
            .spawn((
                MissionCmp::new(7),
                GlobalTransform::from_translation(Vec3::new(170.0, 40.0, 0.0)),
                Sprite::from_color(Color::WHITE, Vec2::splat(50.0)),
                Visibility::Inherited,
            ))
            .id();
        for time in [0.0, 0.6] {
            if time > 0.0 {
                if despawn {
                    app.world_mut().despawn(fleet);
                } else {
                    *app.world_mut().get_mut::<Visibility>(fleet).unwrap() = Visibility::Hidden;
                }
            }
            let mut output = context.run_ui(
                egui::RawInput {
                    time: Some(time),
                    ..default()
                },
                |_| app.update(),
            );
            output.textures_delta.clear();
            assert_eq!(app.world().resource::<UiState>().location_marker.is_none(), time > 0.0);
            assert!(!output
                .shapes
                .iter()
                .any(|shape| matches!(shape.shape, egui::Shape::Circle(_))));
            let mut rings = app.world_mut().query_filtered::<&Visibility, With<FleetMarkerRing>>();
            assert_eq!(
                rings.iter(app.world()).any(|visibility| *visibility != Visibility::Hidden),
                time == 0.0
            );
        }
    }
}
