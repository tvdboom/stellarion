use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

use crate::core::combat::MissionReport;
use crate::core::map::icon::Icon;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Unit};

pub struct PlanetInfo {
    pub turn: usize,
    pub owner: Option<ClientId>,
    pub controlled: Option<ClientId>,
    pub army: Army,
    pub objective: Icon,
}

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: ClientId,
    pub home_planet: PlanetId,
    pub resources: Resources,
    pub reports: Vec<MissionReport>,
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
            reports: Vec::new(),
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
        planet.controlled() == Some(self.id)
    }

    pub fn resource_production(&self, planets: &Vec<Planet>) -> Resources {
        planets.iter().filter(|p| p.owned == Some(self.id)).map(|p| p.resource_production()).sum()
    }

    pub fn last_info(&self, id: PlanetId) -> Option<PlanetInfo> {
        let mut last_report = None;
        for r in self.reports.iter().rev() {
            if r.mission.owner == self.id && r.mission.origin == id {
                // Mission send by the player
                last_report = Some((r.mission.send, r.mission.origin_army.clone()));
            } else if r.planet.controlled == Some(self.id)
                && r.mission.destination == id
                && r.surviving_defender.amount(&Unit::Building(Building::Mine)) > 0
            {
                // Player was the defender and owned the planet
                last_report = Some((r.turn, r.surviving_defender.clone()));
            } else if r.planet.controlled != Some(self.id)
                && r.mission.destination == id
                && 
            {
                // Player was the attacker and won the battle
            }
            // r.defender != Some(self.id)
            //     && r.mission.destination == planet_id
            // && r.mission.objective != Icon::MissileStrike)
        }

        None
    }
}
