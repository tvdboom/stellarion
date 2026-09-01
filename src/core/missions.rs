//! Persisted fleet missions, movement calculations, visibility, and Bevy adapters.

#[cfg(feature = "app")]
use std::f32::consts::PI;
#[cfg(feature = "app")]
use std::time::Duration;

use bevy::prelude::*;
#[cfg(feature = "app")]
use bevy::window::SystemCursorIcon;
#[cfg(feature = "app")]
use bevy_tweening::{RepeatCount, Tween, TweenAnim};
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

#[cfg(feature = "app")]
use crate::core::assets::WorldAssets;
#[cfg(feature = "app")]
use crate::core::constants::MISSION_Z;
use crate::core::constants::{NEXUS_FACTOR, PHALANX_DISTANCE, RADAR_DISTANCE};
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::model::Map;
#[cfg(feature = "app")]
use crate::core::map::model::MapCmp;
use crate::core::map::planet::{Planet, PlanetId};
#[cfg(feature = "app")]
use crate::core::map::systems::MissionCmp;
#[cfg(feature = "app")]
use crate::core::map::utils::{cursor, SpriteFrameLens};
#[cfg(feature = "app")]
use crate::core::messages::MessageMsg;
use crate::core::player::Player;
#[cfg(feature = "app")]
use crate::core::simulation::TurnCommand;
#[cfg(feature = "app")]
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Combat, Description, Unit};
#[cfg(feature = "app")]
use crate::multiplayer::client::{MultiplayerSession, PendingTurnCommands};
use crate::utils::NameFromEnum;

/// Stable identifier of a persisted fleet mission.
pub type MissionId = u64;

#[cfg(feature = "app")]
const MISSION_ROUTE_CHEVRONS: usize = 7;

#[cfg(feature = "app")]
#[derive(Component)]
/// One animated chevron in the hovered mission's destination route.
pub struct MissionRouteArrowCmp {
    index: usize,
}

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

    /// Returns the runtime image key for this value.
    pub fn image(&self, player: &Player) -> &str {
        match (self.owner == player.id, self.jump_gate) {
            (true, false) => "mission",
            (true, true) => "mission jump",
            (false, _) => "mission enemy",
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

#[cfg(feature = "app")]
/// Advances the visible ECS mission projection after canonical turn installation.
pub fn update_missions(
    mut commands: Commands,
    mut mission_q: Query<(Entity, &mut Sprite, &mut Transform, &MissionCmp)>,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    assets: Res<WorldAssets>,
) {
    let player_id = player.id;

    for mission in missions.iter() {
        if !mission_q.iter().any(|(_, _, _, m)| m.id == mission.id) {
            let id = mission.id;
            let owner = mission.owner;

            let destination = map.get(mission.destination);

            let direction = (-mission.position + destination.position).normalize();
            let angle = direction.y.atan2(direction.x);

            let texture = assets.texture("flame");
            commands
                .spawn((
                    Sprite {
                        image: assets.image(mission.image(&player)),
                        custom_size: Some(Vec2::splat(50.)),
                        ..default()
                    },
                    Transform {
                        translation: mission.position.extend(MISSION_Z),
                        rotation: Quat::from_rotation_z(angle),
                        ..default()
                    },
                    Pickable::default(),
                    MissionCmp::new(id),
                    MapCmp,
                    children![(
                        Sprite::from_atlas_image(texture.image, texture.atlas),
                        Transform {
                            translation: Vec3::new(-25., 0., -0.1),
                            scale: Vec3::splat(0.35),
                            rotation: Quat::from_rotation_z(PI),
                        },
                        TweenAnim::new(
                            Tween::new(
                                EaseFunction::Linear,
                                Duration::from_millis(1000),
                                SpriteFrameLens(texture.last_index),
                            )
                            .with_repeat_count(RepeatCount::Infinite),
                        ),
                    )],
                ))
                .observe(cursor::<Over>(SystemCursorIcon::Pointer))
                .observe(cursor::<Out>(SystemCursorIcon::Default))
                .observe(move |_: On<Pointer<Over>>, mut state: ResMut<UiState>| {
                    state.mission_hover = Some(id);
                })
                .observe(|_: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                    state.mission_hover = None;
                })
                .observe(move |event: On<Pointer<Click>>, mut state: ResMut<UiState>| {
                    if event.button == PointerButton::Primary {
                        state.mission = true;
                        state.planet_selected = None;
                        state.mission_tab = if owner == player_id {
                            MissionTab::ActiveMissions
                        } else {
                            MissionTab::EnemyMissions
                        }
                    }
                });
        }
    }

    for (mission_e, mut mission_s, mut mission_t, mission_c) in &mut mission_q {
        if let Some(mission) = missions.iter().find(|m| m.id == mission_c.id) {
            // Update the direction the image is pointing at
            // Could change if the destination planet was destroyed
            let destination = map.get(mission.destination);

            let direction = (-mission.position + destination.position).normalize();
            let angle = direction.y.atan2(direction.x);

            mission_t.rotation = Quat::from_rotation_z(angle);

            if state.mission_hover.is_some_and(|id| id == mission.id) {
                // Hovered missions show on top of all other components (e.g., planets)
                mission_t.translation = mission.position.extend(MISSION_Z + 10.);
                mission_s.image = assets.image(format!("{} hover", mission.image(&player)));
            } else {
                mission_t.translation = mission.position.extend(MISSION_Z);
                mission_s.image = assets.image(mission.image(&player));
            }
        } else {
            commands.entity(mission_e).despawn();
        }
    }
}

#[cfg(feature = "app")]
/// Animates a directional trail from the hovered mission to its destination planet.
pub fn update_mission_route_arrow(
    mut commands: Commands,
    mut arrow_q: Query<(Entity, &mut Transform, &mut TextColor, &MissionRouteArrowCmp)>,
    state: Res<UiState>,
    map: Res<Map>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    assets: Res<WorldAssets>,
    time: Res<Time>,
) {
    let Some(mission) = state.mission_hover.and_then(|id| missions.get(id)) else {
        for (entity, _, _, _) in &mut arrow_q {
            commands.entity(entity).despawn();
        }
        return;
    };

    let destination = map.get(mission.destination);
    let route = destination.position - mission.position;
    let direction = route.normalize_or_zero();
    let start = mission.position + direction * 38.0;
    let end = destination.position - direction * destination.size() * 0.7;
    if start.distance_squared(end) < 24.0 * 24.0 {
        for (entity, _, _, _) in &mut arrow_q {
            commands.entity(entity).despawn();
        }
        return;
    }

    let angle = direction.y.atan2(direction.x);
    let base_phase = (time.elapsed_secs_wrapped() * 0.42).fract();
    let route_color = session.player_color(mission.owner).color();
    let mut present = [false; MISSION_ROUTE_CHEVRONS];

    for (entity, mut transform, mut text_color, arrow) in &mut arrow_q {
        if arrow.index >= MISSION_ROUTE_CHEVRONS {
            commands.entity(entity).despawn();
            continue;
        }
        present[arrow.index] = true;
        let progress = (base_phase + arrow.index as f32 / MISSION_ROUTE_CHEVRONS as f32).fract();
        let alpha = (std::f32::consts::PI * progress).sin().clamp(0.18, 1.0);
        transform.translation = start.lerp(end, progress).extend(MISSION_Z + 8.0);
        transform.rotation = Quat::from_rotation_z(angle);
        transform.scale = Vec3::splat(0.82 + alpha * 0.18);
        text_color.0 = route_color.with_alpha(alpha);
    }

    for (index, is_present) in present.into_iter().enumerate() {
        if is_present {
            continue;
        }
        let progress = (base_phase + index as f32 / MISSION_ROUTE_CHEVRONS as f32).fract();
        let alpha = (std::f32::consts::PI * progress).sin().clamp(0.18, 1.0);
        commands.spawn((
            Text2d::new(">"),
            TextFont {
                font: assets.font("bold").into(),
                font_size: 28.0.into(),
                ..default()
            },
            TextColor(route_color.with_alpha(alpha)),
            Transform {
                translation: start.lerp(end, progress).extend(MISSION_Z + 8.0),
                rotation: Quat::from_rotation_z(angle),
                scale: Vec3::splat(0.82 + alpha * 0.18),
            },
            Pickable::IGNORE,
            MissionRouteArrowCmp {
                index,
            },
            MapCmp,
        ));
    }
}

#[cfg(feature = "app")]
/// Validates the selected mission UI, removes committed units/resources, and emits its command.
pub fn send_mission(
    mut send_mission: MessageReader<SendMissionMsg>,
    mut message: MessageWriter<MessageMsg>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut missions: ResMut<Missions>,
    mut pending: ResMut<PendingTurnCommands>,
) {
    for SendMissionMsg {
        mission,
    } in send_mission.read()
    {
        let army = mission
            .army
            .iter()
            .filter(|(_, count)| **count > 0)
            .map(|(unit, count)| (*unit, *count))
            .collect::<Army>();
        if !pending.push(TurnCommand::SendMission {
            mission_id: mission.id,
            origin: mission.origin,
            destination: mission.destination,
            objective: mission.objective,
            army,
            bombing: mission.bombing.clone(),
            combat_probes: mission.combat_probes,
            jump_gate: mission.jump_gate,
        }) {
            message.write(MessageMsg::error(
                "This turn already contains the maximum number of commands.",
            ));
            continue;
        }
        player.resources.deuterium =
            player.resources.deuterium.saturating_sub(mission.fuel_consumption(&map));

        let origin = map.get_mut(mission.origin);

        if mission.jump_gate {
            origin.jump_gate = origin.jump_gate.saturating_add(mission.jump_cost());
        }

        // Subtract armies from the origin planet
        origin.army.iter_mut().for_each(|(u, c)| {
            *c = c.saturating_sub(mission.army.amount(u));
        });

        // Update control of the planet
        if !origin.has_fleet() && origin.owned != Some(player.id) && !origin.is_moon() {
            origin.controlled = None;
        }

        missions.0.push(mission.clone());

        message.write(MessageMsg::info("Mission sent."));
    }
}
