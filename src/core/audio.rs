//! Bevy/Kira audio resources and systems for music, effects, and mute state.

use std::collections::HashMap;
use std::time::Duration;

use bevy::prelude::*;
use bevy::window::{PrimaryWindow, SystemCursorIcon};
use bevy_egui::{egui, EguiContexts};
use bevy_kira_audio::prelude::*;

use crate::core::assets::WorldAssets;
use crate::core::camera::MainCamera;
use crate::core::map::systems::{BlackHoleCmp, SolarStarCmp};
use crate::core::map::utils::cursor;
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
pub struct PlayingAudio(pub HashMap<&'static str, Vec<Handle<AudioInstance>>>);

impl PlayingAudio {
    pub const BACKGROUND_VOLUME: f32 = -30.;
    /// Gain for the continuous hovered-mission booster ambience, in decibels.
    pub const AMBIENCE_VOLUME: f32 = -12.;
    pub const TWEEN: AudioTween = AudioTween::new(Duration::from_secs(2), AudioEasing::OutPowi(2));
}

const STAR_AMBIENCE_NAME: &str = "star ambience";
const STAR_AMBIENCE_MAX_VOLUME: f32 = -24.0;
const STAR_AMBIENCE_SILENCE: f32 = -60.0;
const STAR_AMBIENCE_NEAR_DISTANCE: f32 = 260.0;
const STAR_AMBIENCE_FAR_DISTANCE: f32 = 1_100.0;
const STAR_AMBIENCE_FULL_ZOOM: f32 = 0.6;
const STAR_AMBIENCE_SILENT_ZOOM: f32 = 1.0;
const STAR_AMBIENCE_TWEEN: AudioTween = AudioTween::linear(Duration::from_millis(350));

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
            volume: 0.0,
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

#[derive(Component)]
/// Marker for the menu button that displays current audio mode.
pub struct MusicBtnCmp;

#[derive(Message)]
/// Message cycling to a new audio playback mode.
pub struct ChangeAudioMsg(pub Option<AudioState>);

/// Creates the audio entities and resources required on state entry.
pub fn setup_audio(mut commands: Commands, assets: Res<WorldAssets>) {
    commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                width: Val::Percent(3.),
                height: Val::Percent(3.),
                right: Val::Percent(0.),
                top: Val::Percent(2.),
                ..default()
            },
            ZIndex(5),
        ))
        .with_children(|parent| {
            parent
                .spawn((ImageNode::new(assets.image("no-music")), MusicBtnCmp))
                .observe(cursor::<Over>(SystemCursorIcon::Pointer))
                .observe(cursor::<Out>(SystemCursorIcon::Default))
                .observe(play_button_audio)
                .observe(|_: On<Pointer<Click>>, mut commands: Commands| {
                    commands.queue(|w: &mut World| {
                        w.write_message(ChangeAudioMsg(None));
                    })
                });
        });
}

/// Updates audio from the current canonical ECS projection.
pub fn update_audio(
    mut change_audio_msg: MessageReader<ChangeAudioMsg>,
    mut btn_q: Query<&mut ImageNode, With<MusicBtnCmp>>,
    mut settings: ResMut<Settings>,
    game_state: Res<State<GameState>>,
    mut next_audio_state: ResMut<NextState<AudioState>>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut pause_audio_msg: MessageWriter<PauseAudioMsg>,
    mut stop_audio_msg: MessageWriter<StopAudioMsg>,
    mut mute_audio_msg: MessageWriter<MuteAudioMsg>,
    assets: Res<WorldAssets>,
) {
    for ev in change_audio_msg.read() {
        settings.audio = ev.0.unwrap_or(match settings.audio {
            AudioState::Mute => AudioState::NoMusic,
            AudioState::NoMusic => AudioState::Sound,
            AudioState::Sound => AudioState::Mute,
        });

        if let Ok(mut node) = btn_q.single_mut() {
            node.image = match settings.audio {
                AudioState::Mute => {
                    mute_audio_msg.write(MuteAudioMsg);
                    next_audio_state.set(AudioState::Mute);
                    assets.image("mute")
                },
                AudioState::NoMusic => {
                    pause_audio_msg.write(PauseAudioMsg::new("music"));
                    stop_audio_msg.write(StopAudioMsg::new("drums"));
                    next_audio_state.set(AudioState::NoMusic);
                    assets.image("no-music")
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
                    assets.image("sound")
                },
            };
        }
    }
}

/// Cycles the configured mute/music/effects mode from keyboard input.
pub fn toggle_audio(
    keyboard: Res<ButtonInput<KeyCode>>,
    mut change_audio_msg: MessageWriter<ChangeAudioMsg>,
) {
    if keyboard.just_pressed(KeyCode::KeyQ) {
        change_audio_msg.write(ChangeAudioMsg(None));
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

/// Fades in one shared rumble near either massive landmark, but only while closely zoomed in.
pub fn update_celestial_ambience_audio(
    camera_q: Query<(&Transform, &Projection), With<MainCamera>>,
    landmark_q: Query<
        &GlobalTransform,
        (Or<(With<SolarStarCmp>, With<BlackHoleCmp>)>, Without<MainCamera>),
    >,
    settings: Res<Settings>,
    game_state: Res<State<GameState>>,
    playing_audio: Res<PlayingAudio>,
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
                    .map(|landmark| {
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

    let Some(handles) = playing_audio.0.get(STAR_AMBIENCE_NAME) else {
        let mut request = PlayAudioMsg::new(STAR_AMBIENCE_NAME).looped();
        request.volume = STAR_AMBIENCE_SILENCE;
        play.write(request);
        *last_volume = None;
        return;
    };

    if last_volume.is_none_or(|previous| (previous - target_volume).abs() >= 0.5) {
        let mut updated = false;
        for handle in handles {
            if let Some(mut instance) = audio_instances.get_mut(handle) {
                instance.set_decibels(target_volume, STAR_AMBIENCE_TWEEN);
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
    playing_audio.0.retain(|_, handles| {
        handles.retain(|handle| {
            audio_instances
                .get(handle)
                .is_some_and(|instance| !matches!(instance.state(), PlaybackState::Stopped))
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
                    if let Some(mut instance) = audio_instances.get_mut(handle) {
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

        let mut playback = audio.play(assets.audio(message.name));
        playback.with_volume(message.volume);
        if message.is_background {
            playback.fade_in(PlayingAudio::TWEEN);
        }
        if message.is_looped {
            playback.looped();
        }
        playing_audio.0.entry(message.name).or_default().push(playback.handle());
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
                if let Some(mut instance) = audio_instances.get_mut(handle) {
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
                if let Some(mut instance) = audio_instances.get_mut(&handle) {
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
