use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::states::AudioState;

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub audio: AudioState,
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
    pub fn speed(&self) -> f32 {
        if self.combat_paused {
            0.
        } else {
            self.combat_speed
        }
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            audio: AudioState::default(),
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
