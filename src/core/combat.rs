use bevy_renet::renet::ClientId;
use rand::prelude::IndexedMutRandom;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::Mission;
use crate::core::player::Player;
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Description, Unit};

#[derive(EnumIter, Debug, PartialEq)]
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
                "The percentage probability to fire again this round when targeting specific units."
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

    /// Owner of the planet after mission resolution
    pub destination_owned: Option<ClientId>,

    /// Combat logs (if combat took place)
    pub logs: Option<String>,
}

impl MissionReport {
    pub fn winner(&self) -> Option<ClientId> {
        match self.mission.objective {
            Icon::Spy if self.scout_probes > 0 => None,
            _ => {
                if self.surviving_attacker.iter().any(|(u, c)| {
                    if *u == Unit::probe() {
                        *c > self.scout_probes
                    } else {
                        *c > 0
                    }
                }) {
                    Some(self.mission.owner)
                } else {
                    self.planet.controlled
                }
            },
        }
    }

    pub fn image(&self, player: &Player) -> &str {
        match self.mission.objective {
            Icon::MissileStrike => "won",
            Icon::Spy if self.scout_probes > 0 => "eye",
            _ if self.winner() == Some(player.id) => "won",
            _ => "lost",
        }
    }

    pub fn can_see(&self, side: &Side, player_id: ClientId) -> bool {
        match side {
            Side::Attacker => {
                self.mission.owner == player_id
                    || self.planet.owned == Some(player_id)
                    || self.winner() == Some(player_id)
            },
            Side::Defender => {
                self.planet.controlled() == Some(player_id) || self.winner() == Some(player_id)
            },
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
            planet: destination.clone(),
            scout_probes: 0,
            surviving_attacker: mission.army.clone(),
            surviving_defender: destination.army.clone(),
            planet_colonized: false,
            planet_destroyed: false,
            destination_owned: destination.owned,
            logs: None,
        };
    }

    // Bring missiles down with antiballistic
    let mut n_missiles = mission.army.amount(&Unit::interplanetary_missile());
    let mut n_antiballistic =
        destination.army.amount(&Unit::Defense(Defense::AntiballisticMissile));

    let mut missiles_fired = 0;
    let mut missiles_hit = 0;
    while n_antiballistic > 0 && n_missiles > 0 {
        n_antiballistic -= 1;
        missiles_fired += 1;
        if rng().random::<f32>() < 0.5 {
            missiles_hit += 1;
            n_missiles -= 1;
        }
    }

    let colony_ships = mission.army.amount(&Unit::colony_ship());
    let mut planetary_shield = destination.army.amount(&Unit::Building(Building::PlanetaryShield));

    let mut attack_army: Vec<CombatUnit> = mission
        .army
        .iter()
        .filter(|(u, _)| **u != Unit::colony_ship())
        .flat_map(|(unit, count)| {
            std::iter::repeat(CombatUnit::new(unit)).take(
                if *unit != Unit::interplanetary_missile() {
                    *count
                } else {
                    *count - missiles_hit
                },
            )
        })
        .collect();

    let mut defend_army: Vec<CombatUnit> = destination
        .army
        .iter()
        .filter(|(u, _)| {
            !u.is_building()
                && **u != Unit::colony_ship()
                && !matches!(u, Unit::Defense(d) if d.is_missile())
        })
        .flat_map(|(unit, count)| std::iter::repeat(CombatUnit::new(unit)).take(*count))
        .collect();

    let mut logs = String::new();

    let mut round = 1;
    let mut returning_probes = 0;
    let mut planet_destroyed = false;
    while (!attack_army.is_empty() && !defend_army.is_empty()) || round == 1 {
        if attack_army.is_empty() && defend_army.is_empty() {
            // If there are no combat units, skip the battle
            break;
        }

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
                "\n- {missiles_fired} Antiballistic Missiles intercepted {missiles_hit} incoming Interplanetary Missiles."
            ));
        }

        for side in Side::iter() {
            let mut destroyed = Army::new();
            let (army, enemy_army) = if side == Side::Attacker {
                (&mut attack_army, &mut defend_army)
            } else {
                (&mut defend_army, &mut attack_army)
            };

            if !army.is_empty() && !enemy_army.is_empty() {
                logs.push_str(format!("\n- {side:?} shoots:").as_str());
            }

            // Reset all shields
            enemy_army.iter_mut().for_each(|u| u.shield = u.unit.shield());

            for unit in army {
                'shoot: loop {
                    let target = if let Some(target) = enemy_army.choose_mut(&mut rng()) {
                        if target.unit.is_defense() && planetary_shield > 0 {
                            planetary_shield =
                                planetary_shield.saturating_sub(target.unit.damage());

                            if planetary_shield == 0 {
                                logs.push_str("\n >> Planetary Shield destroyed.");
                            }

                            break 'shoot;
                        }

                        target
                    } else {
                        break 'shoot; // No targets left
                    };

                    // Target could already been destroyed by another shot
                    if target.hull > 0 {
                        let mut damage = unit.unit.damage();

                        if target.shield > 0 {
                            damage = damage.saturating_sub(target.shield);
                            target.shield = target.shield.saturating_sub(damage);
                        }

                        if damage > 0 {
                            target.hull = target.hull.saturating_sub(damage);

                            if target.hull == 0 {
                                *destroyed.entry(target.unit).or_insert(0) += 1;
                            }
                        }
                    }

                    if *unit.unit.rapid_fire().get(&target.unit).unwrap_or(&101) as f32 / 100.
                        > rng().random::<f32>()
                    {
                        break 'shoot;
                    }
                }
            }

            for (u, c) in destroyed {
                logs.push_str(format!("\n >> {c} enemy {}s destroyed.", u.to_name()).as_str());
            }
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
                    logs.push_str("\n- Planet destroyed.");
                }
            }
        }

        round += 1;
    }

    // Calculate the surviving units
    let mut surviving_attacker = attack_army.iter().fold(Army::new(), |mut army, cu| {
        *army.entry(cu.unit).or_insert(0) += 1;
        army
    });

    let mut surviving_defense = defend_army.iter().fold(Army::new(), |mut army, cu| {
        *army.entry(cu.unit).or_insert(0) += 1;
        army
    });

    if !attack_army.is_empty() || surviving_defense.is_empty() {
        // Add the non-combat ships to the attacker
        *surviving_attacker.entry(Unit::colony_ship()).or_insert(0) = colony_ships;
    } else {
        if colony_ships > 0 {
            logs.push_str(&format!("\n- {colony_ships} attacking colony ships destroyed."));
        }

        // Add non-combat ships and the remaining missiles to the defender
        *surviving_defense.entry(Unit::colony_ship()).or_insert(0) =
            destination.army.amount(&Unit::colony_ship());
        *surviving_defense.entry(Unit::Defense(Defense::AntiballisticMissile)).or_insert(0) =
            n_antiballistic;
        *surviving_defense.entry(Unit::interplanetary_missile()).or_insert(0) =
            destination.army.amount(&Unit::interplanetary_missile());
    }

    // Add the scout probes to the surviving attacker
    *surviving_attacker.entry(Unit::probe()).or_insert(0) += returning_probes;

    // Add the buildings to the surviving defense
    if !planet_destroyed {
        surviving_defense = surviving_defense
            .iter()
            .chain(destination.army.iter().filter(|(u, _)| u.is_building()))
            .map(|(u, &v)| (u.clone(), v))
            .collect();
    }

    MissionReport {
        turn,
        mission: mission.clone(),
        planet: destination.clone(),
        scout_probes: returning_probes,
        surviving_attacker,
        surviving_defender: surviving_defense,
        planet_colonized: defend_army.is_empty() && mission.objective == Icon::Colonize,
        planet_destroyed,
        destination_owned: None, // Filled in turns.rs after changes have been made to the planet
        logs: Some(logs),
    }
}
