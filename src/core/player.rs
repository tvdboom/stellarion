use bevy::prelude::*;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

use crate::core::combat::{MissionReport, Side};
use crate::core::constants::{PROBES_PER_PRODUCTION_LEVEL, SILO_CAPACITY_FACTOR};
use crate::core::map::icon::Icon;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::missions::Mission;
use crate::core::resources::Resources;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Unit};

#[derive(Clone)]
pub struct PlanetInfo {
    /// Turn this information was valid
    pub turn: usize,

    /// Whether the planet was controlled
    pub controlled: bool,

    /// The army present on the planet this turn
    pub army: Army,
}

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Player {
    pub id: ClientId,
    pub home_planet: PlanetId,
    pub resources: Resources,
    pub reports: Vec<MissionReport>,
    pub spectator: bool,
}

impl Default for Player {
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
    pub fn new(id: ClientId, home_planet: PlanetId) -> Self {
        Self {
            id,
            home_planet,
            ..default()
        }
    }

    pub fn owns(&self, planet: &Planet) -> bool {
        planet.owned == Some(self.id)
    }

    pub fn controls(&self, planet: &Planet) -> bool {
        planet.controlled == Some(self.id)
    }

    pub fn resource_production(&self, planets: &Vec<Planet>) -> Resources {
        planets.iter().filter(|p| p.owned == Some(self.id)).map(|p| p.resource_production()).sum()
    }

    pub fn last_info(&self, id: PlanetId, missions: &Vec<Mission>) -> Option<PlanetInfo> {
        let mut reports = vec![];

        for r in self.reports.iter() {
            reports.push(if r.mission.origin == id {
                if r.mission.owner == self.id {
                    // Own mission send from this planet (and it's no longer controlled)
                    let army: Army = Unit::buildings()
                        .iter()
                        .map(|u| (*u, r.mission.origin_army.amount(u)))
                        .collect();

                    PlanetInfo {
                        turn: r.mission.send,
                        controlled: false,
                        army,
                    }
                } else {
                    // Enemy mission send from this planet
                    // Only missile strikes tell something about the army (silo's level)
                    PlanetInfo {
                        turn: r.mission.send,
                        controlled: true,
                        army: if r.mission.objective == Icon::MissileStrike {
                            Army::from([(
                                Unit::Building(Building::MissileSilo),
                                (r.mission.army.amount(&Unit::interplanetary_missile())
                                    + SILO_CAPACITY_FACTOR
                                    - 1)
                                    / SILO_CAPACITY_FACTOR,
                            )])
                        } else {
                            Army::new()
                        },
                    }
                }
            } else if r.mission.destination == id {
                // Mission arrived at this planet
                let can_see = r.can_see(&Side::Defender, self.id);
                PlanetInfo {
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
                                    Some((u.clone(), r.surviving_defender.amount(u)))
                                } else {
                                    Some((
                                        u.clone(),
                                        if u.is_building() {
                                            r.surviving_defender.amount(u)
                                        } else if *u == Unit::probe() {
                                            r.surviving_attacker.amount(u) - r.scout_probes
                                        } else {
                                            r.surviving_attacker.amount(u)
                                        },
                                    ))
                                }
                            } else if r.mission.owner == self.id
                                && r.scout_probes
                                    > (u.production() - 1) * PROBES_PER_PRODUCTION_LEVEL
                            {
                                Some((u.clone(), r.planet.army.amount(u)))
                            } else {
                                None
                            }
                        })
                        .collect(),
                }
            } else {
                continue;
            });
        }

        // Add missions that haven't arrived yet
        for m in missions.into_iter() {
            if m.origin == id {
                if m.owner == self.id && m.origin_controlled == Some(self.id) {
                    let army: Army = Unit::all()
                        .iter()
                        .flatten()
                        .map(|u| (u.clone(), m.origin_army.amount(u) - m.army.amount(u)))
                        .collect();

                    reports.push(PlanetInfo {
                        turn: m.send,
                        controlled: army.has_army(),
                        army,
                    });
                } else if m.owner != self.id {
                    reports.push(PlanetInfo {
                        turn: m.send,
                        controlled: true,
                        army: if m.objective == Icon::MissileStrike {
                            Army::from([(
                                Unit::Building(Building::MissileSilo),
                                (m.army.amount(&Unit::interplanetary_missile())
                                    + SILO_CAPACITY_FACTOR
                                    - 1)
                                    / SILO_CAPACITY_FACTOR,
                            )])
                        } else {
                            Army::new()
                        },
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
