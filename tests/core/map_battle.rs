use std::time::Duration;

use bevy::ecs::system::RunSystemOnce;

use super::*;
use crate::core::combat::report::CombatReport;
use crate::core::map::fauna::{
    FaunaPart, FAUNA_ACTION_SECONDS, FAUNA_AFTERMATH_SECONDS, FAUNA_RESULT_LABEL_Y,
};
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
        surviving_attacker: if outcome != Outcome::Defeat {
            Army::from([(Unit::Ship(Ship::LightFighter), 1)])
        } else {
            Army::new()
        },
        surviving_defender: if outcome != Outcome::Victory {
            Army::from([(Unit::Ship(Ship::LightFighter), 1)]).into()
        } else {
            Army::new().into()
        },
        destination_owned: Some(2),
        destination_controlled: Some(2),
        combat_report: Some(CombatReport::default()),
        ..crate::test_support::empty_report(
            Mission::new_with_id(
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
            defender,
        )
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
        .init_resource::<SuppressedMapMissions>()
        .init_resource::<Missions>()
        .init_resource::<WorldAssets>()
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<ColorMaterial>>()
        .init_resource::<Time>()
        .add_message::<PlayAudioMsg>()
        .add_systems(Startup, initialize_battles)
        .add_systems(Update, (show_battles, animate_battles, animate_fauna_aftermath).chain());
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

    captured.mission.army = Army::from([(Unit::war_sun(), 1)]);
    captured.mission.jump_gate = true;
    assert_eq!(MissionArrivalImage::from_report(&captured, attacker).key(), "mission destroy");

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
fn joint_attack_supporters_get_the_attacking_sides_map_result() {
    use std::collections::BTreeMap;

    use crate::core::missions::JointAttackMission;

    let (_app, planet) = presentation_app();
    let mut report = battle_report(21, &planet, Outcome::Victory);
    report.mission.joint_attack = Some(JointAttackMission {
        leader: report.mission.owner,
        attackers: BTreeMap::from([
            (1, Army::from([(Unit::Ship(Ship::LightFighter), 1)])),
            (3, Army::from([(Unit::Ship(Ship::LightFighter), 1)])),
            (4, Army::from([(Unit::Ship(Ship::LightFighter), 1)])),
        ]),
        ..default()
    });

    for id in [1, 3, 4] {
        assert_eq!(Outcome::from_report(&report, &Player::new(id, 0)), Some(Outcome::Victory));
    }
    assert_eq!(Outcome::from_report(&report, &Player::new(2, 0)), Some(Outcome::Defeat));

    report.surviving_attacker.clear();
    report.surviving_defender = Army::from([(Unit::Ship(Ship::LightFighter), 1)]).into();
    for id in [1, 3, 4] {
        assert_eq!(Outcome::from_report(&report, &Player::new(id, 0)), Some(Outcome::Defeat));
    }
    assert_eq!(Outcome::from_report(&report, &Player::new(2, 0)), Some(Outcome::Victory));
}

#[test]
fn world_destruction_replaces_the_battle_result_and_names_the_world_kind() {
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

    let moon = app.world().resource::<Map>().moons()[0];
    assert_eq!(sites.outcomes[&planet.id].labels(moon), vec!["MOON DESTROYED"]);
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
    assert!(!own_spy.intercepted);
    let (start, station) = spy_path(planet.size(), own_spy.direction);
    assert!((station - start).normalize().distance(expected_direction) < 0.000_001);
    assert!(start.dot(expected_direction) < station.dot(expected_direction));

    let defender = Player::new(2, planet.id);
    let detected_spy = SpyPresentation::from_report(&report, &defender, map).unwrap();
    assert_eq!(detected_spy.direction, Vec2::X);
    assert!(!detected_spy.intercepted);

    report.scout_probes = 0;
    let destroyed_spy = SpyPresentation::from_report(&report, &defender, map).unwrap();
    assert!(destroyed_spy.intercepted);
}

#[test]
fn spy_probe_stops_outside_the_planet_to_scan_from_orbit() {
    let size = 100.0;
    let direction = Vec2::new(3.0, 4.0).normalize();

    let (start, station) = spy_path(size, direction);
    assert!(start.length() > size);
    assert!(station.length() > size * 0.5);
    assert!(station.length() < start.length());
    assert!((station - start).normalize().distance(direction) < 0.000_001);
    assert!(spy_scan_radius(size, station) < station.length() - size * 0.45);
}

#[test]
fn successful_spy_aftermath_parks_one_probe_and_scans_without_an_interception() {
    let (mut app, planet) = presentation_app();
    let mut report = battle_report(9, &planet, Outcome::Victory);
    report.mission.objective = Icon::Spy;
    report.mission.army = Army::from([(Unit::probe(), 3)]);
    report.combat_report = None;
    report.scout_probes = 2;
    let player = app.world().resource::<Player>().clone();
    let home = app.world().resource::<Map>().get(player.home_planet).clone();
    let returning_id = 90;
    app.world_mut().resource_mut::<Missions>().0.push(
        Mission::new_with_id(
            returning_id,
            2,
            player.id,
            &planet,
            &home,
            Icon::Deploy,
            Army::from([(Unit::probe(), 2)]),
            BombingRaid::None,
            false,
            false,
            None,
        )
        .with_return_objective(Icon::Spy),
    );
    let return_position =
        app.world().resource::<Missions>().get(returning_id).unwrap().position - planet.position;
    app.world_mut().resource_mut::<Player>().reports.push(report);

    app.update();
    assert!(app.world().resource::<SuppressedMapMissions>().contains(returning_id));
    app.update();
    let entity = effects(&mut app)[0];
    let children = app.world().get::<Children>(entity).unwrap().to_vec();
    let probe = *children
        .iter()
        .find(|&&child| {
            matches!(app.world().get::<EffectPart>(child), Some(EffectPart::SpyProbe { .. }))
        })
        .unwrap();
    assert_eq!(
        app.world().get::<Sprite>(probe).unwrap().image,
        app.world().resource::<WorldAssets>().image("mission spy")
    );
    assert_eq!(
        children
            .iter()
            .filter(|&&child| matches!(
                app.world().get::<EffectPart>(child),
                Some(EffectPart::SpyProbe { .. })
            ))
            .count(),
        1
    );
    assert_eq!(
        children
            .iter()
            .filter(|&&child| matches!(
                app.world().get::<EffectPart>(child),
                Some(EffectPart::SpyScanArc { .. })
            ))
            .count(),
        SPY_SCAN_ARC_COUNT
    );
    assert_eq!(
        children
            .iter()
            .filter(|&&child| matches!(
                app.world().get::<EffectPart>(child),
                Some(EffectPart::SpyPlanetWave { .. })
            ))
            .count(),
        SPY_SCAN_ARC_COUNT
    );
    assert!(children.iter().all(|&child| {
        !matches!(
            app.world().get::<EffectPart>(child),
            Some(
                EffectPart::Ripple { .. }
                    | EffectPart::Explosion { .. }
                    | EffectPart::SpyInterceptor { .. }
                    | EffectPart::SpyExplosion { .. }
            )
        )
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
        matches!(app.world().get::<EffectPart>(child), Some(EffectPart::SpyProbe { .. }))
            && app.world().get::<Sprite>(child).is_some_and(|sprite| sprite.color.alpha() > 0.0)
    }));

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(1.0));
    app.update();
    assert!(children.iter().any(|&child| {
        matches!(app.world().get::<EffectPart>(child), Some(EffectPart::SpyScanArc { .. }))
            && app
                .world()
                .get::<MeshMaterial2d<ColorMaterial>>(child)
                .and_then(|handle| app.world().resource::<Assets<ColorMaterial>>().get(&handle.0))
                .is_some_and(|material| material.color.alpha() > 0.0)
    }));
    assert!(children.iter().any(|&child| {
        matches!(app.world().get::<EffectPart>(child), Some(EffectPart::SpyPlanetWave { .. }))
            && app
                .world()
                .get::<MeshMaterial2d<ColorMaterial>>(child)
                .and_then(|handle| app.world().resource::<Assets<ColorMaterial>>().get(&handle.0))
                .is_some_and(|material| material.color.alpha() > 0.0)
    }));

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(
        SPY_DEPART_START_SECONDS + SPY_DEPART_SECONDS - 1.55 - 0.01,
    ));
    app.update();
    assert!(app.world().resource::<SuppressedMapMissions>().contains(returning_id));

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.02));
    app.update();
    assert!(!app.world().resource::<SuppressedMapMissions>().contains(returning_id));
    assert!(
        app.world()
            .get::<Transform>(probe)
            .unwrap()
            .translation
            .truncate()
            .distance(return_position)
            < 0.000_001
    );
    assert_eq!(app.world().get::<Sprite>(probe).unwrap().color.alpha(), 0.0);
    assert!(app.world().get::<Sprite>(probe).unwrap().flip_x);
    assert_eq!(app.world().get::<Transform>(probe).unwrap().rotation, Quat::IDENTITY);
}

#[test]
fn spy_scan_mesh_is_a_partial_arc_instead_of_a_full_ring() {
    let mesh = spy_scan_arc_mesh();
    let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).unwrap().as_float3().unwrap();
    assert_eq!(positions.len(), 50);
    assert!(positions.iter().all(|position| position[0] > 0.0));
    assert!(positions.iter().any(|position| position[1] < 0.0));
    assert!(positions.iter().any(|position| position[1] > 0.0));
}

#[test]
fn failed_spy_aftermath_launches_ships_that_destroy_the_probe_during_the_scan() {
    let (mut app, planet) = presentation_app();
    let mut report = battle_report(10, &planet, Outcome::Defeat);
    report.mission.objective = Icon::Spy;
    report.mission.army = Army::from([(Unit::probe(), 3)]);
    report.combat_report = None;
    report.scout_probes = 0;
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
                Some(EffectPart::SpyInterceptor { .. })
            ))
            .count(),
        2
    );
    assert_eq!(
        children
            .iter()
            .filter(|&&child| matches!(
                app.world().get::<EffectPart>(child),
                Some(EffectPart::SpyExplosion { .. })
            ))
            .count(),
        1
    );
    assert_eq!(app.world().resource::<Messages<PlayAudioMsg>>().len(), 1);

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(1.55));
    app.update();
    assert!(children.iter().any(|&child| {
        matches!(app.world().get::<EffectPart>(child), Some(EffectPart::SpyInterceptor { .. }))
            && app.world().get::<Sprite>(child).is_some_and(|sprite| sprite.color.alpha() > 0.0)
    }));

    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.5));
    app.update();
    assert!(children.iter().any(|&child| {
        matches!(app.world().get::<EffectPart>(child), Some(EffectPart::SpyProbe { .. }))
            && app.world().get::<Sprite>(child).is_some_and(|sprite| sprite.color.alpha() == 0.0)
    }));
    assert!(children.iter().any(|&child| {
        matches!(app.world().get::<EffectPart>(child), Some(EffectPart::SpyExplosion { .. }))
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
    let arrival = *children
        .iter()
        .find(|&&child| {
            matches!(
                app.world().get::<EffectPart>(child),
                Some(EffectPart::TerritoryArrival { .. })
            )
        })
        .unwrap();
    assert_eq!(
        app.world().get::<Sprite>(arrival).unwrap().image,
        app.world().resource::<WorldAssets>().image("mission")
    );
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
    assert!(app.world().get::<Sprite>(arrival).unwrap().color.alpha() > 0.0);
    let arrival_position = app.world().get::<Transform>(arrival).unwrap().translation.truncate();
    let EffectPart::TerritoryArrival {
        start,
        orbit,
        ..
    } = app.world().get::<EffectPart>(arrival).unwrap()
    else {
        unreachable!();
    };
    assert!(arrival_position.distance(*start) > 0.0);
    assert!(arrival_position.distance(*orbit) < start.distance(*orbit));
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

    let mut previous_label_y = app.world().get::<Transform>(label).unwrap().translation.y;
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(crate::core::map::AFTERMATH_LABEL_EXTENSION_SECONDS));
    app.update();
    assert!(app.world().get_entity(entity).is_ok());
    assert!(app.world().get::<TextColor>(label).unwrap().0.alpha() > 0.9);
    assert!(app.world().get::<Transform>(label).unwrap().translation.y > previous_label_y);

    for (step, max_alpha) in [(0.9, 0.6), (0.6, 0.1)] {
        previous_label_y = app.world().get::<Transform>(label).unwrap().translation.y;
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(step));
        app.update();
        let label_alpha = app.world().get::<TextColor>(label).unwrap().0.alpha();
        assert!(label_alpha > 0.0 && label_alpha <= max_alpha);
        assert!(app.world().get::<Transform>(label).unwrap().translation.y > previous_label_y);
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
        player.color = viewer_color;
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
fn fauna_aftermath_uses_the_viewing_players_color_for_wins_and_losses() {
    for (id, outcome) in [(1, Outcome::Victory), (2, Outcome::Defeat)] {
        let (mut app, planet) = presentation_app();
        let viewer_color = PlayerColor::new(4).unwrap();
        app.world_mut().resource_mut::<Player>().color = viewer_color;

        let mut report = battle_report(id, &planet, outcome);
        report.planet.name = "Rift Serpent".into();
        report.planet.army.clear();
        report
            .planet
            .army
            .insert(Unit::Fauna(crate::core::units::fauna::SpaceFauna::RiftSerpent), 1);
        app.world_mut().resource_mut::<Player>().reports.push(report);
        app.update();
        app.update();

        let entity = app
            .world_mut()
            .query_filtered::<Entity, With<FaunaEffect>>()
            .single(app.world())
            .unwrap();
        let expected = viewer_color.color();
        for &child in app.world().get::<Children>(entity).unwrap() {
            if let Some(sprite) = app.world().get::<Sprite>(child) {
                if matches!(
                    app.world().get::<FaunaPart>(child),
                    Some(FaunaPart::Shot { .. } | FaunaPart::Blast { .. })
                ) {
                    continue;
                } else {
                    assert_same_rgb(sprite.color, expected);
                }
            }
            if let Some(text) = app.world().get::<TextColor>(child) {
                assert_same_rgb(text.0, expected);
            }
            if let Some(material) = app.world().get::<MeshMaterial2d<ColorMaterial>>(child) {
                if matches!(app.world().get::<FaunaPart>(child), Some(FaunaPart::Creature(_))) {
                    let actual = app
                        .world()
                        .resource::<Assets<ColorMaterial>>()
                        .get(&material.0)
                        .unwrap()
                        .color;
                    assert_same_rgb(actual, Color::srgb(0.86, 0.93, 1.0));
                }
            }
        }
    }
}

#[test]
fn fauna_aftermath_replaces_the_original_map_mission_until_it_finishes() {
    let (mut app, planet) = presentation_app();
    let mut report = battle_report(1, &planet, Outcome::Victory);
    report.planet.name = "Rift Serpent".into();
    report.planet.army =
        Army::from([(Unit::Fauna(crate::core::units::fauna::SpaceFauna::RiftSerpent), 1)]).into();
    let mission = report.mission.clone();
    let mission_id = mission.id;
    app.world_mut().resource_mut::<Missions>().0.push(mission);
    app.world_mut().resource_mut::<Player>().reports.push(report);

    app.update();
    assert!(
        !app.world().resource::<SuppressedMapMissions>().contains(mission_id),
        "turn-start observation must wait before replacing the map mission"
    );
    app.update();
    let effect = app.world_mut().query::<&FaunaEffect>().single(app.world()).unwrap();
    assert_eq!(effect.mission, mission_id);
    assert!(app.world().resource::<SuppressedMapMissions>().contains(mission_id));

    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(FAUNA_AFTERMATH_SECONDS + 0.1));
    app.update();
    assert!(!app.world().resource::<SuppressedMapMissions>().contains(mission_id));
    assert_eq!(
        app.world_mut().query_filtered::<Entity, With<FaunaEffect>>().iter(app.world()).count(),
        0
    );
}

#[test]
fn fauna_map_miniatures_follow_the_actual_formation_size_and_variety() {
    use crate::core::units::fauna::SpaceFauna::{AetherRay, RiftSerpent, StarKraken};

    let (app, planet) = presentation_app();
    let mut report = battle_report(1, &planet, Outcome::Victory);
    report.planet.army.clear();
    report.planet.army.extend([
        (Unit::Fauna(AetherRay), 4),
        (Unit::Fauna(StarKraken), 2),
        (Unit::Fauna(RiftSerpent), 1),
    ]);

    let displayed = fauna_display_creatures(&report);
    assert_eq!(displayed.len(), 3);
    assert!(displayed.contains(&AetherRay));
    assert!(displayed.contains(&StarKraken));
    assert!(displayed.contains(&RiftSerpent));

    report.planet.army = Army::from([(Unit::Fauna(AetherRay), 2)]).into();
    assert_eq!(fauna_display_creatures(&report), vec![AetherRay, AetherRay]);
    report.planet.army = Army::from([(Unit::Fauna(AetherRay), 1)]).into();
    assert_eq!(fauna_display_creatures(&report), vec![AetherRay]);

    drop(app);
}

#[test]
fn fauna_outcomes_show_recorded_survivors_and_only_recorded_return_fire() {
    use crate::core::combat::report::RoundReport;
    use crate::core::combat::resolution::{CombatUnit, ShotReport};
    let (app, planet) = presentation_app();
    let mut report = battle_report(1, &planet, Outcome::Victory);
    let fauna = Unit::Fauna(SpaceFauna::AetherRay);
    report.planet.army = Army::from([(fauna, 3)]).into();
    report.surviving_defender = Army::from([(fauna, 1)]).into();
    report.combat_report.as_mut().unwrap().rounds.push(RoundReport {
        attacker: vec![CombatUnit {
            id: 1,
            owner: Some(1),
            unit: Unit::Ship(Ship::Cruiser),
            hull: 1,
            shield: 0,
            repairs: vec![],
            shots: vec![ShotReport::default()],
        }],
        ..default()
    });
    let mut player = app.world().resource::<Player>().clone();
    player.reports.push(report.clone());
    let mut sites = BattleSites::default();
    assert!(sites.observe(&player, app.world().resource::<Map>(), 2));
    assert_eq!(sites.fauna_outcomes[&1].creature_survivors, [true, false, false]);
    assert_eq!(sites.fauna_outcomes[&1].return_fire, Some(Unit::Ship(Ship::Cruiser)));

    report.id = 2;
    report.combat_report.as_mut().unwrap().rounds[0].attacker[0].shots.clear();
    player.reports.push(report);
    assert!(sites.observe(&player, app.world().resource::<Map>(), 2));
    assert_eq!(sites.fauna_outcomes[&2].return_fire, None);
}

#[test]
fn a_lost_fauna_encounter_still_explodes_the_mission_and_flies_past_it() {
    let (mut app, planet) = presentation_app();
    let mut report = battle_report(1, &planet, Outcome::Defeat);
    report.planet.army = Army::from([(Unit::Fauna(SpaceFauna::AetherRay), 1)]).into();
    report.surviving_defender = report.planet.army.clone();
    app.world_mut().resource_mut::<Player>().reports.push(report);
    app.update();
    app.update();
    let root =
        app.world_mut().query_filtered::<Entity, With<FaunaEffect>>().single(app.world()).unwrap();
    let children = app.world().get::<Children>(root).unwrap().to_vec();
    assert_eq!(
        children
            .iter()
            .filter(|child| {
                matches!(
                    app.world().get::<FaunaPart>(**child),
                    Some(FaunaPart::Blast {
                        creature: None,
                        ..
                    })
                )
            })
            .count(),
        3
    );
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(FAUNA_ACTION_SECONDS * 0.76));
    app.update();
    for child in children {
        match app.world().get::<FaunaPart>(child) {
            Some(FaunaPart::Mission) => {
                assert_eq!(app.world().get::<Sprite>(child).unwrap().color.alpha(), 0.0);
            },
            Some(FaunaPart::Creature(_)) => {
                let material = app.world().get::<MeshMaterial2d<ColorMaterial>>(child).unwrap();
                assert!(
                    app.world()
                        .resource::<Assets<ColorMaterial>>()
                        .get(&material.0)
                        .unwrap()
                        .color
                        .alpha()
                        > 0.9
                );
                assert!(
                    app.world().get::<Transform>(child).unwrap().translation.truncate().length()
                        > 60.0
                );
            },
            _ => {},
        }
    }
}

#[test]
fn fauna_rings_open_every_outcome_then_stay_hidden_for_the_rest_of_the_encounter() {
    for outcome in [Outcome::Victory, Outcome::Draw, Outcome::Defeat] {
        let (mut app, planet) = presentation_app();
        let viewer_color = PlayerColor::new(4).unwrap();
        app.world_mut().resource_mut::<Player>().color = viewer_color;
        let mut report = battle_report(1, &planet, outcome);
        report.planet.army = Army::from([(Unit::Fauna(SpaceFauna::RiftSerpent), 1)]).into();
        if outcome != Outcome::Victory {
            report.surviving_defender = report.planet.army.clone();
        }
        app.world_mut().resource_mut::<Player>().reports.push(report);
        app.update();
        app.update();
        let root = app
            .world_mut()
            .query_filtered::<Entity, With<FaunaEffect>>()
            .single(app.world())
            .unwrap();
        let children = app.world().get::<Children>(root).unwrap().to_vec();
        let rings = children
            .iter()
            .copied()
            .filter(|child| {
                matches!(app.world().get::<FaunaPart>(*child), Some(FaunaPart::Ripple(_)))
            })
            .collect::<Vec<_>>();
        assert_eq!(rings.len(), 4, "every outcome must include the opening rings");
        let alpha = |world: &World, ring| {
            let color = world.get::<Sprite>(ring).unwrap().color;
            assert_same_rgb(color, viewer_color.color());
            color.alpha()
        };
        assert!(rings.iter().all(|ring| alpha(app.world(), *ring) == 0.0));
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(FAUNA_ACTION_SECONDS * 0.06));
        app.update();
        assert_eq!(rings.iter().filter(|ring| alpha(app.world(), **ring) > 0.0).count(), 2);
        let outer = app.world().get::<Transform>(rings[0]).unwrap();
        let inner = app.world().get::<Transform>(rings[1]).unwrap();
        assert!(outer.scale.x > inner.scale.x);
        let mission = children
            .iter()
            .find(|child| matches!(app.world().get::<FaunaPart>(**child), Some(FaunaPart::Mission)))
            .unwrap();
        let ship_position = app.world().get::<Transform>(*mission).unwrap().translation.truncate();
        assert!(rings.iter().all(|ring| {
            app.world()
                .get::<Transform>(*ring)
                .unwrap()
                .translation
                .truncate()
                .abs_diff_eq(ship_position, 0.001)
        }));
        let paused_alpha = alpha(app.world(), rings[0]);
        let paused_scale = outer.scale;
        app.insert_resource(State::new(GameState::GameMenu));
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(2));
        app.update();
        assert_eq!(alpha(app.world(), rings[0]), paused_alpha);
        assert_eq!(app.world().get::<Transform>(rings[0]).unwrap().scale, paused_scale);
        app.insert_resource(State::new(GameState::Playing));
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(FAUNA_ACTION_SECONDS * 0.13));
        app.update();
        assert_eq!(alpha(app.world(), rings[0]), 0.0);
        assert!(alpha(app.world(), rings[3]) > 0.1);
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(FAUNA_ACTION_SECONDS * 0.08));
        app.update();
        assert!(
            rings.iter().all(|ring| alpha(app.world(), *ring) == 0.0),
            "the opening rings must finish before the attack"
        );
        app.world_mut()
            .resource_mut::<Time>()
            .advance_by(Duration::from_secs_f32(FAUNA_ACTION_SECONDS * 0.53));
        app.update();
        assert!(
            rings.iter().all(|ring| alpha(app.world(), *ring) == 0.0),
            "rings must not return when the outcome is revealed"
        );
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(
            FAUNA_ACTION_SECONDS * 0.20
                + crate::core::map::AFTERMATH_LABEL_EXTENSION_SECONDS
                + 0.01,
        ));
        app.update();
        assert!(rings.iter().all(|ring| app.world().get_entity(*ring).is_err()));
        assert!(!app.world().resource::<SuppressedMapMissions>().contains(1));
    }
}

#[test]
fn fauna_group_animation_deforms_all_creatures_then_removes_them_and_releases_the_mission() {
    use crate::core::map::fauna::FaunaPart;
    use crate::core::units::fauna::SpaceFauna::{AetherRay, IonWisp, RiftSerpent};
    assert!((7.0..=8.0).contains(&FAUNA_ACTION_SECONDS));
    assert_eq!(
        FAUNA_AFTERMATH_SECONDS,
        FAUNA_ACTION_SECONDS + crate::core::map::AFTERMATH_LABEL_EXTENSION_SECONDS
    );
    let (mut app, planet) = presentation_app();
    let mut report = battle_report(1, &planet, Outcome::Victory);
    report.planet.army = Army::from([
        (Unit::Fauna(AetherRay), 1),
        (Unit::Fauna(IonWisp), 1),
        (Unit::Fauna(RiftSerpent), 1),
    ])
    .into();
    app.world_mut().resource_mut::<Player>().reports.push(report);
    app.update();
    app.update();
    let root =
        app.world_mut().query_filtered::<Entity, With<FaunaEffect>>().single(app.world()).unwrap();
    let children = app.world().get::<Children>(root).unwrap().to_vec();
    let creatures = children
        .iter()
        .copied()
        .filter(|child| {
            matches!(app.world().get::<FaunaPart>(*child), Some(FaunaPart::Creature(_)))
        })
        .collect::<Vec<_>>();
    assert_eq!(creatures.len(), 3);
    let label = children
        .iter()
        .copied()
        .find(|child| matches!(app.world().get::<FaunaPart>(*child), Some(FaunaPart::Label(_))))
        .unwrap();
    assert!(matches!(
        app.world().get::<FaunaPart>(label),
        Some(FaunaPart::Label(y)) if *y == FAUNA_RESULT_LABEL_Y
    ));
    assert!(
        children.iter().all(|child| {
            !matches!(app.world().get::<FaunaPart>(*child), Some(FaunaPart::Blast { .. }))
        }),
        "a victory should fade the diving creatures without explosions"
    );
    let mesh_positions = |world: &World, entity| {
        world
            .resource::<Assets<Mesh>>()
            .get(&world.get::<Mesh2d>(entity).unwrap().0)
            .unwrap()
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .unwrap()
            .as_float3()
            .unwrap()
            .to_vec()
    };
    let original = mesh_positions(app.world(), creatures[0]);
    let orbit_time = FAUNA_ACTION_SECONDS * 0.16;
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(orbit_time));
    app.update();
    assert_ne!(original, mesh_positions(app.world(), creatures[0]));
    for creature in &creatures {
        let position = app.world().get::<Transform>(*creature).unwrap().translation;
        assert!(
            position.truncate().length() > 75.0,
            "orbit surrounds the ship instead of wobbling beside it"
        );
    }
    app.insert_resource(State::new(GameState::GameMenu));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(4));
    app.update();
    assert_eq!(app.world().get::<FaunaEffect>(root).unwrap().timer.elapsed_secs(), orbit_time);
    assert!(app.world().resource::<SuppressedMapMissions>().contains(1));
    app.insert_resource(State::new(GameState::Playing));
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(FAUNA_ACTION_SECONDS * 0.34));
    app.update();
    let creature_alpha = |world: &World, creature| {
        world
            .resource::<Assets<ColorMaterial>>()
            .get(&world.get::<MeshMaterial2d<ColorMaterial>>(creature).unwrap().0)
            .unwrap()
            .color
            .alpha()
    };
    assert!(
        creature_alpha(app.world(), creatures[0]) > 0.1,
        "the action must stretch with the duration, not finish early and leave a blank hold"
    );
    for creature in &creatures {
        let scale = app.world().get::<Transform>(*creature).unwrap().scale.x;
        assert!((0.0..1.0).contains(&scale), "the terminal dive must visibly recede");
        assert!(creature_alpha(app.world(), *creature) < 1.0, "the fade must accompany the dive");
    }
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(FAUNA_ACTION_SECONDS * 0.26));
    app.update();
    for creature in &creatures {
        assert_eq!(creature_alpha(app.world(), *creature), 0.0);
        assert_eq!(app.world().get::<Transform>(*creature).unwrap().scale, Vec3::ZERO);
        assert!(
            app.world().get::<Transform>(*creature).unwrap().translation.truncate().length() < 0.01,
            "defeated creatures disappear into the mission instead of flying out the other side"
        );
    }
    let mission = children
        .iter()
        .find(|child| matches!(app.world().get::<FaunaPart>(**child), Some(FaunaPart::Mission)))
        .unwrap();
    assert_eq!(app.world().get::<Sprite>(*mission).unwrap().color.alpha(), 1.0);
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(FAUNA_ACTION_SECONDS * 0.24 - 0.01));
    app.update();
    assert!(app.world().get_entity(root).is_ok());
    assert!(app.world().resource::<SuppressedMapMissions>().contains(1));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.02));
    app.update();
    assert!(app.world().get_entity(root).is_ok());
    assert!(app.world().get::<TextColor>(label).unwrap().0.alpha() > 0.9);
    app.world_mut()
        .resource_mut::<Time>()
        .advance_by(Duration::from_secs_f32(crate::core::map::AFTERMATH_LABEL_EXTENSION_SECONDS));
    app.update();
    assert!(app.world().get_entity(root).is_err());
    assert!(children.iter().all(|child| app.world().get_entity(*child).is_err()));
    assert!(!app.world().resource::<SuppressedMapMissions>().contains(1));
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
