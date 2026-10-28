//! Deterministic, cached fleet staging with room for hulls and their escorts.

use super::*;

struct Slot {
    index: usize,
    anchor: Vec2,
    home: Vec2,
    radius: Vec2,
}

/// Arrange whole hulls once, retaining the shared class scale and every recorded actor.
pub(super) fn layout_fleets(visuals: &mut [ActorVisual], actors: &[CinematicActor]) {
    for side in [Side::Attacker, Side::Defender] {
        let mut indices: Vec<_> = actors
            .iter()
            .enumerate()
            .filter(|(index, actor)| {
                actor.side == side && actor.unit.is_ship() && visuals[*index].visible
            })
            .map(|(index, _)| index)
            .collect();
        if indices.is_empty() {
            continue;
        }
        indices.sort_by(|&a, &b| {
            visuals[b]
                .size
                .total_cmp(&visuals[a].size)
                .then_with(|| visuals[a].phase.total_cmp(&visuals[b].phase))
                .then_with(|| actors[a].id.cmp(&actors[b].id))
                .then_with(|| a.cmp(&b))
        });
        let leaders =
            indices.iter().filter(|&&index| unit_size(actors[index].unit) >= 132.0).count();
        let squadrons = indices.len().div_ceil(4).max(leaders).max(1);
        let direction = if side == Side::Attacker {
            1.0
        } else {
            -1.0
        };
        let mut slots: Vec<_> = indices
            .iter()
            .enumerate()
            .map(|(rank, &index)| {
                let visual = &visuals[index];
                let (squadron, escort) = if rank < squadrons {
                    (rank, 0)
                } else {
                    ((rank - squadrons) % squadrons, (rank - squadrons) / squadrons + 1)
                };
                let spread = if squadrons == 1 {
                    0.0
                } else {
                    (squadron as f32 / (squadrons - 1) as f32).sqrt()
                };
                let bearing = squadron as f32 * 2.399_963_1 + 0.35;
                let disk = Vec2::angled(bearing) * spread;
                let capital = (visual.size / 242.0).clamp(0.0, 1.0);
                let center = if side == Side::Attacker {
                    vec2(0.26 - capital * 0.035, 0.44) + disk * vec2(0.13, 0.205)
                } else {
                    // Heavy defenders need the broad upper-left approach, not the tight
                    // strip directly above the planet where projected hulls pile up.
                    vec2(0.73 - capital * 0.10, 0.28) + disk * vec2(0.15, 0.13)
                };
                let wing = if escort == 0 {
                    Vec2::ZERO
                } else {
                    let flank = if escort.is_multiple_of(2) {
                        -1.0
                    } else {
                        1.0
                    };
                    vec2(-direction * (0.018 + escort as f32 * 0.014), flank * 0.035)
                };
                let variation =
                    vec2((visual.phase * 1.7).cos() * 0.005, visual.phase.sin() * 0.009);
                let anchor = constrain_home(center + wing + variation, visual, &side);
                Slot {
                    index,
                    anchor,
                    home: anchor,
                    radius: footprint(visual),
                }
            })
            .collect();
        let widest = slots.iter().map(|slot| slot.radius.x).fold(0.0, f32::max);
        let mut order: Vec<_> = (0..slots.len()).collect();
        // The work budget scales with the actor count, not its square. Sorting exposes
        // nearby hulls; a bounded forward sweep handles crowding without a startup stall.
        for _ in 0..20 {
            for slot in &mut slots {
                slot.home += (slot.anchor - slot.home) * 0.012;
            }
            order.sort_by(|&a, &b| {
                slots[a].home.x.total_cmp(&slots[b].home.x).then_with(|| a.cmp(&b))
            });
            for (position, &left) in order.iter().enumerate() {
                for &right in order[position + 1..].iter().take(64) {
                    let delta = slots[right].home - slots[left].home;
                    if delta.x > slots[left].radius.x + widest {
                        break;
                    }
                    let radii = slots[left].radius + slots[right].radius;
                    let metric = delta / radii;
                    let distance = metric.length();
                    if distance >= 1.0 {
                        continue;
                    }
                    let normal = if distance > 0.0001 {
                        metric / distance
                    } else {
                        // Stable separation for identical anchors; never process randomness.
                        Vec2::angled((left + right) as f32 * 2.399_963_1)
                    };
                    let correction = normal * radii * ((1.0 - distance) * 0.72);
                    let left_area = slots[left].radius.x * slots[left].radius.y;
                    let right_area = slots[right].radius.x * slots[right].radius.y;
                    // Escorts yield more space; capital hulls retain the formation's spine.
                    let left_weight = right_area / (left_area + right_area);
                    slots[left].home -= correction * left_weight;
                    slots[right].home += correction * (1.0 - left_weight);
                }
            }
            for slot in &mut slots {
                slot.home = constrain_home(slot.home, &visuals[slot.index], &side);
            }
        }
        for slot in slots {
            visuals[slot.index].home = slot.home;
        }
    }
}

fn footprint(visual: &ActorVisual) -> Vec2 {
    // The ellipse includes a small maneuvering gap and the vertical room needed for a
    // shallow bank. Express it on the same design canvas used by the renderer's scale.
    vec2(visual.size * 0.50 + 5.0, visual.size * 0.36 + 5.0) / vec2(1440.0, 820.0)
}

fn constrain_home(home: Vec2, visual: &ActorVisual, side: &Side) -> Vec2 {
    if *side == Side::Attacker {
        return home.clamp(vec2(0.085, 0.18), vec2(0.43, 0.70));
    }
    let mut home = home.clamp(vec2(0.46, 0.155), vec2(0.92, 0.43));
    let scene = Scene::new(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 820.0)));
    // Reserve the nominal arrival corridor. The continuous renderer supplies the
    // exact seeded excursion and clearance at every viewport and time; reserving all
    // possible downward excursions here would crush defenders into the top screen edge.
    let clearance = scene.planet_radius * 1.11 + visual.size * 0.60;
    let top_distance = scene.planet.y - 0.155 * 820.0;
    if clearance > top_distance {
        let x_limit = scene.planet.x - (clearance * clearance - top_distance * top_distance).sqrt();
        // Defenders' free flight begins just left of their saved home, then continues left.
        home.x = home.x.min(x_limit / 1440.0 + 0.055);
    }
    let horizontal = ((home.x - 0.055) * 1440.0 - scene.planet.x).abs();
    if horizontal < clearance {
        let y_limit = scene.planet.y - (clearance * clearance - horizontal * horizontal).sqrt();
        home.y = home.y.min(y_limit / 820.0);
    }
    home
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_formation.rs"]
mod tests;
