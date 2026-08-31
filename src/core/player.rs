//! Persisted player economy, reports, home world, and spectator state.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::combat::report::{MissionReport, Side};
use crate::core::constants::PROBES_PER_PRODUCTION_LEVEL;
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::missions::Mission;
use crate::core::resources::Resources;
use crate::core::settings::Settings;
use crate::core::units::{Amount, Army, Unit};

/// Maximum number of resolved mission reports retained for one player.
pub const MAX_REPORTS_PER_PLAYER: usize = 512;

#[derive(Clone)]
/// Historical intelligence snapshot visible to one player.
pub struct PlanetInfo {
    /// Turn this information was valid
    pub turn: usize,

    /// Whether the planet was controlled
    pub controlled: bool,

    /// The army present on the planet this turn
    pub army: Army,
}

#[derive(Resource, Clone, Serialize, Deserialize)]
/// Persisted player slot, economy, home world, reports, and elimination state.
pub struct Player {
    /// Stable identifier used to cross-reference this value.
    pub id: PlayerId,
    /// Stable home world whose loss eliminates this player.
    pub home_planet: PlanetId,
    /// Resource production for a world or stockpile for a player.
    pub resources: Resources,
    /// Resolved mission reports visible to this player.
    pub reports: Vec<MissionReport>,
    /// Whether this player is eliminated and no longer submits turns.
    pub spectator: bool,
}

impl Default for Player {
    /// Constructs the default value and its gameplay-safe initial state.
    fn default() -> Self {
        Self {
            id: 0,
            home_planet: 0,
            resources: Resources {
                metal: 1500,
                crystal: 1200,
                deuterium: 1000,
            },
            reports: Vec::new(),
            spectator: false,
        }
    }
}

impl Player {
    /// Creates a new value from the supplied state.
    pub fn new(id: PlayerId, home_planet: PlanetId) -> Self {
        Self {
            id,
            home_planet,
            ..default()
        }
    }

    /// Appends a report while pruning the oldest entries from the bounded history.
    pub fn push_report(&mut self, mut report: MissionReport) {
        if report.id == 0 || self.reports.iter().any(|existing| existing.id == report.id) {
            if let Some(replacement) = (1..=self.reports.len() as u64 + 1)
                .find(|candidate| self.reports.iter().all(|existing| existing.id != *candidate))
            {
                report.id = replacement;
            }
        }
        self.reports.push(report);
        let excess = self.reports.len().saturating_sub(MAX_REPORTS_PER_PLAYER);
        if excess > 0 {
            self.reports.drain(..excess);
        }
    }

    /// Returns whether this player owns the supplied planet.
    pub fn owns(&self, planet: &Planet) -> bool {
        planet.owned == Some(self.id)
    }

    /// Returns whether this player controls the supplied planet.
    pub fn controls(&self, planet: &Planet) -> bool {
        planet.controlled == Some(self.id)
    }

    /// Computes resource production for the current owned worlds.
    pub fn resource_production(&self, planets: &[Planet]) -> Resources {
        planets.iter().filter(|p| p.owned == Some(self.id)).map(|p| p.resource_production()).sum()
    }

    /// Counts non-moon planets currently owned by this player.
    pub fn planets_owned(&self, map: &Map, settings: &Settings) -> (usize, usize) {
        let n_owned = map.planets().iter().filter(|p| p.owned == Some(self.id)).count();
        let n_max =
            (map.planets().len() as f32 * settings.p_colonizable as f32 / 100.).ceil() as usize;

        (n_owned, n_max)
    }

    /// Returns the most recent information report for a planet when present.
    pub fn last_info(&self, planet: &Planet, missions: &[Mission]) -> Option<PlanetInfo> {
        let mut reports = vec![];

        if planet.is_destroyed {
            return None;
        }

        for r in self.reports.iter() {
            if r.mission.origin == planet.id {
                if r.mission.owner == self.id {
                    // Ignore returning probes or from destroy mission
                    if r.mission.origin_controlled == Some(self.id) {
                        // Own mission send from this planet (and it's no longer controlled)
                        reports.push(PlanetInfo {
                            turn: r.mission.send,
                            controlled: false,
                            army: Unit::all()
                                .iter()
                                .flatten()
                                .map(|u| {
                                    (
                                        *u,
                                        r.mission
                                            .origin_army
                                            .amount(u)
                                            .saturating_sub(r.mission.army.amount(u)),
                                    )
                                })
                                .collect(),
                        });
                    }
                } else if !r.mission.objective.is_hidden() {
                    // Enemy mission send from this planet
                    reports.push(PlanetInfo {
                        turn: r.mission.send,
                        controlled: true,
                        army: Army::new(),
                    });
                }
            } else if r.mission.destination == planet.id
                && r.mission.objective != Icon::MissileStrike
            {
                // Mission arrived at this planet
                let can_see = r.can_see(&Side::Defender, self.id);
                reports.push(PlanetInfo {
                    turn: r.turn,
                    controlled: r.destination_controlled.is_some(),
                    army: Unit::all()
                        .iter()
                        .flatten()
                        .filter_map(|u| {
                            if can_see {
                                if r.winner() == r.planet.controlled
                                    || r.mission.objective == Icon::Destroy
                                {
                                    Some((*u, r.surviving_defender.amount(u)))
                                } else {
                                    Some((
                                        *u,
                                        if u.is_building() {
                                            r.surviving_defender.amount(u)
                                        } else if *u == Unit::probe() {
                                            r.surviving_attacker
                                                .amount(u)
                                                .saturating_sub(r.scout_probes)
                                        } else {
                                            r.surviving_attacker.amount(u)
                                        },
                                    ))
                                }
                            } else if r.mission.owner == self.id
                                && r.scout_probes
                                    > u.production()
                                        .saturating_sub(1)
                                        .saturating_mul(PROBES_PER_PRODUCTION_LEVEL)
                            {
                                Some((*u, r.planet.army.amount(u)))
                            } else {
                                None
                            }
                        })
                        .collect(),
                });
            }
        }

        // Add missions that haven't arrived yet
        for m in missions {
            if m.origin == planet.id {
                if m.owner == self.id {
                    // Ignore returning probes or from destroy mission
                    if m.origin_controlled == Some(self.id) {
                        // Own mission send from this planet (and it's no longer controlled)
                        let army: Army = Unit::all()
                            .iter()
                            .flatten()
                            .map(|u| (*u, m.origin_army.amount(u).saturating_sub(m.army.amount(u))))
                            .collect();

                        reports.push(PlanetInfo {
                            turn: m.send,
                            controlled: false, // It's no longer controlled or we wouldn't need last_info
                            army,
                        });
                    }
                } else if !m.objective.is_hidden() {
                    // Enemy mission
                    reports.push(PlanetInfo {
                        turn: m.send,
                        controlled: true,
                        army: Army::new(),
                    });
                }
            }
        }

        // Select the latest report and take the highest building level from every report
        reports.iter().max_by_key(|r| r.turn).cloned().map(|mut best| {
            for building in Unit::buildings() {
                if let Some(highest) =
                    reports.iter().map(|r| r.army.amount(&building)).filter(|a| *a > 0).max()
                {
                    best.army.insert(building, highest);
                }
            }

            best
        })
    }
}
