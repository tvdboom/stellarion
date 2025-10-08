use crate::core::resources::Resources;
use crate::core::units::buildings::{Building, BuildingName};
use bevy::math::Vec2;
use bevy::prelude::Component;
use rand::prelude::IteratorRandom;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use std::ops::Range;

pub type PlanetId = usize;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub enum PlanetKind {
    Desert,
    Gas,
    Ice,
    Normal,
}

impl PlanetKind {
    pub fn indices(self) -> Vec<usize> {
        match self {
            PlanetKind::Desert => vec![1, 2, 4, 7, 8, 11, 12, 14, 15, 18, 19, 20],
            PlanetKind::Gas => vec![6, 9, 10, 13, 17, 36, 42, 44],
            PlanetKind::Ice => vec![0, 3, 5, 16, 21, 22, 25, 26, 27, 34, 35, 37, 39],
            PlanetKind::Normal => vec![24, 31, 33, 51, 61],
        }
    }
}

#[derive(Component, Clone, Debug, Serialize, Deserialize)]
pub struct Planet {
    pub id: PlanetId,
    pub name: String,
    pub kind: PlanetKind,
    pub image: usize,
    pub position: Vec2,
    pub resources: Resources,
    pub is_destroyed: bool,
    pub buildings: Vec<Building>,
}

impl Planet {
    // Pixel size of a planet on the screen
    pub const SIZE: f32 = 100.;

    pub fn new(id: PlanetId, name: String, position: Vec2) -> Self {
        let low = 0..3;
        let medium = 2..4;
        let high = 2..5;

        let configs: &[(PlanetKind, [&Range<usize>; 3])] = &[
            (PlanetKind::Desert, [&high, &low, &low]),
            (PlanetKind::Gas, [&low, &low, &high]),
            (PlanetKind::Ice, [&low, &high, &low]),
            (PlanetKind::Normal, [&medium, &medium, &low]),
        ];

        let (kind, ranges) = configs.iter().choose(&mut rng()).unwrap();

        let resources = Resources::new(
            rng().random_range(ranges[0].clone()) * 100,
            rng().random_range(ranges[1].clone()) * 100,
            rng().random_range(ranges[2].clone()) * 100,
        );

        Self {
            id,
            name,
            kind: *kind,
            image: *kind.indices().iter().choose(&mut rng()).unwrap(),
            position,
            resources,
            is_destroyed: false,
            buildings: vec![],
        }
    }

    pub fn make_home_planet(&mut self) {
        self.resources = Resources::new(200, 200, 100);
        self.buildings = vec![
            Building::new(BuildingName::Mine),
            Building::new(BuildingName::Shipyard),
            Building::new(BuildingName::Factory),
        ];
    }
}
