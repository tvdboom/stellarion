use crate::core::constants::{HEIGHT, MAX_PLANETS, MIN_PLANETS, WIDTH};
use crate::core::resources::Resources;
use bevy::prelude::*;
use rand::prelude::IteratorRandom;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};
use std::ops::Range;

#[derive(Component)]
pub struct MapCmp;

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
    pub kind: PlanetKind,
    pub image: usize,
    pub resources: Resources,
    pub position: Vec2,
}

impl Planet {
    // Pixel size of a planet on the screen
    pub const SIZE: f32 = 100.;

    pub fn new(position: Vec2) -> Self {
        let low = 0.0..3.0;
        let medium = 2.0..4.0;
        let high = 2.0..5.0;

        let configs: &[(PlanetKind, [&Range<f32>; 3])] = &[
            (PlanetKind::Desert, [&high, &low, &low]),
            (PlanetKind::Gas, [&low, &low, &high]),
            (PlanetKind::Ice, [&low, &high, &low]),
            (PlanetKind::Normal, [&medium, &medium, &low]),
        ];

        let (kind, ranges) = configs.iter().choose(&mut rng()).unwrap();

        let resources = Resources::new(
            rng().random_range(ranges[0].clone()),
            rng().random_range(ranges[1].clone()),
            rng().random_range(ranges[2].clone()),
            0.,
        );

        Self {
            kind: *kind,
            image: *kind.indices().iter().choose(&mut rng()).unwrap(),
            resources,
            position,
        }
    }
}

#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
pub struct Map {
    pub rect: Rect,
    pub planets: Vec<Planet>,
}

impl Map {
    pub fn new(n_planets: u8) -> Self {
        // Determine map size based on number of planets
        let scale = 0.5
            + ((n_planets as f32 - 10.) / (MAX_PLANETS - MIN_PLANETS) as f32).clamp(0., 1.)
                * (1. - 0.5);
        let rect = Rect::new(
            -WIDTH * scale,
            -HEIGHT * scale,
            WIDTH * scale,
            HEIGHT * scale,
        );

        // Determine positions for planets
        let mut positions: Vec<Vec2> = Vec::new();
        while positions.len() < n_planets as usize {
            let candidate = Vec2::new(
                rng().random_range(rect.min.x * 0.9..rect.max.x * 0.9),
                rng().random_range(rect.min.y * 0.9..rect.max.y * 0.9),
            );

            if positions
                .iter()
                .all(|&pos| pos.distance(candidate) > 2. * Planet::SIZE)
            {
                positions.push(candidate);
            }
        }

        Self {
            rect,
            planets: positions.iter().map(|pos| Planet::new(*pos)).collect(),
        }
    }
}
