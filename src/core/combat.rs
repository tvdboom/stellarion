use bevy_renet::renet::ClientId;
use rand::prelude::{IndexedMutRandom, IteratorRandom};
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;
use strum_macros::EnumIter;

use crate::core::constants::PLANETARY_SHIELD_STRENGTH_PER_LEVEL;
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::player::Player;
use crate::core::units::buildings::Building;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Description, Unit};

pub type ReportId = u64;

#[derive(EnumIter, Clone, Debug, PartialEq)]
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
    /// Unique identifier for the report
    pub id: ReportId,

    /// Turn the report was generated
    pub turn: usize,

    /// Mission that created the report
    pub mission: Mission,

    /// Planet as it was before the mission resolution
    pub planet: Planet,

    /// Number of attacking probes that left after one round of combat
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

    /// Controller of the planet after mission resolution
    pub destination_controlled: Option<ClientId>,

    /// Combat report (if combat took place)
    pub combat_report: Option<CombatReport>,

    /// Whether to show this report in the report mission tab
    pub hidden: bool,
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
            Icon::MissileStrike => "missile",
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
                    || self.mission.objective == Icon::Spy // Spy winner returns None
            },
            Side::Defender => {
                self.planet.controlled == Some(player_id) || self.winner() == Some(player_id)
            },
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct CombatReport {
    pub rounds: Vec<RoundReport>,
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct RoundReport {
    pub attacker: Vec<CombatUnit>,
    pub defender: Vec<CombatUnit>,
    pub planetary_shield: usize,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CombatUnit {
    pub id: u64,
    pub unit: Unit,
    pub hull: usize,
    pub shield: usize,
    pub shots: Vec<ShotReport>,
}

impl CombatUnit {
    pub fn new(unit: &Unit) -> Self {
        Self {
            id: rand::random(),
            unit: unit.clone(),
            hull: unit.hull(),
            shield: unit.shield(),
            shots: vec![],
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct ShotReport {
    pub unit: Option<Unit>,
    pub shield_damage: usize,
    pub hull_damage: usize,
    pub killed: bool,
    pub planetary_shield_damage: usize,
    pub rapid_fire: bool,
}

pub fn combat(turn: usize, mission: &Mission, destination: &Planet) -> MissionReport {
    if mission.objective == Icon::Deploy
        || (mission.objective == Icon::Colonize && destination.controlled == Some(mission.owner))
    {
        return MissionReport {
            id: rand::random(),
            turn,
            mission: mission.clone(),
            planet: destination.clone(),
            scout_probes: 0,
            surviving_attacker: mission.army.clone(),
            surviving_defender: destination.army.clone(),
            planet_colonized: mission.objective == Icon::Colonize,
            planet_destroyed: false,
            destination_owned: destination.owned,
            destination_controlled: destination.controlled,
            combat_report: None,
            hidden: false,
        };
    }

    let mut combat_report = CombatReport::default();

    // Bring missiles down with antiballistic
    let mut n_missiles = mission.army.amount(&Unit::interplanetary_missile());
    let mut n_antiballistic =
        destination.army.amount(&Unit::Defense(Defense::AntiballisticMissile));

    let mut missiles_hit = 0;
    while n_antiballistic > 0 && n_missiles > 0 {
        n_antiballistic -= 1;
        if rng().random::<f32>() < 0.5 {
            missiles_hit += 1;
            n_missiles -= 1;
        }
    }

    let mut buildings: Army =
        destination.army.iter().filter_map(|(u, c)| u.is_building().then_some((*u, *c))).collect();
    let colony_ships = mission.army.amount(&Unit::colony_ship());
    let mut planetary_shield = destination.army.amount(&Unit::Building(Building::PlanetaryShield))
        * PLANETARY_SHIELD_STRENGTH_PER_LEVEL;

    let mut attack_army: Vec<CombatUnit> = mission
        .army
        .iter()
        .filter(|(u, _)| **u != Unit::colony_ship())
        .flat_map(|(unit, count)| {
            let n = if *unit != Unit::interplanetary_missile() {
                *count
            } else {
                *count - missiles_hit
            };

            (0..n).map(|_| CombatUnit::new(unit))
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
        .flat_map(|(unit, count)| (0..*count).map(|_| CombatUnit::new(unit)))
        .collect();

    let mut round = 1;
    let mut returning_probes = 0;
    let mut planet_destroyed = false;
    while (!attack_army.is_empty() && !defend_army.is_empty()) || round == 1 {
        if attack_army.is_empty() && defend_army.is_empty() {
            // If there are no combat units, skip the battle
            break;
        }

        for side in Side::iter() {
            if mission.objective == Icon::MissileStrike && side == Side::Defender {
                continue;
            }

            let (army, enemy_army) = match side {
                Side::Attacker => (&mut attack_army, &mut defend_army),
                Side::Defender => (&mut defend_army, &mut attack_army),
            };

            // Reset all attacker's shots and defender's shields
            army.iter_mut().for_each(|u| u.shots = vec![]);
            enemy_army.iter_mut().for_each(|u| u.shield = u.unit.shield());

            for unit in army {
                let mut damage = unit.unit.damage();

                if damage == 0 {
                    // Skip the shooting (for probes for example)
                    continue;
                }

                'shoot: loop {
                    let mut shot_report = ShotReport::default();

                    let target = if matches!(unit.unit, Unit::Defense(d) if d.is_missile()) {
                        // Interplanetary Missiles only shoot on defenses
                        enemy_army.iter_mut().filter(|u| u.unit.is_defense()).choose(&mut rng())
                    } else if unit.unit == Unit::Ship(Ship::Bomber) && planetary_shield > 0 {
                        // Bombers always target the planetary shield first
                        shot_report.planetary_shield_damage = damage.min(planetary_shield);
                        planetary_shield -= shot_report.planetary_shield_damage;
                        None
                    } else if let Some(target) = enemy_army.choose_mut(&mut rng()) {
                        // If shooting on a defense, shoot on the planetary shield instead
                        if target.unit.is_defense() && planetary_shield > 0 {
                            shot_report.planetary_shield_damage = damage.min(planetary_shield);
                            planetary_shield -= shot_report.planetary_shield_damage;
                            None
                        } else {
                            Some(target)
                        }
                    } else {
                        None
                    };

                    let target = if let Some(target) = target {
                        shot_report.unit = Some(target.unit.clone());
                        target
                    } else {
                        unit.shots.push(shot_report);
                        break 'shoot; // No unit to target
                    };

                    // Target could already been destroyed by another shot
                    if target.hull > 0 {
                        if target.shield > 0 {
                            shot_report.shield_damage = damage.min(target.shield);
                            damage -= shot_report.shield_damage;
                            target.shield -= shot_report.shield_damage;
                        }

                        if damage > 0 {
                            shot_report.hull_damage = damage.min(target.hull);
                            target.hull -= shot_report.hull_damage;

                            if target.hull == 0 {
                                shot_report.killed = true;
                            }
                        }
                    }

                    if *unit.unit.rapid_fire().get(&target.unit).unwrap_or(&101) as f32 / 100.
                        > rng().random::<f32>()
                    {
                        unit.shots.push(shot_report);
                        break 'shoot;
                    }

                    shot_report.rapid_fire = true;
                    unit.shots.push(shot_report);
                }
            }
        }

        // Save snapshot of the state of the armies this turn
        let round_report = RoundReport {
            attacker: attack_army.clone(),
            defender: defend_army.clone(),
            planetary_shield,
        };

        // Remove units that are destroyed after both armies have fired
        attack_army.retain(|u| u.hull > 0);
        defend_army.retain(|u| u.hull > 0);

        if round == 1 {
            // Send probes back if there are still remaining enemies or objective is spying
            let probes = attack_army.iter().filter(|u| u.unit == Unit::probe()).count();
            if ((!mission.combat_probes && !defend_army.is_empty())
                || mission.objective == Icon::Spy)
                && probes > 0
            {
                attack_army.retain(|u| u.unit != Unit::probe());
                returning_probes = probes;
            }
        }

        // Resolve bombing raids
        if mission.bombing != BombingRaid::None && planetary_shield == 0 {
            for _ in attack_army.iter().filter(|u| u.unit == Unit::Ship(Ship::Bomber)) {
                let mut rng = rng();
                if rng.random::<f32>() < 0.1 {
                    let f = match mission.bombing {
                        BombingRaid::Economic => {
                            |u: &Unit, c: &&mut usize| u.is_resource_building() && **c > 0
                        },
                        BombingRaid::Industrial => {
                            |u: &Unit, c: &&mut usize| u.is_industrial_building() && **c > 0
                        },
                        _ => unreachable!(),
                    };

                    if let Some((_, c)) =
                        buildings.iter_mut().filter(|(u, c)| f(u, c)).choose(&mut rng)
                    {
                        *c -= 1;
                    }
                }
            }
        }

        // Try to destroy planet
        if mission.objective == Icon::Destroy && !defend_army.iter().any(|u| u.unit.is_ship()) {
            for _ in attack_army.iter().filter(|u| u.unit == Unit::Ship(Ship::WarSun)) {
                if rng().random::<f32>() < destination.destroy_probability() - 0.01 * round as f32 {
                    defend_army = vec![];
                    planet_destroyed = true;
                }
            }
        }

        combat_report.rounds.push(round_report);
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
        surviving_defense =
            surviving_defense.iter().chain(buildings.iter()).map(|(u, v)| (*u, *v)).collect();
    }

    MissionReport {
        id: rand::random(),
        turn,
        mission: mission.clone(),
        planet: destination.clone(),
        scout_probes: returning_probes,
        surviving_attacker,
        surviving_defender: surviving_defense,
        planet_colonized: defend_army.is_empty() && mission.objective == Icon::Colonize,
        planet_destroyed,
        destination_owned: None, // Filled in turns.rs after changes have been made to the planet
        destination_controlled: None, // Filled in turns.rs as well
        combat_report: (!combat_report.rounds.is_empty()).then_some(combat_report),
        hidden: false,
    }
}
