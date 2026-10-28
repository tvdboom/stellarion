//! Cached surface staging and soft environmental light for the recorded battle.

use super::*;

pub(super) fn atmosphere_color(kind: PlanetKind) -> Color32 {
    match kind {
        PlanetKind::Dry | PlanetKind::Yellow => Color32::from_rgb(231, 184, 119),
        PlanetKind::Gas | PlanetKind::Brown => Color32::from_rgb(186, 157, 121),
        PlanetKind::Ice | PlanetKind::Blue => Color32::from_rgb(130, 205, 245),
        PlanetKind::Metallic | PlanetKind::Gray => Color32::from_rgb(161, 181, 208),
        PlanetKind::Water => Color32::from_rgb(93, 167, 248),
        PlanetKind::Red => Color32::from_rgb(235, 137, 100),
    }
}

/// Far-side emplacements recede gently; the front terrace retains the authored unit scale.
pub(super) fn surface_perspective(home: Vec2) -> f32 {
    (0.86 + (home.y + 0.65) * 0.18).clamp(0.84, 1.06)
}

pub(super) fn scorches_surface(shot: &CinematicShot) -> bool {
    !shot.outcome.missed
        && (shot.outcome.planetary_shield_damage == 0 || shot.outcome.hull_damage > 0)
        && (shot.outcome.hull_damage > 0 || shot.outcome.killed)
}

impl CinematicPlayback {
    /// Persistent marks come only from recorded penetrating surface hits. They are sampled
    /// at impact so a passing repair truck cannot drag a crater around the planet afterward.
    pub(super) fn paint_surface_damage(&self, painter: &Painter, scene: Scene) {
        if !self.show_planet
            || self.gas_planet
            || self
                .timeline
                .planet_attacks
                .iter()
                .any(|attack| attack.destroyed && self.elapsed >= attack.end_at)
        {
            return;
        }
        for &index in &self.draw_order {
            let visual = &self.visuals[index];
            if !visual.ground {
                continue;
            }
            let end = visual
                .surface_hits
                .partition_point(|shot| self.timeline.shots[*shot].impact_at <= self.elapsed);
            // Keep the last few contacts only; dense bombardments must not make an unbounded
            // pile of decals or repeatedly traverse an entire battle's shot history.
            for &shot_index in visual.surface_hits[..end].iter().rev().take(2) {
                let shot = &self.timeline.shots[shot_index];
                if !scorches_surface(shot) {
                    continue;
                }
                let age = self.elapsed - shot.impact_at;
                let pose = self.actor_flight_pose(scene, index, shot.impact_at);
                let seed = shot_index as u32;
                let center = pose.center + vec2((noise(seed + 13) - 0.5) * 0.22, 0.24) * pose.size;
                let radius = pose.size * (0.30 + noise(seed + 29) * 0.10);
                let reveal = smooth(age / 0.24);
                soft_ellipse(
                    painter,
                    center,
                    vec2(radius, radius * 0.38),
                    Color32::from_rgb(15, 13, 17),
                    0.68 * reveal,
                );
                soft_ellipse(
                    painter,
                    center + vec2(-0.06, -0.04) * radius,
                    vec2(radius * 0.52, radius * 0.15),
                    Color32::from_rgb(147, 131, 115),
                    0.20 * reveal,
                );
                if !(0.0..1.8).contains(&age) {
                    continue;
                }
                // Brief terrain-colored ejecta rise and disperse over the actual hit. No
                // smoke is invented for shield-only contacts or the cloud deck of gas worlds.
                for puff in 0..5_u32 {
                    let phase = noise(seed.wrapping_mul(31).wrapping_add(puff * 17));
                    let elapsed = (age - phase * 0.12).max(0.0);
                    let life = (elapsed / (1.25 + phase * 0.5)).clamp(0.0, 1.0);
                    let drift =
                        vec2((phase - 0.5) * 0.70, -0.18 - phase * 0.30) * pose.size * elapsed;
                    let scale = radius * (0.10 + life * 0.65);
                    soft_ellipse(
                        painter,
                        center + drift,
                        vec2(scale, scale * 0.7),
                        Color32::from_rgb(157, 142, 120),
                        (PI * life).sin().max(0.0) * 0.16,
                    );
                }
            }
        }
    }
}

/// Crawlers keep a small patrol lane that shrinks along with crowded surface formations.
pub(super) fn crawler_patrol_extent(size: f32) -> Vec2 {
    vec2(18.0, 5.0) * (size / unit_size(Unit::crawler())).clamp(0.0, 1.0)
}

/// Pack by hull footprint instead of a fixed grid: expensive guns must not overlap their
/// neighbors just because their class happens to occupy the end of the recorded roster.
pub(super) fn layout_surface(
    visuals: &mut [ActorVisual],
    actors: &[CinematicActor],
    settlement: bool,
) {
    let mut indices: Vec<_> = visuals
        .iter()
        .enumerate()
        .filter(|(index, visual)| {
            visual.ground && visual.visible && actors[*index].initial_levels.is_none()
        })
        .map(|(index, _)| index)
        .collect();
    if indices.is_empty() {
        return;
    }
    // Stable individual phases break up identical rows without rerolling the scene on seek.
    indices.sort_by(|a, b| visuals[*a].phase.total_cmp(&visuals[*b].phase));
    let columns = (indices.len() as f32 * 1.45).sqrt().ceil().max(1.0) as usize;
    let rows = indices.len().div_ceil(columns);
    let radius = 820.0 * 0.32;
    let gap = 9.0;
    let lane = |index: usize, size: f32| {
        if actors[index].unit == Unit::crawler() {
            crawler_patrol_extent(size) * 2.0
        } else {
            Vec2::ZERO
        }
    };
    let row_widths: Vec<f32> = indices
        .chunks(columns)
        .map(|row| {
            row.iter()
                .map(|index| {
                    let size = visuals[*index].size;
                    size * 0.90 + gap + lane(*index, size).x
                })
                .sum()
        })
        .collect();
    let row_heights: Vec<f32> = indices
        .chunks(columns)
        .map(|row| {
            row.iter()
                .map(|index| {
                    let size = visuals[*index].size;
                    size * 0.76 + lane(*index, size).y
                })
                .fold(0.0, f32::max)
                + gap
        })
        .collect();
    let height: f32 = row_heights.iter().sum();
    let available_height = radius
        * if settlement {
            0.67
        } else {
            1.12
        };
    let fit = (radius * 1.42 / row_widths.iter().copied().fold(1.0, f32::max))
        .min(available_height / height.max(1.0))
        .min(1.0);
    let top = if settlement {
        -0.66
    } else {
        -0.65
    };
    let mut y = top;
    for (row_index, row) in indices.chunks(columns).enumerate() {
        let row_height = row_heights[row_index] * fit / radius;
        let center_y = if rows == 1 {
            -0.25
        } else {
            y + row_height * 0.5
        };
        let width = row_widths[row_index] * fit / radius;
        let stagger = if row_index % 2 == 0 {
            -0.025
        } else {
            0.025
        };
        let mut x = -width * 0.5 - 0.04 + stagger;
        for &index in row {
            let visual = &mut visuals[index];
            visual.size *= fit;
            let cell = (visual.size * 0.90 + gap * fit + lane(index, visual.size).x) / radius;
            let center_x = x + cell * 0.5;
            visual.home =
                vec2(center_x, center_y + center_x * center_x * 0.12 + visual.phase.sin() * 0.014);
            x += cell;
        }
        y += row_height;
    }
}

/// Feathered contact shadows anchor pedestals without dark, hard-edged oval stickers.
pub(super) fn paint_surface_shadow(painter: &Painter, pose: ActorPose) {
    soft_ellipse(
        painter,
        pose.center + vec2(0.10, 0.28) * pose.size,
        vec2(0.48, 0.16) * pose.size,
        Color32::BLACK,
        0.40,
    );
    soft_ellipse(
        painter,
        pose.center + vec2(0.0, 0.25) * pose.size,
        vec2(0.27, 0.07) * pose.size,
        Color32::BLACK,
        0.58,
    );
}

pub(super) fn soft_ellipse(
    painter: &Painter,
    center: Pos2,
    radii: Vec2,
    color: Color32,
    opacity: f32,
) {
    const SEGMENTS: u32 = 32;
    let mut mesh = Mesh::default();
    for (radius, strength) in [(0.0, 1.0), (0.38, 0.70), (0.72, 0.22), (1.0, 0.0)] {
        for step in 0..=SEGMENTS {
            mesh.colored_vertex(
                center + Vec2::angled(step as f32 / SEGMENTS as f32 * TAU) * radii * radius,
                alpha(color, opacity * strength),
            );
        }
    }
    for ring in 0..3 {
        for step in 0..SEGMENTS {
            let a = ring * (SEGMENTS + 1) + step;
            let b = a + SEGMENTS + 1;
            mesh.add_triangle(a, a + 1, b);
            mesh.add_triangle(a + 1, b + 1, b);
        }
    }
    painter.add(mesh);
}

pub(super) fn paint_atmosphere(painter: &Painter, center: Pos2, radius: f32, color: Color32) {
    const SEGMENTS: u32 = 128;
    let mut mesh = Mesh::default();
    for (scale, opacity) in
        [(0.992, 0.0), (1.002, 0.40), (1.012, 0.19), (1.035, 0.055), (1.08, 0.0)]
    {
        for step in 0..=SEGMENTS {
            let angle = step as f32 / SEGMENTS as f32 * TAU;
            // The planet artwork is lit from the upper left; its rim shares that light.
            let light = 0.16 + 0.84 * (0.5 + 0.5 * (angle + 2.35).cos()).powi(2);
            mesh.colored_vertex(
                center + Vec2::angled(angle) * radius * scale,
                alpha(color, opacity * light),
            );
        }
    }
    for ring in 0..4 {
        for step in 0..SEGMENTS {
            let a = ring * (SEGMENTS + 1) + step;
            let b = a + SEGMENTS + 1;
            mesh.add_triangle(a, a + 1, b);
            mesh.add_triangle(a + 1, b + 1, b);
        }
    }
    painter.add(mesh);
}

pub(super) fn paint_vignette(painter: &Painter, rect: Rect) {
    // One low-density mesh avoids visible bands while keeping the battle's center clear.
    const COLUMNS: u32 = 16;
    const ROWS: u32 = 12;
    let mut mesh = Mesh::default();
    for row in 0..=ROWS {
        for column in 0..=COLUMNS {
            let uv = vec2(column as f32 / COLUMNS as f32, row as f32 / ROWS as f32);
            let offset = (uv - Vec2::splat(0.5)) * 2.0;
            let edge = ((offset.length() - 0.52) / 0.90).clamp(0.0, 1.0);
            mesh.colored_vertex(
                rect.min + uv * rect.size(),
                alpha(Color32::from_rgb(2, 5, 12), edge * edge * 0.48),
            );
        }
    }
    for row in 0..ROWS {
        for column in 0..COLUMNS {
            let a = row * (COLUMNS + 1) + column;
            let b = a + COLUMNS + 1;
            mesh.add_triangle(a, a + 1, b);
            mesh.add_triangle(a + 1, b + 1, b);
        }
    }
    painter.add(mesh);
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_environment.rs"]
mod tests;
