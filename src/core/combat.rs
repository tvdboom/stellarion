use std::collections::HashMap;

use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::missions::Mission;
use crate::core::units::{Army, Description};

#[derive(EnumIter, Debug, PartialEq)]
pub enum CombatStats {
    Hull,
    Shield,
    Damage,
    Production,
    Speed,
    FuelConsumption,
    RapidFire,
}

impl Description for CombatStats {
    fn description(&self) -> &str {
        match self {
            CombatStats::Production => {
                "\
                Production cost of the unit. The production cost also determines the minimum level \
                of the building required to build it, as well as the minimum level a Sensor \
                Phalanx must have to see it, and the jump cost it has through a Jump Gate."
            },
            CombatStats::Hull => "The amount of damage a unit can take before being destroyed.",
            CombatStats::Shield => {
                "\
                The amount of damage a unit absorbs before it starts taking hull damage. The \
                shield is regenerated every round."
            },
            CombatStats::Damage => "The amount of damage a unit deals per round.",
            CombatStats::Speed => "The speed at which a unit travels through space (in AU / turn).",
            CombatStats::FuelConsumption => {
                "The amount of deuterium a unit requires to travel 1 AU."
            },
            CombatStats::RapidFire => {
                "The chance to fire again this round when targeting specific units."
            },
        }
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CombatReport {
    pub mission: Mission,
    pub defender: Option<ClientId>,
    pub defense: Army,
    pub surviving_attacker: Army,
    pub surviving_defense: Army,
    pub planetary_shield: usize,
    pub planet_colonized: bool,
    pub planet_destroyed: bool,
}

impl CombatReport {
    pub fn winner(&self) -> Option<ClientId> {
        if !self.surviving_attacker.is_empty() {
            Some(self.mission.owner)
        } else {
            self.defender
        }
    }
}

pub fn combat(
    mission: &Mission,
    defender: Option<ClientId>,
    defense: Army,
    planetary_shield: usize,
) -> CombatReport {
    // Surviving missiles are destroyed

    CombatReport {
        mission: mission.clone(),
        defender,
        defense: defense.clone(),
        surviving_attacker: HashMap::new(),
        surviving_defense: defense,
        planetary_shield,
        planet_colonized: false,
        planet_destroyed: false,
    }
}
