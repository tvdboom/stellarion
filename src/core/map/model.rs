//! Deterministic finite map generation and planet lookup helpers.

use bevy::prelude::*;
use itertools::Itertools;
use rand::prelude::IteratorRandom;
use rand::seq::index::sample;
use rand::{rng, Rng, RngExt};
use serde::{Deserialize, Serialize};

use crate::core::constants::{HEIGHT, PLANET_NAMES, SOLAR_STAR_SIZE, WIDTH};
use crate::core::map::planet::{Planet, PlanetId, SolarBand};

const SOLAR_SECTOR_CENTER_ANGLE: f32 = std::f32::consts::FRAC_PI_4;
const SOLAR_SECTOR_ANGLE: f32 = std::f32::consts::TAU / 3.0;
const SOLAR_SECTOR_START_ANGLE: f32 = SOLAR_SECTOR_CENTER_ANGLE - SOLAR_SECTOR_ANGLE * 0.5;
const SOLAR_SECTOR_END_ANGLE: f32 = SOLAR_SECTOR_CENTER_ANGLE + SOLAR_SECTOR_ANGLE * 0.5;
// The 120-degree arc extends this fraction of its radius behind each adjacent map edge.
const SOLAR_SECTOR_WING: f32 = 0.258_819_04;
const SOLAR_STAR_OUTSIDE_FACTOR: f32 = 0.06;
const SOLAR_SECTOR_MAP_SCALE: f32 = 1.08;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SolarCorner {
    BottomLeft,
    BottomRight,
    TopLeft,
    TopRight,
    WideBottomLeft,
    WideBottomRight,
    WideTopLeft,
    WideTopRight,
}

impl SolarCorner {
    fn from_index(index: usize) -> Self {
        match index % 4 {
            0 => Self::WideBottomLeft,
            1 => Self::WideBottomRight,
            2 => Self::WideTopLeft,
            _ => Self::WideTopRight,
        }
    }

    fn direction(self) -> Vec2 {
        match self {
            Self::BottomLeft | Self::WideBottomLeft => Vec2::new(-1.0, -1.0),
            Self::BottomRight | Self::WideBottomRight => Vec2::new(1.0, -1.0),
            Self::TopLeft | Self::WideTopLeft => Vec2::new(-1.0, 1.0),
            Self::TopRight | Self::WideTopRight => Vec2::ONE,
        }
    }

    fn uses_wide_arc(self) -> bool {
        matches!(
            self,
            Self::WideBottomLeft | Self::WideBottomRight | Self::WideTopLeft | Self::WideTopRight
        )
    }
}

#[derive(Component)]
/// Bevy component marking map presentation entities.
pub struct MapCmp;

#[derive(Resource, Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
/// Complete strategic map bounds and stable ordered planet collection.
pub struct Map {
    /// World-space bounds of the generated strategic map.
    pub rect: Rect,
    /// Corner containing the solar landmark and origin of the generated world arc.
    pub(crate) solar_corner: SolarCorner,
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

        let solar_corner = SolarCorner::from_index(rng.random_range(0..4));

        // Preserve the rectangular generator's area per world while presenting that area as a
        // broad annular sector. This keeps ordinary inter-world distance and mission-duration
        // scales stable without making the galaxy look like a rectangular scatter plot.
        let scale = 0.5 + (n_total as f32 / 60.).clamp(0., 2.) * 0.5;
        let mut rect = Rect::new(-WIDTH * scale, -HEIGHT * scale, WIDTH * scale, HEIGHT * scale);

        // Scatter worlds in continuous polar space; only enforce clearance, not fixed intervals.
        let positions = generate_positions(&mut rect, &moons, solar_corner, rng);

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

        let bands = solar_bands(&positions, &moons, solar_star_position(rect, solar_corner));
        let names = PLANET_NAMES.iter().sample(rng, n_total);
        Self {
            rect,
            solar_corner,
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

    /// Returns a planet from a map whose identifiers have already been validated.
    ///
    /// # Panics
    /// Panics if the identifier is missing. Use [`Self::try_get`] for external identifiers.
    pub fn get(&self, planet_id: PlanetId) -> &Planet {
        self.try_get(planet_id).unwrap_or_else(|| missing_validated_planet(planet_id))
    }

    /// Mutably borrows a planet from a map whose identifiers have already been validated.
    ///
    /// # Panics
    /// Panics if the identifier is missing. Use [`Self::try_get_mut`] for external identifiers.
    pub fn get_mut(&mut self, planet_id: PlanetId) -> &mut Planet {
        self.try_get_mut(planet_id).unwrap_or_else(|| missing_validated_planet(planet_id))
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
        self.solar_corner.direction()
    }

    /// Returns the authoritative center of the rendered primary star.
    pub fn solar_star_position(&self) -> Vec2 {
        solar_star_position(self.rect, self.solar_corner)
    }

    /// Returns the relative solar band for a non-moon planet.
    pub fn solar_band(&self, planet_id: PlanetId) -> Option<SolarBand> {
        let planet = self.try_get(planet_id)?;
        if planet.is_moon() {
            return None;
        }
        let star = self.solar_star_position();
        let distance = planet.position.distance(star);
        // Count the same stable rank used during generation without allocating/sorting all bands.
        let mut count = 0;
        let mut rank = 0;
        for (index, world) in self.planets.iter().enumerate().filter(|(_, world)| !world.is_moon())
        {
            count += 1;
            if world
                .position
                .distance(star)
                .total_cmp(&distance)
                .then_with(|| index.cmp(&planet_id))
                .is_lt()
            {
                rank += 1;
            }
        }
        Some(solar_band_for_rank(rank, count))
    }
}

/// Reports an internal identifier invariant shared by immutable and mutable map access.
fn missing_validated_planet(planet_id: PlanetId) -> ! {
    panic!("planet {planet_id} is missing from validated map state")
}

fn scenery_seed(positions: impl IntoIterator<Item = Vec2>) -> u32 {
    positions.into_iter().fold(0x915f_43b7_u32, |seed, position| {
        seed.rotate_left(7)
            ^ position.x.to_bits().wrapping_mul(0x9e37_79b9)
            ^ position.y.to_bits().rotate_left(13)
    })
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

fn solar_star_position(rect: Rect, corner: SolarCorner) -> Vec2 {
    let direction = corner.direction();
    let map_corner = map_corner(rect, direction);
    if !corner.uses_wide_arc() {
        return map_corner + direction * SOLAR_STAR_SIZE * SOLAR_STAR_OUTSIDE_FACTOR;
    }

    // Keep the star close to the selected corner while leaving room for the two short wings of
    // the 120-degree sector. The remaining map-scale padding is split evenly across both sides.
    let outer_radius = rect.width() / ((1.0 + SOLAR_SECTOR_WING) * SOLAR_SECTOR_MAP_SCALE);
    let inset = outer_radius
        * (SOLAR_SECTOR_WING + (SOLAR_SECTOR_MAP_SCALE - 1.0) * (1.0 + SOLAR_SECTOR_WING) * 0.5);
    map_corner - direction * inset
}

fn solar_bands(positions: &[Vec2], moons: &[bool], star: Vec2) -> Vec<Option<SolarBand>> {
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
        bands[index] = Some(solar_band_for_rank(rank, ordered.len()));
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

/// Shares the inner-quarter, middle-half, outer-quarter rule between generation and queries.
fn solar_band_for_rank(rank: usize, planet_count: usize) -> SolarBand {
    if rank < planet_count.div_ceil(4) {
        SolarBand::Inner
    } else if rank >= planet_count.saturating_sub(planet_count / 4) {
        SolarBand::Outer
    } else {
        SolarBand::Temperate
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

/// Scatters worlds through a broad solar arc with hard pair-specific clearance.
fn generate_positions<R: Rng + ?Sized>(
    rect: &mut Rect,
    moons: &[bool],
    solar_corner: SolarCorner,
    rng: &mut R,
) -> Vec<Vec2> {
    if moons.is_empty() {
        return Vec::new();
    }

    let center = rect.center();
    let target_area = rect.width() * 0.9 * rect.height() * 0.9;
    let inner_radius = SOLAR_STAR_SIZE * 0.5 + Planet::SIZE * 1.25;
    let mut outer_radius = (inner_radius.powi(2) + target_area * 2.0 / SOLAR_SECTOR_ANGLE).sqrt();
    let mut positions: Vec<Vec2> = Vec::with_capacity(moons.len());
    for &is_moon in moons {
        let position = (0..512)
            .map(|_| random_sector_position(inner_radius, outer_radius, rng))
            .find(|&candidate| {
                positions.iter().enumerate().all(|(other, position)| {
                    let clearance = minimum_separation(is_moon, moons[other]);
                    position.distance_squared(candidate) >= clearance * clearance
                        && route_clears_star(*position, candidate)
                })
            })
            .unwrap_or_else(|| {
                // A new outer shell is farther from every earlier point than the largest
                // possible clearance. This bounds generation without weakening separation.
                outer_radius += minimum_separation(false, false) + 1.0;
                Vec2::from_angle(SOLAR_SECTOR_CENTER_ANGLE) * outer_radius
            });
        positions.push(position);
    }

    let side = outer_radius * (1.0 + SOLAR_SECTOR_WING) * SOLAR_SECTOR_MAP_SCALE;
    *rect = Rect::from_center_size(center, Vec2::splat(side));
    let star = solar_star_position(*rect, solar_corner);
    let corner = solar_corner.direction();
    let inward = -corner;
    positions.into_iter().map(|position| star + position * inward).collect()
}

/// Keeps every straight inter-world route outside the visible body of the star.
fn route_clears_star(start: Vec2, end: Vec2) -> bool {
    let route = end - start;
    let progress = (-start.dot(route) / route.length_squared()).clamp(0.0, 1.0);
    let closest = start + route * progress;
    closest.length_squared() >= (SOLAR_STAR_SIZE * 0.5).powi(2)
}

/// Samples a continuous annular sector uniformly by area.
fn random_sector_position<R: Rng + ?Sized>(
    inner_radius: f32,
    outer_radius: f32,
    rng: &mut R,
) -> Vec2 {
    let angle = rng.random_range(SOLAR_SECTOR_START_ANGLE..=SOLAR_SECTOR_END_ANGLE);
    let radius_squared = rng.random_range(inner_radius.powi(2)..=outer_radius.powi(2));
    Vec2::from_angle(angle) * radius_squared.sqrt()
}

#[cfg(test)]
#[path = "../../../tests/core/map_model.rs"]
mod tests;
