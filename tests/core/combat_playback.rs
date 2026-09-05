use super::*;
use crate::core::camera::MainCamera;
use crate::core::combat::effects::PendingImpact;
use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::map::model::Map;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::random::DeterministicRngState;
use crate::core::units::{defense::Defense, ships::Ship, Army};
use crate::multiplayer::client::MultiplayerSession;
use bevy_tweening::AnimCompletedEvent;

fn app(bombers: usize) -> App {
    app_with_raid(bombers, BombingRaid::None)
}

fn app_with_raid(bombers: usize, raid: BombingRaid) -> App {
    let mut rng = DeterministicRngState::from_u64(2).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    origin.colonize(1);
    let mut target = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
    target.colonize(2);
    target.army =
        Army::from([(Unit::Defense(Defense::GaussCannon), 12), (Unit::planetary_shield(), 5)]);
    if raid != BombingRaid::None {
        for unit in Unit::resource_buildings() {
            target.army.insert(unit, 5);
        }
    }
    let mission = Mission::new_with_id(
        10,
        1,
        1,
        &origin,
        &target,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Bomber), bombers)]),
        raid,
        false,
        false,
        None,
    );
    let report = resolve_combat_with_rng(1, &mission, &target, &mut rng);
    let id = report.id;
    let mut player = Player::new(1, 0);
    player.reports.push(report);
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<WorldAssets>()
        .init_resource::<Settings>()
        .init_resource::<Time>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<MultiplayerSession>()
        .insert_resource(Map {
            rect: Rect::new(-100., -100., 100., 100.),
            planets: vec![origin, target],
        })
        .insert_resource(player)
        .insert_resource(UiState {
            in_combat: Some(id),
            ..default()
        })
        .insert_resource(State::new(CombatState::Fire))
        .init_resource::<NextState<CombatState>>()
        .add_message::<SpawnShotMsg>()
        .add_message::<PlayAudioMsg>()
        .add_message::<MuteAudioMsg>()
        .add_message::<AnimCompletedEvent>();
    app.world_mut()
        .run_system_once(
            |mut assets: ResMut<WorldAssets>,
             server: Res<AssetServer>,
             mut layouts: ResMut<Assets<TextureAtlasLayout>>| {
                assets.begin_gameplay_loading(&server, &mut layouts);
            },
        )
        .unwrap();
    app.world_mut().spawn((
        MainCamera,
        Transform::default(),
        Projection::Orthographic(OrthographicProjection::default_2d()),
    ));
    app.world_mut().spawn(Window::default());
    app.world_mut().run_system_once(setup_combat).unwrap();
    app
}

fn key(app: &mut App, key: KeyCode, shift: bool) {
    let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
    keys.reset_all();
    keys.press(KeyCode::ControlLeft);
    if shift {
        keys.press(KeyCode::ShiftLeft);
    }
    keys.press(key);
}

#[test]
fn round_shortcuts_restore_saved_hull_and_destroyed_cards_even_while_paused() {
    let mut app = app(12);
    let report = app.world().resource::<Player>().reports[0].clone();
    assert!(report.combat_report.as_ref().unwrap().rounds.len() > 1);
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    key(&mut app, KeyCode::ArrowRight, true);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, 1);
    for (card, transform, home) in
        app.world_mut().query::<(&CombatUnitCmp, &Transform, &CombatCardHome)>().iter(app.world())
    {
        let rounds = &report.combat_report.as_ref().unwrap().rounds;
        if combatant(card.unit) {
            let retained_hull: usize = rounds[0]
                .units(&card.side)
                .iter()
                .filter(|record| record.unit == card.unit)
                .map(|record| record.hull)
                .sum();
            let start_count = rounds[1]
                .units(&card.side)
                .iter()
                .filter(|record| record.unit == card.unit)
                .count();
            assert_eq!((card.hull, card.shield), (retained_hull, start_count * card.unit.shield()));
        } else if card.unit == Unit::planetary_shield() {
            assert_eq!(card.shield, rounds[0].planetary_shield);
        }
        assert_eq!(transform.translation, home.0);
    }
    let attacker = app
        .world_mut()
        .query::<(Entity, &CombatUnitCmp)>()
        .iter(app.world())
        .find(|(_, card)| card.side == Side::Attacker)
        .unwrap()
        .0;
    app.world_mut().despawn(attacker);
    key(&mut app, KeyCode::ArrowLeft, true);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, 0);
    assert!(app
        .world_mut()
        .query::<&CombatUnitCmp>()
        .iter(app.world())
        .any(|card| card.side == Side::Attacker));
    assert!(app.world().resource::<Settings>().combat_paused);
    assert!(app.world_mut().query::<&PendingImpact>().iter(app.world()).next().is_none());
    assert_eq!(
        serde_json::to_value(&report).unwrap(),
        serde_json::to_value(&app.world().resource::<Player>().reports[0]).unwrap()
    );
}

#[test]
fn eliminated_army_ends_playback_without_replaying_its_remaining_shots() {
    let mut app = app(150);
    let report = app.world().resource::<Player>().reports[0].clone();
    let (source, kind, position) = app
        .world_mut()
        .query::<(Entity, &CombatUnitCmp, &Transform)>()
        .iter(app.world())
        .find(|(_, card, _)| card.side == Side::Defender && combatant(card.unit))
        .map(|(entity, card, transform)| (entity, card.unit, transform.translation))
        .unwrap();
    app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
        source: Some((source, kind, position)),
        side: Side::Attacker,
        repair: false,
        shot: crate::core::combat::resolution::ShotReport {
            unit: Some(Unit::Ship(Ship::Bomber)),
            hull_damage: 5,
            ..default()
        },
    });
    app.world_mut().run_system_once(super::super::effects::run_combat_animations).unwrap();
    assert!(app.world_mut().query::<&PendingImpact>().iter(app.world()).next().is_some());
    for mut card in app.world_mut().query::<&mut CombatUnitCmp>().iter_mut(app.world_mut()) {
        if card.side == Side::Defender && combatant(card.unit) {
            card.hull = 0;
        }
        card.fire = FireState::Idle;
    }
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::EndCombat)
    ));
    assert!(app.world().resource::<Messages<SpawnShotMsg>>().is_empty());
    assert!(app.world_mut().query::<&PendingImpact>().iter(app.world()).next().is_none());
    app.world_mut().run_system_once(super::super::systems::animate_combat).unwrap();
    assert!(app.world().resource::<Messages<SpawnShotMsg>>().is_empty());
    let last = report.combat_report.as_ref().unwrap().rounds.len() - 1;
    for card in app.world_mut().query::<&CombatUnitCmp>().iter(app.world()) {
        if combatant(card.unit) {
            let final_hull: usize = report.combat_report.as_ref().unwrap().rounds[last]
                .units(&card.side)
                .iter()
                .filter(|record| record.unit == card.unit)
                .map(|record| record.hull)
                .sum();
            assert_eq!(card.hull, final_hull);
        }
        assert!(card.fire == FireState::Fired);
    }
}

#[test]
fn finishing_ship_fire_keeps_recorded_bombing_and_applies_building_losses_once() {
    let mut app = app_with_raid(150, BombingRaid::Economic);
    let report = app.world().resource::<Player>().reports[0].clone();
    let last = report.combat_report.as_ref().unwrap().rounds.last().unwrap();
    assert!(last.attacker.iter().flat_map(|unit| &unit.shots).any(|shot| shot.is_bombing()));
    for mut card in app.world_mut().query::<&mut CombatUnitCmp>().iter_mut(app.world_mut()) {
        if card.side == Side::Defender && combatant(card.unit) {
            card.hull = 0;
        }
    }
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::Bomb)
    ));
    for mut card in app.world_mut().query::<&mut CombatUnitCmp>().iter_mut(app.world_mut()) {
        if card.unit.is_building() && card.unit != Unit::planetary_shield() {
            assert_eq!(card.hull, report.planet.army.amount(&card.unit));
        }
        if card.unit == Unit::Ship(Ship::Bomber) {
            card.fire = FireState::Firing;
        }
    }
    app.insert_resource(State::new(CombatState::Bomb));
    app.insert_resource(NextState::<CombatState>::Unchanged);
    app.world_mut().run_system_once(super::super::systems::animate_combat).unwrap();
    app.add_systems(Update, super::super::effects::run_combat_animations);
    app.update();
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(3));
    app.update();
    for card in app.world_mut().query::<&CombatUnitCmp>().iter(app.world()) {
        if card.unit.is_building() && card.unit != Unit::planetary_shield() {
            assert_eq!(card.hull, last.buildings.amount(&card.unit));
        }
    }
}

#[test]
fn ctrl_arrows_without_shift_do_not_seek_and_round_bounds_do_not_wrap() {
    let mut app = app(12);
    key(&mut app, KeyCode::ArrowRight, false);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, 0);
    key(&mut app, KeyCode::ArrowLeft, true);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, 0);
    let last =
        app.world().resource::<Player>().reports[0].combat_report.as_ref().unwrap().rounds.len()
            - 1;
    app.world_mut().resource_mut::<UiState>().combat_round = last;
    key(&mut app, KeyCode::ArrowRight, true);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, last);
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::EndCombat)
    ));
}

#[test]
fn destroyed_planet_backdrop_switches_only_under_an_opaque_flash() {
    for destroys in [false, true] {
        let mut app = app(150);
        app.add_systems(Update, super::super::effects::run_combat_animations);
        let original = app
            .world_mut()
            .query_filtered::<&Sprite, With<BackgroundImageCmp>>()
            .single(app.world())
            .unwrap()
            .image
            .clone();
        app.world_mut().spawn(super::super::effects::Cinematic::new(
            Vec3::Y * 200.,
            Vec3::ZERO,
            Vec2::new(900., 600.),
            100.,
            destroys,
        ));
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(3.6));
        app.update();
        assert_eq!(
            app.world_mut()
                .query_filtered::<&Sprite, With<BackgroundImageCmp>>()
                .single(app.world())
                .unwrap()
                .image,
            original
        );
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(0.15));
        app.update();
        let image = app
            .world_mut()
            .query_filtered::<&Sprite, With<BackgroundImageCmp>>()
            .single(app.world())
            .unwrap()
            .image
            .clone();
        if destroys {
            assert_eq!(image, app.world().resource::<WorldAssets>().image("destroyed bg"));
            let flash = app
                .world_mut()
                .query_filtered::<&Sprite, With<super::super::effects::PlanetFlash>>()
                .single(app.world())
                .unwrap();
            assert_eq!(flash.color.alpha(), 1.0);
            assert!(flash.custom_size.unwrap().cmpge(Vec2::new(900., 600.)).all());
        } else {
            assert_eq!(image, original);
            assert!(app
                .world_mut()
                .query_filtered::<Entity, With<super::super::effects::PlanetFlash>>()
                .iter(app.world())
                .next()
                .is_none());
        }
    }
}

#[test]
fn round_jump_key_release_does_not_also_change_playback_speed() {
    let mut app = App::new();
    app.init_resource::<Settings>()
        .init_resource::<ButtonInput<KeyCode>>()
        .add_systems(Update, crate::core::systems::check_keys_combat);
    let speed = app.world().resource::<Settings>().combat_speed;
    key(&mut app, KeyCode::ArrowRight, true);
    app.update();
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.release(KeyCode::ControlLeft);
        keys.release(KeyCode::ShiftLeft);
        keys.release(KeyCode::ArrowRight);
    }
    app.update();
    assert_eq!(app.world().resource::<Settings>().combat_speed, speed);
    key(&mut app, KeyCode::ArrowRight, false);
    app.update();
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.release(KeyCode::ArrowRight);
    }
    app.update();
    assert_eq!(app.world().resource::<Settings>().combat_speed, speed * 2.);
}
