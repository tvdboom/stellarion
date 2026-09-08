use super::*;
use crate::core::constants::MIN_SPY_PROBES;
use crate::core::map::icon::Icon;
use crate::core::missions::Mission;
use crate::core::simulation::MAX_COMMANDS_PER_SUBMISSION;
use crate::core::units::buildings::Building;
use crate::core::units::Unit;

/// Exercises the mission command system with a valid fleet and optional full command draft.
fn launch_mission(draft_full: bool) -> App {
    let mut map = Map::new(2, 0);
    let origin_id = map.planets[0].id;
    let army = Army::from([(Unit::probe(), MIN_SPY_PROBES)]);
    map.planets[0].army = army.clone();
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
fn accepted_recall_updates_the_visible_route_and_turn_draft_once() {
    let mut app = launch_mission(false);
    let mission_id = app.world().resource::<Missions>().0[0].id;
    app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().for_each(drop);
    app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().for_each(drop);

    app.world_mut().write_message(RecallMissionMsg::new(mission_id));
    app.update();

    let mission = &app.world().resource::<Missions>().0[0];
    assert!(mission.is_returning());
    assert_eq!(mission.objective, Icon::Deploy);
    assert_eq!(mission.return_objective, Some(Icon::Spy));
    assert_eq!(app.world().resource::<PendingTurnCommands>().commands.len(), 2);
    assert!(matches!(
        app.world().resource::<PendingTurnCommands>().commands[1],
        TurnCommand::RecallMission { mission_id: id } if id == mission_id
    ));
    assert_eq!(app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().count(), 0);
    let notices =
        app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
    assert_eq!(notices.len(), 1);
    assert!(notices[0].silent);
    assert_eq!(
        app.world_mut().resource_mut::<Messages<MissionRecallAnimationMsg>>().drain().count(),
        1
    );

    app.world_mut().write_message(RecallMissionMsg::new(mission_id));
    app.update();
    assert_eq!(app.world().resource::<PendingTurnCommands>().commands.len(), 2);
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
