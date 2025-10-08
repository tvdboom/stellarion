use crate::core::map::planet::{Planet, PlanetId};
use crate::core::resources::Resources;
use crate::core::units::defense::Battery;
use crate::core::units::missions::Mission;
use crate::core::units::ships::Fleet;
use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: ClientId,
    pub resources: Resources,
    pub planets: Vec<PlanetId>,
    pub fleets: HashMap<PlanetId, Fleet>,
    pub defenses: HashMap<PlanetId, Battery>,
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
            planets: vec![0],
            fleets: HashMap::new(),
            defenses: HashMap::new(),
            missions: vec![],
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

    pub fn controls(&self, planet: &PlanetId) -> bool {
        self.planets.contains(planet)
    }

    pub fn production(&self, planets: &Vec<Planet>) -> Resources {
        planets.iter().filter(|p| self.controls(&p.id)).map(|p| p.production()).sum()
    }
}
