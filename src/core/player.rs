use crate::core::map::map::PlanetId;
use crate::core::resources::Resources;
use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: ClientId,
    pub resources: Resources,
    pub planets: Vec<PlanetId>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            id: 0,
            resources: Resources {
                metal: 1500,
                crystal: 1500,
                deuterium: 1500,
                energy: 1500,
            },
            planets: vec![0],
        }
    }
}

impl Player {
    pub fn new(id: ClientId, home_planet: PlanetId) -> Self {
        Self {
            id,
            planets: vec![home_planet],
            ..default()
        }
    }
}
