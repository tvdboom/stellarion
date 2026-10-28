//! Recorded firing cycles, attachment points and separately aimed turret artwork.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct FiringSheet {
    pub texture: &'static str,
    pub aspect: f32,
    /// The War Sun keeps one registered hull while light and beams animate over it.
    single_frame: bool,
    muzzle_x: [f32; 8],
    /// Animated gun region; everything outside it always uses the resting hull frame.
    weapon: Rect,
    pivot: Pos2,
    ground: bool,
}

pub(super) fn firing_sheet(unit: Unit) -> Option<FiringSheet> {
    let (texture, y, muzzle_x, left, top, bottom) = match unit {
        Unit::Ship(Ship::LightFighter) => (
            "firing light fighter",
            0.530,
            [0.930, 0.910, 0.860, 0.840, 0.890, 0.920, 0.930, 0.930],
            0.810,
            0.430,
            0.630,
        ),
        Unit::Ship(Ship::HeavyFighter) => (
            "firing heavy fighter",
            0.650,
            [0.950, 0.950, 0.830, 0.810, 0.910, 0.920, 0.950, 0.950],
            0.800,
            0.540,
            0.770,
        ),
        Unit::Ship(Ship::Destroyer) => (
            "firing destroyer",
            0.430,
            [0.790, 0.790, 0.740, 0.670, 0.680, 0.760, 0.780, 0.790],
            0.640,
            0.340,
            0.460,
        ),
        Unit::Ship(Ship::Cruiser) => (
            "firing cruiser",
            0.550,
            [0.970, 0.970, 0.870, 0.850, 0.900, 0.940, 0.970, 0.970],
            0.850,
            0.480,
            0.600,
        ),
        Unit::Ship(Ship::Bomber) => (
            "firing bomber",
            0.560,
            [0.970, 0.970, 0.880, 0.870, 0.960, 0.970, 0.970, 0.970],
            0.860,
            0.510,
            0.610,
        ),
        Unit::Ship(Ship::Battleship) => (
            "firing battleship",
            0.430,
            [0.890, 0.890, 0.840, 0.800, 0.870, 0.880, 0.890, 0.890],
            0.680,
            0.380,
            0.490,
        ),
        Unit::Ship(Ship::Dreadnought) => (
            "firing dreadnought",
            0.460,
            [0.990, 0.990, 0.930, 0.890, 0.920, 0.970, 0.990, 0.990],
            0.850,
            0.400,
            0.500,
        ),
        Unit::Ship(Ship::WarSun) => ("cinematic war sun", 0.505, [0.950; 8], 0.630, 0.350, 0.620),
        Unit::Defense(Defense::SpaceDock) => {
            ("firing space dock", 0.400, [0.055; 8], 0.640, 0.440, 0.650)
        },
        Unit::Defense(Defense::RocketLauncher) => (
            "firing rocket launcher",
            0.240,
            [0.860, 0.860, 0.860, 0.860, 0.860, 0.860, 0.860, 0.860],
            0.000,
            0.000,
            0.550,
        ),
        Unit::Defense(Defense::LightLaser) => (
            "firing light laser",
            0.310,
            [0.930, 0.940, 0.840, 0.690, 0.860, 0.930, 0.960, 0.930],
            0.000,
            0.000,
            0.480,
        ),
        Unit::Defense(Defense::HeavyLaser) => (
            "firing heavy laser",
            0.340,
            [0.970, 0.960, 0.810, 0.670, 0.900, 0.970, 0.970, 0.970],
            0.000,
            0.000,
            0.480,
        ),
        Unit::Defense(Defense::GaussCannon) => (
            "firing gauss cannon",
            0.360,
            [0.940, 0.940, 0.810, 0.680, 0.820, 0.920, 0.940, 0.940],
            0.000,
            0.000,
            0.530,
        ),
        Unit::Defense(Defense::IonCannon) => (
            "firing ion cannon",
            0.360,
            [0.920, 0.920, 0.800, 0.650, 0.800, 0.880, 0.910, 0.920],
            0.000,
            0.000,
            0.460,
        ),
        Unit::Defense(Defense::PlasmaTurret) => (
            "firing plasma turret",
            0.360,
            [0.950, 0.940, 0.820, 0.690, 0.820, 0.890, 0.950, 0.950],
            0.000,
            0.000,
            0.480,
        ),
        _ => return None,
    };
    let ground = unit.is_defense() && unit != Unit::space_dock();
    Some(FiringSheet {
        texture,
        single_frame: unit == Unit::war_sun(),
        aspect: if ground || unit == Unit::space_dock() {
            1.0
        } else {
            1.5
        },
        muzzle_x,
        weapon: if ground {
            Rect::from_min_max(pos2(left, top), pos2(1.0, bottom))
        } else {
            FULL_UV
        },
        ground,
        pivot: pos2(
            if ground {
                0.40
            } else {
                left
            },
            y,
        ),
    })
}

impl FiringSheet {
    pub(super) fn uv(self, frame: usize, part: Rect) -> Rect {
        if self.single_frame {
            part
        } else {
            frame_uv(frame, part)
        }
    }

    fn dimensions(self, size: f32) -> Vec2 {
        vec2(size, size / self.aspect)
    }

    fn muzzle(self, frame: usize) -> Pos2 {
        pos2(self.muzzle_x[frame], self.pivot.y)
    }
}

fn frame_uv(frame: usize, part: Rect) -> Rect {
    // The last pose reuses the registered rest frame for a seamless return to idle.
    let frame = if frame == 7 {
        0
    } else {
        frame
    };
    let offset = vec2((frame % 4) as f32, (frame / 4) as f32);
    Rect::from_min_max(
        ((part.min.to_vec2() + offset) / vec2(4.0, 2.0)).to_pos2(),
        ((part.max.to_vec2() + offset) / vec2(4.0, 2.0)).to_pos2(),
    )
}

/// One cycle is tied to a recorded release, never to frame count or wall-clock time.
fn release_frame(age: f32) -> usize {
    match age {
        t if !t.is_finite() || !(-0.16..0.58).contains(&t) => 0,
        t if t < 0.0 => 1,
        t if t < 0.07 => 2,
        t if t < 0.15 => 3,
        t if t < 0.25 => 4,
        t if t < 0.37 => 5,
        t if t < 0.49 => 6,
        _ => 7,
    }
}

fn hull_tint(color: Color32, strength: f32) -> Color32 {
    let channel = |value: u8| (255.0 + (value as f32 - 255.0) * strength).round() as u8;
    Color32::from_rgb(channel(color.r()), channel(color.g()), channel(color.b()))
}

/// Paint the source hull with a gentle sky-facing highlight, without faction bands.
pub(super) fn paint_owner_hull(
    painter: &Painter,
    texture: TextureId,
    center: Pos2,
    dimensions: Vec2,
    angle: f32,
    mirror: bool,
    uv: Rect,
    _owner: Option<Color32>,
) {
    // Vertex illumination retains every source pixel and its transparency. The subtle cool
    // underside gives the hull volume without painting flat stripes over its armor details.
    let mut mesh = Mesh::with_texture(texture);
    for (corner, coordinates) in [
        (vec2(-0.5, -0.5), uv.left_top()),
        (vec2(0.5, -0.5), uv.right_top()),
        (vec2(0.5, 0.5), uv.right_bottom()),
        (vec2(-0.5, 0.5), uv.left_bottom()),
    ] {
        mesh.vertices.push(Vertex {
            pos: center + rotate(corner * dimensions, angle),
            uv: pos2(
                if mirror {
                    uv.right() + uv.left() - coordinates.x
                } else {
                    coordinates.x
                },
                coordinates.y,
            ),
            color: if corner.y < 0.0 {
                Color32::WHITE
            } else {
                Color32::from_rgb(210, 225, 242)
            },
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
}

/// Nozzles are attached to the resting artwork, independently of animated gun barrels.
fn engine_ports(ship: Ship) -> &'static [(f32, f32)] {
    match ship {
        Ship::LightFighter => &[(0.105, 0.44), (0.105, 0.58)],
        Ship::HeavyFighter => &[(0.065, 0.51), (0.075, 0.70)],
        Ship::Destroyer => &[(0.070, 0.48), (0.075, 0.60)],
        Ship::Cruiser => &[(0.065, 0.56), (0.070, 0.62)],
        Ship::Bomber => &[(0.100, 0.38), (0.065, 0.62)],
        Ship::Battleship => &[(0.065, 0.49), (0.120, 0.71)],
        Ship::Dreadnought => &[(0.065, 0.46), (0.065, 0.55), (0.065, 0.66)],
        Ship::WarSun => &[(0.037, 0.441), (0.037, 0.506), (0.037, 0.562)],
        // These two use the original oblique canvases rather than firing atlases.
        Ship::Probe => &[(0.45, 0.67)],
        Ship::ColonyShip => &[(0.050, 0.49), (0.065, 0.57)],
    }
}

fn engine_attachment(pose: ActorPose, dimensions: Vec2, port: (f32, f32)) -> Pos2 {
    let reflection = vec2(
        if pose.mirror {
            -1.0
        } else {
            1.0
        },
        1.0,
    );
    pose.center
        + rotate((vec2(port.0, port.1) - Vec2::splat(0.5)) * dimensions * reflection, pose.angle)
}

/// Continuous transparent edges avoid the stacked circles and hard cones of sprite trails.
fn engine_plume_mesh(
    origin: Pos2,
    axis: Vec2,
    length: f32,
    radius: f32,
    bend: Vec2,
    color: Color32,
    strength: f32,
) -> Mesh {
    let mut mesh = Mesh::default();
    let normal = vec2(-axis.y, axis.x);
    for row in 0..=8 {
        let t = row as f32 / 8.0;
        let center = origin - axis * (length * t) + bend * t * t;
        let width = radius * (0.65 + 1.4 * (PI * t).sin()) * (1.0 - t);
        let tint = if row < 2 {
            Color32::from_rgb(230, 247, 255)
        } else {
            color
        };
        for column in 0..=4 {
            let cross = column as f32 * 0.5 - 1.0;
            let intensity = (1.0 - cross.abs()).powi(2) * (1.0 - t).powi(2) * strength;
            mesh.vertices.push(Vertex {
                pos: center + normal * width * cross,
                uv: bevy_egui::egui::epaint::WHITE_UV,
                color: alpha(tint, intensity),
            });
            if row > 0 && column > 0 {
                let i = (row * 5 + column) as u32;
                mesh.indices.extend_from_slice(&[i - 6, i - 5, i, i - 6, i, i - 1]);
            }
        }
    }
    mesh
}

fn maneuver_thrust(velocity: Vec2, acceleration: Vec2, scale: f32) -> (f32, f32) {
    if velocity.length_sq() < 0.01 {
        return (0.0, 0.0);
    }
    let forward = velocity.normalized();
    let normal = vec2(-forward.y, forward.x);
    let lateral = acceleration.dot(normal) / scale.max(0.01);
    let braking = -acceleration.dot(forward) / scale.max(0.01);
    // Ignore numerical noise and imperceptibly shallow turns; tiny thrusters react only to
    // an actual course correction or deceleration in the sampled path.
    (
        lateral.signum() * smooth(((lateral.abs() - 7.0) / 75.0).clamp(0.0, 1.0)),
        smooth(((braking - 12.0) / 160.0).clamp(0.0, 1.0)),
    )
}

/// Align for a release and recover to the course between salvos. Overlapping windows merge
/// smoothly, so rapid fire holds its firing attitude instead of nodding for every projectile.
fn salvo_aim_weight(launches: &[f32], time: f32, unit: Unit) -> f32 {
    let inertia = (unit_size(unit) / 160.0).clamp(0.5, 1.5);
    let lead = 0.65 + inertia * 0.30;
    let recovery = 0.55 + inertia * 0.40;
    let first = launches.partition_point(|launch| *launch < time - 0.32 - recovery);
    let end = launches.partition_point(|launch| *launch <= time + lead);
    let mut free = 1.0;
    for &launch in &launches[first..end] {
        let prepare = smooth(((time - launch + lead) / (lead - 0.16)).clamp(0.0, 1.0));
        let recover = 1.0 - smooth(((time - launch - 0.32) / recovery).clamp(0.0, 1.0));
        free *= 1.0 - prepare * recover;
    }
    1.0 - free
}

fn ship_aim_limit(unit: Unit) -> f32 {
    (0.10 * (66.0 / unit_size(unit)).sqrt()).clamp(0.045, 0.10)
}

fn perspective_roll(incoming: Vec2, outgoing: Vec2, horizon: f32, unit: Unit) -> f32 {
    if incoming.length_sq().min(outgoing.length_sq()) < 0.000_001 {
        return 0.0;
    }
    let cross = incoming.x * outgoing.y - incoming.y * outgoing.x;
    let turn_rate = cross.atan2(incoming.dot(outgoing)) / horizon.max(0.01);
    let response = turn_rate * 2.4;
    let mass = (80.0 / unit_size(unit)).clamp(0.30, 1.0);
    // A smooth bounded response gives a suggestion of roll without narrowing a sprite into
    // an edge-on card, even at a tight retreat turn or an abrupt path constraint.
    response / (1.0 + response.abs()) * mass
}

fn engine_color(owner: Option<Color32>) -> Color32 {
    owner.map_or(BLUE, |owner| {
        Color32::from_rgb(
            ((owner.r() as u16 + 104) / 2) as u8,
            ((owner.g() as u16 + 210) / 2) as u8,
            ((owner.b() as u16 + 255) / 2) as u8,
        )
    })
}

#[derive(Clone, Copy)]
struct HullLight {
    position: Pos2,
    radius: f32,
    color: Color32,
    strength: f32,
}

struct HullLighting {
    center: Pos2,
    size: f32,
    upper_surface: Vec2,
    roll: f32,
    planet: Option<(Vec2, Color32, f32)>,
    flashes: [Option<HullLight>; 2],
    engine: Option<HullLight>,
}

impl HullLighting {
    fn color_at(&self, point: Pos2) -> Color32 {
        let offset = (point - self.center) / self.size.max(0.01);
        // Low blue skylight keeps small metallic silhouettes legible against deep space.
        let sky = (0.5 + offset.dot(self.upper_surface)).clamp(0.0, 1.0)
            * (0.035 + self.roll.abs() * 0.055);
        let mut rgb = [0.55 * sky, 0.78 * sky, sky];
        let mut illuminate = |color: Color32, strength: f32| {
            for (channel, value) in rgb.iter_mut().zip([color.r(), color.g(), color.b()]) {
                *channel += value as f32 / 255.0 * strength;
            }
        };
        if let Some((direction, color, strength)) = self.planet {
            let facing = (0.45 + offset.dot(direction)).clamp(0.0, 1.0).powi(2);
            illuminate(color, facing * strength);
        }
        for light in self.flashes.iter().flatten().chain(self.engine.iter()) {
            let falloff = (1.0 - point.distance(light.position) / light.radius.max(0.01))
                .clamp(0.0, 1.0)
                .powi(2);
            illuminate(light.color, light.strength * falloff);
        }
        // Zero alpha intentionally selects additive blending. The sampled texture still masks
        // the light to the exact hull silhouette, including holes between station structures.
        Color32::from_rgba_premultiplied(
            (rgb[0].clamp(0.0, 0.8) * 255.0) as u8,
            (rgb[1].clamp(0.0, 0.8) * 255.0) as u8,
            (rgb[2].clamp(0.0, 0.8) * 255.0) as u8,
            0,
        )
    }
}

fn lit_hull_mesh(
    texture: TextureId,
    pose: ActorPose,
    dimensions: Vec2,
    uv: Rect,
    light: &HullLighting,
) -> Mesh {
    let mut mesh = Mesh::with_texture(texture);
    for row in 0..=4 {
        for column in 0..=6 {
            let point = vec2(column as f32 / 6.0, row as f32 / 4.0);
            let position =
                pose.center + rotate((point - Vec2::splat(0.5)) * dimensions, pose.angle);
            mesh.vertices.push(Vertex {
                pos: position,
                uv: uv.min
                    + vec2(
                        if pose.mirror {
                            1.0 - point.x
                        } else {
                            point.x
                        },
                        point.y,
                    ) * uv.size(),
                color: light.color_at(position),
            });
            if row > 0 && column > 0 {
                let i = (row * 7 + column) as u32;
                mesh.indices.extend_from_slice(&[i - 8, i - 7, i, i - 8, i, i - 1]);
            }
        }
    }
    mesh
}

impl CinematicPlayback {
    fn hull_roll(&self, scene: Scene, index: usize, time: f32) -> f32 {
        let actor = &self.timeline.actors[index];
        if !actor.unit.is_ship() {
            return 0.0;
        }
        let visual = &self.visuals[index];
        let mut free = 1.0 - salvo_aim_weight(&visual.firing_times, time, actor.unit);
        for attack in &self.timeline.planet_attacks {
            if attack.sources.contains(&index) {
                let aligning = smooth((time - attack.start_at + 1.0).clamp(0.0, 1.0))
                    * (1.0 - smooth((time - attack.end_at).clamp(0.0, 1.0)));
                free *= 1.0 - aligning;
            }
        }
        if free <= 0.0 {
            return 0.0;
        }
        let horizon = (unit_size(actor.unit) / 240.0).clamp(0.35, 0.85);
        // Sample the smooth course, excluding reactive damage displacement. A forthcoming
        // hit must not start rolling the hull before the actual recorded contact.
        let before = self.actor_course_pose(scene, index, time - horizon).center;
        let center = self.actor_course_pose(scene, index, time).center;
        let after = self.actor_course_pose(scene, index, time + horizon).center;
        perspective_roll(center - before, after - center, horizon, actor.unit) * free
    }

    /// Shared perspective for the hull, textured light, nozzles and weapon attachments.
    /// The strongest fighter turn compresses height by at most eight percent.
    pub(super) fn hull_dimensions(
        &self,
        scene: Scene,
        index: usize,
        _pose: ActorPose,
        time: f32,
        dimensions: Vec2,
    ) -> Vec2 {
        dimensions * vec2(1.0, 1.0 - self.hull_roll(scene, index, time).abs() * 0.08)
    }

    /// Maps an image-local pixel offset through exactly the same perspective as the hull.
    pub(super) fn hull_attachment(
        &self,
        scene: Scene,
        index: usize,
        pose: ActorPose,
        time: f32,
        offset: Vec2,
    ) -> Pos2 {
        let banked = self.hull_dimensions(scene, index, pose, time, offset);
        let reflection = vec2(
            if pose.mirror {
                -1.0
            } else {
                1.0
            },
            1.0,
        );
        pose.center + rotate(banked * reflection, pose.angle)
    }

    fn hull_lighting(&self, scene: Scene, index: usize, pose: ActorPose) -> HullLighting {
        let visual = &self.visuals[index];
        let roll = self.hull_roll(scene, index, self.elapsed);
        let mut lighting = HullLighting {
            center: pose.center,
            size: pose.size,
            upper_surface: rotate(vec2(0.0, -1.0), pose.angle + roll * 0.5),
            roll,
            planet: self.show_planet.then(|| {
                let to_planet = scene.planet - pose.center;
                let proximity =
                    (scene.planet_radius / to_planet.length().max(scene.planet_radius)).powi(2);
                (to_planet / to_planet.length().max(0.01), self.atmosphere_color, proximity * 0.22)
            }),
            flashes: [None, None],
            engine: None,
        };
        if let Unit::Ship(ship) = self.timeline.actors[index].unit {
            let ports = engine_ports(ship);
            let mean =
                ports.iter().fold(Vec2::ZERO, |sum, &(x, y)| sum + vec2(x, y)) / ports.len() as f32;
            let dimensions = visual.firing_sheet.map_or_else(
                || vec2(pose.size * visual.aspect.min(1.0), pose.size / visual.aspect.max(1.0)),
                |sheet| sheet.dimensions(pose.size),
            ) * vec2(1.0, 1.0 - roll.abs() * 0.08);
            lighting.engine = Some(HullLight {
                position: engine_attachment(pose, dimensions, (mean.x, mean.y)),
                radius: pose.size * 0.48,
                color: engine_color(visual.owner_color),
                strength: 0.13 + 0.015 * (self.elapsed * 8.0 + visual.phase).sin(),
            });
        }
        let released = visual.firing_times.partition_point(|at| *at <= self.elapsed);
        if let Some(&shot_index) =
            released.checked_sub(1).and_then(|slot| visual.firing_shots.get(slot))
        {
            let shot = &self.timeline.shots[shot_index];
            let age = self.elapsed - shot.launch_at;
            if (0.0..0.20).contains(&age) {
                let weapon = Weapon::for_shot(self.timeline.actors[index].unit, &shot.outcome);
                lighting.flashes[0] = Some(HullLight {
                    position: self.actor_muzzle(scene, index, self.elapsed, scene.planet),
                    radius: pose.size * 0.85,
                    color: weapon_color(weapon.color()),
                    strength: (1.0 - age / 0.20).powi(2) * 0.72,
                });
            }
        }
        let impacted = visual
            .incoming_shots
            .partition_point(|&shot| self.timeline.shots[shot].impact_at <= self.elapsed);
        // The incoming index is sorted once at replay construction. Only the last four
        // contacts can contribute, regardless of total battle duration or fleet size.
        for &shot_index in visual.incoming_shots[impacted.saturating_sub(4)..impacted].iter().rev()
        {
            let shot = &self.timeline.shots[shot_index];
            let age = self.elapsed - shot.impact_at;
            if age >= 0.36 {
                break;
            }
            if shot.outcome.hull_damage == 0 && shot.outcome.shield_damage == 0 {
                continue;
            }
            let source = self.actor_flight_pose(scene, shot.source, shot.launch_at).center;
            lighting.flashes[1] = Some(HullLight {
                position: pose.center + (source - pose.center).normalized() * pose.size * 0.26,
                radius: pose.size * 0.90,
                color: if shot.outcome.shield_damage > 0 {
                    BLUE
                } else {
                    GOLD
                },
                strength: (1.0 - age / 0.36).powi(2) * 0.85,
            });
            break;
        }
        lighting
    }

    /// Light is sampled through the same texture, preserving transparent margins and hull holes.
    pub(super) fn paint_hull_illumination(
        &self,
        painter: &Painter,
        scene: Scene,
        index: usize,
        pose: ActorPose,
        texture: TextureId,
        dimensions: Vec2,
        uv: Rect,
    ) {
        let lighting = self.hull_lighting(scene, index, pose);
        painter.add(Shape::mesh(lit_hull_mesh(texture, pose, dimensions, uv, &lighting)));
    }

    /// Engine light follows the rendered hull; throttle follows its actual replay trajectory.
    /// Sampling the clock makes pauses and arbitrary seeks reproduce the same exhaust.
    pub(super) fn paint_ship_propulsion(
        &self,
        painter: &Painter,
        scene: Scene,
        index: usize,
        pose: ActorPose,
    ) {
        let Unit::Ship(ship) = self.timeline.actors[index].unit else {
            return;
        };
        let visual = &self.visuals[index];
        let previous = self.actor_flight_pose(scene, index, self.elapsed - 0.12);
        let earlier = self.actor_flight_pose(scene, index, self.elapsed - 0.24);
        // Propulsion reacts to motion that has already happened. A symmetric derivative
        // would fire maneuvering jets before a future recorded hull impact arrived.
        let velocity = (pose.center - previous.center) / 0.12;
        let acceleration =
            ((pose.center - previous.center) - (previous.center - earlier.center)) / (0.12 * 0.12);
        let history = self.actor_pose(scene, index, self.elapsed - 0.22);
        let speed = velocity.length() / scene.scale.max(0.01);
        let throttle = (speed / 150.0).clamp(0.0, 1.0);
        let axis = rotate(
            Vec2::angled(visual.art_heading)
                * vec2(
                    if pose.mirror {
                        -1.0
                    } else {
                        1.0
                    },
                    1.0,
                ),
            pose.angle,
        );
        let base_dimensions = visual.firing_sheet.map_or_else(
            || vec2(pose.size * visual.aspect.min(1.0), pose.size / visual.aspect.max(1.0)),
            |sheet| sheet.dimensions(pose.size),
        );
        let dimensions = self.hull_dimensions(scene, index, pose, self.elapsed, base_dimensions);
        let history_dimensions =
            self.hull_dimensions(scene, index, history, self.elapsed - 0.22, base_dimensions);
        let color = engine_color(visual.owner_color);
        let pulse = 0.94 + 0.06 * (self.elapsed * 8.0 + visual.phase).sin();
        let large = matches!(ship, Ship::Battleship | Ship::Dreadnought | Ship::WarSun);
        let length = pose.size
            * (0.13
                + throttle
                    * if large {
                        0.28
                    } else {
                        0.55
                    })
            * pulse;
        let radius = pose.size
            * if large {
                0.028
            } else {
                0.040
            };
        for (nozzle, &port) in engine_ports(ship).iter().enumerate() {
            let origin = engine_attachment(pose, dimensions, port);
            let emitted = engine_attachment(history, history_dimensions, port) - origin;
            // The short wake remembers where this nozzle was, including the turn of its hull.
            // Only lateral history curves it; the jet itself continues to leave the nozzle aft.
            let lateral_history = emitted - axis * emitted.dot(axis);
            let bend =
                lateral_history * (length * 0.65 / lateral_history.length().max(0.01)).min(1.0);
            let variation =
                0.94 + 0.06 * (self.elapsed * 10.0 + nozzle as f32 * 1.7 + visual.phase).sin();
            painter.add(Shape::mesh(engine_plume_mesh(
                origin,
                axis,
                length * variation,
                radius,
                bend,
                color,
                0.58 + throttle * 0.32,
            )));
            glow(painter, origin, radius * 2.5, color, 0.6 + throttle * 0.35);
        }
        let (turning, braking) = maneuver_thrust(velocity, acceleration, scene.scale);
        let normal = vec2(-axis.y, axis.x);
        if turning.abs() > 0.04 {
            let reflection = if pose.mirror {
                -1.0
            } else {
                1.0
            };
            let port = (0.67, 0.50 - turning.signum() * reflection * 0.19);
            let origin = engine_attachment(pose, dimensions, port);
            painter.add(Shape::mesh(engine_plume_mesh(
                origin,
                normal * turning.signum(),
                pose.size * 0.14,
                pose.size * 0.018,
                Vec2::ZERO,
                BLUE,
                turning.abs() * 0.85,
            )));
        }
        if braking > 0.04 {
            for port in [(0.82, 0.40), (0.82, 0.63)] {
                let origin = engine_attachment(pose, dimensions, port);
                painter.add(Shape::mesh(engine_plume_mesh(
                    origin,
                    -axis,
                    pose.size * 0.11,
                    pose.size * 0.016,
                    Vec2::ZERO,
                    BLUE,
                    braking * 0.72,
                )));
            }
        }
    }

    pub(super) fn aim_ship_cannon(
        &self,
        scene: Scene,
        index: usize,
        time: f32,
        pose: &mut ActorPose,
    ) {
        let visual = &self.visuals[index];
        if visual.firing_sheet.is_none() {
            return;
        }
        let unit = self.timeline.actors[index].unit;
        let horizon = (1.05 + unit_size(unit) / 300.0).min(1.85);
        let first = visual.firing_times.partition_point(|launch| *launch < time - horizon);
        let end = visual.firing_times.partition_point(|launch| *launch <= time + horizon);
        if first == end {
            return;
        }
        let sign = if pose.mirror {
            -1.0
        } else {
            1.0
        };
        let mut bias = 0.0;
        let mut total = 0.0_f32;
        for slot in first..end {
            let shot = &self.timeline.shots[visual.firing_shots[slot]];
            let target = shot.target.map_or(scene.planet, |target| {
                self.actor_course_pose(scene, target, shot.impact_at).center
            });
            let direction = target - pose.center;
            let aim = sign * direction.y.atan2(direction.x.abs());
            let delta = (aim - pose.angle).sin().atan2((aim - pose.angle).cos());
            let phase = (time - shot.launch_at) / horizon;
            let weight = (1.0 - phase * phase).max(0.0).powi(3);
            bias += (delta / 0.7).tanh() * weight;
            total += weight;
        }
        // Ships retain their travel attitude. A small, mass-sensitive firing suggestion is
        // averaged over neighboring salvos, so alternating high/low targets cannot wrench a
        // complete hull from one elevation to the other. Projectiles still leave the actual
        // rendered muzzle and travel to the exact recorded target, including off-axis shots.
        // This compact temporal kernel has no frame state and no jumps at its endpoints.
        pose.angle += ship_aim_limit(unit) * bias / total.max(1.0);
    }

    pub(super) fn aim_planet_cannon(
        &self,
        index: usize,
        time: f32,
        target: Pos2,
        pose: &mut ActorPose,
    ) {
        let Some(sheet) = self.visuals[index].firing_sheet else {
            return;
        };
        let offset = (sheet.muzzle(self.firing_frame(index, time)) - pos2(0.5, 0.5))
            * sheet.dimensions(pose.size);
        let direction = target - pose.center;
        pose.mirror = false;
        // Align the barrel axis, accounting for its offset above the image center.
        pose.angle =
            direction.angle() - (offset.y / direction.length().max(1.0)).clamp(-1.0, 1.0).asin();
    }

    fn firing_shot(&self, index: usize, time: f32) -> Option<&CinematicShot> {
        let visual = &self.visuals[index];
        let end = visual.firing_times.partition_point(|at| *at <= time + 0.16);
        end.checked_sub(1).map(|slot| &self.timeline.shots[visual.firing_shots[slot]])
    }

    fn firing_frame(&self, index: usize, time: f32) -> usize {
        for attack in &self.timeline.planet_attacks {
            if attack.sources.contains(&index) && time >= attack.start_at && time <= attack.end_at {
                return if time < attack.start_at + super::super::effects::DEATH_RAY_FOCUS_AT {
                    1
                } else {
                    2
                };
            }
        }
        self.firing_shot(index, time).map_or(0, |shot| release_frame(time - shot.launch_at))
    }

    fn turret_target(&self, scene: Scene, index: usize, time: f32) -> Pos2 {
        for attack in &self.timeline.planet_attacks {
            if attack.sources.contains(&index) && (attack.start_at..=attack.end_at).contains(&time)
            {
                return self.planet_attack_focus(scene, attack);
            }
        }
        let visual = &self.visuals[index];
        let released = visual.firing_times.partition_point(|at| *at <= time);
        let target = |slot: usize| {
            let shot = &self.timeline.shots[visual.firing_shots[slot]];
            shot.target.map_or(scene.planet, |target| {
                self.actor_flight_pose(scene, target, shot.impact_at).center
            })
        };
        if visual.firing_shots.is_empty() {
            let pose = self.actor_pose(scene, index, time);
            return pose.center
                + rotate(
                    vec2(
                        if pose.mirror {
                            -500.0
                        } else {
                            500.0
                        },
                        0.0,
                    ),
                    pose.angle,
                );
        }
        if released == 0 {
            return target(0);
        }
        let slot = released - 1;
        let Some(&next_launch) = visual.firing_times.get(released) else {
            return target(slot);
        };
        let launch = visual.firing_times[slot];
        let gap = (next_launch - launch).max(0.001);
        // Finish the present release before leading the next. Even dense salvos pass through
        // each exact target at its recorded launch instead of skipping it via look-ahead.
        let hold = (gap * 0.35).min(0.32);
        let lead = (unit_size(self.timeline.actors[index].unit) / 200.0).clamp(0.45, 0.95);
        let begin = (next_launch - lead).max(launch + hold);
        let blend = smooth(((time - begin) / (next_launch - begin).max(0.001)).clamp(0.0, 1.0));
        target(slot).lerp(target(released), blend)
    }

    fn gun_angle(&self, pose: ActorPose, sheet: FiringSheet, target: Pos2) -> f32 {
        if !sheet.ground {
            return pose.angle;
        }
        let reflection = vec2(
            if pose.mirror {
                -1.0
            } else {
                1.0
            },
            1.0,
        );
        let pivot = pose.center
            + rotate(
                (sheet.pivot - pos2(0.5, 0.5)) * sheet.dimensions(pose.size) * reflection,
                pose.angle,
            );
        let heading = (target - pivot).angle()
            - if pose.mirror {
                PI
            } else {
                0.0
            };
        // Physical elevation stops prevent the assembly folding back through its pedestal.
        heading.sin().atan2(heading.cos()).clamp(-0.85, 0.85)
    }

    pub(super) fn actor_muzzle(&self, scene: Scene, index: usize, time: f32, target: Pos2) -> Pos2 {
        let pose = self.actor_pose(scene, index, time);
        let Some(sheet) = self.visuals[index].firing_sheet else {
            return weapon_muzzle(pose, self.visuals[index].ground, target);
        };
        let reflection = vec2(
            if pose.mirror {
                -1.0
            } else {
                1.0
            },
            1.0,
        );
        let dimensions = if sheet.ground {
            sheet.dimensions(pose.size)
        } else {
            self.hull_dimensions(scene, index, pose, time, sheet.dimensions(pose.size))
        };
        let muzzle = sheet.muzzle(self.firing_frame(index, time));
        let pivot = pose.center
            + rotate((sheet.pivot - pos2(0.5, 0.5)) * dimensions * reflection, pose.angle);
        pivot
            + rotate(
                (muzzle - sheet.pivot) * dimensions * reflection,
                self.gun_angle(pose, sheet, self.turret_target(scene, index, time)),
            )
    }

    pub(super) fn paint_firing_sprite(
        &self,
        painter: &Painter,
        scene: Scene,
        index: usize,
        pose: ActorPose,
        sheet: FiringSheet,
        texture: TextureId,
    ) {
        let frame = self.firing_frame(index, self.elapsed);
        // Ground artwork owns a rigid foundation. Decide this from the rendered sheet,
        // independently of any flight model used to preview or aim the firing cycle.
        let dimensions = if sheet.ground {
            sheet.dimensions(pose.size)
        } else {
            self.hull_dimensions(scene, index, pose, self.elapsed, sheet.dimensions(pose.size))
        };
        let weapon = sheet.weapon;
        let reflection = vec2(
            if pose.mirror {
                -1.0
            } else {
                1.0
            },
            1.0,
        );
        if !sheet.ground {
            // The ring, docking arms and hull form one unbroken rigid body. Never
            // rotate rectangular slices through their structural supports.
            paint_owner_hull(
                painter,
                texture,
                pose.center,
                dimensions,
                pose.angle,
                pose.mirror,
                sheet.uv(frame, FULL_UV),
                self.visuals[index].owner_color,
            );
            self.paint_hull_illumination(
                painter,
                scene,
                index,
                pose,
                texture,
                dimensions,
                sheet.uv(frame, FULL_UV),
            );
            return;
        }
        let angle = self.gun_angle(pose, sheet, self.turret_target(scene, index, self.elapsed));
        let pivot = pose.center
            + rotate((sheet.pivot - pos2(0.5, 0.5)) * dimensions * reflection, pose.angle);
        let resting = |point: Pos2| {
            pose.center + rotate((point - pos2(0.5, 0.5)) * dimensions * reflection, pose.angle)
        };
        let aimed =
            |point: Pos2| pivot + rotate((point - sheet.pivot) * dimensions * reflection, angle);
        // One continuous joint connects the rigid gun to the stationary pedestal.
        // Shared edge vertices cannot open holes like independently cut rectangles.
        let shoulder = weapon.bottom();
        let foundation = 0.70;
        let mut mesh = Mesh::with_texture(texture);
        let edge = |row: usize| {
            if row == 0 {
                0.0
            } else if row <= 8 {
                shoulder + (foundation - shoulder) * (row - 1) as f32 / 7.0
            } else {
                1.0
            }
        };
        for band in 0..9 {
            let uv = frame_uv(
                if band == 0 {
                    frame
                } else {
                    0
                },
                FULL_UV,
            );
            let start = mesh.vertices.len() as u32;
            for row in 0..=1 {
                let y = edge(band + row);
                let weight = (1.0 - (y - shoulder) / (foundation - shoulder)).clamp(0.0, 1.0);
                for col in 0..=4 {
                    let point = pos2(col as f32 / 4.0, y);
                    mesh.vertices.push(Vertex {
                        pos: resting(point).lerp(aimed(point), weight),
                        uv: uv.min + point.to_vec2() * uv.size(),
                        color: self.visuals[index].owner_color.map_or(Color32::WHITE, |color| {
                            // Fixed pedestal armor carries the owner's strongest paint;
                            // the barrel keeps its metal highlights and muzzle flash.
                            hull_tint(
                                color,
                                if band >= 8 {
                                    0.52
                                } else {
                                    0.08
                                },
                            )
                        }),
                    });
                    if row > 0 && col > 0 {
                        let i = start + (row * 5 + col) as u32;
                        mesh.indices.extend_from_slice(&[i - 6, i - 5, i, i - 6, i, i - 1]);
                    }
                }
            }
        }
        let lighting = self.hull_lighting(scene, index, pose);
        let mut illuminated = mesh.clone();
        for vertex in &mut illuminated.vertices {
            vertex.color = lighting.color_at(vertex.pos);
        }
        painter.add(Shape::mesh(mesh));
        painter.add(Shape::mesh(illuminated));
    }
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_sprites.rs"]
mod tests;
