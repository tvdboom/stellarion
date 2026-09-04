use bevy::ecs::system::SystemState;
use bevy_egui::{EguiContext, EguiUserTextures, PrimaryEguiContext};

use super::*;
use crate::core::identity::{GameCode, GameId, UserId};
use crate::core::simulation::{GameModel, PersistedGame};
use crate::multiplayer::model::GameMembership;

/// Runs the production menu areas, including their persistent egui layout state.
fn menu_app() -> (App, egui::Context) {
    let mut app = App::new();
    app.init_resource::<EguiUserTextures>()
        .init_resource::<MultiplayerForm>()
        .init_resource::<MultiplayerSession>()
        .init_resource::<ConnectionIndicator>()
        .init_resource::<Settings>()
        .init_resource::<NextState<AppState>>()
        .insert_resource(State::new(AppState::Boot))
        .add_message::<MultiplayerRequest>()
        .add_message::<ChangeAudioMsg>()
        .add_systems(Update, (crate::core::ui::systems::set_ui_style, draw_menu).chain());
    app.world_mut().spawn((Window::default(), PrimaryWindow));
    let mut context = EguiContext::default();
    let egui = context.get_mut().clone();
    app.world_mut().spawn((context, PrimaryEguiContext));
    (app, egui)
}

fn menu_app_frame(
    app: &mut App,
    context: &egui::Context,
    viewport: egui::Vec2,
    state: AppState,
    events: Vec<egui::Event>,
) -> Vec<egui::epaint::ClippedShape> {
    app.world_mut().insert_resource(State::new(state));
    app.world_mut()
        .query::<&mut Window>()
        .single_mut(app.world_mut())
        .unwrap()
        .resolution
        .set(viewport.x, viewport.y);
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
            events,
            ..default()
        },
        |_| app.update(),
    );
    output.textures_delta.clear();
    output.shapes
}

fn main_action_labels() -> &'static [&'static str] {
    &[
        "New Game",
        "Join Game",
        "Resume Game",
        #[cfg(debug_assertions)]
        "Local Practice",
        "Settings",
        #[cfg(not(target_arch = "wasm32"))]
        "Quit",
    ]
}

fn visible_menu_label(shapes: &[egui::epaint::ClippedShape], label: &str) -> Option<egui::Rect> {
    shapes.iter().find_map(|shape| match &shape.shape {
        egui::Shape::Text(text) if text.galley.job.text == label => {
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            shape.clip_rect.contains_rect(rect).then_some(rect)
        },
        _ => None,
    })
}

fn assert_main_actions_visible(shapes: &[egui::epaint::ClippedShape], viewport: egui::Vec2) {
    let title = visible_menu_label(shapes, TITLE).unwrap();
    let footer = visible_menu_label(shapes, "Created by Mavs").unwrap();
    for label in main_action_labels() {
        let text = visible_menu_label(shapes, label)
            .unwrap_or_else(|| panic!("{label} clipped at {viewport:?}"));
        let (button, clip) = shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Rect(rect)
                    if rect.rect.contains_rect(text)
                        && rect.rect.center().distance(text.center()) < 1.0 =>
                {
                    Some((rect.rect, shape.clip_rect))
                },
                _ => None,
            })
            .unwrap();
        // egui rounds widget coordinates separately from the scroll clip rectangle.
        assert!(
            clip.expand(0.5).contains_rect(button),
            "{label} background {button:?} clipped by {clip:?} at {viewport:?}"
        );
        assert!(button.top() > title.bottom(), "{label} overlapped the title");
        assert!(button.bottom() < footer.top(), "{label} overlapped the footer");
    }
}

#[test]
fn gameplay_asset_failure_replaces_spinner_with_escape_action() {
    let (mut app, context) = menu_app();
    app.world_mut().resource_mut::<MultiplayerSession>().menu_error =
        Some("Could not load gameplay asset `missing.ktx2`.".to_string());
    let viewport = egui::vec2(900.0, 700.0);
    for _ in 0..3 {
        menu_app_frame(&mut app, &context, viewport, AppState::LoadingGame, vec![]);
    }
    let shapes = menu_app_frame(&mut app, &context, viewport, AppState::LoadingGame, vec![]);

    assert!(visible_menu_label(&shapes, "Starting game…").is_none());
    assert!(visible_menu_label(&shapes, "Game assets could not be loaded.").is_some());
    let back = visible_menu_label(&shapes, "Back to Menu").unwrap();
    click_menu_app(&mut app, &context, viewport, AppState::LoadingGame, back.center());
    let requests: Vec<_> =
        app.world_mut().resource_mut::<Messages<MultiplayerRequest>>().drain().collect();
    assert!(matches!(requests.as_slice(), [MultiplayerRequest::LeaveGame]));
}

#[test]
fn main_menu_shows_all_actions_after_boot_and_resize() {
    let (mut app, context) = menu_app();
    let initial_size = egui::vec2(786.0, 788.0);
    for _ in 0..3 {
        menu_app_frame(&mut app, &context, initial_size, AppState::Boot, vec![]);
    }
    for viewport in [
        initial_size,
        egui::vec2(786.0, 660.0),
        egui::vec2(400.0, 600.0),
        egui::vec2(1600.0, 900.0),
    ] {
        for _ in 0..3 {
            menu_app_frame(&mut app, &context, viewport, AppState::MainMenu, vec![]);
        }
        let shapes = menu_app_frame(&mut app, &context, viewport, AppState::MainMenu, vec![]);
        assert_main_actions_visible(&shapes, viewport);
    }
}

#[test]
fn main_menu_error_keeps_all_actions_visible_and_sits_above_offline_status() {
    let (mut app, context) = menu_app();
    let viewport = egui::vec2(898.0, 934.0);
    app.world_mut().resource_mut::<MultiplayerSession>().menu_error = Some(
        "backend protocol error: Supabase returned 400 Bad Request: Refresh token is not valid"
            .to_string(),
    );
    app.world_mut().resource_mut::<ConnectionIndicator>().status =
        crate::multiplayer::client::ConnectionStatus::Offline;

    for _ in 0..3 {
        menu_app_frame(&mut app, &context, viewport, AppState::MainMenu, vec![]);
    }
    let shapes = menu_app_frame(&mut app, &context, viewport, AppState::MainMenu, vec![]);
    assert_main_actions_visible(&shapes, viewport);

    let offline = visible_menu_label(&shapes, "Offline").unwrap();
    let status_dot = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle)
                if (circle.radius - 4.0).abs() < f32::EPSILON
                    && circle.center.x < offline.left()
                    && (offline.top()..=offline.bottom()).contains(&circle.center.y) =>
            {
                Some(circle)
            },
            _ => None,
        })
        .unwrap();
    let toast_title = visible_menu_label(&shapes, "Unable to complete action").unwrap();
    let toast = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect) if rect.rect.contains_rect(toast_title) => Some(rect.rect),
            _ => None,
        })
        .unwrap();
    let last_action = visible_menu_label(&shapes, main_action_labels().last().unwrap()).unwrap();
    let last_button = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.rect.contains_rect(last_action)
                    && rect.rect.center().distance(last_action.center()) < 1.0 =>
            {
                Some(rect.rect)
            },
            _ => None,
        })
        .unwrap();
    assert!(status_dot.center.x < offline.left());
    assert!((offline.top()..=offline.bottom()).contains(&status_dot.center.y));
    assert!(u16::from(status_dot.fill.r()) > u16::from(status_dot.fill.g()) * 2);
    assert!(toast.bottom() < offline.top());
    assert!(toast.center().x > viewport.x * 0.5);
    assert!((toast.right() - (viewport.x - 24.0)).abs() < 1.0);
    assert!(toast.left() > last_button.right());
    assert!(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport).contains_rect(toast));

    app.world_mut().resource_mut::<MultiplayerSession>().menu_error = None;
    app.world_mut().resource_mut::<ConnectionIndicator>().status =
        crate::multiplayer::client::ConnectionStatus::Connected;
    for _ in 0..3 {
        menu_app_frame(&mut app, &context, viewport, AppState::MainMenu, vec![]);
    }
    let shapes = menu_app_frame(&mut app, &context, viewport, AppState::MainMenu, vec![]);
    let connected = visible_menu_label(&shapes, "Connected").unwrap();
    let status_dot = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle)
                if (circle.radius - 4.0).abs() < f32::EPSILON
                    && circle.center.x < connected.left()
                    && (connected.top()..=connected.bottom()).contains(&circle.center.y) =>
            {
                Some(circle)
            },
            _ => None,
        })
        .unwrap();
    assert!(u16::from(status_dot.fill.g()) > u16::from(status_dot.fill.r()) * 2);
    assert!(visible_menu_label(&shapes, "Unable to complete action").is_none());
}

#[test]
fn main_menu_scrolls_short_windows_and_recovers_after_navigation_and_growth() {
    let (mut app, context) = menu_app();
    let small = egui::vec2(400.0, 400.0);
    for _ in 0..3 {
        menu_app_frame(&mut app, &context, small, AppState::Boot, vec![]);
    }
    for _ in 0..3 {
        menu_app_frame(&mut app, &context, small, AppState::MainMenu, vec![]);
    }
    let shapes = menu_app_frame(&mut app, &context, small, AppState::MainMenu, vec![]);
    let new_game = visible_menu_label(&shapes, "New Game").unwrap().center();
    assert!(visible_menu_label(&shapes, "Settings").is_none());
    menu_app_frame(
        &mut app,
        &context,
        small,
        AppState::MainMenu,
        vec![
            egui::Event::PointerMoved(new_game),
            egui::Event::MouseWheel {
                unit: egui::MouseWheelUnit::Point,
                delta: egui::vec2(0.0, -1000.0),
                phase: egui::TouchPhase::Move,
                modifiers: egui::Modifiers::NONE,
            },
        ],
    );
    for _ in 0..30 {
        menu_app_frame(&mut app, &context, small, AppState::MainMenu, vec![]);
    }
    let shapes = menu_app_frame(&mut app, &context, small, AppState::MainMenu, vec![]);
    assert!(visible_menu_label(&shapes, main_action_labels().last().unwrap()).is_some());
    let settings = visible_menu_label(&shapes, "Settings").unwrap().center();
    click_menu_app(&mut app, &context, small, AppState::MainMenu, settings);
    assert!(matches!(
        app.world().resource::<NextState<AppState>>(),
        NextState::Pending(AppState::Settings)
    ));

    let large = egui::vec2(786.0, 788.0);
    for _ in 0..3 {
        menu_app_frame(&mut app, &context, large, AppState::Settings, vec![]);
    }
    let shapes = menu_app_frame(&mut app, &context, large, AppState::Settings, vec![]);
    let back = visible_menu_label(&shapes, "Back").unwrap().center();
    click_menu_app(&mut app, &context, large, AppState::Settings, back);
    assert!(matches!(
        app.world().resource::<NextState<AppState>>(),
        NextState::Pending(AppState::MainMenu)
    ));
    for _ in 0..3 {
        menu_app_frame(&mut app, &context, large, AppState::MainMenu, vec![]);
    }
    let shapes = menu_app_frame(&mut app, &context, large, AppState::MainMenu, vec![]);
    assert_main_actions_visible(&shapes, large);
}

#[cfg(debug_assertions)]
#[test]
fn local_practice_color_survives_scrolling_and_is_used_by_start_and_enter() {
    for viewport in [egui::vec2(400.0, 700.0), egui::vec2(400.0, 400.0), egui::vec2(1600.0, 900.0)]
    {
        let (mut app, context) = menu_app();
        let state = AppState::SinglePlayerMenu;
        for _ in 0..3 {
            menu_app_frame(&mut app, &context, viewport, state, vec![]);
        }
        let shapes = menu_app_frame(&mut app, &context, viewport, state, vec![]);
        let swatches = filled_circles(&shapes, 11.0);
        assert_eq!(swatches.len(), PLAYER_COLOR_PALETTE.len());
        let title = visible_menu_label(&shapes, "Local Practice").unwrap();
        assert!(title.top() >= 0.0);
        for shape in &shapes {
            if let egui::Shape::Circle(circle) = &shape.shape {
                assert!(shape.clip_rect.contains_rect(circle.visual_bounding_rect()));
                assert!(circle.center.y > title.bottom());
            }
        }
        assert_eq!(
            app.world().resource::<MultiplayerForm>().practice_color,
            PLAYER_COLOR_PALETTE[0]
        );
        click_menu_app(&mut app, &context, viewport, state, swatches[4]);
        let chosen = PLAYER_COLOR_PALETTE[4];
        assert_eq!(app.world().resource::<MultiplayerForm>().practice_color, chosen);
        assert!(app.world().resource::<Messages<MultiplayerRequest>>().is_empty());

        let shapes = menu_app_frame(
            &mut app,
            &context,
            viewport,
            state,
            vec![egui::Event::PointerMoved(egui::Pos2::ZERO)],
        );
        assert!(shapes.iter().any(|shape| matches!(
            &shape.shape,
            egui::Shape::Circle(circle) if circle.radius == 14.0 && circle.center == swatches[4]
        )));

        app.world_mut().resource_mut::<MultiplayerSession>().busy = true;
        menu_app_frame(&mut app, &context, viewport, state, vec![]);
        click_menu_app(&mut app, &context, viewport, state, swatches[1]);
        menu_app_frame(
            &mut app,
            &context,
            viewport,
            state,
            vec![
                enter_event(false, egui::Modifiers::NONE),
                egui::Event::Key {
                    key: egui::Key::Enter,
                    physical_key: None,
                    pressed: false,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        assert_eq!(app.world().resource::<MultiplayerForm>().practice_color, chosen);
        assert!(app.world().resource::<Messages<MultiplayerRequest>>().is_empty());
        app.world_mut().resource_mut::<MultiplayerSession>().busy = false;

        // Short windows must let the player reach the actions below all setup rows.
        menu_app_frame(
            &mut app,
            &context,
            viewport,
            state,
            vec![
                egui::Event::PointerMoved(swatches[4]),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -1000.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        );
        for _ in 0..30 {
            menu_app_frame(&mut app, &context, viewport, state, vec![]);
        }
        let shapes = menu_app_frame(&mut app, &context, viewport, state, vec![]);
        let start = visible_menu_label(&shapes, "Start Practice").unwrap();
        let back = visible_menu_label(&shapes, "Back").unwrap();
        let footer = visible_menu_label(&shapes, "Created by Mavs").unwrap();
        assert!(start.bottom() < footer.top());
        assert!(back.bottom() < footer.top());
        click_menu_app(&mut app, &context, viewport, state, start.center());
        let requests: Vec<_> =
            app.world_mut().resource_mut::<Messages<MultiplayerRequest>>().drain().collect();
        assert!(matches!(requests.as_slice(), [MultiplayerRequest::StartLocalPractice {
            player_color, rules,
        }] if *player_color == chosen && rules.practice_mode && rules.player_count == 1));

        menu_app_frame(
            &mut app,
            &context,
            viewport,
            state,
            vec![enter_event(false, egui::Modifiers::NONE)],
        );
        let requests: Vec<_> =
            app.world_mut().resource_mut::<Messages<MultiplayerRequest>>().drain().collect();
        assert!(matches!(requests.as_slice(), [MultiplayerRequest::StartLocalPractice {
            player_color, ..
        }] if *player_color == chosen));
    }
}

fn click_menu_app(
    app: &mut App,
    context: &egui::Context,
    viewport: egui::Vec2,
    state: AppState,
    pos: egui::Pos2,
) {
    for pressed in [true, false] {
        menu_app_frame(
            app,
            context,
            viewport,
            state,
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
fn modal_gameplay_blocker_captures_the_whole_viewport() {
    let viewport = egui::vec2(800.0, 600.0);
    for pointer in [egui::pos2(1.0, 1.0), egui::pos2(799.0, 599.0)] {
        let context = egui::Context::default();
        let mut blocked = false;
        let mut output = context.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
                events: vec![egui::Event::PointerMoved(pointer)],
                ..default()
            },
            |context| {
                block_gameplay_pointer(context);
                blocked = context.is_pointer_over_egui();
            },
        );
        output.textures_delta.clear();
        assert!(blocked, "pointer at {pointer:?} escaped the modal layer");
    }
}

fn enter_event(repeat: bool, modifiers: egui::Modifiers) -> egui::Event {
    egui::Event::Key {
        key: egui::Key::Enter,
        physical_key: None,
        pressed: true,
        repeat,
        modifiers,
    }
}

/// Exercises the actual egui screen with a plain Enter event and captures its requests.
fn submit_menu(
    draw: impl FnMut(&mut egui::Ui, &mut MessageWriter<MultiplayerRequest>),
) -> Vec<MultiplayerRequest> {
    menu_frame(&egui::Context::default(), vec![enter_event(false, egui::Modifiers::NONE)], draw).1
}

fn menu_frame(
    context: &egui::Context,
    events: Vec<egui::Event>,
    draw: impl FnMut(&mut egui::Ui, &mut MessageWriter<MultiplayerRequest>),
) -> (Vec<egui::epaint::ClippedShape>, Vec<MultiplayerRequest>) {
    menu_frame_sized(context, egui::vec2(1600.0, 900.0), events, draw)
}

fn menu_frame_sized(
    context: &egui::Context,
    viewport: egui::Vec2,
    events: Vec<egui::Event>,
    mut draw: impl FnMut(&mut egui::Ui, &mut MessageWriter<MultiplayerRequest>),
) -> (Vec<egui::epaint::ClippedShape>, Vec<MultiplayerRequest>) {
    let mut world = World::new();
    world.init_resource::<Messages<MultiplayerRequest>>();
    let mut system = SystemState::<MessageWriter<MultiplayerRequest>>::new(&mut world);
    let mut requests = system.get_mut(&mut world).unwrap();
    let mut output = context.run_ui(
        egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, viewport)),
            events,
            ..default()
        },
        |ui| {
            ui.set_width(560.0);
            draw(ui, &mut requests);
        },
    );
    // Headless tests inspect shapes without uploading font textures to a GPU.
    output.textures_delta.clear();
    let requests = world.resource_mut::<Messages<MultiplayerRequest>>().drain().collect();
    (output.shapes, requests)
}

fn click_menu(
    context: &egui::Context,
    pos: egui::Pos2,
    mut draw: impl FnMut(&mut egui::Ui, &mut MessageWriter<MultiplayerRequest>),
) -> (Vec<egui::epaint::ClippedShape>, Vec<MultiplayerRequest>) {
    let mut shapes = Vec::new();
    let mut requests = Vec::new();
    for pressed in [true, false] {
        let (frame_shapes, frame_requests) = menu_frame(
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
            &mut draw,
        );
        shapes = frame_shapes;
        requests.extend(frame_requests);
    }
    (shapes, requests)
}

fn filled_circles(shapes: &[egui::epaint::ClippedShape], radius: f32) -> Vec<egui::Pos2> {
    shapes
        .iter()
        .filter_map(|shape| match &shape.shape {
            egui::Shape::Circle(circle)
                if circle.radius == radius && circle.fill != egui::Color32::TRANSPARENT =>
            {
                Some(circle.center)
            },
            _ => None,
        })
        .collect()
}

fn test_lobby() -> MultiplayerSession {
    let id = GameId::new("menu-test");
    let members = (1..=2)
        .map(|player_id| GameMembership {
            game_id: id.clone(),
            player_id,
            user_id: UserId::new(format!("user-{player_id}")),
            display_name: format!("Player {player_id}"),
            is_creator: player_id == 1,
            identity_version: 1,
            connected: true,
        })
        .collect::<Vec<_>>();
    let mut session = MultiplayerSession::default();
    session.membership = Some(members[0].clone());
    session.active_game = Some(GameRecord {
        submitted_players: Vec::new(),
        id,
        code: GameCode::new("ABCDEF"),
        revision: 0,
        saved_at: 1_700_000_000,
        max_players: 2,
        status: MatchStatus::Lobby,
        persisted: PersistedGame::new(GameModel::new([3; 32], GameRules::default()).unwrap()),
        members,
    });
    session
}

#[test]
fn menu_button_registers_its_click_sound() {
    let context = egui::Context::default();
    let (shapes, _) = menu_frame(&context, vec![], |ui, _| {
        menu_button_widget(ui, "Audio test", true, egui::vec2(240.0, 52.0), 20.0);
    });
    let center = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == "Audio test" => {
                Some(text.galley.rect.translate(text.pos.to_vec2()).center())
            },
            _ => None,
        })
        .unwrap();

    click_menu(&context, center, |ui, _| {
        menu_button_widget(ui, "Audio test", true, egui::vec2(240.0, 52.0), 20.0);
    });

    let sound =
        context.data_mut(|data| data.remove_temp::<Option<SoundEffect>>(egui::Id::new("ui_sound")));
    assert_eq!(sound, Some(Some(SoundEffect::Button)));
}

#[test]
fn resume_save_timestamp_uses_a_stable_utc_date_and_minute() {
    assert_eq!(format_saved_timestamp(1_700_000_000), "Saved 2023-11-14 22:13 UTC");
    assert_eq!(format_saved_timestamp(0), "Saved time unavailable");
}

#[test]
fn resume_overview_fits_available_height_and_keeps_status_dots_clear() {
    for (height, visible_cards) in [(514.0, 2), (900.0, 5)] {
        let context = egui::Context::default();
        context.add_font(egui::epaint::text::FontInsert::new(
            "firasans",
            egui::FontData::from_static(include_bytes!("../../assets/fonts/FiraSans-Bold.ttf")),
            vec![egui::epaint::text::InsertFontFamily {
                family: egui::FontFamily::Proportional,
                priority: egui::epaint::text::FontPriority::Highest,
            }],
        ));
        let mut session = MultiplayerSession::default();
        session.games = (0..6)
            .map(|index| GameSummary {
                id: GameId::new(format!("resume-{index}")),
                code: GameCode::new("ABCDEF"),
                revision: 0,
                saved_at: 1_700_000_000 + index as u64 * 60,
                status: [MatchStatus::Active, MatchStatus::Finished][index % 2],
                turn: 7,
                player_id: 1,
                display_name: "Nova".to_string(),
                player_color: PlayerColor::new(4).unwrap(),
                player_count: 2,
                max_players: 2,
            })
            .collect();
        let mut refreshing = false;
        let mut next = NextState::default();
        let (shapes, _) =
            menu_frame_sized(&context, egui::vec2(800.0, height), vec![], |ui, requests| {
                resume_screen(ui, &session, &mut refreshing, requests, &mut next)
            });
        let cards = shapes
            .iter()
            .filter(|shape| {
                matches!(&shape.shape,
                    egui::Shape::Rect(rect) if rect.rect.height() == 90.0
                        && shape.clip_rect.contains_rect(rect.rect)
                )
            })
            .count();
        assert_eq!(cards, visible_cards, "viewport height {height}");
        let dots = filled_circles(&shapes, 3.5);
        for text in shapes.iter().filter_map(|shape| match &shape.shape {
            egui::Shape::Text(text)
                if ["Waiting for players", "In progress", "Finished"]
                    .contains(&text.galley.job.text.as_str()) =>
            {
                Some(text)
            },
            _ => None,
        }) {
            let bounds = text.galley.rect.translate(text.pos.to_vec2());
            let dot = dots.iter().find(|dot| (dot.y - bounds.center().y).abs() < 1.0).unwrap();
            assert!(dot.x + 3.5 < bounds.left(), "dot overlaps {}", text.galley.job.text);
        }
    }
}

#[test]
fn resume_identity_fits_narrow_cards_and_busy_cards_do_not_resume() {
    for width in [280.0, 640.0] {
        for name in ["Nova".to_string(), "W".repeat(32)] {
            for enabled in [true, false] {
                let context = egui::Context::default();
                let game = GameSummary {
                    id: GameId::new("resume-identity"),
                    code: GameCode::new("ABCDEF"),
                    revision: 1,
                    saved_at: 1_700_000_000,
                    status: MatchStatus::Active,
                    turn: 6,
                    player_id: 2,
                    display_name: name.clone(),
                    player_color: PlayerColor::new(4).unwrap(),
                    player_count: 2,
                    max_players: 2,
                };
                let draw = |ui: &mut egui::Ui, requests: &mut MessageWriter<MultiplayerRequest>| {
                    ui.allocate_ui(egui::vec2(width, 120.0), |ui| {
                        if resume_game_card(ui, &game, enabled) {
                            requests.write(MultiplayerRequest::ResumeGame(game.id.clone()));
                        }
                    });
                };
                let (shapes, _) = menu_frame(&context, vec![], draw);
                let card = shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) if (rect.rect.width() - width).abs() < 1.0 => {
                            Some(rect.rect)
                        },
                        _ => None,
                    })
                    .unwrap();
                let name_bounds = shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == name => {
                            assert_eq!(text.galley.rows.len(), 1);
                            Some(text.galley.rect.translate(text.pos.to_vec2()))
                        },
                        _ => None,
                    })
                    .unwrap();
                assert!(card.contains_rect(name_bounds));
                assert!(name_bounds.right() <= card.right() - 38.0);
                let [r, g, b] = game.player_color.rgb();
                let color = egui::Color32::from_rgb(r, g, b);
                assert!(shapes.iter().any(|shape| matches!(&shape.shape,
                    egui::Shape::Circle(dot) if dot.radius == 5.0
                        && dot.fill == if enabled { color } else { color.gamma_multiply(0.5) }
                        && (dot.center.y - name_bounds.center().y).abs() < 1.0
                        && dot.center.x + dot.radius < name_bounds.left()
                )));
                for shape in &shapes {
                    if let egui::Shape::Text(text) = &shape.shape {
                        let bounds = text.galley.rect.translate(text.pos.to_vec2());
                        assert!(card.contains_rect(bounds), "{}", text.galley.job.text);
                        if text.galley.job.text.starts_with("Turn ") {
                            assert!(!bounds.intersects(name_bounds));
                        }
                    }
                }
                let (_, requests) = click_menu(&context, name_bounds.center(), draw);
                assert_eq!(requests.len(), usize::from(enabled));
            }
        }
    }
}

#[test]
fn menu_enter_is_consumed_once_without_repeats_or_modifiers() {
    let mut events = vec![enter_event(false, egui::Modifiers::NONE)];
    assert!(take_menu_submit(&mut events));
    assert!(!take_menu_submit(&mut events));
    events.push(enter_event(true, egui::Modifiers::NONE));
    assert!(!take_menu_submit(&mut events));
    assert!(events.is_empty());
    events.push(enter_event(false, egui::Modifiers::CTRL));
    assert!(!take_menu_submit(&mut events));
    assert_eq!(events.len(), 1);
}

#[test]
fn recovery_is_accessible_from_an_empty_resume_list_and_returns_to_it() {
    let context = egui::Context::default();
    let session = MultiplayerSession::default();
    let mut refreshing = false;
    let mut next = NextState::default();
    let text_center = |shapes: &[egui::epaint::ClippedShape], label: &str| {
        shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == label => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()).center())
                },
                _ => None,
            })
            .unwrap()
    };
    let (shapes, _) = menu_frame(&context, vec![], |ui, requests| {
        resume_screen(ui, &session, &mut refreshing, requests, &mut next);
    });
    let refresh = text_center(&shapes, "Refresh");
    let recover = text_center(&shapes, "Recover Game");
    assert!(recover.x > refresh.x);
    assert!((recover.y - refresh.y).abs() < 1.0);
    let (_, requests) =
        click_menu(&context, text_center(&shapes, "Recover Game"), |ui, requests| {
            resume_screen(ui, &session, &mut refreshing, requests, &mut next)
        });
    assert!(requests.is_empty());
    assert!(matches!(next, NextState::Pending(AppState::RecoverPlayer)));

    let mut form = MultiplayerForm::default();
    let (shapes, _) = menu_frame(&context, vec![], |ui, requests| {
        recovery_screen(ui, &mut form, false, None, requests, &mut next);
    });
    let (_, requests) = click_menu(&context, text_center(&shapes, "Back"), |ui, requests| {
        recovery_screen(ui, &mut form, false, None, requests, &mut next);
    });
    assert!(requests.is_empty());
    assert!(matches!(next, NextState::Pending(AppState::ResumeGame)));

    form.game_code = "ABCDEF".to_string();
    form.recovery_code = "0123-4567-89AB-CDEF".to_string();
    for busy in [false, true] {
        let requests = submit_menu(|ui, requests| {
            recovery_screen(ui, &mut form, busy, None, requests, &mut next);
        });
        if busy {
            assert!(requests.is_empty());
        } else {
            assert!(
                matches!(requests.as_slice(), [MultiplayerRequest::RecoverPlayer { code, recovery_code }]
                if code == &form.game_code && recovery_code == &form.recovery_code)
            );
        }
    }
}

#[test]
fn join_game_code_uses_the_labeled_help_card_above_the_actions() {
    let context = egui::Context::default();
    let mut form = MultiplayerForm {
        saved_display_name: Some("Nova".to_string()),
        ..default()
    };
    let mut next = NextState::default();
    let (shapes, _) = menu_frame(&context, vec![], |ui, requests| {
        join_screen(ui, &mut form, false, None, requests, &mut next);
    });

    let title = visible_menu_label(&shapes, "Join Game").unwrap();
    let heading = visible_menu_label(&shapes, "GAME CODE").unwrap();
    let hint = visible_menu_label(&shapes, "Enter game code").unwrap();
    let info = visible_menu_label(&shapes, "i").unwrap();
    let back = visible_menu_label(&shapes, "Back").unwrap();
    let join = visible_menu_label(&shapes, "Join").unwrap();
    let card = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == egui::Color32::from_rgba_unmultiplied(14, 22, 31, 232) =>
            {
                Some(rect.rect)
            },
            _ => None,
        })
        .unwrap();

    assert!(title.bottom() < card.top());
    assert!(card.contains_rect(heading));
    assert!(card.contains_rect(hint));
    assert!(card.contains_rect(info));
    assert!(card.bottom() < back.top());
    assert!((back.center().y - join.center().y).abs() < 1.0);
}

#[test]
fn joining_reuses_saved_name_and_only_prompts_when_needed() {
    for (saved_name, draft, expected_name, asks_for_name) in [
        (Some("Nova"), "", "Nova", false),
        (Some("Nova"), "Unsubmitted edit", "Nova", false),
        (None, "First Pilot", "First Pilot", true),
        (Some("   "), "First Pilot", "First Pilot", true),
        (Some("This saved player name is too long"), "First Pilot", "First Pilot", true),
    ] {
        let mut form = MultiplayerForm {
            display_name: draft.to_string(),
            saved_display_name: saved_name.map(str::to_string),
            game_code: "ABCDEF".to_string(),
            ..default()
        };
        let mut next = NextState::default();
        let (shapes, requests) = menu_frame(
            &egui::Context::default(),
            vec![enter_event(false, egui::Modifiers::NONE)],
            |ui, requests| join_screen(ui, &mut form, false, None, requests, &mut next),
        );
        assert_eq!(
            shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text)
                    if text.galley.job.text == "Player name")
            }),
            asks_for_name,
        );
        assert!(matches!(requests.as_slice(),
            [MultiplayerRequest::JoinGame { display_name, code }]
                if display_name == expected_name && code == "ABCDEF"
        ));
        assert!(submit_menu(|ui, requests| {
            join_screen(ui, &mut form, true, None, requests, &mut next);
        })
        .is_empty());
    }
}

#[test]
fn recovery_help_is_only_shown_on_hover() {
    let context = egui::Context::default();
    context.global_style_mut(|style| {
        style.interaction.tooltip_delay = 0.0;
        style.interaction.show_tooltips_only_when_still = false;
    });
    let mut code = String::new();
    let mut draw = |ui: &mut egui::Ui, _: &mut MessageWriter<MultiplayerRequest>| {
        recovery_code_field(ui, "Recovery code", &mut code, true, "Enter recovery code");
    };
    let (shapes, _) = menu_frame(&context, vec![], &mut draw);
    let help_visible = |shapes: &[egui::epaint::ClippedShape]| {
        shapes.iter().any(|shape| {
        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text.starts_with("Each player has a different private recovery code"))
    })
    };
    assert!(!help_visible(&shapes));
    let icon = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == "i" => {
                Some(text.galley.rect.translate(text.pos.to_vec2()).center())
            },
            _ => None,
        })
        .unwrap();
    menu_frame(&context, vec![egui::Event::PointerMoved(icon)], &mut draw);
    menu_frame(&context, vec![], &mut draw);
    let (shapes, _) = menu_frame(&context, vec![], &mut draw);
    assert!(help_visible(&shapes));
}

#[test]
fn resume_buttons_keep_their_layout_before_during_and_after_refresh() {
    for width in [280.0, 320.0, 640.0] {
        let context = egui::Context::default();
        let mut idle_layout = None;
        for refreshing in [false, true, false] {
            let (shapes, _) = menu_frame(&context, vec![], |ui, _| {
                ui.allocate_ui(egui::vec2(width, 72.0), |ui| {
                    resume_action_buttons(ui, refreshing, refreshing);
                });
            });
            let buttons = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect) => Some(rect.rect),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(buttons.len(), 3);
            assert!(buttons[2].right() - buttons[0].left() <= width + 0.1);
            let labels = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.job.text.clone(),
                        text.galley.rect.translate(text.pos.to_vec2()),
                        text.galley.job.sections[0].format.font_id.size,
                    )),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(labels.len(), 3);
            assert_eq!(labels[1].0, "Refresh");
            for (button, (_, bounds, _)) in buttons.iter().zip(&labels) {
                assert!(button.contains_rect(*bounds));
            }
            let spinners = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Path(path) => Some(path),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(spinners.len(), usize::from(refreshing));
            for spinner in spinners {
                for point in &spinner.points {
                    assert!(buttons[1].contains(*point));
                    assert!(point.x + spinner.stroke.width * 0.5 < labels[1].1.left());
                }
            }
            let layout = (buttons, labels);
            if let Some(idle) = &idle_layout {
                assert_eq!(&layout, idle, "layout changed while refreshing at width {width}");
            } else {
                idle_layout = Some(layout);
            }
        }
    }
}

#[test]
fn enter_submits_forms_but_respects_validation_and_busy_state() {
    let mut form = MultiplayerForm::default();
    let mut settings = Settings::default();
    let mut next = NextState::default();
    for busy in [false, true] {
        let requests = submit_menu(|ui, requests| {
            create_screen(ui, &mut form, &mut settings, busy, requests, &mut next);
        });
        if busy {
            assert!(requests.is_empty());
        } else {
            assert!(matches!(requests.as_slice(), [MultiplayerRequest::CreateGame { .. }]));
        }
    }
    form.display_name.clear();
    assert!(submit_menu(|ui, requests| {
        create_screen(ui, &mut form, &mut settings, false, requests, &mut next);
    })
    .is_empty());

    form.display_name = "Commander".to_string();
    form.game_code = "ABCDEF".to_string();
    let joined = submit_menu(|ui, requests| {
        join_screen(ui, &mut form, false, None, requests, &mut next);
    });
    assert!(matches!(joined.as_slice(), [MultiplayerRequest::JoinGame { .. }]));
    form.display_name.clear();
    assert!(submit_menu(|ui, requests| {
        join_screen(ui, &mut form, false, None, requests, &mut next);
    })
    .is_empty());
    form.display_name = "Commander".to_string();
    form.game_code.clear();
    assert!(submit_menu(|ui, requests| {
        join_screen(ui, &mut form, false, None, requests, &mut next);
    })
    .is_empty());
}

#[test]
fn busy_lobby_always_allows_leaving_for_both_roles() {
    for host in [true, false] {
        let context = egui::Context::default();
        let mut session = test_lobby();
        session.busy = true;
        session.membership.as_mut().unwrap().is_creator = host;
        let mut next = NextState::default();
        let (shapes, _) = menu_frame(&context, vec![], |ui, requests| {
            lobby_screen(ui, &session, requests, &mut next);
        });
        let leave = shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Leave Lobby" => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()).center())
                },
                _ => None,
            })
            .unwrap();
        let (_, requests) = click_menu(&context, leave, |ui, requests| {
            lobby_screen(ui, &session, requests, &mut next);
        });
        assert!(matches!(requests.as_slice(), [MultiplayerRequest::LeaveGame]));
    }
}

#[test]
fn recovery_error_panel_precedes_buttons_and_short_windows_keep_navigation_visible() {
    for size in [egui::vec2(715.0, 715.0), egui::vec2(400.0, 400.0)] {
        let context = egui::Context::default();
        let mut form = MultiplayerForm::default();
        let mut next = NextState::default();
        let error = "This recovery code is already in use by a connected player. Use your own private recovery code.";
        let mut draw = |ui: &mut egui::Ui, requests: &mut MessageWriter<MultiplayerRequest>| {
            let width = (size.x * 0.4).clamp(320.0, 640.0).min(size.x - 32.0);
            ui.allocate_ui(egui::vec2(width, size.y), |ui| {
                ui.vertical_centered(|ui| {
                    recovery_screen(ui, &mut form, false, Some(error), requests, &mut next)
                });
            });
        };
        // Allow the scroll animation to reveal the newly reported error.
        for _ in 0..30 {
            menu_frame_sized(&context, size, vec![], &mut draw);
        }
        let (shapes, _) = menu_frame_sized(&context, size, vec![], &mut draw);
        let back = shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.job.text == "Back" => {
                    Some(text.galley.rect.translate(text.pos.to_vec2()))
                },
                _ => None,
            })
            .unwrap();
        assert!(back.bottom() < size.y - 96.0, "navigation overlapped footer at {size:?}");
        {
            let panel = shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Rect(rect)
                        if rect.fill == egui::Color32::from_rgba_unmultiplied(67, 23, 32, 232) =>
                    {
                        Some(rect.rect)
                    },
                    _ => None,
                })
                .unwrap();
            assert!(panel.bottom() < back.top());
            assert!(shapes.iter().any(|shape| matches!(&shape.shape,
                egui::Shape::Text(text) if text.galley.job.text == error
                && shape.clip_rect.contains_rect(text.galley.rect.translate(text.pos.to_vec2()))
                && panel.contains_rect(text.galley.rect.translate(text.pos.to_vec2())))));
        }
    }
}

#[test]
fn enter_in_lobby_requires_a_ready_host() {
    let mut session = test_lobby();
    let mut next = NextState::default();
    let started = submit_menu(|ui, requests| lobby_screen(ui, &session, requests, &mut next));
    assert!(matches!(started.as_slice(), [MultiplayerRequest::StartGame]));
    session.busy = true;
    assert!(!lobby_primary_enabled(&session));
    session.busy = false;
    session.membership.as_mut().unwrap().is_creator = false;
    assert!(submit_menu(|ui, requests| lobby_screen(ui, &session, requests, &mut next)).is_empty());
    session.membership.as_mut().unwrap().is_creator = true;
    session.active_game.as_mut().unwrap().members.pop();
    assert!(!lobby_primary_enabled(&session));

    session = test_lobby();
    session.active_game.as_mut().unwrap().status = MatchStatus::Active;
    session.reconnect_lobby = true;
    let resumed = submit_menu(|ui, requests| lobby_screen(ui, &session, requests, &mut next));
    assert!(matches!(resumed.as_slice(), [MultiplayerRequest::ResumeActiveGame]));
    session.active_game.as_mut().unwrap().members[1].connected = false;
    assert!(!lobby_primary_enabled(&session));
}

#[test]
fn hovering_player_color_opens_overlay_without_moving_the_page() {
    let session = test_lobby();
    let game = session.active_game.as_ref().unwrap();
    let context = egui::Context::default();
    let draw = |ui: &mut egui::Ui, requests: &mut MessageWriter<MultiplayerRequest>| {
        lobby_players_card(ui, game, false, Some(1), false, requests);
    };
    let (shapes, requests) = menu_frame(&context, vec![], draw);
    assert!(requests.is_empty());
    assert!(filled_circles(&shapes, 11.0).is_empty());
    let markers = filled_circles(&shapes, 6.0);
    assert_eq!(markers.len(), 2);

    let host = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Text(text) if text.galley.job.text == "HOST" => Some(text),
            _ => None,
        })
        .unwrap();
    let row = shapes
        .iter()
        .find_map(|shape| match &shape.shape {
            egui::Shape::Rect(rect)
                if rect.fill == egui::Color32::from_rgba_unmultiplied(255, 255, 255, 14) =>
            {
                Some(rect.rect)
            },
            _ => None,
        })
        .unwrap();
    let host_right = host.pos.x + host.galley.rect.right();
    assert!(
        (host_right - (row.right() - 10.0)).abs() < 1.0,
        "host right edge {host_right}, player row {row:?}"
    );

    let (shapes, requests) =
        menu_frame(&context, vec![egui::Event::PointerMoved(markers[1])], draw);
    assert!(requests.is_empty());
    assert!(filled_circles(&shapes, 11.0).is_empty());
    let (_, requests) = menu_frame(&context, vec![egui::Event::PointerMoved(markers[0])], draw);
    assert!(requests.is_empty());
    // Popup areas use their first frame to measure; no click is needed to open them.
    let (shapes, _) = menu_frame(&context, vec![], draw);
    let swatches = filled_circles(&shapes, 11.0);
    let available = available_player_colors(game, 1);
    assert_eq!(swatches.len(), available.len());
    assert!(swatches.iter().all(|pos| pos.y > markers[0].y + 20.0));
    assert_eq!(filled_circles(&shapes, 6.0), markers);

    let selected = game.persisted.state.player(1).unwrap().color();
    let occupied = game.persisted.state.player(2).unwrap().color();
    assert!(!available.contains(&selected));
    assert!(!available.contains(&occupied));
    let (shapes, requests) =
        menu_frame(&context, vec![egui::Event::PointerMoved(swatches[0])], draw);
    assert!(requests.is_empty());
    assert_eq!(filled_circles(&shapes, 11.0).len(), available.len());
    let (_, requests) = click_menu(&context, swatches[0], draw);
    assert!(matches!(
        requests.as_slice(),
        [MultiplayerRequest::SetPlayerColor(color)] if *color == available[0]
    ));
    let (shapes, _) = menu_frame(&context, vec![], draw);
    assert!(filled_circles(&shapes, 11.0).is_empty());
    assert_eq!(filled_circles(&shapes, 6.0), markers);

    menu_frame(&context, vec![egui::Event::PointerMoved(markers[0])], draw);
    let (shapes, _) = menu_frame(&context, vec![], draw);
    assert_eq!(filled_circles(&shapes, 11.0).len(), available.len());
    let (shapes, _) =
        menu_frame(&context, vec![egui::Event::PointerMoved(egui::pos2(1000.0, 700.0))], draw);
    assert!(filled_circles(&shapes, 11.0).is_empty());
    assert_eq!(filled_circles(&shapes, 6.0), markers);
}

#[test]
/// Wide viewports crop the top and bottom instead of exposing empty side bars.
fn cover_crop_fills_widescreen() {
    let rect = cover_source_rect(Vec2::new(1536.0, 1024.0), Vec2::new(1600.0, 900.0));
    assert_eq!(rect.min, Vec2::new(0.0, 80.0));
    assert_eq!(rect.max, Vec2::new(1536.0, 944.0));
}

#[test]
/// Tall viewports crop both horizontal edges while preserving source proportions.
fn cover_crop_fills_portrait() {
    let rect = cover_source_rect(Vec2::new(1536.0, 1024.0), Vec2::new(900.0, 1600.0));
    assert_eq!(rect.min, Vec2::new(480.0, 0.0));
    assert_eq!(rect.max, Vec2::new(1056.0, 1024.0));
}
