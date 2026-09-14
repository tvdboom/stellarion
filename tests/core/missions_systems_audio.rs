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
        .add_message::<crate::multiplayer::client::MultiplayerRequest>()
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
fn solo_dispatch_after_inviting_players_cancels_only_after_the_launch_is_accepted() {
    use crate::core::simulation::JointAttackContribution;
    use crate::multiplayer::client::MultiplayerRequest;
    use crate::multiplayer::model::{
        JointAttackInvitation, JointAttackParticipant, JointAttackResponse,
    };

    for (selected, succeeds) in [(2, true), (4, false)] {
        let mut map = Map::new(2, 0);
        let origin_id = map.planets[0].id;
        let destination_id = map.planets[1].id;
        let fighter = Unit::Ship(crate::core::units::ships::Ship::LightFighter);
        map.planets[0].army.insert(fighter, 3);
        map.planets[0].owned = Some(1);
        map.planets[0].controlled = Some(1);
        let mut player = Player::new(1, origin_id);
        player.resources.deuterium = 100_000;
        let fleet = Army::from([(fighter, selected)]);
        let mission = Mission::from_mission(
            1,
            player.id,
            map.get(origin_id),
            map.get(destination_id),
            &Mission {
                objective: Icon::Attack,
                army: fleet.clone(),
                ..default()
            },
        );
        let invitation = JointAttackInvitation {
            id: 99,
            revision: 0,
            turn: 1,
            inviter: player.id,
            destination: destination_id,
            objective: Icon::Attack,
            bombing: BombingRaid::None,
            combat_probes: false,
            canceled: false,
            launched: false,
            participants: vec![
                JointAttackParticipant {
                    player_id: player.id,
                    response: JointAttackResponse::Accepted,
                    contribution: Some(JointAttackContribution {
                        player_id: player.id,
                        origin: origin_id,
                        army: Army::from([(fighter, 2)]),
                        bombing: BombingRaid::None,
                        combat_probes: false,
                    }),
                },
                JointAttackParticipant {
                    player_id: 2,
                    response: JointAttackResponse::Pending,
                    contribution: None,
                },
            ],
        };
        let mut session = MultiplayerSession::default();
        session.joint_attacks.push(invitation);
        let mut app = App::new();
        app.insert_resource(map)
            .insert_resource(player)
            .insert_resource(session)
            .insert_resource(PendingTurnCommands {
                turn: 1,
                ..default()
            })
            .init_resource::<Missions>()
            .add_message::<SendMissionMsg>()
            .add_message::<MessageMsg>()
            .add_message::<PlayAudioMsg>()
            .add_message::<MultiplayerRequest>()
            .add_systems(Update, send_mission);
        app.world_mut().write_message(SendMissionMsg::solo_after_cancel(mission, 99));
        app.update();
        assert_eq!(app.world().resource::<Missions>().0.len() == 1, succeeds);
        assert_eq!(
            app.world_mut()
                .resource_mut::<Messages<MultiplayerRequest>>()
                .drain()
                .filter(|request| matches!(
                    request,
                    MultiplayerRequest::CancelJointAttack {
                        attack_id: 99
                    }
                ))
                .count(),
            usize::from(succeeds),
        );
        assert_eq!(app.world().resource::<PendingTurnCommands>().commands.len() == 1, succeeds,);
    }
}

#[test]
fn sending_allied_mission_projects_only_own_contingent_and_requests_publication() {
    use crate::core::identity::{GameCode, GameId};
    use crate::core::missions::JointMissionLaunch;
    use crate::core::simulation::{GameModel, GameRules, JointAttackContribution, PersistedGame};
    use crate::multiplayer::client::MultiplayerRequest;
    use crate::multiplayer::model::GameRecord;

    let mut model = GameModel::new(
        [71; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let fleet = Army::from([(Unit::war_sun(), 1)]);
    for player in model.players.iter_mut().take(2) {
        player.resources.deuterium = 100_000;
        model.map.get_mut(player.home_planet).army.extend(fleet.clone());
    }
    let contributions = model
        .players
        .iter()
        .take(2)
        .map(|player| JointAttackContribution {
            player_id: player.id,
            origin: player.home_planet,
            army: fleet.clone(),
            bombing: BombingRaid::None,
            combat_probes: false,
        })
        .collect::<Vec<_>>();
    let mission = Mission::new_with_id(
        77,
        model.turn as usize,
        1,
        model.map.get(contributions[0].origin),
        model.map.get(model.players[2].home_planet),
        Icon::Attack,
        fleet,
        BombingRaid::None,
        false,
        false,
        None,
    );
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: GameId::new("allied-launch"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 1_700_000_000,
        max_players: 3,
        status: model.status,
        persisted: PersistedGame::new(model.clone()),
        members: vec![],
        submitted_players: vec![],
    });
    let mut app = App::new();
    app.insert_resource(model.map.clone())
        .insert_resource(model.players[0].clone())
        .insert_resource(session)
        .insert_resource(PendingTurnCommands {
            turn: model.turn,
            ..default()
        })
        .init_resource::<Missions>()
        .add_message::<SendMissionMsg>()
        .add_message::<MessageMsg>()
        .add_message::<PlayAudioMsg>()
        .add_message::<MultiplayerRequest>()
        .add_systems(Update, send_mission);
    app.world_mut().write_message(SendMissionMsg::joint(
        mission,
        JointMissionLaunch {
            attack_id: 77,
            contributions,
        },
    ));
    app.update();
    let pending = app.world().resource::<PendingTurnCommands>();
    let expected = crate::core::simulation::preview_commands(&model, 1, &pending.commands).unwrap();
    let missions = app.world().resource::<Missions>();
    assert_eq!(missions.0.len(), 1);
    assert_eq!(
        missions.0.iter().map(|mission| (mission.id, mission.owner)).collect::<Vec<_>>(),
        vec![(77, 1)]
    );
    assert_eq!(
        serde_json::to_value(&missions.0).unwrap(),
        serde_json::to_value(crate::core::turns::filter_missions(
            &expected.missions,
            &expected.map,
            &expected.players[0],
        ))
        .unwrap()
    );
    for player in &model.players[..2] {
        assert_eq!(
            app.world().resource::<Map>().get(player.home_planet).army.amount(&Unit::war_sun()),
            0
        );
        assert_eq!(
            crate::core::turns::filter_missions(&expected.missions, &expected.map, player)
                .iter()
                .map(|mission| mission.owner)
                .collect::<Vec<_>>(),
            vec![player.id]
        );
    }
    assert_eq!(
        app.world().resource::<Player>().resources.deuterium,
        expected.players[0].resources.deuterium
    );
    assert_eq!(app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().count(), 1);
    let requests =
        app.world_mut().resource_mut::<Messages<MultiplayerRequest>>().drain().collect::<Vec<_>>();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::PublishJointMission]));
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
