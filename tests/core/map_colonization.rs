use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;

use super::*;
use crate::core::player::PlayerColor;
use crate::core::simulation::{GameModel, GameRules, TurnCommand};

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
        .init_resource::<PendingTurnCommands>()
        .init_resource::<Settings>()
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

fn assert_same_rgb(actual: Color, expected: Color) {
    assert!(actual
        .with_alpha(1.0)
        .to_srgba()
        .to_vec4()
        .abs_diff_eq(expected.to_srgba().to_vec4(), 1e-6));
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
fn abandoned_colonies_announce_and_animate_in_the_colony_style() {
    let (mut app, planet_id, _) = presentation_app();
    let player_id = app.world().resource::<Player>().id;
    let planet_name = {
        let mut map = app.world_mut().resource_mut::<Map>();
        let planet = map.get_mut(planet_id);
        planet.colonize(player_id);
        planet.name.clone()
    };
    // Treat this colony as established before the tested local turn action.
    app.world_mut().run_system_once(initialize_colonies).unwrap();
    app.world_mut().resource_mut::<PendingTurnCommands>().push(TurnCommand::AbandonPlanet {
        planet_id,
    });
    app.world_mut().resource_mut::<Map>().get_mut(planet_id).abandon();

    app.update();
    assert!(take_toasts(&mut app).is_empty(), "the ownership transition settles first");
    app.update();

    let toasts = take_toasts(&mut app);
    assert_eq!(toasts.len(), 1);
    assert_eq!(toasts[0].message, format!("Planet {planet_name} has been abandoned."));
    assert_eq!(toasts[0].action, Some(MessageAction::FocusColony(planet_id)));
    let (effect, children) =
        app.world_mut().query::<(&ColonyEffect, &Children)>().single(app.world()).unwrap();
    assert_eq!(effect.event, ColonyEvent::Abandoned);
    assert!(children.iter().any(|child| {
        app.world().get::<Text2d>(child).is_some_and(|label| label.0 == "PLANET ABANDONED")
    }));

    // Reinstalling the same local preview must not replay its toast or effect.
    let map = app.world().resource::<Map>().clone();
    app.insert_resource(map);
    app.update();
    assert!(take_toasts(&mut app).is_empty());
    assert_eq!(app.world_mut().query::<&ColonyEffect>().iter(app.world()).count(), 1);
}

#[test]
fn ownership_loss_without_an_abandon_command_is_not_mislabeled() {
    let (mut app, planet_id, _) = presentation_app();
    let player_id = app.world().resource::<Player>().id;
    app.world_mut().resource_mut::<Map>().get_mut(planet_id).colonize(player_id);
    app.world_mut().run_system_once(initialize_colonies).unwrap();
    app.world_mut().resource_mut::<Map>().get_mut(planet_id).owned = None;

    app.update();
    app.update();

    assert!(take_toasts(&mut app).is_empty());
    assert_eq!(app.world_mut().query::<&ColonyEffect>().iter(app.world()).count(), 0);
}

#[test]
fn colony_celebration_uses_the_viewing_players_color_for_every_part() {
    let (mut app, planet, _) = presentation_app();
    let viewer_color = PlayerColor::new(4).unwrap();
    let player_id = {
        let mut player = app.world_mut().resource_mut::<Player>();
        player.color = Some(viewer_color);
        player.id
    };
    app.world_mut().resource_mut::<Map>().get_mut(planet).colonize(player_id);
    app.update();
    app.update();

    let entity =
        app.world_mut().query_filtered::<Entity, With<ColonyEffect>>().single(app.world()).unwrap();
    let expected = viewer_color.color();
    for &child in app.world().get::<Children>(entity).unwrap() {
        if let Some(text) = app.world().get::<TextColor>(child) {
            assert_same_rgb(text.0, expected);
        }
        if let Some(material) = app.world().get::<MeshMaterial2d<ColorMaterial>>(child) {
            let actual =
                app.world().resource::<Assets<ColorMaterial>>().get(&material.0).unwrap().color;
            assert_same_rgb(actual, expected);
        }
    }
}

#[test]
fn colony_result_label_uses_the_shared_above_planet_lane() {
    let (mut app, planet_id, _) = presentation_app();
    let player_id = app.world().resource::<Player>().id;
    let planet_size = app.world().resource::<Map>().get(planet_id).size();
    app.world_mut().resource_mut::<Map>().get_mut(planet_id).colonize(player_id);
    app.update();
    app.update();

    let effect =
        app.world_mut().query_filtered::<Entity, With<ColonyEffect>>().single(app.world()).unwrap();
    let label = app
        .world()
        .get::<Children>(effect)
        .unwrap()
        .iter()
        .find(|child| {
            matches!(app.world().get::<EffectPart>(*child), Some(EffectPart::Label { .. }))
        })
        .unwrap();
    let EffectPart::Label {
        y,
    } = app.world().get::<EffectPart>(label).unwrap()
    else {
        unreachable!();
    };

    assert_eq!(*y, crate::core::map::aftermath_label_y(planet_size, 0));
    assert!(*y > planet_size * 0.7, "the result belongs above the planet name");
    assert!(app.world().get::<Transform>(label).unwrap().translation.y >= *y);
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
