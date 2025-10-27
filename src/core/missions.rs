use bevy::prelude::*;
use bevy::window::SystemCursorIcon;
use bevy_renet::renet::ClientId;
use serde::{Deserialize, Serialize};

use crate::core::assets::WorldAssets;
use crate::core::constants::MISSION_Z;
use crate::core::map::icon::Icon;
use crate::core::map::map::{Map, MapCmp};
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::map::systems::MissionCmp;
use crate::core::map::utils::cursor;
use crate::core::messages::MessageMsg;
use crate::core::player::Player;
use crate::core::ui::systems::{MissionTab, UiState};
use crate::core::units::{Army, Combat, Unit};

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

#[derive(Clone, Default, Serialize, Deserialize)]
pub struct Mission {
    pub id: MissionId,
    pub owner: ClientId,
    pub origin: PlanetId,
    pub destination: PlanetId,
    pub position: Vec2,
    pub objective: Icon,
    pub army: Army,
    pub probes_stay: bool,
    pub jump_gate: bool,
}

impl Mission {
    pub fn new(
        owner: ClientId,
        origin: &Planet,
        destination: &Planet,
        objective: Icon,
        army: Army,
        probes_stay: bool,
        jump_gate: bool,
    ) -> Self {
        Mission {
            id: rand::random(),
            owner,
            origin: origin.id,
            destination: destination.id,
            position: {
                // Start a bit outside the origin planet to be able to see the image
                let direction = (-origin.position + destination.position).normalize();
                origin.position + direction * Planet::SIZE
            },
            objective,
            army,
            probes_stay,
            jump_gate,
        }
    }

    pub fn image(&self, player: &Player) -> &str {
        match (self.owner == player.id, self.jump_gate) {
            (true, false) => "mission",
            (true, true) => "mission jump",
            (false, _) => "mission enemy",
        }
    }

    pub fn distance(&self, map: &Map) -> f32 {
        // Minus 1 since the mission ends a bit outside the destination planets
        self.position.distance(map.get(self.destination).position) / Planet::SIZE - 1.
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

    pub fn get(&self, unit: &Unit) -> usize {
        *self.army.get(unit).unwrap_or(&0)
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

    pub fn has_reached_destination(&self, map: &Map) -> bool {
        let destination = map.get(self.destination);
        self.position.distance(destination.position) <= Planet::SIZE
    }

    pub fn turns_to_destination(&self, map: &Map) -> usize {
        (self.distance(map) / self.speed()).ceil() as usize
    }

    pub fn jump_cost(&self) -> usize {
        self.army.iter().map(|(u, c)| u.production() * c).sum()
    }

    pub fn merge(&mut self, other: &mut Mission) {
        // Select objective based on priority
        self.objective =
            [self.objective, other.objective].into_iter().max_by_key(|o| o.priority()).unwrap();

        for (u, c) in &other.army {
            *self.army.entry(*u).or_default() += c;
        }

        self.probes_stay = other.probes_stay || self.probes_stay;
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

            let origin = map.get(mission.origin);
            let destination = map.get(mission.destination);

            let direction = (-origin.position + destination.position).normalize();
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
                        state.mission_tab = if owner == player_id {
                            MissionTab::ActiveMissions
                        } else {
                            MissionTab::IncomingAttacks
                        }
                    }
                });
        }
    }

    for (mission_e, mut mission_s, mut mission_t, mission_c) in &mut mission_q {
        if let Some(mission) = missions.iter().find(|m| m.id == mission_c.id) {
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

        missions.0.push(mission.clone());

        message.write(MessageMsg::info("Mission sent."));
    }
}
