use bevy::ecs::system::RunSystemOnce;

use super::*;
use crate::core::combat::plan::{BattlePlan, CombatTactic};
use crate::core::identity::{GameCode, GameId};
use crate::core::map::icon::Icon;
use crate::core::messages::MessageLevel;
use crate::core::simulation::{GameModel, GameRules, PersistedGame};
use crate::core::systems::testing_boost_keys;
use crate::multiplayer::client::SubmissionState;
use crate::multiplayer::model::GameRecord;

fn launched_mission_app(submission: SubmissionState) -> App {
    let mut model = GameModel::new([73; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let origin = model.players[0].home_planet;
    model.map.get_mut(origin).army.insert(Unit::war_sun(), 1);
    model.players[0].resources.deuterium = 100_000;
    let mission = Mission::new_with_id(
        77,
        model.turn as usize,
        1,
        model.map.get(origin),
        model.map.get(model.players[1].home_planet),
        Icon::Attack,
        Army::from([(Unit::war_sun(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: GameId::new("recall-shortcut"),
        code: GameCode::new("ABCDEF"),
        revision: 1,
        saved_at: 0,
        max_players: 2,
        status: model.status,
        persisted: PersistedGame::new(model.clone()),
        members: Vec::new(),
        submitted_players: Vec::new(),
    });
    let mut keyboard = ButtonInput::default();
    keyboard.press(KeyCode::ControlLeft);
    keyboard.press(KeyCode::ShiftLeft);
    keyboard.press(KeyCode::ArrowUp);
    let mut app = App::new();
    app.insert_resource(model.map)
        .insert_resource(model.players[0].clone())
        .insert_resource(Settings {
            turn: model.turn as usize,
            ..default()
        })
        .insert_resource(session)
        .insert_resource(keyboard)
        .insert_resource(PendingTurnCommands {
            turn: model.turn,
            submission,
            ..default()
        })
        .init_resource::<Missions>()
        .add_message::<SendMissionMsg>()
        .add_message::<RecallMissionMsg>()
        .add_message::<MissionRecallAnimationMsg>()
        .add_message::<MessageMsg>()
        .add_message::<PlayAudioMsg>()
        .add_message::<crate::multiplayer::client::MultiplayerRequest>();
    app.world_mut().write_message(SendMissionMsg::new(mission));
    app.world_mut().run_system_once(send_mission).unwrap();
    assert_eq!(app.world().resource::<Missions>().0.len(), 1);
    app
}

fn resume_draft_and_boost(app: &mut App) {
    {
        let mut pending = app.world_mut().resource_mut::<PendingTurnCommands>();
        let queued = std::mem::take(&mut pending.queued_commands);
        pending.commands.extend(queued);
        pending.submission = SubmissionState::Draft;
        pending.resume_requested = false;
    }
    let resources = app.world().resource::<Player>().resources;
    let home = app.world().resource::<Player>().home_planet;
    let ships = app.world().resource::<Map>().get(home).army.amount(&Unit::war_sun());
    app.world_mut().run_system_once(testing_boost_keys).unwrap();
    assert_eq!(app.world().resource::<Player>().resources, resources + 1_000usize);
    assert_eq!(app.world().resource::<Map>().get(home).army.amount(&Unit::war_sun()), ships + 3);
    assert!(matches!(
        app.world().resource::<PendingTurnCommands>().commands.last(),
        Some(TurnCommand::PracticeBoost)
    ));
    let messages =
        app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
    assert!(messages.iter().all(|message| message.level != MessageLevel::Error));
}

#[test]
fn ctrl_shift_up_works_after_canceling_a_launch_with_edited_battle_plans() {
    for submission in [SubmissionState::Draft, SubmissionState::Accepted] {
        let mut app = launched_mission_app(submission);
        let home = app.world().resource::<Player>().home_planet;
        {
            let mut pending = app.world_mut().resource_mut::<PendingTurnCommands>();
            for tactic in [CombatTactic::Siege, CombatTactic::Swarm] {
                assert!(pending.push(TurnCommand::SetMissionBattlePlan {
                    mission_id: 77,
                    battle_plan: BattlePlan {
                        automatic_tactic: false,
                        tactic,
                        ..default()
                    },
                }));
            }
            assert!(pending.push(TurnCommand::SetBattlePlan {
                planet_id: home,
                battle_plan: BattlePlan::default(),
            }));
        }
        app.world_mut().write_message(RecallMissionMsg::new(77));
        app.world_mut().run_system_once(recall_mission).unwrap();
        assert!(app.world().resource::<Missions>().0.is_empty());
        let pending = app.world().resource::<PendingTurnCommands>();
        let remaining = if submission == SubmissionState::Draft {
            assert!(pending.queued_commands.is_empty());
            &pending.commands
        } else {
            assert!(pending.commands.is_empty());
            &pending.queued_commands
        };
        assert!(
            matches!(remaining.as_slice(), [TurnCommand::SetBattlePlan { planet_id, .. }] if *planet_id == home)
        );
        resume_draft_and_boost(&mut app);
    }
}

#[test]
fn recalling_a_submitted_launch_keeps_the_frozen_payload_and_ordered_recall() {
    let mut app = launched_mission_app(SubmissionState::Draft);
    let frozen = {
        let mut pending = app.world_mut().resource_mut::<PendingTurnCommands>();
        assert!(pending.push(TurnCommand::SetMissionBattlePlan {
            mission_id: 77,
            battle_plan: BattlePlan::default(),
        }));
        pending.submission = SubmissionState::Accepted;
        let frozen = serde_json::to_value(&pending.commands).unwrap();
        assert!(pending.push(TurnCommand::SetMissionBattlePlan {
            mission_id: 77,
            battle_plan: BattlePlan {
                automatic_tactic: false,
                tactic: CombatTactic::Siege,
                ..default()
            },
        }));
        frozen
    };
    app.world_mut().write_message(RecallMissionMsg::new(77));
    app.world_mut().run_system_once(recall_mission).unwrap();
    assert!(app.world().resource::<Missions>().0.is_empty());
    let pending = app.world().resource::<PendingTurnCommands>();
    assert_eq!(serde_json::to_value(&pending.commands).unwrap(), frozen);
    assert!(matches!(
        pending.queued_commands.as_slice(),
        [
            TurnCommand::SetMissionBattlePlan {
                mission_id: 77,
                ..
            },
            TurnCommand::RecallMission {
                mission_id: 77
            }
        ]
    ));
    resume_draft_and_boost(&mut app);
}
