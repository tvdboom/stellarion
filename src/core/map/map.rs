use std::ops::Range;
use crate::core::resources::Resources;
use bevy::prelude::*;
use rand::prelude::IteratorRandom;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};

#[derive(Component)]
pub struct MapCmp;

#[derive(Clone, Copy, Serialize, Deserialize)]
pub enum PlanetKind {
    Desert,
    Gas,
    Ice,
    Normal,
    Water,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Planet {
    pub kind: PlanetKind,
    pub image: String,
    pub resources: Resources,
    pub position: Vec2,
}

impl Planet {
    pub fn new() -> Self {
        let low = 0.0..3.0;
        let medium = 2.0..4.0;
        let high = 2.0..5.0;

        let configs: &[(PlanetKind, [&Range<f32>; 3])] = &[
            (PlanetKind::Desert, [&high, &medium, &low]),
            (PlanetKind::Gas,    [&low, &low, &high]),
            (PlanetKind::Ice,    [&medium, &high, &low]),
            (PlanetKind::Normal, [&medium, &medium, &low]),
            (PlanetKind::Water,  [&low, &medium, &medium]),
        ];

        let (kind, ranges) = configs.iter().choose(&mut rng()).unwrap();

        let resources = Resources::new(
            rng().random_range(ranges[0].clone()),
            rng().random_range(ranges[1].clone()),
            rng().random_range(ranges[2].clone()),
            0.,
        );

        let position = Vec2::new(
            rand::random::<f32>() * Map::SIZE,
            rand::random::<f32>() * Map::SIZE,
        );

        Self {
            kind: *kind,
            image: "water".into(),
            resources,
            position,
        }
    }
}

#[derive(Resource, Clone, Serialize, Deserialize)]
pub struct Map {
    pub planets: Vec<Planet>,
}

impl Map {
    pub const SIZE: f32 = 2000.;

    pub fn new(n_planets: u8) -> Self {
        Self {
            planets: (0..n_planets).map(|_| Planet::new()).collect(),
        }
    }
}
