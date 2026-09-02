//! Deterministic finite map generation and planet lookup helpers.

use bevy::prelude::*;
use itertools::Itertools;
use rand::prelude::IteratorRandom;
use rand::seq::index::sample;
use rand::{rng, Rng, RngExt};
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

        let mut moons = vec![false; n_total];
        for index in sample(rng, n_total, n_moons) {
            moons[index] = true;
        }

        // Determine map size based on number of planets
        let scale = 0.5 + (n_total as f32 / 60.).clamp(0., 2.) * 0.5;
        let mut rect = Rect::new(-WIDTH * scale, -HEIGHT * scale, WIDTH * scale, HEIGHT * scale);

        // Scatter worlds in continuous space; only enforce clearance, not fixed intervals.
        let positions = generate_positions(&mut rect, &moons, rng);

        // Compute total distance per world to the three closest planets (ignore moons).
        let mut sum_closest = Vec::with_capacity(positions.len());
        for (i, p) in positions.iter().enumerate() {
            sum_closest.push(
                positions
                    .iter()
                    .enumerate()
                    .filter_map(|(j, pos)| (j != i && !moons[j]).then_some(p.distance(*pos)))
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
                    Planet::new_with_rng(id, name.to_string(), pos, moons[id], f, rng)
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

/// Minimum center-to-center separation, leaving a visible gap beyond the sprite radii.
fn minimum_separation(left_is_moon: bool, right_is_moon: bool) -> f32 {
    let diameters = match (left_is_moon, right_is_moon) {
        (false, false) => 2.5,
        (true, true) => 1.25,
        _ => 1.5,
    };
    diameters * Planet::SIZE
}

/// Scatters worlds with hard pair-specific clearance, expanding crowded maps when needed.
fn generate_positions<R: Rng + ?Sized>(rect: &mut Rect, moons: &[bool], rng: &mut R) -> Vec<Vec2> {
    let mut positions: Vec<Vec2> = Vec::with_capacity(moons.len());
    for &is_moon in moons {
        let bounds = Rect::from_center_size(rect.center(), rect.size() * 0.9);
        let position = (0..512)
            .map(|_| random_position(bounds, rng))
            .find(|&candidate| {
                positions.iter().enumerate().all(|(other, position)| {
                    let clearance = minimum_separation(is_moon, moons[other]);
                    position.distance_squared(candidate) >= clearance * clearance
                })
            })
            .unwrap_or_else(|| {
                // All earlier worlds are inside these bounds. Placing beyond one edge by
                // more than the largest clearance is safe even if the random stream repeats.
                // This bounds generation work without ever weakening the separation rule.
                let mut candidate = random_position(bounds, rng);
                let clearance = minimum_separation(false, false) * rng.random_range(1.1..1.6);
                match rng.random_range(0..4) {
                    0 => candidate.x = bounds.min.x - clearance,
                    1 => candidate.x = bounds.max.x + clearance,
                    2 => candidate.y = bounds.min.y - clearance,
                    _ => candidate.y = bounds.max.y + clearance,
                }

                // Preserve the aspect ratio and leave a rounding margin inside the safe bounds.
                let required_half_size = (candidate - rect.center()).abs() + Vec2::ONE;
                let scale = (required_half_size / bounds.half_size()).max_element();
                *rect = Rect::from_center_size(rect.center(), rect.size() * scale);
                candidate
            });
        positions.push(position);
    }
    positions
}

/// Samples continuous coordinates, so distance limits do not introduce a placement grid.
fn random_position<R: Rng + ?Sized>(bounds: Rect, rng: &mut R) -> Vec2 {
    Vec2::new(
        rng.random_range(bounds.min.x..=bounds.max.x),
        rng.random_range(bounds.min.y..=bounds.max.y),
    )
}

#[cfg(test)]
#[path = "../../../tests/core/map_model.rs"]
mod tests;
