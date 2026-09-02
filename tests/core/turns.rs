use rand::SeedableRng;

use super::*;

#[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
#[test]
fn local_practice_end_turn_advances_the_displayed_game_after_testing_shortcuts() {
    use crate::core::loading::{refresh_gameplay_projection, refresh_turn_draft};
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
