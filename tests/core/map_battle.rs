use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;

use super::*;
use crate::core::combat::report::CombatReport;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::player::PlayerColor;
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

fn assert_same_rgb(actual: Color, expected: Color) {
    assert!(actual
        .with_alpha(1.0)
        .to_srgba()
        .to_vec4()
        .abs_diff_eq(expected.to_srgba().to_vec4(), 1e-6));
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
        assert!(sites.observe(&player, app.world().resource::<Map>(), 2));
        let labels = sites.outcomes[&planet.id].labels(&planet);
        assert_eq!(labels.len(), 1);
        assert_eq!(labels[0], expected);
    }

    let moon = app.world().resource::<Map>().moons()[0];
    let mut captured_moon = battle_report(3, moon, Outcome::Victory);
    captured_moon.destination_owned = Some(attacker.id);
    captured_moon.destination_controlled = Some(attacker.id);
    assert_eq!(
        TerritoryOutcome::from_report(&captured_moon, attacker),
        Some(TerritoryOutcome::Conquered)
    );
    assert_eq!(
        SiteOutcome {
            territory: Some(TerritoryOutcome::Conquered),
            ..default()
        }
        .labels(moon),
        vec!["MOON CONQUERED"]
    );
}

#[test]
fn planet_destruction_replaces_the_ordinary_battle_result_headline() {
    let (app, planet) = presentation_app();
    let mut player = app.world().resource::<Player>().clone();
    let mut report = battle_report(11, &planet, Outcome::Victory);
    report.mission.objective = Icon::Destroy;
    report.planet_destroyed = true;
    report.destination_owned = None;
    report.destination_controlled = None;

    assert_eq!(Outcome::from_report(&report, &player), None);
    assert_eq!(TerritoryOutcome::from_report(&report, &player), None);
    assert!(planet_destruction_visible(&report, &player));

    player.reports.push(report);
    let mut sites = BattleSites::default();
    assert!(sites.observe(&player, app.world().resource::<Map>(), 2));
    assert_eq!(sites.outcomes[&planet.id].labels(&planet), vec!["PLANET DESTROYED"]);
}

#[test]
fn only_current_visible_report_outcomes_produce_aftermath() {
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
    assert!(sites.observe(&player, app.world().resource::<Map>(), 2));
    assert_eq!(sites.pending, BTreeSet::from([planet.id]));
    sites.pending.clear();
    player.reports.push(report);
    assert!(sites.observe(&player, app.world().resource::<Map>(), 2));
    assert_eq!(sites.pending, BTreeSet::from([planet.id]));
}

#[test]
fn spy_outcomes_follow_local_visibility_and_probe_survival() {
    let (app, planet) = presentation_app();
    let attacker = app.world().resource::<Player>();
    let defender = Player::new(2, planet.id);
    let mut report = battle_report(8, &planet, Outcome::Victory);
    report.mission.objective = Icon::Spy;
    report.mission.army = Army::from([(Unit::probe(), 3)]);
    report.combat_report = None;
    report.scout_probes = 2;

    assert_eq!(SpyOutcome::from_report(&report, attacker), Some(SpyOutcome::Success));
    assert_eq!(SpyOutcome::from_report(&report, &defender), Some(SpyOutcome::Detected));
    assert_eq!(SpyOutcome::from_report(&report, &Player::new(3, 0)), None);

    report.scout_probes = 0;
    assert_eq!(SpyOutcome::from_report(&report, attacker), Some(SpyOutcome::Failed));
    report.hidden = true;
    assert_eq!(SpyOutcome::from_report(&report, attacker), None);
}

#[test]
fn spy_aftermath_approaches_from_the_reported_origin_without_revealing_enemy_origins() {
    let (app, planet) = presentation_app();
    let player = app.world().resource::<Player>();
    let map = app.world().resource::<Map>();
    let origin = map.get(player.home_planet);
    assert_ne!(origin.id, planet.id);

    let mut report = battle_report(7, &planet, Outcome::Victory);
    report.mission.objective = Icon::Spy;
    report.mission.origin = origin.id;
    report.mission.army = Army::from([(Unit::probe(), 2)]);
    report.combat_report = None;
    report.scout_probes = 2;
    let expected_direction = (planet.position - origin.position).normalize();

    let own_spy = SpyPresentation::from_report(&report, player, map).unwrap();
    assert!(own_spy.direction.distance(expected_direction) < 0.000_001);
    let (start, end) = spy_path(planet.size(), 0.0, own_spy.direction);
    assert!((end - start).normalize().distance(expected_direction) < 0.000_001);
    assert!(start.dot(expected_direction) < end.dot(expected_direction));

    let defender = Player::new(2, planet.id);
    let detected_spy = SpyPresentation::from_report(&report, &defender, map).unwrap();
    assert_eq!(detected_spy.direction, Vec2::X);
}

#[test]
fn spy_sweep_stops_on_the_planet_before_fading_out() {
    let size = 100.0;
    let direction = Vec2::new(3.0, 4.0).normalize();

    for lateral_offset in [-0.28, 0.32] {
        let (start, end) = spy_path(size, lateral_offset, direction);
        let eased_fade_start =
            SPY_FADE_OUT_START * SPY_FADE_OUT_START * (3.0 - 2.0 * SPY_FADE_OUT_START);
        let fade_start = start.lerp(end, eased_fade_start);
        assert!(start.length() > size);
        assert!(fade_start.length() < size * 0.5);
        assert!(end.length() < size * 0.5);
        assert!(start.dot(direction) < end.dot(direction));
    }

    assert_eq!(spy_sweep_alpha(0.0), 0.0);
    assert_eq!(spy_sweep_alpha(0.1), 1.0);
    assert_eq!(spy_sweep_alpha(SPY_FADE_OUT_START), 1.0);
    assert!(spy_sweep_alpha(0.85) < 1.0);
    assert_eq!(spy_sweep_alpha(1.0), 0.0);
}

#[test]
fn spy_aftermath_sweeps_probes_and_uses_scan_ripples_without_explosions() {
    let (mut app, planet) = presentation_app();
    let mut report = battle_report(9, &planet, Outcome::Victory);
    report.mission.objective = Icon::Spy;
    report.mission.army = Army::from([(Unit::probe(), 3)]);
    report.combat_report = None;
    report.scout_probes = 2;
    app.world_mut().resource_mut::<Player>().reports.push(report);

    app.update();
    app.update();
    let entity = effects(&mut app)[0];
    let children = app.world().get::<Children>(entity).unwrap().to_vec();
    assert_eq!(
        children
            .iter()
            .filter(|&&child| matches!(
                app.world().get::<EffectPart>(child),
                Some(EffectPart::SpySweep { .. })
            ))
            .count(),
        2
    );
    assert_eq!(
        children
            .iter()
            .filter(|&&child| matches!(
                app.world().get::<EffectPart>(child),
                Some(EffectPart::Ripple { .. })
            ))
            .count(),
        RIPPLE_COUNT
    );
    assert!(children.iter().all(|&child| {
        !matches!(app.world().get::<EffectPart>(child), Some(EffectPart::Explosion { .. }))
    }));
    assert!(app
        .world_mut()
        .query::<&Text2d>()
        .iter(app.world())
        .any(|text| text.0 == "SPY MISSION SUCCESSFUL"));
    assert!(app.world().resource::<Messages<PlayAudioMsg>>().is_empty());

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.55));
    app.update();
    assert!(children.iter().any(|&child| {
        matches!(app.world().get::<EffectPart>(child), Some(EffectPart::SpySweep { .. }))
            && app.world().get::<Sprite>(child).is_some_and(|sprite| sprite.color.alpha() > 0.0)
    }));
}

#[test]
fn missile_aftermath_approaches_from_the_reported_origin() {
    let (app, planet) = presentation_app();
    let mut player = app.world().resource::<Player>().clone();
    let map = app.world().resource::<Map>();
    let origin = map.get(player.home_planet);
    assert_ne!(origin.id, planet.id);

    let mut report = battle_report(7, &planet, Outcome::Victory);
    report.mission.objective = Icon::MissileStrike;
    report.mission.origin = origin.id;
    let expected_direction = (planet.position - origin.position).normalize();
    player.reports.push(report);

    let mut sites = BattleSites::default();
    assert!(sites.observe(&player, map, 2));
    let missile = sites.outcomes[&planet.id].missile.unwrap();
    assert!(missile.direction.distance(expected_direction) < 0.000_001);

    let (start, end) = missile_path(planet.size(), 1, 0.0, missile.direction);
    assert!((end - start).normalize().distance(expected_direction) < 0.000_001);
    assert!(start.dot(expected_direction) < end.dot(expected_direction));
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
fn conquest_ripples_expand_fade_and_finish_before_the_result_label() {
    let (mut app, planet) = presentation_app();
    let mut report = battle_report(1, &planet, Outcome::Victory);
    let local_player = app.world().resource::<Player>().id;
    report.destination_owned = Some(local_player);
    report.destination_controlled = Some(local_player);
    app.world_mut().resource_mut::<Player>().reports.push(report);
    app.update();
    app.update();
    let entity = effects(&mut app)[0];
    let children = app.world().get::<Children>(entity).unwrap().to_vec();
    let label =
        *children.iter().find(|&&child| app.world().get::<TextColor>(child).is_some()).unwrap();
    let ripples = children
        .iter()
        .filter_map(|&child| {
            matches!(app.world().get::<EffectPart>(child), Some(EffectPart::Ripple { .. }))
                .then_some(child)
        })
        .collect::<Vec<_>>();
    assert_eq!(ripples.len(), RIPPLE_COUNT);

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.8));
    app.update();
    let active_ripples = ripples
        .iter()
        .filter_map(|&ripple| {
            let material = app.world().get::<MeshMaterial2d<ColorMaterial>>(ripple)?;
            let alpha =
                app.world().resource::<Assets<ColorMaterial>>().get(&material.0)?.color.alpha();
            (alpha > 0.0).then_some((app.world().get::<Transform>(ripple)?.scale.x, alpha))
        })
        .collect::<Vec<_>>();
    assert_eq!(active_ripples.len(), 3, "three staggered waves should be travelling");
    assert!(active_ripples.windows(2).all(|waves| waves[0].0 > waves[1].0));
    assert!(active_ripples.iter().all(|(_, alpha)| *alpha < 0.68));

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(1.8));
    app.update();
    assert!(ripples.iter().all(|ripple| app.world().get_entity(*ripple).is_err()));
    assert!(app.world().get::<TextColor>(label).unwrap().0.alpha() > 0.9);

    let mut previous_label_alpha = 1.0;
    for (step, max_alpha) in [(0.2, 1.0), (0.7, 0.6), (0.6, 0.1)] {
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(step));
        app.update();
        let label_alpha = app.world().get::<TextColor>(label).unwrap().0.alpha();
        assert!(
            label_alpha > 0.0 && label_alpha <= max_alpha && label_alpha <= previous_label_alpha
        );
        previous_label_alpha = label_alpha;
    }
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.2));
    app.update();
    assert!(effects(&mut app).is_empty());
    assert!(children.iter().all(|child| app.world().get_entity(*child).is_err()));
}

#[test]
fn planet_aftermath_uses_the_viewing_players_color_for_every_part() {
    let (mut app, planet) = presentation_app();
    let viewer_color = PlayerColor::new(4).unwrap();
    let local_player = {
        let mut player = app.world_mut().resource_mut::<Player>();
        player.color = Some(viewer_color);
        player.id
    };
    let mut report = battle_report(1, &planet, Outcome::Victory);
    report.destination_owned = Some(local_player);
    report.destination_controlled = Some(local_player);
    let mut missile = battle_report(2, &planet, Outcome::Victory);
    missile.mission.objective = Icon::MissileStrike;
    let mut spy = battle_report(3, &planet, Outcome::Victory);
    spy.mission.objective = Icon::Spy;
    app.world_mut().resource_mut::<Player>().reports.extend([report, missile, spy]);
    app.update();
    app.update();

    let entity = effects(&mut app)[0];
    let expected = viewer_color.color();
    for &child in app.world().get::<Children>(entity).unwrap() {
        if let Some(sprite) = app.world().get::<Sprite>(child) {
            assert_same_rgb(sprite.color, expected);
        }
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
    assert_eq!(app.world().get::<Children>(entity).unwrap().len(), RIPPLE_COUNT + 1);
    assert!(app.world().resource::<Messages<PlayAudioMsg>>().is_empty());
}
