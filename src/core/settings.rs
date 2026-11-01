//! User preferences and local match-generation settings used by the Bevy layer.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::states::AudioState;

/// Local combat renderer selected before a replay begins.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatView {
    /// Tactical cards and phase animations.
    #[default]
    Schematic,
    /// Cinematic fleet replay.
    Cinematic,
}

/// Player-specific choices saved with a game on the backend, never on disk.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GamePreferences {
    /// Enabled audio channels.
    pub audio: AudioState,
    /// Master output level in the inclusive range zero to one.
    pub volume: f32,
    /// Audio mode restored after mute.
    pub unmuted_audio: AudioState,
    /// Positive output level restored after mute.
    pub unmuted_volume: f32,
    /// Whether map territory cells and boundaries are visible.
    pub show_cells: bool,
    /// Whether planet information is always visible.
    pub show_info: bool,
    /// Whether hover cards use the shorter presentation.
    pub brief_hover_info: bool,
    /// Whether the planet shop is shown when selecting a world.
    pub show_menu: bool,
    /// Combat playback multiplier, from 0.25 to 64.
    pub combat_speed: f32,
    /// Whether unit kinds fire together.
    pub combat_volley_fire: bool,
    /// Whether combatants use individual cards.
    pub combat_individual_units: bool,
    /// Preferred replay renderer.
    pub combat_view: CombatView,
}

impl Default for GamePreferences {
    fn default() -> Self {
        Settings::default().game_preferences()
    }
}

impl GamePreferences {
    /// Rejects invalid numeric values at the persistence boundary.
    pub fn validate(&self) -> Result<(), &'static str> {
        if !self.volume.is_finite()
            || !(0.0..=1.0).contains(&self.volume)
            || !self.unmuted_volume.is_finite()
            || !(0.0..=1.0).contains(&self.unmuted_volume)
            || self.unmuted_volume == 0.0
            || !self.combat_speed.is_finite()
            || !(0.25..=64.0).contains(&self.combat_speed)
        {
            return Err("game preferences contain invalid volume or combat speed");
        }
        Ok(())
    }
}

#[derive(Resource, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Session preferences and map-generation values; fresh program sessions always use defaults.
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
    /// Keeps hover cards to prices and unit-specific controls where practical.
    pub brief_hover_info: bool,
    pub show_menu: bool,
    pub combat_paused: bool,
    pub combat_speed: f32,
    /// Plays each side's combat cards as one presentation volley instead of one kind at a time.
    pub combat_volley_fire: bool,
    /// Renders each combatant as its own card rather than aggregating equal unit kinds.
    #[serde(default)]
    pub combat_individual_units: bool,
    /// Replay renderer retained across games in this program session.
    pub combat_view: CombatView,
    pub turn: usize,
}

impl Settings {
    /// Captures preferences for a game save, excluding transient playback and turn state.
    pub fn game_preferences(&self) -> GamePreferences {
        GamePreferences {
            audio: self.audio,
            volume: self.volume,
            unmuted_audio: self.unmuted_audio,
            unmuted_volume: self.unmuted_volume,
            show_cells: self.show_cells,
            show_info: self.show_info,
            brief_hover_info: self.brief_hover_info,
            show_menu: self.show_menu,
            combat_speed: self.combat_speed,
            combat_volley_fire: self.combat_volley_fire,
            combat_individual_units: self.combat_individual_units,
            combat_view: self.combat_view,
        }
    }

    /// Restores saved-game preferences without replacing game rules or the turn.
    pub fn apply_game_preferences(&mut self, preferences: GamePreferences) {
        self.audio = preferences.audio;
        self.volume = preferences.volume;
        self.unmuted_audio = preferences.unmuted_audio;
        self.unmuted_volume = preferences.unmuted_volume;
        self.show_cells = preferences.show_cells;
        self.show_info = preferences.show_info;
        self.brief_hover_info = preferences.brief_hover_info;
        self.show_menu = preferences.show_menu;
        self.combat_speed = preferences.combat_speed;
        self.combat_volley_fire = preferences.combat_volley_fire;
        self.combat_individual_units = preferences.combat_individual_units;
        self.combat_view = preferences.combat_view;
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
            brief_hover_info: false,
            show_menu: true,
            combat_paused: false,
            combat_speed: 1.0,
            combat_volley_fire: false,
            combat_individual_units: false,
            combat_view: CombatView::default(),
            turn: 1,
        }
    }
}

fn default_volume() -> f32 {
    1.0
}

#[cfg(test)]
#[path = "../../tests/core/settings.rs"]
mod tests;
