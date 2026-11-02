use super::*;
use crate::core::patrols::PatrolPhase;
use crate::core::simulation::{GameModel, GameRules};
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Unit};
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
fn zone_effects_reuse_geometry_and_entities_and_show_only_owned_active_coverage() {
    let (model, mission) = scene();
    let mut foreign = mission.clone();
    foreign.id = 701;
    foreign.owner = 2;
    let mut returning = mission.clone();
    returning.id = 702;
    returning.return_patrol(&model.map, 2);
    let missions = Missions(vec![mission, foreign, returning]);
    let original = serde_json::to_value(&missions.0).unwrap();
    let mut app = App::new();
    app.init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<UiState>()
        .init_resource::<Time>()
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
            assert_eq!(zone.id, 700);
            entity
        })
        .collect::<BTreeSet<_>>();
    assert!(!entities.is_empty());
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
    app.world_mut().resource_mut::<Missions>().0.clear();
    app.update();
    assert_eq!(query.iter(app.world()).count(), 0);
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
