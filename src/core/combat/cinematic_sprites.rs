//! Recorded firing cycles, attachment points and separately aimed turret artwork.

use super::*;

#[derive(Clone, Copy)]
pub(super) struct FiringSheet {
    pub texture: &'static str,
    pub aspect: f32,
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
        Unit::Ship(Ship::WarSun) => (
            "firing war sun",
            0.460,
            [0.970, 0.970, 0.830, 0.650, 0.800, 0.900, 0.940, 0.970],
            0.630,
            0.350,
            0.620,
        ),
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

impl CinematicPlayback {
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
        let end = visual.firing_times.partition_point(|at| *at <= time + 0.60);
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
        let slot = end.saturating_sub(1);
        if slot == 0 {
            return target(0);
        }
        let launch = visual.firing_times[slot];
        let lead = (launch - visual.firing_times[slot - 1]).clamp(0.01, 0.60);
        target(slot - 1).lerp(target(slot), smooth(((time - launch + lead) / lead).clamp(0.0, 1.0)))
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
        let dimensions = sheet.dimensions(pose.size);
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
        let dimensions = sheet.dimensions(pose.size);
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
            rotated_image(
                painter,
                texture,
                pose.center,
                dimensions,
                pose.angle,
                pose.mirror,
                frame_uv(frame, FULL_UV),
                Color32::WHITE,
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
                        color: Color32::WHITE,
                    });
                    if row > 0 && col > 0 {
                        let i = start + (row * 5 + col) as u32;
                        mesh.indices.extend_from_slice(&[i - 6, i - 5, i, i - 6, i, i - 1]);
                    }
                }
            }
        }
        painter.add(Shape::mesh(mesh));
    }
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_sprites.rs"]
mod tests;
