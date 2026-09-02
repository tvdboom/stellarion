use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;

use super::*;
use crate::core::simulation::{GameModel, GameRules};

fn presentation_app() -> (App, PlanetId, PlanetId) {
    let mut model = GameModel::new(
        [8; 32],
        GameRules {
            player_count: 1,
            practice_mode: true,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let neutral = model
        .map
        .planets
        .iter()
        .filter(|p| p.owned.is_none() && !p.is_moon())
        .take(2)
        .map(|p| p.id)
        .collect::<Vec<_>>();
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .insert_resource(model.map)
        .insert_resource(model.players[0].clone())
        .insert_resource(State::new(GameState::Playing))
        .init_resource::<Colonies>()
        .init_resource::<Settings>()
        .init_resource::<MultiplayerSession>()
        .init_resource::<WorldAssets>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<Time>()
        .add_message::<MessageMsg>()
        .add_systems(Startup, initialize_colonies)
        .add_systems(Update, (celebrate_colonies, animate_colonies).chain());
    app.update();
    (app, neutral[0], neutral[1])
}

fn take_toasts(app: &mut App) -> Vec<MessageMsg> {
    app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect()
}

#[test]
fn colonies_announce_once_and_wait_for_combat_to_finish() {
    let (mut app, first, second) = presentation_app();
    assert!(take_toasts(&mut app).is_empty(), "existing home worlds are not new colonies");
    let player = app.world().resource::<Player>().id;
    app.world_mut().resource_mut::<Map>().get_mut(first).colonize(player);
    app.update();
    assert!(take_toasts(&mut app).is_empty(), "allow turn-start combat to open first");
    app.update();
    let toasts = take_toasts(&mut app);
    assert_eq!(toasts.len(), 1);
    assert_eq!(toasts[0].action, Some(MessageAction::FocusColony(first)));
    assert_eq!(app.world_mut().query::<&ColonyEffect>().iter(app.world()).count(), 1);

    // A refresh of the same snapshot or a recovered draft must not replay the celebration.
    let map = app.world().resource::<Map>().clone();
    app.insert_resource(map);
    app.update();
    assert!(take_toasts(&mut app).is_empty());
    app.world_mut().resource_mut::<Map>().get_mut(first).owned = None;
    app.update();
    app.world_mut().resource_mut::<Map>().get_mut(first).colonize(player);
    app.update();
    app.update();
    assert!(take_toasts(&mut app).is_empty());

    app.insert_resource(State::new(GameState::CombatMenu));
    app.world_mut().resource_mut::<Map>().get_mut(second).colonize(player);
    app.update();
    app.update();
    assert!(take_toasts(&mut app).is_empty());
    app.insert_resource(State::new(GameState::Playing));
    app.update();
    assert_eq!(take_toasts(&mut app)[0].action, Some(MessageAction::FocusColony(second)));
}

#[test]
fn resumed_games_and_lost_pending_colonies_do_not_celebrate() {
    let (mut app, first, second) = presentation_app();
    let player = app.world().resource::<Player>().id;
    app.world_mut().resource_mut::<Map>().get_mut(first).colonize(player);
    // Mirrors the OnEnter baseline installation for a saved game.
    app.world_mut().run_system_once(initialize_colonies).unwrap();
    app.update();
    app.update();
    assert!(take_toasts(&mut app).is_empty());

    app.insert_resource(State::new(GameState::Combat));
    app.world_mut().resource_mut::<Map>().get_mut(second).colonize(player);
    app.update();
    app.world_mut().resource_mut::<Map>().get_mut(second).owned = None;
    app.update();
    app.insert_resource(State::new(GameState::Playing));
    app.update();
    assert!(take_toasts(&mut app).is_empty());
}

#[test]
fn colony_effects_pause_and_clean_up_their_children() {
    let (mut app, first, _) = presentation_app();
    let player = app.world().resource::<Player>().id;
    app.world_mut().resource_mut::<Map>().get_mut(first).colonize(player);
    app.update();
    app.update();
    let (entity, children) = app
        .world_mut()
        .query::<(Entity, &Children)>()
        .iter(app.world())
        .next()
        .map(|(e, c)| (e, c.to_vec()))
        .unwrap();
    assert!(children.len() >= 7);
    for child in &children {
        assert_eq!(*app.world().get::<Pickable>(*child).unwrap(), Pickable::IGNORE);
    }
    app.insert_resource(State::new(GameState::GameMenu));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(6));
    app.update();
    assert_eq!(app.world().get::<ColonyEffect>(entity).unwrap().timer.elapsed_secs(), 0.0);
    assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
    app.insert_resource(State::new(GameState::Playing));
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    assert!(children.iter().all(|child| app.world().get_entity(*child).is_err()));
}

#[test]
fn territory_wave_is_clipped_to_the_colony_cell() {
    let polygon = [
        Vec2::new(-80.0, -60.0),
        Vec2::new(160.0, -60.0),
        Vec2::new(100.0, 90.0),
        Vec2::new(-80.0, 90.0),
    ];
    let boundary = sample_boundary(&polygon);
    let mut mesh = territory_mesh(&boundary);
    for radius in [0.0, 40.0, 100.0, 220.0, 1000.0] {
        advance_wave(&mut mesh, &boundary, radius);
        for point in mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap() {
            let point = Vec2::new(point[0], point[1]);
            assert!(point.is_finite());
            for (index, &a) in polygon.iter().enumerate() {
                let b = polygon[(index + 1) % polygon.len()];
                assert!((b - a).perp_dot(point - a) >= -0.01, "wave escaped its cell");
            }
        }
    }
}
