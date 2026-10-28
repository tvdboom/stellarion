//! Recorded hull impulses with finite recovery, independent of playback step or seek order.

use super::*;

#[derive(Default)]
pub(super) struct HullImpactResponse {
    pub displacement: Vec2,
    pub bank: f32,
}

impl CinematicPlayback {
    /// Keep a stable flight lane beside an orbital installation on a broad approach.
    /// Stations never clear other stations here, so this path cannot recurse through the fleet.
    pub(super) fn station_berth(
        &self,
        scene: Scene,
        index: usize,
        time: f32,
        mut center: Pos2,
        size: f32,
    ) -> Pos2 {
        for &station in &self.orbital_obstacles {
            if station == index {
                continue;
            }
            let strength = self.timeline.actors[station].death_at.map_or(1.0, |death| {
                // Keep the corridor through the visible wreck hold, then reopen it gently.
                1.0 - smooth(((time - death - 3.6) / 1.8).clamp(0.0, 1.0))
            });
            if strength <= 0.0 {
                continue;
            }
            let obstacle = self.actor_cruise_pose(scene, station, time);
            let radius = obstacle.size * 0.49 + size * 0.56 + 10.0 * scene.scale;
            let offset = center - obstacle.center;
            let lateral = offset.y.abs();
            let blend = 1.0 - smooth(((lateral / radius - 1.0) / 0.9).clamp(0.0, 1.0));
            if blend <= 0.0 {
                continue;
            }
            // High outer defenders hold the far berth; lower hulls use the open left side
            // away from the planet. The choice is fixed by staging, never a moving sign test.
            let home = self.visuals[index].home;
            let station_home =
                self.actor_cruise_pose(scene, station, self.timeline.entrance_duration).center;
            let side = if self.timeline.actors[index].side == Side::Defender
                && home.y < 0.27
                && scene.point(home).x > station_home.x + 36.0 * scene.scale
            {
                1.0
            } else {
                -1.0
            };
            // This smooth skirt always lies outside the circular footprint. Horizontal
            // displacement preserves height, including hulls already near the top edge.
            let extent = radius / (1.0 + (offset.y / radius).powi(2)).sqrt();
            let gap = offset.x * side;
            let margin = radius * 0.10;
            if gap < extent + margin {
                let p = ((gap - extent + margin) / (2.0 * margin)).clamp(0.0, 1.0);
                let clear = extent + margin * p.powi(3) * (2.0 - p);
                center.x += side * (clear - gap) * blend * strength;
            }
        }
        center
    }

    pub(super) fn hull_impact_response(
        &self,
        scene: Scene,
        index: usize,
        time: f32,
    ) -> HullImpactResponse {
        let actor = &self.timeline.actors[index];
        let visual = &self.visuals[index];
        if !actor.unit.is_ship() || !time.is_finite() {
            return HullImpactResponse::default();
        }
        let mass = ((unit_size(actor.unit) - 66.0) / (242.0 - 66.0)).clamp(0.0, 1.0);
        let duration = 1.15 + mass * 0.90;
        let rise = 0.12 + mass * 0.13;
        let first = visual
            .incoming_shots
            .partition_point(|&shot| self.timeline.shots[shot].impact_at < time - duration);
        let end = visual
            .incoming_shots
            .partition_point(|&shot| self.timeline.shots[shot].impact_at <= time);
        let density = visual.size / unit_size(actor.unit);
        let amplitude = 14.0 * scene.scale * density * (1.0 - mass * 0.82);
        let mut displacement = Vec2::ZERO;
        let mut bank = 0.0;
        for &shot_index in &visual.incoming_shots[first..end] {
            let shot = &self.timeline.shots[shot_index];
            // Shield light and missed shots never kick the physical hull. Their damage
            // remains visible through the existing shield effect and recorded state.
            if shot.outcome.hull_damage == 0 || shot.outcome.missed {
                continue;
            }
            let age = time - shot.impact_at;
            let envelope = if age < rise {
                smooth((age / rise).clamp(0.0, 1.0))
            } else {
                1.0 - smooth(((age - rise) / (duration - rise)).clamp(0.0, 1.0))
            };
            if envelope <= 0.0 {
                continue;
            }
            let fraction = (shot.outcome.hull_damage as f32 / actor.max_hull.max(1) as f32)
                .clamp(0.0, 1.0)
                .sqrt();
            // Sample the intended release and contact positions, never another actor's
            // reacted pose. Reciprocal hits can therefore be evaluated in any order.
            let from = self.actor_course_pose(scene, shot.source, shot.launch_at).center;
            let to = self.actor_course_pose(scene, index, shot.impact_at).center;
            let incoming = (to - from).normalized();
            let strength = envelope * fraction;
            displacement += incoming * amplitude * strength;
            let lever =
                noise(shot_index as u32 ^ actor.id.unwrap_or(index as u64) as u32) * 2.0 - 1.0;
            bank += (incoming.y * 0.65 + lever * 0.35) * 0.10 * (1.0 - mass * 0.84) * strength;
        }
        // Smooth saturation bounds simultaneous barrages without abruptly clipping a
        // recovered trajectory or changing the response when an unrelated hit is added.
        let travel_limit = (amplitude * 1.65).max(0.001);
        displacement /= (1.0 + displacement.length_sq() / (travel_limit * travel_limit)).sqrt();
        let bank_limit = 0.12 * (1.0 - mass * 0.84);
        bank = bank_limit * (bank / bank_limit).tanh();
        HullImpactResponse {
            displacement,
            bank,
        }
    }
}
