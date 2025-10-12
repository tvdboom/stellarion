use crate::core::map::planet::{Planet, PlanetId};
use crate::core::resources::Resources;
use crate::core::units::missions::Mission;
use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: ClientId,
    pub home_planet: PlanetId,
    pub resources: Resources,
    pub missions: Vec<Mission>,
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

    pub fn controls(&self, planet: &Planet) -> bool {
        planet.owner == Some(self.id)
    }

    /// Return the planets owned by the player, with the home planet first
    pub fn planets<'a>(&self, planets: &'a Vec<Planet>) -> Vec<&'a Planet> {
        let (home, others): (Vec<_>, Vec<_>) = planets
            .iter()
            .filter(|p| p.owner == Some(self.id))
            .partition(|p| p.id == self.home_planet);

        home.into_iter().chain(others).collect()
    }

    pub fn resource_production(&self, planets: &Vec<Planet>) -> Resources {
        planets.iter().filter(|p| p.owner == Some(self.id)).map(|p| p.resource_production()).sum()
    }
}
