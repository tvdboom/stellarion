use super::*;
use crate::core::map::icon::Icon;
use crate::core::missions::Mission;
use crate::core::simulation::MAX_COMMANDS_PER_SUBMISSION;
use crate::core::units::Unit;

/// Exercises the mission command system with a valid fleet and optional full command draft.
fn launch_mission(draft_full: bool) -> App {
    let mut map = Map::new(2, 0);
    let origin_id = map.planets[0].id;
    let army = Army::from([(Unit::probe(), 1)]);
    map.planets[0].army = army.clone();
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
    let mut pending = PendingTurnCommands::default();
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
        .insert_resource(pending)
        .init_resource::<Missions>()
        .add_message::<SendMissionMsg>()
        .add_message::<MessageMsg>()
        .add_message::<PlayAudioMsg>()
        .add_systems(Update, send_mission);
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
