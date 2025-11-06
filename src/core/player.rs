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

pub struct PlanetInfo {
    pub turn: usize,
    pub owner: Option<ClientId>,
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
                crystal: 1500,
                deuterium: 1500,
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
        planet.controlled() == Some(self.id)
    }

    pub fn resource_production(&self, planets: &Vec<Planet>) -> Resources {
        planets.iter().filter(|p| p.owned == Some(self.id)).map(|p| p.resource_production()).sum()
    }

    pub fn last_info(&self, id: PlanetId, missions: &Vec<Mission>) -> Option<PlanetInfo> {
        let mut reports = vec![];

        for r in self.reports.iter() {
            reports.push(if r.mission.origin == id && r.mission.owner == self.id {
                // Mission send from this planet
                PlanetInfo {
                    turn: r.mission.send,
                    owner: r.mission.origin_owned,
                    army: Unit::all()
                        .iter()
                        .flatten()
                        .map(|u| (*u, r.mission.origin_army.amount(u) - r.mission.army.amount(&u)))
                        .collect(),
                }
            } else if r.mission.destination == id {
                // Mission arrived at this planet
                let can_see = r.can_see(&Side::Defender, self.id);
                PlanetInfo {
                    turn: r.turn,
                    owner: r.destination_owned,
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

        // Add missions that were sent from the planet but haven't arrived yet
        reports.extend(missions.into_iter().filter_map(|m| {
            (m.origin == id && m.owner == self.id).then_some(PlanetInfo {
                turn: m.send,
                owner: m.origin_owned,
                army: Unit::all()
                    .iter()
                    .flatten()
                    .map(|u| (u.clone(), m.origin_army.amount(u) - m.army.amount(u)))
                    .collect(),
            })
        }));

        // Clean reports where no units can be seen (e.g., if combat is lost)
        reports.retain(|r| r.army.iter().any(|(_, c)| *c > 0));

        // If there are no reports, add missile strikes reports,
        // which say something about the silo's level
        if reports.is_empty() {
            reports.extend(missions.into_iter().filter_map(|m| {
                (m.origin == id && m.owner != self.id && m.objective == Icon::MissileStrike)
                    .then_some(PlanetInfo {
                        turn: m.send,
                        owner: m.origin_owned,
                        army: Army::from([(
                            Unit::Building(Building::MissileSilo),
                            (m.army.amount(&Unit::interplanetary_missile()) + 9)
                                / SILO_CAPACITY_FACTOR,
                        )]),
                    })
            }));
        }

        reports.into_iter().max_by_key(|i| i.turn)
    }
}
