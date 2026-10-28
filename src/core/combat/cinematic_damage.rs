//! Seekable damage: venting hulls and torn wrecks retain their original inertia.

use super::*;

// Matching jagged edges partition the original hull. No rectangular sticker or intact duplicate
// remains after a death; all three sections separate underneath the ordinary explosion atlas.
const STERN: &[(f32, f32)] =
    &[(0.0, 0.0), (0.36, 0.0), (0.31, 0.22), (0.39, 0.48), (0.30, 0.68), (0.34, 1.0), (0.0, 1.0)];
const MIDSHIP: &[(f32, f32)] = &[
    (0.36, 0.0),
    (0.69, 0.0),
    (0.64, 0.25),
    (0.73, 0.56),
    (0.62, 0.80),
    (0.67, 1.0),
    (0.34, 1.0),
    (0.30, 0.68),
    (0.39, 0.48),
    (0.31, 0.22),
];
const BOW: &[(f32, f32)] =
    &[(0.69, 0.0), (1.0, 0.0), (1.0, 1.0), (0.67, 1.0), (0.62, 0.80), (0.73, 0.56), (0.64, 0.25)];

#[derive(Clone, Copy)]
struct WreckMotion {
    origin: ActorPose,
    dimensions: Vec2,
    velocity: Vec2,
    mass: f32,
    phase: f32,
}

#[derive(Clone, Copy)]
struct HullFragment {
    center: Pos2,
    angle: f32,
    opacity: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct DamagePlume {
    center: Pos2,
    radius: f32,
    opacity: f32,
}

impl WreckMotion {
    fn lifetime(self) -> f32 {
        // The final casualty's pieces clear before the replay's 2.7-second closing hold ends.
        1.8 + self.mass * 0.8
    }

    fn reflection(self) -> Vec2 {
        vec2(
            if self.origin.mirror {
                -1.0
            } else {
                1.0
            },
            1.0,
        )
    }

    fn fragment(self, piece: usize, age: f32) -> Option<HullFragment> {
        if !(0.0..self.lifetime()).contains(&age) {
            return None;
        }
        let pivot = fragment_pivot(piece);
        let resting_offset = (pivot - Vec2::splat(0.5)) * self.dimensions * self.reflection();
        let ejection = [vec2(-0.18, -0.23), vec2(0.02, 0.13), vec2(0.22, 0.18)][piece];
        // The blast accelerates the pieces over a short interval; the pre-death flight
        // velocity is retained continuously rather than discarded at the casualty frame.
        let impulse_age = age - 0.10 * (1.0 - (-age / 0.10).exp());
        let separation = rotate(ejection * self.reflection(), self.origin.angle)
            * self.origin.size
            * impulse_age
            / (1.0 + self.mass * 0.7);
        let spin = [-0.67, 0.31, 0.56][piece]
            * (1.0 - self.mass * 0.62)
            * (0.92 + self.phase.sin() * 0.08);
        Some(HullFragment {
            center: self.origin.center
                + rotate(resting_offset, self.origin.angle)
                + self.velocity * age
                + separation,
            angle: self.origin.angle + spin * impulse_age,
            opacity: 0.90 * (1.0 - smooth((age / self.lifetime() - 0.46) / 0.54)),
        })
    }
}

fn fragment_pivot(piece: usize) -> Vec2 {
    vec2([0.17, 0.50, 0.83][piece], 0.5)
}

fn fragment_mesh(
    texture: TextureId,
    uv: Rect,
    motion: WreckMotion,
    piece: usize,
    fragment: HullFragment,
) -> Mesh {
    let polygon = [STERN, MIDSHIP, BOW][piece];
    let pivot = fragment_pivot(piece);
    let mut mesh = Mesh::with_texture(texture);
    // Preserve enough warmed armor detail to read the drifting wreck against deep space.
    let tint = alpha(Color32::from_rgb(145, 134, 124), fragment.opacity);
    let mut vertex = |point: Vec2| {
        mesh.vertices.push(Vertex {
            pos: fragment.center
                + rotate((point - pivot) * motion.dimensions * motion.reflection(), fragment.angle),
            uv: uv.min + point * uv.size(),
            color: tint,
        });
    };
    vertex(pivot);
    for &(x, y) in polygon {
        vertex(vec2(x, y));
    }
    for edge in 0..polygon.len() as u32 {
        mesh.add_triangle(0, edge + 1, (edge + 1) % polygon.len() as u32 + 1);
    }
    mesh
}

impl CinematicPlayback {
    fn damage_strength(&self, index: usize, time: f32) -> f32 {
        let actor = &self.timeline.actors[index];
        let state = actor.state_at(time);
        if state.hull == 0 || actor.death_at.is_some_and(|at| time >= at) {
            return 0.0;
        }
        let health = actor
            .levels_at(time)
            .map_or(state.hull as f32 / actor.max_hull.max(1) as f32, |levels| {
                levels as f32 / actor.initial_levels.unwrap_or(1).max(1) as f32
            });
        ((0.65 - health) / 0.60).clamp(0.0, 1.0)
    }

    fn damage_vent_position(&self, scene: Scene, index: usize, pose: ActorPose, time: f32) -> Pos2 {
        let local = if self.visuals[index].ground {
            vec2(-0.10, -0.23)
        } else {
            vec2(-0.16, -0.04)
        };
        self.hull_attachment(scene, index, pose, time, local * pose.size)
    }

    /// Sample the state at emission, not now: a new hit cannot retroactively create a
    /// full smoke trail, and repairs stop emission without erasing vapor already in flight.
    fn damage_plume(
        &self,
        scene: Scene,
        index: usize,
        emitted: f32,
        time: f32,
    ) -> Option<DamagePlume> {
        let age = time - emitted;
        if emitted < 0.0 || !(0.0..1.6).contains(&age) {
            return None;
        }
        let strength = self.damage_strength(index, emitted);
        if strength <= 0.0 {
            return None;
        }
        let visual = &self.visuals[index];
        let pose = self.actor_pose(scene, index, emitted);
        let source = self.damage_vent_position(scene, index, pose, emitted);
        let previous = self.actor_flight_pose(scene, index, emitted - 0.08);
        let velocity = (pose.center - previous.center) / 0.08;
        let direction = if visual.ground {
            vec2(-0.22, -0.98)
        } else {
            rotate(
                vec2(
                    if pose.mirror {
                        0.30
                    } else {
                        -0.30
                    },
                    -0.95,
                ),
                pose.angle,
            )
        };
        let curl = vec2(-direction.y, direction.x)
            * (age * 3.8 + emitted * 2.3 + visual.phase).sin()
            * pose.size
            * 0.055
            * smooth(age / 0.4);
        let progress = age / 1.6;
        Some(DamagePlume {
            center: source + velocity * age * 0.72 + direction * pose.size * age * 0.42 + curl,
            radius: pose.size * (0.025 + progress.sqrt() * 0.14),
            opacity: strength * smooth(age / 0.10) * (1.0 - smooth(progress)) * 0.34,
        })
    }

    pub(super) fn paint_damage_wake(&self, painter: &Painter, scene: Scene, index: usize) {
        // A fixed, bounded emission lattice is stable across seeks and frame rates. Every
        // puff keeps the position/velocity it had when emitted instead of orbiting the hull.
        // Density, not camera zoom, sets detail. Navigation must not reroll old emissions.
        let hull_size = self.visuals[index].size;
        let interval = if hull_size < 28.0 {
            0.32
        } else {
            0.16
        };
        let history = if hull_size < 28.0 {
            4
        } else {
            9
        };
        let phase = self.visuals[index].phase.rem_euclid(1.0) * interval;
        let latest = ((self.elapsed - phase) / interval).floor() as i32;
        for sample in (latest - history)..=latest {
            let emitted = sample as f32 * interval + phase;
            let Some(plume) = self.damage_plume(scene, index, emitted, self.elapsed) else {
                continue;
            };
            environment::soft_ellipse(
                painter,
                plume.center,
                Vec2::splat(plume.radius),
                Color32::from_rgb(84, 91, 103),
                plume.opacity,
            );
            if hull_size > 48.0 {
                environment::soft_ellipse(
                    painter,
                    plume.center + vec2(-0.14, 0.17) * plume.radius,
                    Vec2::splat(plume.radius * 0.63),
                    Color32::from_rgb(21, 26, 35),
                    plume.opacity * 0.70,
                );
            }
        }
    }

    pub(super) fn paint_damage_vent(
        &self,
        painter: &Painter,
        scene: Scene,
        index: usize,
        pose: ActorPose,
    ) {
        let strength = self.damage_strength(index, self.elapsed);
        if strength <= 0.0 {
            return;
        }
        let scar = self.damage_vent_position(scene, index, pose, self.elapsed);
        let pulse = 0.78 + 0.13 * (self.elapsed * 11.0 + self.visuals[index].phase).sin();
        // A compact hot breach sits on the armor; cooling smoke is drawn behind the hull.
        environment::soft_ellipse(
            painter,
            scar,
            vec2(0.08, 0.045) * pose.size,
            Color32::from_rgb(18, 17, 21),
            strength * 0.75,
        );
        glow(
            painter,
            scar,
            pose.size * 0.105,
            Color32::from_rgb(255, 108, 45),
            strength * pulse * 0.72,
        );
        glow(painter, scar, pose.size * 0.035, GOLD, strength * pulse);
        glow_line(
            painter,
            scar - rotate(vec2(pose.size * 0.025, 0.0), pose.angle),
            scar + rotate(vec2(pose.size * 0.025, 0.0), pose.angle),
            (pose.size * 0.012).max(scene.scale * 0.7),
            Color32::from_rgb(255, 233, 171),
            strength * pulse,
        );
    }

    /// A recorded ship casualty keeps its final momentum while the hull breaks behind the blast.
    /// All samples depend only on replay age, so pauses, seeks and speed changes are exact.
    pub(super) fn paint_actor_destruction(
        &self,
        painter: &Painter,
        scene: Scene,
        images: &ImageIds,
        index: usize,
        death: f32,
    ) {
        let actor = &self.timeline.actors[index];
        let age = self.elapsed - death;
        if !actor.unit.is_ship() || !(0.0..3.6).contains(&age) {
            return;
        }
        let visual = &self.visuals[index];
        let pose = self.actor_pose(scene, index, death);
        let before = self.actor_flight_pose(scene, index, death - 0.08);
        let velocity = (pose.center - before.center) / 0.08;
        let (texture, uv, aspect) = if let Some((sheet, texture)) = visual
            .firing_sheet
            .and_then(|sheet| images.0.get(sheet.texture).map(|texture| (sheet, *texture)))
        {
            // Use the registered rest pose, whether it occupies one cell of an atlas or
            // the complete War Sun cutout. A wreck must never retain a baked muzzle flash.
            (Some(texture), sheet.uv(0, FULL_UV), sheet.aspect)
        } else if let Some(texture) = images.0.get(&visual.texture) {
            (Some(*texture), FULL_UV, visual.aspect)
        } else {
            (images.0.get(&visual.fallback_texture).copied(), FULL_UV, 1.0)
        };
        let motion = WreckMotion {
            origin: pose,
            dimensions: self.hull_dimensions(
                scene,
                index,
                pose,
                death,
                vec2(pose.size * aspect.min(1.0), pose.size / aspect.max(1.0)),
            ),
            velocity,
            mass: ((unit_size(actor.unit) - 66.0) / (242.0 - 66.0)).clamp(0.0, 1.0),
            phase: visual.phase,
        };
        for piece in 0..3 {
            let Some(fragment) = motion.fragment(piece, age) else {
                continue;
            };
            // A bounded history leaves smoke at actual earlier fragment locations; it does
            // not move an entire particle train rigidly with the wreck or respawn on a seek.
            let latest = (age.min(motion.lifetime() * 0.65) / 0.16).floor() as u32;
            for puff in latest.saturating_sub(3)..=latest {
                let emitted = puff as f32 * 0.16;
                let smoke_age = age - emitted;
                if !(0.0..0.64).contains(&smoke_age) {
                    continue;
                }
                let Some(source) = motion.fragment(piece, emitted) else {
                    continue;
                };
                let progress = smoke_age / 0.64;
                let center = source.center
                    + motion.velocity * smoke_age * 0.65
                    + vec2(-0.08, -0.14) * pose.size * smoke_age;
                paint_effect_sprite(
                    painter,
                    images,
                    "combat fx glow",
                    center,
                    Vec2::splat(pose.size * (0.12 + progress * 0.22)),
                    0.0,
                    alpha(
                        Color32::from_rgb(84, 67, 58),
                        particle_envelope(progress) * fragment.opacity * 0.42,
                    ),
                );
            }
            if let Some(texture) = texture {
                painter.add(Shape::mesh(fragment_mesh(texture, uv, motion, piece, fragment)));
            }
            let burn = (1.0 - age / (motion.lifetime() * 0.72)).max(0.0) * fragment.opacity;
            let edge = fragment.center
                + rotate(
                    vec2(
                        if piece == 0 {
                            0.14
                        } else {
                            -0.12
                        },
                        0.02,
                    ) * motion.dimensions
                        * motion.reflection(),
                    fragment.angle,
                );
            glow(
                painter,
                edge,
                pose.size * 0.065,
                GOLD,
                burn * (0.6 + 0.15 * (age * 17.0 + piece as f32 * 2.3 + visual.phase).sin()),
            );
        }
        // Carry the existing fire/debris atlas with the same momentum as its parent hull.
        // The immutable report still supplies the only death and its exact starting time.
        explosion(painter, images, pose.center + velocity * age, pose.size, age, actor.unit);
    }
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_damage.rs"]
mod tests;
