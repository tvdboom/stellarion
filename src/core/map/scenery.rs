//! Stable celestial landmarks shared by simulation, asset loading, and map presentation.

use bevy::prelude::Vec2;

use super::model::Map;

pub(crate) const CELESTIAL_SIZE: Vec2 = Vec2::new(480.0, 270.0);
pub(crate) const CELESTIAL_MAP_MARGIN: f32 = 8.0;

/// One compact looping animation, independent of authoritative game state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CelestialKind {
    BlackHole,
    NeutronStar,
    Magnetar,
}

impl CelestialKind {
    #[cfg(feature = "app")]
    pub(crate) const ALL: [Self; 3] = [Self::BlackHole, Self::NeutronStar, Self::Magnetar];

    #[cfg(feature = "app")]
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::BlackHole => "black hole",
            Self::NeutronStar => "neutron star",
            Self::Magnetar => "magnetar",
        }
    }

    #[cfg(feature = "app")]
    pub(crate) fn frame_count(self) -> usize {
        match self {
            Self::BlackHole => 66,
            Self::NeutronStar | Self::Magnetar => 48,
        }
    }

    #[cfg(feature = "app")]
    pub(crate) fn frame_seconds(self) -> f32 {
        match self {
            Self::BlackHole => 0.08,
            Self::NeutronStar => 0.375,
            Self::Magnetar => 0.125,
        }
    }

    /// Keeps corner landmarks compact enough to sit clear of nearby planets.
    pub(crate) fn size_scale(self) -> f32 {
        match self {
            Self::BlackHole => 0.55,
            Self::NeutronStar => 0.6,
            Self::Magnetar => 1.0,
        }
    }

    #[cfg(feature = "app")]
    pub(crate) fn opacity(self) -> f32 {
        match self {
            Self::BlackHole => 0.7,
            Self::NeutronStar => 0.85,
            Self::Magnetar => 0.75,
        }
    }
}

/// One compact landmark accompanies the large solar arc in every game.
pub(crate) fn map_scenery_selection(map: &Map) -> CelestialKind {
    let seed = map.scenery_seed();
    match (seed >> 8) % 3 {
        0 => CelestialKind::NeutronStar,
        1 => CelestialKind::Magnetar,
        _ => CelestialKind::BlackHole,
    }
}

fn celestial_planet_clearance(map: &Map, position: Vec2, size: Vec2) -> f32 {
    map.planets
        .iter()
        .map(|planet| {
            let gap = (planet.position - position).abs() - size * 0.5;
            gap.max(Vec2::ZERO).length() - planet.size() * 0.5
        })
        .fold(f32::INFINITY, f32::min)
}

pub(crate) fn celestial_position(map: &Map, kind: CelestialKind) -> Vec2 {
    let sun_corner = map.solar_corner();
    let size = CELESTIAL_SIZE * kind.size_scale();
    let edge_direction = Vec2::new(-sun_corner.x, 0.0);
    let edge_x = if edge_direction.x < 0.0 {
        map.rect.min.x
    } else {
        map.rect.max.x
    };
    // Keep the complete landmark on the map opposite the large sun. Very small synthetic maps
    // cannot contain the full sprite, so clamp their center to the inner tenth of the bounds.
    let horizontal_inset = (size.x * 0.5 + CELESTIAL_MAP_MARGIN).min(map.rect.half_size().x * 0.9);
    let x = edge_x - edge_direction.x * horizontal_inset;
    let usable_half_height =
        (map.rect.half_size().y - size.y * 0.5 - CELESTIAL_MAP_MARGIN).max(0.0);
    let corner = |side: f32| Vec2::new(x, map.rect.center().y + usable_half_height * side);
    let clearance = |candidate| celestial_planet_clearance(map, candidate, size);
    let first = corner(sun_corner.y);
    let second = corner(-sun_corner.y);
    if kind != CelestialKind::Magnetar {
        return if clearance(first) >= clearance(second) {
            first
        } else {
            second
        };
    }

    // The wider magnetar may need an opening between corners. Score the full sprite footprint,
    // not just its center, and prefer a corner when several placements are equally clear.
    [sun_corner.y, -sun_corner.y, -0.5, 0.0, 0.5]
        .into_iter()
        .map(corner)
        .reduce(|best, candidate| {
            if clearance(candidate) > clearance(best) {
                candidate
            } else {
                best
            }
        })
        .unwrap_or(first)
}
