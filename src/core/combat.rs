use std::collections::HashMap;

use bevy_renet::renet::ClientId;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::map::icon::Icon;
use crate::core::missions::Mission;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Army, Combat, Description, Unit};

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
pub struct MissionReport {
    pub turn: usize,
    pub mission: Mission,
    pub defender: Option<ClientId>,
    pub defense: Army,
    pub returning_probes: usize,
    pub surviving_attacker: Army,
    pub surviving_defense: Army,
    pub planetary_shield: usize,
    pub planet_colonized: bool,
    pub planet_destroyed: bool,
}

impl MissionReport {
    pub fn winner(&self) -> Option<ClientId> {
        if self.surviving_attacker.iter().any(|(_, c)| *c > 0) {
            Some(self.mission.owner)
        } else {
            self.defender
        }
    }
}

#[derive(Clone, Debug)]
pub struct CombatUnit {
    pub unit: Unit,
    pub hull: usize,
    pub shield: usize,
}

impl CombatUnit {
    pub fn new(unit: &Unit) -> Self {
        Self {
            unit: unit.clone(),
            hull: unit.hull(),
            shield: unit.shield(),
        }
    }
}

pub fn combat(
    turn: usize,
    mission: &Mission,
    defender: Option<ClientId>,
    defense: Army,
    planetary_shield: usize,
) -> MissionReport {
    if mission.objective == Icon::Deploy {
        return MissionReport {
            turn,
            mission: mission.clone(),
            defender,
            defense: defense.clone(),
            returning_probes: 0,
            surviving_attacker: mission.army.clone(),
            surviving_defense: defense,
            planetary_shield,
            planet_colonized: false,
            planet_destroyed: false,
        };
    }

    let mut attack_army: Vec<CombatUnit> = mission
        .army
        .iter()
        .filter(|(u, _)| **u != Unit::Ship(Ship::ColonyShip))
        .flat_map(|(unit, count)| std::iter::repeat(CombatUnit::new(unit)).take(*count))
        .collect();

    let mut defend_army: Vec<CombatUnit> = defense
        .iter()
        .filter(|(u, _)| !matches!(u, Unit::Defense(d) if d.is_missile()))
        .flat_map(|(unit, count)| std::iter::repeat(CombatUnit::new(unit)).take(*count))
        .collect();

    // Bring missiles down with anti-ballistic
    let mut n_antiballistic = defense
        .iter()
        .filter_map(|(u, c)| (*u == Unit::Defense(Defense::AntiballisticMissile)).then_some(c))
        .count();

    while n_antiballistic > 0 {
        if let Some(pos) = attack_army
            .iter()
            .position(|cu| cu.unit == Unit::Defense(Defense::AntiballisticMissile))
        {
            n_antiballistic -= 1;
            if rng().random::<f32>() < 0.5 {
                attack_army.remove(pos);
            }
        } else {
            break; // No Interplanetary Missiles left
        }
    }

    let mut round = 0;
    let mut returning_probes = 0;
    let mut planet_destroyed = false;
    while !attack_army.is_empty() && (!defend_army.is_empty() || round == 0) {
        for army in [&mut attack_army, &mut defend_army] {
            // Reset all shields
            army.iter_mut().for_each(|u| u.shield = u.unit.shield());
        }

        // Remove units that are destroyed after both armies have fired
        attack_army.retain(|u| u.hull > 0);
        defend_army.retain(|u| u.hull > 0);

        if round == 0 {
            // Send probes back if there are still remaining enemies
            let probes = attack_army.iter().filter(|u| u.unit == Unit::Ship(Ship::Probe)).count();
            if !mission.probes_stay && probes > 0 && !defend_army.is_empty() {
                attack_army.retain(|u| u.unit != Unit::Ship(Ship::Probe));
                returning_probes = probes;
            }
        }

        // Try to destroy planet
        if mission.objective == Icon::Destroy && !defend_army.iter().any(|u| u.unit.is_ship()) {
            for _ in attack_army.iter().filter(|u| u.unit == Unit::Ship(Ship::WarSun)) {
                if rng().random::<f32>() < 0.1 - 0.01 * round as f32 {
                    defend_army = vec![];
                    planet_destroyed = true;
                }
            }
        }

        println!("Round: {}", round);
        round += 1;
    }

    let mut surviving_attacker = attack_army.iter().fold(HashMap::new(), |mut army, cu| {
        *army.entry(cu.unit).or_insert(0) += 1;
        army
    });

    let mut surviving_defense = defend_army.iter().fold(HashMap::new(), |mut army, cu| {
        *army.entry(cu.unit).or_insert(0) += 1;
        army
    });

    // If no defense, the attacker won, add the non-combat ships
    if surviving_defense.is_empty() {
        *surviving_attacker.entry(Unit::Ship(Ship::ColonyShip)).or_insert(0) =
            mission.get(&Unit::Ship(Ship::ColonyShip));
    }

    // If no attacker, the defender won, add the remaining missiles
    if surviving_attacker.is_empty() {
        *surviving_defense.entry(Unit::Defense(Defense::AntiballisticMissile)).or_insert(0) =
            n_antiballistic;
        *surviving_defense.entry(Unit::Defense(Defense::InterplanetaryMissile)).or_insert(0) =
            *defense.get(&Unit::Defense(Defense::InterplanetaryMissile)).unwrap_or(&0);
    }

    MissionReport {
        turn,
        mission: mission.clone(),
        defender,
        defense: defense.clone(),
        returning_probes,
        surviving_attacker,
        surviving_defense,
        planetary_shield,
        planet_colonized: defend_army.is_empty() && mission.objective == Icon::Colonize,
        planet_destroyed,
    }
}
