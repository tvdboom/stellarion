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
    let hull = if unit.is_building() || unit == Unit::colony_ship() {
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
fn colonial_withdrawal_hides_colony_ships_and_flies_combat_ships_to_an_upper_corner() {
    use crate::core::combat::resolution::resolve_combat_with_retreat_with_rng;
    use crate::core::energy::EnergyGrid;
    use crate::core::units::buildings::{Building, FleetWithdrawal};
    for level in [4, 5] {
        let mut source = report(1, 0, false, 17);
        source.mission.bombing = BombingRaid::None;
        source.mission.army = Army::from([(Unit::Ship(Ship::LightFighter), 1)]);
        source.planet.army = Army::from([
            (Unit::Building(Building::ColonialAdministration), level),
            (Unit::Ship(Ship::Dreadnought), 2),
            (Unit::colony_ship(), 1),
            (Unit::Defense(Defense::GaussCannon), 3),
        ])
        .into();
        source.planet.fleet_withdrawal = FleetWithdrawal::Immediate;
        let mut rng = DeterministicRngState::from_u64(17).next_rng();
        let report = resolve_combat_with_retreat_with_rng(
            1,
            &source.mission,
            &source.planet,
            EnergyGrid {
                supply: 1,
                demand: 1,
            },
            Some(2),
            &mut rng,
        );
        assert!(report.combat_report.as_ref().unwrap().defender_retreat.is_some());
        let mut app = playback_app(report, 0, CombatState::Fire);
        app.add_plugins(bevy_tweening::TweeningPlugin);
        let ship = spawn_unit(
            &mut app,
            Unit::Ship(Ship::Dreadnought),
            2,
            Side::Defender,
            FireState::Fired,
        );
        let ground = spawn_unit(
            &mut app,
            Unit::Defense(Defense::GaussCannon),
            3,
            Side::Defender,
            FireState::Fired,
        );
        app.world_mut().resource_mut::<Settings>().combat_paused = true;
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get::<FleetRetreatCmp>(ship).is_none());
        app.world_mut().resource_mut::<Settings>().combat_paused = false;
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get::<FleetRetreatCmp>(ship).is_some());
        assert!(app.world().get::<FleetRetreatCmp>(ground).is_none());
        assert!(app
            .world_mut()
            .query::<&CombatUnitCmp>()
            .iter(app.world())
            .all(|unit| unit.unit != Unit::colony_ship()));
        TweenAnim::step_all(app.world_mut(), Duration::from_millis(450));
        let retreat_position = app.world().get::<Transform>(ship).unwrap().translation;
        assert!(retreat_position.x > 0.);
        assert!(retreat_position.y > 0.);
        assert_eq!(app.world().get::<Transform>(ground).unwrap().translation, Vec3::ZERO);
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get_entity(ship).is_ok());
        TweenAnim::step_all(app.world_mut(), Duration::from_millis(500));
        app.world_mut().run_system_once(animate_combat).unwrap();
        assert!(app.world().get_entity(ship).is_err());
        assert!(app.world().get_entity(ground).is_ok());
        app.world_mut().run_system_once(animate_combat).unwrap();
        let playback =
            app.world_mut().query::<&FleetRetreatPlayback>().single(app.world()).unwrap();
        assert!(playback.complete);
        assert!(app.world().resource::<Messages<SpawnShotMsg>>().is_empty());
    }
}

#[test]
fn combat_setup_never_spawns_colony_ship_cards() {
    let mut report = report(1, 0, true, 29);
    report.mission.army.insert(Unit::colony_ship(), 2);
    report.planet.army.insert(Unit::colony_ship(), 3);
    let mut rng = DeterministicRngState::from_u64(29).next_rng();
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    app.world_mut().run_system_once(setup_combat).unwrap();

    assert!(app
        .world_mut()
        .query::<&CombatUnitCmp>()
        .iter(app.world())
        .all(|unit| unit.unit != Unit::colony_ship()));
}

#[test]
fn volley_fire_selects_every_attacker_before_any_defender() {
    let mut report = report(20, 0, true, 41);
    let round = &mut report.combat_report.as_mut().unwrap().rounds[0];
    let mut fighter = round
        .attacker
        .iter()
        .find(|unit| unit.shots.iter().any(|shot| !shot.is_bombing()))
        .unwrap()
        .clone();
    fighter.unit = Unit::Ship(Ship::LightFighter);
    round.attacker.push(fighter);

    let mut app = playback_app(report, 0, CombatState::Fire);
    let bomber =
        spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 20, Side::Attacker, FireState::Idle);
    let fighter =
        spawn_unit(&mut app, Unit::Ship(Ship::LightFighter), 1, Side::Attacker, FireState::Idle);
    let defender = spawn_unit(
        &mut app,
        Unit::Defense(Defense::GaussCannon),
        12,
        Side::Defender,
        FireState::Idle,
    );
    let repair_truck =
        spawn_unit(&mut app, Unit::repair_truck(), 1, Side::Defender, FireState::Idle);
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert_eq!(
        app.world_mut()
            .query::<&CombatUnitCmp>()
            .iter(app.world())
            .filter(|card| card.side == Side::Attacker && card.fire == FireState::Select)
            .count(),
        1,
        "the default presentation still selects one unit kind at a time"
    );
    {
        let world = app.world_mut();
        let mut cards = world.query::<&mut CombatUnitCmp>();
        for mut card in cards.iter_mut(world) {
            card.fire = FireState::Idle;
        }
    }

    app.world_mut().resource_mut::<Settings>().combat_volley_fire = true;
    app.world_mut().run_system_once(animate_combat).unwrap();

    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Select);
    assert!(app.world().get::<CombatUnitCmp>(fighter).unwrap().fire == FireState::Select);
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Idle);
    assert!(app.world().get::<CombatUnitCmp>(repair_truck).unwrap().fire == FireState::Idle);

    app.world_mut().get_mut::<CombatUnitCmp>(bomber).unwrap().fire = FireState::Fired;
    app.world_mut().get_mut::<CombatUnitCmp>(fighter).unwrap().fire = FireState::Fired;
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Select);
    assert!(
        app.world().get::<CombatUnitCmp>(repair_truck).unwrap().fire == FireState::Idle,
        "healing support must not animate as part of the defender's weapon volley"
    );
}

#[test]
fn volley_fire_shoots_immediately_then_waits_after_impacts_before_return_fire() {
    let report = report(20, 0, true, 41);
    let expected_shots = report.combat_report.as_ref().unwrap().rounds[0]
        .attacker
        .iter()
        .filter(|unit| unit.unit == Unit::Ship(Ship::Bomber))
        .flat_map(|unit| &unit.shots)
        .filter(|shot| !shot.is_bombing())
        .count();
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.add_plugins(bevy_tweening::TweeningPlugin);
    app.world_mut().resource_mut::<Settings>().combat_volley_fire = true;
    let bomber =
        spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 20, Side::Attacker, FireState::Select);
    let defender = spawn_unit(
        &mut app,
        Unit::Defense(Defense::GaussCannon),
        12,
        Side::Defender,
        FireState::Idle,
    );
    let original = Transform::from_xyz(25.0, 40.0, 3.0).with_scale(Vec3::splat(0.8));
    app.world_mut().entity_mut(bomber).insert(original);
    let mut shots = app.world().resource::<Messages<SpawnShotMsg>>().get_cursor();

    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Firing);
    assert!(app.world().get::<TweenAnim>(bomber).is_none());
    assert_eq!(*app.world().get::<Transform>(bomber).unwrap(), original);
    assert_eq!(shots.read(app.world().resource::<Messages<SpawnShotMsg>>()).count(), 0);

    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Deselect);
    assert_eq!(
        shots.read(app.world().resource::<Messages<SpawnShotMsg>>()).count(),
        expected_shots
    );

    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::VolleyResolving);
    assert!(app.world().get::<TweenAnim>(bomber).is_none());
    assert_eq!(*app.world().get::<Transform>(bomber).unwrap(), original);
    assert_eq!(shots.read(app.world().resource::<Messages<SpawnShotMsg>>()).count(), 0);

    app.world_mut().run_system_once(animate_combat).unwrap();
    let pause =
        app.world_mut().query_filtered::<Entity, With<VolleyResolutionPause>>().single(app.world());
    assert!(pause.is_ok());
    let pause = pause.unwrap();
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Idle);
    assert_eq!(
        app.world().get::<TweenAnim>(pause).unwrap().tweenable().cycle_duration(),
        Duration::from_millis(VOLLEY_RESOLUTION_PAUSE_MS)
    );

    TweenAnim::step_all(app.world_mut(), Duration::from_millis(VOLLEY_RESOLUTION_PAUSE_MS));
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(bomber).unwrap().fire == FireState::Fired);
    assert!(app.world().get_entity(pause).is_err());
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Idle);

    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<CombatUnitCmp>(defender).unwrap().fire == FireState::Select);
}

#[test]
fn repair_keeps_its_deliberate_highlight_when_volley_fire_is_enabled() {
    let mut app = playback_app(report(1, 0, true, 41), 0, CombatState::Repair);
    app.world_mut().resource_mut::<Settings>().combat_volley_fire = true;
    let repair_truck =
        spawn_unit(&mut app, Unit::repair_truck(), 1, Side::Defender, FireState::Select);

    app.world_mut().run_system_once(animate_combat).unwrap();

    assert!(app.world().get::<CombatUnitCmp>(repair_truck).unwrap().fire == FireState::PreFire);
    assert_eq!(
        app.world().get::<TweenAnim>(repair_truck).unwrap().tweenable().cycle_duration(),
        Duration::from_millis(500)
    );
}

#[test]
fn combat_cards_show_empty_shield_slots_for_probes_and_support_units() {
    let mut report = report(1, 0, true, 7);
    report.mission.objective = Icon::MissileStrike;
    report.mission.bombing = BombingRaid::None;
    report.mission.army = Army::from([(Unit::interplanetary_missile(), 4)]);
    report.planet.army.insert(Unit::antiballistic_missile(), 3);
    report.planet.army.insert(Unit::crawler(), 1);
    report.planet.army.insert(Unit::repair_truck(), 1);
    let mut rng = DeterministicRngState::from_u64(7).next_rng();
    let mut report = resolve_combat_with_rng(1, &report.mission, &report.planet, &mut rng);
    report.mission.army.insert(Unit::probe(), 1);
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
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
    for expected in [
        Unit::antiballistic_missile(),
        Unit::interplanetary_missile(),
        Unit::crawler(),
        Unit::repair_truck(),
        Unit::probe(),
    ] {
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
        let empty_shield_slots = descendants
            .iter()
            .filter(|&&child| app.world().get::<EmptyShieldCmp>(child).is_some())
            .count();
        assert_eq!(hull_bars, usize::from(unit.hull() > 0), "{unit:?} hull bars");
        assert_eq!(shield_bars, usize::from(unit.shield() > 0), "{unit:?} shield bars");
        assert_eq!(
            empty_shield_slots,
            usize::from(
                unit == Unit::probe() || unit == Unit::crawler() || unit == Unit::repair_truck()
            ),
            "{unit:?} empty shield slots"
        );

        for empty in descendants
            .iter()
            .copied()
            .filter(|&child| app.world().get::<EmptyShieldCmp>(child).is_some())
        {
            let fill = app.world().get::<Sprite>(empty).unwrap();
            assert_eq!(fill.color, BG2_COLOR);
            let frame = app.world().get::<ChildOf>(empty).unwrap().parent();
            assert_eq!(app.world().get::<Sprite>(frame).unwrap().color, BG2_COLOR);
        }
    }
}

#[test]
fn planetary_shield_uses_the_full_width_bar_with_its_icon_and_level_count() {
    let mut report = report(5, 3, true, 11);
    for unit in Unit::ships().into_iter().filter(|unit| *unit != Unit::colony_ship()) {
        report.planet.army.insert(unit, 1);
    }
    report.planet.army.insert(Unit::space_dock(), 1);
    let mut rng = DeterministicRngState::from_u64(11).next_rng();
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    let camera =
        app.world_mut().query_filtered::<Entity, With<MainCamera>>().single(app.world()).unwrap();
    let mut projection = OrthographicProjection::default_2d();
    projection.area = Rect::new(-800.0, -450.0, 800.0, 450.0);
    app.world_mut().entity_mut(camera).insert(Projection::Orthographic(projection));
    app.world_mut().run_system_once(setup_combat).unwrap();

    let (shield_entity, dimensions, shield_home) = app
        .world_mut()
        .query::<(Entity, &Sprite, &CombatUnitCmp, &CombatCardHome)>()
        .iter(app.world())
        .find_map(|(entity, sprite, card, home)| {
            (card.unit == Unit::planetary_shield()).then_some((
                entity,
                sprite.custom_size.unwrap(),
                home.0,
            ))
        })
        .unwrap();
    assert!(dimensions.x > dimensions.y * 30.0, "the old shield is a full-width bar");

    let descendants = app
        .world_mut()
        .run_system_once(move |children: Query<&Children>| {
            children.iter_descendants(shield_entity).collect::<Vec<_>>()
        })
        .unwrap();
    let fill = descendants
        .iter()
        .find_map(|&entity| {
            app.world().get::<ShieldCmp>(entity).and_then(|_| app.world().get::<Sprite>(entity))
        })
        .unwrap();
    assert!(fill.custom_size.unwrap().x > dimensions.x * 0.99);
    assert!(descendants
        .iter()
        .any(|&entity| app.world().get::<PSCombatImageCmp>(entity).is_some()));
    assert!(descendants
        .iter()
        .any(|&entity| { app.world().get::<Text2d>(entity).is_some_and(|text| text.0 == "3") }));
    let shield_icon_offset = descendants
        .iter()
        .find_map(|&entity| {
            app.world()
                .get::<PSCombatImageCmp>(entity)
                .and_then(|_| app.world().get::<Transform>(entity))
                .map(|transform| transform.translation)
        })
        .unwrap();
    let dock_home = app
        .world_mut()
        .query::<(&CombatUnitCmp, &CombatCardHome)>()
        .iter(app.world())
        .find_map(|(card, home)| (card.unit == Unit::space_dock()).then_some(home.0))
        .unwrap();
    assert!(
        (shield_home.x
            - dimensions.x * 0.5
            - (shield_home.x + shield_icon_offset.x + UNIT_SIZE * 0.5))
            .abs()
            < 0.01,
        "the planetary shield bar begins exactly at the image's right edge"
    );
    assert!(
        (shield_home.y + dimensions.y * 0.5
            - (shield_home.y + shield_icon_offset.y + UNIT_SIZE * 0.5))
            .abs()
            < 0.01,
        "the planetary shield bar top aligns exactly with the image's top edge"
    );
    assert!(
        (shield_home.x + dimensions.x * 0.5 - (dock_home.x + UNIT_SIZE * 0.5)).abs() < 0.01,
        "the planetary shield bar ends flush with the space dock"
    );
}

#[test]
fn defense_cards_clear_combat_controls_and_bombing_targets() {
    let mut report = report(5, 3, true, 11);
    report.planet.army.insert(Unit::Ship(Ship::LightFighter), 1);
    for unit in Unit::defenses() {
        if !unit.is_missile() && unit != Unit::space_dock() {
            report.planet.army.insert(unit, 1);
        }
    }
    let mut rng = DeterministicRngState::from_u64(11).next_rng();
    let origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
    let map = Map {
        rect: Rect::new(-100., -100., 100., 100.),
        solar_corner: crate::core::map::model::SolarCorner::BottomLeft,
        planets: vec![origin, report.planet.clone()],
    };
    let mut app = playback_app(report, 0, CombatState::Fire);
    app.insert_resource(map).init_resource::<MultiplayerSession>();
    let camera =
        app.world_mut().query_filtered::<Entity, With<MainCamera>>().single(app.world()).unwrap();
    let mut projection = OrthographicProjection::default_2d();
    projection.area = Rect::new(-960.0, -540.0, 960.0, 540.0);
    app.world_mut().entity_mut(camera).insert(Projection::Orthographic(projection));
    app.world_mut().run_system_once(setup_combat).unwrap();

    let homes = app
        .world_mut()
        .query::<(&CombatUnitCmp, &CombatCardHome)>()
        .iter(app.world())
        .map(|(card, home)| (card.unit, home.0))
        .collect::<Vec<_>>();
    let defenses = homes
        .iter()
        .filter(|(unit, _)| matches!(unit, Unit::Defense(_)) && !unit.is_missile())
        .map(|(_, home)| *home)
        .collect::<Vec<_>>();
    let buildings = homes
        .iter()
        .filter(|(unit, _)| unit.is_building() && *unit != Unit::planetary_shield())
        .map(|(_, home)| *home)
        .collect::<Vec<_>>();
    assert!(!defenses.is_empty() && !buildings.is_empty());

    let size = UNIT_SIZE;
    let defense_bottom = defenses
        .iter()
        .map(|home| home.y - size * COMBAT_CARD_LOWER_EXTENT_FACTOR)
        .fold(f32::INFINITY, f32::min);
    let controls_top = -540.0 + MAIN_BUTTON_BOTTOM + MAIN_BUTTON_HEIGHT;
    assert!(
        defense_bottom >= controls_top - size * 0.05
            && defense_bottom < controls_top + COMBAT_SHIELD_DEFENSE_GAP,
        "the defense row moves slightly down without materially entering the controls"
    );

    let defense_right =
        defenses.iter().map(|home| home.x + size * 0.5).fold(f32::NEG_INFINITY, f32::max);
    let building_left = buildings
        .iter()
        .map(|home| home.x - size * COMBAT_BUILDING_SIZE_FACTOR * 0.5)
        .fold(f32::INFINITY, f32::min);
    assert!(
        defense_right + COMBAT_SHIELD_DEFENSE_GAP <= building_left,
        "defense cards clear the bombing targets"
    );

    let (shield_entity, shield_home, shield_size) = app
        .world_mut()
        .query::<(Entity, &Sprite, &CombatUnitCmp, &CombatCardHome)>()
        .iter(app.world())
        .find_map(|(entity, sprite, card, home)| {
            (card.unit == Unit::planetary_shield()).then_some((
                entity,
                home.0,
                sprite.custom_size.unwrap(),
            ))
        })
        .unwrap();
    let shield_icon_offset = app
        .world_mut()
        .run_system_once(
            move |children: Query<&Children>, icons: Query<&Transform, With<PSCombatImageCmp>>| {
                children
                    .iter_descendants(shield_entity)
                    .find_map(|entity| {
                        icons.get(entity).ok().map(|transform| transform.translation)
                    })
                    .unwrap()
            },
        )
        .unwrap();
    let shield_bar_bottom = shield_home.y - shield_size.y * 0.5;
    let shield_bar_top = shield_home.y + shield_size.y * 0.5;
    let shield_icon_top = shield_home.y + shield_icon_offset.y + size * 0.5;
    let shield_bar_left = shield_home.x - shield_size.x * 0.5;
    let shield_icon_left = shield_home.x + shield_icon_offset.x - size * 0.5;
    let shield_icon_right = shield_home.x + shield_icon_offset.x + size * 0.5;
    let crawler_x =
        homes.iter().find_map(|(unit, home)| (*unit == Unit::crawler()).then_some(home.x)).unwrap();
    let repair_truck_x = homes
        .iter()
        .find_map(|(unit, home)| (*unit == Unit::repair_truck()).then_some(home.x))
        .unwrap();
    let shield_to_crawler_gap = crawler_x - size * 0.5 - shield_icon_right;
    let crawler_to_repair_gap = repair_truck_x - size * 0.5 - (crawler_x + size * 0.5);
    assert!(
        (shield_to_crawler_gap - crawler_to_repair_gap).abs() < 0.01,
        "the planetary shield-to-Crawler gap matches the Crawler-to-Repair Truck gap"
    );
    assert!(shield_icon_left > -960.0, "the planetary shield remains inside the viewport");
    assert!(
        (shield_bar_left - shield_icon_right).abs() < 0.01,
        "the planetary shield bar begins exactly at the shield image's right edge"
    );
    let defense_top =
        defenses.iter().map(|home| home.y + size * 0.5).fold(f32::NEG_INFINITY, f32::max);
    assert!(
        (shield_bar_bottom - defense_top - COMBAT_SHIELD_DEFENSE_GAP * 0.5).abs() < 0.01,
        "the planetary shield health bar sits just above the defense row"
    );
    assert!(
        (shield_icon_top - shield_bar_top).abs() < 0.01,
        "the planetary shield image's top-right corner anchors the health bar"
    );
    let shield_fill = app
        .world_mut()
        .query_filtered::<&Sprite, (With<PlanetaryShieldFillCmp>, With<ShieldCmp>)>()
        .single(app.world())
        .unwrap();
    assert_eq!(shield_fill.color, SHIELD_COLOR);
    let full_fill_width = shield_fill.custom_size.unwrap().x;

    app.world_mut().get_mut::<CombatUnitCmp>(shield_entity).unwrap().shield /= 2;
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(100));
    app.world_mut().run_system_once(update_combat_stats).unwrap();
    let animated_fill_width = app
        .world_mut()
        .query_filtered::<&Sprite, (With<PlanetaryShieldFillCmp>, With<ShieldCmp>)>()
        .single(app.world())
        .unwrap()
        .custom_size
        .unwrap()
        .x;
    assert!(
        animated_fill_width > full_fill_width * 0.5 && animated_fill_width < full_fill_width,
        "the single shield fill rapidly interpolates instead of jumping to its new value"
    );
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_millis(400));
    app.world_mut().run_system_once(update_combat_stats).unwrap();
    let settled_fill_width = app
        .world_mut()
        .query_filtered::<&Sprite, (With<PlanetaryShieldFillCmp>, With<ShieldCmp>)>()
        .single(app.world())
        .unwrap()
        .custom_size
        .unwrap()
        .x;
    assert!((settled_fill_width - full_fill_width * 0.5).abs() < 0.01);

    let defender_ship = homes
        .iter()
        .find_map(|(unit, home)| (*unit == Unit::Ship(Ship::LightFighter)).then_some(*home))
        .unwrap();
    assert!(
        defender_ship.y < -108.0,
        "defending ships move slightly down from their former middle-row position"
    );
    assert!(
        defender_ship.y - size * COMBAT_CARD_LOWER_EXTENT_FACTOR
            >= shield_bar_top + COMBAT_SHIELD_DEFENSE_GAP,
        "defending ships and their stat bars clear the planetary shield health bar"
    );
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
fn round_banner_after_navigation_uses_the_normal_playback_duration() {
    let report = report(12, 5, true, 2);
    assert!(report.combat_report.as_ref().unwrap().rounds.len() > 1);
    let mut app = playback_app(report, 0, CombatState::DisplayRound);
    app.insert_resource(CombatRoundJump);

    app.world_mut().run_system_once(animate_combat).unwrap();

    let tween = app
        .world_mut()
        .query_filtered::<&TweenAnim, With<DisplayTextCmp>>()
        .single(app.world())
        .unwrap();
    assert_eq!(tween.tweenable().cycle_duration(), Duration::from_millis(1500));
}

#[test]
fn surviving_probes_play_one_flyaway_cue_when_their_retreat_begins() {
    let mut report = report(12, 5, true, 2);
    assert!(report.combat_report.as_ref().unwrap().rounds.len() > 1);
    report.mission.bombing = BombingRaid::None;
    let mut app = playback_app(report, 0, CombatState::Fire);
    let probe = spawn_unit(&mut app, Unit::probe(), 3, Side::Attacker, FireState::Fired);

    app.world_mut().run_system_once(animate_combat).unwrap();

    assert!(app.world().get::<ProbeRetreatCmp>(probe).is_some());
    assert!(app.world().get::<TweenAnim>(probe).is_some());
    let sounds =
        app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(sounds.len(), 1);
    assert_eq!(sounds[0].name, "probe retreat");
    assert_eq!(sounds[0].playback_rate, 1.2);

    // The marker prevents later first-round phases from retriggering the same retreat cue.
    app.world_mut().resource_mut::<UiState>().combat_round = 0;
    app.world_mut().insert_resource(NextState::<CombatState>::Unchanged);
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().resource::<Messages<PlayAudioMsg>>().is_empty());
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
fn completion_received_while_paused_is_applied_after_resume() {
    let mut app = playback_app(report(1, 0, true, 41), 0, CombatState::Fire);
    app.add_plugins(bevy_tweening::TweeningPlugin).add_systems(Update, animate_combat);
    let unit =
        spawn_unit(&mut app, Unit::Ship(Ship::Bomber), 1, Side::Attacker, FireState::PreFire);
    app.world_mut().entity_mut(unit).insert((
        CombatCmp,
        TweenAnim::new(Tween::new(
            EaseFunction::Linear,
            Duration::from_millis(1),
            TransformScaleLens {
                start: Vec3::ONE,
                end: Vec3::splat(1.3),
            },
        )),
    ));
    app.world_mut().resource_mut::<Settings>().combat_paused = true;

    // Reproduce the race: the tween finishes on the frame pause becomes visible. Its Bevy
    // completion message expires while the state machine remains paused.
    TweenAnim::step_all(app.world_mut(), Duration::from_millis(1));
    assert!(app.world().get::<TweenAnim>(unit).is_none());
    for _ in 0..4 {
        app.update();
    }
    assert!(matches!(app.world().get::<CombatUnitCmp>(unit).unwrap().fire, FireState::PreFire));

    app.world_mut().resource_mut::<Settings>().combat_paused = false;
    app.update();
    assert!(matches!(app.world().get::<CombatUnitCmp>(unit).unwrap().fire, FireState::Firing));
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
fn repair_truck_replays_real_resolver_repairs_and_restores_exact_recorded_hull() {
    // Find a deterministic battle with surviving, damaged turrets. Missing shields
    // or a reduced unit count alone are not eligible for Repair Truck repairs.
    let mut selected = None;
    for seed in 0..40 {
        let mut rng = DeterministicRngState::from_u64(seed).next_rng();
        let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1., &mut rng);
        origin.colonize(1);
        let mut target = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1., &mut rng);
        target.colonize(2);
        target.army = Army::from([
            (Unit::repair_truck(), 8),
            (Unit::Defense(Defense::GaussCannon), 8),
            (Unit::Defense(Defense::PlasmaTurret), 3),
        ])
        .into();
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
    let repair_truck =
        targets.iter().find(|(kind, _, _, _)| *kind == Unit::repair_truck()).unwrap().1;
    app.world_mut().insert_resource(State::new(CombatState::Repair));
    app.world_mut().get_mut::<CombatUnitCmp>(repair_truck).unwrap().fire = FireState::Firing;
    app.world_mut().run_system_once(animate_combat).unwrap();
    let messages = app.world().resource::<Messages<SpawnShotMsg>>();
    let mut cursor = messages.get_cursor();
    let messages = cursor.read(messages).collect::<Vec<_>>();
    assert!(!messages.is_empty());
    assert!(messages.iter().all(|m| m.repair && m.source.unwrap().0 == repair_truck));
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

#[test]
fn crawler_pulses_in_place_and_only_non_zero_salvage_pickups_float_up() {
    let mut report = report(1, 0, true, 19);
    report.planet.army =
        Army::from([(Unit::crawler(), 4), (Unit::Defense(Defense::RocketLauncher), 5)]).into();
    report.surviving_attacker.clear();
    report.surviving_defender =
        Army::from([(Unit::crawler(), 2), (Unit::Defense(Defense::RocketLauncher), 3)]).into();
    assert_eq!(report.defender_salvage(), crate::core::resources::Resources::new(2, 0, 0));

    let mut app = playback_app(report, 0, CombatState::Salvage);
    app.add_plugins(bevy_tweening::TweeningPlugin);
    let crawler = spawn_unit(&mut app, Unit::crawler(), 2, Side::Defender, FireState::Fired);
    let crawler_home = app.world().get::<Transform>(crawler).unwrap().translation;
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert!(app.world().get::<SalvageCrawlerCmp>(crawler).is_some());
    assert_eq!(app.world().get::<Transform>(crawler).unwrap().translation, crawler_home);
    assert_eq!(app.world_mut().query::<&SalvagePickupCmp>().iter(app.world()).count(), 0);
    assert_eq!(app.world_mut().query::<&SalvageTimerCmp>().iter(app.world()).count(), 0);
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));

    TweenAnim::step_all(app.world_mut(), Duration::from_millis(SALVAGE_HIGHLIGHT_TIME_MS));
    app.world_mut().run_system_once(animate_combat).unwrap();
    let pickups = app
        .world_mut()
        .query::<(Entity, &SalvagePickupCmp)>()
        .iter(app.world())
        .map(|(entity, pickup)| (entity, pickup.resource, pickup.amount))
        .collect::<Vec<_>>();
    assert_eq!(pickups.len(), 1);
    assert_eq!((pickups[0].1, pickups[0].2), (ResourceName::Metal, 2));
    let metal_image = app.world().resource::<WorldAssets>().image("metal");
    assert!(app
        .world()
        .get::<Children>(pickups[0].0)
        .unwrap()
        .iter()
        .filter_map(|child| app.world().get::<Sprite>(child))
        .any(|sprite| sprite.image == metal_image));
    assert_eq!(app.world_mut().query::<&SalvageTimerCmp>().iter(app.world()).count(), 1);
    assert_eq!(app.world().get::<Transform>(crawler).unwrap().translation, crawler_home);
    assert!(app.world_mut().query::<&Text2d>().iter(app.world()).any(|text| text.0 == "+2"));
    let amount_text = app
        .world()
        .get::<Children>(pickups[0].0)
        .unwrap()
        .iter()
        .find(|child| app.world().get::<Text2d>(*child).is_some())
        .unwrap();
    assert_eq!(
        app.world().get::<TweenAnim>(pickups[0].0).unwrap().tweenable().cycle_duration(),
        Duration::from_millis(SALVAGE_PICKUP_TIME_MS)
    );
    assert_eq!(
        app.world().get::<TweenAnim>(amount_text).unwrap().tweenable().cycle_duration(),
        Duration::from_millis(SALVAGE_PICKUP_TIME_MS)
    );
    assert!(matches!(*app.world().resource::<NextState<CombatState>>(), NextState::Unchanged));

    TweenAnim::step_all(app.world_mut(), Duration::from_millis(SALVAGE_PICKUP_REVEAL_TIME_MS));
    assert_eq!(app.world().get::<Transform>(amount_text).unwrap().scale, Vec3::ONE);
    TweenAnim::step_all(app.world_mut(), Duration::from_millis(SALVAGE_PICKUP_DRIFT_TIME_MS));
    assert_eq!(app.world().get::<Transform>(amount_text).unwrap().scale, Vec3::ONE);
    TweenAnim::step_all(app.world_mut(), Duration::from_millis(SALVAGE_PICKUP_REVEAL_TIME_MS));
    app.world_mut().run_system_once(animate_combat).unwrap();
    assert_eq!(app.world().get::<Transform>(crawler).unwrap().translation, crawler_home);
    assert!(matches!(
        *app.world().resource::<NextState<CombatState>>(),
        NextState::Pending(CombatState::EndCombat)
    ));
}
