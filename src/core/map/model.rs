//! Deterministic finite map generation and planet lookup helpers.

use bevy::prelude::*;
use itertools::Itertools;
use rand::prelude::IteratorRandom;
use rand::seq::{index::sample, SliceRandom};
use rand::{rng, Rng};
use serde::{Deserialize, Serialize};

use crate::core::constants::{HEIGHT, PLANET_NAMES, WIDTH};
use crate::core::map::planet::{Planet, PlanetId};

#[derive(Component)]
/// Bevy component marking map presentation entities.
pub struct MapCmp;

#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
/// Complete strategic map bounds and stable ordered planet collection.
pub struct Map {
    /// World-space bounds of the generated strategic map.
    pub rect: Rect,
    /// All planets and moons in stable ID order.
    pub planets: Vec<Planet>,
}

impl Map {
    /// Generates a map using process randomness for a standalone local game.
    pub fn new(n_planets: usize, p_moons: usize) -> Self {
        Self::new_with_rng(n_planets, p_moons, &mut rng())
    }

    /// Generates a map from the supplied deterministic random stream.
    pub fn new_with_rng<R: Rng + ?Sized>(n_planets: usize, p_moons: usize, rng: &mut R) -> Self {
        let n_moons = (n_planets as f32 * p_moons as f32 / 100.) as usize;
        let n_total = n_planets + n_moons;

        let moon_idx: Vec<usize> = sample(rng, n_total, n_moons).into_iter().collect();

        // Determine map size based on number of planets
        let scale = 0.5 + (n_total as f32 / 60.).clamp(0., 2.) * 0.5;
        let rect = Rect::new(-WIDTH * scale, -HEIGHT * scale, WIDTH * scale, HEIGHT * scale);

        // A rejection sampler can become permanently jammed once the map is dense. A shuffled
        // hex lattice provides the same visual separation with a strict, finite upper bound.
        let positions = generate_positions(rect, n_total, rng);

        // Compute total distance per world to the three closest planets (ignore moons).
        let mut sum_closest = Vec::with_capacity(positions.len());
        for (i, p) in positions.iter().enumerate() {
            sum_closest.push(
                positions
                    .iter()
                    .enumerate()
                    .filter_map(|(j, pos)| {
                        (j != i && !moon_idx.contains(&j)).then_some(p.distance(*pos))
                    })
                    .sorted_by(f32::total_cmp)
                    .take(3)
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

        let names = PLANET_NAMES.iter().sample(rng, n_total);
        Self {
            rect,
            planets: names
                .iter()
                .zip(positions)
                .zip(factors)
                .enumerate()
                .map(|(id, ((name, pos), f))| {
                    Planet::new_with_rng(id, name.to_string(), pos, moon_idx.contains(&id), f, rng)
                })
                .collect(),
        }
    }

    /// Returns state for the requested stable identifier.
    pub fn get(&self, planet_id: PlanetId) -> &Planet {
        self.try_get(planet_id)
            .unwrap_or_else(|| panic!("planet {planet_id} is missing from validated map state"))
    }

    /// Returns mutable state for the requested stable identifier.
    pub fn get_mut(&mut self, planet_id: PlanetId) -> &mut Planet {
        self.try_get_mut(planet_id)
            .unwrap_or_else(|| panic!("planet {planet_id} is missing from validated map state"))
    }

    /// Looks up a planet without assuming the caller already validated its identifier.
    pub fn try_get(&self, planet_id: PlanetId) -> Option<&Planet> {
        self.planets.get(planet_id).filter(|planet| planet.id == planet_id)
    }

    /// Looks up a mutable planet without assuming the caller already validated its identifier.
    pub fn try_get_mut(&mut self, planet_id: PlanetId) -> Option<&mut Planet> {
        self.planets.get_mut(planet_id).filter(|planet| planet.id == planet_id)
    }

    /// Returns non-moon planets in stable map order.
    pub fn planets(&self) -> Vec<&Planet> {
        self.planets.iter().filter(|p| !p.is_moon()).collect()
    }

    /// Returns moon entries in stable map order.
    pub fn moons(&self) -> Vec<&Planet> {
        self.planets.iter().filter(|p| p.is_moon()).collect()
    }
}

/// Generates a finite, deterministic set of well-separated map positions.
fn generate_positions<R: Rng + ?Sized>(rect: Rect, count: usize, rng: &mut R) -> Vec<Vec2> {
    if count == 0 {
        return Vec::new();
    }

    let mut spacing = 2.75 * Planet::SIZE;
    for _ in 0..64 {
        let mut candidates = hex_lattice(rect, spacing);
        if candidates.len() >= count {
            candidates.shuffle(rng);
            candidates.truncate(count);
            return candidates;
        }
        spacing *= 0.9;
    }

    // This is reachable only for callers far outside GameRules' validated limits. It still
    // terminates and fills the requested map instead of hanging indefinitely.
    let columns = (count as f32).sqrt().ceil() as usize;
    let rows = count.div_ceil(columns);
    let min = rect.min * 0.9;
    let max = rect.max * 0.9;
    let x_step = if columns > 1 {
        (max.x - min.x) / (columns - 1) as f32
    } else {
        0.0
    };
    let y_step = if rows > 1 {
        (max.y - min.y) / (rows - 1) as f32
    } else {
        0.0
    };
    let mut positions = (0..count)
        .map(|index| {
            Vec2::new(
                min.x + (index % columns) as f32 * x_step,
                min.y + (index / columns) as f32 * y_step,
            )
        })
        .collect::<Vec<_>>();
    positions.shuffle(rng);
    positions
}

/// Builds all points in a staggered hexagonal lattice inside the map's safe bounds.
fn hex_lattice(rect: Rect, spacing: f32) -> Vec<Vec2> {
    let min = rect.min * 0.9;
    let max = rect.max * 0.9;
    let row_spacing = spacing * 3.0_f32.sqrt() * 0.5;
    let mut positions = Vec::new();
    let mut row = 0_usize;
    let mut y = min.y;
    while y <= max.y {
        let mut x = min.x
            + if row.is_multiple_of(2) {
                0.0
            } else {
                spacing * 0.5
            };
        while x <= max.x {
            positions.push(Vec2::new(x, y));
            x += spacing;
        }
        row += 1;
        y += row_spacing;
    }
    positions
}

#[cfg(test)]
mod tests {
    use rand::SeedableRng;
    use rand_chacha::ChaCha8Rng;

    use super::*;

    /// Ensures the largest supported map is generated with the intended minimum separation.
    #[test]
    fn maximum_supported_map_has_stable_spacing() {
        let mut rng = ChaCha8Rng::seed_from_u64(42);
        let map = Map::new_with_rng(80, 100, &mut rng);
        assert_eq!(map.planets.len(), 160);
        for (index, planet) in map.planets.iter().enumerate() {
            for other in &map.planets[index + 1..] {
                assert!(planet.position.distance(other.position) > 2.5 * Planet::SIZE);
            }
        }
    }
}
