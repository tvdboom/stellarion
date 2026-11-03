use super::*;
use crate::core::patrols::PatrolPhase;
use crate::core::simulation::{GameModel, GameRules};
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Unit};
use bevy_egui::egui;
use std::{collections::BTreeSet, time::Duration};

fn scene() -> (GameModel, Mission) {
    let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let home = model.players[0].home_planet;
    let center = model.map.get(home).position + Vec2::new(150.0, 80.0);
    let mut patrol = Mission::new_patrol(
        700,
        1,
        1,
        model.map.get(home),
        center,
        Army::from([(Unit::Ship(Ship::Cruiser), 3)]),
        None,
    );
    patrol.position = center;
    patrol.patrol.as_mut().unwrap().phase = PatrolPhase::OnStation;
    (model, patrol)
}

#[test]
fn flight_arrives_smoothly_follows_its_tangent_and_leaves_the_model_untouched() {
    let (_, mut mission) = scene();
    let incoming = Vec2::new(3.0, 4.0).normalize();
    let original = serde_json::to_value(&mission).unwrap();
    let mut flight = PatrolFlight::default();
    let (start, direction) = flight.sample(&mission, incoming, 30.0);
    assert_eq!(start, mission.position);
    assert!(direction.dot(incoming) > 0.9999);
    let mut previous = start;
    for step in 1..=2_000 {
        let age = step as f64 * 0.05;
        let (position, direction) = flight.sample(&mission, incoming, 30.0 + age);
        let next = flight_pose(mission.position, incoming, age + 0.001, mission.id).0;
        assert!(position.is_finite() && direction.is_finite());
        assert!(position.distance(mission.position) < PATROL_RADIUS * 0.51);
        assert!(position.distance(previous) < 5.0, "no visual teleport between frames");
        assert!((next - position).normalize().dot(direction) > 0.99, "ship faces its flight path");
        previous = position;
    }
    assert_eq!(serde_json::to_value(&mission).unwrap(), original);
    let departing = previous;
    mission.patrol.as_mut().unwrap().phase = PatrolPhase::Returning;
    mission.objective = Icon::Deploy;
    mission.return_objective = Some(Icon::Patrol);
    assert_eq!(flight.sample(&mission, incoming, 135.0).0, departing);
    assert!(
        flight.sample(&mission, incoming, 135.5).0.distance(mission.position)
            < departing.distance(mission.position)
    );
    assert_eq!(flight.sample(&mission, incoming, 136.0).0, mission.position);
    mission.patrol.as_mut().unwrap().phase = PatrolPhase::OnStation;
    mission.objective = Icon::Patrol;
    mission.return_objective = None;
    assert_eq!(flight.sample(&mission, incoming, 140.0).0, mission.position);
    flight.reset();
    assert_eq!(flight.sample(&mission, incoming, 145.0).0, mission.position);
}

#[test]
fn animated_border_keeps_its_real_coverage_radius_and_decoration_inside_it() {
    let mut meshes = Assets::<Mesh>::default();
    let geometry = ZoneMeshes::new(&mut meshes);
    let positions = |handle: &Handle<Mesh>| {
        meshes
            .get(handle)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap()
    };
    let boundary = positions(&geometry.boundary);
    assert!(boundary.len() >= 512, "smooth circular silhouette at close zoom");
    let radius = boundary.iter().map(|v| Vec2::new(v[0], v[1]).length()).reduce(f32::max).unwrap();
    assert!((radius - PATROL_RADIUS).abs() < 0.001);
    let center = Vec2::new(20.0, -40.0);
    let color = Color::srgba(0.2, 0.5, 1.0, 0.5);
    for time in [0.0, 0.5, 10.0, 100.0] {
        for (part, handle) in [
            (ZonePart::Boundary, &geometry.boundary),
            (ZonePart::Halo, &geometry.halo),
            (ZonePart::Sweep(0), &geometry.sweep),
            (ZonePart::Head(0), &geometry.head),
        ] {
            let (pose, tint) = zone_part_pose(700, part, center, color, time);
            assert!((0.0..=1.0).contains(&tint.alpha()));
            for position in positions(handle) {
                let world = pose.transform_point(Vec3::from_array(*position)).truncate();
                assert!(world.is_finite());
                assert!(world.distance(center) <= PATROL_RADIUS + 0.001);
            }
            if matches!(part, ZonePart::Boundary) {
                assert_eq!(pose.scale, Vec3::ONE);
                assert_eq!(pose.translation.truncate(), center);
            }
        }
    }
    let first = zone_part_pose(700, ZonePart::Sweep(0), center, color, 0.0).0;
    let later = zone_part_pose(700, ZonePart::Sweep(0), center, color, 1.0).0;
    assert_ne!(first.rotation, later.rotation);
}

#[test]
fn zone_effects_reuse_geometry_and_entities_and_show_detected_stationed_enemy_coverage() {
    let (model, mission) = scene();
    let mut foreign = mission.clone();
    foreign.id = 701;
    foreign.owner = 2;
    let mut returning = mission.clone();
    returning.id = 702;
    returning.return_patrol(&model.map, 2);
    let mut foreign_outbound = foreign.clone();
    foreign_outbound.id = 703;
    foreign_outbound.patrol.as_mut().unwrap().phase = PatrolPhase::Outbound;
    let mut foreign_returning = foreign.clone();
    foreign_returning.id = 704;
    foreign_returning.return_patrol(&model.map, 2);
    let outbound = Mission::new_patrol(
        705,
        mission.send,
        mission.owner,
        model.map.get(mission.origin),
        mission.target_position(&model.map),
        mission.army.clone(),
        None,
    );
    let missions =
        Missions(vec![mission, foreign, returning, foreign_outbound, foreign_returning, outbound]);
    let original = serde_json::to_value(&missions.0).unwrap();
    let mut app = App::new();
    app.init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<bevy_egui::EguiUserTextures>()
        .init_resource::<UiState>()
        .init_resource::<Time>()
        .init_resource::<MultiplayerSession>()
        .insert_resource(model.map)
        .insert_resource(model.players[0].clone())
        .insert_resource(missions)
        .add_systems(Update, update_patrol_zones);
    app.world_mut().spawn((Camera2d, MainCamera));
    app.world_mut().spawn(Window::default());
    app.update();
    let mut query = app.world_mut().query::<(Entity, &PatrolZone)>();
    let entities = query
        .iter(app.world())
        .map(|(entity, zone)| {
            assert!([700, 701].contains(&zone.id));
            entity
        })
        .collect::<BTreeSet<_>>();
    assert!(!entities.is_empty());
    assert_eq!(
        query.iter(app.world()).map(|(_, zone)| zone.id).collect::<BTreeSet<_>>(),
        BTreeSet::from([700, 701])
    );
    let mut borders = app.world_mut().query::<(&PatrolZone, &MeshMaterial2d<ColorMaterial>)>();
    for (zone, material) in borders.iter(app.world()) {
        if matches!(zone.part, ZonePart::Boundary) {
            let owner = if zone.id == 701 {
                2
            } else {
                1
            };
            let expected = MultiplayerSession::default().player_color(owner).color().to_srgba();
            let actual = app
                .world()
                .resource::<Assets<ColorMaterial>>()
                .get(&material.0)
                .unwrap()
                .color
                .to_srgba();
            assert_eq!(
                [actual.red, actual.green, actual.blue],
                [expected.red, expected.green, expected.blue]
            );
            assert!(actual.alpha > 0.0);
        }
    }
    let meshes = app.world().resource::<Assets<Mesh>>().len();
    let materials = app.world().resource::<Assets<ColorMaterial>>().len();
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(3));
    app.update();
    assert_eq!(
        query.iter(app.world()).map(|(entity, _)| entity).collect::<BTreeSet<_>>(),
        entities
    );
    assert_eq!(app.world().resource::<Assets<Mesh>>().len(), meshes);
    assert_eq!(app.world().resource::<Assets<ColorMaterial>>().len(), materials);
    assert_eq!(serde_json::to_value(&app.world().resource::<Missions>().0).unwrap(), original);
    // A contact lost from the visible projection must also lose its coverage overlay.
    app.world_mut().resource_mut::<Missions>().0.retain(|mission| mission.id != 701);
    app.update();
    assert_eq!(
        query.iter(app.world()).map(|(_, zone)| zone.id).collect::<BTreeSet<_>>(),
        BTreeSet::from([700])
    );
    app.world_mut().resource_scope(|world, mut missions: Mut<Missions>| {
        let outbound = missions.0.iter_mut().find(|mission| mission.id == 705).unwrap();
        assert!(!outbound.is_patrolling());
        assert_ne!(outbound.position, outbound.target_position(world.resource::<Map>()));
        outbound.advance(world.resource::<Map>());
        assert!(outbound.is_patrolling());
    });
    app.update();
    assert_eq!(
        query.iter(app.world()).map(|(_, zone)| zone.id).collect::<BTreeSet<_>>(),
        BTreeSet::from([700, 705]),
        "coverage appears when the outbound fleet reaches its destination"
    );
    app.world_mut().resource_scope(|world, mut missions: Mut<Missions>| {
        missions
            .0
            .iter_mut()
            .find(|mission| mission.id == 705)
            .unwrap()
            .return_patrol(world.resource::<Map>(), 3);
    });
    app.update();
    assert_eq!(
        query.iter(app.world()).map(|(_, zone)| zone.id).collect::<BTreeSet<_>>(),
        BTreeSet::from([700]),
        "returning fleets no longer show coverage"
    );
    let draft = app.world().resource::<Missions>().0[0].clone();
    {
        let mut state = app.world_mut().resource_mut::<UiState>();
        state.mission = true;
        state.patrol_center = draft.patrol.as_ref().map(|patrol| patrol.center);
        state.mission_info = draft;
    }
    app.update();
    assert_eq!(
        query.iter(app.world()).map(|(_, zone)| zone.id).collect::<BTreeSet<_>>(),
        BTreeSet::from([700]),
        "a reinforcement draft reuses the existing coverage boundary"
    );
    app.world_mut().resource_mut::<UiState>().patrol_center =
        Some(app.world().resource::<UiState>().patrol_center.unwrap() + Vec2::splat(200.0));
    app.update();
    assert_eq!(
        query.iter(app.world()).map(|(_, zone)| zone.id).collect::<BTreeSet<_>>(),
        BTreeSet::from([0, 700]),
        "a separate space destination still has its own coverage preview"
    );
    app.world_mut().resource_mut::<UiState>().mission = false;
    app.world_mut().resource_mut::<Missions>().0.clear();
    app.update();
    assert_eq!(query.iter(app.world()).count(), 0);
}

#[test]
fn placement_preview_hides_at_invalid_locations_over_ui_and_owned_operating_patrols() {
    use bevy::camera::{CameraProjection, ComputedCameraValues, RenderTargetInfo};

    let (model, mission) = scene();
    let mut foreign = mission.clone();
    foreign.id = 701;
    foreign.owner = 2;
    let mut outbound = mission.clone();
    outbound.id = 702;
    outbound.patrol.as_mut().unwrap().phase = PatrolPhase::Outbound;
    let mut returning = mission.clone();
    returning.id = 703;
    returning.return_patrol(&model.map, 2);
    let mut app = App::new();
    app.init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<bevy_egui::EguiUserTextures>()
        .init_resource::<Time>()
        .init_resource::<MultiplayerSession>()
        .insert_resource(UiState {
            selecting_patrol_zone: true,
            ..default()
        })
        .insert_resource(model.map)
        .insert_resource(model.players[0].clone())
        .insert_resource(Missions(vec![mission, foreign, outbound, returning]))
        .add_systems(Update, update_patrol_zones);
    let mut projection = OrthographicProjection::default_2d();
    projection.update(800.0, 600.0);
    app.world_mut().spawn((
        MainCamera,
        Camera {
            computed: ComputedCameraValues {
                clip_from_view: projection.get_clip_from_view(),
                target_info: Some(RenderTargetInfo {
                    physical_size: UVec2::new(800, 600),
                    scale_factor: 1.0,
                }),
                ..default()
            },
            ..default()
        },
        GlobalTransform::default(),
    ));
    let mut window = Window::default();
    window.set_cursor_position(Some(Vec2::new(400.0, 300.0)));
    app.world_mut().spawn(window);
    let mut query = app.world_mut().query::<&PatrolZone>();
    for (hover, preview) in [
        (None, true),
        (Some(700), false),
        (None, true),
        (Some(701), true),
        (Some(702), true),
        (Some(703), true),
        (Some(700), false),
    ] {
        app.world_mut().resource_mut::<UiState>().mission_hover = hover;
        app.update();
        let ids = query.iter(app.world()).map(|zone| zone.id).collect::<BTreeSet<_>>();
        assert_eq!(ids.contains(&0), preview, "hover {hover:?}");
        assert!(ids.contains(&700), "the existing patrol circle must remain visible");
        assert!(ids.contains(&701));
    }

    app.world_mut().resource_mut::<UiState>().mission_hover = None;
    let map = app.world().resource::<Map>();
    let planet = map.planets.iter().find(|planet| !planet.is_moon()).unwrap().position;
    let moon = map.planets.iter().find(|planet| planet.is_moon()).unwrap().position;
    let outside = map.rect.max + Vec2::ONE;
    for (center, preview) in [
        (Vec2::ZERO, true),
        (planet, false),
        (Vec2::ZERO, true),
        (moon, false),
        (outside, false),
        (Vec2::ZERO, true),
    ] {
        *app.world_mut()
            .query_filtered::<&mut GlobalTransform, With<MainCamera>>()
            .single_mut(app.world_mut())
            .unwrap() = GlobalTransform::from_translation(center.extend(0.0));
        app.update();
        let ids = query.iter(app.world()).map(|zone| zone.id).collect::<BTreeSet<_>>();
        assert_eq!(ids.contains(&0), preview, "placement at {center:?}");
        assert!(ids.contains(&700) && ids.contains(&701), "existing coverage remains visible");
        assert!(app.world().resource::<UiState>().selecting_patrol_zone);
    }
    let mut egui_context = bevy_egui::EguiContext::default();
    let context = egui_context.get_mut().clone();
    app.world_mut().spawn((egui_context, bevy_egui::PrimaryEguiContext));
    for (pointer, over_ui) in [(egui::pos2(540.0, 130.0), true), (egui::pos2(400.0, 300.0), false)]
    {
        for _ in 0..2 {
            let mut output = context.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events: vec![egui::Event::PointerMoved(pointer)],
                    ..default()
                },
                |context| {
                    egui::Area::new("selection toast".into())
                        .fixed_pos(egui::pos2(500.0, 100.0))
                        .show(context, |ui| {
                            ui.allocate_exact_size(egui::vec2(200.0, 100.0), egui::Sense::hover());
                        });
                },
            );
            output.textures_delta.clear();
        }
        assert_eq!(context.is_pointer_over_egui(), over_ui);
        app.update();
        let ids = query.iter(app.world()).map(|zone| zone.id).collect::<BTreeSet<_>>();
        assert_eq!(ids.contains(&0), !over_ui);
        assert!(ids.contains(&700) && ids.contains(&701), "existing coverage remains visible");
    }
}

#[test]
fn wake_follows_the_curved_displayed_route_and_disappears_on_recall() {
    let (model, mission) = scene();
    let incoming = (mission.position - model.map.get(mission.origin).position).normalize();
    let mut flight = PatrolFlight::default();
    flight.sample(&mission, incoming, 0.0);
    let (position, direction) = flight.sample(&mission, incoming, 3.0);
    let pose = Transform::from_translation(position.extend(MISSION_Z))
        .with_rotation(Quat::from_rotation_z(direction.to_angle()));
    let original = serde_json::to_value(&mission).unwrap();
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<MultiplayerSession>()
        .insert_resource(model.map)
        .insert_resource(Missions(vec![mission]))
        .add_systems(Update, animate_patrol_wakes);
    let root = app.world_mut().spawn((MissionCmp::new(700), flight, pose)).id();
    app.world_mut().commands().entity(root).with_children(spawn_patrol_wake);
    app.world_mut().flush();
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(3));
    app.update();
    let mut wakes = app.world_mut().query::<(&PatrolWake, &Transform, &Visibility)>();
    for (wake, transform, visibility) in wakes.iter(app.world()) {
        assert_eq!(*visibility, Visibility::Inherited);
        let mission = &app.world().resource::<Missions>().0[0];
        let expected =
            flight_pose(mission.position, incoming, 3.0 - 0.8 - wake.0 as f64 * 0.14, 700).0;
        assert!(pose.transform_point(transform.translation).truncate().distance(expected) < 0.001);
    }
    assert_eq!(serde_json::to_value(&app.world().resource::<Missions>().0[0]).unwrap(), original);
    app.world_mut().resource_mut::<Missions>().0[0].patrol.as_mut().unwrap().phase =
        PatrolPhase::Returning;
    app.update();
    assert!(wakes.iter(app.world()).all(|(_, _, visibility)| *visibility == Visibility::Hidden));
}
