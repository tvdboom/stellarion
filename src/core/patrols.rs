//! Persistent space patrols, paid operating turns, and deterministic route interception.

use std::collections::{BTreeMap, BTreeSet, HashSet};

use bevy::prelude::Vec2;
use rand::{Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::core::combat::report::combat_fleet_strength;
use crate::core::combat::resolution::resolve_combat_with_rng;
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{IndependentPopulation, Planet, PlanetId, ShieldOverloadState};
use crate::core::missions::{BombingRaid, Mission, MissionId};
use crate::core::resources::Resources;
use crate::core::simulation::{GameModel, TurnCommand};
use crate::core::units::buildings::FleetWithdrawal;
use crate::core::units::{Amount, Army, Combat, Unit};

/// Every patrol covers a fixed three-AU radius; local flight is abstracted within this circle.
pub const PATROL_RADIUS: f32 = Planet::SIZE * 3.0;
/// Fully exposed production strength giving a fifty-percent contact probability.
pub const PATROL_DETECTION_SCALE: f64 = 20.0;
/// Bounds the optional player-selected number of complete operating turns.
pub const MAX_PATROL_TURNS: usize = 10_000;

/// Explicit lifecycle, independent of the return world's ownership or distance.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum PatrolPhase {
    /// Travelling from the launch world to the center of the zone.
    Outbound,
    /// Operating in the zone; coverage starts the turn after arrival.
    OnStation,
    /// Travelling to a valid docking world, without providing patrol coverage.
    Returning,
}

/// Space destination and remaining paid operating turns of one fleet.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Patrol {
    /// Exact strategic location; the displayed radius and local maneuvers do not change it.
    pub center: Vec2,
    /// Complete operating turns still available, or no deadline until manually recalled.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub remaining_turns: Option<usize>,
    /// Current travel or operating phase.
    pub phase: PatrolPhase,
}

impl Mission {
    /// Creates a patrol with a space destination and the launch world as its return port.
    pub fn new_patrol(
        id: MissionId,
        turn: usize,
        owner: PlayerId,
        origin: &Planet,
        center: Vec2,
        army: Army,
        turns: Option<usize>,
    ) -> Self {
        let mut mission = Self::new_with_id(
            id,
            turn,
            owner,
            origin,
            origin,
            Icon::Patrol,
            army,
            BombingRaid::None,
            true,
            false,
            None,
        );
        mission.patrol = Some(Patrol {
            center,
            remaining_turns: turns,
            phase: PatrolPhase::Outbound,
        });
        mission.position =
            origin.position + (center - origin.position).normalize_or_zero() * Planet::SIZE * 0.7;
        mission.logs = format!(
            "- ({turn}) Patrol sent to ({:.1}, {:.1}) AU.",
            center.x / Planet::SIZE,
            center.y / Planet::SIZE
        );
        mission
    }

    /// Returns the authoritative space or world destination for this travel leg.
    pub fn target_position(&self, map: &Map) -> Vec2 {
        self.patrol
            .as_ref()
            .filter(|patrol| patrol.phase != PatrolPhase::Returning)
            .map_or_else(|| map.get(self.destination).position, |patrol| patrol.center)
    }

    /// Whether the fleet currently supplies patrol coverage and owes operating fuel.
    pub fn is_patrolling(&self) -> bool {
        self.patrol.as_ref().is_some_and(|patrol| patrol.phase == PatrolPhase::OnStation)
    }

    /// Fuel for one complete operating turn, using each surviving ship's one-AU consumption.
    pub fn patrol_upkeep(&self) -> usize {
        self.army.iter().fold(0_usize, |fuel, (unit, count)| {
            fuel.saturating_add(unit.fuel_consumption().saturating_mul(*count))
        })
    }

    /// Validates space state at the persisted-data boundary, including return metadata.
    pub(crate) fn valid_patrol(&self, map: &Map) -> bool {
        let Some(patrol) = &self.patrol else {
            return self.objective != Icon::Patrol && self.return_objective != Some(Icon::Patrol);
        };
        patrol.center.is_finite()
            && map.rect.contains(patrol.center)
            && patrol.remaining_turns.is_none_or(|turns| turns <= MAX_PATROL_TURNS)
            && !self.jump_gate
            && !self.deep_cover
            && self.joint_attack.is_none()
            && self.bombing == BombingRaid::None
            && self.protected_player.is_none()
            && self.army.has_army()
            && self.army.keys().all(Unit::is_ship)
            && match patrol.phase {
                PatrolPhase::Outbound | PatrolPhase::OnStation => {
                    self.objective == Icon::Patrol
                        && self.return_objective.is_none()
                        && self.origin == self.destination
                        && (patrol.phase != PatrolPhase::OnStation
                            || self.position == patrol.center)
                },
                PatrolPhase::Returning => {
                    self.objective == Icon::Deploy && self.return_objective == Some(Icon::Patrol)
                },
            }
    }

    /// Starts or redirects a free return from the exact current position to a safe own port.
    pub(crate) fn return_patrol(&mut self, map: &Map, turn: usize) {
        if let Some(destination) = return_port(map, self.owner, self.destination, self.position) {
            self.destination = destination;
        }
        if let Some(patrol) = &mut self.patrol {
            patrol.phase = PatrolPhase::Returning;
        }
        self.objective = Icon::Deploy;
        self.return_objective = Some(Icon::Patrol);
        self.travel_turns = 0;
        self.send = turn;
        self.logs.push_str(&format!(
            "\n- ({turn}) Patrol returning to {}.",
            map.get(self.destination).name
        ));
    }
}

/// An uncontested owned world remains a port after its last ships and buildings leave.
pub(crate) fn can_return_to(planet: &Planet, owner: PlayerId) -> bool {
    !planet.is_destroyed
        && (planet.controlled == Some(owner)
            || (planet.controlled.is_none() && planet.owned == Some(owner)))
}

/// Prefers an available launch port, otherwise the nearest safe world owned or controlled.
/// Absence is left to normal player elimination; a return must never conquer a foreign port.
pub(crate) fn return_port(
    map: &Map,
    owner: PlayerId,
    preferred: PlanetId,
    position: Vec2,
) -> Option<PlanetId> {
    let valid = |planet: &&Planet| can_return_to(planet, owner);
    map.try_get(preferred)
        .filter(valid)
        .or_else(|| {
            map.planets.iter().filter(valid).min_by(|a, b| {
                a.position
                    .distance_squared(position)
                    .total_cmp(&b.position.distance_squared(position))
                    .then_with(|| a.id.cmp(&b.id))
            })
        })
        .map(|planet| planet.id)
}

/// Reserves operating fuel before ordinary purchases; explicit recalls release their reservation.
/// Stable mission IDs decide which fleets return when the empire cannot fund every patrol.
pub(crate) fn reserve_upkeep(
    model: &mut GameModel,
    player_id: PlayerId,
    turn: usize,
    commands: &[&TurnCommand],
) {
    let recalled = commands
        .iter()
        .filter_map(|command| match command {
            TurnCommand::RecallMission {
                mission_id,
            } => Some(*mission_id),
            _ => None,
        })
        .collect::<HashSet<_>>();
    let mut ids = model
        .missions
        .iter()
        .filter(|mission| {
            mission.owner == player_id && mission.is_patrolling() && !recalled.contains(&mission.id)
        })
        .map(|mission| mission.id)
        .collect::<Vec<_>>();
    ids.sort_unstable();
    for id in ids {
        let Some(index) = model.missions.iter().position(|mission| mission.id == id) else {
            continue;
        };
        let expired = model.missions[index]
            .patrol
            .as_ref()
            .is_some_and(|patrol| patrol.remaining_turns == Some(0));
        let upkeep = model.missions[index].patrol_upkeep();
        let Some(player) = model.players.iter_mut().find(|player| player.id == player_id) else {
            continue;
        };
        if expired || player.resources.deuterium < upkeep {
            model.missions[index].return_patrol(&model.map, turn);
            model.missions[index].logs.push_str(if expired {
                " Patrol duration completed."
            } else {
                " Insufficient operating deuterium."
            });
        } else {
            player.resources.deuterium -= upkeep;
        }
    }
}

/// Fraction of a linearly traversed turn inside a circle, plus the midpoint of its exposure.
/// A stationary fleet inside has full exposure; tangency alone has zero exposure.
pub fn route_exposure(start: Vec2, end: Vec2, center: Vec2) -> (f64, Vec2) {
    let offset = start.as_dvec2() - center.as_dvec2();
    let delta = end.as_dvec2() - start.as_dvec2();
    let a = delta.length_squared();
    let radius = f64::from(PATROL_RADIUS);
    if a <= f64::EPSILON {
        return (
            if offset.length_squared() < radius * radius {
                1.0
            } else {
                0.0
            },
            start,
        );
    }
    let b = 2.0 * offset.dot(delta);
    let c = offset.length_squared() - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant <= 0.0 {
        return (0.0, start);
    }
    let root = discriminant.sqrt();
    let entry = ((-b - root) / (2.0 * a)).clamp(0.0, 1.0);
    let exit = ((-b + root) / (2.0 * a)).clamp(0.0, 1.0);
    let exposure = (exit - entry).max(0.0);
    (exposure, (start.as_dvec2() + delta * ((entry + exit) * 0.5)).as_vec2())
}

/// Contact probability with diminishing returns from production-weighted fleet size.
pub fn detection_probability(exposure: f64, production: u128) -> f64 {
    let strength = exposure.clamp(0.0, 1.0) * production as f64;
    strength / (strength + PATROL_DETECTION_SCALE)
}

#[derive(Clone)]
struct PatrolGroup {
    owner: PlayerId,
    center: Vec2,
    ids: Vec<MissionId>,
    army: Army,
}

fn combined_army(model: &GameModel, ids: &[MissionId]) -> Army {
    let mut combined = Army::new();
    for mission in model.missions.iter().filter(|mission| ids.contains(&mission.id)) {
        for (unit, count) in &mission.army {
            let total = combined.entry(*unit).or_default();
            *total = total.saturating_add(*count);
        }
    }
    combined
}

/// Distributes each ship class's casualties proportionally, retaining each mission's own orders.
fn install_survivors(model: &mut GameModel, ids: &[MissionId], survivors: &Army) {
    let original = combined_army(model, ids);
    let mut allocated = BTreeMap::<MissionId, Army>::new();
    for (unit, total) in original {
        let alive = survivors.amount(&unit).min(total);
        let mut shares = model
            .missions
            .iter()
            .filter(|mission| ids.contains(&mission.id))
            .map(|mission| {
                let numerator = alive as u128 * mission.army.amount(&unit) as u128;
                (mission.id, (numerator / total as u128) as usize, numerator % total as u128)
            })
            .collect::<Vec<_>>();
        let used: usize = shares.iter().map(|(_, count, _)| count).sum();
        shares.sort_by_key(|(id, _, remainder)| (std::cmp::Reverse(*remainder), *id));
        for (index, (id, count, _)) in shares.into_iter().enumerate() {
            let amount = count + usize::from(index < alive.saturating_sub(used));
            if amount > 0 {
                allocated.entry(id).or_default().insert(unit, amount);
            }
        }
    }
    for mission in model.missions.iter_mut().filter(|mission| ids.contains(&mission.id)) {
        mission.army = allocated.remove(&mission.id).unwrap_or_default();
        mission.battle_fleets.clear();
    }
    model.missions.retain(|mission| mission.army.has_army());
}

/// One attempt per observing player/target each turn, using its strongest intersecting zone.
/// Fleets in the same zone combine before rolling, so splitting never multiplies contact rolls.
/// Space battles consume the participants' movement but never mutate any real planet.
pub(crate) fn resolve_encounters<R: Rng + ?Sized>(
    model: &mut GameModel,
    turn: usize,
    rng: &mut R,
) -> HashSet<MissionId> {
    let mut groups = BTreeMap::<(PlayerId, u32, u32), PatrolGroup>::new();
    for mission in model.missions.iter().filter(|mission| mission.is_patrolling()) {
        let center = mission.position;
        let bits = |value: f32| {
            if value == 0.0 {
                0
            } else {
                value.to_bits()
            }
        };
        let group =
            groups.entry((mission.owner, bits(center.x), bits(center.y))).or_insert_with(|| {
                PatrolGroup {
                    owner: mission.owner,
                    center,
                    ids: Vec::new(),
                    army: Army::new(),
                }
            });
        group.ids.push(mission.id);
        for (unit, count) in &mission.army {
            let amount = group.army.entry(*unit).or_default();
            *amount = amount.saturating_add(*count);
        }
    }
    for group in groups.values_mut() {
        group.ids.sort_unstable();
    }
    if groups.is_empty() {
        return HashSet::new();
    }
    let patrol_targets =
        groups.values().filter_map(|group| group.ids.first().copied()).collect::<HashSet<_>>();
    let mut target_ids = model
        .missions
        .iter()
        .filter(|mission| {
            mission.objective != Icon::MissileStrike
                && !mission.jump_gate
                && !mission.deep_cover
                && (!mission.is_patrolling() || patrol_targets.contains(&mission.id))
        })
        .map(|mission| mission.id)
        .collect::<Vec<_>>();
    target_ids.sort_unstable();
    // A seeded rotation prevents the lowest mission identifier always consuming a blockade first.
    if !target_ids.is_empty() {
        let rotation = rng.random_range(0..target_ids.len());
        target_ids.rotate_left(rotation);
    }
    let mut engaged = HashSet::new();
    let mut delayed = HashSet::new();
    let mut delayed_joint_attacks = BTreeSet::new();
    for target_id in target_ids {
        if engaged.contains(&target_id) {
            continue;
        }
        let Some(target) = model.missions.iter().find(|mission| mission.id == target_id).cloned()
        else {
            continue;
        };
        let end = target.next_turn_position(&model.map);
        let mut candidates = BTreeMap::<PlayerId, (f64, Vec2, PatrolGroup)>::new();
        for group in groups.values().filter(|group| group.owner != target.owner) {
            if group.ids.iter().any(|id| engaged.contains(id)) {
                continue;
            }
            let (exposure, point) = route_exposure(target.position, end, group.center);
            let probability = detection_probability(exposure, combat_fleet_strength(&group.army));
            if probability <= 0.0 {
                continue;
            }
            if candidates.get(&group.owner).is_none_or(|(best, _, _)| probability > *best) {
                candidates.insert(group.owner, (probability, point, group.clone()));
            }
        }
        // Rotate player priority as well; a target can fight only one interception per turn.
        let mut candidates = candidates.into_values().collect::<Vec<_>>();
        if !candidates.is_empty() {
            let rotation = rng.random_range(0..candidates.len());
            candidates.rotate_left(rotation);
        }
        for (probability, point, group) in candidates {
            if rng.random::<f64>() >= probability {
                continue;
            }
            let defender_ids = if target.is_patrolling() {
                groups
                    .values()
                    .find(|group| group.owner == target.owner && group.center == target.position)
                    .map_or_else(
                        || vec![target.id],
                        |group| {
                            group.ids.iter().copied().filter(|id| !engaged.contains(id)).collect()
                        },
                    )
            } else {
                vec![target.id]
            };
            let Some(leader) = model
                .missions
                .iter()
                .filter(|mission| group.ids.contains(&mission.id))
                .max_by_key(|mission| {
                    (combat_fleet_strength(&mission.army), std::cmp::Reverse(mission.id))
                })
                .cloned()
            else {
                continue;
            };
            let mut battle = leader.clone();
            battle.army = combined_army(model, &group.ids);
            battle.battle_fleets.clear();
            let mut site = model.map.get(battle.destination).clone();
            site.name = "Deep-space interception".to_owned();
            site.position = point;
            site.owned = None;
            site.controlled = Some(target.owner);
            site.army = combined_army(model, &defender_ids).into();
            site.protection_permissions.clear();
            site.protector_battle_plans.clear();
            site.battle_plan = target.battle_plan;
            site.fleet_withdrawal = FleetWithdrawal::Off;
            site.shield_overload = ShieldOverloadState::Ready;
            site.independent_population = IndependentPopulation::Empty;
            site.resources = Resources::default();
            site.raid_cache = Resources::default();
            site.buy.clear();
            site.is_destroyed = false;
            let mut report = resolve_combat_with_rng(turn, &battle, &site, rng);
            report.planet_colonized = false;
            report.planet_destroyed = false;
            report.destination_owned = None;
            report.destination_controlled = Some(target.owner);
            report.mission.logs.push_str(&format!(
                "\n- ({turn}) Patrol intercepted player {} in deep space.",
                target.owner
            ));
            install_survivors(model, &group.ids, &report.surviving_attacker);
            install_survivors(model, &defender_ids, report.surviving_defender.controller());
            for mission in
                model.missions.iter_mut().filter(|mission| group.ids.contains(&mission.id))
            {
                mission.logs.push_str(&format!(
                    "\n- ({turn}) Patrol engaged player {} in deep space.",
                    target.owner
                ));
            }
            for mission in
                model.missions.iter_mut().filter(|mission| defender_ids.contains(&mission.id))
            {
                if !mission.is_patrolling() {
                    mission.position = point;
                    delayed.insert(mission.id);
                    if let Some(joint) = &mission.joint_attack {
                        delayed_joint_attacks.insert(joint.id);
                    }
                }
                mission.logs.push_str(&format!(
                    "\n- ({turn}) Intercepted by player {}'s patrol.",
                    group.owner
                ));
            }
            for player in model
                .players
                .iter_mut()
                .filter(|player| player.id == group.owner || player.id == target.owner)
            {
                player.push_report(report.clone());
            }
            engaged.extend(group.ids);
            engaged.extend(defender_ids);
            break;
        }
    }
    for mission in &mut model.missions {
        if let Some(joint) = &mut mission.joint_attack {
            if delayed_joint_attacks.contains(&joint.id) {
                joint.arrival_turn = joint.arrival_turn.saturating_add(1);
            }
        }
    }
    delayed
}

/// Whether a patrol's space target is valid for a new dispatch.
pub fn valid_patrol_target(
    map: &Map,
    origin: PlanetId,
    center: Vec2,
    turns: Option<usize>,
) -> bool {
    center.is_finite()
        && map.rect.contains(center)
        && turns.is_none_or(|turns| (1..=MAX_PATROL_TURNS).contains(&turns))
        && map.planets.iter().all(|planet| center.distance(planet.position) > planet.size() * 0.7)
        && map.try_get(origin).is_some()
}

#[cfg(test)]
#[path = "../../tests/core/patrols.rs"]
mod tests;
