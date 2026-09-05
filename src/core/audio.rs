//! Bevy/Kira audio resources and systems for music, effects, and mute state.

use std::collections::HashMap;
use std::time::Duration;

use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{egui, EguiContexts};
use bevy_kira_audio::prelude::*;

use crate::core::assets::WorldAssets;
use crate::core::camera::MainCamera;
use crate::core::map::scenery::CelestialKind;
use crate::core::map::systems::{CelestialCmp, SolarStarCmp};
use crate::core::missions::Missions;
use crate::core::settings::Settings;
use crate::core::states::{AppState, AudioState, GameState};
use crate::core::ui::systems::UiState;
use crate::core::units::Unit;

/// Short feedback cues balanced for repeated menu and gameplay actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoundEffect {
    /// Short click for main-menu buttons and explicit action buttons.
    Button,
    /// Purchase accepted by the shipyard.
    ShipPurchased,
    /// Building construction added to the current turn.
    BuildingQueued,
    /// Defense or missile purchase accepted.
    DefensePurchased,
    /// Launch confirmation after accepting a mission command.
    MissionLaunched,
}

impl SoundEffect {
    /// Gain for the mission launch confirmation, in decibels.
    pub const MISSION_LAUNCH_VOLUME: f32 = -18.0;

    /// Selects the confirmation for a successfully queued purchase.
    pub fn purchase(unit: Unit) -> Self {
        match unit {
            Unit::Building(_) => Self::BuildingQueued,
            Unit::Ship(_) => Self::ShipPurchased,
            Unit::Defense(_) => Self::DefensePurchased,
        }
    }

    /// Creates a one-shot request with the cue's configured gain.
    pub fn request(self) -> PlayAudioMsg {
        let name = match self {
            Self::Button => "ui-click",
            Self::ShipPurchased | Self::BuildingQueued | Self::DefensePurchased => "construction",
            Self::MissionLaunched => "launch",
        };
        let mut request = PlayAudioMsg::new(name);
        if self == Self::MissionLaunched {
            request.volume = Self::MISSION_LAUNCH_VOLUME;
        }
        request
    }
}

/// Overrides the generic click for the current egui pass; `None` silences deferred actions.
pub fn set_ui_sound(context: &egui::Context, sound: Option<SoundEffect>) {
    context.data_mut(|data| data.insert_temp(egui::Id::new("ui_sound"), sound));
}

/// Selects one cue for an enabled button, ignoring background clicks and drag controls.
fn take_ui_sound(context: &egui::Context, menu_clicks: bool) -> Option<SoundEffect> {
    if let Some(sound) =
        context.data_mut(|data| data.remove_temp::<Option<SoundEffect>>(egui::Id::new("ui_sound")))
    {
        return sound;
    }
    // Gameplay windows also receive clicks on their empty backgrounds. Only
    // explicit accepted actions may make sound there; browsing stays silent.
    if !menu_clicks {
        return None;
    }
    let id = context.interaction_snapshot(|interaction| interaction.clicked)?;
    let response = context.read_response(id)?;
    (response.enabled() && response.clicked() && !response.sense.senses_drag())
        .then_some(SoundEffect::Button)
}

/// Collects egui feedback after drawing, at most once across egui's repeated layout passes.
pub fn play_ui_audio(
    mut contexts: EguiContexts,
    mut play: MessageWriter<PlayAudioMsg>,
    app_state: Res<State<AppState>>,
    game_state: Res<State<GameState>>,
    mut last_frame: Local<Option<u64>>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let menu_clicks = *app_state.get() != AppState::Game
        || matches!(game_state.get(), GameState::GameMenu | GameState::Settings);
    let sound = take_ui_sound(context, menu_clicks);
    let frame = context.cumulative_frame_nr();
    if *last_frame != Some(frame) {
        *last_frame = Some(frame);
        if let Some(sound) = sound {
            play.write(sound.request());
        }
    }
}

/// Adds click feedback to a Bevy UI button without responding to right-clicks.
pub fn play_button_audio(event: On<Pointer<Click>>, mut play: MessageWriter<PlayAudioMsg>) {
    if event.button == PointerButton::Primary {
        play.write(SoundEffect::Button.request());
    }
}

#[derive(Resource, Default)]
/// Tracked music and effect instances grouped by logical channel name.
pub struct PlayingAudio(pub HashMap<&'static str, Vec<AudioPlayback>>);

/// An instance and its original gain, separate from the master volume.
pub struct AudioPlayback {
    handle: Handle<AudioInstance>,
    base_volume: f32,
    master_volume: f32,
}

impl PlayingAudio {
    pub const BACKGROUND_VOLUME: f32 = -30.;
    /// Gain for the continuous hovered-mission booster ambience, in decibels.
    pub const AMBIENCE_VOLUME: f32 = -12.;
    pub const TWEEN: AudioTween = AudioTween::new(Duration::from_secs(2), AudioEasing::OutPowi(2));
}

const STAR_AMBIENCE_NAME: &str = "star ambience";
const STAR_AMBIENCE_MAX_VOLUME: f32 = -21.0;
const STAR_AMBIENCE_SILENCE: f32 = -60.0;
const STAR_AMBIENCE_NEAR_DISTANCE: f32 = 260.0;
const STAR_AMBIENCE_FAR_DISTANCE: f32 = 1_100.0;
const STAR_AMBIENCE_FULL_ZOOM: f32 = 0.6;
const STAR_AMBIENCE_SILENT_ZOOM: f32 = 1.0;
const STAR_AMBIENCE_TWEEN: AudioTween = AudioTween::linear(Duration::from_millis(350));
// Keep layered volleys audible without exhausting Kira's shared sound capacity.
const MAX_COMBAT_SOUND_INSTANCES: usize = 10;

#[derive(Message, Clone)]
/// Message requesting playback of one named audio asset.
pub struct PlayAudioMsg {
    pub name: &'static str,
    pub volume: f32,
    pub is_background: bool,
    /// Repeats the asset until stopped, independently of the music mute mode.
    pub is_looped: bool,
}

impl PlayAudioMsg {
    /// Creates a new value from the supplied state.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
            volume: match name {
                "explosion" | "short explosion" | "large explosion" | "death ray" => -18.0,
                "horn" | "repair" | "victory" | "draw" | "defeat" => -12.0,
                _ => 0.0,
            },
            is_background: false,
            is_looped: false,
        }
    }

    /// Repeats an effect at its normal volume until explicitly stopped.
    pub fn looped(mut self) -> Self {
        self.is_looped = true;
        self
    }

    /// Repeats a quiet ambience while leaving it available in effects-only mode.
    pub fn ambience(mut self) -> Self {
        self.volume = PlayingAudio::AMBIENCE_VOLUME;
        self.is_looped = true;
        self
    }

    /// Marks an audio request as looping background music.
    pub fn background(mut self) -> Self {
        self.volume = PlayingAudio::BACKGROUND_VOLUME;
        self.is_background = true;
        self.is_looped = true;
        self
    }
}

#[derive(Message, Clone)]
/// Message requesting that matching audio instances pause.
pub struct PauseAudioMsg {
    pub name: &'static str,
}

impl PauseAudioMsg {
    /// Creates a new value from the supplied state.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
        }
    }
}

#[derive(Message, Clone)]
/// Message requesting that matching audio instances stop.
pub struct StopAudioMsg {
    pub name: &'static str,
}

impl StopAudioMsg {
    /// Creates a new value from the supplied state.
    pub fn new(name: &'static str) -> Self {
        Self {
            name,
        }
    }
}

#[derive(Message, Clone)]
/// Message requesting reapplication of the configured mute mode.
pub struct MuteAudioMsg;

#[derive(Message)]
/// Selects an audio mode, or toggles mute when no mode is supplied.
pub struct ChangeAudioMsg(pub Option<AudioState>);

// Hold the wheel feedback briefly, then fade it without affecting ordinary hover interaction.
const VOLUME_SCROLL_HOLD: f64 = 0.9;
const VOLUME_SCROLL_FADE: f64 = 0.4;

fn scroll_volume(context: &egui::Context, settings: &mut Settings, in_combat: bool) {
    let id = egui::Id::new("audio volume scroll");
    if !in_combat {
        context.data_mut(|data| data.remove::<f64>(id));
        return;
    }
    let frame = context.cumulative_frame_nr();
    let already_handled = context.data_mut(|data| {
        let previous = data.get_temp::<u64>(id.with("frame"));
        data.insert_temp(id.with("frame"), frame);
        previous == Some(frame)
    });
    if already_handled {
        return;
    }
    let (delta, now) = context.input(|input| {
        let delta: f32 = input
            .events
            .iter()
            .filter_map(|event| {
                if let egui::Event::MouseWheel {
                    delta,
                    ..
                } = event
                {
                    // Wheel distance varies by platform; each vertical event is one 10% step.
                    (delta.y.is_finite() && delta.y != 0.0).then(|| delta.y.signum() * 10.0)
                } else {
                    None
                }
            })
            .sum();
        (delta, input.time)
    });
    if delta.is_finite() && delta != 0.0 {
        let current = if settings.audio == AudioState::Mute {
            0.0
        } else {
            settings.volume
        };
        settings.set_volume(((current * 100.0).round() + delta).clamp(0.0, 100.0) / 100.0);
        context.data_mut(|data| data.insert_temp(id, now));
        set_ui_sound(context, None);
    }
}

fn scroll_volume_opacity(context: &egui::Context) -> f32 {
    let last_scroll =
        context.data(|data| data.get_temp::<f64>(egui::Id::new("audio volume scroll")));
    last_scroll.map_or(0.0, |last| {
        let elapsed = context.input(|input| input.time) - last;
        ((VOLUME_SCROLL_HOLD + VOLUME_SCROLL_FADE - elapsed) / VOLUME_SCROLL_FADE).clamp(0.0, 1.0)
            as f32
    })
}

/// Draws the hover popup's master slider in the game's blue-grey palette.
pub fn volume_slider(ui: &mut egui::Ui, settings: &mut Settings) -> egui::Response {
    let mut volume = if settings.audio == AudioState::Mute {
        0.0
    } else {
        settings.volume
    };
    ui.scope(|ui| {
        let width = ui.available_width().clamp(80.0, 280.0);
        let style = ui.style_mut();
        style.spacing.slider_width = width;
        style.spacing.interact_size.y = 24.0;
        style.visuals.selection.bg_fill = egui::Color32::from_rgb(48, 119, 155);
        for state in [
            &mut style.visuals.widgets.inactive,
            &mut style.visuals.widgets.hovered,
            &mut style.visuals.widgets.active,
        ] {
            state.bg_fill = egui::Color32::from_rgb(24, 34, 45);
            state.fg_stroke = egui::Stroke::new(2.0, egui::Color32::from_rgb(220, 233, 244));
            state.corner_radius = egui::CornerRadius::same(6);
        }
        ui.label(egui::RichText::new(format!("Volume  {:.0}%", volume * 100.0)).size(18.0));
        let mut response = ui
            .add(egui::Slider::new(&mut volume, 0.0..=1.0).show_value(false).trailing_fill(true))
            .on_hover_cursor(egui::CursorIcon::PointingHand);
        if response.dragged() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        response.widget_info(|| egui::WidgetInfo::slider(true, volume as f64, "Volume"));
        if response.changed() {
            settings.set_volume(volume);
            set_ui_sound(ui.ctx(), None);
        }
        if response.hovered() {
            let previous_volume = settings.volume;
            // Share the frame guard with combat scrolling so the same event is applied once.
            scroll_volume(ui.ctx(), settings, true);
            if settings.volume != previous_volume {
                response.mark_changed();
            }
        }
        response
    })
    .inner
}

fn volume_popover(button: &egui::Response, settings: &mut Settings) -> Option<egui::Response> {
    let id = button.id.with("volume");
    let was_open = button.ctx.data(|data| data.get_temp::<bool>(id).unwrap_or(false));
    let popup = egui::Popup::from_response(button)
        .id(id)
        .gap(0.0)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .width(200.0_f32.min((button.ctx.content_rect().width() - 40.0).max(80.0)));
    // Join the hover regions and keep the slider open during a drag beyond its edges.
    let hovering = popup.get_popup_rect().is_some_and(|rect| {
        button
            .ctx
            .pointer_hover_pos()
            .is_some_and(|pos| rect.union(button.rect).expand(4.0).contains(pos))
    });
    let dragging =
        button.ctx.data(|data| data.get_temp::<bool>(id.with("dragging")).unwrap_or(false))
            && button.ctx.input(|input| input.pointer.primary_down());
    let hover_open = button.hovered() || (was_open && (hovering || dragging));
    let scroll_opacity = scroll_volume_opacity(&button.ctx);
    let opacity = if hover_open {
        1.0
    } else {
        scroll_opacity
    };
    let mut open = opacity > 0.0;
    if scroll_opacity > 0.0 {
        button.ctx.request_repaint();
    }
    let frame = egui::Frame::new()
        .fill(egui::Color32::from_rgba_unmultiplied(14, 22, 31, 245))
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(100, 133, 162)))
        .corner_radius(6.0)
        .inner_margin(10)
        .multiply_with_opacity(opacity);
    let response = popup.frame(frame).open_bool(&mut open).show(|ui| {
        ui.set_opacity(opacity);
        let response = volume_slider(ui, settings);
        ui.ctx().data_mut(|data| data.insert_temp(id.with("dragging"), response.dragged()));
        response
    });
    button.ctx.data_mut(|data| data.insert_temp(id, open));
    response.map(|response| response.inner)
}

/// Draws the HUD control at its final resolution; fine bitmap bevels blur at this size.
fn audio_mode_button(ui: &mut egui::Ui, mode: AudioState) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(egui::vec2(32.0, 32.0), egui::Sense::click());
    let painter = ui.painter();
    let center = rect.center();
    let white = egui::Color32::from_rgb(232, 242, 250);
    let cyan = egui::Color32::from_rgb(101, 202, 231);
    let highlighted = response.hovered() || response.has_focus();
    painter.circle(
        center,
        15.0,
        if highlighted {
            egui::Color32::from_rgb(27, 49, 66)
        } else {
            egui::Color32::from_rgb(14, 28, 42)
        },
        egui::Stroke::new(
            1.5,
            if highlighted {
                white
            } else {
                cyan
            },
        ),
    );
    let point = |x, y| center + egui::vec2(x, y);
    let stroke = egui::Stroke::new(1.8, white);
    match mode {
        AudioState::Mute | AudioState::NoMusic => {
            painter.add(egui::Shape::convex_polygon(
                vec![
                    point(-9.0, -3.0),
                    point(-6.0, -3.0),
                    point(-1.0, -7.0),
                    point(-1.0, 7.0),
                    point(-6.0, 3.0),
                    point(-9.0, 3.0),
                ],
                white,
                egui::Stroke::NONE,
            ));
            if mode == AudioState::Mute {
                painter.line_segment([point(3.0, -3.0), point(9.0, 3.0)], stroke);
                painter.line_segment([point(9.0, -3.0), point(3.0, 3.0)], stroke);
            } else {
                for radius in [5.0, 9.0] {
                    let points = (0..=12)
                        .map(|step| {
                            let angle = (step as f32 / 12.0 - 0.5) * 2.0;
                            point(-1.0 + radius * angle.cos(), radius * angle.sin())
                        })
                        .collect();
                    painter.add(egui::Shape::line(points, stroke));
                }
            }
        },
        AudioState::Sound => {
            painter.add(egui::Shape::convex_polygon(
                vec![point(-3.0, -6.0), point(7.0, -8.0), point(7.0, -4.5), point(-3.0, -2.5)],
                white,
                egui::Stroke::NONE,
            ));
            painter.line_segment([point(-3.0, -5.0), point(-3.0, 6.0)], stroke);
            painter.line_segment([point(7.0, -7.0), point(7.0, 4.0)], stroke);
            painter.circle_filled(point(-5.0, 6.0), 2.6, white);
            painter.circle_filled(point(5.0, 4.0), 2.6, white);
        },
    }
    response
        .widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Audio mode"));
    response.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn audio_controls(context: &egui::Context, settings: &mut Settings) -> egui::Response {
    // One inset for both axes, independent of the window aspect ratio and UI button padding.
    egui::Area::new(egui::Id::new("audio controls"))
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-20.0, 20.0))
        .order(egui::Order::Foreground)
        .show(context, |ui| {
            let button = audio_mode_button(ui, settings.audio);
            volume_popover(&button, settings);
            button
        })
        .inner
}

/// Draws the top-right audio mode icon and interactive hover volume control.
pub fn draw_audio_controls(
    mut contexts: EguiContexts,
    mut settings: ResMut<Settings>,
    app_state: Res<State<AppState>>,
    game_state: Res<State<GameState>>,
    mut change_audio: MessageWriter<ChangeAudioMsg>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let previous_audio = settings.audio;
    scroll_volume(
        context,
        &mut settings,
        *app_state.get() == AppState::Game && *game_state.get() == GameState::Combat,
    );
    if audio_controls(context, &mut settings).clicked() {
        change_audio.write(ChangeAudioMsg(None));
        set_ui_sound(context, Some(SoundEffect::Button));
    } else if settings.audio != previous_audio {
        change_audio.write(ChangeAudioMsg(Some(settings.audio)));
    }
}

/// Updates audio from the current canonical ECS projection.
pub fn update_audio(
    mut change_audio_msg: MessageReader<ChangeAudioMsg>,
    mut settings: ResMut<Settings>,
    game_state: Res<State<GameState>>,
    mut next_audio_state: ResMut<NextState<AudioState>>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut pause_audio_msg: MessageWriter<PauseAudioMsg>,
    mut stop_audio_msg: MessageWriter<StopAudioMsg>,
    mut mute_audio_msg: MessageWriter<MuteAudioMsg>,
) {
    for ev in change_audio_msg.read() {
        let mode = ev.0.unwrap_or(match settings.audio {
            AudioState::Mute => settings.restored_audio_mode(),
            AudioState::NoMusic | AudioState::Sound => AudioState::Mute,
        });
        settings.set_audio_mode(mode);

        match settings.audio {
            AudioState::Mute => {
                mute_audio_msg.write(MuteAudioMsg);
                next_audio_state.set(AudioState::Mute);
            },
            AudioState::NoMusic => {
                pause_audio_msg.write(PauseAudioMsg::new("music"));
                stop_audio_msg.write(StopAudioMsg::new("drums"));
                next_audio_state.set(AudioState::NoMusic);
            },
            AudioState::Sound => {
                match game_state.get() {
                    GameState::CombatMenu => {
                        play_audio_msg.write(PlayAudioMsg::new("drums").background());
                    },
                    GameState::Combat => (),
                    _ => {
                        play_audio_msg.write(PlayAudioMsg::new("music").background());
                    },
                }
                next_audio_state.set(AudioState::Sound);
            },
        }
    }
}

/// Toggles mute with Q and adjusts volume by ten percentage points with Up/Down.
pub fn toggle_audio(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut settings: ResMut<Settings>,
    mut change_audio_msg: MessageWriter<ChangeAudioMsg>,
) {
    if keyboard.just_pressed(KeyCode::KeyQ) {
        change_audio_msg.write(ChangeAudioMsg(None));
    }
    // Leave modified arrows available for gameplay shortcuts such as Ctrl+Up.
    if keyboard.any_pressed([
        KeyCode::ControlLeft,
        KeyCode::ControlRight,
        KeyCode::AltLeft,
        KeyCode::AltRight,
        KeyCode::SuperLeft,
        KeyCode::SuperRight,
        KeyCode::ShiftLeft,
        KeyCode::ShiftRight,
    ]) {
        return;
    }
    let step = i32::from(keyboard.just_pressed(KeyCode::ArrowUp))
        - i32::from(keyboard.just_pressed(KeyCode::ArrowDown));
    if step != 0 {
        let previous_audio = settings.audio;
        let current = if previous_audio == AudioState::Mute {
            0.0
        } else {
            settings.volume
        };
        // Work in displayed percentages so repeated presses reach zero without float residue.
        let volume = ((current * 100.0).round() + step as f32 * 10.0).clamp(0.0, 100.0) / 100.0;
        settings.set_volume(volume);
        if settings.audio != previous_audio {
            change_audio_msg.write(ChangeAudioMsg(Some(settings.audio)));
        }
    }
}

/// Requests the looping gameplay music when entering active play.
pub fn play_music(mut play_audio_msg: MessageWriter<PlayAudioMsg>) {
    play_audio_msg.write(PlayAudioMsg::new("music").background());
}

/// Keeps one quiet booster loop active only while a visible map mission is hovered during play.
pub fn update_mission_hover_audio(
    state: Option<Res<UiState>>,
    missions: Option<Res<Missions>>,
    app_state: Res<State<AppState>>,
    game_state: Res<State<GameState>>,
    settings: Res<Settings>,
    windows: Query<&Window, With<PrimaryWindow>>,
    mut playing: Local<bool>,
    mut play: MessageWriter<PlayAudioMsg>,
    mut stop: MessageWriter<StopAudioMsg>,
) {
    let hovered = *app_state.get() == AppState::Game
        && *game_state.get() == GameState::Playing
        && settings.audio != AudioState::Mute
        && windows.iter().any(|window| window.focused && window.cursor_position().is_some())
        && state.zip(missions).is_some_and(|(state, missions)| {
            !state.mission_hover_from_ui
                && state.mission_hover.is_some_and(|id| missions.get(id).is_some())
        });
    if hovered != *playing {
        if hovered {
            play.write(PlayAudioMsg::new("booster").ambience());
        } else {
            stop.write(StopAudioMsg::new("booster"));
        }
        *playing = hovered;
    }
}

fn smooth_audio_falloff(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

fn celestial_ambience_volume(camera: Vec2, zoom: f32, landmark: Vec2) -> f32 {
    let zoom_progress =
        (zoom - STAR_AMBIENCE_FULL_ZOOM) / (STAR_AMBIENCE_SILENT_ZOOM - STAR_AMBIENCE_FULL_ZOOM);
    let distance_progress = (camera.distance(landmark) - STAR_AMBIENCE_NEAR_DISTANCE)
        / (STAR_AMBIENCE_FAR_DISTANCE - STAR_AMBIENCE_NEAR_DISTANCE);
    let intensity = (1.0 - smooth_audio_falloff(zoom_progress))
        * (1.0 - smooth_audio_falloff(distance_progress));
    if intensity <= 0.001 {
        STAR_AMBIENCE_SILENCE
    } else {
        (STAR_AMBIENCE_MAX_VOLUME + 20.0 * intensity.log10()).max(STAR_AMBIENCE_SILENCE)
    }
}

/// Fades in a shared rumble near stars while closely zoomed in; black holes stay silent.
pub fn update_celestial_ambience_audio(
    camera_q: Query<(&Transform, &Projection), With<MainCamera>>,
    landmark_q: Query<
        (&GlobalTransform, Option<&CelestialCmp>),
        (Or<(With<SolarStarCmp>, With<CelestialCmp>)>, Without<MainCamera>),
    >,
    settings: Res<Settings>,
    game_state: Res<State<GameState>>,
    mut playing_audio: ResMut<PlayingAudio>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
    mut play: MessageWriter<PlayAudioMsg>,
    mut stop: MessageWriter<StopAudioMsg>,
    mut last_volume: Local<Option<f32>>,
) {
    let target_volume =
        if settings.audio != AudioState::Mute && *game_state.get() == GameState::Playing {
            camera_q.single().ok().and_then(|(camera, projection)| {
                let Projection::Orthographic(projection) = projection else {
                    return None;
                };
                landmark_q
                    .iter()
                    .filter(|(_, celestial)| {
                        celestial.is_none_or(|celestial| celestial.kind != CelestialKind::BlackHole)
                    })
                    .map(|(landmark, _)| {
                        celestial_ambience_volume(
                            camera.translation.truncate(),
                            projection.scale,
                            landmark.translation().truncate(),
                        )
                    })
                    .max_by(f32::total_cmp)
            })
        } else {
            None
        };

    let Some(target_volume) = target_volume else {
        if playing_audio.0.contains_key(STAR_AMBIENCE_NAME) {
            stop.write(StopAudioMsg::new(STAR_AMBIENCE_NAME));
        }
        *last_volume = None;
        return;
    };

    let Some(handles) = playing_audio.0.get_mut(STAR_AMBIENCE_NAME) else {
        let mut request = PlayAudioMsg::new(STAR_AMBIENCE_NAME).looped();
        request.volume = STAR_AMBIENCE_SILENCE;
        play.write(request);
        *last_volume = None;
        return;
    };

    if last_volume.is_none_or(|previous| (previous - target_volume).abs() >= 0.5) {
        let mut updated = false;
        for handle in handles {
            if let Some(mut instance) = audio_instances.get_mut(&handle.handle) {
                handle.base_volume = target_volume;
                instance.set_decibels(
                    output_volume(target_volume, settings.volume),
                    STAR_AMBIENCE_TWEEN,
                );
                updated = true;
            }
        }
        if updated {
            *last_volume = Some(target_volume);
        }
    }
}

/// Starts queued audio handles with the requested channel and volume settings.
pub fn play_audio(
    mut play_audio_msg: MessageReader<PlayAudioMsg>,
    settings: Res<Settings>,
    mut playing_audio: ResMut<PlayingAudio>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
    audio: Res<Audio>,
    assets: Res<WorldAssets>,
) {
    // Kira removes completed instances in PreUpdate. Keep every live one-shot so
    // mute/stop also reaches overlapping clicks, then discard finished handles.
    // Queued sounds may still be waiting for their asset and have no instance yet.
    playing_audio.0.retain(|_, handles| {
        handles.retain(|handle| {
            audio_instances.get(&handle.handle).map_or_else(
                || matches!(audio.state(&handle.handle), PlaybackState::Queued),
                |instance| !matches!(instance.state(), PlaybackState::Stopped),
            )
        });
        !handles.is_empty()
    });

    for message in play_audio_msg.read() {
        if settings.audio == AudioState::Mute
            || (message.is_background && settings.audio == AudioState::NoMusic)
        {
            continue;
        }

        if message.is_looped {
            if let Some(handles) = playing_audio.0.get(message.name) {
                for handle in handles {
                    if let Some(mut instance) = audio_instances.get_mut(&handle.handle) {
                        if matches!(
                            instance.state(),
                            PlaybackState::Paused { .. } | PlaybackState::Pausing { .. }
                        ) {
                            instance.resume(PlayingAudio::TWEEN);
                        }
                    }
                }
                continue;
            }
        }

        if matches!(
            message.name,
            "explosion" | "short explosion" | "large explosion" | "death ray" | "repair"
        ) && playing_audio
            .0
            .get(message.name)
            .is_some_and(|handles| handles.len() >= MAX_COMBAT_SOUND_INSTANCES)
        {
            // Drop excess cues instead of delaying them past their animation.
            continue;
        }

        let mut playback = audio.play(assets.audio(message.name));
        playback.with_volume(output_volume(message.volume, settings.volume));
        if message.is_background {
            playback.fade_in(PlayingAudio::TWEEN);
        }
        if message.is_looped {
            playback.looped();
        }
        playing_audio.0.entry(message.name).or_default().push(AudioPlayback {
            handle: playback.handle(),
            base_volume: message.volume,
            master_volume: settings.volume,
        });
    }
}

/// Combines cue gain with a linear master level; Kira treats -60 dB as silence.
fn output_volume(base_volume: f32, master: f32) -> f32 {
    if !master.is_finite() || master <= 0.0 {
        -60.0
    } else {
        (base_volume + 20.0 * master.min(1.0).log10()).max(-60.0)
    }
}

/// Updates live instances without restarting loops or flattening their individual balance.
pub fn update_master_volume(
    settings: Res<Settings>,
    mut playing_audio: ResMut<PlayingAudio>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
) {
    for playback in playing_audio.0.values_mut().flatten() {
        if playback.master_volume == settings.volume {
            continue;
        }
        // A queued asset may not have an instance yet. Retry it when it starts.
        if let Some(mut instance) = audio_instances.get_mut(&playback.handle) {
            instance.set_decibels(
                output_volume(playback.base_volume, settings.volume),
                AudioTween::linear(Duration::from_millis(50)),
            );
            playback.master_volume = settings.volume;
        }
    }
}

/// Pauses matching active audio instances without discarding their positions.
pub fn pause_audio(
    mut pause_audio_msg: MessageReader<PauseAudioMsg>,
    playing_audio: Res<PlayingAudio>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
    settings: Res<Settings>,
) {
    for message in pause_audio_msg.read() {
        if let Some(handles) = playing_audio.0.get(message.name) {
            for handle in handles {
                if let Some(mut instance) = audio_instances.get_mut(&handle.handle) {
                    instance.pause(if settings.audio == AudioState::Mute {
                        AudioTween::default()
                    } else {
                        PlayingAudio::TWEEN
                    });
                }
            }
        }
    }
}

/// Stops matching active audio instances and removes their tracking handles.
pub fn stop_audio(
    mut stop_audio_msg: MessageReader<StopAudioMsg>,
    mut playing_audio: ResMut<PlayingAudio>,
    mut audio_instances: ResMut<Assets<AudioInstance>>,
) {
    for message in stop_audio_msg.read() {
        if let Some(handles) = playing_audio.0.remove(message.name) {
            for handle in handles {
                if let Some(mut instance) = audio_instances.get_mut(&handle.handle) {
                    instance.stop(AudioTween::default());
                }
            }
        }
    }
}

/// Applies the current mute mode to music and effect channels.
pub fn mute_audio(
    mut mute_audio_msg: MessageReader<MuteAudioMsg>,
    playing_audio: Res<PlayingAudio>,
    mut pause_audio_msg: MessageWriter<PauseAudioMsg>,
    mut stop_audio_msg: MessageWriter<StopAudioMsg>,
) {
    for _ in mute_audio_msg.read() {
        for name in playing_audio.0.keys() {
            if *name == "music" {
                pause_audio_msg.write(PauseAudioMsg::new(name));
            } else {
                stop_audio_msg.write(StopAudioMsg::new(name));
            }
        }
    }
}

#[cfg(test)]
#[path = "../../tests/core/audio.rs"]
mod tests;
