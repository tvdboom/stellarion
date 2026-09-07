use std::time::Duration;

use super::*;

fn recall_animation_app() -> (App, Entity) {
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .add_message::<MissionRecallAnimationMsg>()
        .add_systems(Update, animate_mission_recalls);
    let mission = app
        .world_mut()
        .spawn((
            Sprite::default(),
            Transform {
                rotation: Quat::from_rotation_z(PI),
                ..default()
            },
            Visibility::Inherited,
            MissionCmp::new(7),
        ))
        .id();
    app.world_mut().write_message(MissionRecallAnimationMsg {
        mission_id: 7,
        position: Vec2::new(20.0, -10.0),
        color: Color::srgb(0.2, 0.7, 1.0),
        radius: 62.5,
        from_rotation: Quat::IDENTITY,
        from_flip_x: false,
        from_flip_y: false,
        to_rotation: Quat::from_rotation_z(PI),
        to_flip_x: false,
        to_flip_y: true,
    });
    (app, mission)
}

#[test]
fn immediate_recall_faces_back_along_the_route_even_at_home() {
    use crate::core::map::icon::Icon;
    use crate::core::map::planet::Planet;
    use crate::core::missions::BombingRaid;
    use crate::core::units::Unit;

    let origin = Planet::new(0, "Origin".into(), Vec2::ZERO, false, 1.0);
    let destination = Planet::new(1, "Destination".into(), Vec2::X * 500.0, false, 1.0);
    let map = Map {
        rect: Rect::default(),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin.clone(), destination.clone()],
    };
    let mut mission = Mission::new_with_id(
        7,
        1,
        1,
        &origin,
        &destination,
        Icon::Attack,
        Army::from([(Unit::probe(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let outbound = mission_map_direction(&mission, &map);
    mission.recall(&map, 1);
    let returning = mission_map_direction(&mission, &map);

    assert!(outbound.dot(returning) < -0.999);
}

#[test]
fn recall_rotates_the_visible_ship_into_its_return_facing() {
    let (mut app, mission) = recall_animation_app();
    app.update();

    let transform = app.world().get::<Transform>(mission).unwrap();
    assert_eq!(transform.rotation, Quat::IDENTITY);
    assert!(!app.world().get::<Sprite>(mission).unwrap().flip_y);
    assert_eq!(
        app.world_mut().query::<&MissionRecallPulse>().iter(app.world()).count(),
        RECALL_PULSE_COUNT
    );

    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(RECALL_TURN_SECONDS * 0.5));
    app.update();
    let halfway_forward = app.world().get::<Transform>(mission).unwrap().rotation * Vec3::X;
    assert!(halfway_forward.x.abs() < 0.01);
    assert!(halfway_forward.y.abs() > 0.99);
    assert_eq!(*app.world().get::<Visibility>(mission).unwrap(), Visibility::Inherited);
    assert_eq!(app.world().get::<Transform>(mission).unwrap().scale, Vec3::ONE);

    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(RECALL_TURN_SECONDS * 0.5 + 0.01));
    app.update();
    assert_eq!(*app.world().get::<Visibility>(mission).unwrap(), Visibility::Inherited);
    let return_forward = app.world().get::<Transform>(mission).unwrap().rotation * Vec3::X;
    assert!(return_forward.distance(-Vec3::X) < 0.0001);
    assert!(app.world().get::<Sprite>(mission).unwrap().flip_y);
}

#[test]
fn recall_pulses_expand_and_clean_up_with_the_animation() {
    let (mut app, mission) = recall_animation_app();
    app.update();
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.28));
    app.update();

    let pulse_visuals = {
        let world = app.world_mut();
        world
            .query::<(&Transform, &MeshMaterial2d<ColorMaterial>)>()
            .iter(world)
            .map(|(transform, material)| (transform.scale.x, material.0.clone()))
            .collect::<Vec<_>>()
    };
    let materials = app.world().resource::<Assets<ColorMaterial>>();
    assert!(pulse_visuals.iter().any(|(scale, material)| {
        *scale > 62.5 * 0.08
            && materials.get(material).is_some_and(|material| material.color.alpha() > 0.0)
    }));

    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(RECALL_ANIMATION_SECONDS));
    app.update();
    app.update();
    assert!(app.world().get::<MissionRecallAnimation>(mission).is_none());
    assert_eq!(app.world_mut().query::<&MissionRecallEffect>().iter(app.world()).count(), 0);
    assert_eq!(app.world().get::<Transform>(mission).unwrap().scale, Vec3::ONE);
}
