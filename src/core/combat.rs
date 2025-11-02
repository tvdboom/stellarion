use bevy_renet::renet::ClientId;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::Mission;
use crate::core::player::Player;
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Description, Unit};

#[derive(PartialEq)]
pub enum Side {
    Attacker,
    Defender,
}

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
    /// Turn the report was generated
    pub turn: usize,

    /// Mission that created the report
    pub mission: Mission,

    /// Planet as it was before the mission resolution
    pub planet: Planet,

    /// Number of probes that left after one round of combat
    pub scout_probes: usize,

    /// Surviving units from the attacker
    pub surviving_attacker: Army,

    /// Surviving units from the defender
    pub surviving_defender: Army,

    /// Whether the planet was colonized
    pub planet_colonized: bool,

    /// Whether the planet was destroyed
    pub planet_destroyed: bool,

    /// Combat logs (if combat took place)
    pub logs: Option<String>,
}

impl MissionReport {
    pub fn winner(&self) -> Option<ClientId> {
        match self.mission.objective {
            Icon::Deploy => None,
            Icon::Spy if self.scout_probes > 0 => Some(self.mission.owner),
            _ => {
                if self.surviving_attacker.iter().any(|(_, c)| *c > 0) {
                    Some(self.mission.owner)
                } else {
                    self.planet.controlled
                }
            },
        }
    }

    pub fn image(&self, player: &Player) -> &str {
        if self.winner() == Some(player.id) {
            "win"
        } else {
            "lose"
        }
    }

    /// Return the amount of units from a side that survived
    pub fn amount(&self, unit: &Unit, side: &Side, player: &Player) -> Option<usize> {
        match side {
            Side::Attacker
                if self.mission.owner == player.id
                    || self.planet.owned == Some(player.id)
                    || self.winner() == Some(player.id) =>
            {
                Some(self.surviving_attacker.amount(unit))
            },
            Side::Defender
                if self.planet.controlled == Some(player.id)
                    || self.winner() == Some(player.id)
                    || self.scout_probes > 10 * (unit.production() - 1) =>
            {
                Some(self.surviving_defender.amount(unit))
            },
            _ => None,
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

pub fn combat(turn: usize, mission: &Mission, destination: &Planet) -> MissionReport {
    if mission.objective == Icon::Deploy {
        return MissionReport {
            turn,
            mission: mission.clone(),
            defender,
            defense: defense.clone(),
            scout_probes: 0,
            surviving_attacker: mission.army.clone(),
            surviving_defender: defense.clone(),
            planet_colonized: false,
            planet_destroyed: false,
            logs: None,
        };
    }

    let mut attack_army: Vec<CombatUnit> = mission
        .army
        .iter()
        .filter(|(u, _)| **u != Unit::colony_ship())
        .flat_map(|(unit, count)| std::iter::repeat(CombatUnit::new(unit)).take(*count))
        .collect();

    let mut defend_army: Vec<CombatUnit> = defense
        .iter()
        .filter(|(u, _)| {
            !u.is_building()
                && **u != Unit::colony_ship()
                && !matches!(u, Unit::Defense(d) if d.is_missile())
        })
        .flat_map(|(unit, count)| std::iter::repeat(CombatUnit::new(unit)).take(*count))
        .collect();

    // Bring missiles down with antiballistic
    let mut n_antiballistic = defense
        .iter()
        .filter_map(|(u, c)| (*u == Unit::Defense(Defense::AntiballisticMissile)).then_some(c))
        .count();

    let planetary_shield = defense.amount(&Unit::Building(Building::PlanetaryShield));

    let mut missiles_fired = 0;
    let mut missiles_hit = 0;
    while n_antiballistic > 0 {
        if let Some(pos) = attack_army
            .iter()
            .position(|cu| cu.unit == Unit::Defense(Defense::AntiballisticMissile))
        {
            n_antiballistic -= 1;
            missiles_fired += 1;
            if rng().random::<f32>() < 0.5 {
                missiles_hit += 1;
                attack_army.remove(pos);
            }
        } else {
            break; // No Interplanetary Missiles left
        }
    }

    let mut logs = String::new();

    let mut round = 1;
    let mut returning_probes = 0;
    let mut planet_destroyed = false;
    while (!attack_army.is_empty() && !defend_army.is_empty()) || round == 1 {
        logs.push_str(&format!(
            "{}Round {round}",
            if round == 1 {
                ""
            } else {
                "\n\n"
            }
        ));

        if missiles_fired > 0 && round == 1 {
            logs.push_str(&format!(
                "\n- {missiles_fired} Antiballistic Missiles destroyed {missiles_hit} incoming Interplanetary Missiles."
            ));
        }

        for army in [&mut attack_army, &mut defend_army] {
            // Reset all shields
            army.iter_mut().for_each(|u| u.shield = u.unit.shield());
        }

        // Remove units that are destroyed after both armies have fired
        attack_army.retain(|u| u.hull > 0);
        defend_army.retain(|u| u.hull > 0);

        if round == 1 {
            // Send probes back if there are still remaining enemies or objective is spying
            let probes = attack_army.iter().filter(|u| u.unit == Unit::Ship(Ship::Probe)).count();
            if ((!mission.combat_probes && !defend_army.is_empty())
                || mission.objective == Icon::Spy)
                && probes > 0
            {
                attack_army.retain(|u| u.unit != Unit::Ship(Ship::Probe));
                returning_probes = probes;
                logs.push_str(&format!("\n- {probes} probes leaving combat."));
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

        round += 1;
    }

    // Remove any surviving Interplanetary Missiles
    attack_army.retain(|u| u.unit != Unit::Defense(Defense::InterplanetaryMissile));

    // Calculate the surviving units
    let mut surviving_attacker = attack_army.iter().fold(Army::new(), |mut army, cu| {
        *army.entry(cu.unit).or_insert(0) += 1;
        army
    });

    let mut surviving_defense = defend_army.iter().fold(Army::new(), |mut army, cu| {
        *army.entry(cu.unit).or_insert(0) += 1;
        army
    });

    // Add the scout probes to the surviving attacker
    *surviving_attacker.entry(Unit::probe()).or_insert(0) += returning_probes;

    // Add the buildings to the surviving defense
    if !planet_destroyed {
        surviving_defense = surviving_defense
            .iter()
            .chain(defense.iter().filter(|(u, _)| u.is_building()))
            .map(|(u, &v)| (u.clone(), v))
            .collect();
    }

    // If no defense, the attacker won, add the non-combat ships
    if surviving_defense.is_empty() {
        *surviving_attacker.entry(Unit::colony_ship()).or_insert(0) =
            mission.army.amount(&Unit::colony_ship());
    }

    // If no attacker, the defender won, add non-combat ships and the remaining missiles
    if surviving_attacker.is_empty() {
        *surviving_defense.entry(Unit::colony_ship()).or_insert(0) =
            *defense.get(&Unit::interplanetary_missile()).unwrap_or(&0);
        *surviving_defense.entry(Unit::Defense(Defense::AntiballisticMissile)).or_insert(0) =
            n_antiballistic;
        *surviving_defense.entry(Unit::interplanetary_missile()).or_insert(0) =
            *defense.get(&Unit::interplanetary_missile()).unwrap_or(&0);
    }

    MissionReport {
        turn,
        mission: mission.clone(),
        defender,
        defense: defense.clone(),
        scout_probes: returning_probes,
        surviving_attacker,
        surviving_defender: surviving_defense,
        planet_colonized: defend_army.is_empty() && mission.objective == Icon::Colonize,
        planet_destroyed,
        logs: Some(logs),
    }
}
