//! Deterministic asteroid-field placement shared by simulation and map presentation.

use std::collections::BTreeMap;
use std::f32::consts::TAU;

use bevy::math::Vec2;

use super::model::Map;
use super::planet::{Planet, PlanetId, SolarBand};
use crate::core::constants::SOLAR_STAR_SIZE;

#[derive(Clone, Copy, Debug)]
pub(crate) struct AsteroidBeltGap {
    pub(crate) between_bands: usize,
    pub(crate) inner_center: f32,
    pub(crate) outer_center: f32,
    pub(crate) inner_edge: f32,
    pub(crate) outer_edge: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AsteroidBeltLayout {
    pub(crate) between_bands: usize,
    pub(crate) radius: f32,
    pub(crate) radial_half_width: f32,
    pub(crate) maximum_asteroid_diameter: f32,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct AsteroidBeltPlacement {
    #[cfg(feature = "app")]
    pub(crate) seed: u32,
    pub(crate) phase: f32,
    pub(crate) radius: f32,
    pub(crate) diameter: f32,
}

pub(crate) const ASTEROID_PLANET_CLEARANCE: f32 = 12.0;
pub(crate) const ASTEROID_BELT_RADIUS_SAMPLES: usize = 12;
pub(crate) const ASTEROID_BELT_TARGET_SPACING: f32 = 34.0;
pub(crate) const ASTEROID_BELT_MINIMUM_COUNT: usize = 96;
pub(crate) const ASTEROID_BELT_MAXIMUM_COUNT: usize = 420;
pub(crate) const ASTEROID_BELT_MINIMUM_VISIBLE_COUNT: usize = 32;
pub(crate) const ASTEROID_BELT_VISIBLE_SUBDIVISIONS: usize = 8;
/// Smallest physical rock diameter used by placement and Recycler reach calculations.
pub(crate) const ASTEROID_MINIMUM_DIAMETER: f32 = 18.0;
pub(crate) const ASTEROID_MAXIMUM_RADIUS: f32 = 15.0;
pub(crate) const ASTEROID_MINIMUM_WOBBLE: f32 = 4.0;
pub(crate) const ASTEROID_WOBBLE_RANGE: f32 = 6.0;
pub(crate) const ASTEROID_MAXIMUM_WOBBLE: f32 = ASTEROID_MINIMUM_WOBBLE + ASTEROID_WOBBLE_RANGE;
/// Slow counter-clockwise orbit of the asteroid belt, in radians per second.
#[cfg(feature = "app")]
pub(crate) const ASTEROID_BELT_ANGULAR_SPEED: f32 = 0.0045;

/// Recycler reach beyond a planet's and asteroid's visible surfaces, measured in AU.
pub(crate) const RECYCLER_ASTEROID_REACH_AU: f32 = 2.25;
/// Preferred minimum flight distance beyond both visible surfaces, measured in AU.
pub(crate) const RECYCLER_ASTEROID_MINIMUM_TRAVEL_AU: f32 = 0.75;
/// Nearby rocks rotated between a planet's level-based Recycler workers.
pub(crate) const RECYCLER_ASTEROID_TARGET_COUNT: usize = 4;

pub(crate) fn solar_band_gaps(map: &Map) -> Vec<AsteroidBeltGap> {
    let star = map.solar_star_position();
    let mut bands = [Vec::new(), Vec::new(), Vec::new()];
    for planet in map.planets() {
        let index = match map.solar_band(planet.id) {
            Some(SolarBand::Inner) => 0,
            Some(SolarBand::Temperate) => 1,
            Some(SolarBand::Outer) => 2,
            None => continue,
        };
        bands[index].push((planet.position.distance(star), planet.size() * 0.5));
    }
    [(0, 1), (1, 2)]
        .into_iter()
        .enumerate()
        .filter_map(|(between_bands, (near, far))| {
            let inner_center =
                bands[near].iter().map(|(distance, _)| *distance).reduce(f32::max)?;
            let outer_center = bands[far].iter().map(|(distance, _)| *distance).reduce(f32::min)?;
            let inner_edge =
                bands[near].iter().map(|(distance, radius)| distance + radius).reduce(f32::max)?;
            let outer_edge =
                bands[far].iter().map(|(distance, radius)| distance - radius).reduce(f32::min)?;
            (inner_center < outer_center).then_some(AsteroidBeltGap {
                between_bands,
                inner_center,
                outer_center,
                inner_edge,
                outer_edge,
            })
        })
        .collect()
}

fn asteroid_belt_layout_in_gap(gap: AsteroidBeltGap, radial_position: f32) -> AsteroidBeltLayout {
    let surface_width = gap.outer_edge - gap.inner_edge;
    let (safe_inner, safe_outer, maximum_asteroid_radius) = if surface_width > 0.0 {
        let maximum_asteroid_radius = ASTEROID_MAXIMUM_RADIUS.min(surface_width * 0.2);
        let planet_padding = 8.0_f32.min(surface_width * 0.08);
        (
            gap.inner_edge + maximum_asteroid_radius + planet_padding,
            gap.outer_edge - maximum_asteroid_radius - planet_padding,
            maximum_asteroid_radius,
        )
    } else {
        let center_width = gap.outer_center - gap.inner_center;
        (
            gap.inner_center + center_width * 0.12,
            gap.outer_center - center_width * 0.12,
            ASTEROID_MAXIMUM_RADIUS,
        )
    };
    let safe_width = safe_outer - safe_inner;
    let radial_half_width = (safe_width * 0.32).min(50.0);
    let minimum_radius = safe_inner + radial_half_width;
    let maximum_radius = safe_outer - radial_half_width;
    AsteroidBeltLayout {
        between_bands: gap.between_bands,
        radius: minimum_radius + (maximum_radius - minimum_radius) * radial_position,
        radial_half_width,
        maximum_asteroid_diameter: maximum_asteroid_radius * 2.0,
    }
}

fn visual_noise(mut value: u32) -> f32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^= value >> 16;
    value as f32 / u32::MAX as f32
}

pub(crate) fn asteroid_is_visible_on_map(map: &Map, placement: &AsteroidBeltPlacement) -> bool {
    let center = map.solar_star_position();
    let position = center + Vec2::from_angle(placement.phase) * placement.radius;
    let visibility_margin = placement.diameter * 0.5 + ASTEROID_MAXIMUM_WOBBLE;
    let clear_of_star = placement.radius - visibility_margin > SOLAR_STAR_SIZE * 0.5;
    let fully_on_map = position.x - visibility_margin >= map.rect.min.x
        && position.x + visibility_margin <= map.rect.max.x
        && position.y - visibility_margin >= map.rect.min.y
        && position.y + visibility_margin <= map.rect.max.y;
    clear_of_star && fully_on_map
}

pub(crate) fn visible_asteroid_count(map: &Map, placements: &[AsteroidBeltPlacement]) -> usize {
    placements.iter().filter(|placement| asteroid_is_visible_on_map(map, placement)).count()
}

/// Selects the one stable decorative/gameplay asteroid field for this map.
pub(crate) fn asteroid_belt_layout(map: &Map) -> Option<AsteroidBeltLayout> {
    let seed = map.scenery_seed().wrapping_add(0x7a21_6d4b);
    let gaps = solar_band_gaps(map);
    let clear_gaps =
        gaps.iter().copied().filter(|gap| gap.inner_edge < gap.outer_edge).collect::<Vec<_>>();
    let candidates = if clear_gaps.is_empty() {
        gaps
    } else {
        clear_gaps
    };
    let radial_offset = visual_noise(seed.wrapping_add(1));
    candidates
        .into_iter()
        .flat_map(|gap| {
            (0..ASTEROID_BELT_RADIUS_SAMPLES).map(move |sample| {
                let radial_position =
                    (radial_offset + sample as f32 / ASTEROID_BELT_RADIUS_SAMPLES as f32).fract();
                asteroid_belt_layout_in_gap(gap, radial_position)
            })
        })
        .enumerate()
        .max_by_key(|(candidate, layout)| {
            let placements = asteroid_belt_base_placements(map, *layout);
            let complete = placements.len() == asteroid_belt_asteroid_count(layout.radius);
            (
                visible_asteroid_count(map, &placements),
                complete,
                visual_noise(seed.wrapping_add(*candidate as u32 + 2)).to_bits(),
            )
        })
        .map(|(_, layout)| layout)
}

pub(crate) fn asteroid_belt_asteroid_count(radius: f32) -> usize {
    ((TAU * radius / ASTEROID_BELT_TARGET_SPACING).round() as usize)
        .clamp(ASTEROID_BELT_MINIMUM_COUNT, ASTEROID_BELT_MAXIMUM_COUNT)
}

struct AsteroidBeltPlacementGenerator {
    center: Vec2,
    planets: Vec<(Vec2, f32)>,
    layout: AsteroidBeltLayout,
    count: usize,
    belt_seed: u32,
    starting_angle: f32,
    search_increment: f32,
}

impl AsteroidBeltPlacementGenerator {
    fn new(map: &Map, layout: AsteroidBeltLayout) -> Self {
        let count = asteroid_belt_asteroid_count(layout.radius);
        let belt_seed = map
            .scenery_seed()
            .wrapping_add(0x2c91_7a4d)
            .wrapping_add((layout.between_bands as u32).wrapping_mul(0x51d7_34ab));
        Self {
            center: map.solar_star_position(),
            planets: map
                .planets()
                .into_iter()
                .map(|planet| (planet.position, planet.size() * 0.5))
                .collect(),
            layout,
            count,
            belt_seed,
            starting_angle: visual_noise(belt_seed) * TAU,
            search_increment: TAU / count as f32 * 0.25,
        }
    }

    fn placement(&self, sequence: usize, angular_index: f32) -> Option<AsteroidBeltPlacement> {
        let seed = self.belt_seed.wrapping_add((sequence as u32).wrapping_mul(31));
        let base_phase = self.starting_angle
            + angular_index / self.count as f32 * TAU
            + (visual_noise(seed) - 0.5) * TAU / self.count as f32 * 0.7;
        let radius = self.layout.radius
            + (visual_noise(seed.wrapping_add(1)) - 0.5) * self.layout.radial_half_width * 2.0;
        let minimum_diameter = ASTEROID_MINIMUM_DIAMETER.min(self.layout.maximum_asteroid_diameter);
        let diameter = minimum_diameter
            + visual_noise(seed.wrapping_add(2))
                * (self.layout.maximum_asteroid_diameter - minimum_diameter);
        let phase = (0..=self.count * 4).find_map(|search| {
            let offset = match search {
                0 => 0.0,
                value if value % 2 == 1 => (value / 2 + 1) as f32,
                value => -((value / 2) as f32),
            };
            let phase = base_phase + offset * self.search_increment;
            let position = self.center + Vec2::from_angle(phase) * radius;
            self.planets
                .iter()
                .all(|(planet_position, planet_radius)| {
                    position.distance(*planet_position)
                        > planet_radius
                            + diameter * 0.5
                            + ASTEROID_PLANET_CLEARANCE
                            + ASTEROID_MAXIMUM_WOBBLE
                })
                .then_some(phase)
        })?;
        Some(AsteroidBeltPlacement {
            #[cfg(feature = "app")]
            seed,
            phase,
            radius,
            diameter,
        })
    }

    fn base_placements(&self) -> Vec<AsteroidBeltPlacement> {
        (0..self.count).filter_map(|index| self.placement(index, index as f32)).collect()
    }
}

pub(crate) fn asteroid_belt_base_placements(
    map: &Map,
    layout: AsteroidBeltLayout,
) -> Vec<AsteroidBeltPlacement> {
    AsteroidBeltPlacementGenerator::new(map, layout).base_placements()
}

pub(crate) fn asteroid_belt_placements(
    map: &Map,
    layout: AsteroidBeltLayout,
) -> Vec<AsteroidBeltPlacement> {
    let generator = AsteroidBeltPlacementGenerator::new(map, layout);
    let mut placements = generator.base_placements();
    let mut visible = visible_asteroid_count(map, &placements);
    if visible >= ASTEROID_BELT_MINIMUM_VISIBLE_COUNT {
        return placements;
    }

    'supplements: for subdivision in 1..ASTEROID_BELT_VISIBLE_SUBDIVISIONS {
        for index in 0..generator.count {
            let sequence = generator.count + (subdivision - 1) * generator.count + index;
            let angular_index =
                index as f32 + subdivision as f32 / ASTEROID_BELT_VISIBLE_SUBDIVISIONS as f32;
            let Some(placement) = generator.placement(sequence, angular_index) else {
                continue;
            };
            if asteroid_is_visible_on_map(map, &placement) {
                placements.push(placement);
                visible += 1;
                if visible >= ASTEROID_BELT_MINIMUM_VISIBLE_COUNT {
                    break 'supplements;
                }
            }
        }
    }
    placements
}

fn recycler_targets_from_positions(
    map: &Map,
    asteroids: &[(Vec2, f32)],
) -> BTreeMap<PlanetId, Vec<Vec2>> {
    map.planets()
        .into_iter()
        .filter_map(|planet| {
            let mut targets = asteroids
                .iter()
                .map(|(position, radius)| {
                    let surface_gap = (planet.position.distance(*position)
                        - (planet.size() * 0.5 + radius))
                        .max(0.0);
                    (*position, surface_gap)
                })
                .filter(|(_, gap)| *gap <= Planet::SIZE * RECYCLER_ASTEROID_REACH_AU)
                .collect::<Vec<_>>();
            let preferred_gap = Planet::SIZE * RECYCLER_ASTEROID_MINIMUM_TRAVEL_AU;
            // Prefer the nearest rocks in the farther working band. Close rocks remain a fallback
            // so unusual map shapes never disable an otherwise reachable Recycler.
            targets.sort_by(|left, right| {
                (left.1 < preferred_gap)
                    .cmp(&(right.1 < preferred_gap))
                    .then_with(|| left.1.total_cmp(&right.1))
            });
            let targets = targets
                .into_iter()
                .take(RECYCLER_ASTEROID_TARGET_COUNT)
                .map(|(position, _)| position)
                .collect::<Vec<_>>();
            (!targets.is_empty()).then_some((planet.id, targets))
        })
        .collect()
}

/// Returns a rock's current presentation position, including its small radial wobble.
#[cfg(feature = "app")]
pub(crate) fn asteroid_position_at_elapsed(
    map: &Map,
    placement: &AsteroidBeltPlacement,
    elapsed: f32,
) -> Vec2 {
    let angle = placement.phase + elapsed * ASTEROID_BELT_ANGULAR_SPEED;
    let wobble_phase = visual_noise(placement.seed.wrapping_add(6)) * TAU;
    let wobble_speed = 0.35 + visual_noise(placement.seed.wrapping_add(7)) * 0.5;
    let wobble_amplitude = ASTEROID_MINIMUM_WOBBLE
        + visual_noise(placement.seed.wrapping_add(8)) * ASTEROID_WOBBLE_RANGE;
    let radius =
        placement.radius + (elapsed * wobble_speed + wobble_phase).sin() * wobble_amplitude;
    map.solar_star_position() + Vec2::from_angle(angle) * radius
}

/// Returns the nearest currently reachable rocks for each planet, ordered by surface distance.
/// This advances only presentation coordinates; canonical planet positions remain unchanged.
#[cfg(feature = "app")]
pub(crate) fn recycler_asteroid_target_groups_at_elapsed(
    map: &Map,
    placements: &[AsteroidBeltPlacement],
    elapsed: f32,
) -> BTreeMap<PlanetId, Vec<Vec2>> {
    let asteroids = placements
        .iter()
        .map(|placement| {
            (asteroid_position_at_elapsed(map, placement, elapsed), placement.diameter * 0.5)
        })
        .collect::<Vec<_>>();
    recycler_targets_from_positions(map, &asteroids)
}

/// Returns the nearest reachable rock centers at the belt's initial presentation phase.
pub(crate) fn recycler_asteroid_target_groups(map: &Map) -> BTreeMap<PlanetId, Vec<Vec2>> {
    let Some(layout) = asteroid_belt_layout(map) else {
        return BTreeMap::new();
    };
    let placements = asteroid_belt_placements(map, layout);
    let center = map.solar_star_position();
    let asteroids = placements
        .iter()
        .map(|placement| {
            (
                center + Vec2::from_angle(placement.phase) * placement.radius,
                placement.diameter * 0.5,
            )
        })
        .collect::<Vec<_>>();
    recycler_targets_from_positions(map, &asteroids)
}

/// Returns the nearest reachable rock center used by authoritative Recycler production.
pub(crate) fn recycler_asteroid_targets(map: &Map) -> BTreeMap<PlanetId, Vec2> {
    recycler_asteroid_target_groups(map)
        .into_iter()
        .filter_map(|(planet, targets)| targets.first().copied().map(|target| (planet, target)))
        .collect()
}
