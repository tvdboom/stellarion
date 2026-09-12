//! Deterministic fleet-combat resolution independent of rendering and networking.

use std::collections::{BTreeMap, HashMap};
use std::sync::LazyLock;

use bevy::prelude::*;
use rand::prelude::IteratorRandom;
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::core::combat::report::{
    CombatReport, DefenderRetreat, MissionReport, RoundReport, Side,
};
use crate::core::constants::REPAIR_TRUCK_HEALING_PER_ROUND;
use crate::core::energy::EnergyGrid;
use crate::core::map::icon::Icon;
use crate::core::map::planet::{Garrison, Planet};
use crate::core::missions::{BombingRaid, Mission};
use crate::core::units::ships::Ship;
use crate::core::units::{Amount, Army, Combat, Unit};

/// Hard ceiling that turns shield stalemates into deterministic draws.
pub const MAX_COMBAT_ROUNDS: usize = 100;

/// Hard ceiling for one unit's probabilistic rapid-fire chain in one round.
pub const MAX_SHOTS_PER_UNIT_PER_ROUND: usize = 256;

/// Chance that one surviving Bomber destroys a building level during its single raid.
pub const BOMBING_HIT_CHANCE: f32 = 0.25;

/// Per-building raid limit; the three buildings in either category allow nine levels in total.
pub const MAX_BOMBING_LEVELS_PER_BUILDING: usize = 3;

/// Unit statistics are immutable; build the rapid-fire tables once instead of once per shot.
static RAPID_FIRE: LazyLock<HashMap<Unit, HashMap<Unit, usize>>> = LazyLock::new(|| {
    Unit::all().into_iter().flatten().map(|unit| (unit, unit.rapid_fire())).collect()
});

#[derive(Component, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Outcome of one combatant firing once at a selected target.
pub struct ShotReport {
    /// Exact combatant selected by this shot, when it targeted an ordinary unit.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub target_id: Option<u64>,
    /// Unit kind represented by this record or presentation component.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub unit: Option<Unit>,
    /// Damage absorbed by the target's ordinary shield.
    pub shield_damage: usize,
    /// Damage applied to the target's hull.
    pub hull_damage: usize,
    /// Whether the shot failed to hit or penetrate its target.
    pub missed: bool,
    /// Whether this shot destroyed the selected target.
    pub killed: bool,
    /// Damage absorbed by the shared planetary shield.
    pub planetary_shield_damage: usize,
    /// Whether the shooter earned another rapid-fire shot.
    pub rapid_fire: bool,
}

impl ShotReport {
    /// Whether this shot belongs to the building raid rather than ordinary weapon fire.
    pub fn is_bombing(&self) -> bool {
        self.unit.is_some_and(|unit| unit.is_building() && unit != Unit::planetary_shield())
    }
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Per-unit combat state retained for deterministic reports and animation playback.
pub struct CombatUnit {
    /// Stable identifier used to cross-reference this value.
    pub id: u64,
    /// Player whose independently persisted fleet supplied this unit.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub owner: Option<crate::core::identity::PlayerId>,
    /// Unit kind represented by this record or presentation component.
    pub unit: Unit,
    /// Hull points remaining at this stage of combat.
    pub hull: usize,
    /// Shield points remaining at this stage of combat.
    pub shield: usize,
    /// Repair amounts applied after each corresponding round.
    pub repairs: Vec<usize>,
    /// Ordered shot outcomes produced by this unit.
    pub shots: Vec<ShotReport>,
}

impl CombatUnit {
    /// Creates a combat unit from the supplied deterministic stream.
    pub fn new_with_rng<R: Rng + ?Sized>(unit: &Unit, rng: &mut R) -> Self {
        Self::new_owned_with_rng(unit, None, rng)
    }

    /// Creates a combat unit while retaining the contributing player's identity.
    pub fn new_owned_with_rng<R: Rng + ?Sized>(
        unit: &Unit,
        owner: Option<crate::core::identity::PlayerId>,
        rng: &mut R,
    ) -> Self {
        Self {
            id: rng.random(),
            owner,
            unit: *unit,
            hull: unit.hull(),
            shield: unit.shield(),
            repairs: vec![],
            shots: vec![],
        }
    }
}

/// Resolves combat with fully powered defenses and the supplied deterministic random stream.
pub fn resolve_combat_with_rng<R: Rng + ?Sized>(
    turn: usize,
    mission: &Mission,
    destination: &Planet,
    rng: &mut R,
) -> MissionReport {
    resolve_combat_with_energy_with_rng(
        turn,
        mission,
        destination,
        EnergyGrid {
            supply: 1,
            demand: 1,
        },
        rng,
    )
}

/// Resolves combat with the defender's turn-start energy grid determining shield power.
pub fn resolve_combat_with_energy_with_rng<R: Rng + ?Sized>(
    turn: usize,
    mission: &Mission,
    destination: &Planet,
    energy: EnergyGrid,
    rng: &mut R,
) -> MissionReport {
    resolve_combat_with_retreat_with_rng(turn, mission, destination, energy, None, rng)
}

/// Resolves a battle with an optional, currently owned homeworld for colonial withdrawal.
/// The simulation supplies this destination only for an eligible non-home defending world.
pub fn resolve_combat_with_retreat_with_rng<R: Rng + ?Sized>(
    turn: usize,
    mission: &Mission,
    destination: &Planet,
    energy: EnergyGrid,
    retreat_home: Option<crate::core::map::planet::PlanetId>,
    rng: &mut R,
) -> MissionReport {
    if matches!(mission.objective, Icon::Deploy | Icon::Protect)
        || (mission.objective == Icon::Colonize && destination.controlled == Some(mission.owner))
    {
        return MissionReport {
            id: rng.random(),
            turn,
            mission: mission.clone(),
            planet: destination.clone(),
            scout_probes: 0,
            surviving_attacker: mission.army.clone(),
            surviving_defender: destination.army.clone(),
            planet_colonized: mission.objective == Icon::Colonize
                && destination.owned != Some(mission.owner),
            planet_destroyed: false,
            destination_owned: destination.owned,
            destination_controlled: destination.controlled,
            combat_report: None,
            hidden: mission.origin_controlled != Some(mission.owner), // Hide returning probes or fleets
        };
    }

    let mut combat_report = CombatReport::default();
    let administration = destination
        .army
        .amount(&Unit::Building(crate::core::units::buildings::Building::ColonialAdministration));
    let retreat_home = retreat_home.filter(|home| {
        *home != destination.id
            && !destination.is_moon()
            && destination.controlled.is_some()
            && matches!(mission.objective, Icon::Attack | Icon::Colonize | Icon::Destroy)
            && administration >= destination.fleet_withdrawal.minimum_level()
    });
    let threshold = retreat_home.and(destination.fleet_withdrawal.losses_percent());
    let initial_fleet_strength = destination
        .army
        .iter()
        .filter(|(unit, _)| unit.is_ship())
        .map(|(unit, count)| unit.production() as u128 * *count as u128)
        .sum::<u128>();
    let defender_owner = destination.controlled.or(destination.owned);
    let mut support_colonies = BTreeMap::<crate::core::identity::PlayerId, usize>::new();
    if let Some(owner) = defender_owner {
        support_colonies.insert(owner, destination.army.controller().amount(&Unit::colony_ship()));
    }
    for (owner, fleet) in destination.army.protectors() {
        support_colonies.insert(owner, fleet.amount(&Unit::colony_ship()));
    }

    let mut buildings: Army =
        destination.army.iter().filter_map(|(u, c)| u.is_building().then_some((*u, *c))).collect();
    let mut planetary_shield = energy.planetary_shield(
        destination.army.amount(&Unit::planetary_shield()),
        destination.shield_overload.is_overloaded(),
    );

    let joint_attackers = mission
        .joint_attack
        .as_ref()
        .filter(|attack| !attack.attackers.is_empty())
        .map(|attack| &attack.attackers);
    let mut attack_army = Vec::new();
    if let Some(attackers) = joint_attackers {
        for (owner, fleet) in attackers {
            for (unit, count) in fleet.iter().filter(|(unit, _)| **unit != Unit::colony_ship()) {
                for _ in 0..*count {
                    attack_army.push(CombatUnit::new_owned_with_rng(unit, Some(*owner), rng));
                }
            }
        }
    } else {
        for (unit, count) in mission.army.iter().filter(|(unit, _)| **unit != Unit::colony_ship()) {
            for _ in 0..*count {
                attack_army.push(CombatUnit::new_owned_with_rng(unit, Some(mission.owner), rng));
            }
        }
    }

    let mut defend_army = Vec::new();
    for (unit, count) in destination.army.iter().filter(|(unit, _)| {
        !unit.is_building()
            && **unit != Unit::colony_ship()
            && if mission.objective == Icon::MissileStrike {
                **unit != Unit::interplanetary_missile()
            } else {
                !unit.is_missile()
            }
    }) {
        for _ in 0..*count {
            defend_army.push(CombatUnit::new_owned_with_rng(unit, defender_owner, rng));
        }
    }

    for (owner, fleet) in destination.army.protectors() {
        for (unit, count) in fleet.iter().filter(|(unit, _)| {
            **unit != Unit::colony_ship()
                && if mission.objective == Icon::MissileStrike {
                    **unit != Unit::interplanetary_missile()
                } else {
                    !unit.is_missile()
                }
        }) {
            for _ in 0..*count {
                defend_army.push(CombatUnit::new_owned_with_rng(unit, Some(owner), rng));
            }
        }
    }

    // Sort armies by firing order
    let firing_order = Unit::all_firing_order();
    let rank: Army = firing_order.iter().enumerate().map(|(i, u)| (*u, i)).collect();
    attack_army.sort_by_key(|cu| rank.get(&cu.unit).copied().unwrap_or(usize::MAX));
    defend_army.sort_by_key(|cu| rank.get(&cu.unit).copied().unwrap_or(usize::MAX));

    let mut round = 1;
    let mut returning_probes = 0;
    let mut returning_probes_by_owner = BTreeMap::<crate::core::identity::PlayerId, usize>::new();
    let mut used_antiballistic = vec![];
    let mut planet_destroyed = false;
    let mut bombing_resolved = false;
    let mut withdrawal_ordered = threshold == Some(0) && initial_fleet_strength > 0;
    let mut withdrawal_cover = withdrawal_ordered && administration < 5;
    if withdrawal_ordered && administration >= 5 {
        record_defender_retreat(
            &mut combat_report,
            &mut defend_army,
            &mut support_colonies,
            defender_owner,
            retreat_home,
            None,
        );
    }
    while ((!attack_army.is_empty() && !defend_army.is_empty()) || round == 1 || withdrawal_cover)
        && (round <= MAX_COMBAT_ROUNDS || withdrawal_cover)
    {
        if attack_army.is_empty()
            && defend_army.is_empty()
            && combat_report.defender_retreat.is_none()
            && !withdrawal_cover
        {
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
                u.repairs.clear();
                u.shots.clear();
            });
            enemy_army.iter_mut().for_each(|u| u.shield = u.unit.shield());

            'unit: for unit in army {
                if withdrawal_cover && side == Side::Defender && unit.unit.is_ship() {
                    // Ships committed to withdrawal cannot return fire during the final volley.
                    continue;
                }
                // Intercept incoming missiles before resolving damage
                if unit.unit == Unit::interplanetary_missile()
                    && intercept_incoming_missile(enemy_army, &mut used_antiballistic, || {
                        rng.random::<f32>()
                    })
                {
                    continue 'unit;
                }

                if unit.unit.damage() == 0 {
                    // Skip the shooting (for probes or antiballistic missiles)
                    continue 'unit;
                }

                let mut shots_fired = 0;
                'shoot: loop {
                    let mut damage = unit.unit.damage();
                    shots_fired += 1;
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
                            .choose(&mut *rng)
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
                        choose_combat_target(unit.unit, enemy_army, &mut *rng)
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
                        shot.unit = Some(target.unit);
                        shot.target_id = Some(target.id);
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

                    if rapid_fire_stops(&unit.unit, &target.unit, shots_fired, rng.random::<f32>())
                    {
                        unit.shots.push(shot);
                        break 'shoot;
                    }

                    shot.rapid_fire = true;
                    unit.shots.push(shot);
                }
            }
        }

        // Repair Trucks restore damaged defense turrets after both sides have fired.
        let n_repair_trucks = defend_army
            .iter()
            .filter(|unit| unit.unit == Unit::repair_truck() && unit.hull > 0)
            .count();

        for _ in 0..n_repair_trucks {
            let pool = defend_army
                .iter_mut()
                .filter(|u| u.unit.is_turret() && u.hull > 0 && u.hull < u.unit.hull());

            if let Some(target) = pool.choose(&mut *rng) {
                let heal = (target.unit.hull() - target.hull).min(REPAIR_TRUCK_HEALING_PER_ROUND);
                target.repairs.push(heal);
                target.hull += heal;
            }
        }

        // One raid at the end of the first unshielded round. Surviving longer must not
        // multiply building damage; Bombers destroyed during this round cannot take part.
        if !bombing_resolved && mission.bombing != BombingRaid::None && planetary_shield == 0 {
            bombing_resolved = true;
            resolve_bombing_raid(&mission.bombing, &mut attack_army, &mut buildings, rng);
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

        if withdrawal_cover {
            record_defender_retreat(
                &mut combat_report,
                &mut defend_army,
                &mut support_colonies,
                defender_owner,
                retreat_home,
                Some(round - 1),
            );
            withdrawal_cover = false;
        } else if !withdrawal_ordered && !attack_army.is_empty() {
            let remaining = defend_army
                .iter()
                .filter(|unit| unit.unit.is_ship() && unit.owner == defender_owner)
                .map(|unit| unit.unit.production() as u128)
                .sum::<u128>()
                + defender_owner
                    .and_then(|owner| support_colonies.get(&owner).copied())
                    .unwrap_or_default() as u128
                    * Unit::colony_ship().production() as u128;
            if threshold.is_some_and(|percent| {
                remaining > 0
                    && initial_fleet_strength > 0
                    && initial_fleet_strength.saturating_sub(remaining) * 100
                        >= initial_fleet_strength * percent as u128
            }) {
                withdrawal_ordered = true;
                if administration >= 5 {
                    record_defender_retreat(
                        &mut combat_report,
                        &mut defend_army,
                        &mut support_colonies,
                        defender_owner,
                        retreat_home,
                        Some(round - 1),
                    );
                } else {
                    withdrawal_cover = true;
                }
            }
        }

        if round == 1 {
            // Send probes back if there are still remaining enemies or objective is spying
            let probes = attack_army.iter().filter(|u| u.unit == Unit::probe()).count();
            if ((!mission.combat_probes && !defend_army.is_empty())
                || mission.objective == Icon::Spy)
                && probes > 0
            {
                for probe in attack_army.iter().filter(|unit| unit.unit == Unit::probe()) {
                    if let Some(owner) = probe.owner {
                        let count = returning_probes_by_owner.entry(owner).or_default();
                        *count = count.saturating_add(1);
                    }
                }
                attack_army.retain(|u| u.unit != Unit::probe());
                returning_probes = probes;
            }
        }

        // The fleet must break through every defending ship and the Space Dock first.
        if mission.objective == Icon::Destroy
            && !defend_army.iter().any(|u| u.unit.is_ship() || u.unit == Unit::space_dock())
        {
            let war_suns = attack_army.iter().filter(|u| u.unit == Unit::war_sun()).count();
            let destroy_probability =
                (destination.destroy_probability() - 0.01 * round as f32).max(0.);
            for _ in 0..war_suns {
                if rng.random::<f32>() < destroy_probability {
                    defend_army.clear();
                    planet_destroyed = true;
                }
            }

            round_report.destroy_probability =
                1. - (1. - destroy_probability).powi(war_suns as i32);
        }

        combat_report.rounds.push(round_report);
        round += 1;
    }

    // Calculate the surviving units
    let mut surviving_attacker = attack_army.iter().fold(Army::new(), |mut army, cu| {
        *army.entry(cu.unit).or_insert(0) += 1;
        army
    });
    let mut surviving_attackers = BTreeMap::<crate::core::identity::PlayerId, Army>::new();
    for combatant in &attack_army {
        if let Some(owner) = combatant.owner {
            let count =
                surviving_attackers.entry(owner).or_default().entry(combatant.unit).or_default();
            *count = count.saturating_add(1);
        }
    }

    let defense_survives = !defend_army.is_empty();
    let mut surviving_controller = Army::new();
    let mut surviving_protectors = BTreeMap::<crate::core::identity::PlayerId, Army>::new();
    for combatant in &defend_army {
        let army = if combatant.owner == defender_owner {
            &mut surviving_controller
        } else if let Some(owner) = combatant.owner {
            surviving_protectors.entry(owner).or_default()
        } else {
            continue;
        };
        let count = army.entry(combatant.unit).or_default();
        *count = count.saturating_add(1);
    }

    if !attack_army.is_empty() || !defense_survives {
        // Add the non-combat ships to the attacker
        *surviving_attacker.entry(Unit::colony_ship()).or_insert(0) =
            mission.army.amount(&Unit::colony_ship());
        if let Some(attackers) = joint_attackers {
            for (owner, army) in attackers {
                let colonies = army.amount(&Unit::colony_ship());
                if colonies > 0 {
                    surviving_attackers
                        .entry(*owner)
                        .or_default()
                        .insert(Unit::colony_ship(), colonies);
                }
            }
        }
    }
    if defense_survives {
        // Defender support units survive a stalemate as well as a defensive victory.
        // Add each commander's non-combat ships and the controller's remaining missiles.
        if let Some(owner) = defender_owner {
            let colonies = support_colonies.get(&owner).copied().unwrap_or_default();
            if colonies > 0 {
                surviving_controller.insert(Unit::colony_ship(), colonies);
            }
        }
        for (owner, count) in &support_colonies {
            if Some(*owner) != defender_owner && *count > 0 {
                surviving_protectors.entry(*owner).or_default().insert(Unit::colony_ship(), *count);
            }
        }
        *surviving_controller.entry(Unit::antiballistic_missile()).or_insert(0) = destination
            .army
            .controller()
            .amount(&Unit::antiballistic_missile())
            .saturating_sub(used_antiballistic.len());
        *surviving_controller.entry(Unit::interplanetary_missile()).or_insert(0) =
            destination.army.controller().amount(&Unit::interplanetary_missile());
    }

    // Add the scout probes to the surviving attacker
    *surviving_attacker.entry(Unit::probe()).or_insert(0) += returning_probes;
    for (owner, probes) in returning_probes_by_owner {
        let count = surviving_attackers.entry(owner).or_default().entry(Unit::probe()).or_default();
        *count = count.saturating_add(probes);
    }

    // Add the buildings to the surviving defense
    if !planet_destroyed {
        surviving_controller.extend(buildings);
    } else {
        surviving_controller.clear();
        surviving_protectors.clear();
    }
    let surviving_defense = Garrison::from_parts(surviving_controller, surviving_protectors);

    let mut resolved_mission = mission.clone();
    if let Some(attack) = &mut resolved_mission.joint_attack {
        attack.survivors = surviving_attackers;
    }
    MissionReport {
        id: rng.random(),
        turn,
        mission: resolved_mission,
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
            || mission.objective == Icon::Destroy
            || combat_report.defender_retreat.is_some())
        .then_some(combat_report),
        hidden: false,
    }
}

/// Removes escaped ships from the garrison before conquest and planet-destruction checks.
fn record_defender_retreat(
    report: &mut CombatReport,
    army: &mut Vec<CombatUnit>,
    colonies: &mut BTreeMap<crate::core::identity::PlayerId, usize>,
    withdrawing_owner: Option<crate::core::identity::PlayerId>,
    home: Option<crate::core::map::planet::PlanetId>,
    after_round: Option<usize>,
) {
    let (Some(home_planet), Some(withdrawing_owner)) = (home, withdrawing_owner) else {
        return;
    };
    let mut ships = Army::new();
    army.retain(|unit| {
        if unit.unit.is_ship() && unit.owner == Some(withdrawing_owner) {
            if unit.hull > 0 {
                *ships.entry(unit.unit).or_default() += 1;
            }
            false
        } else {
            true
        }
    });
    if let Some(count) = colonies.get_mut(&withdrawing_owner) {
        if *count > 0 {
            *ships.entry(Unit::colony_ship()).or_default() += *count;
            *count = 0;
        }
    }
    if ships.has_army() {
        report.defender_retreat = Some(DefenderRetreat {
            after_round,
            home_planet,
            ships,
        });
    }
}

/// Records one bounded raid using the same deterministic stream as fleet combat.
fn resolve_bombing_raid<R: Rng + ?Sized>(
    raid: &BombingRaid,
    attackers: &mut [CombatUnit],
    buildings: &mut Army,
    rng: &mut R,
) {
    let mut losses = Army::new();
    for bomber in
        attackers.iter_mut().filter(|cu| cu.unit == Unit::Ship(Ship::Bomber) && cu.hull > 0)
    {
        // Choose uniformly among eligible building types for every attempt. A depleted
        // or capped building leaves the pool, so hits are never forced onto one type.
        let Some((unit, levels)) = buildings
            .iter_mut()
            .filter(|(unit, count)| {
                **count > 0
                    && losses.amount(unit) < MAX_BOMBING_LEVELS_PER_BUILDING
                    && match raid {
                        BombingRaid::Economic => unit.is_economic_building(),
                        BombingRaid::Industrial => unit.is_industrial_building(),
                        BombingRaid::None => false,
                    }
            })
            .choose(&mut *rng)
        else {
            break;
        };
        let hit = rng.random::<f32>() < BOMBING_HIT_CHANCE;
        if hit {
            *levels -= 1;
            *losses.entry(*unit).or_default() += 1;
        }
        bomber.shots.push(ShotReport {
            unit: Some(*unit),
            killed: hit,
            missed: !hit,
            ..Default::default()
        });
    }
}

/// Returns whether a probabilistic rapid-fire chain ends after this shot.
fn rapid_fire_stops(attacker: &Unit, target: &Unit, shots_fired: usize, roll: f32) -> bool {
    shots_fired >= MAX_SHOTS_PER_UNIT_PER_ROUND
        || roll
            >= RAPID_FIRE.get(attacker).and_then(|table| table.get(target)).copied().unwrap_or(0)
                as f32
                / 100.0
}

/// Selects a target from the highest-priority non-missile category available to the shooter.
fn choose_combat_target<'a, R: Rng + ?Sized>(
    attacker: Unit,
    defenders: &'a mut [CombatUnit],
    rng: &mut R,
) -> Option<&'a mut CombatUnit> {
    let priority = defenders
        .iter()
        .filter(|defender| !defender.unit.is_missile())
        .map(|defender| combat_target_priority(attacker, defender.unit))
        .min()?;

    defenders
        .iter_mut()
        .filter(|defender| {
            !defender.unit.is_missile()
                && combat_target_priority(attacker, defender.unit) == priority
        })
        .choose(rng)
}

/// Ships engage their preferred battlefield role before falling back to the other category.
fn combat_target_priority(attacker: Unit, target: Unit) -> u8 {
    match attacker {
        Unit::Ship(Ship::Bomber) => u8::from(!target.is_defense()),
        Unit::Ship(_) => u8::from(!(target.is_ship() || target == Unit::space_dock())),
        _ => 0,
    }
}

/// Fires unused antiballistic missiles sequentially until this incoming missile is destroyed.
/// A successful interceptor ends the attempt immediately, preserving every later interceptor for
/// the next incoming missile.
fn intercept_incoming_missile(
    defenders: &mut [CombatUnit],
    used_antiballistic: &mut Vec<u64>,
    mut roll: impl FnMut() -> f32,
) -> bool {
    for defender in defenders {
        if defender.unit != Unit::antiballistic_missile()
            || used_antiballistic.contains(&defender.id)
        {
            continue;
        }
        used_antiballistic.push(defender.id);
        let intercepted = roll() < 0.5;
        defender.shots.push(ShotReport {
            unit: Some(Unit::interplanetary_missile()),
            missed: !intercepted,
            killed: intercepted,
            ..Default::default()
        });

        if intercepted {
            return true;
        }
    }

    false
}

#[cfg(test)]
#[path = "../../../tests/core/combat_withdrawal.rs"]
mod withdrawal_tests;

#[cfg(test)]
#[path = "../../../tests/core/combat_resolution.rs"]
mod tests;

#[cfg(test)]
#[path = "../../../tests/core/combat_balance.rs"]
mod balance_tests;

#[cfg(test)]
#[path = "../../../tests/core/combat_bombing.rs"]
mod bombing_tests;
