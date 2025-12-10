use bevy::prelude::*;
use rand::prelude::IteratorRandom;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::core::combat::report::{CombatReport, MissionReport, RoundReport, Side};
use crate::core::constants::{CRAWLER_HEALING_PER_ROUND, PS_SHIELD_PER_LEVEL};
use crate::core::map::icon::Icon;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission};
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Unit};

#[derive(Component, Clone, Default, Serialize, Deserialize)]
pub struct ShotReport {
    pub unit: Option<Unit>,
    pub shield_damage: usize,
    pub hull_damage: usize,
    pub missed: bool,
    pub killed: bool,
    pub planetary_shield_damage: usize,
    pub rapid_fire: bool,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CombatUnit {
    pub id: u64,
    pub unit: Unit,
    pub hull: usize,
    pub shield: usize,
    pub repairs: Vec<usize>,
    pub shots: Vec<ShotReport>,
}

impl CombatUnit {
    pub fn new(unit: &Unit) -> Self {
        Self {
            id: rand::random(),
            unit: unit.clone(),
            hull: unit.hull(),
            shield: unit.shield(),
            repairs: vec![],
            shots: vec![],
        }
    }
}

pub fn resolve_combat(turn: usize, mission: &Mission, destination: &Planet) -> MissionReport {
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
            hidden: mission.origin_controlled != Some(mission.owner), // Hide returning probes or fleets
        };
    }

    let mut combat_report = CombatReport::default();

    let mut buildings: Army =
        destination.army.iter().filter_map(|(u, c)| u.is_building().then_some((*u, *c))).collect();
    let mut planetary_shield =
        destination.army.amount(&Unit::planetary_shield()) * PS_SHIELD_PER_LEVEL;

    let mut attack_army: Vec<CombatUnit> = mission
        .army
        .iter()
        .filter(|(u, _)| **u != Unit::colony_ship())
        .flat_map(|(unit, count)| (0..*count).map(|_| CombatUnit::new(unit)))
        .collect();

    let mut defend_army: Vec<CombatUnit> = destination
        .army
        .iter()
        .filter(|(u, _)| {
            !u.is_building()
                && **u != Unit::colony_ship()
                && if mission.objective == Icon::MissileStrike {
                    **u != Unit::interplanetary_missile()
                } else {
                    !u.is_missile()
                }
        })
        .flat_map(|(unit, count)| (0..*count).map(|_| CombatUnit::new(unit)))
        .collect();

    // Sort armies by firing order
    let firing_order = Unit::all_firing_order();
    let rank: Army = firing_order.iter().enumerate().map(|(i, u)| (*u, i)).collect();
    attack_army.sort_by_key(|cu| rank.get(&cu.unit).copied().unwrap_or(usize::MAX));
    defend_army.sort_by_key(|cu| rank.get(&cu.unit).copied().unwrap_or(usize::MAX));

    let mut rng = rng();

    let mut round = 1;
    let mut returning_probes = 0;
    let mut used_antiballistic = vec![];
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

            // Reset all repairs, shots and defender's shields
            army.iter_mut().for_each(|u| {
                u.repairs = vec![];
                u.shots = vec![];
            });
            enemy_army.iter_mut().for_each(|u| u.shield = u.unit.shield());

            'unit: for unit in army {
                // Intercept incoming missiles before resolving damage
                if unit.unit == Unit::interplanetary_missile() {
                    for cu in enemy_army.iter_mut() {
                        if cu.unit == Unit::antiballistic_missile()
                            && !used_antiballistic.contains(&cu.id)
                        {
                            used_antiballistic.push(cu.id);

                            let mut shot = ShotReport::default();
                            shot.unit = Some(Unit::interplanetary_missile());

                            if rng.random::<f32>() < 0.5 {
                                shot.killed = true;
                                cu.shots.push(shot);
                                continue 'unit;
                            } else {
                                shot.missed = true;
                            }

                            cu.shots.push(shot);
                        }
                    }
                }

                let mut damage = unit.unit.damage();

                if damage == 0 {
                    // Skip the shooting (for probes or antiballistic missiles)
                    continue 'unit;
                }

                'shoot: loop {
                    let mut shot = ShotReport::default();

                    let target = if unit.unit == Unit::interplanetary_missile() {
                        // Interplanetary Missiles only shoot on defenses
                        enemy_army
                            .iter_mut()
                            .filter(|u| {
                                u.unit.is_defense()
                                    && !u.unit.is_missile()
                                    && u.unit != Unit::space_dock()
                            })
                            .choose(&mut rng)
                    } else if unit.unit == Unit::Ship(Ship::Bomber)
                        && planetary_shield > 0
                        && side == Side::Attacker
                        && mission.bombing != BombingRaid::None
                    {
                        // Bombers always target the planetary shield first when bombing
                        shot.planetary_shield_damage = damage.min(planetary_shield);
                        planetary_shield -= shot.planetary_shield_damage;
                        shot.unit = Some(Unit::planetary_shield());
                        None
                    } else if let Some(target) =
                        enemy_army.iter_mut().filter(|cu| !cu.unit.is_missile()).choose(&mut rng)
                    {
                        // If shooting on a defense, shoot on the planetary shield instead
                        if target.unit.is_defense()
                            && target.unit != Unit::space_dock()
                            && planetary_shield > 0
                        {
                            shot.planetary_shield_damage = damage.min(planetary_shield);
                            planetary_shield -= shot.planetary_shield_damage;
                            shot.unit = Some(Unit::planetary_shield());
                            None
                        } else {
                            Some(target)
                        }
                    } else {
                        None
                    };

                    let target = if let Some(target) = target {
                        shot.unit = Some(target.unit.clone());
                        target
                    } else {
                        if shot.unit.is_some() {
                            unit.shots.push(shot);
                        }
                        break 'shoot; // No unit to target
                    };

                    // Target could already been destroyed by another shot
                    if target.hull > 0 {
                        if target.shield > 0 {
                            shot.shield_damage = damage.min(target.shield);
                            damage -= shot.shield_damage;
                            target.shield -= shot.shield_damage;
                        }

                        if damage > 0 {
                            shot.hull_damage = damage.min(target.hull);
                            target.hull -= shot.hull_damage;

                            if target.hull == 0 {
                                shot.killed = true;
                            }
                        }
                    } else {
                        shot.missed = true;
                    }

                    if *unit.unit.rapid_fire().get(&target.unit).unwrap_or(&101) as f32 / 100.
                        > rng.random::<f32>()
                    {
                        unit.shots.push(shot);
                        break 'shoot;
                    }

                    shot.rapid_fire = true;
                    unit.shots.push(shot);
                }
            }
        }

        // Repair defense turrets
        let n_crawlers =
            defend_army.iter().filter(|u| u.unit == Unit::crawler() && u.hull > 0).count();

        for _ in 0..n_crawlers {
            let pool = defend_army
                .iter_mut()
                .filter(|u| u.unit.is_turret() && u.hull > 0 && u.hull < u.unit.hull());

            if let Some(target) = pool.choose(&mut rng) {
                let heal = (target.unit.hull() - target.hull).min(CRAWLER_HEALING_PER_ROUND);
                target.repairs.push(heal);
                target.hull += heal;
            }
        }

        // Resolve bombing raids
        if mission.bombing != BombingRaid::None && planetary_shield == 0 {
            for cu in
                attack_army.iter_mut().filter(|u| u.unit == Unit::Ship(Ship::Bomber) && u.hull > 0)
            {
                let f = match mission.bombing {
                    BombingRaid::Economic => {
                        |u: &Unit, c: &&mut usize| u.is_economic_building() && **c > 0
                    },
                    BombingRaid::Industrial => {
                        |u: &Unit, c: &&mut usize| u.is_industrial_building() && **c > 0
                    },
                    _ => unreachable!(),
                };

                if let Some((u, c)) = buildings.iter_mut().filter(|(u, c)| f(u, c)).choose(&mut rng)
                {
                    let mut shot = ShotReport::default();
                    shot.unit = Some(*u);
                    if rng.random::<f32>() < 0.1 {
                        *c -= 1;
                        shot.killed = true;
                    } else {
                        shot.missed = true;
                    }
                    cu.shots.push(shot);
                }
            }
        }

        // Save snapshot of the state of the armies this turn
        let mut round_report = RoundReport {
            attacker: attack_army.clone(),
            defender: defend_army.clone(),
            planetary_shield,
            antiballistic_fired: used_antiballistic.len(),
            buildings: buildings.clone(),
            destroy_probability: 0.,
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

        // Try to destroy planet
        if mission.objective == Icon::Destroy
            && !defend_army.iter().any(|u| u.unit.is_ship() || u.unit == Unit::space_dock())
        {
            let war_suns =
                attack_army.iter().filter(|u| u.unit == Unit::war_sun()).collect::<Vec<_>>();
            let destroy_probability =
                (destination.destroy_probability() - 0.01 * round as f32).max(0.);
            for _ in war_suns.iter() {
                if rng.random::<f32>() < destroy_probability {
                    defend_army = vec![];
                    planet_destroyed = true;
                }
            }

            round_report.destroy_probability =
                1. - (1. - destroy_probability).powi(war_suns.len() as i32);
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
        *surviving_attacker.entry(Unit::colony_ship()).or_insert(0) =
            mission.army.amount(&Unit::colony_ship());
    } else {
        // Add non-combat ships and the remaining missiles to the defender
        *surviving_defense.entry(Unit::colony_ship()).or_insert(0) =
            destination.army.amount(&Unit::colony_ship());
        *surviving_defense.entry(Unit::antiballistic_missile()).or_insert(0) =
            destination.army.amount(&Unit::antiballistic_missile()) - used_antiballistic.len();
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
        combat_report: (combat_report
            .rounds
            .iter()
            .flat_map(|r| r.attacker.iter().chain(r.defender.iter()))
            .any(|cu| !cu.shots.is_empty())
            || mission.objective == Icon::Destroy)
            .then_some(combat_report),
        hidden: false,
    }
}
