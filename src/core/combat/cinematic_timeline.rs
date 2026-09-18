//! A deterministic, seekable movie of saved combat; no simulation is run by playback.

use std::collections::BTreeMap;

use crate::core::combat::report::{MissionReport, RoundReport, Side};
use crate::core::combat::resolution::ShotReport;
use crate::core::identity::PlayerId;
use crate::core::units::Unit;

/// Minimum spacing for readable, separately animated losses on the same building.
pub(crate) const LEVEL_LOSS_INTERVAL: f32 = 0.36;
/// War Sun discharge and collapse times also used by the schematic presentation.
pub(crate) const DEATH_RAY_DISCHARGE_AT: f32 = 2.0;
pub(crate) const DEATH_RAY_COLLAPSE_AT: f32 = 3.7;
/// Includes a heavy wreck's final flash and delayed debris before the result banner.
pub(crate) const CLOSING_HOLD: f32 = 2.7;

/// One visible combatant, retaining its identity through casualties and reordered snapshots.
pub(crate) struct CinematicActor {
    pub unit: Unit,
    pub side: Side,
    pub id: Option<u64>,
    pub owner: Option<PlayerId>,
    pub max_hull: usize,
    pub max_shield: usize,
    pub death_at: Option<f32>,
    pub retreat_at: Option<f32>,
    /// Surface buildings retain one identity while bombing removes individual levels.
    pub initial_levels: Option<usize>,
    levels: Vec<(f32, usize)>,
    states: Vec<(f32, CinematicActorState)>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CinematicActorState {
    pub hull: usize,
    pub shield: usize,
}

impl CinematicActor {
    pub fn levels_at(&self, time: f32) -> Option<usize> {
        self.initial_levels.map(|initial| {
            let end = self.levels.partition_point(|(at, _)| *at <= time);
            end.checked_sub(1).map_or(initial, |index| self.levels[index].1)
        })
    }

    /// Sampling is independent of frame rate, playback speed and the direction of a seek.
    pub fn state_at(&self, time: f32) -> CinematicActorState {
        let end = self.states.partition_point(|(at, _)| *at <= time);
        self.states[end.saturating_sub(1)].1
    }

    fn record(&mut self, time: f32, state: CinematicActorState) {
        self.states.push((time, state));
    }
}

/// Each saved shot becomes one projectile, including misses, interceptions and bombing.
pub(crate) struct CinematicShot {
    pub source: usize,
    /// `None` denotes the planet or its shared shield.
    pub target: Option<usize>,
    pub launch_at: f32,
    pub impact_at: f32,
    pub outcome: ShotReport,
}

pub(crate) struct CinematicRepair {
    /// Reports identify the healed turret, but do not retain the individual truck that healed it.
    pub source: Option<usize>,
    pub target: usize,
    pub start_at: f32,
    pub end_at: f32,
    pub amount: usize,
}

pub(crate) struct CinematicPlanetAttack {
    /// The report preserves the participating survivors, not which individual roll succeeded.
    pub sources: Vec<usize>,
    pub start_at: f32,
    pub discharge_at: f32,
    pub end_at: f32,
    /// Success belongs to the recorded aggregate attempt, not a new random roll.
    pub destroyed: bool,
}

pub(crate) struct CinematicLevelLoss {
    pub target: usize,
    pub impact_at: f32,
    pub remaining_levels: usize,
}

/// Immutable playback data. Round boundaries are internal constraints, never movie captions.
pub(crate) struct CinematicTimeline {
    pub actors: Vec<CinematicActor>,
    pub shots: Vec<CinematicShot>,
    pub repairs: Vec<CinematicRepair>,
    pub planet_attacks: Vec<CinematicPlanetAttack>,
    pub level_losses: Vec<CinematicLevelLoss>,
    pub entrance_duration: f32,
    pub duration: f32,
    pub initial_planetary_shield: usize,
    planetary_shield: Vec<(f32, usize)>,
}

type ActorKey = (bool, u64);

impl CinematicTimeline {
    pub fn new(report: &MissionReport) -> Self {
        let initial_planetary_shield = report.initial_planetary_shield();
        let mut movie = Self {
            actors: Vec::new(),
            shots: Vec::new(),
            repairs: Vec::new(),
            planet_attacks: Vec::new(),
            level_losses: Vec::new(),
            entrance_duration: 4.8,
            duration: 4.0,
            initial_planetary_shield,
            planetary_shield: vec![(0.0, initial_planetary_shield)],
        };
        let Some(combat) = &report.combat_report else {
            return movie;
        };
        let mut indices = BTreeMap::<ActorKey, usize>::new();
        for round in &combat.rounds {
            for (defender, army) in [(false, &round.attacker), (true, &round.defender)] {
                for record in army {
                    indices.entry((defender, record.id)).or_insert_with(|| {
                        movie.add_actor(
                            report,
                            record.unit,
                            defender,
                            Some(record.id),
                            record.owner,
                        )
                    });
                }
            }
        }

        // Orbitals are scenery unless the combat report records their destruction. They never
        // gain invented weapons merely because their shop artwork looks like a combat station.
        for (unit, count) in report.planet.army.iter() {
            if *count > 0
                && !unit.is_orbital()
                && (unit.is_economic_building() || unit.is_industrial_building())
            {
                let index = movie.add_actor(
                    report,
                    *unit,
                    true,
                    None,
                    report.planet.controlled.or(report.planet.owned),
                );
                movie.actors[index].initial_levels = Some(*count);
            } else if unit.is_orbital() && unit.is_building() {
                for _ in 0..*count {
                    movie.add_actor(
                        report,
                        *unit,
                        true,
                        None,
                        report.planet.controlled.or(report.planet.owned),
                    );
                }
            }
        }
        if let Some(retreat) = &combat.defender_retreat {
            for (unit, count) in &retreat.ships {
                if retreat.after_round.is_some() && *unit != Unit::colony_ship() {
                    continue;
                }
                for _ in 0..*count {
                    let index = movie.add_actor(
                        report,
                        *unit,
                        true,
                        None,
                        report.planet.controlled.or(report.planet.owned),
                    );
                    if retreat.after_round.is_none() {
                        movie.actors[index].retreat_at = Some(0.45);
                    }
                }
            }
        }

        let mut cursor = movie.entrance_duration;
        for (round_index, round) in combat.rounds.iter().enumerate() {
            cursor = movie.add_round(report, round, round_index, cursor, &indices);
        }
        movie.duration = cursor
            + if report.planet_destroyed {
                4.0
            } else {
                CLOSING_HOLD
            };
        movie.shots.sort_by(|left, right| left.launch_at.total_cmp(&right.launch_at));
        movie
    }

    pub fn planetary_shield_at(&self, time: f32) -> usize {
        let end = self.planetary_shield.partition_point(|(at, _)| *at <= time);
        self.planetary_shield[end.saturating_sub(1)].1
    }

    fn add_actor(
        &mut self,
        report: &MissionReport,
        unit: Unit,
        defender: bool,
        id: Option<u64>,
        owner: Option<PlayerId>,
    ) -> usize {
        let side = if defender {
            Side::Defender
        } else {
            Side::Attacker
        };
        let max_hull = report.unit_hull(unit, &side).max(1);
        let max_shield = report.unit_shield(unit, &side);
        let index = self.actors.len();
        self.actors.push(CinematicActor {
            unit,
            side,
            id,
            owner,
            max_hull,
            max_shield,
            death_at: None,
            retreat_at: None,
            initial_levels: None,
            levels: Vec::new(),
            states: vec![(
                0.0,
                CinematicActorState {
                    hull: max_hull,
                    shield: max_shield,
                },
            )],
        });
        index
    }

    fn add_round(
        &mut self,
        report: &MissionReport,
        round: &RoundReport,
        round_index: usize,
        start: f32,
        indices: &BTreeMap<ActorKey, usize>,
    ) -> f32 {
        let shot_begin = self.shots.len();
        let mut last_launch = BTreeMap::<usize, f32>::new();
        let mut bombs = Vec::new();
        for (defender, army) in [(false, &round.attacker), (true, &round.defender)] {
            for record in army {
                let source = indices[&(defender, record.id)];
                let actor = &mut self.actors[source];
                let mut state = actor.state_at(start);
                state.shield = actor.max_shield;
                actor.record(start, state);
                let ordinary = record.shots.iter().filter(|shot| !shot.is_bombing()).count();
                // During a planet strike, escorts and defenses spread their recorded fire
                // across the charge/discharge instead of falling silent before it begins.
                let covering_fire =
                    round.destroy_probability > 0.0 && record.unit != Unit::war_sun();
                let volley_window = if covering_fire {
                    3.6
                } else {
                    2.0
                };
                let cadence = (volley_window / ordinary.max(1) as f32).clamp(
                    0.016,
                    if covering_fire {
                        0.36
                    } else {
                        0.22
                    },
                );
                let stagger = unit_fraction(record.id ^ round_index as u64)
                    * if covering_fire {
                        2.1
                    } else {
                        1.25
                    };
                let mut sequence = 0;
                for shot in &record.shots {
                    if shot.is_bombing() {
                        bombs.push((source, shot));
                        continue;
                    }
                    let launch_at = start + stagger + sequence as f32 * cadence;
                    sequence += 1;
                    last_launch.insert(source, launch_at);
                    self.shots.push(CinematicShot {
                        source,
                        target: shot
                            .target_id
                            .and_then(|id| indices.get(&(!defender, id)).copied()),
                        launch_at,
                        impact_at: launch_at
                            + 0.42
                            + unit_fraction(record.id ^ sequence as u64) * 0.24,
                        outcome: shot.clone(),
                    });
                }
            }
        }

        if round.destroy_probability > 0.0 {
            let suns_ready = round
                .attacker
                .iter()
                .filter(|record| record.unit == Unit::war_sun() && record.hull > 0)
                .filter_map(|record| last_launch.get(&indices[&(false, record.id)]))
                .copied()
                .fold(start, f32::max);
            // Even a turret with just one saved shot should fire around the discharge,
            // rather than exhausting a short volley before the War Suns finish charging.
            let delay = suns_ready - start + 1.45;
            for shot in &mut self.shots[shot_begin..] {
                if self.actors[shot.source].unit != Unit::war_sun() {
                    shot.launch_at += delay;
                    shot.impact_at += delay;
                    last_launch.insert(shot.source, shot.launch_at);
                }
            }
        }

        // Every combatant fires its saved volley even if destroyed in the same round. Launches
        // overlap between sides; fatal impacts wait for the victim's own final launch. Target
        // impact order is retained so shield hits, lethal hits and later misses remain causal.
        let gap = (2.0 / (self.shots.len() - shot_begin).max(1) as f32).min(0.035);
        let mut previous_impact = BTreeMap::<Option<usize>, f32>::new();
        let mut shield_barrier = start;
        for shot in &mut self.shots[shot_begin..] {
            if let Some(previous) = previous_impact.get(&shot.target) {
                shot.impact_at = shot.impact_at.max(*previous + gap);
            }
            if shot.outcome.killed {
                if let Some(launch) = shot.target.and_then(|target| last_launch.get(&target)) {
                    shot.impact_at = shot.impact_at.max(*launch + 0.12);
                }
            }
            if shot.outcome.planetary_shield_damage > 0 {
                shot.impact_at = shot.impact_at.max(shield_barrier + gap);
                shield_barrier = shot.impact_at;
            } else if shot.target.is_some_and(|target| {
                self.actors[target].side == Side::Defender
                    && self.actors[target].unit.is_defense()
                    && self.actors[target].unit != Unit::space_dock()
            }) {
                shot.impact_at = shot.impact_at.max(shield_barrier + gap);
            }
            previous_impact.insert(shot.target, shot.impact_at);
        }
        let mut cursor = self.apply_shots(shot_begin).max(start + 1.3);

        // Repairs are target-accurate. Select each surviving truck once only for its visual beam;
        // the saved report contains repair amounts but not a source-to-target association.
        let trucks = round
            .defender
            .iter()
            .filter(|record| record.unit == Unit::repair_truck() && record.hull > 0)
            .map(|record| indices[&(true, record.id)])
            .collect::<Vec<_>>();
        let mut truck = 0;
        for record in &round.defender {
            let target = indices[&(true, record.id)];
            for amount in record.repairs.iter().copied().filter(|amount| *amount > 0) {
                let source = trucks.get(truck).copied();
                let target_ready = previous_impact.get(&Some(target)).copied().unwrap_or(start);
                let source_ready = source
                    .and_then(|source| previous_impact.get(&Some(source)))
                    .copied()
                    .unwrap_or(start);
                let source_fired =
                    source.and_then(|source| last_launch.get(&source)).copied().unwrap_or(start);
                // Repairs can overlap unrelated salvos once this target and its surviving truck
                // are clear. Multiple heals on one turret still complete in recorded order.
                let ready = target_ready.max(source_ready).max(source_fired);
                let prior_heal = self.actors[target].states.last().map_or(start, |state| state.0);
                let end_at = (ready + 0.76 + (truck % 7) as f32 * 0.055).max(prior_heal + 0.055);
                let start_at = end_at - 0.7;
                self.repairs.push(CinematicRepair {
                    source,
                    target,
                    start_at,
                    end_at,
                    amount,
                });
                let actor = &mut self.actors[target];
                let mut state = actor.state_at(end_at);
                state.hull = state.hull.saturating_add(amount).min(actor.max_hull);
                actor.record(end_at, state);
                cursor = cursor.max(end_at);
                truck += 1;
            }
        }

        let bomb_begin = self.shots.len();
        let mut previous_bomb = start;
        let mut previous_level_loss = BTreeMap::<usize, f32>::new();
        for (sequence, (source, outcome)) in bombs.into_iter().enumerate() {
            // Bombers peel toward the surface as other combatants continue firing. They must
            // finish their own volley and wait for the recorded shared-shield breach first.
            let launch_at = (start + 0.5 + sequence as f32 * 0.075)
                .max(last_launch.get(&source).copied().unwrap_or(start) + 0.12)
                .max(shield_barrier + 0.02);
            // Keep losses in report order even when different Bombers launch concurrently.
            let mut impact_at = (launch_at + 0.8).max(previous_bomb + 0.14);
            let target = outcome.unit.and_then(|unit| {
                self.actors.iter().position(|actor| {
                    actor.unit == unit
                        && actor.side == Side::Defender
                        && actor.id.is_none()
                        && actor.death_at.is_none()
                })
            });
            if outcome.killed && !outcome.missed {
                if let Some(target) =
                    target.filter(|target| self.actors[*target].initial_levels.is_some())
                {
                    // Separate repeated labels on one building without serializing other targets.
                    if let Some(previous) = previous_level_loss.get(&target) {
                        impact_at = impact_at.max(*previous + LEVEL_LOSS_INTERVAL);
                    }
                    previous_level_loss.insert(target, impact_at);
                }
            }
            previous_bomb = impact_at;
            self.shots.push(CinematicShot {
                source,
                target,
                launch_at,
                impact_at,
                outcome: outcome.clone(),
            });
            // Reserve an orbital level for each successful bomb before selecting the next one.
            if outcome.killed && !outcome.missed {
                if let Some(target) = target {
                    if self.actors[target].initial_levels.is_none() {
                        self.actors[target].death_at = Some(impact_at);
                    }
                }
            }
        }
        cursor = cursor.max(self.apply_shots(bomb_begin));
        for (defender, army) in [(false, &round.attacker), (true, &round.defender)] {
            for record in army {
                let actor = &mut self.actors[indices[&(defender, record.id)]];
                if !record.unit.is_missile() && actor.death_at.is_none() {
                    actor.record(
                        cursor + 0.01,
                        CinematicActorState {
                            hull: record.hull,
                            shield: record.shield,
                        },
                    );
                    if record.hull == 0 {
                        actor.death_at = Some(cursor + 0.01);
                    }
                }
            }
        }
        self.planetary_shield.push((cursor + 0.01, round.planetary_shield));
        cursor += 0.02;

        cursor = self.add_departures(report, round, round_index, cursor, indices);
        if round.destroy_probability > 0.0 {
            let suns = round
                .attacker
                .iter()
                .filter(|unit| unit.unit == Unit::war_sun() && unit.hull > 0)
                .map(|unit| indices[&(false, unit.id)])
                .collect::<Vec<_>>();
            if !suns.is_empty() {
                // Surviving War Suns finish their own salvos before charging, while the
                // remaining recorded fighting continues. Collapse still follows every impact,
                // repair and departure, so the overlap cannot alter casualties or outcomes.
                let suns_ready = suns
                    .iter()
                    .filter_map(|source| last_launch.get(source))
                    .copied()
                    .fold(start, f32::max)
                    + 0.12;
                let start_at =
                    (cursor - DEATH_RAY_COLLAPSE_AT + 0.35).max(start + 0.25).max(suns_ready);
                let discharge_at = start_at + DEATH_RAY_DISCHARGE_AT;
                let end_at = start_at + DEATH_RAY_COLLAPSE_AT;
                let destroyed = report.planet_destroyed
                    && report
                        .combat_report
                        .as_ref()
                        .is_some_and(|combat| round_index + 1 == combat.rounds.len());
                self.planet_attacks.push(CinematicPlanetAttack {
                    sources: suns,
                    start_at,
                    discharge_at,
                    end_at,
                    destroyed,
                });
                if destroyed {
                    for actor in &mut self.actors {
                        if actor.side == Side::Defender
                            && actor.death_at.is_none()
                            && actor.retreat_at.is_none()
                        {
                            actor.death_at = Some(end_at);
                            if actor.initial_levels.is_some() {
                                actor.levels.push((end_at, 0));
                            }
                            actor.record(
                                end_at,
                                CinematicActorState {
                                    hull: 0,
                                    shield: 0,
                                },
                            );
                        }
                    }
                    self.planetary_shield.push((end_at, 0));
                }
            }
            if let Some(attack) = self.planet_attacks.last() {
                cursor = cursor.max(attack.end_at + 0.15);
            }
        }
        cursor + 0.12
    }

    fn apply_shots(&mut self, begin: usize) -> f32 {
        let mut chronological = (begin..self.shots.len()).collect::<Vec<_>>();
        chronological.sort_by(|left, right| {
            self.shots[*left].impact_at.total_cmp(&self.shots[*right].impact_at)
        });
        let mut end: f32 = 0.0;
        for index in chronological {
            let shot = &self.shots[index];
            end = end.max(shot.impact_at);
            if !shot.outcome.missed {
                if let Some(target) = shot.target {
                    let actor = &mut self.actors[target];
                    let mut state = actor.state_at(shot.impact_at);
                    if let Some(levels) =
                        actor.levels_at(shot.impact_at).filter(|_| shot.outcome.is_bombing())
                    {
                        if shot.outcome.killed && levels > 0 {
                            let remaining_levels = levels - 1;
                            actor.levels.push((shot.impact_at, remaining_levels));
                            self.level_losses.push(CinematicLevelLoss {
                                target,
                                impact_at: shot.impact_at,
                                remaining_levels,
                            });
                            if remaining_levels == 0 {
                                state.hull = 0;
                                actor.death_at = Some(shot.impact_at);
                            }
                        }
                    } else {
                        state.hull = state.hull.saturating_sub(shot.outcome.hull_damage);
                        state.shield = state.shield.saturating_sub(shot.outcome.shield_damage);
                        if shot.outcome.killed {
                            state.hull = 0;
                            actor.death_at = Some(shot.impact_at);
                        }
                    }
                    actor.record(shot.impact_at, state);
                }
                if shot.outcome.planetary_shield_damage > 0 {
                    let remaining = self
                        .planetary_shield_at(shot.impact_at)
                        .saturating_sub(shot.outcome.planetary_shield_damage);
                    self.planetary_shield.push((shot.impact_at, remaining));
                }
            }
            let source = &mut self.actors[shot.source];
            if source.unit.is_missile() && source.death_at.is_none() {
                source.retreat_at = Some(shot.impact_at);
            }
        }
        end
    }

    fn add_departures(
        &mut self,
        report: &MissionReport,
        round: &RoundReport,
        round_index: usize,
        cursor: f32,
        indices: &BTreeMap<ActorKey, usize>,
    ) -> f32 {
        let Some(combat) = &report.combat_report else {
            return cursor;
        };
        let retreat = combat
            .defender_retreat
            .as_ref()
            .filter(|retreat| retreat.after_round == Some(round_index));
        let defender_owner = report.planet.controlled.or(report.planet.owned);
        let mut departing = false;
        if retreat.is_some() {
            for actor in &mut self.actors {
                if actor.side == Side::Defender
                    && actor.unit == Unit::colony_ship()
                    && actor.id.is_none()
                    && actor.retreat_at.is_none()
                {
                    actor.retreat_at = Some(cursor);
                    departing = true;
                }
            }
        }
        for record in &round.defender {
            if record.hull > 0
                && record.unit.is_ship()
                && record.owner == defender_owner
                && retreat.is_some_and(|retreat| {
                    retreat.ships.get(&record.unit).is_some_and(|count| *count > 0)
                })
            {
                self.actors[indices[&(true, record.id)]].retreat_at = Some(cursor);
                departing = true;
            }
        }
        // Incoming missiles are consumed even when there is no eligible surface target. Unlike
        // an interception, this has no recorded impact and must not manufacture an explosion.
        for record in &round.attacker {
            if record.unit == Unit::interplanetary_missile() {
                let actor = &mut self.actors[indices[&(false, record.id)]];
                if actor.death_at.is_none() && actor.retreat_at.is_none() {
                    actor.retreat_at = Some(cursor);
                    departing = true;
                }
            }
        }
        // Returning probes are living scouts, never explosions or additional enemy kills.
        if round_index == 0 && report.scout_probes > 0 {
            let next = combat.rounds.get(1);
            let mut remaining = report.scout_probes;
            for record in &round.attacker {
                if record.unit == Unit::probe()
                    && record.hull > 0
                    && remaining > 0
                    && next
                        .is_none_or(|next| !next.attacker.iter().any(|unit| unit.id == record.id))
                {
                    self.actors[indices[&(false, record.id)]].retreat_at = Some(cursor);
                    remaining -= 1;
                    departing = true;
                }
            }
        }
        if departing {
            cursor + 0.45
        } else {
            cursor
        }
    }
}

/// Stable visual staggering avoids consuming simulation RNG or depending on hash-map order.
fn unit_fraction(mut value: u64) -> f32 {
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    ((value ^ (value >> 31)) & 0xffff) as f32 / 65535.0
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_timeline.rs"]
mod tests;
