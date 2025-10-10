use crate::core::map::planet::Planet;
use crate::core::resources::Resources;
use crate::core::units::missions::Mission;
use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: ClientId,
    pub resources: Resources,
    pub missions: Vec<Mission>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            id: 0,
            resources: Resources {
                metal: 1500,
                crystal: 1500,
                deuterium: 1500,
            },
            missions: vec![],
        }
    }
}

impl Player {
    pub fn new(id: ClientId) -> Self {
        Self {
            id,
            ..default()
        }
    }

    pub fn controls(&self, planet: &Planet) -> bool {
        planet.owner == Some(self.id)
    }

    pub fn resource_production(&self, planets: &Vec<Planet>) -> Resources {
        planets.iter().filter(|p| p.owner == Some(self.id)).map(|p| p.resource_production()).sum()
    }
}
