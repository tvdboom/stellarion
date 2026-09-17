//! Persisted fleet missions, movement calculations, visibility, and optional Bevy adapters.

use std::collections::BTreeMap;
#[cfg(feature = "app")]
use std::collections::BTreeSet;

#[cfg(feature = "app")]
pub use super::mission_systems::{
    recall_mission, recall_protection, send_mission, update_mission_route_arrow, update_missions,
    MissionRecallAnimationMsg, MissionRouteArrowCmp,
};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::constants::{PHALANX_DISTANCE, RADAR_DISTANCE, REACTOR_FUEL_REDUCTION_FACTOR};
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

/// Presentation-only mission IDs temporarily hidden by a map aftermath animation.
#[derive(Resource, Default)]
#[doc(hidden)]
#[cfg(feature = "app")]
pub struct SuppressedMapMissions(BTreeSet<MissionId>);

#[cfg(feature = "app")]
impl SuppressedMapMissions {
    /// Returns whether an aftermath animation currently replaces this mission's map sprite.
    pub(crate) fn contains(&self, mission: MissionId) -> bool {
        self.0.contains(&mission)
    }

    /// Hides a mission while its aftermath animation supplies the visible replacement.
    pub(crate) fn suppress(&mut self, mission: MissionId) {
        self.0.insert(mission);
    }

    /// Restores a mission's ordinary map presentation after its animation.
    pub(crate) fn release(&mut self, mission: MissionId) {
        self.0.remove(&mission);
    }

    /// Drops presentation suppression when the active game or turn is replaced.
    pub(crate) fn clear(&mut self) {
        self.0.clear();
    }
}

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

#[cfg(test)]
#[path = "../../tests/core/missions_visibility.rs"]
mod visibility_tests;

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
    /// Accepted contributions when this launch came from a private invitation.
    pub joint_attack: Option<JointMissionLaunch>,
    /// Published invitation to cancel after this solo mission is accepted locally.
    pub cancel_joint_attack: Option<u64>,
}

/// Final accepted invitation data attached to the inviter's launch command.
#[derive(Clone)]
pub struct JointMissionLaunch {
    /// Stable invitation identifier.
    pub attack_id: u64,
    /// Accepted contributions with the inviter first.
    pub contributions: Vec<crate::core::simulation::JointAttackContribution>,
}

impl SendMissionMsg {
    /// Creates a new value from the supplied state.
    pub fn new(mission: Mission) -> Self {
        Self {
            mission,
            joint_attack: None,
            cancel_joint_attack: None,
        }
    }

    /// Creates a solo launch that withdraws its published invitation on success.
    pub fn solo_after_cancel(mission: Mission, attack_id: u64) -> Self {
        Self {
            mission,
            joint_attack: None,
            cancel_joint_attack: Some(attack_id),
        }
    }

    /// Creates a coordinated launch from the frozen invitation panel.
    pub fn joint(mission: Mission, joint_attack: JointMissionLaunch) -> Self {
        Self {
            mission,
            joint_attack: Some(joint_attack),
            cancel_joint_attack: None,
        }
    }
}

#[derive(Message)]
/// Bevy message requesting that one active local fleet begin its return leg.
pub struct RecallMissionMsg {
    /// Stable mission selected for recall by the local UI.
    pub mission_id: MissionId,
}

impl RecallMissionMsg {
    /// Creates a recall request for the selected mission.
    pub fn new(mission_id: MissionId) -> Self {
        Self {
            mission_id,
        }
    }
}

#[derive(EnumIter, Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
                Planetary Shield down, each surviving Bomber has a 25% chance to destroy a \
                level. Targets are chosen randomly, with at most 3 levels lost per building."
            },
            BombingRaid::Industrial => {
                "Bombers target unit production buildings: Shipyard, Factory and Missile Silo. \
                Reducing a Silo's level does not destroy the enemy's missiles that surpass the \
                new capacity limit. Once per battle, after the first round ending with the \
                Planetary Shield down, each surviving Bomber has a 25% chance to destroy a \
                level. Targets are chosen randomly, with at most 3 levels lost per building."
            },
        }
    }
}

#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Complete persisted fleet movement and objective state.
pub struct Mission {
    /// Stable identifier used to cross-reference this value.
    pub id: MissionId,
    /// Stable player slot that owns and can view this mission.
    pub owner: PlayerId,
    /// Stable planet from which the fleet was dispatched.
    pub origin: PlanetId,
    /// Owner of the origin when the mission was dispatched.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub origin_owned: Option<PlayerId>,
    /// Controller of the origin when the mission was dispatched.
    #[serde(deserialize_with = "crate::serialization::required_option")]
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
    /// Controller whose permission this Protect mission relies on.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub protected_player: Option<PlayerId>,
    /// Original objective whose silhouette is retained while a mission returns home.
    #[serde(deserialize_with = "crate::serialization::required_option")]
    pub return_objective: Option<Icon>,
    /// Units stationed on this world or travelling with this mission.
    pub army: Army,
    /// Optional building category selected for post-combat bombing.
    pub bombing: BombingRaid,
    /// Whether probes remain in fleet combat beyond reconnaissance.
    pub combat_probes: bool,
    /// Whether this Spy mission attempts to bypass combat using the origin's Command Relay.
    pub deep_cover: bool,
    /// Jump-gate capacity consumed during the current turn.
    pub jump_gate: bool,
    /// Append-only human-readable mission history.
    pub logs: String,
    /// Shared-assault identity and timing when this is one contingent of a joint mission.
    #[serde(default)]
    pub joint_attack: Option<JointAttackMission>,
}

#[derive(Message)]
/// Bevy message requesting that a locally stationed protection fleet return home.
pub struct RecallProtectionMsg {
    /// World currently defended by the local player's protection fleet.
    pub planet_id: PlanetId,
}

impl RecallProtectionMsg {
    /// Creates a stationed-protection recall request.
    pub fn new(planet_id: PlanetId) -> Self {
        Self {
            planet_id,
        }
    }
}

/// Persisted link that makes independently owned fleets one coordinated attacking force.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JointAttackMission {
    /// Stable invitation identifier shared by every participating fleet.
    pub id: u64,
    /// Player whose objective and conquest claim govern the operation.
    pub leader: PlayerId,
    /// Absolute turn on which all contingents enter combat together. Each fleet stretches its
    /// movement over this shared duration, chosen from the slowest uncoordinated arrival.
    pub arrival_turn: usize,
    /// Per-player armies populated on the synthetic mission in the shared battle report.
    #[serde(default)]
    pub attackers: BTreeMap<PlayerId, Army>,
    /// Per-player surviving armies, populated only on the resolved battle report.
    #[serde(default)]
    pub survivors: BTreeMap<PlayerId, Army>,
    /// Original departure world for each participant, used for independent returns.
    #[serde(default)]
    pub origins: BTreeMap<PlayerId, PlanetId>,
    /// Independently chosen combat orders, retained when the fleets combine.
    pub combat_orders: BTreeMap<PlayerId, FleetCombatOrders>,
    /// Surviving Probes that left combat early and return to their own departure world.
    pub scouts: BTreeMap<PlayerId, usize>,
}

impl JointAttackMission {
    /// Removes separately returning scouts before docking or retreating the rest of a fleet.
    pub(crate) fn fleet_without_scouts(&self, owner: PlayerId, army: &Army) -> Army {
        army.iter()
            .filter_map(|(unit, count)| {
                let remaining = if *unit == Unit::probe() {
                    count.saturating_sub(self.scouts.get(&owner).copied().unwrap_or(0))
                } else {
                    *count
                };
                (remaining > 0).then_some((*unit, remaining))
            })
            .collect()
    }
}

/// Combat choices belonging to one commander in a coordinated attack.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FleetCombatOrders {
    /// Buildings this commander's Bombers may target.
    pub bombing: BombingRaid,
    /// Whether this commander's Probes stay after the first combat round.
    pub combat_probes: bool,
}

impl Mission {
    /// Returns the bombing order for the owner of an individual combat unit.
    pub(crate) fn bombing_for(&self, owner: Option<PlayerId>) -> &BombingRaid {
        owner
            .and_then(|owner| self.joint_attack.as_ref()?.combat_orders.get(&owner))
            .map_or(&self.bombing, |orders| &orders.bombing)
    }

    /// Returns the probe order for the owner of an individual combat unit.
    pub(crate) fn combat_probes_for(&self, owner: Option<PlayerId>) -> bool {
        owner
            .and_then(|owner| self.joint_attack.as_ref()?.combat_orders.get(&owner))
            .map_or(self.combat_probes, |orders| orders.combat_probes)
    }

    /// Includes every participant's bombing policy when displaying a shared battle.
    pub(crate) fn includes_bombing(&self, raid: &BombingRaid) -> bool {
        self.joint_attack.as_ref().map_or(self.bombing == *raid, |attack| {
            attack.combat_orders.values().any(|orders| orders.bombing == *raid)
        })
    }

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
            origin_army: origin.mission_origin_army(owner).cloned().unwrap_or_default(),
            destination: destination.id,
            send: turn,
            travel_turns: 0,
            position: {
                // Start at the edge of the origin planet
                let direction = (-origin.position + destination.position).normalize_or_zero();
                origin.position + direction * Planet::SIZE * 0.7
            },
            objective,
            protected_player: (objective == Icon::Protect)
                .then_some(destination.controlled)
                .flatten(),
            return_objective: None,
            army,
            bombing,
            combat_probes,
            jump_gate,
            logs: logs.unwrap_or(format!("- ({turn}) Mission send to {}.", destination.name)),
            deep_cover: false,
            joint_attack: None,
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
        .with_deep_cover(mission.deep_cover)
    }

    /// Selects Deep Cover; launch validation requires a Spy mission and an origin Command Relay.
    pub fn with_deep_cover(mut self, enabled: bool) -> Self {
        self.deep_cover = enabled;
        self
    }

    /// Returns the mission silhouette.
    ///
    /// Jump-gate travel is presented by a map-only animated effect instead of alternate artwork.
    pub fn image(&self, player: &Player) -> &str {
        let image_objective = self.return_objective.unwrap_or(self.objective);
        // A resolved joint attack is represented by one combined mission, but each participant
        // should continue to see the silhouette of their own contributed fleet in reports.
        let visible_army = self
            .joint_attack
            .as_ref()
            .and_then(|attack| attack.attackers.get(&player.id))
            .unwrap_or(&self.army);
        if image_objective == Icon::Colonize {
            "mission colonize"
        } else if visible_army.amount(&Unit::war_sun()) > 0
            || self.return_objective == Some(Icon::Destroy)
        {
            "mission destroy"
        } else if image_objective == Icon::MissileStrike {
            "mission missile"
        } else if image_objective == Icon::Spy {
            "mission spy"
        } else {
            "mission"
        }
    }

    /// Returns the objective presentation visible to one player.
    ///
    /// Hostile objectives stay concealed behind the generic enemy-fleet marker. Joint-attack
    /// participants know the shared objective once another contingent becomes visible to them,
    /// and a protection target sees Protect.
    #[cfg(feature = "app")]
    pub(crate) fn displayed_objective(&self, player_id: PlayerId) -> Icon {
        if self.owner == player_id
            || self.is_joint_attack_participant(player_id)
            || self.is_incoming_protection_for(player_id)
        {
            self.objective
        } else {
            Icon::EnemyFleet
        }
    }

    /// Returns whether this player contributes a fleet to the same coordinated attack.
    #[cfg(feature = "app")]
    pub(crate) fn is_joint_attack_participant(&self, player_id: PlayerId) -> bool {
        self.joint_attack.as_ref().is_some_and(|attack| attack.attackers.contains_key(&player_id))
    }

    /// Returns whether this fleet is travelling to protect the requested player.
    ///
    /// The protected player is told about an invited fleet as soon as it launches. That visibility
    /// is independent of Sensor Phalanx and Orbital Radar coverage and includes its full formation.
    #[cfg(feature = "app")]
    pub(crate) fn is_incoming_protection_for(&self, player_id: PlayerId) -> bool {
        self.objective == Icon::Protect && self.protected_player == Some(player_id)
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

    /// Retains an outbound objective's silhouette while this mission resolves as a safe deploy.
    pub(crate) fn with_return_objective(mut self, objective: Icon) -> Self {
        debug_assert!(objective.is_mission());
        debug_assert_eq!(self.objective, Icon::Deploy);
        self.return_objective = Some(objective);
        self
    }

    /// Returns whether this fleet is already travelling back from an outbound mission.
    pub(crate) fn is_returning(&self) -> bool {
        // A fresh deployment can originate from a foreign world with our protection fleet.
        // Only explicit return metadata distinguishes it from a homeward leg.
        self.return_objective.is_some()
    }

    /// A fleet launched from another player's world may no longer return there after its
    /// protection invitation is revoked, even if the launch was made this turn.
    pub(crate) fn recall_blocked_by_revoked_protection(
        &self,
        map: &Map,
        player_id: PlayerId,
    ) -> bool {
        self.origin_owned != Some(player_id)
            && self.origin_controlled != Some(player_id)
            && map
                .try_get(self.origin)
                .is_none_or(|origin| origin.is_destroyed || !origin.allows_protection(player_id))
    }

    /// Starts a free return leg from the fleet's exact current position to its original world.
    pub(crate) fn recall(&mut self, map: &Map, turn: usize) {
        debug_assert!(!self.is_returning());
        let original_origin = self.origin;
        let outbound_destination = map.get(self.destination);
        let original_objective = self.objective;

        // Return legs use the outbound destination as their presentation/report origin. The fleet
        // has not reached that world, so its persisted world-space position deliberately stays put.
        self.origin = outbound_destination.id;
        self.origin_owned = outbound_destination.owned;
        self.origin_controlled = outbound_destination.controlled;
        self.origin_army.clone_from(outbound_destination.army.controller());
        self.destination = original_origin;
        self.send = turn;
        self.travel_turns = 0;
        self.objective = Icon::Deploy;
        self.protected_player = None;
        self.return_objective = Some(original_objective);
        self.bombing = BombingRaid::None;
        self.combat_probes = false;
        self.deep_cover = false;
        self.jump_gate = false;
        self.joint_attack = None;
        self.logs.push_str(&format!(
            "\n- ({turn}) Mission recalled to planet {}.",
            map.get(original_origin).name
        ));
    }

    /// Returns whether the optional return-trip presentation metadata is internally consistent.
    pub(crate) fn has_valid_return_objective(&self) -> bool {
        let protection_is_valid = if self.objective == Icon::Protect {
            self.protected_player.is_some()
        } else {
            self.protected_player.is_none()
        };
        protection_is_valid
            && self.return_objective.is_none_or(|objective| {
                objective.is_mission()
                    && matches!(self.objective, Icon::Deploy | Icon::Attack | Icon::Protect)
            })
    }

    /// Returns the next-turn route-marker speed shared by the strategic map and mission panels.
    ///
    /// This keeps chevrons synchronized with acceleration, coordinated-attack pacing, and the
    /// shortened final movement instead of freezing them at launch speed. Jump Gate routes use
    /// their own helix animation when that travel method is visible to the viewer.
    #[cfg(feature = "app")]
    pub(crate) fn route_animation_speed(&self, map: &Map) -> f64 {
        let movement = f64::from(self.next_turn_movement(map));
        if movement.is_finite() {
            16.0 * movement.max(0.0)
        } else {
            0.0
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

    /// Returns remaining travel duration, including acceleration, allied timing and jump gates.
    pub fn duration(&self, map: &Map) -> usize {
        self.turns_to_destination(map)
    }

    /// Returns the Deep Cover surcharge paid at launch, including on unsuccessful attempts.
    pub fn deep_cover_cost(&self) -> usize {
        if self.deep_cover && self.objective == Icon::Spy {
            self.army
                .amount(&Unit::probe())
                .saturating_mul(crate::core::constants::DEEP_COVER_DEUTERIUM_PER_PROBE)
        } else {
            0
        }
    }

    /// Returns travel fuel after the reactor discount plus any undiscounted Deep Cover cost.
    pub fn fuel_consumption(&self, map: &Map) -> usize {
        let travel_fuel = if self.jump_gate {
            0
        } else {
            let origin = map.get(self.origin);
            let reactor = origin.army.amount(&Unit::Building(Building::Reactor)) as f32;

            let distance = self.distance(map);
            let fuel = self
                .army
                .iter()
                .map(|(u, n)| u.fuel_consumption().saturating_mul(*n) as f32 * distance)
                .sum::<f32>();

            (fuel * (1. - REACTOR_FUEL_REDUCTION_FACTOR * reactor)).ceil() as usize
        };
        travel_fuel.saturating_add(self.deep_cover_cost())
    }

    /// Returns the fuel charged when this player dispatches the mission.
    /// Protection fleets may always return to their home planet for free, including after
    /// permission is revoked. Other destinations use the ordinary travel cost.
    pub fn dispatch_fuel_consumption(&self, map: &Map, player: &Player) -> usize {
        if self.owner == player.id
            && self.objective == Icon::Deploy
            && self.destination == player.home_planet
            && self.origin_owned != Some(player.id)
            && self.origin_controlled != Some(player.id)
        {
            0
        } else {
            self.fuel_consumption(map)
        }
    }

    /// Returns the total number of units, saturating if their counts exceed the platform limit.
    pub fn total(&self) -> usize {
        self.army.values().copied().fold(0, usize::saturating_add)
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

        if self.joint_attack.is_some() {
            let remaining = self.turns_to_destination(map);
            if remaining <= 1 {
                return destination.position;
            }
            // Stretch each fleet's acceleration curve over the shared journey. With t elapsed
            // turns and n remaining, the remaining acceleration weights sum to n*(2*t+n+2).
            // This keeps short/fast routes in flight instead of parking them at the target.
            let elapsed = self.travel_turns as f64;
            let turns = remaining as f64;
            let fraction = (2.0 * elapsed + 3.0) / (turns * (2.0 * elapsed + turns + 2.0));
            let distance = self.distance(map);
            // Adjacent worlds can put the launch point inside the arrival margin already.
            // Still animate their short approach throughout the coordinated journey.
            let distance = if distance > 0.0 {
                distance * Planet::SIZE
            } else {
                self.position.distance(destination.position)
            };
            let step = distance * fraction as f32;
            return self.position
                + (destination.position - self.position).normalize_or_zero() * step;
        }

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
        if let Some(attack) = &self.joint_attack {
            return attack.arrival_turn.saturating_sub(self.send.saturating_add(self.travel_turns));
        }
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
        self.army.total_production()
    }

    /// One Energy per five fleet production, rounded up separately for each jump.
    pub fn jump_energy_cost(&self) -> usize {
        if self.jump_gate {
            self.jump_cost().div_ceil(5)
        } else {
            0
        }
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
        self.joint_attack = None;
    }

    /// Return the origin planet if still controlled by the player,
    /// else go to the nearest friendly planet
    pub fn check_origin(&self, map: &Map) -> PlanetId {
        let origin = map.get(self.origin);
        if origin.controlled == Some(self.owner) || origin.allows_protection(self.owner) {
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

    /// Returns the strongest in-range Sensor Phalanx on an owned endpoint of this mission.
    pub fn is_seen_by_phalanx(&self, map: &Map, player: &Player) -> Option<usize> {
        // Return legs resolve as Deploy, but spies and missiles retain their concealment.
        if self.objective.is_hidden() || self.return_objective.is_some_and(|icon| icon.is_hidden())
        {
            return None;
        }

        // Recall swaps the endpoints without moving the fleet. Both inbound and departing
        // missions remain detectable while their actual position is within endpoint coverage.
        // Every participant's departure world is also an endpoint of a coordinated attack, even
        // though each persisted contingent keeps only its own origin in `Mission::origin`.
        let coordinated_origin =
            self.joint_attack.as_ref().and_then(|attack| attack.origins.get(&player.id)).copied();
        [Some(self.origin), Some(self.destination), coordinated_origin]
            .into_iter()
            .flatten()
            .filter_map(|id| {
                let planet = map.get(id);
                let phalanx = planet.army.amount(&Unit::Building(Building::SensorPhalanx));
                (player.owns(planet)
                    && phalanx > 0
                    && PHALANX_DISTANCE * phalanx as f32 * Planet::SIZE + planet.size() * 0.5
                        >= planet.position.distance(self.position))
                .then_some(phalanx)
            })
            .max()
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
