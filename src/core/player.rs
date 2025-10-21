use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

use crate::core::map::planet::{Planet, PlanetId};
use crate::core::missions::{Mission, MissionId};
use crate::core::resources::Resources;

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: ClientId,
    pub home_planet: PlanetId,
    pub resources: Resources,
    pub missions: Vec<Mission>,
    pub enemy_missions: Vec<Mission>,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            id: 0,
            home_planet: 0,
            resources: Resources {
                metal: 1500,
                crystal: 1500,
                deuterium: 1500,
            },
            missions: vec![],
            enemy_missions: vec![],
        }
    }
}

impl Player {
    pub fn new(id: ClientId, home_planet: PlanetId) -> Self {
        Self {
            id,
            home_planet,
            ..default()
        }
    }

    pub fn owns(&self, planet: &Planet) -> bool {
        planet.owned == Some(self.id)
    }

    pub fn controls(&self, planet: &Planet) -> bool {
        planet.controlled == Some(self.id)
    }

    pub fn resource_production(&self, planets: &Vec<Planet>) -> Resources {
        planets.iter().filter(|p| p.owned == Some(self.id)).map(|p| p.resource_production()).sum()
    }

    pub fn get_mission(&self, mission_id: MissionId) -> &Mission {
        self.missions.iter().find(|m| m.id == mission_id).expect("Mission not found.")
    }
}
