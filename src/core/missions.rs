use crate::core::map::icon::Icon;
use crate::core::map::map::Map;
use crate::core::map::planet::{Planet, PlanetId};
use crate::core::units::{Army, Combat};
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
        self.position.distance(map.get(self.destination).position) / Planet::SIZE
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
}
