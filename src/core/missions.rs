//! Persisted fleet missions, movement calculations, visibility, and optional Bevy adapters.

#[cfg(feature = "app")]
pub use super::mission_systems::{
    send_mission, update_mission_route_arrow, update_missions, MissionRouteArrowCmp,
};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::constants::{NEXUS_FACTOR, PHALANX_DISTANCE, RADAR_DISTANCE};
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::player::Player;
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Combat, Description, Unit};
use crate::utils::NameFromEnum;

/// Stable identifier of a persisted fleet mission.
pub type MissionId = u64;

#[derive(Resource, Clone, Default, Serialize, Deserialize)]
/// Bevy resource containing the selected player's currently visible missions.
pub struct Missions(pub Vec<Mission>);

impl Missions {
    /// Returns the mission with the requested stable identifier when it is still visible.
    pub fn get(&self, mission_id: MissionId) -> Option<&Mission> {
        self.0.iter().find(|mission| mission.id == mission_id)
    }

    /// Iterates over the contained values without transferring ownership.
    pub fn iter(&self) -> std::slice::Iter<'_, Mission> {
        self.0.iter()
    }
}

#[derive(Message)]
/// Bevy message carrying a mission selected in the local UI.
pub struct SendMissionMsg {
    /// Mission selected for dispatch by the local UI.
    pub mission: Mission,
}

impl SendMissionMsg {
    /// Creates a new value from the supplied state.
    pub fn new(mission: Mission) -> Self {
        Self {
            mission,
        }
    }
}

#[derive(EnumIter, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
/// Optional building category targeted by bombers after combat.
pub enum BombingRaid {
    #[default]
    /// Bomb the none building category after combat.
    None,
    /// Bomb the economic building category after combat.
    Economic,
    /// Bomb the industrial building category after combat.
    Industrial,
}

impl Description for BombingRaid {
    /// Returns the user-facing description of this gameplay value.
    fn description(&self) -> &str {
        match self {
            BombingRaid::None => "No bombing raid.",
            BombingRaid::Economic => {
                "Bombers target resource production buildings: Metal Mine, Crystal Mine and \
                Deuterium Synthesizer."
            },
            BombingRaid::Industrial => {
                "Bombers target unit production buildings: Shipyard, Factory and Missile Silo. \
                Reducing a Silo's level does not destroy the enemy's missiles that surpass the \
                new capacity limit."
            },
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
/// Complete persisted fleet movement and objective state.
pub struct Mission {
    /// Stable identifier used to cross-reference this value.
    pub id: MissionId,
    /// Stable player slot that owns and can view this mission.
    pub owner: PlayerId,
    /// Stable planet from which the fleet was dispatched.
    pub origin: PlanetId,
    /// Owner of the origin when the mission was dispatched.
    pub origin_owned: Option<PlayerId>,
    /// Controller of the origin when the mission was dispatched.
    pub origin_controlled: Option<PlayerId>,
    /// Origin army snapshot used by later intelligence reports.
    pub origin_army: Army,
    /// Stable planet toward which the mission is travelling.
    pub destination: PlanetId,
    /// Turn on which the mission was dispatched.
    pub send: usize,
    /// Current world-space position.
    pub position: Vec2,
    /// Strategic objective applied on arrival.
    pub objective: Icon,
    /// Units stationed on this world or travelling with this mission.
    pub army: Army,
    /// Optional building category selected for post-combat bombing.
    pub bombing: BombingRaid,
    /// Whether probes remain in fleet combat beyond reconnaissance.
    pub combat_probes: bool,
    /// Jump-gate capacity consumed during the current turn.
    pub jump_gate: bool,
    /// Append-only human-readable mission history.
    pub logs: String,
}

impl Mission {
    /// Creates a new value from the supplied state.
    pub fn new(
        turn: usize,
        owner: PlayerId,
        origin: &Planet,
        destination: &Planet,
        objective: Icon,
        army: Army,
        bombing: BombingRaid,
        combat_probes: bool,
        jump_gate: bool,
        logs: Option<String>,
    ) -> Self {
        Self::new_with_id(
            rand::random::<u64>().max(1),
            turn,
            owner,
            origin,
            destination,
            objective,
            army,
            bombing,
            combat_probes,
            jump_gate,
            logs,
        )
    }

    /// Creates a mission with an explicit deterministic identifier.
    pub fn new_with_id(
        id: MissionId,
        turn: usize,
        owner: PlayerId,
        origin: &Planet,
        destination: &Planet,
        objective: Icon,
        army: Army,
        bombing: BombingRaid,
        combat_probes: bool,
        jump_gate: bool,
        logs: Option<String>,
    ) -> Self {
        Mission {
            id,
            owner,
            origin: origin.id,
            origin_owned: origin.owned,
            origin_controlled: origin.controlled,
            origin_army: origin.army.clone(),
            destination: destination.id,
            send: turn,
            position: {
                // Start at the edge of the origin planet
                let direction = (-origin.position + destination.position).normalize_or_zero();
                origin.position + direction * Planet::SIZE * 0.7
            },
            objective,
            army,
            bombing,
            combat_probes,
            jump_gate,
            logs: logs.unwrap_or(format!("- ({turn}) Mission send to {}.", destination.name)),
        }
    }

    /// Creates this value from mission.
    pub fn from_mission(
        turn: usize,
        owner: PlayerId,
        origin: &Planet,
        destination: &Planet,
        mission: &Mission,
    ) -> Self {
        Self::new_with_id(
            if mission.id == 0 {
                rand::random::<u64>().max(1)
            } else {
                mission.id
            },
            turn,
            owner,
            origin,
            destination,
            mission.objective,
            mission.army.clone(),
            mission.bombing.clone(),
            mission.combat_probes,
            mission.jump_gate,
            None,
        )
    }

    /// Returns the neutral silhouette, keeping jump-gate details private to the owner.
    pub fn image(&self, player: &Player) -> &str {
        if self.owner == player.id && self.jump_gate {
            "mission jump"
        } else {
            "mission"
        }
    }

    /// Returns remaining world-space distance from the mission to its destination.
    pub fn distance(&self, map: &Map) -> f32 {
        // Minus 0.7 since the mission ends at the edge of the planet
        (self.position.distance(map.get(self.destination).position) / Planet::SIZE - 0.7).max(0.)
    }

    /// Returns the movement or animation speed represented by this value.
    pub fn speed(&self) -> f32 {
        self.army
            .iter()
            .filter_map(|(u, c)| {
                (*c > 0).then_some(if self.jump_gate {
                    f32::MAX
                } else {
                    u.speed()
                })
            })
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.)
    }

    /// Returns travel duration after applying ship speed and jump-gate rules.
    pub fn duration(&self, map: &Map) -> usize {
        let distance = self.distance(map);
        let speed = self.speed();
        if speed != 0. {
            (distance / speed).ceil() as usize
        } else {
            0
        }
    }

    /// Returns the deuterium consumed for the supplied movement distance.
    pub fn fuel_consumption(&self, map: &Map) -> usize {
        if self.jump_gate {
            0
        } else {
            let origin = map.get(self.origin);
            let reactor = origin.army.amount(&Unit::Building(Building::Reactor)) as f32;

            let distance = self.distance(map);
            let fuel = self
                .army
                .iter()
                .map(|(u, n)| (u.fuel_consumption() * n) as f32 * distance)
                .sum::<f32>();

            (fuel * (1. - NEXUS_FACTOR * reactor)).ceil() as usize
        }
    }

    /// Returns the army's total production-value score.
    pub fn total(&self) -> usize {
        self.army.values().sum()
    }

    /// Moves the mission toward its destination without overshooting it.
    pub fn advance(&mut self, map: &Map) {
        let destination = map.get(self.destination);

        if self.jump_gate {
            self.position = destination.position;
        } else {
            let offset = destination.position - self.position;
            let step = self.speed() * Planet::SIZE;
            if offset.length() <= step {
                self.position = destination.position;
            } else {
                self.position += offset.normalize_or_zero() * step;
            }
        }
    }

    /// Returns whole turns remaining before this mission arrives.
    pub fn turns_to_destination(&self, map: &Map) -> usize {
        (self.distance(map) / self.speed()).ceil() as usize
    }

    /// Returns jump-gate capacity consumed by this mission's fleet.
    pub fn jump_cost(&self) -> usize {
        self.army.iter().map(|(u, c)| u.production() * c).sum()
    }

    /// Merges compatible simultaneous arrivals into one deterministic mission.
    pub fn merge(&mut self, other: &Mission) {
        // The planet of origin becomes the one that send the
        // largest army (measured by production amount)
        if self.army.total_production() < other.army.total_production() {
            self.origin = other.origin;
            self.origin_owned = other.origin_owned;
            self.origin_controlled = other.origin_controlled;
            self.origin_army = other.origin_army.clone();
        }

        // Select objective based on priority
        if other.objective.priority().unwrap_or(0) > self.objective.priority().unwrap_or(0) {
            self.objective = other.objective;
        }

        for (u, c) in &other.army {
            let count = self.army.entry(*u).or_default();
            *count = count.saturating_add(*c);
        }

        self.combat_probes = other.combat_probes || self.combat_probes;

        self.logs.push_str(
            format!("\n- Merged with other mission with objective {}.", other.objective.to_name())
                .as_str(),
        );
    }

    /// Return the origin planet if still controlled by the player,
    /// else go to the nearest friendly planet
    pub fn check_origin(&self, map: &Map) -> PlanetId {
        let origin = map.get(self.origin);
        if origin.controlled == Some(self.owner) {
            origin.id
        } else {
            map.planets
                .iter()
                .filter(|p| p.controlled == Some(self.owner))
                .min_by(|left, right| {
                    left.position
                        .distance(self.position)
                        .total_cmp(&right.position.distance(self.position))
                })
                .map(|p| p.id)
                .unwrap_or(origin.id)
        }
    }

    /// If a player can see this mission by Sensor Phalanx, return the level of the radar
    pub fn is_seen_by_phalanx(&self, map: &Map, player: &Player) -> Option<usize> {
        let destination = map.get(self.destination);
        let phalanx = destination.army.amount(&Unit::Building(Building::SensorPhalanx));
        (player.owns(destination)
            && PHALANX_DISTANCE * phalanx as f32 * Planet::SIZE + destination.size() * 0.5
                >= destination.position.distance(self.position)
            && !self.objective.is_hidden())
        .then_some(phalanx)
    }

    /// If a player can see this mission by Orbital Radar, return the level of the radar
    pub fn is_seen_by_radar(&self, map: &Map, player: &Player) -> Option<usize> {
        map.moons().into_iter().find_map(|moon| {
            let radar = moon.army.amount(&Unit::Building(Building::OrbitalRadar));
            (player.controls(moon)
                && RADAR_DISTANCE * radar as f32 * Planet::SIZE + moon.size() * 0.5
                    >= moon.position.distance(self.position))
            .then_some(radar)
        })
    }
}
