//! User preferences and local match-generation settings used by the Bevy layer.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::states::AudioState;

#[derive(Resource, Clone, Serialize, Deserialize)]
/// Local preferences and map-generation values; deterministic values are copied into game rules.
pub struct Settings {
    pub audio: AudioState,
    /// Master output level from zero (silent) to one (full volume).
    #[serde(default = "default_volume")]
    pub volume: f32,
    /// Last enabled mode, restored when leaving mute.
    #[serde(default)]
    pub unmuted_audio: AudioState,
    /// Last audible master level, retained while the slider shows zero.
    #[serde(default = "default_volume")]
    pub unmuted_volume: f32,
    pub n_planets: usize,
    pub p_colonizable: usize,
    pub p_moons: usize,
    pub autosave: bool,
    pub show_cells: bool,
    pub show_info: bool,
    pub show_hover: bool,
    pub show_menu: bool,
    pub combat_paused: bool,
    pub combat_speed: f32,
    pub turn: usize,
}

impl Settings {
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

    /// Returns the enabled mode to restore, including for older saved preferences.
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
            autosave: false,
            show_cells: true,
            show_info: false,
            show_hover: true,
            show_menu: true,
            combat_paused: false,
            combat_speed: 1.0,
            turn: 1,
        }
    }
}

fn default_volume() -> f32 {
    1.0
}
