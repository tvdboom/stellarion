use super::*;
use crate::core::camera::MainCamera;
use crate::core::combat::effects::PendingImpact;
use crate::core::combat::resolution::{
    resolve_combat_with_energy_with_rng, resolve_combat_with_rng,
};
use crate::core::combat::systems::CombatFormationState;
use crate::core::energy::EnergyGrid;
use crate::core::map::model::Map;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::random::DeterministicRngState;
use crate::core::units::{defense::Defense, ships::Ship, Army, Combat};
use crate::multiplayer::client::MultiplayerSession;
use bevy_tweening::{AnimCompletedEvent, AnimTargetKind};

#[test]
fn snapshot_matches_hull_by_id_across_casualties_and_reordered_rounds() {
    use crate::core::combat::report::{CombatReport, RoundReport};
    use crate::core::combat::resolution::CombatUnit;

    let unit = Unit::Ship(Ship::LightFighter);
    let record = |id, hull, shield| CombatUnit {
        fleet_id: 0,
        battle_plan: Default::default(),
        id,
        owner: Some(1),
        unit,
        hull,
        shield,
        repairs: Vec::new(),
        shots: Vec::new(),
    };
    let mut report = crate::test_support::empty_report(
        Mission::default(),
        Planet::new(1, "Target".into(), Vec2::ZERO, false, 1.0),
    );
    report.combat_report = Some(CombatReport {
        rounds: vec![
            RoundReport {
                attacker: vec![record(31, 12, 3), record(8, 0, 0), record(72, 25, 6)],
                ..default()
            },
            RoundReport {
                attacker: vec![record(72, 18, 2), record(31, 0, 0)],
                ..default()
            },
        ],
        ..default()
    });
    let before = snapshot_card(&report, 1, false, unit, Side::Attacker).unwrap();
    assert_eq!((before.hull, before.shield), (37, 2 * unit.shield()));
    let after = snapshot_card(&report, 1, true, unit, Side::Attacker).unwrap();
    assert_eq!((after.hull, after.shield), (18, 2));
}

fn app(bombers: usize) -> App {
    app_with_raid(bombers, BombingRaid::None)
}

fn app_with_raid(bombers: usize, raid: BombingRaid) -> App {
    app_with_army(Army::from([(Unit::Ship(Ship::Bomber), bombers)]), raid)
}

fn app_with_army(army: Army, raid: BombingRaid) -> App {
    let mut rng = DeterministicRngState::from_u64(2).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    origin.colonize(1);
    let mut target = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
    target.colonize(2);
    target.army =
        Army::from([(Unit::Defense(Defense::GaussCannon), 12), (Unit::planetary_shield(), 5)])
            .into();
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
        army,
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
            solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
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
fn playback_uses_the_recorded_energy_scaled_planetary_shield_strength() {
    let mut rng = DeterministicRngState::from_u64(29).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    origin.colonize(1);
    let mut target = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
    target.colonize(2);
    target.army =
        Army::from([(Unit::planetary_shield(), 5), (Unit::Defense(Defense::GaussCannon), 12)])
            .into();
    let mission = Mission::new_with_id(
        29,
        1,
        1,
        &origin,
        &target,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::WarSun), 2)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let report = resolve_combat_with_energy_with_rng(
        1,
        &mission,
        &target,
        EnergyGrid {
            supply: 0,
            demand: 10,
        },
        &mut rng,
    );

    // Five levels normally display 1,500 strength. This defender's 30%-efficient grid gave the
    // resolver 450, and playback must not invent the other 1,050 as an indestructible remainder.
    assert_eq!(report.initial_planetary_shield(), 450);
    let card = snapshot_card(&report, 0, false, Unit::planetary_shield(), Side::Defender).unwrap();
    assert_eq!((card.shield, card.max_shield), (450, 450));
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
fn eliminated_army_finishes_its_wrecks_before_concluding_playback() {
    let mut app = app(2);
    for mut card in app.world_mut().query::<&mut CombatUnitCmp>().iter_mut(app.world_mut()) {
        if card.side == Side::Attacker && combatant(card.unit) {
            card.hull = 0;
        }
        card.fire = FireState::Fired;
    }

    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));

    app.world_mut().run_system_once(super::super::systems::animate_combat).unwrap();
    assert!(app
        .world_mut()
        .query::<&super::super::effects::Wreck>()
        .iter(app.world())
        .next()
        .is_some());

    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));
}

#[test]
fn automatic_individual_conclusion_waits_for_wrecks_and_preserves_survivor_layout() {
    use super::super::systems::{HullCmp, ShieldCmp};

    #[derive(Debug, PartialEq)]
    struct CardGeometry {
        home: Vec3,
        display_size: f32,
        image_size: Option<Vec2>,
        children: Vec<(Transform, Option<Vec2>)>,
    }

    let geometry = |app: &mut App| {
        let mut children_query = app.world_mut().query::<&Children>();
        let mut cards = app.world_mut().query::<(Entity, &IndividualCombatUnitCmp, &Sprite)>();
        let children = children_query.query(app.world());
        cards
            .iter(app.world())
            .filter_map(|(entity, card, sprite)| {
                card.id.map(|id| {
                    (
                        (card.side == Side::Defender, id),
                        CardGeometry {
                            home: card.home(),
                            display_size: card.display_size(),
                            image_size: sprite.custom_size,
                            children: children
                                .iter_descendants(entity)
                                // Filled lengths reflect damage; tracks and borders retain their geometry.
                                .filter(|child| {
                                    app.world().get::<ShieldCmp>(*child).is_none()
                                        && app.world().get::<HullCmp>(*child).is_none()
                                })
                                .filter_map(|child| {
                                    Some((
                                        *app.world().get::<Transform>(child)?,
                                        app.world().get::<Sprite>(child)?.custom_size,
                                    ))
                                })
                                .collect(),
                        },
                    )
                })
            })
            .collect::<HashMap<_, _>>()
    };

    for (army, defeated_side) in [
        (
            Army::from([
                (Unit::Ship(Ship::LightFighter), 60),
                (Unit::Ship(Ship::Bomber), 12),
                (Unit::war_sun(), 2),
            ]),
            Side::Defender,
        ),
        (Army::from([(Unit::Ship(Ship::Bomber), 2)]), Side::Attacker),
    ] {
        for individual in [true, false] {
            let mut app = app_with_army(army.clone(), BombingRaid::None);
            // Exercise actual viewport dimensions, including a smaller window.
            let viewport = if individual {
                Vec2::new(1280.0, 720.0)
            } else {
                Vec2::new(800.0, 600.0)
            };
            for mut projection in
                app.world_mut().query::<&mut Projection>().iter_mut(app.world_mut())
            {
                let Projection::Orthographic(projection) = &mut *projection else {
                    unreachable!();
                };
                projection.area = Rect::from_center_size(Vec2::ZERO, viewport);
            }
            let report = app.world().resource::<Player>().reports[0].clone();
            let rounds = &report.combat_report.as_ref().unwrap().rounds;
            if defeated_side == Side::Defender {
                assert!(rounds.len() > 1);
                assert!(report.surviving_attacker.has_army());
                assert!(
                    rounds.last().unwrap().attacker.len() < rounds[0].attacker.len(),
                    "the victory fixture must lose ships before the final round's layout is fitted"
                );
            }
            seek(app.world_mut(), &report, 0, false, CombatState::DisplayRound, None);
            app.insert_resource(NextState::<CombatState>::Unchanged);
            app.world_mut().resource_mut::<Settings>().combat_individual_units = individual;
            app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(1));
            app.world_mut()
                .run_system_once(super::super::systems::update_combat_formation)
                .unwrap();
            assert_eq!(app.world().resource::<CombatFormationState>().individual(), individual);
            let expected = geometry(&mut app);
            assert!(!expected.is_empty());

            let defeated = app
                .world_mut()
                .query::<(Entity, &CombatUnitCmp)>()
                .iter(app.world())
                .filter_map(|(entity, card)| {
                    (card.side == defeated_side && combatant(card.unit)).then_some(entity)
                })
                .collect::<Vec<_>>();
            for entity in defeated {
                app.world_mut().despawn(entity);
            }

            let wreck = app
                .world_mut()
                .spawn(super::super::effects::Wreck::new(Vec3::ZERO, 40.0, Unit::crawler()))
                .id();
            app.world_mut().run_system_once(control_combat_playback).unwrap();
            assert!(matches!(
                *app.world().resource::<NextState<CombatState>>(),
                NextState::Unchanged
            ));
            app.world_mut().despawn(wreck);

            app.world_mut().run_system_once(control_combat_playback).unwrap();
            assert!(matches!(
                *app.world().resource::<NextState<CombatState>>(),
                NextState::Pending(CombatState::EndCombat)
            ));
            assert!(!app.world().contains_resource::<CombatLayoutSnapshot>());
            let survivors = geometry(&mut app);
            assert!(!survivors.is_empty());
            assert!(survivors.len() < expected.len(), "fixture must remove casualties");
            for (id, layout) in &survivors {
                assert_eq!(layout, &expected[id]);
            }

            // A later combine/split must return to the same slots rather than the rebuilt fit.
            app.insert_resource(State::new(CombatState::EndCombat));
            for individual in [!individual, individual, true] {
                app.world_mut().resource_mut::<Settings>().combat_individual_units = individual;
                app.world_mut()
                    .run_system_once(super::super::systems::update_combat_formation)
                    .unwrap();
            }
            assert_eq!(geometry(&mut app), survivors);
            for (card, transform) in
                app.world_mut().query::<(&IndividualCombatUnitCmp, &Transform)>().iter(app.world())
            {
                assert_eq!(transform.translation, card.home());
                assert_eq!(transform.scale, Vec3::ONE);
            }
        }
    }
}

#[test]
fn automatic_conclusion_keeps_damaged_bars_at_their_recorded_values() {
    use super::super::systems::{update_combat_stats, HullCmp, ShieldCmp};

    let bars = |app: &mut App| {
        let mut cards =
            app.world_mut()
                .query::<(Entity, Option<&CombatUnitCmp>, Option<&IndividualCombatUnitCmp>)>();
        let mut children_query = app.world_mut().query::<&Children>();
        let world = app.world();
        let children = children_query.query(world);
        cards
            .iter(world)
            .filter_map(|(entity, group, individual)| {
                let (side, unit, id) = if let Some(card) = group {
                    (&card.side, card.unit, None)
                } else {
                    let card = individual?;
                    (&card.side, card.unit, card.id)
                };
                (!unit.is_building()).then_some((entity, side, unit, id))
            })
            .flat_map(|(entity, side, unit, id)| {
                children.iter_descendants(entity).filter_map(move |child| {
                    let shield = world.get::<ShieldCmp>(child).is_some();
                    if !shield && world.get::<HullCmp>(child).is_none() {
                        return None;
                    }
                    Some((
                        (side == &Side::Defender, unit, id, shield),
                        (
                            world.get::<Sprite>(child)?.custom_size?.x,
                            world.get::<Transform>(child)?.translation.x,
                        ),
                    ))
                })
            })
            .collect::<HashMap<_, _>>()
    };

    for bombers in [150, 2] {
        let mut app = app(bombers);
        let mut report = app.world().resource::<Player>().reports[0].clone();
        let rounds = &mut report.combat_report.as_mut().unwrap().rounds;
        let last = rounds.len() - 1;
        let round = &mut rounds[last];
        // Give every survivor partial hull and shields, so both bar types expose a reset.
        for record in round.attacker.iter_mut().chain(&mut round.defender) {
            if record.hull > 0 {
                record.hull = (record.unit.hull() / 2).max(1);
                record.shield = record.unit.shield() / 3;
            }
        }
        app.world_mut().resource_mut::<Player>().reports[0] = report.clone();
        seek(app.world_mut(), &report, last, true, CombatState::EndCombat, None);
        app.add_systems(Update, update_combat_stats);
        app.insert_resource(State::new(CombatState::Fire));
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs(1));
        app.update();
        let expected = bars(&mut app);
        assert!(expected.keys().any(|(_, _, id, shield)| id.is_some() && *shield));
        assert!(expected.values().all(|(_, x)| *x < 0.0), "all bars must be depleted");

        app.insert_resource(NextState::<CombatState>::Unchanged);
        app.world_mut().run_system_once(control_combat_playback).unwrap();
        assert!(matches!(
            *app.world().resource::<NextState<CombatState>>(),
            NextState::Pending(CombatState::EndCombat)
        ));
        app.insert_resource(State::new(CombatState::EndCombat));
        app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_millis(16));
        for _ in 0..3 {
            app.update();
            assert_eq!(bars(&mut app), expected, "result bars must never refill after rebuilding");
        }
    }
}

#[test]
fn finishing_ship_fire_keeps_recorded_bombing_and_applies_building_losses_once() {
    let mut app = app_with_raid(150, BombingRaid::Economic);
    let report = app.world().resource::<Player>().reports[0].clone();
    let last = report.combat_report.as_ref().unwrap().rounds.last().unwrap();
    assert!(last.attacker.iter().flat_map(|unit| &unit.shots).any(|shot| shot.is_bombing()));
    let defeated = app
        .world_mut()
        .query::<(Entity, &CombatUnitCmp)>()
        .iter(app.world())
        .filter_map(|(entity, card)| {
            (card.side == Side::Defender && combatant(card.unit)).then_some(entity)
        })
        .collect::<Vec<_>>();
    for entity in defeated {
        app.world_mut().despawn(entity);
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
fn ctrl_arrows_without_shift_do_not_seek_and_round_bounds_restart_or_finish() {
    let mut app = app(12);
    key(&mut app, KeyCode::ArrowRight, false);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, 0);
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));
    key(&mut app, KeyCode::ArrowLeft, true);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, 0);
    assert!(app.world().contains_resource::<CombatRoundJump>());
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::Setup)
    ));
    let last =
        app.world().resource::<Player>().reports[0].combat_report.as_ref().unwrap().rounds.len()
            - 1;
    app.world_mut().resource_mut::<UiState>().combat_round = last;
    app.world_mut().resource_mut::<Settings>().combat_individual_units = true;
    key(&mut app, KeyCode::ArrowRight, true);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, last);
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::EndCombat)
    ));
    let individuals = app
        .world_mut()
        .query_filtered::<Entity, With<IndividualCombatUnitCmp>>()
        .iter(app.world())
        .collect::<Vec<_>>();
    assert!(!individuals.is_empty());
    assert!(individuals.iter().all(|entity| app.world().get::<TweenAnim>(*entity).is_none()));
}

#[test]
fn rewinding_round_one_finishes_the_restored_fly_in_before_weapon_selection() {
    let mut app = app(12);
    app.world_mut().resource_mut::<Settings>().combat_individual_units = true;
    key(&mut app, KeyCode::ArrowLeft, true);
    app.world_mut().run_system_once(control_combat_playback).unwrap();

    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::Setup)
    ));
    assert!(app
        .world_mut()
        .query::<&CombatUnitCmp>()
        .iter(app.world())
        .all(|card| card.fire == FireState::Idle));
    assert!(app
        .world_mut()
        .query_filtered::<&TweenAnim, With<IndividualCombatUnitCmp>>()
        .iter(app.world())
        .next()
        .is_some());

    app.insert_resource(State::new(CombatState::Setup));
    app.insert_resource(NextState::<CombatState>::Unchanged);
    app.world_mut().run_system_once(super::super::systems::animate_combat).unwrap();
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));
    assert!(app.world().resource::<Messages<SpawnShotMsg>>().is_empty());
}

#[test]
fn backward_shortcut_restarts_a_started_round_before_moving_to_the_previous_round() {
    let mut app = app(12);
    let last =
        app.world().resource::<Player>().reports[0].combat_report.as_ref().unwrap().rounds.len()
            - 1;
    assert!(last > 0);
    app.world_mut().resource_mut::<UiState>().combat_round = last;
    app.insert_resource(State::new(CombatState::EndCombat));

    key(&mut app, KeyCode::ArrowLeft, true);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, last);
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::DisplayRound)
    ));

    app.insert_resource(State::new(CombatState::DisplayRound));
    app.insert_resource(NextState::<CombatState>::Unchanged);
    key(&mut app, KeyCode::ArrowLeft, true);
    app.world_mut().run_system_once(control_combat_playback).unwrap();
    assert_eq!(app.world().resource::<UiState>().combat_round, last - 1);
}

#[test]
fn rewinding_a_fully_intercepted_missile_strike_replays_the_interceptors() {
    let mut rng = DeterministicRngState::from_u64(17).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    origin.colonize(1);
    let mut target = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
    target.colonize(2);
    target.army =
        Army::from([(Unit::antiballistic_missile(), 16), (Unit::Defense(Defense::GaussCannon), 1)])
            .into();
    let mission = Mission::new_with_id(
        11,
        1,
        1,
        &origin,
        &target,
        Icon::MissileStrike,
        Army::from([(Unit::interplanetary_missile(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let report = resolve_combat_with_rng(1, &mission, &target, &mut rng);
    let round = &report.combat_report.as_ref().unwrap().rounds[0];
    assert_eq!(round.missiles_shot(), round.n_missiles());

    let mut app = app(1);
    app.world_mut().resource_mut::<Player>().reports = vec![report.clone()];
    app.world_mut().resource_mut::<UiState>().in_combat = Some(report.id);
    app.insert_resource(State::new(CombatState::EndCombat));
    app.insert_resource(NextState::<CombatState>::Unchanged);
    key(&mut app, KeyCode::ArrowLeft, true);

    app.world_mut().run_system_once(control_combat_playback).unwrap();

    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::Setup)
    ));
    let cards = app
        .world_mut()
        .query::<&CombatUnitCmp>()
        .iter(app.world())
        .map(|card| (card.unit, card.fire == FireState::Idle))
        .collect::<Vec<_>>();
    assert!(cards.contains(&(Unit::antiballistic_missile(), true)), "rewound cards: {cards:?}");

    let entry = app
        .world_mut()
        .query_filtered::<Entity, With<TweenAnim>>()
        .iter(app.world())
        .next()
        .unwrap();
    app.insert_resource(State::new(CombatState::Setup));
    app.insert_resource(NextState::<CombatState>::Unchanged);
    app.world_mut().resource_mut::<Messages<AnimCompletedEvent>>().write(AnimCompletedEvent {
        anim_entity: entry,
        target: AnimTargetKind::Component {
            entity: entry,
        },
    });
    app.world_mut().run_system_once(super::super::systems::animate_combat).unwrap();
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::AntiBallistic)
    ));
    assert!(app
        .world_mut()
        .query::<&CombatUnitCmp>()
        .iter(app.world())
        .any(|card| card.unit == Unit::antiballistic_missile() && card.fire == FireState::Select));
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
        .insert_resource(State::new(CombatState::Fire))
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

#[test]
fn final_result_cannot_be_paused() {
    let mut app = App::new();
    app.init_resource::<Settings>()
        .init_resource::<ButtonInput<KeyCode>>()
        .insert_resource(State::new(CombatState::EndCombat))
        .add_systems(Update, crate::core::systems::check_keys_combat);
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    key(&mut app, KeyCode::Space, false);

    app.update();

    assert!(!app.world().resource::<Settings>().combat_paused);
}
