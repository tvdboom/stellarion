use crate::core::constants::{HEIGHT, MAX_PLANETS, MIN_PLANETS, PLANET_NAMES, WIDTH};
use crate::core::map::planet::Planet;
use bevy::prelude::*;
use rand::prelude::IteratorRandom;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};

#[derive(Component)]
pub struct MapCmp;

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
        let rect = Rect::new(-WIDTH * scale, -HEIGHT * scale, WIDTH * scale, HEIGHT * scale);

        // Determine positions for planets
        let mut positions: Vec<Vec2> = Vec::new();
        while positions.len() < n_planets as usize {
            let candidate = Vec2::new(
                rng().random_range(rect.min.x * 0.9..rect.max.x * 0.9),
                rng().random_range(rect.min.y * 0.9..rect.max.y * 0.9),
            );

            if positions.iter().all(|&pos| pos.distance(candidate) > 2. * Planet::SIZE) {
                positions.push(candidate);
            }
        }

        let names = PLANET_NAMES.iter().cloned().choose_multiple(&mut rng(), n_planets as usize);

        Self {
            rect,
            planets: names
                .iter()
                .zip(positions)
                .enumerate()
                .map(|(id, (name, pos))| Planet::new(id, name.to_string(), pos))
                .collect(),
        }
    }
}
