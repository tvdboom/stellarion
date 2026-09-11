use rand::SeedableRng;

use super::*;
use crate::core::combat::report::CombatReport;
use crate::core::messages::MessageLevel;
use crate::core::missions::BombingRaid;
use crate::core::simulation::{GameModel, GameRules, MatchStatus, PersistedGame};
use crate::core::units::Army;
use crate::multiplayer::client::MultiplayerSession;
use crate::multiplayer::model::{GameMembership, GameRecord};
use bevy::ecs::system::RunSystemOnce;

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
#[test]
fn local_practice_end_turn_advances_the_displayed_game_after_testing_shortcuts() {
    use crate::core::loading::{
        refresh_gameplay_projection, refresh_turn_draft, PublicStructureChangeMsg,
    };
    use crate::core::simulation::{MatchStatus, TurnCommand};
    use crate::core::systems::debug_cheat_keys;
    use crate::multiplayer::client::tests::{local_practice_app, settle_local_practice};
    use crate::multiplayer::client::MultiplayerSession;
    use bevy::ecs::system::RunSystemOnce;

    let mut app = local_practice_app();
    app.add_plugins(AssetPlugin::default())
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .init_resource::<Settings>()
        .init_resource::<WorldAssets>()
        .init_resource::<UiState>()
        .init_resource::<NextState<GameState>>()
        .add_message::<StartTurnMsg>()
        .add_message::<MessageMsg>()
        .add_message::<PublicStructureChangeMsg>()
        .add_message::<PlayAudioMsg>()
        .add_systems(First, start_turn.run_if(resource_exists::<Map>))
        .add_systems(Update, (refresh_gameplay_projection, refresh_turn_draft).chain())
        .add_systems(PostUpdate, check_turn_ended);
    settle_local_practice(&mut app);

    for turn in 1..=3 {
        assert_eq!(app.world().resource::<Settings>().turn, turn);
        if turn == 2 {
            let mut keyboard = ButtonInput::default();
            keyboard.press(KeyCode::ControlLeft);
            keyboard.press(KeyCode::ShiftLeft);
            keyboard.press(KeyCode::ArrowUp);
            app.insert_resource(keyboard);
            app.world_mut().run_system_once(debug_cheat_keys).unwrap();
            let planet_id = app.world().resource::<Player>().home_planet;
            assert!(app.world_mut().resource_mut::<PendingTurnCommands>().push(
                TurnCommand::BuyUnits {
                    planet_id,
                    unit: Unit::war_sun(),
                    count: 1
                }
            ));
        }
        let old_resources = app.world().resource::<Player>().resources;
        app.world_mut().resource_mut::<UiState>().end_turn = true;
        app.update();
        settle_local_practice(&mut app);
        let session = app.world().resource::<MultiplayerSession>();
        let model = &session.active_game.as_ref().unwrap().persisted.state;
        assert_eq!(model.turn, (turn + 1) as u64);
        assert_eq!(model.status, MatchStatus::Active);
        assert_eq!(app.world().resource::<Settings>().turn, turn + 1);
        assert_eq!(app.world().resource::<Player>().resources, model.players[0].resources);
        assert_ne!(app.world().resource::<Player>().resources, old_resources);
        let pending = app.world().resource::<PendingTurnCommands>();
        assert!(pending.is_editable());
        assert_eq!(pending.turn, model.turn);
        if turn >= 2 {
            use crate::core::units::Amount;
            assert_eq!(
                app.world()
                    .resource::<Map>()
                    .get(model.players[0].home_planet)
                    .army
                    .amount(&Unit::war_sun()),
                4
            );
        }
    }
}

#[test]
fn end_turn_control_continues_a_ready_or_in_flight_turn() {
    for submission in [
        SubmissionState::Draft,
        SubmissionState::Sending,
        SubmissionState::Accepted,
        SubmissionState::ResumeRetry,
    ] {
        let mut app = App::new();
        app.insert_resource(UiState {
            end_turn: true,
            ..default()
        })
        .insert_resource(PendingTurnCommands {
            submission,
            ..default()
        })
        .add_message::<MultiplayerRequest>()
        .add_systems(Update, check_turn_ended);
        app.update();
        let pending = app.world().resource::<PendingTurnCommands>();
        assert_eq!(pending.resume_requested, submission != SubmissionState::Draft);
        assert!(!app.world().resource::<UiState>().end_turn);
        let requests = app.world().resource::<Messages<MultiplayerRequest>>();
        assert_eq!(requests.len(), usize::from(submission == SubmissionState::Draft));
    }
}

#[test]
/// Mission visibility always includes the owning player's commands.
fn owner_can_see_own_empty_mission_list() {
    let player = Player::default();
    let map = Map::new_with_rng(5, 0, &mut rand_chacha::ChaCha8Rng::from_seed([3; 32]));
    assert!(filter_missions(&[], &map, &player).is_empty());
}

#[test]
fn returning_probes_create_a_reports_panel_toast() {
    use crate::core::combat::resolution::resolve_combat_with_rng;

    let mut model = GameModel::new(
        [7; 32],
        GameRules {
            player_count: 1,
            practice_mode: true,
            ..default()
        },
    )
    .unwrap();
    model.start().unwrap();
    let turn = 3;
    let player_id = model.players[0].id;
    let destination = model.map.get(model.players[0].home_planet).clone();
    let origin = model
        .map
        .planets
        .iter()
        .find(|planet| planet.controlled != Some(player_id))
        .unwrap()
        .clone();
    let mission = Mission::new_with_id(
        77,
        turn - 1,
        player_id,
        &origin,
        &destination,
        Icon::Deploy,
        [(Unit::probe(), 2)].into_iter().collect(),
        default(),
        false,
        false,
        None,
    );
    let mut rng = rand_chacha::ChaCha8Rng::from_seed([4; 32]);
    let report = resolve_combat_with_rng(turn, &mission, &destination, &mut rng);
    assert!(report.hidden, "return arrivals stay out of the visible report list");
    model.players[0].push_report(report);

    let origin_name = origin.name;
    let mut app = App::new();
    app.add_plugins((TaskPoolPlugin::default(), AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<bevy_kira_audio::AudioSource>()
        .insert_resource(model.map)
        .insert_resource(model.players.remove(0))
        .insert_resource(Settings {
            turn,
            ..default()
        })
        .init_resource::<UiState>()
        .init_resource::<NextState<GameState>>()
        .init_resource::<WorldAssets>()
        .add_message::<StartTurnMsg>()
        .add_message::<MessageMsg>()
        .add_message::<PlayAudioMsg>()
        .add_message::<MultiplayerRequest>()
        .add_systems(Update, start_turn);
    app.world_mut().write_message(StartTurnMsg::new(true, true));
    app.update();

    let toasts = app.world_mut().resource_mut::<Messages<MessageMsg>>().drain().collect::<Vec<_>>();
    let returned = toasts
        .iter()
        .find(|toast| toast.message.contains("returned from"))
        .expect("the completed return trip should be announced");
    assert_eq!(returned.message, format!("Probes returned from planet {origin_name}."));
    assert_eq!(returned.action, Some(MessageAction::OpenMissionReports));
}

#[test]
fn known_enemy_one_planet_from_victory_creates_a_warning() {
    use crate::core::identity::{GameCode, GameId, UserId};

    let mut model = GameModel::new([29; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let local_player = model.players[0].clone();
    let enemy_id = model.players[1].id;
    let homes = model.players.iter().map(|player| player.home_planet).collect::<BTreeSet<_>>();
    let destroyed = model
        .map
        .planets
        .iter()
        .filter(|planet| !planet.is_moon() && !homes.contains(&planet.id))
        .map(|planet| planet.id)
        .take(10)
        .collect::<Vec<_>>();
    for planet_id in destroyed {
        model.map.get_mut(planet_id).destroy();
    }
    assert_eq!(model.planets_to_win(), 8);

    let visible_enemy_planets = model
        .map
        .planets
        .iter()
        .filter(|planet| {
            !planet.is_moon() && !planet.is_destroyed && planet.id != local_player.home_planet
        })
        .map(|planet| planet.id)
        .take(7)
        .collect::<Vec<_>>();
    for planet_id in visible_enemy_planets {
        let planet = model.map.get_mut(planet_id);
        planet.owned = Some(enemy_id);
        planet.controlled = Some(enemy_id);
        planet.army.insert(Unit::space_dock(), 1);
    }

    let game_id = GameId::new("territorial-threat");
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: game_id.clone(),
        code: GameCode::new("THREAT"),
        revision: 1,
        saved_at: 0,
        max_players: 2,
        status: MatchStatus::Active,
        persisted: PersistedGame::new(model.clone()),
        members: vec![GameMembership {
            game_id,
            player_id: enemy_id,
            user_id: UserId::new("enemy"),
            display_name: "Rival".into(),
            is_creator: false,
            identity_version: 1,
            connected: true,
        }],
        submitted_players: vec![],
    });

    let warnings = territorial_threat_notifications(&session, &model.map, &local_player);
    assert_eq!(warnings.len(), 1);
    assert_eq!(warnings[0].level, MessageLevel::Warning);
    assert_eq!(warnings[0].message, "Rival controls 7 of 8 planets needed for victory.");

    for planet in &mut model.map.planets {
        planet.army.remove(&Unit::space_dock());
    }
    assert!(
        territorial_threat_notifications(&session, &model.map, &local_player).is_empty(),
        "hidden control must not leak through the warning"
    );
}

#[test]
fn spy_report_notifications_distinguish_success_and_failure_and_open_the_report() {
    let mut model = GameModel::new([19; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = model.players[0].clone();
    let origin = model.map.get(player.home_planet).clone();
    let destination = model.map.get(model.players[1].home_planet).clone();
    let mission = Mission::new_with_id(
        42,
        1,
        player.id,
        &origin,
        &destination,
        Icon::Spy,
        Army::from([(Unit::probe(), 3)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let mut report = MissionReport {
        id: 9,
        turn: 2,
        mission,
        planet: destination.clone(),
        scout_probes: 2,
        surviving_attacker: Army::from([(Unit::probe(), 2)]),
        surviving_defender: destination.army.clone(),
        planet_colonized: false,
        planet_destroyed: false,
        destination_owned: destination.owned,
        destination_controlled: destination.controlled,
        combat_report: Some(CombatReport::default()),
        hidden: false,
    };

    let success = report_notification(&report, &player, &origin, &destination);
    assert_eq!(success.message, format!("Spy mission successful at planet {}.", destination.name));
    assert_eq!(success.level, MessageLevel::Info);
    assert_eq!(success.action, Some(MessageAction::OpenMissionReport(42)));

    report.scout_probes = 0;
    report.surviving_attacker = Army::new();
    let failed = report_notification(&report, &player, &origin, &destination);
    assert_eq!(
        failed.message,
        format!("Spy mission failed at planet {}; all probes were lost.", destination.name)
    );
    assert_eq!(failed.level, MessageLevel::Warning);
    assert_eq!(failed.action, Some(MessageAction::OpenMissionReport(42)));
}

#[test]
fn war_sun_fleets_use_the_destroy_icon_with_their_visible_route_treatment() {
    let player = Player::default();
    let mut mission = Mission {
        owner: player.id,
        objective: Icon::Attack,
        army: Army::from([(Unit::war_sun(), 1)]),
        jump_gate: true,
        ..default()
    };
    assert_eq!(mission.image(&player), "mission destroy jump");

    mission.objective = Icon::Deploy;
    mission.jump_gate = false;
    assert_eq!(mission.image(&player), "mission destroy");

    mission.army.clear();
    assert_eq!(mission.image(&player), "mission");
}

#[test]
fn only_colonize_objectives_and_their_returns_use_the_colony_icon() {
    let player = Player::default();
    let mut mission = Mission {
        owner: player.id,
        objective: Icon::Colonize,
        army: Army::from([(Unit::colony_ship(), 1), (Unit::war_sun(), 1)]),
        jump_gate: true,
        ..default()
    };

    // The objective is authoritative even when another special ship is present.
    assert_eq!(mission.image(&player), "mission colonize");

    mission.objective = Icon::Deploy;
    mission.army = Army::from([(Unit::colony_ship(), 2), (Unit::probe(), 0)]);
    assert_eq!(mission.image(&player), "mission jump");

    mission.jump_gate = false;
    assert_eq!(mission.image(&player), "mission");

    mission.return_objective = Some(Icon::Colonize);
    assert_eq!(mission.image(&player), "mission colonize");
}

#[test]
fn resolved_spy_and_destroy_returns_keep_their_outbound_silhouettes() {
    let player = Player::default();
    let returning_spy = Mission {
        owner: player.id,
        objective: Icon::Deploy,
        return_objective: Some(Icon::Spy),
        ..default()
    };
    let returning_destroy = Mission {
        return_objective: Some(Icon::Destroy),
        ..returning_spy.clone()
    };

    assert!(returning_spy.has_valid_return_objective());
    assert!(returning_destroy.has_valid_return_objective());
    assert_eq!(returning_spy.image(&player), "mission spy");
    assert_eq!(returning_destroy.image(&player), "mission destroy");
}

#[test]
fn planet_destruction_animation_only_starts_once_for_its_reported_turn() {
    let mut model = GameModel::new([23; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let player = model.players[0].clone();
    let origin = model.map.get(player.home_planet).clone();
    let destination = model.map.get(model.players[1].home_planet).clone();
    let mission = Mission::new_with_id(
        73,
        3,
        player.id,
        &origin,
        &destination,
        Icon::Destroy,
        Army::new(),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let report = MissionReport {
        id: 91,
        turn: 4,
        mission,
        planet: destination.clone(),
        scout_probes: 0,
        surviving_attacker: Army::new(),
        surviving_defender: Army::new().into(),
        planet_colonized: false,
        planet_destroyed: true,
        destination_owned: None,
        destination_controlled: None,
        combat_report: Some(CombatReport::default()),
        hidden: false,
    };
    let mut destroyed = destination;
    destroyed.destroy();
    let reports = [&report];
    let mut animating = BTreeSet::new();

    assert!(!should_start_planet_destruction(&destroyed, &reports, 3, &mut animating));
    assert!(should_start_planet_destruction(&destroyed, &reports, 4, &mut animating));
    assert!(!should_start_planet_destruction(&destroyed, &reports, 4, &mut animating));
}

#[test]
fn terminal_overlay_waits_until_planet_destruction_finishes() {
    let mut presentation = EndGamePresentation::default();
    presentation.request();
    let mut app = App::new();
    app.insert_resource(presentation).init_resource::<NextState<GameState>>();
    let explosion = app
        .world_mut()
        .spawn(ExplosionCmp {
            timer: Timer::from_seconds(0.1, TimerMode::Repeating),
            last_index: 12,
            planet: 0,
        })
        .id();

    app.world_mut().run_system_once(finish_end_game_presentation).unwrap();
    assert!(matches!(*app.world().resource::<NextState<GameState>>(), NextState::Unchanged));

    app.world_mut().despawn(explosion);
    app.world_mut().run_system_once(finish_end_game_presentation).unwrap();
    assert!(matches!(
        *app.world().resource::<NextState<GameState>>(),
        NextState::Pending(GameState::EndGame)
    ));
}
