use bevy::prelude::*;
use itertools::Itertools;
use rand::prelude::IteratorRandom;
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};

use crate::core::constants::{HEIGHT, MAX_PLANETS, MIN_PLANETS, PLANET_NAMES, WIDTH};
use crate::core::map::planet::{Planet, PlanetId};

#[derive(Component)]
pub struct MapCmp;

#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
pub struct Map {
    pub rect: Rect,
    pub planets: Vec<Planet>,
}

impl Map {
    pub fn new(n_planets: usize) -> Self {
        // Determine map size based on number of planets
        let scale = 0.5
            + ((n_planets as f32 - 10.) / (MAX_PLANETS - MIN_PLANETS) as f32).clamp(0., 1.)
                * (1. - 0.5);
        let rect = Rect::new(-WIDTH * scale, -HEIGHT * scale, WIDTH * scale, HEIGHT * scale);

        // Determine positions for planets
        let mut positions: Vec<Vec2> = Vec::new();
        while positions.len() < n_planets {
            let candidate = Vec2::new(
                rng().random_range(rect.min.x * 0.9..rect.max.x * 0.9),
                rng().random_range(rect.min.y * 0.9..rect.max.y * 0.9),
            );

            if positions.iter().all(|&pos| pos.distance(candidate) > 2. * Planet::SIZE) {
                positions.push(candidate);
            }
        }

        // Compute total distance per planet to the three closest planets
        let mut sum_closest = Vec::with_capacity(positions.len());
        for (i, p) in positions.iter().enumerate() {
            sum_closest.push(
                positions
                    .iter()
                    .enumerate()
                    .filter_map(|(j, pos)| (j != i).then_some(p.distance(*pos)))
                    .sorted_by(|a, b| b.partial_cmp(a).unwrap())
                    .take(4)
                    .sum::<f32>(),
            );
        }

        // Normalize totals and compute the resource factor for every planet
        let mean = sum_closest.iter().sum::<f32>() / sum_closest.len() as f32;
        let max_dev = sum_closest.iter().map(|&x| (x - mean).abs()).fold(0.0, f32::max).max(1e-6);
        let factors = sum_closest
            .iter()
            .map(|td| (1. + (td - mean) / max_dev).clamp(1., 2.))
            .collect::<Vec<_>>();

        let names = PLANET_NAMES.iter().choose_multiple(&mut rng(), n_planets);
        Self {
            rect,
            planets: names
                .iter()
                .zip(positions)
                .zip(factors)
                .enumerate()
                .map(|(id, ((name, pos), f))| Planet::new(id, name.to_string(), pos, f))
                .collect(),
        }
    }

    pub fn get(&self, planet_id: PlanetId) -> &Planet {
        self.planets.iter().find(|p| p.id == planet_id).expect("Planet not found.")
    }

    pub fn get_mut(&mut self, planet_id: PlanetId) -> &mut Planet {
        self.planets.iter_mut().find(|p| p.id == planet_id).expect("Planet not found.")
    }
}
