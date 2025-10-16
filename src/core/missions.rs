use crate::core::assets::WorldAssets;
use crate::core::constants::MISSION_Z;
use crate::core::map::icon::Icon;
use crate::core::map::map::{Map, MapCmp};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::systems::MissionCmp;
use crate::core::messages::MessageMsg;
use crate::core::player::Player;
use crate::core::ui::systems::UiState;
use crate::core::units::{Army, Combat, Unit};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub type MissionId = u64;

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

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Mission {
    pub id: MissionId,
    pub origin: PlanetId,
    pub destination: PlanetId,
    pub position: Vec2,
    pub objective: Icon,
    pub army: Army,
}

impl Mission {
    pub fn from(other: &Mission) -> Self {
        Self {
            id: rand::random(),
            ..other.clone()
        }
    }

    pub fn distance(&self, map: &Map) -> f32 {
        // Minus 2 since the mission starts and ends a bit outside the planets
        self.position.distance(map.get(self.destination).position) / Planet::SIZE - 2.
    }

    pub fn speed(&self) -> f32 {
        self.army
            .iter()
            .filter_map(|(u, c)| (*c > 0).then_some(u.speed()))
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.)
    }

    pub fn duration(&self, map: &Map) -> usize {
        let distance = self.distance(map);
        let speed = self.speed();
        (speed != 0.).then(|| (distance / speed).ceil() as usize).unwrap_or(0)
    }

    pub fn fuel_consumption(&self, map: &Map) -> usize {
        let distance = self.distance(map);
        self.army
            .iter()
            .map(|(u, n)| (u.fuel_consumption() * n) as f32 * distance)
            .sum::<f32>()
            .ceil() as usize
    }

    pub fn get(&self, unit: &Unit) -> usize {
        *self.army.get(unit).unwrap_or(&0)
    }

    pub fn total(&self) -> usize {
        self.army.values().sum()
    }

    pub fn advance(&mut self, map: &Map) {
        let destination = map.get(self.destination);
        let direction = (-self.position + destination.position).normalize();

        self.position += direction * self.speed() * Planet::SIZE;
    }

    pub fn has_reached_destination(&self, map: &Map) -> bool {
        let destination = map.get(self.destination);
        self.position.distance(destination.position) <= Planet::SIZE
    }
}

pub fn update_mission(
    mut commands: Commands,
    mut mission_q: Query<(Entity, &mut Sprite, &mut Transform, &MissionCmp)>,
    state: Res<UiState>,
    player: Res<Player>,
    assets: Local<WorldAssets>,
) {
    for (mission_e, mut mission_s, mut mission_t, mission_c) in mission_q.iter_mut() {
        if let Some(mission) = player.missions.iter().find(|m| m.id == mission_c.id) {
            mission_t.translation = mission.position.extend(MISSION_Z);
            if state.mission_hover.is_some_and(|id| id == mission.id) {
                mission_s.image = assets.image("mission hover");
            } else {
                mission_s.image = assets.image("mission");
            }
        } else {
            commands.entity(mission_e).despawn();
        }
    }
}

pub fn send_mission_message(
    mut commands: Commands,
    mut send_mission: MessageReader<SendMissionMsg>,
    mut message: MessageWriter<MessageMsg>,
    mut map: ResMut<Map>,
    mut player: ResMut<Player>,
    assets: Local<WorldAssets>,
) {
    for SendMissionMsg {
        mission,
    } in send_mission.read()
    {
        let mut mission = mission.clone();
        let mission_id = mission.id;

        player.resources.deuterium -= mission.fuel_consumption(&map);

        let origin = map.get_mut(mission.origin);
        origin.fleet.iter_mut().for_each(|(s, c)| {
            if let Some(n) = mission.army.get(&Unit::Ship(s.clone())) {
                *c -= n;
            }
        });
        origin.battery.iter_mut().for_each(|(d, c)| {
            if let Some(n) = mission.army.get(&Unit::Defense(d.clone())) {
                *c -= n;
            }
        });

        let origin = map.get(mission.origin);
        let destination = map.get(mission.destination);

        let direction = (-origin.position + destination.position).normalize();
        let angle = direction.y.atan2(direction.x);

        mission.position += direction * Planet::SIZE; // Start a bit outside the planet
        player.missions.push(mission.clone());

        commands
            .spawn((
                Sprite {
                    image: assets.image("mission"),
                    custom_size: Some(Vec2::splat(50.)),
                    ..default()
                },
                Transform {
                    translation: mission.position.extend(MISSION_Z),
                    rotation: Quat::from_rotation_z(angle),
                    ..default()
                },
                Pickable::default(),
                MissionCmp::new(mission_id),
                MapCmp,
            ))
            .observe(move |_: On<Pointer<Over>>, mut state: ResMut<UiState>| {
                state.mission_hover = Some(mission_id);
            })
            .observe(|_: On<Pointer<Out>>, mut state: ResMut<UiState>| {
                state.mission_hover = None;
            });

        message.write(MessageMsg::info("Mission sent."));
    }
}
