//! Targeted, committed deep-space rendezvous using the normal fleet movement model.

use std::collections::HashSet;

use bevy::prelude::Vec2;
use rand::Rng;
use serde::{Deserialize, Serialize};

use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::Planet;
use crate::core::missions::{BombingRaid, Mission, MissionId};
use crate::core::patrols::{encounter_site, install_survivors, return_port};
use crate::core::player::Player;
use crate::core::simulation::GameModel;
use crate::core::units::{Amount, Army, Unit};

/// Bounds trajectory prediction independently of untrusted persisted turn counters.
pub const MAX_INTERCEPT_TURNS: usize = 256;

// World positions are f32; this absorbs their rounding noise without granting useful range.
const CONTACT_TOLERANCE: f32 = 0.01;
const MIN_CONTACT_FRACTION: f64 = 1e-6;

/// Whether a targeted fleet is pursuing its rendezvous or returning from the operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum InterceptPhase {
    /// Fly to the committed coordinate and wait for the scheduled contact.
    Outbound,
    /// Return without tracking or engaging the original target again.
    Returning,
}

/// A fixed ambush, not an omniscient moving destination.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Intercept {
    /// Stable enemy mission selected while visible.
    pub target: MissionId,
    /// Identifies the selected travel leg; recalling a target invalidates the ambush.
    pub target_send: usize,
    /// Committed contact position along the target's flight path, in world units.
    pub point: Vec2,
    /// Absolute contact turn, including the target's arrival turn when reachable in space.
    pub encounter_turn: usize,
    /// Time within the contact turn, greater than zero and at most one.
    pub encounter_fraction: f32,
    /// Explicit outbound/return lifecycle, independent of planet ownership.
    pub phase: InterceptPhase,
}

/// A contact must be visible now; knowing its identifier never grants a targeting lock.
pub fn target_available(target: &Mission, map: &Map, player: &Player) -> bool {
    !player.spectator
        && target.owner != player.id
        && target.objective != Icon::MissileStrike
        && !target.jump_gate
        && !target.deep_cover
        && !target.is_patrolling()
        && target.duration(map) > 0
        && (target.is_seen_by_phalanx(map, player).is_some()
            || target.is_seen_by_radar(map, player).is_some())
}

impl Mission {
    /// Plots the earliest reachable meeting along the target's predicted flight segments.
    /// Both fleets follow normal accelerated travel; planetary arrival space is excluded.
    pub fn plan_intercept(
        id: MissionId,
        turn: usize,
        owner: PlayerId,
        origin: &Planet,
        target: &Mission,
        army: Army,
        map: &Map,
    ) -> Option<Self> {
        if !Icon::Intercept.accepts_army(&army) {
            return None;
        }
        let mut projected = target.clone();
        let mut mission = Self::new_with_id(
            id,
            turn,
            owner,
            origin,
            origin,
            Icon::Intercept,
            army,
            BombingRaid::None,
            true,
            false,
            None,
        );
        let horizon = target.duration(map).min(MAX_INTERCEPT_TURNS);
        let speed = mission.speed();
        for step in 1..=horizon {
            let start = projected.position;
            let end = projected.next_turn_position(map);
            let space_destination = projected.objective == Icon::Intercept
                || projected.patrol.as_ref().is_some_and(|patrol| {
                    patrol.phase != crate::core::patrols::PatrolPhase::Returning
                });
            let max_fraction = if space_destination {
                1.0
            } else {
                // Landing snaps to the world's center. Contact must precede the arrival
                // margin, so the planet's garrison can never join this fleet-only battle.
                ((start.distance(map.get(projected.destination).position)
                    - Planet::SIZE * 0.7
                    - CONTACT_TOLERANCE)
                    / start.distance(end))
                .min(1.0)
            };
            let fraction =
                reachable_fraction(start, end, origin.position, speed, step - 1, max_fraction);
            projected.advance(map);
            let Some(fraction) = fraction else {
                continue;
            };
            let point = start.lerp(end, fraction);
            if !point.is_finite() || !map.rect.contains(point) {
                continue;
            }
            let encounter_turn = turn.checked_add(step)?;
            mission.intercept = Some(Intercept {
                target: target.id,
                target_send: target.send,
                point,
                encounter_turn,
                encounter_fraction: fraction,
                phase: InterceptPhase::Outbound,
            });
            mission.position = origin.position
                + (point - origin.position).normalize_or_zero()
                    * (Planet::SIZE * 0.7).min(origin.position.distance(point));
            mission.logs = format!(
                "- ({turn}) Intercept mission sent; rendezvous on turn {encounter_turn} at ({:.1}, {:.1}) AU.",
                point.x / Planet::SIZE, point.y / Planet::SIZE,
            );
            return Some(mission);
        }
        None
    }

    /// Origin concealment applies to both outbound and homeward interception legs.
    pub fn conceals_origin(&self) -> bool {
        self.intercept.is_some()
            || self.objective.is_hidden()
            || self.return_objective.is_some_and(|objective| objective.is_hidden())
    }

    /// Checks persisted interception state without requiring the target to still exist.
    pub(crate) fn valid_intercept(&self, map: &Map) -> bool {
        let Some(intercept) = &self.intercept else {
            return self.objective != Icon::Intercept
                && self.return_objective != Some(Icon::Intercept);
        };
        intercept.target != 0
            && intercept.target != self.id
            && intercept.target_send > 0
            && intercept.target_send < intercept.encounter_turn
            && intercept.encounter_fraction.is_finite()
            && intercept.encounter_fraction > 0.0
            && intercept.encounter_fraction <= 1.0
            && intercept.point.is_finite()
            && map.rect.contains(intercept.point)
            && self.patrol.is_none()
            && !self.jump_gate
            && !self.deep_cover
            && self.joint_attack.is_none()
            && self.protected_player.is_none()
            && self.bombing == BombingRaid::None
            && self.army.has_army()
            && self.army.keys().all(Unit::is_ship)
            && match intercept.phase {
                InterceptPhase::Outbound => {
                    self.objective == Icon::Intercept
                        && self.return_objective.is_none()
                        && self.origin == self.destination
                        && intercept.encounter_turn > self.send
                        && intercept.encounter_turn.saturating_sub(self.send) <= MAX_INTERCEPT_TURNS
                },
                InterceptPhase::Returning => {
                    self.objective == Icon::Deploy && self.return_objective == Some(Icon::Intercept)
                },
            }
    }

    /// Starts a physical return journey; losing a port never turns it into a hostile arrival.
    pub(crate) fn return_intercept(&mut self, map: &Map, turn: usize) {
        if let Some(port) = return_port(map, self.owner, self.destination, self.position) {
            self.destination = port;
        }
        if let Some(intercept) = &mut self.intercept {
            intercept.phase = InterceptPhase::Returning;
        }
        self.objective = Icon::Deploy;
        self.return_objective = Some(Icon::Intercept);
        self.travel_turns = 0;
        self.send = turn;
        self.logs.push_str(&format!("\n- ({turn}) Interception complete; returning to port."));
    }
}

/// Intersects a linearly swept turn with the interceptor's expanding travel radius.
/// Segment pacing matches map travel and patrol exposure; acceleration changes at turn boundaries.
fn reachable_fraction(
    start: Vec2,
    end: Vec2,
    origin: Vec2,
    speed: f32,
    elapsed: usize,
    max_fraction: f32,
) -> Option<f32> {
    let upper = f64::from(max_fraction);
    if !upper.is_finite() || upper < MIN_CONTACT_FRACTION || speed <= 0.0 {
        return None;
    }
    let offset = (start.as_dvec2() - origin.as_dvec2()) / f64::from(Planet::SIZE);
    let segment = (end.as_dvec2() - start.as_dvec2()) / f64::from(Planet::SIZE);
    let elapsed = elapsed as f64;
    let speed = f64::from(speed);
    let radius = 0.7 + speed * elapsed * (elapsed + 2.0) / 3.0;
    let growth = speed * (1.0 + 2.0 * elapsed / 3.0);
    // |offset + segment*f|² <= (radius + growth*f)².
    let a = segment.length_squared() - growth * growth;
    let b = 2.0 * (offset.dot(segment) - radius * growth);
    let c = offset.length_squared() - radius * radius;
    let value = |fraction: f64| (a * fraction + b) * fraction + c;
    if value(MIN_CONTACT_FRACTION) <= 0.0 {
        return Some(MIN_CONTACT_FRACTION as f32);
    }
    let mut roots = if a.abs() <= 1e-12 {
        if b >= 0.0 {
            return None;
        }
        [-c / b, f64::INFINITY]
    } else {
        let discriminant = b * b - 4.0 * a * c;
        let rounding = 1e-12 * (b * b + (4.0 * a * c).abs() + 1.0);
        if discriminant < -rounding {
            return None;
        }
        // This form avoids cancellation when one root is much smaller than the other.
        let q = -0.5 * (b + discriminant.max(0.0).sqrt().copysign(b));
        if q == 0.0 {
            [-b / (2.0 * a), f64::INFINITY]
        } else {
            [q / a, c / q]
        }
    };
    roots.sort_by(f64::total_cmp);
    roots
        .into_iter()
        .find(|fraction| *fraction >= MIN_CONTACT_FRACTION && *fraction <= upper)
        .map(|fraction| (fraction as f32).next_up().min(max_fraction))
}

/// Resolves rendezvous before planet arrivals. Combat never buys a free movement delay.
/// Patrol contacts share the battle budget; a delay or a changed leg makes an ambush miss.
/// Returns surviving targets whose completed movement still needs normal arrival resolution.
pub(crate) fn resolve_interceptions<R: Rng + ?Sized>(
    model: &mut GameModel,
    turn: usize,
    rng: &mut R,
    engaged: &mut HashSet<MissionId>,
    movement_spent: &mut HashSet<MissionId>,
) -> HashSet<MissionId> {
    let mut arrivals = HashSet::new();
    let mut due =
        model
            .missions
            .iter()
            .filter_map(|mission| {
                let intercept = mission.intercept.as_ref()?;
                (mission.objective == Icon::Intercept && intercept.encounter_turn <= turn)
                    .then_some((intercept.encounter_turn, intercept.encounter_fraction, mission.id))
            })
            .collect::<Vec<_>>();
    due.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
    for (_, _, id) in due {
        let Some(mut attacker) = model.missions.iter().find(|mission| mission.id == id).cloned()
        else {
            continue;
        };
        let Some(intercept) = attacker.intercept.clone() else {
            continue;
        };
        let can_reach = attacker.position.distance(intercept.point)
            <= attacker.speed()
                * (1.0 + 2.0 * attacker.travel_turns as f32 / 3.0)
                * Planet::SIZE
                * intercept.encounter_fraction
                + CONTACT_TOLERANCE;
        if !movement_spent.contains(&id) {
            attacker.advance(&model.map);
        }
        let target = model
            .missions
            .iter()
            .find(|mission| mission.id == intercept.target)
            .filter(|target| {
                intercept.encounter_turn == turn
                    && can_reach
                    && !engaged.contains(&id)
                    && !engaged.contains(&target.id)
                    && target.owner != attacker.owner
                    && target.send == intercept.target_send
                    && target.objective != Icon::MissileStrike
                    && !target.deep_cover
                    && !target.jump_gate
                    && !target.is_patrolling()
                    && target.duration(&model.map) > 0
                    && target
                        .position
                        .lerp(target.next_turn_position(&model.map), intercept.encounter_fraction)
                        .distance(intercept.point)
                        <= CONTACT_TOLERANCE
                    && attacker.position.distance(intercept.point) <= CONTACT_TOLERANCE
            })
            .cloned();
        if let Some(target) = target {
            let arriving = target.duration(&model.map) <= 1;
            let mut completed = target.clone();
            completed.advance(&model.map);
            let site =
                encounter_site(model, &target, &[target.id], intercept.point, attacker.destination);
            let mut report = crate::core::combat::resolution::resolve_combat_with_rng(
                turn, &attacker, &site, rng,
            );
            report.planet_colonized = false;
            report.planet_destroyed = false;
            report.destination_owned = None;
            report.destination_controlled = Some(target.owner);
            report.scout_probes = 0;
            // The encounter position is public to its participants; no real planet is scanned.
            report.mission.position = intercept.point;
            install_survivors(model, &[id], &report.surviving_attacker);
            install_survivors(model, &[target.id], report.surviving_defender.controller());
            if let Some(survivor) =
                model.missions.iter_mut().find(|mission| mission.id == target.id)
            {
                survivor.raid_cargo =
                    crate::core::raids::surviving_cargo(survivor.raid_cargo, &survivor.army, rng);
                // Keep this turn's original movement even when combat destroys the slowest
                // ship. Normal arrival handling still runs for a final-approach survivor.
                survivor.position = completed.position;
                survivor.travel_turns = completed.travel_turns;
                if let (Some(patrol), Some(completed)) = (&mut survivor.patrol, completed.patrol) {
                    patrol.phase = completed.phase;
                }
                survivor.logs.push_str(&format!("\n- ({turn}) Mission intercepted in deep space."));
                if arriving {
                    arrivals.insert(target.id);
                }
            }
            for player in model
                .players
                .iter_mut()
                .filter(|player| player.id == attacker.owner || player.id == target.owner)
            {
                player.push_report(report.clone());
            }
            engaged.extend([id, target.id]);
            movement_spent.insert(target.id);
        } else {
            attacker.logs.push_str(&format!("\n- ({turn}) No contact at the plotted rendezvous."));
            let mut site =
                encounter_site(model, &attacker, &[], attacker.position, attacker.destination);
            site.controlled = None;
            site.name = "No contact at rendezvous".to_owned();
            let report = crate::core::combat::report::MissionReport {
                id: rand::RngExt::random(rng),
                turn,
                mission: attacker.clone(),
                planet: site,
                scout_probes: 0,
                surviving_attacker: attacker.army.clone(),
                surviving_defender: Army::new().into(),
                planet_colonized: false,
                planet_destroyed: false,
                raid_loot: Default::default(),
                destination_owned: None,
                destination_controlled: None,
                combat_report: None,
                hidden: false,
            };
            if let Some(player) =
                model.players.iter_mut().find(|player| player.id == attacker.owner)
            {
                player.push_report(report);
            }
        }
        if let Some(mission) = model.missions.iter_mut().find(|mission| mission.id == id) {
            mission.position = attacker.position;
            if !engaged.contains(&id) {
                mission.logs.clone_from(&attacker.logs);
            }
            mission.return_intercept(&model.map, turn);
        }
        movement_spent.insert(id);
    }
    arrivals
}

#[cfg(test)]
#[path = "../../tests/core/interception.rs"]
mod tests;
