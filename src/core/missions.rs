use bevy::prelude::*;
use bevy::window::SystemCursorIcon;
use bevy_egui::egui::emath::OrderedFloat;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};
use strum_macros::EnumIter;

use crate::core::assets::WorldAssets;
use crate::core::constants::{MISSION_Z, PHALANX_DISTANCE, RADAR_DISTANCE};
use crate::core::map::icon::Icon;
use crate::core::map::map::{Map, MapCmp};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::systems::MissionCmp;
use crate::core::map::utils::cursor;
use crate::core::messages::MessageMsg;
use crate::core::player::Player;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::buildings::Building;
use crate::core::units::{Amount, Army, Combat, Description, Unit};
use crate::utils::NameFromEnum;

pub type MissionId = u64;

#[derive(Resource, Clone, Default, Serialize, Deserialize)]
pub struct Missions(pub Vec<Mission>);

impl Missions {
    pub fn get(&self, mission_id: MissionId) -> &Mission {
        self.0.iter().find(|m| m.id == mission_id).expect("Mission not found.")
    }

    pub fn iter(&self) -> std::slice::Iter<'_, Mission> {
        self.0.iter()
    }
}

#[derive(Message)]
pub struct SendMissionMsg {
    pub mission: Mission,
}

impl SendMissionMsg {
    pub fn new(mission: Mission) -> Self {
        Self {
            mission,
        }
    }
}

#[derive(EnumIter, Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub enum BombingRaid {
    #[default]
    None,
    Economic,
    Industrial,
}

impl Description for BombingRaid {
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
pub struct Mission {
    pub id: MissionId,
    pub owner: ClientId,
    pub origin: PlanetId,
    pub origin_owned: Option<ClientId>,
    pub origin_controlled: Option<ClientId>,
    pub origin_army: Army,
    pub destination: PlanetId,
    pub send: usize,
    pub position: Vec2,
    pub objective: Icon,
    pub army: Army,
    pub bombing: BombingRaid,
    pub combat_probes: bool,
    pub jump_gate: bool,
    pub logs: String,
}

impl Mission {
    pub fn new(
        turn: usize,
        owner: ClientId,
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
            id: rand::random(),
            owner,
            origin: origin.id,
            origin_owned: origin.owned,
            origin_controlled: origin.controlled,
            // Hide origin army if leaving an enemy planet (missions returning after attack)
            // Show when leaving a controlled or empty planet
            origin_army: if origin.controlled.map_or(true, |id| id == owner) {
                origin.army.clone()
            } else {
                Army::new()
            },
            destination: destination.id,
            send: turn,
            position: {
                // Start at the edge of the origin planet
                let direction = (-origin.position + destination.position).normalize();
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

    pub fn from_mission(
        turn: usize,
        owner: ClientId,
        origin: &Planet,
        destination: &Planet,
        mission: &Mission,
    ) -> Self {
        Self::new(
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

    pub fn image(&self, player: &Player) -> &str {
        match (self.owner == player.id, self.jump_gate) {
            (true, false) => "mission",
            (true, true) => "mission jump",
            (false, _) => "mission enemy",
        }
    }

    pub fn distance(&self, map: &Map) -> f32 {
        // Minus 0.7 since the mission ends at the edge of the planet
        (self.position.distance(map.get(self.destination).position) / Planet::SIZE - 0.7).max(0.)
    }

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

    pub fn duration(&self, map: &Map) -> usize {
        let distance = self.distance(map);
        let speed = self.speed();
        (speed != 0.).then(|| (distance / speed).ceil() as usize).unwrap_or(0)
    }

    pub fn fuel_consumption(&self, map: &Map) -> usize {
        if self.jump_gate {
            0
        } else {
            let distance = self.distance(map);
            self.army
                .iter()
                .map(|(u, n)| (u.fuel_consumption() * n) as f32 * distance)
                .sum::<f32>()
                .ceil() as usize
        }
    }

    pub fn total(&self) -> usize {
        self.army.values().sum()
    }

    pub fn advance(&mut self, map: &Map) {
        let destination = map.get(self.destination);

        if self.jump_gate {
            self.position = destination.position;
        } else {
            let direction = (-self.position + destination.position).normalize();
            self.position += direction * self.speed() * Planet::SIZE;
        }
    }

    pub fn turns_to_destination(&self, map: &Map) -> usize {
        (self.distance(map) / self.speed()).ceil() as usize
    }

    pub fn jump_cost(&self) -> usize {
        self.army.iter().map(|(u, c)| u.production() * c).sum()
    }

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
        self.objective =
            [self.objective, other.objective].into_iter().max_by_key(|o| o.priority()).unwrap();

        for (u, c) in &other.army {
            *self.army.entry(*u).or_default() += c;
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
                .min_by_key(|p| OrderedFloat(p.position.distance(self.position)))
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

pub fn update_missions(
    mut commands: Commands,
    mut mission_q: Query<(Entity, &mut Sprite, &mut Transform, &MissionCmp)>,
    state: Res<UiState>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    assets: Local<WorldAssets>,
) {
    let player_id = player.id;

    for mission in missions.iter() {
        if !mission_q.iter().any(|(_, _, _, m)| m.id == mission.id) {
            let id = mission.id;
            let owner = mission.owner;

            let destination = map.get(mission.destination);

            let direction = (-mission.position + destination.position).normalize();
            let angle = direction.y.atan2(direction.x);

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

pub fn send_mission(
    mut send_mission: MessageReader<SendMissionMsg>,
    mut message: MessageWriter<MessageMsg>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    mut missions: ResMut<Missions>,
) {
    for SendMissionMsg {
        mission,
    } in send_mission.read()
    {
        player.resources.deuterium -= mission.fuel_consumption(&map);

        let origin = map.get_mut(mission.origin);

        if mission.jump_gate {
            origin.jump_gate += mission.jump_cost();
        }

        // Subtract armies from the origin planet
        origin.army.iter_mut().for_each(|(u, c)| {
            *c -= mission.army.amount(u);
        });

        // Update control of the planet
        if !origin.has_fleet() && origin.owned != Some(player.id) && !origin.is_moon() {
            origin.controlled = None;
        }

        missions.0.push(mission.clone());

        message.write(MessageMsg::info("Mission sent."));
    }
}
