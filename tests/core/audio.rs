use super::*;
use crate::core::missions::Mission;

/// Runs actual egui pointer press/release frames against an interactive widget.
fn click_widget(
    enabled: bool,
    sense: egui::Sense,
    override_sound: Option<Option<SoundEffect>>,
    menu_clicks: bool,
) -> Vec<SoundEffect> {
    let context = egui::Context::default();
    let mut rect = egui::Rect::NOTHING;
    let mut sounds = Vec::new();
    for pressed in [None, Some(true), Some(false)] {
        let events = pressed.map_or_else(Vec::new, |pressed| {
            vec![
                egui::Event::PointerMoved(rect.center()),
                egui::Event::PointerButton {
                    pos: rect.center(),
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                },
            ]
        });
        let mut output = context.run_ui(
            egui::RawInput {
                events,
                ..default()
            },
            |ui| {
                ui.add_enabled_ui(enabled, |ui| {
                    let (widget_rect, response) =
                        ui.allocate_exact_size(egui::vec2(120.0, 40.0), sense);
                    rect = widget_rect;
                    if response.clicked() {
                        if let Some(sound) = override_sound {
                            set_ui_sound(ui.ctx(), sound);
                        }
                    }
                });
                if let Some(sound) = take_ui_sound(ui.ctx(), menu_clicks) {
                    sounds.push(sound);
                }
            },
        );
        output.textures_delta.clear();
    }
    sounds
}

#[test]
fn button_clicks_ignore_disabled_widgets_and_drag_controls() {
    assert_eq!(click_widget(true, egui::Sense::click(), None, true), [SoundEffect::Button]);
    assert!(click_widget(false, egui::Sense::click(), None, true).is_empty());
    assert!(click_widget(true, egui::Sense::hover(), None, true).is_empty());
    assert!(click_widget(true, egui::Sense::click_and_drag(), None, true).is_empty());
}

#[test]
fn action_feedback_replaces_the_generic_click() {
    assert_eq!(
        click_widget(true, egui::Sense::click(), Some(Some(SoundEffect::ShipPurchased)), true),
        [SoundEffect::ShipPurchased]
    );
    assert!(click_widget(true, egui::Sense::click(), Some(None), true).is_empty());
}

#[test]
fn gameplay_browsing_is_silent_but_accepted_purchases_still_sound() {
    assert!(click_widget(true, egui::Sense::click(), None, false).is_empty());
    for sound in
        [SoundEffect::ShipPurchased, SoundEffect::BuildingQueued, SoundEffect::DefensePurchased]
    {
        assert_eq!(click_widget(true, egui::Sense::click(), Some(Some(sound)), false), [sound]);
    }
    assert!(click_widget(true, egui::Sense::click(), Some(None), false).is_empty());
}

/// Builds the real playback system without opening a speaker or GPU device.
fn audio_app() -> App {
    let mut app = App::new();
    app.add_plugins((MinimalPlugins, AssetPlugin::default()))
        .init_asset::<Image>()
        .init_asset::<Font>()
        .init_asset::<TextureAtlasLayout>()
        .init_asset::<AudioSource>()
        .init_resource::<Assets<AudioInstance>>()
        .init_resource::<Audio>()
        .init_resource::<WorldAssets>()
        .init_resource::<PlayingAudio>()
        .init_resource::<Settings>()
        .add_message::<PlayAudioMsg>()
        .add_systems(Update, play_audio);
    app
}

#[test]
fn repeated_clicks_are_queued_even_after_the_previous_instance_is_gone() {
    let mut app = audio_app();
    for _ in 0..2 {
        app.world_mut().write_message(SoundEffect::Button.request());
    }
    app.update();
    assert_eq!(app.world().resource::<PlayingAudio>().0["ui-click"].len(), 2);

    // Without a playback device these handles have no AudioInstance, just
    // like handles removed by Kira after a short effect has finished.
    app.world_mut().write_message(SoundEffect::Button.request());
    app.update();
    assert_eq!(app.world().resource::<PlayingAudio>().0["ui-click"].len(), 1);
}

#[test]
fn mute_discards_requests_and_effects_mode_keeps_music_off() {
    let mut app = audio_app();
    app.world_mut().resource_mut::<Settings>().audio = AudioState::Mute;
    app.world_mut().write_message(SoundEffect::Button.request());
    app.update();
    assert!(app.world().resource::<PlayingAudio>().0.is_empty());

    app.world_mut().resource_mut::<Settings>().audio = AudioState::NoMusic;
    app.world_mut().write_message(PlayAudioMsg::new("music").background());
    app.update();
    assert!(app.world().resource::<PlayingAudio>().0.is_empty());

    app.world_mut().write_message(SoundEffect::Button.request());
    app.update();
    assert_eq!(app.world().resource::<PlayingAudio>().0.len(), 1);

    app.world_mut().resource_mut::<Settings>().audio = AudioState::Sound;
    for _ in 0..2 {
        app.world_mut().write_message(PlayAudioMsg::new("music").background());
    }
    app.update();
    assert_eq!(app.world().resource::<PlayingAudio>().0["music"].len(), 1);
}

#[test]
fn looping_effects_play_once_in_effects_mode_and_stop_on_request() {
    let mut app = audio_app();
    app.add_message::<StopAudioMsg>().add_systems(Update, stop_audio.before(play_audio));
    app.world_mut().resource_mut::<Settings>().audio = AudioState::NoMusic;
    for _ in 0..2 {
        app.world_mut().write_message(PlayAudioMsg::new("booster").looped());
    }
    app.update();
    assert_eq!(app.world().resource::<PlayingAudio>().0["booster"].len(), 1);
    app.world_mut().write_message(StopAudioMsg::new("booster"));
    app.update();
    assert!(!app.world().resource::<PlayingAudio>().0.contains_key("booster"));
}

/// Exercises hover lifecycle transitions without opening an audio device or window.
fn hover_audio_app() -> (App, Entity) {
    let mut app = App::new();
    app.insert_resource(State::new(AppState::Game))
        .insert_resource(State::new(GameState::Playing))
        .init_resource::<Settings>()
        .insert_resource(UiState {
            mission_hover: Some(1),
            ..default()
        })
        .insert_resource(Missions(vec![
            Mission {
                id: 1,
                ..default()
            },
            Mission {
                id: 2,
                ..default()
            },
        ]))
        .add_message::<PlayAudioMsg>()
        .add_message::<StopAudioMsg>()
        .add_systems(Update, update_mission_hover_audio);
    let mut window = Window {
        focused: true,
        ..default()
    };
    window.set_cursor_position(Some(Vec2::splat(50.)));
    let window = app.world_mut().spawn((window, PrimaryWindow)).id();
    (app, window)
}

fn hover_audio_frame(app: &mut App) -> (usize, usize) {
    app.update();
    let plays: Vec<_> = app.world_mut().resource_mut::<Messages<PlayAudioMsg>>().drain().collect();
    for play in &plays {
        assert_eq!(play.name, "booster");
        assert!(play.is_looped);
        assert!(!play.is_background);
    }
    let stops: Vec<_> = app.world_mut().resource_mut::<Messages<StopAudioMsg>>().drain().collect();
    assert!(stops.iter().all(|stop| stop.name == "booster"));
    (plays.len(), stops.len())
}

#[test]
fn mission_hover_repeats_without_restarting_until_hover_ends() {
    let (mut app, _) = hover_audio_app();
    assert_eq!(hover_audio_frame(&mut app), (1, 0));
    for hovered in [Some(1), Some(2), Some(1)] {
        app.world_mut().resource_mut::<UiState>().mission_hover = hovered;
        assert_eq!(hover_audio_frame(&mut app), (0, 0));
    }
    app.world_mut().resource_mut::<UiState>().mission_hover = None;
    assert_eq!(hover_audio_frame(&mut app), (0, 1));
    assert_eq!(hover_audio_frame(&mut app), (0, 0));
    app.world_mut().resource_mut::<UiState>().mission_hover = Some(1);
    assert_eq!(hover_audio_frame(&mut app), (1, 0));
}

#[test]
fn mission_hover_stops_on_mute_and_resumes_in_effects_mode() {
    let (mut app, _) = hover_audio_app();
    app.world_mut().resource_mut::<Settings>().audio = AudioState::Mute;
    assert_eq!(hover_audio_frame(&mut app), (0, 0));
    app.world_mut().resource_mut::<Settings>().audio = AudioState::NoMusic;
    assert_eq!(hover_audio_frame(&mut app), (1, 0));
    app.world_mut().resource_mut::<Settings>().audio = AudioState::Sound;
    assert_eq!(hover_audio_frame(&mut app), (0, 0));
    app.world_mut().resource_mut::<Settings>().audio = AudioState::Mute;
    assert_eq!(hover_audio_frame(&mut app), (0, 1));
    assert_eq!(hover_audio_frame(&mut app), (0, 0));
    app.world_mut().resource_mut::<Settings>().audio = AudioState::NoMusic;
    assert_eq!(hover_audio_frame(&mut app), (1, 0));
}

#[test]
fn mission_hover_stops_when_the_mission_or_gameplay_disappears() {
    for next_state in [
        GameState::GameMenu,
        GameState::Settings,
        GameState::CombatMenu,
        GameState::Combat,
        GameState::EndGame,
    ] {
        let (mut app, _) = hover_audio_app();
        assert_eq!(hover_audio_frame(&mut app), (1, 0));
        app.insert_resource(State::new(next_state));
        assert_eq!(hover_audio_frame(&mut app), (0, 1));
        assert_eq!(hover_audio_frame(&mut app), (0, 0));
    }

    let (mut app, _) = hover_audio_app();
    assert_eq!(hover_audio_frame(&mut app), (1, 0));
    app.world_mut().resource_mut::<Missions>().0.clear();
    assert_eq!(hover_audio_frame(&mut app), (0, 1));
    assert_eq!(hover_audio_frame(&mut app), (0, 0));

    let (mut app, _) = hover_audio_app();
    assert_eq!(hover_audio_frame(&mut app), (1, 0));
    app.insert_resource(State::new(AppState::MainMenu));
    assert_eq!(hover_audio_frame(&mut app), (0, 1));
    app.world_mut().remove_resource::<UiState>();
    app.world_mut().remove_resource::<Missions>();
    assert_eq!(hover_audio_frame(&mut app), (0, 0));
}

#[test]
fn mission_hover_stops_when_the_window_loses_focus_or_the_pointer() {
    let (mut app, window) = hover_audio_app();
    assert_eq!(hover_audio_frame(&mut app), (1, 0));
    app.world_mut().get_mut::<Window>(window).unwrap().focused = false;
    assert_eq!(hover_audio_frame(&mut app), (0, 1));
    app.world_mut().get_mut::<Window>(window).unwrap().focused = true;
    assert_eq!(hover_audio_frame(&mut app), (1, 0));
    app.world_mut().get_mut::<Window>(window).unwrap().set_cursor_position(None);
    assert_eq!(hover_audio_frame(&mut app), (0, 1));
    assert_eq!(hover_audio_frame(&mut app), (0, 0));
}
