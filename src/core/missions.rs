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

#[cfg(feature = "app")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
/// Visual language used for a route without affecting authoritative movement.
pub(crate) enum MissionRouteStyle {
    /// Ordinary sub-light fleet travel.
    Standard,
    /// A one-turn, fuel-free jump between two gates.
    JumpGate,
}

#[cfg(test)]
#[path = "../../tests/core/missions_movement.rs"]
mod movement_tests;

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
                Deuterium Synthesizer. Once per battle, after the first round ending with the \
                Planetary Shield down, each surviving Bomber has one 10% chance to destroy a \
                level. Targets are chosen randomly, with at most 3 levels lost per building \
                and 9 in total."
            },
            BombingRaid::Industrial => {
                "Bombers target unit production buildings: Shipyard, Factory and Missile Silo. \
                Reducing a Silo's level does not destroy the enemy's missiles that surpass the \
                new capacity limit. Once per battle, after the first round ending with the \
                Planetary Shield down, each surviving Bomber has one 10% chance to destroy a \
                level. Targets are chosen randomly, with at most 3 levels lost per building \
                and 9 in total."
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
    /// Completed movement turns on this leg; acceleration resets on a new leg.
    pub travel_turns: usize,
    /// Current world-space position.
    pub position: Vec2,
    /// Strategic objective applied on arrival.
    pub objective: Icon,
    /// Original objective whose silhouette is retained while a resolved mission returns home.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub return_objective: Option<Icon>,
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
            travel_turns: 0,
            position: {
                // Start at the edge of the origin planet
                let direction = (-origin.position + destination.position).normalize_or_zero();
                origin.position + direction * Planet::SIZE * 0.7
            },
            objective,
            return_objective: None,
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

    /// Returns the mission silhouette, keeping jump-gate details private to the owner.
    pub fn image(&self, player: &Player) -> &str {
        let image_objective = self.return_objective.unwrap_or(self.objective);
        if image_objective == Icon::Colonize || self.uses_colony_ship_image() {
            "mission colonize"
        } else if self.uses_war_sun_image() {
            "mission destroy"
        } else if image_objective == Icon::MissileStrike {
            "mission missile"
        } else if image_objective == Icon::Spy {
            "mission spy"
        } else if self.owner == player.id && self.jump_gate {
            "mission jump"
        } else {
            "mission"
        }
    }

    /// Returns whether every dispatched unit is a colony ship.
    pub(crate) fn uses_colony_ship_image(&self) -> bool {
        self.army.amount(&Unit::colony_ship()) > 0
            && self.army.iter().all(|(unit, count)| *count == 0 || *unit == Unit::colony_ship())
    }

    /// Returns whether this fleet uses the War Sun silhouette on the strategic map.
    pub(crate) fn uses_war_sun_image(&self) -> bool {
        self.army.amount(&Unit::war_sun()) > 0 || self.return_objective == Some(Icon::Destroy)
    }

    /// Returns the route treatment visible to this player.
    #[cfg(feature = "app")]
    pub(crate) fn route_style(&self, player: &Player) -> MissionRouteStyle {
        if self.owner == player.id && self.jump_gate {
            MissionRouteStyle::JumpGate
        } else {
            MissionRouteStyle::Standard
        }
    }

    /// Scales cosmetic route motion from the fleet's actual slowest unit.
    #[cfg(feature = "app")]
    pub(crate) fn route_speed_factor(&self) -> f64 {
        if self.jump_gate {
            2.0
        } else {
            (f64::from(self.speed()) / 2.0).clamp(0.55, 1.65)
        }
    }

    /// Retains an outbound objective's silhouette while this mission resolves as a safe deploy.
    pub(crate) fn with_return_objective(mut self, objective: Icon) -> Self {
        debug_assert!(matches!(objective, Icon::Spy | Icon::Destroy));
        debug_assert_eq!(self.objective, Icon::Deploy);
        self.return_objective = Some(objective);
        self
    }

    /// Returns whether the optional return-trip presentation metadata is internally consistent.
    pub(crate) fn has_valid_return_objective(&self) -> bool {
        self.return_objective.is_none_or(|objective| {
            self.objective == Icon::Deploy && matches!(objective, Icon::Spy | Icon::Destroy)
        })
    }

    /// Returns the route-marker speed shared by the strategic map and mission panels.
    #[cfg(feature = "app")]
    pub(crate) fn route_animation_speed(&self) -> f64 {
        if self.jump_gate {
            300.0
        } else {
            32.0 * self.route_speed_factor()
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

    /// Returns remaining travel duration, including acceleration and jump-gate rules.
    pub fn duration(&self, map: &Map) -> usize {
        self.turns_to_destination(map)
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
        self.position = self.next_turn_position(map);
        self.travel_turns = self.travel_turns.saturating_add(1);
    }

    /// Returns the next turn's displacement in AU, including the final arrival snap.
    pub fn next_turn_movement(&self, map: &Map) -> f32 {
        self.position.distance(self.next_turn_position(map)) / Planet::SIZE
    }

    fn next_turn_position(&self, map: &Map) -> Vec2 {
        let destination = map.get(self.destination);

        if self.jump_gate || (self.speed() > 0.0 && self.turns_to_destination(map) <= 1) {
            destination.position
        } else {
            let offset = destination.position - self.position;
            let step = self.speed() * (1.0 + 2.0 * self.travel_turns as f32 / 3.0) * Planet::SIZE;
            if offset.length() <= step {
                destination.position
            } else {
                self.position + offset.normalize_or_zero() * step
            }
        }
    }

    /// Returns whole turns remaining before this mission arrives.
    pub fn turns_to_destination(&self, map: &Map) -> usize {
        let distance = f64::from(self.distance(map));
        if distance == 0.0 || self.speed() == 0.0 {
            return 0;
        }
        if self.jump_gate {
            return 1;
        }
        // D(t) = s*t*(t+2)/3. Solve D(t+n)-D(t) for remaining turns n.
        // Rationalizing the root avoids cancellation late in long journeys.
        let age = self.travel_turns as f64 + 1.0;
        let scaled = 3.0 * distance / f64::from(self.speed());
        let remaining = scaled / ((age * age + scaled).sqrt() + age);
        // World positions use f32; absorb only their rounding noise at whole-turn boundaries.
        (remaining - 1e-6).ceil().max(1.0) as usize
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
