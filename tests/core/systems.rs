use bevy::ecs::system::RunSystemOnce;

use super::*;
use crate::core::ui::systems::MapRangePreview;

#[test]
fn ctrl_tab_recenters_after_manual_selection_with_the_shop_open() {
    use crate::core::camera::move_camera;
    use crate::core::map::systems::{select_planet, PlanetCmp};
    use crate::core::simulation::{GameModel, GameRules};

    for reverse in [false, true] {
        let mut model = GameModel::new([9; 32], GameRules::default()).unwrap();
        let player = model.players[0].clone();
        model.map.planets.truncate(3);
        for (index, planet) in model.map.planets.iter_mut().enumerate() {
            planet.name = format!("Planet {index}");
            planet.owned = Some(player.id);
            planet.position = Vec2::new(index as f32 * 500.0, 0.0);
        }
        let mut state = UiState::default();
        select_planet(&model.map.planets[0], &mut state, &player);
        assert!(!state.to_selected);
        let shop = state.shop;
        let target = &model.map.planets[if reverse {
            2
        } else {
            1
        }];
        let target_id = target.id;
        let target_position = target.position;

        let mut keyboard = ButtonInput::default();
        keyboard.press(KeyCode::ControlLeft);
        keyboard.press(KeyCode::Tab);
        if reverse {
            keyboard.press(KeyCode::ShiftLeft);
        }
        let mut app = App::new();
        app.insert_resource(keyboard)
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<bevy_egui::EguiUserTextures>()
            .add_message::<bevy::input::mouse::MouseWheel>()
            .insert_resource(Settings {
                show_menu: true,
                ..default()
            })
            .insert_resource(state)
            .insert_resource(player);
        for planet in &model.map.planets {
            app.world_mut().spawn((
                Transform::from_translation(planet.position.extend(0.0)),
                PlanetCmp {
                    id: planet.id,
                },
            ));
        }
        app.insert_resource(model.map);
        app.world_mut().spawn(Window::default());
        let camera = app.world_mut().spawn((Camera2d, MainCamera)).id();

        app.world_mut().run_system_once(check_keys).unwrap();
        let state = app.world().resource::<UiState>();
        assert_eq!(state.planet_selected, Some(target_id));
        assert_eq!(state.shop, shop);
        assert!(app.world().resource::<Settings>().show_menu);
        // Exercise the camera consumer too: updating the shop alone is insufficient.
        for _ in 0..256 {
            app.world_mut().run_system_once(move_camera).unwrap();
        }
        let position = app.world().get::<Transform>(camera).unwrap().translation.truncate();
        assert!(position.distance(target_position) < 0.75, "camera stayed at {position:?}");
    }
}

#[test]
fn escape_closes_mission_and_combat_details_without_opening_the_menu() {
    let mut keyboard = ButtonInput::default();
    keyboard.press(KeyCode::Escape);

    let mut app = App::new();
    app.insert_resource(State::new(AppState::Game))
        .insert_resource(State::new(GameState::Playing))
        .init_resource::<NextState<AppState>>()
        .init_resource::<NextState<GameState>>()
        .insert_resource(keyboard)
        .insert_resource(UiState {
            mission: true,
            mission_tab: MissionTab::MissionReports,
            mission_report: Some(42),
            combat_report: Some(7),
            ..default()
        })
        .add_message::<StartTurnMsg>()
        .add_message::<MultiplayerRequest>();

    app.world_mut().run_system_once(check_keys_menu).unwrap();

    let state = app.world().resource::<UiState>();
    assert_eq!(state.combat_report, None);
    assert!(!state.mission);
    assert!(matches!(*app.world().resource::<NextState<GameState>>(), NextState::Unchanged));
}

#[test]
fn escape_discards_attack_invitee_choices_but_keeps_the_mission_open() {
    let mut keyboard = ButtonInput::default();
    keyboard.press(KeyCode::Escape);
    let mut app = App::new();
    app.insert_resource(State::new(AppState::Game))
        .insert_resource(State::new(GameState::Playing))
        .init_resource::<NextState<AppState>>()
        .init_resource::<NextState<GameState>>()
        .insert_resource(keyboard)
        .insert_resource(UiState {
            mission: true,
            joint_attack_invitees: [2].into(),
            joint_attack_invite_selection: Some([3].into()),
            ..default()
        })
        .add_message::<StartTurnMsg>()
        .add_message::<MultiplayerRequest>();
    app.world_mut().run_system_once(check_keys_menu).unwrap();
    let state = app.world().resource::<UiState>();
    assert!(state.mission);
    assert!(state.joint_attack_invite_selection.is_none());
    assert_eq!(state.joint_attack_invitees, [2].into());
}

#[test]
fn escape_closes_trade_without_discarding_an_unpublished_offer() {
    let mut keyboard = ButtonInput::default();
    keyboard.press(KeyCode::Escape);
    let mut app = App::new();
    app.insert_resource(State::new(AppState::Game))
        .insert_resource(State::new(GameState::Playing))
        .init_resource::<NextState<AppState>>()
        .init_resource::<NextState<GameState>>()
        .insert_resource(keyboard)
        .insert_resource(UiState {
            trade_open: Some(11),
            trade_draft_id: Some(11),
            trade_resources: crate::core::resources::Resources::new(123, 0, 0),
            ..default()
        })
        .add_message::<StartTurnMsg>()
        .add_message::<MultiplayerRequest>();
    app.world_mut().run_system_once(check_keys_menu).unwrap();
    let state = app.world().resource::<UiState>();
    assert!(state.trade_open.is_none());
    assert_eq!(state.trade_draft_id, Some(11));
    assert_eq!(state.trade_resources.metal, 123);
    assert!(matches!(*app.world().resource::<NextState<GameState>>(), NextState::Unchanged));
}

#[test]
fn escape_closes_confirmation_without_closing_the_planet_or_opening_the_menu() {
    for (abandon_confirmation, colonize_confirmation, railgun_confirmation) in
        [(Some(2), None, None), (None, Some(4), None), (None, None, Some(3))]
    {
        let mut keyboard = ButtonInput::default();
        keyboard.press(KeyCode::Escape);

        let mut app = App::new();
        app.insert_resource(State::new(AppState::Game))
            .insert_resource(State::new(GameState::Playing))
            .init_resource::<NextState<AppState>>()
            .init_resource::<NextState<GameState>>()
            .insert_resource(keyboard)
            .insert_resource(UiState {
                planet_selected: Some(1),
                abandon_confirmation,
                colonize_confirmation,
                railgun_confirmation,
                ..default()
            })
            .add_message::<StartTurnMsg>()
            .add_message::<MultiplayerRequest>();

        app.world_mut().run_system_once(check_keys_menu).unwrap();

        let state = app.world().resource::<UiState>();
        assert_eq!(state.abandon_confirmation, None);
        assert_eq!(state.colonize_confirmation, None);
        assert_eq!(state.railgun_confirmation, None);
        assert_eq!(state.planet_selected, Some(1));
        assert!(matches!(*app.world().resource::<NextState<GameState>>(), NextState::Unchanged));
    }
}

#[test]
fn modal_menus_block_map_picking_clear_hover_and_restore_input_on_resume() {
    let mut app = App::new();
    app.insert_resource(UiState {
        planet_hover: Some(1),
        world_shortcut_hover: Some(4),
        mission_planet_hover: Some(3),
        range_preview: Some(MapRangePreview::SensorPhalanx(1)),
        planet_selected: Some(2),
        mission_hover: Some(5),
        mission_hover_from_ui: true,
        ..default()
    });
    let window = app
        .world_mut()
        .spawn((Window::default(), CursorIcon::from(SystemCursorIcon::Pointer)))
        .id();
    app.add_systems(Update, suspend_gameplay_interactions);

    app.update();
    app.update(); // Switching from the pause menu to Settings must not duplicate the blocker.

    let mut blockers =
        app.world_mut().query_filtered::<(Entity, &Pickable), With<GameplayInputBlocker>>();
    let (blocker, pickable) = blockers.single(app.world()).unwrap();
    assert!(pickable.should_block_lower);
    assert!(!pickable.is_hoverable);
    let state = app.world().resource::<UiState>();
    assert_eq!(state.planet_hover, None);
    assert_eq!(state.world_shortcut_hover, None);
    assert_eq!(state.mission_planet_hover, None);
    assert_eq!(state.range_preview, None);
    assert_eq!(state.planet_selected, Some(2), "persistent selection is preserved");
    assert_eq!(state.mission_hover, None);
    assert!(!state.mission_hover_from_ui);
    assert_eq!(
        app.world().get::<CursorIcon>(window),
        Some(&CursorIcon::from(SystemCursorIcon::Default))
    );

    app.world_mut().run_system_once(resume_gameplay_interactions).unwrap();
    assert!(app.world().get_entity(blocker).is_err());
}

#[cfg(debug_assertions)]
#[test]
fn ctrl_shift_up_queues_the_testing_boost_in_a_real_match() {
    use crate::core::identity::{GameCode, GameId};
    use crate::core::messages::MessageMsg;
    use crate::core::simulation::{GameModel, GameRules, MatchStatus, PersistedGame};
    use crate::core::units::{Amount, Unit};
    use crate::multiplayer::model::GameRecord;

    let mut model = GameModel::new([9; 32], GameRules::default()).unwrap();
    model.start().unwrap();
    let home = model.players[0].home_planet;
    let initial_resources = model.players[0].resources;
    let mut keyboard = ButtonInput::default();
    keyboard.press(KeyCode::ControlLeft);
    keyboard.press(KeyCode::ShiftLeft);
    keyboard.press(KeyCode::ArrowUp);

    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: GameId::new("real-testing-shortcut"),
        code: GameCode::new("ABCDEF"),
        revision: 1,
        saved_at: 0,
        max_players: 2,
        status: MatchStatus::Active,
        persisted: PersistedGame::new(model.clone()),
        members: Vec::new(),
        submitted_players: Vec::new(),
    });
    assert!(!session.local_practice);

    let mut app = App::new();
    app.insert_resource(keyboard)
        .insert_resource(model.map.clone())
        .insert_resource(model.players[0].clone())
        .insert_resource(session)
        .insert_resource(PendingTurnCommands {
            turn: model.turn,
            ..default()
        })
        .add_message::<MessageMsg>();
    app.world_mut().run_system_once(debug_cheat_keys).unwrap();

    let pending = app.world().resource::<PendingTurnCommands>();
    assert!(matches!(
        pending.commands.as_slice(),
        [TurnCommand::PracticeBoost {
            owned_worlds_only: true
        }]
    ));
    let player = app.world().resource::<Player>();
    let map = app.world().resource::<Map>();
    assert_eq!(player.resources, initial_resources + 1_000usize);
    assert_eq!(map.get(home).army.amount(&Unit::war_sun()), 3);
}
