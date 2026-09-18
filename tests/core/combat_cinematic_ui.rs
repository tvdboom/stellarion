use super::*;
use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::combat::systems::CombatCmp;
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::random::DeterministicRngState;
use crate::core::units::{ships::Ship, Army, Unit};
use bevy::ecs::system::RunSystemOnce;
use std::time::Duration;

fn app() -> App {
    let mut rng = DeterministicRngState::from_u64(423).next_rng();
    let mut origin = Planet::new_with_rng(0, "Origin".into(), Vec2::ZERO, false, 1.0, &mut rng);
    origin.colonize(1);
    let mut planet = Planet::new_with_rng(1, "Target".into(), Vec2::X, false, 1.0, &mut rng);
    planet.colonize(2);
    planet.army = Army::from([(Unit::Ship(Ship::LightFighter), 1)]).into();
    let mission = Mission::new_with_id(
        1,
        1,
        1,
        &origin,
        &planet,
        Icon::Attack,
        Army::from([(Unit::war_sun(), 1)]),
        BombingRaid::None,
        false,
        false,
        None,
    );
    let report = resolve_combat_with_rng(1, &mission, &planet, &mut rng);
    let report_id = report.id;
    let mut player = Player::new(1, 0);
    player.reports.push(report);
    let mut app = App::new();
    app.init_resource::<Time>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<Settings>()
        .insert_resource(player)
        .insert_resource(UiState {
            in_combat: Some(report_id),
            combat_view: CombatView::Cinematic,
            ..Default::default()
        })
        .add_message::<PlayAudioMsg>()
        .add_message::<StopAudioMsg>();
    app
}

fn audio(app: &mut App) -> Vec<&'static str> {
    app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().map(|cue| cue.name).collect()
}

fn advance(app: &mut App, seconds: f32) {
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(seconds));
    app.world_mut().run_system_once(advance_cinematic).unwrap();
}

#[test]
fn schematic_is_the_default_selection_and_cinematic_is_explicit() {
    let mut world = World::new();
    assert!(!world.run_system_once(cinematic_selected).unwrap());
    world.init_resource::<UiState>();
    assert_eq!(world.resource::<UiState>().combat_view, CombatView::Schematic);
    assert!(!world.run_system_once(cinematic_selected).unwrap());
    world.resource_mut::<UiState>().combat_view = CombatView::Cinematic;
    assert!(world.run_system_once(cinematic_selected).unwrap());
}

#[test]
fn setup_adds_movie_resources_without_schematic_entities_or_changing_the_report() {
    let mut app = app();
    let saved = serde_json::to_string(&app.world().resource::<Player>().reports).unwrap();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    assert!(app.world().contains_resource::<CinematicPlayback>());
    assert!(app.world().contains_resource::<CinematicSoundtrack>());
    let world = app.world_mut();
    assert_eq!(world.query_filtered::<Entity, With<CombatCmp>>().iter(world).count(), 0);
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.0);
    assert_eq!(serde_json::to_string(&app.world().resource::<Player>().reports).unwrap(), saved);
    assert_eq!(audio(&mut app), vec!["horn"]);
}

#[test]
fn shared_pause_and_speed_settings_control_the_one_movie_clock() {
    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    audio(&mut app);
    app.world_mut().resource_mut::<Settings>().combat_speed = 2.0;
    advance(&mut app, 0.25);
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.5);
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    advance(&mut app, 2.0);
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.5);
    assert!(audio(&mut app).is_empty());
    {
        let mut settings = app.world_mut().resource_mut::<Settings>();
        settings.combat_paused = false;
        settings.combat_speed = 0.25;
    }
    advance(&mut app, 1.0);
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.75);
}

#[test]
fn completion_emits_one_result_and_drops_expired_weapon_sounds() {
    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    audio(&mut app);
    let duration = app.world().resource::<CinematicPlayback>().timeline.duration;
    advance(&mut app, duration + 5.0);
    assert!(app.world().resource::<CinematicPlayback>().is_finished());
    assert_eq!(audio(&mut app), vec!["victory"]);
    advance(&mut app, 1.0);
    advance(&mut app, 1.0);
    assert!(audio(&mut app).is_empty());
}

#[test]
fn rewinding_rearms_audio_and_can_complete_a_second_time() {
    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    let first_cue_at = app.world().resource::<CinematicSoundtrack>().cues[0].0;
    let duration = app.world().resource::<CinematicPlayback>().timeline.duration;
    audio(&mut app);
    advance(&mut app, duration + 1.0);
    assert_eq!(audio(&mut app), vec!["victory"]);
    app.world_mut().resource_mut::<CinematicPlayback>().elapsed = 0.0;
    advance(&mut app, 0.0);
    assert_eq!(app.world().resource::<CinematicSoundtrack>().next, 0);
    advance(&mut app, first_cue_at);
    assert!(!audio(&mut app).is_empty());
    advance(&mut app, 0.0);
    assert!(audio(&mut app).is_empty());
    advance(&mut app, duration);
    assert_eq!(audio(&mut app), vec!["victory"]);
}

#[test]
fn restart_shortcut_replays_from_zero_even_when_paused_or_finished() {
    for (finished, paused) in [(false, false), (false, true), (true, false)] {
        let mut app = app();
        app.world_mut().run_system_once(setup_cinematic).unwrap();
        let report = serde_json::to_string(&app.world().resource::<Player>().reports).unwrap();
        let duration = app.world().resource::<CinematicPlayback>().timeline.duration;
        advance(
            &mut app,
            if finished {
                duration + 1.0
            } else {
                duration * 0.5
            },
        );
        {
            let mut settings = app.world_mut().resource_mut::<Settings>();
            settings.combat_paused = paused;
            settings.combat_speed = 2.0;
            settings.volume = 0.3;
        }
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ControlRight);
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::ArrowLeft);
        }
        advance(&mut app, 0.25);
        assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.0);
        assert!(!app.world().resource::<Settings>().combat_paused);
        assert_eq!(app.world().resource::<Settings>().combat_speed, 2.0);
        assert_eq!(app.world().resource::<Settings>().volume, 0.3);
        let soundtrack = app.world().resource::<CinematicSoundtrack>();
        assert_eq!((soundtrack.next, soundtrack.previous_time), (0, 0.0));
        let first_cue_at = soundtrack.cues[0].0;
        assert_eq!(audio(&mut app), vec!["horn"], "Discard queued sounds from the old playback");
        let stopped: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<StopAudioMsg>>()
            .drain()
            .map(|cue| cue.name)
            .collect();
        assert!(stopped.contains(&"horn") && stopped.contains(&"victory"));
        assert!(!stopped.contains(&"music") && !stopped.contains(&"drums"));
        // Holding the combination must not keep resetting the movie each frame.
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
        advance(&mut app, first_cue_at / 2.0);
        assert!(!audio(&mut app).is_empty(), "The original weapon cues must play again");
        advance(&mut app, duration);
        assert_eq!(audio(&mut app), vec!["victory"]);
        assert_eq!(
            serde_json::to_string(&app.world().resource::<Player>().reports).unwrap(),
            report
        );
    }
}

#[test]
fn restart_requires_both_modifiers_and_does_not_change_speed_on_release() {
    use crate::core::states::CombatState;
    use crate::core::systems::check_keys_combat;
    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    app.insert_resource(State::new(CombatState::Fire))
        .add_systems(Update, (check_keys_combat, advance_cinematic).chain());
    app.world_mut().resource_mut::<Settings>().combat_speed = 4.0;
    for modifiers in [vec![], vec![KeyCode::ControlLeft], vec![KeyCode::ShiftRight]] {
        app.world_mut().resource_mut::<CinematicPlayback>().elapsed = 2.0;
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        for key in modifiers.into_iter().chain([KeyCode::ArrowLeft]) {
            keys.press(key);
        }
        app.update();
        assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 2.0);
    }
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.reset_all();
        for key in [KeyCode::ControlLeft, KeyCode::ShiftRight, KeyCode::ArrowLeft] {
            keys.press(key);
        }
    }
    app.update();
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.0);
    {
        let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        keys.clear();
        keys.release_all();
    }
    app.update();
    assert_eq!(app.world().resource::<Settings>().combat_speed, 4.0);
}

#[test]
fn exit_discards_playback_and_soundtrack_so_the_next_report_starts_fresh() {
    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    advance(&mut app, 0.5);
    app.world_mut().run_system_once(exit_cinematic).unwrap();
    assert!(!app.world().contains_resource::<CinematicPlayback>());
    assert!(!app.world().contains_resource::<CinematicSoundtrack>());
    assert!(app.world().resource::<UiState>().in_combat.is_some());
    audio(&mut app);
    advance(&mut app, 1.0);
    assert!(audio(&mut app).is_empty());
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    assert_eq!(app.world().resource::<CinematicPlayback>().elapsed, 0.0);
    assert_eq!(app.world().resource::<CinematicSoundtrack>().next, 0);
}

#[test]
fn missing_selected_report_creates_no_movie_and_emits_no_audio() {
    let mut app = app();
    app.world_mut().resource_mut::<UiState>().in_combat = None;
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    assert!(!app.world().contains_resource::<CinematicPlayback>());
    assert!(!app.world().contains_resource::<CinematicSoundtrack>());
    advance(&mut app, 1.0);
    assert!(audio(&mut app).is_empty());
}

/// Exercise the production overlay and shared HUD together, without a GPU or backend.
fn controls_app() -> (App, egui::Context) {
    use crate::core::audio::{
        draw_audio_controls, update_audio, ChangeAudioMsg, MuteAudioMsg, PauseAudioMsg,
        StopAudioMsg, VolumeFeedbackMsg,
    };
    use crate::core::states::{AppState, AudioState};
    use bevy_egui::{EguiContext, EguiUserTextures, PrimaryEguiContext};

    let mut app = app();
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    app.init_resource::<EguiUserTextures>()
        .init_resource::<ImageIds>()
        .init_resource::<NextState<GameState>>()
        .init_resource::<NextState<AudioState>>()
        .insert_resource(State::new(AppState::Game))
        .insert_resource(State::new(GameState::Combat))
        .add_message::<ChangeAudioMsg>()
        .add_message::<VolumeFeedbackMsg>()
        .add_message::<PauseAudioMsg>()
        .add_message::<StopAudioMsg>()
        .add_message::<MuteAudioMsg>()
        .add_systems(Update, (draw_cinematic, draw_audio_controls, update_audio).chain());
    let mut context = EguiContext::default();
    let egui = context.get_mut().clone();
    app.world_mut().spawn((context, PrimaryEguiContext));
    (app, egui)
}

fn controls_frame(
    app: &mut App,
    context: &egui::Context,
    events: Vec<egui::Event>,
) -> Vec<egui::epaint::ClippedShape> {
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(640.0, 480.0),
            )),
            events,
            ..default()
        },
        |_| app.update(),
    );
    output.textures_delta.clear();
    output.shapes
}

fn label_rect(shapes: &[egui::epaint::ClippedShape], label: &str) -> Option<egui::Rect> {
    shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.job.text == label => {
            Some(text.galley.rect.translate(text.pos.to_vec2()))
        },
        _ => None,
    })
}

fn click_control(app: &mut App, context: &egui::Context, pos: egui::Pos2) {
    for pressed in [true, false] {
        controls_frame(
            app,
            context,
            vec![
                egui::Event::PointerMoved(pos),
                egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
    }
}

#[test]
fn cinematic_shared_hud_controls_volume_mute_and_speed_without_schematic_options() {
    use crate::core::states::AudioState;
    let (mut app, context) = controls_app();
    app.world_mut().resource_mut::<Settings>().volume = 0.4;
    for _ in 0..4 {
        controls_frame(&mut app, &context, vec![]);
    }
    let shapes = controls_frame(&mut app, &context, vec![]);
    for label in ["Play", "Pause", "Close", "Replay", "1×"] {
        assert!(label_rect(&shapes, label).is_none(), "bottom playback bar must be absent");
    }
    let scale = crate::core::ui::systems::viewport_ui_scale(egui::vec2(640.0, 480.0));
    let mut buttons: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle)
                if (circle.radius - 15.0 * scale).abs() < 0.1 && circle.center.y < 70.0 =>
            {
                Some(circle.center)
            },
            _ => None,
        })
        .collect();
    buttons.sort_by(|a, b| a.x.total_cmp(&b.x));
    assert_eq!(buttons.len(), 2, "shared settings and audio icons must be visible");
    let [gear, sound] = [buttons[0], buttons[1]];
    for _ in 0..3 {
        controls_frame(&mut app, &context, vec![egui::Event::PointerMoved(gear)]);
    }
    let settings_shapes = controls_frame(&mut app, &context, vec![]);
    assert!(label_rect(&settings_shapes, "Combat settings").is_some());
    let speed_label = label_rect(&settings_shapes, "Playback speed  1×").unwrap();
    assert!(label_rect(&settings_shapes, "Volley fire").is_none());
    assert!(label_rect(&settings_shapes, "Individual units").is_none());
    // Click the actual speed slider below its label, well beyond the current 1× position.
    click_control(&mut app, &context, speed_label.left_bottom() + egui::vec2(85.0, 15.0));
    assert!(app.world().resource::<Settings>().combat_speed > 1.0);
    let selected_speed = app.world().resource::<Settings>().combat_speed;
    controls_frame(
        &mut app,
        &context,
        vec![egui::Event::Key {
            key: egui::Key::ArrowLeft,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: egui::Modifiers {
                ctrl: true,
                shift: true,
                ..egui::Modifiers::NONE
            },
        }],
    );
    assert_eq!(
        app.world().resource::<Settings>().combat_speed,
        selected_speed,
        "Restart must not also adjust the focused speed slider"
    );

    for _ in 0..3 {
        controls_frame(&mut app, &context, vec![egui::Event::PointerMoved(sound)]);
    }
    let volume_shapes = controls_frame(&mut app, &context, vec![]);
    assert!(label_rect(&volume_shapes, "Volume  40%").is_some());
    assert!(label_rect(&volume_shapes, "Combat settings").is_none());
    click_control(&mut app, &context, sound);
    assert_eq!(app.world().resource::<Settings>().audio, AudioState::Mute);
    click_control(&mut app, &context, sound);
    assert_eq!(app.world().resource::<Settings>().audio, AudioState::NoMusic);
    controls_frame(
        &mut app,
        &context,
        vec![
            egui::Event::PointerMoved(egui::pos2(300.0, 400.0)),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Line,
                delta: egui::vec2(0.0, 1.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    assert_eq!(app.world().resource::<Settings>().volume, 0.5);
    controls_frame(&mut app, &context, vec![]);
    assert_eq!(app.world().resource::<Settings>().volume, 0.5, "scroll applies once");
}

#[test]
fn cinematic_pause_is_visible_and_completed_banner_returns_to_selection() {
    let (mut app, context) = controls_app();
    let victory_texture = egui::TextureId::User(700);
    app.world_mut().resource_mut::<ImageIds>().0.insert("victory".into(), victory_texture);
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    for _ in 0..3 {
        controls_frame(&mut app, &context, vec![]);
    }
    assert!(label_rect(&controls_frame(&mut app, &context, vec![]), "PAUSED").is_some());
    let duration = app.world().resource::<CinematicPlayback>().timeline.duration;
    app.world_mut().resource_mut::<CinematicPlayback>().elapsed = duration;
    for _ in 0..3 {
        controls_frame(&mut app, &context, vec![]);
    }
    let shapes = controls_frame(&mut app, &context, vec![]);
    assert!(label_rect(&shapes, "PAUSED").is_none());
    assert!(label_rect(&shapes, "Click to return to battle selection · Esc").is_none());
    app.world_mut().resource_mut::<Settings>().combat_paused = false;
    app.world_mut().resource_mut::<Time>().advance_by(Duration::from_secs_f32(0.75));
    let shapes = controls_frame(&mut app, &context, vec![]);
    let artwork_rect = |shapes: &[egui::epaint::ClippedShape]| {
        shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == victory_texture => {
                    Some(mesh.calc_bounds())
                },
                _ => None,
            })
            .unwrap()
    };
    let half = artwork_rect(&shapes);
    assert!(
        (half.width() - 640.0 * result_banner::artwork("victory").width_fraction * 0.5).abs() < 0.1
    );
    app.world_mut().resource_mut::<Settings>().combat_paused = true;
    let paused = artwork_rect(&controls_frame(&mut app, &context, vec![]));
    assert_eq!(paused, half, "result entrance must freeze with playback");
    app.world_mut().resource_mut::<Settings>().combat_paused = false;
    let full = artwork_rect(&controls_frame(&mut app, &context, vec![]));
    assert!((full.width() - half.width() * 2.0).abs() < 0.1);
    click_control(&mut app, &context, egui::pos2(320.0, 240.0));
    assert!(matches!(
        app.world().resource::<NextState<GameState>>(),
        NextState::Pending(GameState::CombatMenu)
    ));
}

#[test]
fn cinematic_shared_effect_textures_survive_replays_without_reallocation() {
    let (mut app, context) = controls_app();
    app.init_resource::<Assets<Image>>();
    for _ in 0..3 {
        controls_frame(&mut app, &context, vec![]);
    }
    let first_images = app.world().resource::<ImageIds>().0.clone();
    assert_eq!(first_images.len(), 5);
    assert_eq!(app.world().resource::<Assets<Image>>().len(), 5);
    app.world_mut().run_system_once(setup_cinematic).unwrap();
    controls_frame(&mut app, &context, vec![]);
    assert_eq!(app.world().resource::<ImageIds>().0, first_images);
    assert_eq!(app.world().resource::<Assets<Image>>().len(), 5);
}

#[test]
fn cinematic_identity_banners_keep_commander_order_and_shared_player_colors() {
    use crate::core::identity::{GameCode, GameId, UserId};
    use crate::core::missions::JointAttackMission;
    use crate::core::simulation::{GameModel, GameRules, PersistedGame};
    use crate::multiplayer::model::{GameMembership, GameRecord};

    let (mut app, context) = controls_app();
    // Area fade-in multiplies every painted color during its first 150 ms. This assertion
    // compares canonical player colors, so remove that unrelated transition from the fixture.
    context.global_style_mut(|style| style.animation_time = 0.0);
    let model = GameModel::new(
        [45; 32],
        GameRules {
            player_count: 3,
            ..default()
        },
    )
    .unwrap();
    let mut session = MultiplayerSession::default();
    session.active_game = Some(GameRecord {
        id: GameId::new("identity-test"),
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 0,
        max_players: 3,
        status: model.status,
        persisted: PersistedGame::new(model),
        members: (1..=3)
            .map(|player_id| GameMembership {
                game_id: GameId::new("identity-test"),
                player_id,
                user_id: UserId::new(format!("identity-{player_id}")),
                display_name: format!("Practice P{player_id}"),
                is_creator: player_id == 1,
                identity_version: 0,
                connected: true,
            })
            .collect(),
        submitted_players: vec![],
    });
    let attacker_color = session.player_color(3).color().to_color32();
    let ally_color = session.player_color(1).color().to_color32();
    app.insert_resource(session);
    let fighter = Unit::Ship(Ship::LightFighter);
    {
        let mut player = app.world_mut().resource_mut::<Player>();
        player.reports[0].mission.owner = 3;
        player.reports[0].mission.joint_attack = Some(JointAttackMission {
            attackers: [(1, Army::from([(fighter, 1)])), (3, Army::from([(fighter, 3)]))].into(),
            ..default()
        });
    }
    for _ in 0..4 {
        controls_frame(&mut app, &context, vec![]);
    }
    let shapes = controls_frame(&mut app, &context, vec![]);
    let attacker = label_rect(&shapes, "ATTACKER").unwrap();
    let defender = label_rect(&shapes, "DEFENDER").unwrap();
    assert!((attacker.top() - defender.top()).abs() < 0.1, "Both roles align below the HUD icons");
    assert!(attacker.right() < defender.left());
    assert!(attacker.top() > 52.0 * viewport_ui_scale(egui::vec2(640.0, 480.0)));
    assert!(
        label_rect(&shapes, "Practice P3").unwrap().top()
            < label_rect(&shapes, "Practice P1").unwrap().top()
    );
    assert!(label_rect(&shapes, "Practice P2").unwrap().left() > 320.0);
    let segments: Vec<_> = shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.rect.width() < 4.0 && rect.rect.left() < 100.0 => {
                Some(rect)
            },
            _ => None,
        })
        .collect();
    let commander = segments.iter().find(|rect| rect.fill == attacker_color).unwrap();
    let ally = segments.iter().find(|rect| rect.fill == ally_color).unwrap();
    assert!((commander.rect.height() / ally.rect.height() - 3.0).abs() < 0.01);
}

#[test]
fn cinematic_exit_matches_schematic_atlas_states_and_returns_before_completion() {
    let (mut app, context) = controls_app();
    let texture = egui::TextureId::User(900);
    app.world_mut().resource_mut::<ImageIds>().0.insert("long button".into(), texture);
    for _ in 0..4 {
        controls_frame(&mut app, &context, vec![]);
    }
    let viewport = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(640.0, 480.0));
    let expected = egui::Rect::from_min_max(egui::pos2(390.0, 402.0), egui::pos2(590.0, 450.0));
    assert_eq!(exit_button_rect(viewport), expected);
    let shapes = controls_frame(&mut app, &context, vec![]);
    assert!(expected.contains_rect(label_rect(&shapes, "Exit combat").unwrap()));
    let frame_uv = |shapes: &[egui::epaint::ClippedShape]| {
        shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Mesh(mesh) if mesh.texture_id == texture => {
                    Some((mesh.calc_bounds(), mesh.vertices[0].uv.y))
                },
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(frame_uv(&shapes), (expected, 0.0));
    controls_frame(&mut app, &context, vec![egui::Event::PointerMoved(expected.center())]);
    let hovered = controls_frame(&mut app, &context, vec![]);
    assert_eq!(frame_uv(&hovered), (expected, 0.5));
    assert!(!app.world().resource::<CinematicPlayback>().is_finished());
    click_control(&mut app, &context, expected.center());
    assert!(matches!(
        app.world().resource::<NextState<GameState>>(),
        NextState::Pending(GameState::CombatMenu)
    ));
}

#[test]
fn soundtrack_uses_recorded_bombs_penetrating_hull_gain_and_miss_pitch() {
    use crate::core::combat::cinematic_timeline::CinematicShot;
    use crate::core::combat::resolution::ShotReport;
    use crate::core::units::buildings::Building;

    let app = app();
    let mut movie = CinematicPlayback::new(&app.world().resource::<Player>().reports[0]);
    movie.timeline.actors[0].unit = Unit::Ship(Ship::Bomber);
    for actor in &mut movie.timeline.actors {
        actor.death_at = None;
    }
    movie.timeline.repairs.clear();
    movie.timeline.planet_attacks.clear();
    movie.timeline.shots = vec![
        CinematicShot {
            source: 0,
            target: None,
            launch_at: 2.0,
            impact_at: 2.3,
            outcome: ShotReport {
                unit: Some(Unit::Building(Building::MetalMine)),
                killed: true,
                ..Default::default()
            },
        },
        CinematicShot {
            source: 0,
            target: None,
            launch_at: 3.0,
            impact_at: 3.3,
            outcome: ShotReport {
                hull_damage: 5,
                shield_damage: 5,
                ..Default::default()
            },
        },
        CinematicShot {
            source: 0,
            target: None,
            launch_at: 4.0,
            impact_at: 4.3,
            outcome: ShotReport {
                missed: true,
                ..Default::default()
            },
        },
    ];
    let soundtrack = CinematicSoundtrack::new(&movie);
    let at = |time: f32| {
        soundtrack
            .cues
            .iter()
            .filter(|(at, _)| (*at - time).abs() < 0.001)
            .map(|(_, cue)| cue)
            .collect::<Vec<_>>()
    };
    let bomb = at(2.0);
    assert_eq!(bomb.len(), 1);
    assert_eq!((bomb[0].name, bomb[0].volume, bomb[0].playback_rate), ("missile fire", -9.0, 0.72));
    let penetrating = at(3.3);
    assert_eq!(penetrating.len(), 1, "penetration must not stack a shield cue over its hull cue");
    assert_eq!((penetrating[0].name, penetrating[0].volume), ("short explosion", -10.0));
    let miss = at(4.3);
    assert_eq!((miss[0].name, miss[0].playback_rate), ("missile miss", 1.45));
}

#[test]
fn soundtrack_omits_hidden_orbitals_and_unselected_buildings_without_mutating_events() {
    use crate::core::combat::cinematic_timeline::{CinematicRepair, CinematicShot};
    use crate::core::combat::resolution::ShotReport;
    use crate::core::units::buildings::Building;

    let app = app();
    let mut report = app.world().resource::<Player>().reports[0].clone();
    let orbital_unit = Unit::Building(Building::SolarSatellite);
    let building_unit = Unit::Building(Building::MetalMine);
    report.planet.army.insert(orbital_unit, 1);
    report.planet.army.insert(building_unit, 3);
    let mut movie = CinematicPlayback::new(&report);
    let find = |unit| movie.timeline.actors.iter().position(|actor| actor.unit == unit).unwrap();
    let source = find(Unit::war_sun());
    let target = find(Unit::Ship(Ship::LightFighter));
    let orbital = find(orbital_unit);
    let building = find(building_unit);
    assert!(movie.actor_visible(source) && movie.actor_visible(target));
    assert!(!movie.actor_visible(orbital) && !movie.actor_visible(building));
    for actor in &mut movie.timeline.actors {
        actor.death_at = None;
    }
    movie.timeline.planet_attacks.clear();
    movie.timeline.shots = [
        (orbital, Some(target)),
        (source, Some(orbital)),
        (source, Some(building)),
        (source, Some(target)),
        (source, None),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (source, target))| CinematicShot {
        source,
        target,
        launch_at: 2.0 + index as f32,
        impact_at: 2.3 + index as f32,
        outcome: ShotReport {
            hull_damage: usize::from(target.is_some()),
            planetary_shield_damage: usize::from(target.is_none()),
            ..Default::default()
        },
    })
    .collect();
    movie.timeline.actors[orbital].death_at = Some(7.0);
    movie.timeline.actors[target].death_at = Some(8.0);
    movie.timeline.actors[building].death_at = Some(9.0);
    movie.timeline.repairs = [
        (Some(orbital), target),
        (Some(source), orbital),
        (None, building),
        (Some(source), target),
        (None, target),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (source, target))| CinematicRepair {
        source,
        target,
        start_at: 10.0 + index as f32,
        end_at: 10.5 + index as f32,
        amount: 1,
    })
    .collect();

    let soundtrack = CinematicSoundtrack::new(&movie);
    let expected = [
        (5.0, "beam fire"),
        (5.3, "short explosion"),
        (6.0, "beam fire"),
        (6.3, "shield impact"),
        (8.0 + wreck_cue(Unit::Ship(Ship::LightFighter)).0, "explosion"),
        (13.0, "repair"),
        (14.0, "repair"),
    ];
    assert_eq!(soundtrack.cues.len(), expected.len());
    for ((at, cue), (expected_at, name)) in soundtrack.cues.iter().zip(expected) {
        assert!((at - expected_at).abs() < 0.001);
        assert_eq!(cue.name, name);
    }
    assert_eq!(movie.timeline.shots.len(), 5);
    assert_eq!(movie.timeline.repairs.len(), 5);
    assert_eq!(movie.timeline.actors[orbital].death_at, Some(7.0));
    assert_eq!(movie.timeline.actors[building].death_at, Some(9.0));
}
