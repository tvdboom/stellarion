//! User preferences and local match-generation settings used by the Bevy layer.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::states::AudioState;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Presentation preferences retained in the client-local player profile.
pub struct CombatPreferences {
    /// Playback multiplier for combat presentation.
    pub speed: f32,
    /// Whether each side presents all firing unit kinds together.
    pub volley_fire: bool,
}

impl Default for CombatPreferences {
    fn default() -> Self {
        Self {
            speed: 1.0,
            volley_fire: false,
        }
    }
}

#[derive(Resource, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Local preferences and map-generation values; deterministic values are copied into game rules.
pub struct Settings {
    pub audio: AudioState,
    /// Master output level from zero (silent) to one (full volume).
    pub volume: f32,
    /// Last enabled mode, restored when leaving mute.
    pub unmuted_audio: AudioState,
    /// Last audible master level, retained while the slider shows zero.
    pub unmuted_volume: f32,
    pub n_planets: usize,
    pub p_colonizable: usize,
    pub p_moons: usize,
    pub show_cells: bool,
    pub show_info: bool,
    pub show_hover: bool,
    pub show_menu: bool,
    pub combat_paused: bool,
    pub combat_speed: f32,
    /// Plays each side's combat cards as one presentation volley instead of one kind at a time.
    pub combat_volley_fire: bool,
    pub turn: usize,
}

impl Settings {
    /// Returns the combat preferences suitable for client-local persistence.
    pub fn combat_preferences(&self) -> CombatPreferences {
        CombatPreferences {
            speed: self.combat_speed,
            volley_fire: self.combat_volley_fire,
        }
    }

    /// Restores persisted combat preferences while rejecting invalid playback speeds.
    pub fn apply_combat_preferences(&mut self, preferences: CombatPreferences) {
        self.combat_speed = if preferences.speed.is_finite() {
            preferences.speed.clamp(0.25, 64.0)
        } else {
            CombatPreferences::default().speed
        };
        self.combat_volley_fire = preferences.volley_fire;
    }

    /// Changes audio mode while preserving the level and mode across mute.
    pub fn set_audio_mode(&mut self, mode: AudioState) {
        if mode == AudioState::Mute {
            if self.audio != AudioState::Mute {
                self.unmuted_audio = self.audio;
            }
            if self.volume.is_finite() && self.volume > 0.0 {
                self.unmuted_volume = self.volume.min(1.0);
            }
            self.volume = 0.0;
        } else {
            if self.volume <= 0.0 || !self.volume.is_finite() {
                self.volume = if self.unmuted_volume.is_finite() && self.unmuted_volume > 0.0 {
                    self.unmuted_volume.min(1.0)
                } else {
                    default_volume()
                };
            }
            self.unmuted_audio = mode;
        }
        self.audio = mode;
    }

    /// Returns the enabled mode to restore after muting.
    pub fn restored_audio_mode(&self) -> AudioState {
        match self.unmuted_audio {
            AudioState::Mute | AudioState::NoMusic => AudioState::NoMusic,
            AudioState::Sound => AudioState::Sound,
        }
    }

    /// Applies a slider level; zero mutes and a positive value restores the previous mode.
    pub fn set_volume(&mut self, volume: f32) {
        if !volume.is_finite() {
            return;
        }
        if volume <= 0.0 {
            self.set_audio_mode(AudioState::Mute);
        } else {
            if self.audio == AudioState::Mute {
                self.set_audio_mode(self.restored_audio_mode());
            }
            self.volume = volume.min(1.0);
            self.unmuted_volume = self.volume;
        }
    }

    /// Returns the movement or animation speed represented by this value.
    pub fn speed(&self) -> f32 {
        if self.combat_paused {
            0.
        } else {
            self.combat_speed
        }
    }
}

impl Default for Settings {
    /// Constructs the default value and its gameplay-safe initial state.
    fn default() -> Self {
        Self {
            audio: AudioState::default(),
            volume: default_volume(),
            unmuted_audio: AudioState::default(),
            unmuted_volume: default_volume(),
            n_planets: 10,
            p_colonizable: 25,
            p_moons: 30,
            show_cells: true,
            show_info: false,
            show_hover: true,
            show_menu: true,
            combat_paused: false,
            combat_speed: 1.0,
            combat_volley_fire: false,
            turn: 1,
        }
    }
}

fn default_volume() -> f32 {
    1.0
}
