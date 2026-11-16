use super::*;
use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo};
use bevy::window::WindowResolution;
use bevy_egui::{EguiContext, EguiUserTextures, PrimaryEguiContext};

#[test]
fn camera_outline_follows_world_pan_zoom_and_viewport_aspect() {
    let minimap = Minimap::fit(
        Rect::new(-1600.0, -800.0, 1600.0, 800.0),
        egui::Rect::from_min_size(Pos2::ZERO, egui::Vec2::splat(160.0)),
    )
    .unwrap();
    assert_eq!(minimap.project(Vec2::new(-1600.0, 800.0)), Pos2::new(0.0, 40.0));
    assert_eq!(minimap.project(Vec2::new(1600.0, -800.0)), Pos2::new(160.0, 120.0));

    for (zoom, size, pan, expected_center, expected_size) in [
        (1.0, UVec2::new(800, 600), Vec2::ZERO, Pos2::new(80.0, 80.0), egui::vec2(40.0, 30.0)),
        (
            0.5,
            UVec2::new(600, 800),
            Vec2::new(200.0, 100.0),
            Pos2::new(90.0, 75.0),
            egui::vec2(15.0, 20.0),
        ),
    ] {
        let camera = test_camera(size, zoom, 2);
        let transform = GlobalTransform::from_translation(pan.extend(1.0));
        let outline =
            minimap.viewport_outline(camera_bounds(&camera, &transform).unwrap()).unwrap();
        assert!((outline.center() - expected_center).length() < 0.001);
        assert!((outline.size() - expected_size).length() < 0.001);
    }
}

fn test_camera(size: UVec2, zoom: f32, display_scale: u32) -> Camera {
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = zoom;
    projection.update(size.x as f32, size.y as f32);
    Camera {
        computed: ComputedCameraValues {
            clip_from_view: projection.get_clip_from_view(),
            target_info: Some(RenderTargetInfo {
                physical_size: size * display_scale,
                scale_factor: display_scale as f32,
            }),
            ..default()
        },
        ..default()
    }
}

#[test]
fn camera_outline_is_clipped_and_remains_visible_when_zoomed_out_past_the_map() {
    let chart = egui::Rect::from_min_size(Pos2::ZERO, egui::Vec2::splat(100.0));
    let minimap = Minimap::fit(Rect::new(-100.0, -100.0, 100.0, 100.0), chart).unwrap();
    assert_eq!(
        minimap.viewport_outline(Rect::new(0.0, 0.0, 150.0, 150.0)),
        Some(egui::Rect::from_min_max(Pos2::new(50.0, 0.0), Pos2::new(100.0, 50.0)))
    );
    assert_eq!(minimap.viewport_outline(Rect::new(-1000.0, -800.0, 1000.0, 800.0)), Some(chart));
    assert_eq!(minimap.viewport_outline(Rect::new(150.0, 150.0, 250.0, 250.0)), None);
}

#[test]
fn minimap_fits_in_the_lower_right_above_end_turn_at_different_viewport_sizes() {
    for (width, height) in [(1600.0, 900.0), (800.0, 500.0), (480.0, 360.0)] {
        let viewport = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(width, height));
        let turn_scale = viewport_ui_scale(viewport.size());
        let panel = panel_rect(viewport, turn_scale).unwrap();
        assert!(viewport.contains_rect(panel));
        assert_eq!(panel.right(), width - MAIN_BUTTON_RIGHT * turn_scale);
        assert!(panel.bottom() < height - (MAIN_BUTTON_BOTTOM + MAIN_BUTTON_HEIGHT) * turn_scale);
    }
}

#[test]
fn overlapping_hover_panel_paints_over_minimap_without_moving_it() {
    let mut app = App::new();
    let mut egui_context = EguiContext::default();
    let context = egui_context.get_mut().clone();
    let minimap_id = egui::Id::new(PANEL_ID);
    let viewport = egui::Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 500.0));
    let panel = panel_rect(viewport, viewport_ui_scale(viewport.size())).unwrap();
    app.init_resource::<EguiUserTextures>()
        .init_resource::<MultiplayerSession>()
        .init_resource::<UiState>()
        .insert_resource(Map::new(4, 0))
        .insert_resource(Player::new(1, 0))
        .insert_resource(Missions(Vec::new()))
        .add_systems(Update, draw_minimap);
    app.world_mut().spawn((egui_context, PrimaryEguiContext));
    app.world_mut().spawn(Window {
        resolution: WindowResolution::new(800, 500).with_scale_factor_override(1.0),
        ..default()
    });
    app.world_mut().spawn((
        test_camera(UVec2::new(800, 500), 1.0, 1),
        GlobalTransform::IDENTITY,
        MainCamera,
    ));

    let mut chart_before_hover = None;
    for (frame, hovered) in [false, false, true, true, false].into_iter().enumerate() {
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                ..default()
            },
            |_| {
                app.update();
                if hovered {
                    egui::Area::new(egui::Id::new("planet details"))
                        .fixed_pos(panel.min - egui::Vec2::splat(16.0))
                        .fade_in(false)
                        .show(&context, |ui| {
                            let (rect, _) =
                                ui.allocate_exact_size(panel.size(), egui::Sense::hover());
                            ui.painter().rect_filled(rect, 0.0, Color32::RED);
                        });
                }
            },
        );
        output.textures_delta.clear();
        if frame == 0 {
            continue;
        }
        let (chart_index, chart) = output
            .shapes
            .iter()
            .enumerate()
            .find_map(|(index, shape)| match &shape.shape {
                egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(7, 13, 22) => {
                    Some((index, rect.rect))
                },
                _ => None,
            })
            .unwrap();
        assert_eq!(chart, *chart_before_hover.get_or_insert(chart));
        let frame_rect = context.memory(|memory| memory.area_rect(minimap_id)).unwrap();
        let inset = chart.left() - frame_rect.left();
        assert!((1.0..=5.0).contains(&inset), "unexpected minimap padding: {inset}");
        if frame == 3 {
            let hover_panel = output
                .shapes
                .iter()
                .position(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == Color32::RED
                    && rect.rect.intersects(chart))
                })
                .unwrap();
            assert!(chart_index < hover_panel);
        }
    }
}

#[test]
fn minimap_clicks_and_drags_obey_map_bounds_and_keep_the_viewport_grab_offset() {
    use crate::core::camera::{clamp_camera_to_worlds, move_camera};
    use bevy::ecs::system::RunSystemOnce;
    use bevy::input::mouse::MouseWheel;
    use bevy::picking::pointer::Location;
    use bevy::window::WindowRef;
    use std::time::Duration;

    let mut app = App::new();
    let mut egui_context = EguiContext::default();
    let context = egui_context.get_mut().clone();
    let size = UVec2::new(800, 500);
    let viewport = egui::Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 500.0));
    let world = Rect::new(-2000.0, -2000.0, 2000.0, 2000.0);
    let mut map = Map::new(4, 0);
    map.rect = world;
    let edge = Vec2::new(1500.0, 1200.0);
    for (planet, signs) in map.planets.iter_mut().zip([
        Vec2::ONE,
        Vec2::NEG_ONE,
        Vec2::new(1.0, -1.0),
        Vec2::new(-1.0, 1.0),
    ]) {
        planet.position = edge * signs;
    }
    let zoom = 0.8;
    let mut time = Time::<()>::default();
    time.advance_by(Duration::from_secs_f32(1.0 / 60.0));
    let mut projection = OrthographicProjection::default_2d();
    projection.scale = zoom;
    projection.update(800.0, 500.0);
    app.add_plugins(TransformPlugin)
        .init_resource::<EguiUserTextures>()
        .init_resource::<MultiplayerSession>()
        .init_resource::<ButtonInput<MouseButton>>()
        .insert_resource(time)
        .add_message::<MouseWheel>()
        .add_message::<PointerHits>()
        .insert_resource(EguiPickingOrder(0.6))
        .insert_resource(map)
        .insert_resource(Player::new(1, 0))
        .insert_resource(Missions(Vec::new()))
        .insert_resource(UiState {
            to_selected: true,
            planet_selected: Some(0),
            focus_zoom: Some(1.5),
            ..default()
        })
        .add_systems(Update, (move_camera, clamp_camera_to_worlds).chain())
        .add_systems(bevy_egui::EguiPrimaryContextPass, draw_minimap);
    app.world_mut().spawn((egui_context, PrimaryEguiContext));
    let window = app
        .world_mut()
        .spawn(Window {
            resolution: WindowResolution::new(size.x, size.y).with_scale_factor_override(1.0),
            ..default()
        })
        .id();
    let camera = app
        .world_mut()
        .spawn((
            test_camera(size, zoom, 1),
            Transform::from_xyz(0.0, 0.0, 1.0),
            Projection::Orthographic(projection),
            MainCamera,
        ))
        .id();

    let frame = |app: &mut App, events: Vec<egui::Event>| {
        for event in &events {
            if let egui::Event::PointerButton {
                button: PointerButton::Primary,
                pressed,
                ..
            } = event
            {
                let mut buttons = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
                if *pressed {
                    buttons.press(MouseButton::Left);
                } else {
                    buttons.release(MouseButton::Left);
                }
            }
        }
        app.update();
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(viewport),
                events,
                ..default()
            },
            |_| {
                app.world_mut().run_schedule(bevy_egui::EguiPrimaryContextPass);
            },
        );
        output.textures_delta.clear();
        output.shapes
    };
    let button = |pos, pressed| {
        vec![
            egui::Event::PointerMoved(pos),
            egui::Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            },
        ]
    };
    let assert_camera = |app: &App, expected: Vec2| {
        let transform = app.world().get::<Transform>(camera).unwrap();
        assert!(
            transform.translation.truncate().distance(expected) < 0.01,
            "actual camera: {transform:?}, expected: {expected:?}"
        );
        assert_eq!(transform.translation.z, 1.0);
        let Projection::Orthographic(projection) = app.world().get::<Projection>(camera).unwrap()
        else {
            panic!("expected orthographic camera");
        };
        assert_eq!(projection.scale, zoom);
    };

    frame(&mut app, vec![]);
    let chart = frame(&mut app, vec![])
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.fill == Color32::from_rgb(7, 13, 22) => Some(rect.rect),
            _ => None,
        })
        .unwrap();
    let minimap = Minimap::fit(world, chart).unwrap();

    let click = Vec2::new(1200.0, 1000.0);
    let pos = minimap.project(click);
    let pointer = app
        .world_mut()
        .spawn((
            PointerId::Mouse,
            PointerLocation::new(Location {
                target: NormalizedRenderTarget::Window(
                    WindowRef::Entity(window).normalize(None).unwrap(),
                ),
                position: Vec2::new(pos.x, pos.y),
            }),
        ))
        .id();
    app.world_mut().run_system_once(block_minimap_world_picking).unwrap();
    let hits = app.world_mut().resource_mut::<Messages<PointerHits>>().drain().collect::<Vec<_>>();
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].picks[0].0, camera);
    assert!(hits[0].order > 0.0);
    app.world_mut().despawn(pointer);

    frame(&mut app, button(minimap.project(click), true));
    frame(&mut app, button(minimap.project(click), false));
    let shapes = frame(&mut app, vec![]);
    assert_camera(&app, click);
    let rendered_outline = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.stroke.color == Color32::WHITE => Some(rect.rect),
            _ => None,
        })
        .unwrap();
    assert!(rendered_outline.center().distance(minimap.project(click)) < 0.01);
    assert_eq!(app.world().resource::<UiState>().planet_selected, Some(0));
    assert!(!app.world().resource::<UiState>().to_selected);

    let grab = click + Vec2::new(-140.0, 90.0);
    frame(&mut app, button(minimap.project(grab), true));
    let overscroll = size.as_vec2() * zoom * 0.2;
    for delta in [Vec2::new(-250.0, -270.0), Vec2::new(-460.0, -380.0), Vec2::new(1600.0, -700.0)] {
        frame(&mut app, vec![egui::Event::PointerMoved(minimap.project(grab + delta))]);
        frame(&mut app, vec![]);
        assert_camera(&app, (click + delta).clamp(-edge - overscroll, edge + overscroll));
    }
    let delta = Vec2::new(200.0, -800.0);
    frame(&mut app, button(minimap.project(grab + delta), false));
    frame(&mut app, vec![]);
    assert_camera(&app, click + delta);

    let unreachable = Vec2::new(1900.0, -1800.0);
    frame(&mut app, button(minimap.project(unreachable), true));
    frame(&mut app, button(minimap.project(unreachable), false));
    app.world_mut().resource_mut::<UiState>().focus_position = Some(unreachable);
    frame(&mut app, vec![]);
    let initial = app.world().get::<Transform>(camera).unwrap().translation.truncate();
    let corner = Vec2::new(edge.x, -edge.y);
    let offset = initial - corner;
    assert!(offset.x > 0.0 && offset.x < overscroll.x);
    assert!(offset.y < 0.0 && -offset.y < overscroll.y);
    assert_eq!(app.world().resource::<UiState>().focus_position, None);
    for _ in 0..90 {
        app.update();
    }
    assert_camera(&app, corner);
}
