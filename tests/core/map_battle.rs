use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;

use super::*;
use crate::core::combat::report::CombatReport;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::simulation::{GameModel, GameRules};
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Unit};

fn battle_report(id: ReportId, planet: &Planet, outcome: Outcome) -> MissionReport {
    let mut defender = planet.clone();
    defender.owned = Some(2);
    defender.controlled = Some(2);
    MissionReport {
        id,
        turn: 2,
        mission: Mission::new_with_id(
            id,
            1,
            1,
            planet,
            &defender,
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::LightFighter), 1)]),
            BombingRaid::None,
            false,
            false,
            None,
        ),
        planet: defender,
        scout_probes: 0,
        surviving_attacker: if outcome != Outcome::Defeat {
            Army::from([(Unit::Ship(Ship::LightFighter), 1)])
        } else {
            Army::new()
        },
        surviving_defender: if outcome != Outcome::Victory {
            Army::from([(Unit::Ship(Ship::LightFighter), 1)])
        } else {
            Army::new()
        },
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: Some(2),
        destination_controlled: Some(2),
        combat_report: Some(CombatReport::default()),
        hidden: false,
    }
}

fn presentation_app() -> (App, Planet) {
    let mut model = GameModel::new(
        [8; 32],
        GameRules {
            player_count: 2,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let planet = model.map.get(model.players[1].home_planet).clone();
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .insert_resource(model.map)
        .insert_resource(model.players[0].clone())
        .insert_resource(State::new(GameState::Playing))
        .insert_resource(Settings {
            turn: 2,
            ..default()
        })
        .init_resource::<BattleSites>()
        .init_resource::<WorldAssets>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<Time>()
        .add_message::<PlayAudioMsg>()
        .add_systems(Startup, initialize_battles)
        .add_systems(Update, (show_battles, animate_battles).chain());
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
    (app, planet)
}

fn effects(app: &mut App) -> Vec<Entity> {
    app.world_mut().query_filtered::<Entity, With<BattleEffect>>().iter(app.world()).collect()
}

#[test]
fn battle_outcomes_follow_the_local_side_while_territory_headlines_take_priority() {
    let (app, planet) = presentation_app();
    let attacker = app.world().resource::<Player>();
    let defender = Player::new(2, planet.id);
    for (result, defense_result) in [
        (Outcome::Victory, Outcome::Defeat),
        (Outcome::Defeat, Outcome::Victory),
        (Outcome::Draw, Outcome::Draw),
    ] {
        let report = battle_report(1, &planet, result);
        assert_eq!(Outcome::from_report(&report, attacker), Some(result));
        assert_eq!(Outcome::from_report(&report, &defender), Some(defense_result));
        assert_eq!(Outcome::from_report(&report, &Player::new(3, 0)), None);
    }

    let mut captured = battle_report(2, &planet, Outcome::Victory);
    captured.destination_owned = Some(attacker.id);
    captured.destination_controlled = Some(attacker.id);
    assert_eq!(Outcome::from_report(&captured, attacker), None);
    assert_eq!(Outcome::from_report(&captured, &defender), None);
    assert_eq!(
        TerritoryOutcome::from_report(&captured, attacker),
        Some(TerritoryOutcome::Conquered)
    );
    assert_eq!(TerritoryOutcome::from_report(&captured, &defender), Some(TerritoryOutcome::Lost));

    for (player, expected) in [(attacker, "PLANET CONQUERED"), (&defender, "PLANET LOST")] {
        let mut sites = BattleSites::default();
        let mut player = player.clone();
        player.reports.push(captured.clone());
        assert!(sites.observe(&player, 2));
        let labels = sites.outcomes[&planet.id].labels();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0].0, expected);
    }
}

#[test]
fn only_current_visible_battles_and_missile_impacts_produce_aftermath() {
    let (app, planet) = presentation_app();
    let mut player = app.world().resource::<Player>().clone();
    let report = battle_report(1, &planet, Outcome::Victory);
    let mut peaceful = report.clone();
    peaceful.id = 2;
    peaceful.combat_report = None;
    let mut hidden = report.clone();
    hidden.id = 3;
    hidden.hidden = true;
    let mut old = report.clone();
    old.id = 4;
    old.turn = 1;
    let mut spy = report.clone();
    spy.id = 5;
    spy.mission.objective = Icon::Spy;
    let mut missiles = report.clone();
    missiles.id = 6;
    missiles.mission.objective = Icon::MissileStrike;
    player.reports = vec![peaceful, hidden, old, spy, missiles];
    let mut sites = BattleSites::default();
    assert!(sites.observe(&player, 2));
    assert_eq!(sites.pending, BTreeSet::from([planet.id]));
    sites.pending.clear();
    player.reports.push(report);
    assert!(sites.observe(&player, 2));
    assert_eq!(sites.pending, BTreeSet::from([planet.id]));
}

#[test]
fn battle_aftermath_waits_for_the_map_and_does_not_replay_on_refresh_or_resume() {
    let (mut app, planet) = presentation_app();
    app.world_mut().resource_mut::<Player>().reports.push(battle_report(
        1,
        &planet,
        Outcome::Victory,
    ));
    app.update();
    assert!(effects(&mut app).is_empty(), "allow turn-start combat to open first");
    app.insert_resource(State::new(GameState::CombatMenu));
    app.update();
    app.insert_resource(State::new(GameState::Combat));
    app.update();
    assert!(effects(&mut app).is_empty());
    app.insert_resource(State::new(GameState::Playing));
    app.update();
    let original = effects(&mut app);
    assert_eq!(original.len(), 1);
    let player = app.world().resource::<Player>().clone();
    app.insert_resource(player);
    app.update();
    assert_eq!(effects(&mut app), original, "same-turn projections must not restart effects");

    // Re-entering a saved game establishes the reports as already observed.
    app.world_mut().entity_mut(original[0]).despawn();
    app.world_mut().run_system_once(initialize_battles).unwrap();
    app.update();
    app.update();
    assert!(effects(&mut app).is_empty());
}

#[test]
fn multiple_battles_share_one_honest_result_marker() {
    let (mut app, planet) = presentation_app();
    app.world_mut().resource_mut::<Player>().reports.extend([
        battle_report(1, &planet, Outcome::Victory),
        battle_report(2, &planet, Outcome::Defeat),
    ]);
    app.update();
    app.update();
    assert_eq!(effects(&mut app).len(), 1);
    let labels = app
        .world_mut()
        .query::<&Text2d>()
        .iter(app.world())
        .map(|t| t.0.as_str())
        .collect::<Vec<_>>();
    assert_eq!(labels, vec!["MIXED BATTLE RESULTS"]);
    assert_eq!(app.world().resource::<Messages<PlayAudioMsg>>().len(), 1);
}

#[test]
fn aftermath_pauses_advances_at_low_frame_rates_and_expires() {
    let (mut app, planet) = presentation_app();
    app.world_mut().resource_mut::<Player>().reports.push(battle_report(
        1,
        &planet,
        Outcome::Defeat,
    ));
    app.update();
    app.update();
    let entity = effects(&mut app)[0];
    let children = app.world().get::<Children>(entity).unwrap().to_vec();
    for child in &children {
        assert_eq!(*app.world().get::<Pickable>(*child).unwrap(), Pickable::IGNORE);
    }
    app.insert_resource(State::new(GameState::GameMenu));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(6));
    app.update();
    assert_eq!(app.world().get::<BattleEffect>(entity).unwrap().timer.elapsed_secs(), 0.0);
    assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
    app.insert_resource(State::new(GameState::Playing));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.5));
    app.update();
    let frames = app
        .world_mut()
        .query::<&Sprite>()
        .iter(app.world())
        .filter_map(|s| s.texture_atlas.as_ref().map(|a| a.index))
        .collect::<Vec<_>>();
    assert!(frames.iter().any(|&index| index > 10), "catch up all atlas frames after a slow frame");
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(6));
    app.update();
    assert!(effects(&mut app).is_empty(), "the result disappears without waiting for a new turn");
    assert!(children.iter().all(|child| app.world().get_entity(*child).is_err()));
    assert_eq!(app.world().resource::<Map>().get(planet.id).image, planet.image);
    assert!(!app.world().resource::<Map>().get(planet.id).is_destroyed);

    let player = app.world().resource::<Player>().clone();
    app.insert_resource(player);
    app.update();
    app.insert_resource(State::new(GameState::GameMenu));
    app.update();
    app.insert_resource(State::new(GameState::Playing));
    app.update();
    assert!(effects(&mut app).is_empty(), "expired results stay gone after refreshes and menus");
}

#[test]
fn battle_result_label_and_ring_fade_before_the_effect_is_removed() {
    let (mut app, planet) = presentation_app();
    app.world_mut().resource_mut::<Player>().reports.push(battle_report(
        1,
        &planet,
        Outcome::Defeat,
    ));
    app.update();
    app.update();
    let entity = effects(&mut app)[0];
    let children = app.world().get::<Children>(entity).unwrap().to_vec();
    let label =
        *children.iter().find(|&&child| app.world().get::<TextColor>(child).is_some()).unwrap();
    let ring = children
        .iter()
        .find_map(|&child| app.world().get::<MeshMaterial2d<ColorMaterial>>(child))
        .unwrap()
        .0
        .clone();
    let mut previous_label_alpha = 1.0;
    let mut previous_ring_alpha = 1.0;
    for (step, max_alpha) in [(2.8, 1.0), (0.7, 0.6), (0.6, 0.1)] {
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(step));
        app.update();
        let label_alpha = app.world().get::<TextColor>(label).unwrap().0.alpha();
        let ring_alpha =
            app.world().resource::<Assets<ColorMaterial>>().get(&ring).unwrap().color.alpha();
        assert!(
            label_alpha > 0.0 && label_alpha <= max_alpha && label_alpha <= previous_label_alpha
        );
        assert!(ring_alpha > 0.0 && ring_alpha <= max_alpha && ring_alpha <= previous_ring_alpha);
        previous_label_alpha = label_alpha;
        previous_ring_alpha = ring_alpha;
    }
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.2));
    app.update();
    assert!(effects(&mut app).is_empty());
    assert!(children.iter().all(|child| app.world().get_entity(*child).is_err()));
}

#[test]
fn advancing_the_turn_removes_unfinished_battle_effects() {
    let (mut app, planet) = presentation_app();
    app.world_mut().resource_mut::<Player>().reports.push(battle_report(
        1,
        &planet,
        Outcome::Victory,
    ));
    app.update();
    app.update();
    let entity = effects(&mut app)[0];
    let children = app.world().get::<Children>(entity).unwrap().to_vec();
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.5));
    app.update();
    assert_eq!(effects(&mut app), vec![entity]);
    app.world_mut().resource_mut::<Settings>().turn = 3;
    app.update();
    assert!(effects(&mut app).is_empty());
    assert!(children.iter().all(|child| app.world().get_entity(*child).is_err()));
}

#[test]
fn destroyed_worlds_keep_a_result_without_duplicate_destruction_explosions() {
    let (mut app, planet) = presentation_app();
    app.world_mut().resource_mut::<Map>().get_mut(planet.id).is_destroyed = true;
    app.world_mut().resource_mut::<Player>().reports.push(battle_report(
        1,
        &planet,
        Outcome::Victory,
    ));
    app.update();
    app.update();
    let entity = effects(&mut app)[0];
    assert_eq!(app.world().get::<Children>(entity).unwrap().len(), 2);
    assert!(app.world().resource::<Messages<PlayAudioMsg>>().is_empty());
}
