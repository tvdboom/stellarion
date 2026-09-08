use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;

use super::*;
use crate::core::map::icon::Icon;
use crate::core::missions::BombingRaid;
use crate::core::simulation::{GameModel, GameRules};
use crate::core::units::buildings::Building;
use crate::core::units::{Army, Unit};

fn enemy_mission(model: &GameModel, id: MissionId) -> Mission {
    let player = &model.players[0];
    let enemy = &model.players[1];
    let destination = model.map.get(player.home_planet);
    let origin = model.map.get(enemy.home_planet);
    let mut mission = Mission::new_with_id(
        id,
        1,
        enemy.id,
        origin,
        destination,
        Icon::Attack,
        Army::new(),
        BombingRaid::None,
        false,
        false,
        None,
    );
    mission.position = destination.position + Vec2::X * destination.size();
    mission
}

fn presentation_app() -> (App, Mission, Player) {
    let mut model = GameModel::new([31; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = model.players[0].clone();
    model.map.get_mut(player.home_planet).army.insert(Unit::Building(Building::SensorPhalanx), 1);
    let mission = enemy_mission(&model, 71);
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .insert_resource(model.map)
        .insert_resource(player.clone())
        .insert_resource(Missions::default())
        .insert_resource(State::new(GameState::Playing))
        .insert_resource(Settings {
            turn: 2,
            ..default()
        })
        .init_resource::<MultiplayerSession>()
        .init_resource::<DetectedMissions>()
        .init_resource::<WorldAssets>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<Time>()
        .add_message::<MessageMsg>()
        .add_message::<PublicStructureChangeMsg>()
        .add_systems(Startup, initialize_detections)
        .add_systems(
            Update,
            (
                show_detections,
                show_public_structure_changes,
                animate_detections,
                animate_public_structure_changes,
            )
                .chain(),
        );
    app.world_mut()
        .run_system_once(
            |mut assets: ResMut<WorldAssets>,
             server: Res<AssetServer>,
             mut layouts: ResMut<Assets<TextureAtlasLayout>>| {
                assets.begin_gameplay_loading(&server, &mut layouts);
            },
        )
        .unwrap();
    app.update();
    (app, mission, player)
}

fn effects(app: &mut App) -> Vec<Entity> {
    app.world_mut().query_filtered::<Entity, With<DetectionEffect>>().iter(app.world()).collect()
}

fn structure_effects(app: &mut App) -> Vec<Entity> {
    app.world_mut()
        .query_filtered::<Entity, With<PublicStructureEffect>>()
        .iter(app.world())
        .collect()
}

#[test]
fn newly_visible_enemy_mission_gets_a_radar_ping_at_its_map_position() {
    let (mut app, mission, player) = presentation_app();
    assert!(detected_by_scanner(&mission, app.world().resource::<Map>(), &player));

    app.insert_resource(Missions(vec![mission.clone()]));
    app.update();
    assert!(effects(&mut app).is_empty(), "turn-start presentation settles first");
    app.update();

    let notifications =
        app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(notifications.len(), 1);
    assert_eq!(notifications[0].message, "Enemy mission detected.");
    assert_eq!(notifications[0].level, crate::core::messages::MessageLevel::Warning);
    assert_eq!(notifications[0].action, Some(MessageAction::OpenEnemyMissions));
    assert!(!notifications[0].silent);

    let effect = effects(&mut app)[0];
    assert_eq!(
        app.world().get::<Transform>(effect).unwrap().translation.truncate(),
        mission.position
    );
    let children = app.world().get::<Children>(effect).unwrap();
    assert_eq!(
        children
            .iter()
            .filter(|child| matches!(
                app.world().get::<DetectionPart>(*child),
                Some(DetectionPart::Pulse { .. })
            ))
            .count(),
        PULSE_COUNT
    );
    assert!(children.iter().any(|child| {
        app.world().get::<Text2d>(child).is_some_and(|text| text.0 == "ENEMY MISSION DETECTED")
    }));
}

#[test]
fn detections_do_not_replay_on_refresh_or_for_the_players_own_missions() {
    let (mut app, mission, player) = presentation_app();
    app.insert_resource(Missions(vec![mission.clone()]));
    app.update();
    app.update();
    app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().for_each(drop);
    let original = effects(&mut app);
    assert_eq!(original.len(), 1);

    app.insert_resource(Missions(vec![mission.clone()]));
    app.update();
    assert_eq!(effects(&mut app), original);
    assert_eq!(app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().count(), 0);

    app.world_mut().entity_mut(original[0]).despawn();
    let mut own = mission;
    own.id = 72;
    own.owner = player.id;
    app.insert_resource(Missions(vec![own]));
    app.update();
    app.update();
    assert!(effects(&mut app).is_empty());
    assert_eq!(app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().count(), 0);
}

#[test]
fn resumed_missions_are_baselined_and_detection_effects_pause_then_expire() {
    let (mut app, mission, _) = presentation_app();
    app.insert_resource(Missions(vec![mission.clone()]));
    app.world_mut().run_system_once(initialize_detections).unwrap();
    app.update();
    app.update();
    assert!(effects(&mut app).is_empty(), "saved missions are not newly detected");

    let mut later = mission;
    later.id = 73;
    app.insert_resource(Missions(vec![later]));
    app.update();
    app.update();
    let effect = effects(&mut app)[0];
    app.insert_resource(State::new(GameState::GameMenu));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(6));
    app.update();
    assert_eq!(app.world().get::<DetectionEffect>(effect).unwrap().timer.elapsed_secs(), 0.0);
    assert_eq!(*app.world().get::<Visibility>(effect).unwrap(), Visibility::Hidden);

    app.insert_resource(State::new(GameState::Playing));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(6));
    app.update();
    assert!(effects(&mut app).is_empty());
}

#[test]
fn public_structure_pulses_use_the_owners_color_follow_the_marker_and_expire() {
    let (mut app, _, player) = presentation_app();
    let planet_id = app
        .world()
        .resource::<Map>()
        .planets
        .iter()
        .find(|planet| planet.controlled.is_some_and(|owner| owner != player.id))
        .unwrap()
        .id;
    let owner = app.world().resource::<Map>().get(planet_id).controlled.unwrap();
    app.world_mut().resource_mut::<Map>().get_mut(planet_id).army.insert(Unit::space_dock(), 1);
    app.world_mut()
        .resource_mut::<Map>()
        .get_mut(planet_id)
        .army
        .insert(Unit::Building(Building::OrbitalRailgun), 1);
    let planet_entity = app
        .world_mut()
        .spawn(PlanetCmp {
            id: planet_id,
        })
        .id();
    let dock = app
        .world_mut()
        .spawn((
            SpaceDockCmp,
            Sprite::default(),
            Transform::from_xyz(70.0, -30.0, 0.25),
            Visibility::Inherited,
            ChildOf(planet_entity),
        ))
        .id();
    let railgun = app
        .world_mut()
        .spawn((
            OrbitalRailgunCmp {
                planet: planet_id,
                anchor: Vec2::X,
                base_rotation: 0.0,
                phase: 0.0,
            },
            Sprite::default(),
            Transform::from_xyz(45.0, 20.0, 0.22),
            Visibility::Inherited,
            Pickable::default(),
            ChildOf(planet_entity),
        ))
        .id();

    for structure in [PublicStructure::SpaceDock, PublicStructure::OrbitalRailgun] {
        app.world_mut().write_message(PublicStructureChangeMsg {
            planet: planet_id,
            structure,
            change: PublicStructureChange::Built,
            owner,
        });
    }
    app.update();

    let effects = structure_effects(&mut app);
    assert_eq!(effects.len(), 2);
    for (structure, marker) in
        [(PublicStructure::SpaceDock, dock), (PublicStructure::OrbitalRailgun, railgun)]
    {
        let effect = effects
            .iter()
            .copied()
            .find(|effect| {
                app.world().get::<PublicStructureEffect>(*effect).unwrap().structure == structure
            })
            .unwrap();
        assert_eq!(app.world().get::<ChildOf>(effect).unwrap().parent(), marker);
        let children = app.world().get::<Children>(effect).unwrap();
        assert_eq!(
            children
                .iter()
                .filter(|child| app.world().get::<PublicStructurePulse>(*child).is_some())
                .count(),
            STRUCTURE_PULSE_COUNT
        );
        let pulse = children
            .iter()
            .find(|child| app.world().get::<PublicStructurePulse>(*child).is_some())
            .unwrap();
        let material = app.world().get::<MeshMaterial2d<ColorMaterial>>(pulse).unwrap();
        assert_eq!(
            app.world().resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color,
            crate::core::player::PlayerColor::for_player(owner).color().with_alpha(0.0)
        );
    }
    assert_ne!(owner, player.id, "the test must exercise another player's structures");

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(4));
    app.update();
    assert!(structure_effects(&mut app).is_empty());
}

#[test]
fn destruction_pulses_hold_a_public_railgun_marker_until_the_effect_finishes() {
    let (mut app, _, player) = presentation_app();
    let planet_id = app
        .world()
        .resource::<Map>()
        .planets
        .iter()
        .find(|planet| planet.owned.is_some_and(|owner| owner != player.id))
        .unwrap()
        .id;
    let owner = app.world().resource::<Map>().get(planet_id).owned.unwrap();
    let planet_entity = app
        .world_mut()
        .spawn(PlanetCmp {
            id: planet_id,
        })
        .id();
    let railgun = app
        .world_mut()
        .spawn((
            OrbitalRailgunCmp {
                planet: planet_id,
                anchor: Vec2::X,
                base_rotation: 0.0,
                phase: 0.0,
            },
            Sprite::default(),
            Transform::default(),
            Visibility::Hidden,
            Pickable::default(),
            ChildOf(planet_entity),
        ))
        .id();

    app.world_mut().write_message(PublicStructureChangeMsg {
        planet: planet_id,
        structure: PublicStructure::OrbitalRailgun,
        change: PublicStructureChange::Destroyed,
        owner,
    });
    app.update();

    let effect = structure_effects(&mut app)[0];
    assert_eq!(app.world().get::<PublicStructureEffect>(effect).unwrap().marker, railgun);
    assert_eq!(*app.world().get::<Visibility>(railgun).unwrap(), Visibility::Inherited);
    assert_eq!(*app.world().get::<Pickable>(railgun).unwrap(), Pickable::IGNORE);
    assert_eq!(
        app.world().get::<Sprite>(railgun).unwrap().color,
        crate::core::player::PlayerColor::for_player(owner).color()
    );

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(4));
    app.update();
    assert!(structure_effects(&mut app).is_empty());
    assert_eq!(*app.world().get::<Visibility>(railgun).unwrap(), Visibility::Hidden);
}
