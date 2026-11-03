use super::*;
use crate::core::combat::report::CombatReport;
use crate::core::interception::{Intercept, InterceptPhase};
use crate::core::missions::{BombingRaid, Mission};
use crate::core::simulation::{GameModel, GameRules};
use crate::core::units::{ships::Ship, Army};
use std::time::Duration;

fn presentation() -> App {
    let mut model = GameModel::new([8; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .insert_resource(model.players[0].clone())
        .insert_resource(State::new(GameState::Playing))
        .insert_resource(Settings {
            turn: 2,
            ..default()
        })
        .init_resource::<InterceptSites>()
        .init_resource::<SuppressedMapMissions>()
        .init_resource::<WorldAssets>()
        .init_resource::<Missions>()
        .init_resource::<MultiplayerSession>()
        .init_resource::<Time>()
        .add_message::<PlayAudioMsg>()
        .add_systems(Startup, initialize)
        .add_systems(Update, (show, animate).chain());
    app.update();
    let origin = model.map.get(model.players[0].home_planet);
    let target = model.map.get(model.players[1].home_planet);
    let mission = Mission::new_with_id(
        9,
        1,
        1,
        origin,
        target,
        Icon::Intercept,
        Army::from([(Unit::Ship(Ship::LightFighter), 3)]),
        BombingRaid::None,
        true,
        false,
        None,
    );
    let mut report = crate::test_support::empty_report(mission, target.clone());
    report.id = 5;
    report.turn = 2;
    report.combat_report = Some(CombatReport::default());
    report.surviving_attacker = report.mission.army.clone();
    app.world_mut().resource_mut::<Player>().reports.push(report);
    app
}

#[test]
fn effects_spawn_once_pause_in_overlays_and_release_suppressed_fleets_on_cleanup() {
    let mut app = presentation();
    app.update();
    assert_eq!(app.world_mut().query::<&InterceptEffect>().iter(app.world()).count(), 0);
    app.update();
    let entity = app
        .world_mut()
        .query_filtered::<Entity, With<InterceptEffect>>()
        .single(app.world())
        .unwrap();
    let children = app.world().get::<Children>(entity).unwrap().to_vec();
    let sides = children
        .iter()
        .filter_map(|child| match app.world().get::<Part>(*child) {
            Some(Part::Ship {
                side,
                ..
            }) => Some(*side),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(sides, [0, 1], "one mission image represents each fleet");
    assert!(app.world().resource::<SuppressedMapMissions>().contains(9));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(1.5));
    app.update();
    let elapsed = app.world().get::<InterceptEffect>(entity).unwrap().timer.elapsed_secs();
    app.insert_resource(State::new(GameState::GameMenu));
    app.update();
    assert_eq!(app.world().get::<InterceptEffect>(entity).unwrap().timer.elapsed_secs(), elapsed);
    assert_eq!(*app.world().get::<Visibility>(entity).unwrap(), Visibility::Hidden);
    app.insert_resource(State::new(GameState::Playing));
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(2.2));
    app.update();
    for child in &children {
        assert!(app.world().get::<Mesh2d>(*child).is_none(), "no shock rings after combat");
        match app.world().get::<Part>(*child).unwrap() {
            Part::Ship {
                survives,
                ..
            }
            | Part::Exhaust {
                survives,
                ..
            } => {
                let alpha = app.world().get::<Sprite>(*child).unwrap().color.alpha();
                assert_eq!(
                    alpha,
                    if *survives {
                        1.0
                    } else {
                        0.0
                    }
                );
            },
            Part::Beam(_) => {
                assert_eq!(app.world().get::<Sprite>(*child).unwrap().color.alpha(), 0.0)
            },
            Part::Label => {},
        }
    }
    app.world_mut().resource_mut::<Settings>().turn = 3;
    app.update();
    assert!(app.world().get_entity(entity).is_err());
    assert!(children.iter().all(|child| app.world().get_entity(*child).is_err()));
    assert!(!app.world().resource::<SuppressedMapMissions>().contains(9));
    app.update();
    assert_eq!(app.world_mut().query::<&InterceptEffect>().iter(app.world()).count(), 0);
}

#[test]
fn dogfight_paths_are_continuous_finite_and_leave_the_encounter_after_combat() {
    for side in 0..2 {
        for boundary in [APPROACH, DEPARTURE] {
            let before = ship_position(side, boundary - 0.001);
            let at = ship_position(side, boundary);
            let after = ship_position(side, boundary + 0.001);
            assert!(before.distance(after) < 0.3);
            // Smooth banking keeps the ship and its exhaust from snapping between headings.
            assert!((at - before).normalize().dot((after - at).normalize()) > 0.999);
        }
        for frame in 0..420 {
            let t = frame as f32 / 60.0;
            let position = ship_position(side, t);
            assert!(position.is_finite());
            assert!(position.distance(ship_position(1 - side, t)) >= 63.9);
        }
        assert!(ship_position(side, 6.0).length() > ship_position(side, DEPARTURE).length());
    }
}

#[test]
fn dogfight_keeps_the_mission_artwork_even_when_the_target_is_destroyed() {
    for (objective, return_objective, army, image) in [
        (Icon::Attack, None, Army::from([(Unit::war_sun(), 1)]), "mission destroy"),
        (Icon::Colonize, None, Army::from([(Unit::colony_ship(), 1)]), "mission colonize"),
        // Transporting a Colony Ship still uses ordinary fleet artwork.
        (Icon::Deploy, None, Army::from([(Unit::colony_ship(), 1)]), "mission"),
        (Icon::Spy, None, Army::from([(Unit::probe(), 10)]), "mission spy"),
        (Icon::Deploy, Some(Icon::Spy), Army::from([(Unit::probe(), 5)]), "mission spy"),
    ] {
        let mut app = presentation();
        let mut report = app.world_mut().resource_mut::<Player>().reports.remove(0);
        report.mission.army.insert(Unit::war_sun(), 1);
        report.surviving_attacker = report.mission.army.clone();
        report.planet.army = army.clone().into();
        report.planet.controlled = Some(2);
        let target = Mission {
            id: 27,
            owner: 2,
            objective,
            return_objective,
            army,
            ..default()
        };
        report.mission.intercept = Some(Intercept {
            target: target.id,
            target_send: 1,
            point: report.planet.position,
            encounter_turn: 2,
            encounter_fraction: 0.5,
            phase: InterceptPhase::Outbound,
        });
        let expected_size = mission_size(&target, false) * 0.8;
        let expected_flip = mission_map_flip_x(&target);
        let mut handles = BTreeMap::new();
        for key in ["mission", "mission destroy", "mission spy", "mission colonize"] {
            let handle = app.world_mut().resource_mut::<Assets<Image>>().add(Image::default());
            app.world_mut().resource_mut::<WorldAssets>().images.insert(key.into(), handle.clone());
            handles.insert(key, handle);
        }
        app.world_mut().resource_mut::<Settings>().turn = 1;
        app.world_mut().resource_mut::<Missions>().0 = vec![target];
        app.update();

        // The new turn has a report but no surviving target in the visible mission projection.
        app.world_mut().resource_mut::<Settings>().turn = 2;
        app.world_mut().resource_mut::<Missions>().0.clear();
        app.world_mut().resource_mut::<Player>().reports.push(report);
        app.update();
        app.update();
        let ships = app
            .world_mut()
            .query::<(&Part, &Sprite)>()
            .iter(app.world())
            .filter_map(|(part, sprite)| match part {
                Part::Ship {
                    side,
                    ..
                } => Some((*side, sprite.clone())),
                _ => None,
            })
            .collect::<BTreeMap<_, _>>();
        assert_eq!(ships.len(), 2);
        assert_eq!(ships[&0].image, handles["mission destroy"]);
        assert_eq!(ships[&1].image, handles[image]);
        assert_eq!(ships[&1].custom_size, Some(Vec2::splat(expected_size)));
        assert_eq!(ships[&1].flip_x, expected_flip);
        assert!(app.world().resource::<SuppressedMapMissions>().contains(27));

        // Ships and engine flames disappear together, with no particle burst left behind.
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(3.6));
        app.update();
        for (part, sprite) in app.world_mut().query::<(&Part, &Sprite)>().iter(app.world()) {
            if matches!(
                part,
                Part::Ship {
                    side: 1,
                    ..
                } | Part::Exhaust {
                    side: 1,
                    ..
                } | Part::Beam(_)
            ) {
                assert_eq!(sprite.color.alpha(), 0.0);
            }
        }
        app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(DURATION));
        app.update();
        app.update();
        assert!(app.world_mut().query::<&Part>().iter(app.world()).next().is_none());
        let suppressed = app.world().resource::<SuppressedMapMissions>();
        assert!(!suppressed.contains(9) && !suppressed.contains(27));
    }
}
