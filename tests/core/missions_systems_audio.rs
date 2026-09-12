use super::*;
use crate::core::constants::MIN_SPY_PROBES;
use crate::core::map::icon::Icon;
use crate::core::missions::{JointAttackMission, Mission};
use crate::core::simulation::MAX_COMMANDS_PER_SUBMISSION;
use crate::core::units::buildings::Building;
use crate::core::units::Unit;

/// Exercises the mission command system with a valid fleet and optional full command draft.
fn launch_mission(draft_full: bool) -> App {
    let mut map = Map::new(2, 0);
    let origin_id = map.planets[0].id;
    let army = Army::from([(Unit::probe(), MIN_SPY_PROBES)]);
    map.planets[0].army = army.clone().into();
    map.planets[0].army.insert(Unit::Building(Building::CommandRelay), Building::MAX_LEVEL);
    map.planets[0].controlled = Some(1);
    map.planets[0].owned = Some(1);
    let mission = Mission::from_mission(
        1,
        1,
        &map.planets[0],
        &map.planets[1],
        &Mission {
            army,
            objective: Icon::Spy,
            ..default()
        },
    );
    let mut pending = PendingTurnCommands {
        turn: 1,
        ..default()
    };
    if draft_full {
        pending.commands = vec![
            TurnCommand::BuyUnits {
                planet_id: origin_id,
                unit: Unit::probe(),
                count: 1
            };
            MAX_COMMANDS_PER_SUBMISSION
        ];
    }
    let mut app = App::new();
    app.insert_resource(map)
        .insert_resource(Player::new(1, origin_id))
        .insert_resource(Settings {
            turn: 1,
            ..default()
        })
        .insert_resource(pending)
        .init_resource::<Missions>()
        .add_message::<SendMissionMsg>()
        .add_message::<RecallMissionMsg>()
        .add_message::<MissionRecallAnimationMsg>()
        .add_message::<MessageMsg>()
        .add_message::<PlayAudioMsg>()
        .add_systems(Update, (send_mission, recall_mission));
    app.world_mut().write_message(SendMissionMsg::new(mission));
    app.update();
    app
}

#[test]
fn accepted_mission_plays_the_new_launch_once_and_keeps_a_silent_toast() {
    let mut app = launch_mission(false);
    assert_eq!(app.world().resource::<Missions>().0.len(), 1);
    let sounds: Vec<_> = app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().collect();
    assert_eq!(sounds.len(), 1);
    assert_eq!(sounds[0].name, "launch");
    assert!(!sounds[0].is_looped);
    assert!(!sounds[0].is_background);
    let notices: Vec<_> = app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].silent);
}

#[test]
fn rejected_mission_keeps_error_feedback_without_an_action_cue() {
    let mut app = launch_mission(true);
    assert!(app.world().resource::<Missions>().0.is_empty());
    assert_eq!(app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().count(), 0);
    let notices: Vec<_> = app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect();
    assert_eq!(notices.len(), 1);
    assert!(!notices[0].silent);
}

#[test]
fn stationed_protection_recall_bypasses_the_new_mission_editor() {
    let mut map = Map::new(3, 0);
    let home = map.planets[0].id;
    let protected = map.planets[1].id;
    let fleet = Army::from([(Unit::Ship(crate::core::units::ships::Ship::LightFighter), 3)]);
    map.get_mut(home).owned = Some(1);
    map.get_mut(home).controlled = Some(1);
    map.get_mut(protected).controlled = Some(2);
    map.get_mut(protected).army.dock_protector(1, fleet.clone());

    let mut app = App::new();
    app.insert_resource(map)
        .insert_resource(Player::new(1, home))
        .insert_resource(Settings {
            turn: 1,
            ..default()
        })
        .insert_resource(PendingTurnCommands {
            turn: 1,
            ..default()
        })
        .init_resource::<Missions>()
        .add_message::<RecallProtectionMsg>()
        .add_message::<MessageMsg>()
        .add_systems(Update, recall_protection);

    app.world_mut().write_message(RecallProtectionMsg::new(protected));
    app.update();

    assert!(app.world().resource::<Map>().get(protected).army.protector(1).is_none());
    let mission = &app.world().resource::<Missions>().0[0];
    assert_eq!((mission.origin, mission.destination), (protected, home));
    assert_eq!(mission.objective, Icon::Deploy);
    assert_eq!(mission.return_objective, Some(Icon::Protect));
    assert_eq!(mission.army, fleet);
    assert!(matches!(
        app.world().resource::<PendingTurnCommands>().commands.as_slice(),
        [TurnCommand::RecallProtection {
            planet_id,
            ..
        }] if *planet_id == protected
    ));
}

#[test]
fn recalling_a_just_launched_mission_erases_the_launch_without_an_animation() {
    let mut app = launch_mission(false);
    let mission_id = app.world().resource::<Missions>().0[0].id;
    let origin = app.world().resource::<Missions>().0[0].origin;
    let launched_army = app.world().resource::<Missions>().0[0].army.clone();
    let refunded_fuel =
        app.world().resource::<Missions>().0[0].fuel_consumption(app.world().resource::<Map>());
    let resources_after_launch = app.world().resource::<Player>().resources;
    app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().for_each(drop);
    app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().for_each(drop);

    app.world_mut().write_message(RecallMissionMsg::new(mission_id));
    app.update();

    assert!(app.world().resource::<Missions>().0.is_empty());
    assert!(app.world().resource::<PendingTurnCommands>().commands.is_empty());
    for (unit, count) in launched_army {
        assert_eq!(app.world().resource::<Map>().get(origin).army.amount(&unit), count);
    }
    assert_eq!(
        app.world().resource::<Player>().resources.deuterium,
        resources_after_launch.deuterium.saturating_add(refunded_fuel)
    );
    assert_eq!(app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().count(), 0);
    let notices =
        app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].silent);
    assert_eq!(
        app.world_mut().resource_mut::<Messages<MissionRecallAnimationMsg>>().drain().count(),
        0
    );

    app.world_mut().write_message(RecallMissionMsg::new(mission_id));
    app.update();
    assert!(app.world().resource::<PendingTurnCommands>().commands.is_empty());
    let notices =
        app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    assert!(!notices[0].silent);
    assert_eq!(
        app.world_mut().resource_mut::<Messages<MissionRecallAnimationMsg>>().drain().count(),
        0
    );
}

#[test]
fn missile_recall_is_rejected_without_changing_the_visible_mission_or_turn_draft() {
    let mut app = launch_mission(false);
    let mission_id = app.world().resource::<Missions>().0[0].id;
    app.world_mut().resource_mut::<Missions>().0[0].objective = Icon::MissileStrike;
    app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().for_each(drop);

    app.world_mut().write_message(RecallMissionMsg::new(mission_id));
    app.update();

    let mission = &app.world().resource::<Missions>().0[0];
    assert_eq!(mission.objective, Icon::MissileStrike);
    assert!(!mission.is_returning());
    assert_eq!(app.world().resource::<PendingTurnCommands>().commands.len(), 1);
    assert_eq!(
        app.world_mut().resource_mut::<Messages<MissionRecallAnimationMsg>>().drain().count(),
        0
    );
    let notices =
        app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    assert!(!notices[0].silent);
}

#[test]
fn allied_attack_recall_is_rejected_without_changing_the_visible_mission_or_turn_draft() {
    let mut app = launch_mission(false);
    let mission_id = app.world().resource::<Missions>().0[0].id;
    app.world_mut().resource_mut::<Missions>().0[0].joint_attack =
        Some(JointAttackMission::default());
    app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().for_each(drop);

    app.world_mut().write_message(RecallMissionMsg::new(mission_id));
    app.update();

    let mission = &app.world().resource::<Missions>().0[0];
    assert!(mission.joint_attack.is_some());
    assert!(!mission.is_returning());
    assert_eq!(app.world().resource::<PendingTurnCommands>().commands.len(), 1);
    assert_eq!(
        app.world_mut().resource_mut::<Messages<MissionRecallAnimationMsg>>().drain().count(),
        0
    );
    let notices =
        app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    assert!(!notices[0].silent);
}
