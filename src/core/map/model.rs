//! Deterministic finite map generation and planet lookup helpers.

use bevy::prelude::*;
use itertools::Itertools;
use rand::prelude::IteratorRandom;
use rand::seq::index::sample;
use rand::{rng, Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::core::constants::{HEIGHT, PLANET_NAMES, SOLAR_STAR_SIZE, WIDTH};
use crate::core::map::planet::{Planet, PlanetId, SolarBand};

#[derive(Component)]
/// Bevy component marking map presentation entities.
pub struct MapCmp;

#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
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

        let bands = solar_bands(rect, &positions, &moons);
        let names = PLANET_NAMES.iter().sample(rng, n_total);
        Self {
            rect,
            planets: names
                .iter()
                .zip(positions)
                .zip(factors)
                .zip(bands)
                .enumerate()
                .map(|(id, (((name, pos), f), band))| {
                    Planet::new_in_solar_band_with_rng(
                        id,
                        name.to_string(),
                        pos,
                        moons[id],
                        f,
                        band,
                        rng,
                    )
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

    /// Returns a stable seed derived only from generated world coordinates.
    pub fn scenery_seed(&self) -> u32 {
        scenery_seed(self.planets.iter().map(|planet| planet.position))
    }

    /// Returns the map corner occupied by the primary solar landmark.
    pub fn solar_corner(&self) -> Vec2 {
        solar_corner(self.scenery_seed())
    }

    /// Returns the authoritative center of the rendered primary star.
    pub fn solar_star_position(&self) -> Vec2 {
        let corner = self.solar_corner();
        map_corner(self.rect, corner) + corner * SOLAR_STAR_SIZE * 0.3
    }

    /// Returns the relative solar band for a non-moon planet.
    pub fn solar_band(&self, planet_id: PlanetId) -> Option<SolarBand> {
        if self.try_get(planet_id).is_none_or(Planet::is_moon) {
            return None;
        }
        let moons = self.planets.iter().map(Planet::is_moon).collect::<Vec<_>>();
        solar_bands(
            self.rect,
            &self.planets.iter().map(|planet| planet.position).collect::<Vec<_>>(),
            &moons,
        )
        .get(planet_id)
        .copied()
        .flatten()
    }
}

fn scenery_seed(positions: impl IntoIterator<Item = Vec2>) -> u32 {
    positions.into_iter().fold(0x915f_43b7_u32, |seed, position| {
        seed.rotate_left(7)
            ^ position.x.to_bits().wrapping_mul(0x9e37_79b9)
            ^ position.y.to_bits().rotate_left(13)
    })
}

fn solar_corner(seed: u32) -> Vec2 {
    match seed & 3 {
        0 => Vec2::new(-1.0, -1.0),
        1 => Vec2::new(1.0, -1.0),
        2 => Vec2::new(-1.0, 1.0),
        _ => Vec2::ONE,
    }
}

fn map_corner(rect: Rect, direction: Vec2) -> Vec2 {
    Vec2::new(
        if direction.x < 0.0 {
            rect.min.x
        } else {
            rect.max.x
        },
        if direction.y < 0.0 {
            rect.min.y
        } else {
            rect.max.y
        },
    )
}

fn solar_bands(rect: Rect, positions: &[Vec2], moons: &[bool]) -> Vec<Option<SolarBand>> {
    let seed = scenery_seed(positions.iter().copied());
    let corner = solar_corner(seed);
    let star = map_corner(rect, corner) + corner * SOLAR_STAR_SIZE * 0.3;
    let mut ordered = positions
        .iter()
        .enumerate()
        .filter_map(|(index, position)| (!moons[index]).then_some((index, position.distance(star))))
        .collect::<Vec<_>>();
    ordered.sort_by(|left, right| left.1.total_cmp(&right.1).then_with(|| left.0.cmp(&right.0)));

    let inner_count = ordered.len().div_ceil(4);
    let outer_start = ordered.len().saturating_sub(ordered.len() / 4);
    let mut bands = vec![None; positions.len()];
    for (rank, &(index, _)) in ordered.iter().enumerate() {
        bands[index] = Some(if rank < inner_count {
            SolarBand::Inner
        } else if rank >= outer_start {
            SolarBand::Outer
        } else {
            SolarBand::Temperate
        });
    }

    // Planet ranks establish the same radial boundaries shown on the map. Classify
    // moons by their actual stellar distance so their airless surface temperature
    // follows where they are located without changing planet-only band gameplay.
    let boundary = |split: usize| {
        ordered
            .get(split.saturating_sub(1))
            .zip(ordered.get(split))
            .map(|(near, far)| (near.1 + far.1) * 0.5)
    };
    let inner_boundary = boundary(inner_count);
    let outer_boundary = boundary(outer_start);
    for (index, position) in positions.iter().enumerate().filter(|(index, _)| moons[*index]) {
        let distance = position.distance(star);
        bands[index] = Some(match (inner_boundary, outer_boundary) {
            (Some(inner), _) if distance < inner => SolarBand::Inner,
            (_, Some(outer)) if distance >= outer => SolarBand::Outer,
            _ => SolarBand::Temperate,
        });
    }
    bands
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
