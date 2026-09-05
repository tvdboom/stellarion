//! Headless playback checks run the real Bevy combat systems against resolver reports.

use bevy::ecs::system::RunSystemOnce;
use bevy_tweening::CycleCompletedEvent;

use super::*;
use crate::core::combat::report::MissionReport;
use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::map::planet::Planet;
use crate::core::missions::Mission;
use crate::core::random::DeterministicRngState;
use crate::core::units::defense::Defense;
use crate::core::units::Army;

fn report(bombers: usize, shield: usize, guarded: bool, seed: u64) -> MissionReport {
    let mut rng = DeterministicRngState::from_u64(seed).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    origin.colonize(1);
    let mut target = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
    target.colonize(2);
    target.army = Unit::resource_buildings().into_iter().map(|u| (u, 5)).collect();
    if shield > 0 {
        target.army.insert(Unit::planetary_shield(), shield);
    }
    if guarded {
        target.army.insert(Unit::Defense(Defense::GaussCannon), 12);
    }
    let mission = Mission::new_with_id(
        10,
        1,
        1,
        &origin,
        &target,
        Icon::Attack,
        Army::from([(Unit::Ship(Ship::Bomber), bombers)]),
        BombingRaid::Economic,
        false,
        false,
        None,
    );
    resolve_combat_with_rng(1, &mission, &target, &mut rng)
}

fn playback_app(report: MissionReport, round: usize, phase: CombatState) -> App {
    let report_id = report.id;
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
        .insert_resource(player)
        .insert_resource(UiState {
            in_combat: Some(report_id),
            combat_round: round,
            ..default()
        })
        .insert_resource(State::new(phase))
        .init_resource::<NextState<CombatState>>()
        .add_message::<SpawnShotMsg>()
        .add_message::<PlayAudioMsg>()
        .add_message::<AnimCompletedEvent>()
        .add_message::<CycleCompletedEvent>();
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
    app.world_mut().spawn((BackgroundImageCmp, Sprite::default()));
    app.world_mut().spawn(Window::default());
    app
}

fn spawn_unit(app: &mut App, unit: Unit, count: usize, side: Side, fire: FireState) -> Entity {
    let hull = if unit.is_building() {
        count
    } else {
        count * unit.hull()
    };
    app.world_mut()
        .spawn((
            Sprite {
                custom_size: Some(Vec2::splat(100.)),
                ..default()
            },
            Transform::default(),
            CombatUnitCmp {
                unit,
                side,
                fire,
                hull,
                max_hull: hull,
                shield: count * unit.shield(),
                max_shield: count * unit.shield(),
            },
        ))
        .id()
}

#[test]
fn combat_cards_only_spawn_bars_for_stats_the_unit_has() {
    let mut report = report(1, 0, true, 7);
    report.mission.objective = Icon::MissileStrike;
    report.mission.bombing = BombingRaid::None;
    report.mission.army = Army::from([(Unit::interplanetary_missile(), 4)]);
    report.planet.army.insert(Unit::antiballistic_missile(), 3);
    report.planet.army.insert(Unit::crawler(), 1);
    let mut rng = DeterministicRngState::from_u64(7).next_rng();
    let report = resolve_combat_with_rng(1, &report.mission, &report.planet, &mut rng);
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::AntiBallistic);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    app.world_mut().run_system_once(setup_combat).unwrap();

    let cards = app
        .world_mut()
        .query::<(Entity, &CombatUnitCmp)>()
        .iter(app.world())
        .map(|(entity, card)| (entity, card.unit))
        .collect::<Vec<_>>();
    for expected in [Unit::antiballistic_missile(), Unit::interplanetary_missile(), Unit::crawler()]
    {
        assert!(cards.iter().any(|(_, unit)| *unit == expected));
    }
    for (entity, unit) in cards {
        let descendants = app
            .world_mut()
            .run_system_once(move |children: Query<&Children>| {
                children.iter_descendants(entity).collect::<Vec<_>>()
            })
            .unwrap();
        let hull_bars = descendants
            .iter()
            .filter(|&&child| app.world().get::<HullCmp>(child).is_some())
            .count();
        let shield_bars = descendants
            .iter()
            .filter(|&&child| app.world().get::<ShieldCmp>(child).is_some())
            .count();
        assert_eq!(hull_bars, usize::from(unit.hull() > 0), "{unit:?} hull bars");
        assert_eq!(shield_bars, usize::from(unit.shield() > 0), "{unit:?} shield bars");
    }
}

#[test]
fn single_round_battles_go_directly_to_fire_without_a_round_banner() {
    let report = report(150, 0, true, 7);
    assert_eq!(report.combat_report.as_ref().unwrap().rounds.len(), 1);
    let mut app = playback_app(report, 0, CombatState::DisplayRound);
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::Fire)
    ));
    assert!(app
        .world_mut()
        .query_filtered::<Entity, With<DisplayTextCmp>>()
        .iter(app.world())
        .next()
        .is_none());
}

#[test]
fn death_ray_playback_completes_for_successful_and_failed_destroy_missions() {
    for destroys in [false, true] {
        let report = (0..128)
            .map(|seed| {
                let mut rng = DeterministicRngState::from_u64(seed).next_rng();
                let mut origin =
                    Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
                origin.colonize(1);
                let mut target =
                    Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
                target.colonize(2);
                let mission = Mission::new_with_id(
                    10,
                    1,
                    1,
                    &origin,
                    &target,
                    Icon::Destroy,
                    Army::from([(Unit::war_sun(), 1)]),
                    BombingRaid::None,
                    false,
                    false,
                    None,
                );
                resolve_combat_with_rng(1, &mission, &target, &mut rng)
            })
            .find(|report| report.planet_destroyed == destroys)
            .expect("seeds must cover both destruction outcomes");
        let combat = report.combat_report.as_ref().unwrap();
        let round = combat.rounds.len() - 1;
        assert!(combat.rounds[round].destroy_probability > 0.);
        let mut app = playback_app(report, round, CombatState::Fire);
        app.add_plugins(bevy_tweening::TweeningPlugin);
        let sun = spawn_unit(&mut app, Unit::war_sun(), 1, Side::Attacker, FireState::Fired);

        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(matches!(
            *app.world().resource::<NextState<CombatState>>(),
            NextState::Pending(CombatState::DeathRay)
        ));
        app.insert_resource(State::new(CombatState::DeathRay));
        app.insert_resource(NextState::<CombatState>::Unchanged);
        app.world_mut().get_mut::<CombatUnitCmp>(sun).unwrap().fire = FireState::Firing;
        app.world_mut().run_system_once(animate_combat).unwrap();
        let ray = app
            .world_mut()
            .query_filtered::<Entity, With<DeathRayCmp>>()
            .single(app.world())
            .unwrap();
        assert!(app.world().get::<Cinematic>(ray).is_some());

        // Exercise the actual tween target and completion message, not a synthetic event.
        TweenAnim::step_all(app.world_mut(), Duration::from_secs_f32(DEATH_RAY_DURATION - 0.2));
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get::<CombatUnitCmp>(sun).unwrap().fire == FireState::Firing);
        assert!(app.world().get_entity(ray).is_ok());
        TweenAnim::step_all(app.world_mut(), Duration::from_millis(250));
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get_entity(ray).is_err());
        assert!(app.world().get::<CombatUnitCmp>(sun).unwrap().fire == FireState::Deselect);
        let destroyed_background = app.world().resource::<WorldAssets>().image("destroyed bg");
        let background = app
            .world_mut()
            .query_filtered::<&Sprite, With<BackgroundImageCmp>>()
            .single(app.world())
            .unwrap();
        assert_eq!(background.image == destroyed_background, destroys);

        app.world_mut().get_mut::<CombatUnitCmp>(sun).unwrap().fire = FireState::Fired;
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(matches!(
            *app.world().resource::<NextState<CombatState>>(),
            NextState::Pending(CombatState::EndCombat)
        ));
    }
}

#[test]
fn bombing_animation_runs_only_for_the_recorded_raid_round() {
    let report = report(12, 5, true, 2);
    let combat = report.combat_report.as_ref().unwrap();
    assert!(combat.rounds.len() > 2);
    let mut raid_rounds = 0;
    for (index, round) in combat.rounds.iter().enumerate() {
        let has_raid = round.attacker.iter().flat_map(|cu| &cu.shots).any(ShotReport::is_bombing);
        raid_rounds += usize::from(has_raid);
        for phase in [CombatState::Fire, CombatState::Repair] {
            let mut app = playback_app(report.clone(), index, phase);
            spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 12, Side::Attacker, FireState::Fired);
            app.world_mut().run_system_once(animate_combat).unwrap();
            assert_eq!(
                matches!(
                    *app.world().resource::<NextState<CombatState>>(),
                    NextState::Pending(CombatState::Bomb)
                ),
                has_raid,
                "wrong raid transition in round {index}",
            );
        }
    }
    assert_eq!(raid_rounds, 1);
}

#[test]
fn bombing_animation_includes_misses_and_emits_each_recorded_attempt_once() {
    for count in [1, 30, 150] {
        let report = report(count, 0, false, 4);
        let expected = report.combat_report.as_ref().unwrap().rounds[0]
            .attacker
            .iter()
            .flat_map(|cu| &cu.shots)
            .filter(|shot| shot.is_bombing())
            .cloned()
            .collect::<Vec<_>>();
        assert!(!expected.is_empty());
        assert!(expected.iter().any(|shot| shot.missed));
        let mut app = playback_app(report, 0, CombatState::Fire);
        let bomber =
            spawn_unit(&mut app, Unit::Ship(Ship::Bomber), count, Side::Attacker, FireState::Fired);
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(matches!(
            *app.world().resource::<NextState<CombatState>>(),
            NextState::Pending(CombatState::Bomb)
        ));

        app.world_mut().insert_resource(State::new(CombatState::Bomb));
        app.world_mut().get_mut::<CombatUnitCmp>(bomber).unwrap().fire = FireState::Firing;
        let mut cursor = app.world().resource::<Messages<SpawnShotMsg>>().get_cursor();
        app.world_mut().run_system_once(animate_combat).unwrap();
        let messages = app.world().resource::<Messages<SpawnShotMsg>>();
        let actual = cursor
            .read(messages)
            .map(|message| {
                assert_eq!(message.side, Side::Defender);
                assert!(!message.repair);
                message.shot.clone()
            })
            .collect::<Vec<_>>();
        assert_eq!(serde_json::to_value(&actual).unwrap(), serde_json::to_value(expected).unwrap());
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert_eq!(cursor.read(app.world().resource::<Messages<SpawnShotMsg>>()).count(), 0);
    }
}

#[test]
fn weapon_animation_keeps_shield_fire_separate_from_building_raids() {
    // An unguarded shield still absorbs weapon fire even though no defender ship exists.
    for count in [1, 30] {
        let report = report(count, 5, false, 7);
        let expected = report.combat_report.as_ref().unwrap().rounds[0]
            .attacker
            .iter()
            .flat_map(|cu| &cu.shots)
            .filter(|shot| !shot.is_bombing())
            .cloned()
            .collect::<Vec<_>>();
        assert!(expected.iter().any(|shot| shot.planetary_shield_damage > 0));
        let mut app = playback_app(report, 0, CombatState::Fire);
        let bomber =
            spawn_unit(&mut app, Unit::Ship(Ship::Bomber), count, Side::Attacker, FireState::Idle);
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Select);
        app.world_mut().get_mut::<CombatUnitCmp>(bomber).unwrap().fire = FireState::Firing;
        let mut cursor = app.world().resource::<Messages<SpawnShotMsg>>().get_cursor();
        app.world_mut().run_system_once(animate_combat).unwrap();
        let actual = cursor
            .read(app.world().resource::<Messages<SpawnShotMsg>>())
            .map(|message| message.shot.clone())
            .collect::<Vec<_>>();
        assert_eq!(serde_json::to_value(actual).unwrap(), serde_json::to_value(expected).unwrap());
    }
}

#[test]
fn weapon_animation_replays_simultaneous_return_fire_from_destroyed_defenders() {
    let report = report(150, 0, true, 7);
    let defenders = &report.combat_report.as_ref().unwrap().rounds[0].defender;
    assert!(defenders.iter().all(|cu| cu.hull == 0));
    let expected = defenders.iter().flat_map(|cu| &cu.shots).cloned().collect::<Vec<_>>();
    assert!(expected.iter().any(|shot| shot.hull_damage > 0));
    let mut app = playback_app(report, 0, CombatState::Fire);
    spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 150, Side::Attacker, FireState::Fired);
    let gauss = spawn_unit(
        &mut app,
        Unit::Defense(Defense::GaussCannon),
        0,
        Side::Defender,
        FireState::Idle,
    );
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(gauss).unwrap().fire == FireState::Select);
    app.world_mut().get_mut::<CombatUnitCmp>(gauss).unwrap().fire = FireState::Firing;
    let mut cursor = app.world().resource::<Messages<SpawnShotMsg>>().get_cursor();
    app.world_mut().run_system_once(animate_combat).unwrap();
    let actual = cursor
        .read(app.world().resource::<Messages<SpawnShotMsg>>())
        .map(|message| message.shot.clone())
        .collect::<Vec<_>>();
    assert_eq!(serde_json::to_value(actual).unwrap(), serde_json::to_value(expected).unwrap());
}

#[test]
fn bombing_explosions_apply_exactly_the_reported_building_losses() {
    let report = report(150, 0, false, 8);
    let shots = report.combat_report.as_ref().unwrap().rounds[0]
        .attacker
        .iter()
        .flat_map(|cu| &cu.shots)
        .filter(|shot| shot.is_bombing())
        .cloned()
        .collect::<Vec<_>>();
    let mut app = playback_app(report.clone(), 0, CombatState::Bomb);
    let mut entities = std::collections::BTreeMap::new();
    for unit in Unit::resource_buildings() {
        entities.insert(unit, spawn_unit(&mut app, unit, 5, Side::Defender, FireState::Fired));
    }
    for shot in &shots {
        app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
            shot: shot.clone(),
            repair: false,
            side: Side::Defender,
            source: None,
        });
    }
    // Keep the system's message cursor across frames, so shots are spawned only once.
    app.add_systems(Update, run_combat_animations);
    app.update();
    for _ in 0..100 {
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(50));
        app.update();
    }
    for (unit, entity) in entities {
        let shown = app.world().get::<CombatUnitCmp>(entity).unwrap().hull;
        assert_eq!(shown, report.surviving_defender.amount(&unit));
        assert!(shown >= 2, "animation exceeded the three-level loss cap");
    }
    assert_eq!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count(), 0);
}

#[test]
fn paused_playback_does_not_start_a_new_volley() {
    let mut app = playback_app(report(30, 0, false, 4), 0, CombatState::Bomb);
    spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 30, Side::Attacker, FireState::Firing);
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().resource::<Messages<SpawnShotMsg>>().is_empty());
    app.world_mut().resource_mut::<Settings>().combat_paused = false;
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(!app.world().resource::<Messages<SpawnShotMsg>>().is_empty());
}

#[test]
fn round_waits_for_travelling_damage_before_finishing() {
    let mut app = playback_app(report(30, 0, false, 4), 0, CombatState::Bomb);
    let bomber =
        spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 30, Side::Attacker, FireState::Fired);
    let building = Unit::resource_buildings()[0];
    spawn_unit(&mut app, building, 5, Side::Defender, FireState::Fired);
    app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
        shot: ShotReport {
            unit: Some(building),
            killed: true,
            ..default()
        },
        repair: false,
        side: Side::Defender,
        source: Some((bomber, Unit::Ship(Ship::Bomber), Vec3::Y * 200.)),
    });
    app.world_mut().run_system_once(run_combat_animations).unwrap();
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));
}

#[test]
fn crawler_replays_real_resolver_repairs_and_restores_exact_recorded_hull() {
    // Find a deterministic battle with surviving, damaged turrets. Missing shields
    // or a reduced unit count alone are not eligible for crawler repairs.
    let mut selected = None;
    for seed in 0..40 {
        let mut rng = DeterministicRngState::from_u64(seed).next_rng();
        let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
        origin.colonize(1);
        let mut target = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
        target.colonize(2);
        target.army = Army::from([
            (Unit::crawler(), 8),
            (Unit::Defense(Defense::GaussCannon), 8),
            (Unit::Defense(Defense::PlasmaTurret), 3),
        ]);
        let mission = Mission::new_with_id(
            10,
            1,
            1,
            &origin,
            &target,
            Icon::Attack,
            Army::from([(Unit::Ship(Ship::Cruiser), 14)]),
            BombingRaid::None,
            false,
            false,
            None,
        );
        let report = resolve_combat_with_rng(1, &mission, &target, &mut rng);
        if let Some(index) = report
            .combat_report
            .as_ref()
            .unwrap()
            .rounds
            .iter()
            .position(|round| round.defender.iter().any(|unit| !unit.repairs.is_empty()))
        {
            selected = Some((report, index));
            break;
        }
    }
    let (report, index) = selected.expect("fixture must exercise real repair events");
    let round = report.combat_report.as_ref().unwrap().rounds[index].clone();
    let mut app = playback_app(report, index, CombatState::Fire);
    let mut targets = Vec::new();
    for kind in Unit::defenses() {
        let survivors =
            round.defender.iter().filter(|u| u.unit == kind && u.hull > 0).collect::<Vec<_>>();
        if survivors.is_empty() {
            continue;
        }
        let final_hull: usize = survivors.iter().map(|u| u.hull).sum();
        let repaired: usize = survivors.iter().flat_map(|u| &u.repairs).sum();
        let entity = spawn_unit(&mut app, kind, survivors.len(), Side::Defender, FireState::Fired);
        app.world_mut().get_mut::<CombatUnitCmp>(entity).unwrap().hull = final_hull - repaired;
        targets.push((kind, entity, final_hull, repaired));
    }
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::Repair)
    ));
    let crawler = targets.iter().find(|(kind, _, _, _)| *kind == Unit::crawler()).unwrap().1;
    app.world_mut().insert_resource(State::new(CombatState::Repair));
    app.world_mut().get_mut::<CombatUnitCmp>(crawler).unwrap().fire = FireState::Firing;
    app.world_mut().run_system_once(animate_combat).unwrap();
    let messages = app.world().resource::<Messages<SpawnShotMsg>>();
    let mut cursor = messages.get_cursor();
    let messages = cursor.read(messages).collect::<Vec<_>>();
    assert!(!messages.is_empty());
    assert!(messages.iter().all(|m| m.repair && m.source.unwrap().0 == crawler));
    app.add_systems(Update, run_combat_animations);
    app.update();
    assert!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count() > 0);
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs(3));
    app.update();
    for (_, entity, final_hull, _) in targets {
        assert_eq!(app.world().get::<CombatUnitCmp>(entity).unwrap().hull, final_hull);
    }
    assert!(
        app.world_mut()
            .query::<&crate::core::combat::effects::CombatReadout>()
            .iter(app.world())
            .count()
            > 0
    );
}
