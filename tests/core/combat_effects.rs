//! Regression tests for asynchronous arrivals, effect limits and playback clocks.

use super::*;
use crate::core::combat::report::Side;
use crate::core::combat::resolution::ShotReport;
use crate::core::combat::systems::FireState;
use crate::core::units::Combat;

fn app() -> App {
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<Settings>()
        .init_resource::<Assets<Image>>()
        .add_message::<SpawnShotMsg>()
        .add_message::<PlayAudioMsg>()
        .add_systems(Update, run_combat_animations);
    app
}

fn unit(app: &mut App, unit: Unit, side: Side, pos: Vec3, hull: usize, shield: usize) -> Entity {
    app.world_mut()
        .spawn((
            CombatUnitCmp {
                unit,
                side,
                hull,
                max_hull: hull,
                shield,
                max_shield: shield,
                fire: FireState::Fired,
                outcome_visible: true,
            },
            Sprite {
                custom_size: Some(Vec2::splat(100.)),
                ..default()
            },
            Transform::from_translation(pos),
            CombatCmp,
        ))
        .id()
}

fn step(app: &mut App, seconds: f32) {
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(seconds));
    app.update();
}

fn fire(
    app: &mut App,
    source: Entity,
    shooter: Unit,
    target: Unit,
    shot: ShotReport,
    repair: bool,
) {
    app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
        shot: ShotReport {
            unit: Some(target),
            ..shot
        },
        repair,
        side: Side::Defender,
        source: Some((source, shooter, Vec3::new(0., 200., 11.))),
    });
}

#[test]
fn large_salvos_are_bounded_without_losing_recorded_damage() {
    let mut app = app();
    let shooter = Unit::Ship(Ship::Battleship);
    let target = Unit::Ship(Ship::Cruiser);
    let source = unit(&mut app, shooter, Side::Attacker, Vec3::Y * 200., 100, 0);
    let defender = unit(&mut app, target, Side::Defender, Vec3::ZERO, 20_000, 20_000);
    for _ in 0..10_000 {
        fire(
            &mut app,
            source,
            shooter,
            target,
            ShotReport {
                hull_damage: 1,
                shield_damage: 1,
                ..default()
            },
            false,
        );
    }
    step(&mut app, 0.);
    assert_eq!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count(), 3);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 20_000);
    // Crossing the entire flight in one frame must still apply each total once.
    step(&mut app, 4.);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 10_000);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().shield, 10_000);
    step(&mut app, 4.);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 10_000);
    // Stop the damaged ship's continuing ambient sparks before checking cleanup.
    app.world_mut().get_mut::<CombatUnitCmp>(defender).unwrap().hull = 20_000;
    step(&mut app, 4.);
    assert_eq!(app.world_mut().query::<&Particle>().iter(app.world()).count(), 0);
}

#[test]
fn damaged_units_emit_sparks_without_the_recurring_blue_flicker() {
    let mut app = app();
    let damaged = unit(&mut app, Unit::Ship(Ship::Cruiser), Side::Defender, Vec3::ZERO, 70, 0);
    app.world_mut().get_mut::<CombatUnitCmp>(damaged).unwrap().max_hull = 100;

    step(&mut app, 0.);
    step(&mut app, 1.5);

    let particles = app.world_mut().query::<&Particle>().iter(app.world()).count();
    assert_eq!(particles, 3, "ambient damage should retain only its three sparks");
}

#[test]
fn weapon_families_use_distinct_launch_and_shield_impact_cues() {
    let kinds = [
        Unit::Ship(Ship::LightFighter),
        Unit::Ship(Ship::HeavyFighter),
        Unit::Ship(Ship::Destroyer),
        Unit::Ship(Ship::Cruiser),
        Unit::Ship(Ship::Bomber),
        Unit::Ship(Ship::Battleship),
        Unit::Ship(Ship::Dreadnought),
        Unit::Ship(Ship::WarSun),
        Unit::Defense(Defense::LightLaser),
        Unit::Defense(Defense::HeavyLaser),
        Unit::Defense(Defense::PlasmaTurret),
        Unit::Defense(Defense::IonCannon),
        Unit::Defense(Defense::SpaceDock),
    ];
    for kind in kinds {
        let mut app = app();
        let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
        let target =
            unit(&mut app, Unit::Ship(Ship::Cruiser), Side::Defender, Vec3::ZERO, 100, 100);
        for _ in 0..30 {
            fire(
                &mut app,
                source,
                kind,
                Unit::Ship(Ship::Cruiser),
                ShotReport {
                    shield_damage: 1,
                    ..default()
                },
                false,
            );
        }
        step(&mut app, 0.);
        step(&mut app, 2.);
        let cues =
            app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().collect::<Vec<_>>();
        let mut expected =
            Weapon::for_unit(kind).launch_cue().map_or_else(Vec::new, |cue| vec![cue.name]);
        expected.push("shield impact");
        assert_eq!(
            cues.iter().map(|cue| cue.name).collect::<Vec<_>>(),
            expected,
            "fast-forward must not layer every shot"
        );
        assert_eq!(cues.iter().find(|cue| cue.name == "shield impact").unwrap().playback_rate, 1.0);
        assert_eq!(app.world().get::<CombatUnitCmp>(target).unwrap().shield, 70);
        assert_eq!(app.world().get::<CombatUnitCmp>(target).unwrap().hull, 100);
    }
}

#[test]
fn every_damaging_unit_uses_one_of_the_three_weapon_cues() {
    for kind in Unit::all().into_iter().flatten().filter(|unit| unit.damage() > 0) {
        let cue = Weapon::for_unit(kind)
            .launch_cue()
            .unwrap_or_else(|| panic!("{kind:?} has no firing sound"));
        assert!(
            matches!(cue.name, "laser fire" | "missile fire" | "beam fire"),
            "{kind:?} uses unexpected firing cue {}",
            cue.name
        );
    }
}

#[test]
fn firing_mix_scales_from_fighters_to_heavy_weapons() {
    let light = Weapon::for_unit(Unit::Ship(Ship::LightFighter)).launch_cue().unwrap();
    let heavy = Weapon::for_unit(Unit::Ship(Ship::HeavyFighter)).launch_cue().unwrap();
    let destroyer = Weapon::for_unit(Unit::Ship(Ship::Destroyer)).launch_cue().unwrap();
    let war_sun = Weapon::for_unit(Unit::Ship(Ship::WarSun)).launch_cue().unwrap();
    let space_dock = Weapon::for_unit(Unit::space_dock()).launch_cue().unwrap();

    assert_eq!((light.name, light.volume, light.playback_rate), ("laser fire", -14.0, 1.0));
    assert_eq!((heavy.name, heavy.volume, heavy.playback_rate), ("laser fire", -14.0, 1.0));
    assert_eq!(
        (destroyer.name, destroyer.volume, destroyer.playback_rate),
        ("laser fire", -9.0, 0.86)
    );
    assert_eq!((war_sun.name, war_sun.volume, war_sun.playback_rate), ("beam fire", -5.0, 0.76));
    assert_eq!(
        (space_dock.name, space_dock.volume, space_dock.playback_rate),
        ("beam fire", -6.0, 0.86)
    );
    assert!(destroyer.volume > light.volume);
    assert!(war_sun.volume > destroyer.volume);
    assert!(space_dock.volume > destroyer.volume);
}

#[test]
fn impact_audio_distinguishes_shield_hull_and_destroyed_buildings() {
    let shooter = Unit::Ship(Ship::LightFighter);

    let mut shield_app = app();
    let source = unit(&mut shield_app, shooter, Side::Attacker, Vec3::Y * 200., 100, 0);
    unit(&mut shield_app, Unit::Ship(Ship::Cruiser), Side::Defender, Vec3::ZERO, 100, 100);
    fire(
        &mut shield_app,
        source,
        shooter,
        Unit::Ship(Ship::Cruiser),
        ShotReport {
            shield_damage: 30,
            ..default()
        },
        false,
    );
    step(&mut shield_app, 0.0);
    step(&mut shield_app, 1.0);
    let shield_cues = shield_app
        .world_mut()
        .resource_mut::<Messages<PlayAudioMsg>>()
        .drain()
        .map(|cue| cue.name)
        .collect::<Vec<_>>();
    assert_eq!(shield_cues, ["laser fire", "shield impact"]);

    let mut planetary_app = app();
    let source = unit(&mut planetary_app, shooter, Side::Attacker, Vec3::Y * 200., 100, 0);
    let planetary =
        unit(&mut planetary_app, Unit::planetary_shield(), Side::Defender, Vec3::ZERO, 5, 100);
    let shield_bar_color = Color::srgb_u8(7, 28, 49);
    planetary_app.world_mut().get_mut::<Sprite>(planetary).unwrap().color = shield_bar_color;
    let shield_image_position = Vec3::new(-140.0, -35.0, 11.0);
    planetary_app.world_mut().spawn((
        PSCombatImageCmp,
        Sprite {
            custom_size: Some(Vec2::splat(100.0)),
            ..default()
        },
        GlobalTransform::from_translation(shield_image_position),
    ));
    fire(
        &mut planetary_app,
        source,
        shooter,
        Unit::planetary_shield(),
        ShotReport {
            planetary_shield_damage: 100,
            ..default()
        },
        false,
    );
    step(&mut planetary_app, 0.0);
    let impact_destination = planetary_app
        .world_mut()
        .query::<&PendingImpact>()
        .single(planetary_app.world())
        .unwrap()
        .destination;
    assert!(
        impact_destination.truncate().distance(shield_image_position.truncate()) <= 50.0,
        "planetary-shield fire must terminate on its image"
    );
    step(&mut planetary_app, 1.0);
    assert!(
        planetary_app.world().get::<Wreck>(planetary).unwrap().origin == impact_destination,
        "planetary-shield destruction must remain centered on the image impact"
    );
    let mut planetary_cues = planetary_app
        .world_mut()
        .resource_mut::<Messages<PlayAudioMsg>>()
        .drain()
        .map(|cue| cue.name)
        .collect::<Vec<_>>();
    step(&mut planetary_app, 0.05);
    step(&mut planetary_app, 0.01);
    assert_eq!(
        planetary_app.world().get::<Sprite>(planetary).unwrap().color,
        shield_bar_color,
        "planetary-shield destruction must never flash the health bar white"
    );
    planetary_cues.extend(
        planetary_app
            .world_mut()
            .resource_mut::<Messages<PlayAudioMsg>>()
            .drain()
            .map(|cue| cue.name),
    );
    step(&mut planetary_app, 1.0);
    planetary_cues.extend(
        planetary_app
            .world_mut()
            .resource_mut::<Messages<PlayAudioMsg>>()
            .drain()
            .map(|cue| cue.name),
    );
    assert_eq!(
        planetary_cues,
        ["laser fire", "shield impact", "large explosion", "large explosion", "large explosion",]
    );

    let mut hull_app = app();
    let source = unit(&mut hull_app, shooter, Side::Attacker, Vec3::Y * 200., 100, 0);
    unit(&mut hull_app, Unit::Ship(Ship::Cruiser), Side::Defender, Vec3::ZERO, 100, 20);
    fire(
        &mut hull_app,
        source,
        shooter,
        Unit::Ship(Ship::Cruiser),
        ShotReport {
            shield_damage: 20,
            hull_damage: 10,
            ..default()
        },
        false,
    );
    step(&mut hull_app, 0.0);
    step(&mut hull_app, 1.0);
    let hull_cues = hull_app
        .world_mut()
        .resource_mut::<Messages<PlayAudioMsg>>()
        .drain()
        .map(|cue| cue.name)
        .collect::<Vec<_>>();
    assert_eq!(hull_cues, ["laser fire", "short explosion"]);

    let mut bombing_app = app();
    let bomber = Unit::Ship(Ship::Bomber);
    let building = Unit::resource_buildings()[0];
    let source = unit(&mut bombing_app, bomber, Side::Attacker, Vec3::Y * 200., 100, 0);
    unit(&mut bombing_app, building, Side::Defender, Vec3::ZERO, 5, 0);
    fire(
        &mut bombing_app,
        source,
        bomber,
        building,
        ShotReport {
            killed: true,
            ..default()
        },
        false,
    );
    step(&mut bombing_app, 0.0);
    step(&mut bombing_app, 2.0);
    let bombing_cues = bombing_app
        .world_mut()
        .resource_mut::<Messages<PlayAudioMsg>>()
        .drain()
        .map(|cue| cue.name)
        .collect::<Vec<_>>();
    assert_eq!(bombing_cues, ["missile fire", "large explosion"]);
}

#[test]
fn one_firing_card_launches_at_ships_shields_and_defenses_together() {
    for shooter in [Unit::Ship(Ship::WarSun), Unit::Ship(Ship::Destroyer)] {
        let mut app = app();
        let ship_kind = Unit::Ship(Ship::Cruiser);
        let defense_kind = Unit::Defense(Defense::GaussCannon);
        let source = unit(&mut app, shooter, Side::Attacker, Vec3::Y * 200., 100, 0);
        let ship = unit(&mut app, ship_kind, Side::Defender, Vec3::new(-150., 0., 0.), 100, 0);
        let shield = unit(&mut app, Unit::planetary_shield(), Side::Defender, Vec3::ZERO, 5, 100);
        let defense = unit(&mut app, defense_kind, Side::Defender, Vec3::X * 150., 100, 0);
        for (target, shot) in [
            (
                ship_kind,
                ShotReport {
                    hull_damage: 10,
                    ..default()
                },
            ),
            (
                Unit::planetary_shield(),
                ShotReport {
                    planetary_shield_damage: 100,
                    ..default()
                },
            ),
            (
                defense_kind,
                ShotReport {
                    hull_damage: 10,
                    ..default()
                },
            ),
            (
                defense_kind,
                ShotReport {
                    hull_damage: 5,
                    rapid_fire: true,
                    ..default()
                },
            ),
        ] {
            fire(&mut app, source, shooter, target, shot, false);
        }
        step(&mut app, 0.0);

        let impacts = app
            .world_mut()
            .query::<&PendingImpact>()
            .iter(app.world())
            .map(|impact| (impact.target, impact.delay, impact.launched))
            .collect::<Vec<_>>();
        let expected_impacts = if shooter == Unit::Ship(Ship::Destroyer) {
            4
        } else {
            3
        };
        assert_eq!(impacts.len(), expected_impacts, "{shooter:?}");
        assert!(impacts.iter().any(|impact| impact.0 == ship), "{shooter:?}");
        assert!(impacts.iter().any(|impact| impact.0 == shield), "{shooter:?}");
        assert_eq!(
            impacts.iter().filter(|impact| impact.0 == defense).count(),
            expected_impacts - 2,
            "{shooter:?}",
        );
        assert!(impacts.iter().all(|impact| impact.1 == impacts[0].1), "{shooter:?}");

        step(&mut app, impacts[0].1 + 0.01);
        assert!(
            app.world_mut()
                .query::<&PendingImpact>()
                .iter(app.world())
                .all(|impact| impact.launched),
            "{shooter:?}",
        );
    }
}

#[test]
fn laser_fire_preserves_the_original_hull_impact_cue() {
    let mut app = app();
    let shooter = Unit::Ship(Ship::LightFighter);
    let target = Unit::Ship(Ship::Cruiser);
    let source = unit(&mut app, shooter, Side::Attacker, Vec3::Y * 200., 100, 0);
    unit(&mut app, target, Side::Defender, Vec3::ZERO, 100, 0);
    fire(
        &mut app,
        source,
        shooter,
        target,
        ShotReport {
            hull_damage: 1,
            ..default()
        },
        false,
    );

    step(&mut app, 0.0);
    step(&mut app, 1.0);

    let cues = app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(
        cues.iter().map(|cue| cue.name).collect::<Vec<_>>(),
        ["laser fire", "short explosion"]
    );
    assert_eq!(cues[1].volume, HULL_IMPACT_VOLUME);
}

#[test]
fn plasma_and_ion_are_beams_and_massive_weapons_have_wider_profiles() {
    let plasma = Weapon::for_unit(Unit::Defense(Defense::PlasmaTurret));
    let ion = Weapon::for_unit(Unit::Defense(Defense::IonCannon));
    let green = plasma.color().to_srgba();
    let blue = ion.color().to_srgba();
    assert!(green.green > green.blue * 2. && green.green > green.red * 2.);
    assert!(blue.blue > blue.green * 2. && blue.blue > blue.red * 2.);
    for heavy in [Weapon::Solar, Weapon::Siege] {
        assert!(heavy.beam_width().unwrap() > plasma.beam_width().unwrap());
        assert!(heavy.charge() > plasma.charge());
    }
}

#[test]
fn every_weapon_ends_at_its_leading_tip_without_overshooting_the_target() {
    let kinds = [
        Unit::Ship(Ship::LightFighter),
        Unit::Ship(Ship::HeavyFighter),
        Unit::Ship(Ship::Destroyer),
        Unit::Ship(Ship::Cruiser),
        Unit::Ship(Ship::Bomber),
        Unit::Ship(Ship::Battleship),
        Unit::Ship(Ship::Dreadnought),
        Unit::Ship(Ship::WarSun),
        Unit::Defense(Defense::LightLaser),
        Unit::Defense(Defense::HeavyLaser),
        Unit::Defense(Defense::GaussCannon),
        Unit::Defense(Defense::PlasmaTurret),
        Unit::Defense(Defense::IonCannon),
        Unit::Defense(Defense::SpaceDock),
    ];
    for kind in kinds {
        for missed in [false, true] {
            let mut app = app();
            let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
            unit(&mut app, kind, Side::Defender, Vec3::ZERO, 100, 0);
            fire(
                &mut app,
                source,
                kind,
                kind,
                ShotReport {
                    missed,
                    ..default()
                },
                false,
            );
            step(&mut app, 0.);
            let weapon = Weapon::for_unit(kind);
            let mut elapsed = 0.;
            for progress in [0.2, 0.55, 0.99] {
                let next = 0.08 + weapon.charge() + weapon.flight() * progress;
                step(&mut app, next - elapsed);
                elapsed = next;
                let (impact, transform) = app
                    .world_mut()
                    .query::<(&PendingImpact, &Transform)>()
                    .single(app.world())
                    .unwrap();
                let tip =
                    transform.translation + transform.rotation * Vec3::X * transform.scale.x * 0.5;
                let expected = impact.position(progress);
                assert!(tip.truncate().distance(expected.truncate()) < 0.001, "{kind:?}");
                assert!(
                    impact.destination.truncate().abs().cmple(Vec2::splat(50.0)).all(),
                    "{kind:?} must terminate inside the 100px target card, including misses"
                );
            }
        }
    }
}

#[test]
fn interceptors_finish_inside_the_incoming_missile_image() {
    let mut app = app();
    let interceptor = Unit::antiballistic_missile();
    let incoming = Unit::interplanetary_missile();
    let source = unit(&mut app, interceptor, Side::Attacker, Vec3::Y * -200., 0, 0);
    let target = unit(&mut app, incoming, Side::Defender, Vec3::ZERO, 0, 0);

    for missed in [false, true] {
        for _ in 0..6 {
            fire(
                &mut app,
                source,
                interceptor,
                incoming,
                ShotReport {
                    missed,
                    killed: !missed,
                    ..default()
                },
                false,
            );
        }
    }
    step(&mut app, 0.);

    let target_center = app.world().get::<Transform>(target).unwrap().translation;
    let impacts = app
        .world_mut()
        .query::<&PendingImpact>()
        .iter(app.world())
        .map(|impact| (impact.destination - target_center, impact.missed))
        .collect::<Vec<_>>();
    assert_eq!(impacts.len(), 12);
    for (offset, _) in &impacts {
        assert!(offset.x.abs() <= 50. && offset.y.abs() <= 50., "{offset:?}");
    }
    assert!(impacts.iter().filter(|(_, missed)| *missed).all(|(offset, _)| offset.x > 25.));
    assert!(impacts.iter().filter(|(_, missed)| !*missed).all(|(offset, _)| offset.x.abs() < 25.));
}

#[test]
fn missile_salvos_are_bounded_above_the_ordinary_weapon_limit() {
    let mut app = app();
    let interceptor = Unit::antiballistic_missile();
    let incoming = Unit::interplanetary_missile();
    let source = unit(&mut app, interceptor, Side::Attacker, Vec3::Y * -200., 0, 0);
    unit(&mut app, incoming, Side::Defender, Vec3::ZERO, 0, 0);

    for _ in 0..20 {
        fire(
            &mut app,
            source,
            interceptor,
            incoming,
            ShotReport {
                missed: true,
                ..default()
            },
            false,
        );
    }
    step(&mut app, 0.);

    assert_eq!(
        app.world_mut().query::<&PendingImpact>().iter(app.world()).count(),
        MISSILE_SALVO_LIMIT
    );
}

#[test]
fn every_recorded_bomber_raid_attempt_gets_its_own_bomb() {
    let mut app = app();
    let bomber = Unit::Ship(Ship::Bomber);
    let building = Unit::resource_buildings()[0];
    let source = unit(&mut app, bomber, Side::Attacker, Vec3::Y * 200., 6, 0);
    unit(&mut app, building, Side::Defender, Vec3::ZERO, 6, 0);

    for _ in 0..6 {
        fire(
            &mut app,
            source,
            bomber,
            building,
            ShotReport {
                missed: true,
                ..default()
            },
            false,
        );
    }
    step(&mut app, 0.);

    let mut delays = app
        .world_mut()
        .query::<&PendingImpact>()
        .iter(app.world())
        .map(|impact| impact.delay)
        .collect::<Vec<_>>();
    delays.sort_by(f32::total_cmp);
    assert_eq!(delays.len(), 6);
    assert!(delays.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn bomb_salvos_share_the_expanded_missile_bound() {
    let mut app = app();
    let bomber = Unit::Ship(Ship::Bomber);
    let building = Unit::resource_buildings()[0];
    let source = unit(&mut app, bomber, Side::Attacker, Vec3::Y * 200., 20, 0);
    unit(&mut app, building, Side::Defender, Vec3::ZERO, 20, 0);

    for _ in 0..20 {
        fire(
            &mut app,
            source,
            bomber,
            building,
            ShotReport {
                missed: true,
                ..default()
            },
            false,
        );
    }
    step(&mut app, 0.);

    assert_eq!(
        app.world_mut().query::<&PendingImpact>().iter(app.world()).count(),
        MISSILE_SALVO_LIMIT
    );
}

#[test]
fn missiles_use_a_slower_shallower_flight() {
    assert_eq!(Weapon::Missile.flight(), MISSILE_FLIGHT_TIME);
    const { assert!(MISSILE_FLIGHT_TIME >= 0.9) };
    assert!(Weapon::Repair.flight() > 1.5);

    let impact = PendingImpact {
        target: Entity::PLACEHOLDER,
        source: None,
        origin: Vec3::new(0., 200., 0.),
        destination: Vec3::ZERO,
        size: 100.,
        weapon: Weapon::Missile,
        missed: false,
        hull: 0,
        shield: 0,
        planetary: 0,
        levels: 0,
        elapsed: 0.,
        delay: 0.,
        lane: 1.,
        launched: false,
        readout_shown: false,
        trail_clock: 0.,
    };
    let midpoint = impact.position(0.5);
    let direct_midpoint = impact.origin.lerp(impact.destination, 0.5);
    assert!(midpoint.distance(direct_midpoint) <= impact.size * 0.6);
}

#[test]
fn bombing_uses_a_large_slow_missile_profile_and_still_targets_the_building() {
    let mut app = app();
    let bomber = Unit::Ship(Ship::Bomber);
    let building = Unit::resource_buildings()[0];
    let source = unit(&mut app, bomber, Side::Attacker, Vec3::Y * 200., 100, 0);
    unit(&mut app, building, Side::Defender, Vec3::ZERO, 5, 0);
    fire(
        &mut app,
        source,
        bomber,
        building,
        ShotReport {
            missed: true,
            ..default()
        },
        false,
    );
    step(&mut app, 0.0);

    let impact = app.world_mut().query::<&PendingImpact>().single(app.world()).unwrap();
    assert_eq!(impact.weapon, Weapon::Bomb);
    assert!(Weapon::Bomb.flight() > Weapon::Missile.flight() * 1.5);
    assert!(
        Weapon::Bomb.projectile_size(100.0).length()
            > Weapon::Missile.projectile_size(100.0).length() * 1.4
    );
    assert_eq!(Weapon::for_unit(bomber).launch_cue().unwrap().name, "missile fire");
    let cue = Weapon::Bomb.launch_cue().unwrap();
    assert_eq!((cue.name, cue.playback_rate), ("missile fire", 0.72));
    assert!(impact.destination.truncate().abs().cmple(Vec2::splat(50.0)).all());

    step(&mut app, 0.2);
    let cues = app
        .world_mut()
        .resource_mut::<Messages<PlayAudioMsg>>()
        .drain()
        .map(|cue| cue.name)
        .collect::<Vec<_>>();
    assert_eq!(cues, ["missile fire"]);
}

#[test]
fn missed_missiles_add_a_flyby_cue_when_they_pass_the_target() {
    let mut app = app();
    let bomber = Unit::Ship(Ship::Bomber);
    let target = Unit::Ship(Ship::Cruiser);
    let source = unit(&mut app, bomber, Side::Attacker, Vec3::Y * 200., 100, 0);
    unit(&mut app, target, Side::Defender, Vec3::ZERO, 100, 0);
    fire(
        &mut app,
        source,
        bomber,
        target,
        ShotReport {
            missed: true,
            ..default()
        },
        false,
    );

    step(&mut app, 0.0);
    step(&mut app, 2.0);

    let cues = app
        .world_mut()
        .resource_mut::<Messages<PlayAudioMsg>>()
        .drain()
        .map(|cue| (cue.name, cue.volume, cue.playback_rate))
        .collect::<Vec<_>>();
    assert_eq!(cues, [("missile fire", -11.0, 1.0), ("missile miss", -11.0, 1.45)]);
}

#[test]
fn missed_lasers_also_add_a_flyby_cue_when_they_pass_the_target() {
    let mut app = app();
    let fighter = Unit::Ship(Ship::LightFighter);
    let target = Unit::Ship(Ship::Cruiser);
    let source = unit(&mut app, fighter, Side::Attacker, Vec3::Y * 200., 100, 0);
    unit(&mut app, target, Side::Defender, Vec3::ZERO, 100, 0);
    fire(
        &mut app,
        source,
        fighter,
        target,
        ShotReport {
            missed: true,
            ..default()
        },
        false,
    );

    step(&mut app, 0.0);
    step(&mut app, 2.0);

    let cues = app
        .world_mut()
        .resource_mut::<Messages<PlayAudioMsg>>()
        .drain()
        .map(|cue| (cue.name, cue.volume, cue.playback_rate))
        .collect::<Vec<_>>();
    assert_eq!(cues, [("laser fire", -14.0, 1.0), ("missile miss", -11.0, 1.45)]);
}

#[test]
fn pause_freezes_projectiles_particles_and_damage_then_speed_resumes_them() {
    let mut app = app();
    let kind = Unit::Ship(Ship::Bomber);
    let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
    let defender = unit(&mut app, kind, Side::Defender, Vec3::ZERO, 100, 0);
    fire(
        &mut app,
        source,
        kind,
        kind,
        ShotReport {
            hull_damage: 20,
            ..default()
        },
        false,
    );
    step(&mut app, 0.);
    step(&mut app, 0.2);
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    let before = app.world_mut().query::<&PendingImpact>().single(app.world()).unwrap().elapsed;
    step(&mut app, 10.);
    assert_eq!(
        app.world_mut().query::<&PendingImpact>().single(app.world()).unwrap().elapsed,
        before
    );
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 100);
    app.world_mut().resource_mut::<Settings>().combat_paused = false;
    app.world_mut().resource_mut::<Settings>().combat_speed = 8.;
    step(&mut app, 0.11);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 80);
}

#[test]
fn misses_show_feedback_without_moving_cards_and_repair_drones_deliver_once() {
    let mut app = app();
    let kind = Unit::Ship(Ship::LightFighter);
    let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
    let defender = unit(&mut app, kind, Side::Defender, Vec3::ZERO, 100, 0);
    fire(
        &mut app,
        source,
        kind,
        kind,
        ShotReport {
            missed: true,
            hull_damage: 99,
            ..default()
        },
        false,
    );
    step(&mut app, 0.);
    step(&mut app, 0.27);
    assert_eq!(app.world().get::<Transform>(defender).unwrap().translation, Vec3::ZERO);
    step(&mut app, 0.3);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 100);
    assert_eq!(app.world().get::<Transform>(defender).unwrap().translation, Vec3::ZERO);
    let (font_size, scale) = app
        .world_mut()
        .query::<(&Text2d, &TextFont, &Transform)>()
        .iter(app.world())
        .find_map(|(label, font, transform)| {
            (label.0 == "MISS").then_some((font.font_size, transform.scale))
        })
        .unwrap();
    assert_eq!(scale, Vec3::splat(COMBAT_READOUT_RASTER_SCALE));
    assert!(matches!(font_size, FontSize::Px(size) if size > 100.0));
    app.world_mut().get_mut::<CombatUnitCmp>(defender).unwrap().hull = 30;
    for _ in 0..10 {
        fire(
            &mut app,
            source,
            Unit::repair_truck(),
            kind,
            ShotReport {
                hull_damage: 5,
                ..default()
            },
            true,
        );
    }
    step(&mut app, 0.);
    step(&mut app, 0.09 + Weapon::Repair.flight() * REPAIR_READOUT_PROGRESS);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 30);
    assert!(app.world_mut().query::<&Text2d>().iter(app.world()).any(|text| text.0 == "+50 HULL"));
    step(&mut app, Weapon::Repair.flight());
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 80);
    step(&mut app, 2.);
    assert_eq!(app.world().get::<CombatUnitCmp>(defender).unwrap().hull, 80);
    assert!(app.world().get::<Transform>(defender).unwrap().translation.length() < 0.001);
}

#[test]
fn destroyed_target_during_flight_is_safe_and_wrecks_finish_at_low_frame_rates() {
    let mut app = app();
    let kind = Unit::Ship(Ship::WarSun);
    let source = unit(&mut app, kind, Side::Attacker, Vec3::Y * 200., 100, 0);
    let target = unit(&mut app, kind, Side::Defender, Vec3::ZERO, 100, 0);
    fire(
        &mut app,
        source,
        kind,
        kind,
        ShotReport {
            hull_damage: 100,
            ..default()
        },
        false,
    );
    step(&mut app, 0.);
    app.world_mut().despawn(target);
    app.world_mut().entity_mut(source).insert(Wreck::new(Vec3::Y * 200., 100., kind));
    step(&mut app, 4.);
    assert!(app.world().get_entity(source).is_err());
    assert_eq!(app.world_mut().query::<&PendingImpact>().iter(app.world()).count(), 0);
    step(&mut app, 4.);
    assert_eq!(app.world_mut().query::<&Particle>().iter(app.world()).count(), 0);
}

#[test]
fn planet_destruction_effect_requires_a_recorded_planet_kill() {
    for destroys in [false, true] {
        let mut app = app();
        app.world_mut().spawn((
            Cinematic::new(Vec3::Y * 200., Vec3::ZERO, Vec2::new(900., 600.), 100., destroys),
            CombatCmp,
        ));
        let mut cursor = app.world().resource::<Messages<PlayAudioMsg>>().get_cursor();
        step(&mut app, 4.);
        // Only the recorded destruction requests a large explosion sound.
        assert_eq!(
            cursor.read(app.world().resource::<Messages<PlayAudioMsg>>()).count() > 0,
            destroys
        );
        step(&mut app, DEATH_RAY_DURATION + 0.1);
        assert_eq!(app.world_mut().query::<&Particle>().iter(app.world()).count(), 0);
    }
}

#[test]
fn planet_kill_uses_a_heavy_sustained_beam_and_irregular_fissures() {
    let mut app = app();
    app.world_mut().spawn((
        Cinematic::new(Vec3::Y * 300., Vec3::ZERO, Vec2::new(900., 600.), 100., true),
        CombatCmp,
    ));

    step(&mut app, 2.01);
    let beam_widths = {
        let world = app.world_mut();
        let mut query = world.query_filtered::<&Sprite, With<CinematicBeam>>();
        query
            .iter(world)
            .filter_map(|sprite| sprite.custom_size.map(|size| size.y))
            .collect::<Vec<_>>()
    };
    assert_eq!(beam_widths.len(), 3, "the discharge should have three energetic layers");
    assert!(
        beam_widths.iter().copied().fold(0.0, f32::max) >= 230.,
        "the outer beam envelope should look heavier than the firing ship"
    );

    step(&mut app, 0.72);
    assert!(
        app.world_mut().query_filtered::<Entity, With<PlanetFissure>>().iter(app.world()).count()
            > 30,
        "the surface should split into several segmented and branching paths"
    );
}

#[test]
fn exiting_during_camera_jolt_restores_exact_position() {
    use bevy::ecs::system::RunSystemOnce;
    let mut app = app();
    let origin = Vec3::new(21., -63., 1000.);
    let camera = app
        .world_mut()
        .spawn((
            MainCamera,
            Transform::from_translation(origin),
            Projection::Orthographic(OrthographicProjection::default_2d()),
        ))
        .id();
    let mut ray = Cinematic::new(Vec3::Y * 200., origin, Vec2::splat(900.), 100., true);
    assert_eq!(ray.target.z, ray.origin.z, "camera depth must not stretch the beam");
    ray.elapsed = 3.8;
    app.world_mut().spawn(ray);
    app.world_mut().run_system_once(shake_combat_camera).unwrap();
    assert_ne!(app.world().get::<Transform>(camera).unwrap().translation, origin);
    app.world_mut().run_system_once(restore_combat_camera).unwrap();
    assert!(app.world().get::<Transform>(camera).unwrap().translation.abs_diff_eq(origin, 0.0001));
    assert!(app.world().get::<CombatCameraMotion>(camera).is_none());
}

/// Explicit GPU visual review, kept out of headless CI. Writes to ignored build output.
#[test]
#[ignore = "renders combat review frames with a local GPU"]
#[cfg(target_os = "windows")]
fn render_combat_effects_preview() {
    use bevy::camera::RenderTarget;
    use bevy::render::{
        render_resource::TextureUsages,
        view::screenshot::{save_to_disk, Screenshot},
        RenderPlugin,
    };
    use bevy::time::TimeUpdateStrategy;
    use bevy::window::ExitCondition;
    use bevy::winit::WinitPlugin;

    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: ExitCondition::DontExit,
                ..default()
            })
            .set(RenderPlugin {
                synchronous_pipeline_compilation: true,
                ..default()
            })
            .disable::<WinitPlugin>(),
    )
    .init_resource::<Settings>()
    .add_message::<SpawnShotMsg>()
    .add_message::<PlayAudioMsg>()
    .insert_resource(ClearColor(Color::srgb(0.015, 0.025, 0.055)))
    .insert_resource(TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f32(
        1. / 60.,
    )))
    .add_systems(Update, run_combat_animations);
    app.finish();
    app.cleanup();
    let mut render_image = Image::new_uninit(
        Extent3d {
            width: 1280,
            height: 800,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    );
    render_image.texture_descriptor.usage |= TextureUsages::RENDER_ATTACHMENT;
    let target = app.world_mut().resource_mut::<Assets<Image>>().add(render_image);
    app.world_mut().spawn((Camera2d, RenderTarget::Image(target.clone().into())));
    let load = |app: &mut App, path: &str| {
        let data = image::open(path).unwrap();
        let mut image = Image::from_dynamic(data, true, RenderAssetUsages::default());
        image.sampler = bevy::image::ImageSampler::linear();
        app.world_mut().resource_mut::<Assets<Image>>().add(image)
    };
    app.init_asset::<bevy_kira_audio::AudioSource>().init_resource::<WorldAssets>();
    use bevy::ecs::system::RunSystemOnce;
    app.world_mut()
        .run_system_once(
            |mut art: ResMut<WorldAssets>,
             server: Res<AssetServer>,
             mut layouts: ResMut<Assets<TextureAtlasLayout>>| {
                art.begin_gameplay_loading(&server, &mut layouts);
            },
        )
        .unwrap();
    let atlas = app.world().resource::<WorldAssets>().texture("explosion").image;
    let pixels = image::open("assets/images/animations/explosion.png").unwrap();
    app.world_mut()
        .resource_mut::<Assets<Image>>()
        .insert(atlas.id(), Image::from_dynamic(pixels, true, RenderAssetUsages::default()))
        .unwrap();
    let destroyed = load(&mut app, "assets/images/planets/destroyed bg.png");
    app.world_mut().resource_mut::<WorldAssets>().images.insert("destroyed bg".into(), destroyed);
    let backdrop = load(&mut app, "assets/images/planets/blue large.png");
    app.world_mut().spawn((
        Sprite {
            image: backdrop,
            custom_size: Some(Vec2::new(1280., 800.)),
            ..default()
        },
        Transform::from_xyz(0., 0., 10.),
        BackgroundImageCmp,
    ));
    let kinds = [
        (Unit::Ship(Ship::Battleship), "ships/battleship", "BATTLESHIP"),
        (Unit::Ship(Ship::HeavyFighter), "ships/heavy fighter", "HEAVY FTR"),
        (Unit::Defense(Defense::LightLaser), "defense/light laser", "LIGHT LASER"),
        (Unit::Defense(Defense::HeavyLaser), "defense/heavy laser", "HEAVY LASER"),
        (Unit::Defense(Defense::PlasmaTurret), "defense/plasma turret", "PLASMA"),
        (Unit::Defense(Defense::IonCannon), "defense/ion cannon", "ION"),
        (Unit::Ship(Ship::WarSun), "ships/war sun", "WAR SUN"),
        (Unit::Defense(Defense::SpaceDock), "defense/space dock", "SPACE DOCK"),
        (Unit::Defense(Defense::GaussCannon), "defense/gauss cannon", "GAUSS"),
    ];
    let mut sources = Vec::new();
    let mut defenders = Vec::new();
    for (index, (kind, path, label)) in kinds.into_iter().enumerate() {
        let x = (index as f32 - 4.) * 140.;
        let art = load(&mut app, &format!("assets/images/{path}.png"));
        let source = unit(&mut app, kind, Side::Attacker, Vec3::new(x, 240., 11.), 1000, 500);
        app.world_mut().get_mut::<Sprite>(source).unwrap().image = art.clone();
        let defender = unit(&mut app, kind, Side::Defender, Vec3::new(x, -190., 11.), 1000, 500);
        app.world_mut().get_mut::<Sprite>(defender).unwrap().image = art;
        sources.push((source, kind, Vec3::new(x, 240., 11.)));
        defenders.push(defender);
        app.world_mut().spawn((
            Text2d::new(label),
            TextFont {
                font_size: 18.0.into(),
                ..default()
            },
            TextColor(Color::srgb(0.7, 0.8, 0.9)),
            Transform::from_xyz(x, 320., 13.),
            CombatCmp,
        ));
    }
    // Warm render pipelines before sampling the effects clock.
    for _ in 0..20 {
        app.update();
    }
    for source in &sources {
        for i in 0..6 {
            app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
                source: Some(*source),
                side: Side::Defender,
                repair: false,
                shot: ShotReport {
                    unit: Some(source.1),
                    hull_damage: 30,
                    shield_damage: 85,
                    missed: i == 5,
                    ..default()
                },
            });
        }
    }
    std::fs::create_dir_all("target/combat-preview").unwrap();
    for frame in 0..190 {
        if frame == 90 {
            app.world_mut().entity_mut(defenders[4]).insert(Wreck::new(
                Vec3::new(0., -190., 11.),
                100.,
                Unit::Defense(Defense::PlasmaTurret),
            ));
            let source = sources[1];
            app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
                source: Some(source),
                side: Side::Defender,
                repair: true,
                shot: ShotReport {
                    unit: Some(Unit::Ship(Ship::HeavyFighter)),
                    hull_damage: 50,
                    ..default()
                },
            });
        }
        if [20, 42, 60, 82, 110, 175].contains(&frame) {
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(save_to_disk(format!("target/combat-preview/frame-{frame}.png")));
        }
        app.update();
    }
    let clear_scene = |app: &mut App| {
        let entities = app
            .world_mut()
            .query_filtered::<Entity, With<CombatCmp>>()
            .iter(app.world())
            .collect::<Vec<_>>();
        for entity in entities {
            if app.world().get_entity(entity).is_ok() {
                app.world_mut().despawn(entity);
            }
        }
    };
    clear_scene(&mut app);
    let bomber_kind = Unit::Ship(Ship::Bomber);
    let bomber_pos = Vec3::new(-200., 240., 11.);
    let bomber = unit(&mut app, bomber_kind, Side::Attacker, bomber_pos, 1000, 0);
    let art = load(&mut app, "assets/images/ships/bomber.png");
    app.world_mut().get_mut::<Sprite>(bomber).unwrap().image = art;
    for (index, building) in Unit::resource_buildings().into_iter().enumerate() {
        let position = Vec3::new((index as f32 - 1.) * 260., -160., 11.);
        let target_unit = unit(&mut app, building, Side::Defender, position, 5, 0);
        let art =
            load(&mut app, &format!("assets/images/buildings/{}.png", building.to_lowername()));
        app.world_mut().get_mut::<Sprite>(target_unit).unwrap().image = art;
        for _ in 0..if index == 0 {
            2
        } else {
            1
        } {
            app.world_mut().resource_mut::<Messages<SpawnShotMsg>>().write(SpawnShotMsg {
                source: Some((bomber, bomber_kind, bomber_pos)),
                side: Side::Defender,
                repair: false,
                shot: ShotReport {
                    unit: Some(building),
                    killed: index != 1,
                    missed: index == 1,
                    ..default()
                },
            });
        }
    }
    for frame in 0..150 {
        if [30, 50, 75, 100].contains(&frame) {
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(save_to_disk(format!("target/combat-preview/bombing-{frame}.png")));
        }
        app.update();
    }
    clear_scene(&mut app);
    let sun = unit(&mut app, Unit::war_sun(), Side::Attacker, Vec3::new(300., 240., 11.), 1000, 0);
    let art = load(&mut app, "assets/images/ships/war sun.png");
    app.world_mut().get_mut::<Sprite>(sun).unwrap().image = art;
    app.world_mut().spawn((
        Cinematic::new(Vec3::new(300., 240., 11.), Vec3::ZERO, Vec2::new(1280., 800.), 100., true),
        CombatCmp,
    ));
    for frame in 0..390 {
        if [40, 95, 150, 190, 238, 330].contains(&frame) {
            app.world_mut()
                .spawn(Screenshot::image(target.clone()))
                .observe(save_to_disk(format!("target/combat-preview/death-ray-{frame}.png")));
        }
        app.update();
    }
    for file in ["frame-20.png", "frame-82.png", "death-ray-95.png", "death-ray-238.png"] {
        assert!(std::path::Path::new("target/combat-preview").join(file).exists());
    }
}
