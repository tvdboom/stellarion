//! A continuous, decorative camera over the recorded battle; no simulation runs here.

use std::f32::consts::{PI, TAU};

use bevy::prelude::{Alpha, Color, Resource, Vec3 as BevyVec3};
use bevy_egui::egui::{
    epaint::{Mesh, Vertex},
    pos2, vec2, Color32, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2,
};

use super::cinematic_timeline::{CinematicActor, CinematicShot, CinematicTimeline};
use super::effects::{particle_envelope, weapon_trail, Weapon, WeaponFlight, WeaponTrail};
use super::report::{MissionReport, Side};
use crate::core::ui::utils::ImageIds;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::Unit;

const FULL_UV: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
const BLUE: Color32 = Color32::from_rgb(104, 210, 255);
const GOLD: Color32 = Color32::from_rgb(255, 177, 90);
const GREEN: Color32 = Color32::from_rgb(102, 255, 182);

/// The movie has one clock. Engines, stars, debris and camera movement freeze with playback.
#[derive(Resource)]
pub(crate) struct CinematicPlayback {
    pub timeline: CinematicTimeline,
    pub elapsed: f32,
    visuals: Vec<ActorVisual>,
    draw_order: Vec<usize>,
    maximum_shot_lifetime: f32,
    maximum_charge_lead: f32,
    repair_order: Vec<usize>,
    maximum_repair_lifetime: f32,
    planet_image: String,
    show_planet: bool,
}

struct ActorVisual {
    texture: String,
    fallback_texture: String,
    aspect: f32,
    art_rotation: f32,
    home: Vec2,
    size: f32,
    phase: f32,
    ground: bool,
    orbital: bool,
    firing_times: Vec<f32>,
}

#[derive(Clone, Copy)]
struct ActorPose {
    center: Pos2,
    size: f32,
    angle: f32,
    mirror: bool,
}

/// Coordinates are viewport-relative; texture sizes share one scale to preserve proportions.
#[derive(Clone, Copy)]
struct Scene {
    rect: Rect,
    scale: f32,
    planet: Pos2,
    planet_radius: f32,
}

impl Scene {
    fn new(rect: Rect) -> Self {
        Self {
            rect,
            scale: (rect.width() / 1440.0).min(rect.height() / 820.0),
            planet: pos2(rect.left() + rect.width() * 0.80, rect.top() + rect.height() * 0.80),
            planet_radius: rect.height().min(rect.width() * 0.78) * 0.32,
        }
    }

    fn point(self, p: Vec2) -> Pos2 {
        self.rect.min + self.rect.size() * p
    }
}

impl CinematicPlayback {
    pub fn new(report: &MissionReport) -> Self {
        let timeline = CinematicTimeline::new(report);
        let mut counts = [0_usize; 4];
        for actor in &timeline.actors {
            counts[formation_group(actor)] += 1;
        }
        let fleet_count = counts[0].max(counts[1]).max(counts[3]);
        let mut slots = [0_usize; 4];
        let mut visuals = Vec::with_capacity(timeline.actors.len());
        for (index, actor) in timeline.actors.iter().enumerate() {
            let group = formation_group(actor);
            let slot = slots[group];
            slots[group] += 1;
            let name = actor.unit.to_lowername();
            let texture = if actor.unit.is_fauna() {
                format!("map {name}")
            } else {
                format!("cinematic {name}")
            };
            // Both fleets and orbitals share a scale: a fighter must not grow just because its
            // side fields fewer ships. Surface equipment uses its own available ground area.
            let density_count = if group == 2 {
                counts[2]
            } else {
                fleet_count
            };
            let density = (9.0 / density_count.max(9) as f32).sqrt().max(0.11);
            visuals.push(ActorVisual {
                texture,
                fallback_texture: name,
                aspect: sprite_aspect(actor.unit),
                art_rotation: sprite_rotation(actor.unit),
                home: formation_home(group, slot, counts[group]),
                size: unit_size(actor.unit) * density,
                phase: {
                    let identity = actor.id.unwrap_or(index as u64);
                    noise((identity ^ (identity >> 32)) as u32) * TAU
                        + actor.owner.unwrap_or(0) as f32 * 0.07
                },
                ground: group == 2,
                orbital: group == 3,
                firing_times: Vec::new(),
            });
        }
        for shot in &timeline.shots {
            visuals[shot.source].firing_times.push(shot.launch_at);
        }
        let mut draw_order: Vec<_> = (0..visuals.len()).collect();
        // Surface equipment lies beneath orbiting hulls; within a layer the foreground wins.
        draw_order.sort_by(|a, b| {
            visuals[*b]
                .ground
                .cmp(&visuals[*a].ground)
                .then_with(|| visuals[*a].home.y.total_cmp(&visuals[*b].home.y))
        });
        let maximum_shot_lifetime = timeline
            .shots
            .iter()
            .map(|shot| {
                let weapon = Weapon::for_shot(timeline.actors[shot.source].unit, &shot.outcome);
                shot.impact_at - shot.launch_at + shot_tail(weapon, shot)
            })
            .fold(0.0_f32, f32::max);
        let maximum_charge_lead = timeline
            .shots
            .iter()
            .map(|shot| {
                let weapon = Weapon::for_shot(timeline.actors[shot.source].unit, &shot.outcome);
                shot.launch_at - weapon.cinematic_charge_start(shot.launch_at, shot.impact_at)
            })
            .fold(0.0_f32, f32::max);
        let mut repair_order: Vec<_> = (0..timeline.repairs.len()).collect();
        repair_order.sort_by(|a, b| {
            timeline.repairs[*a].start_at.total_cmp(&timeline.repairs[*b].start_at)
        });
        let maximum_repair_lifetime = timeline
            .repairs
            .iter()
            .map(|repair| repair.end_at - repair.start_at)
            .fold(0.0_f32, f32::max);
        Self {
            timeline,
            elapsed: 0.0,
            visuals,
            draw_order,
            maximum_shot_lifetime,
            maximum_charge_lead,
            repair_order,
            maximum_repair_lifetime,
            planet_image: report.planet.image(),
            show_planet: !report.is_space_fauna_encounter(),
        }
    }

    pub fn advance(&mut self, delta_seconds: f32, speed: f32, paused: bool) {
        if !paused && delta_seconds.is_finite() && speed.is_finite() {
            self.elapsed = (self.elapsed + delta_seconds.max(0.0) * speed.max(0.0))
                .min(self.timeline.duration);
        }
    }

    pub fn is_finished(&self) -> bool {
        self.elapsed >= self.timeline.duration
    }

    /// Records loaded image dimensions once; native and browser textures share the same layout.
    pub fn set_sprite_size(&mut self, texture: &str, width: u32, height: u32) {
        if width == 0 || height == 0 {
            return;
        }
        let aspect = width as f32 / height as f32;
        for visual in &mut self.visuals {
            if visual.texture == texture {
                visual.aspect = aspect;
            }
        }
    }

    /// Paints behind the playback controls. Every visible hit, wreck and repair comes from a record.
    pub fn paint(&self, painter: &Painter, rect: Rect, images: &ImageIds) {
        if rect.width() < 1.0 || rect.height() < 1.0 {
            return;
        }
        let painter = painter.with_clip_rect(rect);
        let scene = Scene::new(rect);
        self.paint_space(&painter, scene, images);
        if self.show_planet {
            self.paint_planet(&painter, scene, images);
        }
        for &index in &self.draw_order {
            self.paint_actor(&painter, scene, images, index);
        }
        let first_repair = self.repair_order.partition_point(|index| {
            self.timeline.repairs[*index].start_at
                < self.elapsed - self.maximum_repair_lifetime - 0.4
        });
        let last_repair = self
            .repair_order
            .partition_point(|index| self.timeline.repairs[*index].start_at <= self.elapsed);
        for &index in &self.repair_order[first_repair..last_repair] {
            let repair = &self.timeline.repairs[index];
            if self.elapsed < repair.start_at || self.elapsed > repair.end_at + 0.4 {
                continue;
            }
            let target = self.actor_pose(scene, repair.target, self.elapsed);
            let source =
                repair.source.map_or(target.center + vec2(20.0, 25.0) * scene.scale, |index| {
                    self.actor_pose(scene, index, self.elapsed).center
                });
            let progress = ((self.elapsed - repair.start_at)
                / (repair.end_at - repair.start_at).max(0.1))
            .clamp(0.0, 1.0);
            let strength = (progress * PI).sin().abs();
            let drone = source.lerp(target.center, smooth(progress));
            let repair_size = (repair.amount as f32).sqrt().clamp(3.0, 12.0);
            glow(&painter, drone, repair_size * scene.scale, GREEN, strength * 0.45);
            painter.circle_filled(drone, 2.5 * scene.scale, GREEN);
            glow_line(&painter, drone, target.center, 1.0 * scene.scale, GREEN, strength);
            arc(
                &painter,
                target.center,
                target.size * 0.48,
                self.elapsed * 2.0,
                1.5 * PI,
                Stroke::new(1.2 * scene.scale, alpha(GREEN, strength * 0.8)),
            );
            for i in 0..4 {
                let angle = self.elapsed * 2.0 + i as f32 * PI / 2.0;
                let point = target.center + Vec2::angled(angle) * target.size * 0.36;
                painter.line_segment(
                    [point - vec2(2.0, 0.0), point + vec2(2.0, 0.0)],
                    Stroke::new(scene.scale, alpha(GREEN, strength)),
                );
                painter.line_segment(
                    [point - vec2(0.0, 2.0), point + vec2(0.0, 2.0)],
                    Stroke::new(scene.scale, alpha(GREEN, strength)),
                );
            }
        }
        // The schedule overlaps opposing salvos without changing their recorded causal order.
        let first_shot = self
            .timeline
            .shots
            .partition_point(|shot| shot.launch_at < self.elapsed - self.maximum_shot_lifetime);
        let last_shot = self
            .timeline
            .shots
            .partition_point(|shot| shot.launch_at <= self.elapsed + self.maximum_charge_lead);
        for (offset, shot) in self.timeline.shots[first_shot..last_shot].iter().enumerate() {
            let index = first_shot + offset;
            let weapon = Weapon::for_shot(self.timeline.actors[shot.source].unit, &shot.outcome);
            if self.elapsed >= weapon.cinematic_charge_start(shot.launch_at, shot.impact_at)
                && self.elapsed <= shot.impact_at + shot_tail(weapon, shot)
            {
                self.paint_shot(&painter, scene, images, index, shot);
            }
        }
        for (index, actor) in self.timeline.actors.iter().enumerate() {
            if let Some(death) = actor.death_at {
                let age = self.elapsed - death;
                if (0.0..3.6).contains(&age) {
                    let pose = self.actor_pose(scene, index, death);
                    explosion(&painter, images, pose.center, pose.size, age, index as u32);
                }
            }
        }
        self.paint_planet_attacks(&painter, scene, images);
        // Restrained letterbox shading keeps overlaid controls legible without hiding ships.
        let strip = (rect.height() * 0.045).min(28.0);
        for i in 0..8 {
            let strength = (8 - i) as f32 / 8.0;
            let y = strip * i as f32 / 8.0;
            painter.rect_filled(
                Rect::from_min_size(
                    pos2(rect.left(), rect.top() + y),
                    vec2(rect.width(), strip / 8.0),
                ),
                0.0,
                alpha(Color32::BLACK, strength * 0.28),
            );
            painter.rect_filled(
                Rect::from_min_size(
                    pos2(rect.left(), rect.bottom() - y - strip / 8.0),
                    vec2(rect.width(), strip / 8.0),
                ),
                0.0,
                alpha(Color32::BLACK, strength * 0.28),
            );
        }
    }

    fn paint_space(&self, painter: &Painter, scene: Scene, images: &ImageIds) {
        painter.rect_filled(scene.rect, 0.0, Color32::from_rgb(3, 6, 15));
        let camera = vec2((self.elapsed * 0.035).sin(), (self.elapsed * 0.025).cos());
        if let Some(texture) = images.0.get("bg") {
            painter.image(
                *texture,
                scene.rect.expand(30.0).translate(camera * 14.0),
                FULL_UV,
                Color32::from_gray(155),
            );
        }
        if let Some(texture) = images.0.get("nebula") {
            let center = scene.point(vec2(0.49, 0.43)) + camera * 8.0;
            rotated_image(
                painter,
                *texture,
                center,
                vec2(scene.rect.width() * 1.20, scene.rect.height() * 1.65),
                -0.28,
                false,
                FULL_UV,
                Color32::from_rgba_unmultiplied(185, 193, 255, 115),
            );
        }
        // Stable hashes keep the same stars through pausing, resizing, restarting and seeking.
        for i in 0..150_u32 {
            let layer = 0.25 + noise(i + 502) * 0.75;
            let base = vec2(noise(i * 3 + 11), noise(i * 3 + 12));
            let center = scene.point(base) + camera * (12.0 * layer);
            let twinkle = 0.45 + 0.55 * (self.elapsed * (0.45 + layer) + i as f32).sin().powi(2);
            let color = alpha(Color32::from_rgb(182, 220, 255), twinkle * layer);
            let size = (0.5 + layer * 1.2) * scene.scale.max(0.55);
            painter.circle_filled(center, size, color);
            if i % 19 == 0 {
                glow(painter, center, size * 6.0, BLUE, twinkle * 0.22);
                let length = size * (2.0 + twinkle);
                painter.line_segment(
                    [center - vec2(length, 0.0), center + vec2(length, 0.0)],
                    Stroke::new(0.65, alpha(color, 0.8)),
                );
                painter.line_segment(
                    [center - vec2(0.0, length), center + vec2(0.0, length)],
                    Stroke::new(0.65, alpha(color, 0.8)),
                );
            }
        }
        for i in 0..3_u32 {
            let progress = ((self.elapsed + i as f32 * 6.7) % 23.0) / 2.8;
            if progress > 1.0 {
                continue;
            }
            let head =
                scene.point(vec2(-0.1 + progress * 1.3, 0.07 + i as f32 * 0.19 + progress * 0.18));
            let tail = head - vec2(110.0, 25.0) * scene.scale;
            let strength = (progress * PI).sin() * 0.65;
            for part in 0..8 {
                let fraction = part as f32 / 8.0;
                painter.line_segment(
                    [tail.lerp(head, fraction), tail.lerp(head, fraction + 0.125)],
                    Stroke::new(
                        (0.4 + fraction * 1.1) * scene.scale,
                        alpha(BLUE, strength * fraction),
                    ),
                );
            }
            glow(painter, head, 5.0 * scene.scale, BLUE, strength);
        }
    }

    fn paint_planet(&self, painter: &Painter, scene: Scene, images: &ImageIds) {
        let destroyed_at = self
            .timeline
            .planet_attacks
            .iter()
            .find(|attack| attack.destroyed)
            .map(|attack| attack.end_at);
        let destruction =
            destroyed_at.map_or(0.0, |at| ((self.elapsed - at) / 1.6).clamp(0.0, 1.0));
        let radius = scene.planet_radius;
        for ring in (1..=12).rev() {
            painter.circle_stroke(
                scene.planet,
                radius + ring as f32 * 1.6 * scene.scale,
                Stroke::new(
                    2.1 * scene.scale,
                    alpha(BLUE, (1.0 - destruction) * (0.08 - ring as f32 * 0.005)),
                ),
            );
        }
        let texture = if destruction > 0.95 {
            images.0.get("planet0").or_else(|| images.0.get(&self.planet_image))
        } else {
            images.0.get(&self.planet_image)
        };
        if let Some(texture) = texture {
            painter.image(
                *texture,
                Rect::from_center_size(scene.planet, Vec2::splat(radius * 2.0)),
                FULL_UV,
                Color32::from_gray((220.0 - destruction * 90.0) as u8),
            );
        } else {
            painter.circle_filled(scene.planet, radius, Color32::from_rgb(20, 41, 56));
        }
        planet_terminator(painter, scene.planet, radius);
        for ring in 0..5 {
            arc(
                painter,
                scene.planet,
                radius + ring as f32 * scene.scale,
                -3.65,
                2.7,
                Stroke::new(
                    1.8 * scene.scale,
                    alpha(BLUE, (1.0 - destruction) * (0.12 - ring as f32 * 0.018)),
                ),
            );
        }
        let shield = self.timeline.planetary_shield_at(self.elapsed);
        if shield > 0 {
            let ratio = (shield as f32 / self.timeline.initial_planetary_shield.max(1) as f32)
                .clamp(0.0, 1.0);
            // The strategic map's filament artwork supplies the same electromagnetic field.
            // Its three-second breath and two slow counterflows are sampled from the movie
            // clock, so pausing, speeding up and replaying also control the entire shield.
            let pulse = 0.5 - 0.5 * (TAU * self.elapsed / 3.0).cos();
            let strength = (0.25 + ratio * 0.75) * (1.0 - destruction);
            let color = Color32::from_rgb(62, 171, 250);
            painter.circle_filled(
                scene.planet,
                radius * 1.065,
                alpha(color, strength * (0.025 + pulse * 0.035)),
            );
            if let Some(texture) = images.0.get("planetary shield marker") {
                for (rotation, opacity, diameter) in [
                    (self.elapsed * 0.14, 0.32 + pulse * 0.48, 2.38),
                    (-self.elapsed * 0.09 + 1.8, 0.12 + (1.0 - pulse) * 0.16, 2.34),
                ] {
                    rotated_image(
                        painter,
                        *texture,
                        scene.planet,
                        Vec2::splat(radius * diameter),
                        rotation,
                        false,
                        FULL_UV,
                        alpha(color, strength * opacity),
                    );
                }
            }
            // Energy sweeps travel across the near hemisphere, fading at both poles rather
            // than snapping back. The delicate surface filaments leave turrets readable.
            for i in 0..6 {
                let phase = (self.elapsed * 0.11 + i as f32 / 6.0).fract();
                let offset = phase * 2.0 - 1.0;
                let width = radius * (1.0 - offset * offset).sqrt() * 1.065;
                let points: Vec<_> = (0..=36)
                    .map(|step| {
                        let angle = PI + step as f32 / 36.0 * PI;
                        let ripple = (angle * 12.0 + self.elapsed * 2.2 + i as f32).sin()
                            * radius
                            * 0.007
                            * (angle - PI).sin();
                        scene.planet
                            + vec2(
                                angle.cos() * width,
                                angle.sin() * width * 0.28 + offset * radius + ripple,
                            )
                    })
                    .collect();
                painter.add(Shape::line(
                    points,
                    Stroke::new(
                        0.8 * scene.scale,
                        alpha(color, strength * (phase * PI).sin() * (0.10 + pulse * 0.12)),
                    ),
                ));
            }
        }
    }

    fn actor_pose(&self, scene: Scene, index: usize, time: f32) -> ActorPose {
        let actor = &self.timeline.actors[index];
        let visual = &self.visuals[index];
        let mirror = actor.side == Side::Defender;
        let direction = if mirror {
            -1.0
        } else {
            1.0
        };
        let phase = visual.phase;
        let size = visual.size * scene.scale;
        let mut center;
        let mut angle = 0.0;
        if visual.ground {
            // Home positions live on the visible globe, so turrets never float in open space.
            center = scene.planet + visual.home * scene.planet_radius;
            if actor.unit == Unit::repair_truck() || actor.unit == Unit::crawler() {
                center += vec2((time * 0.32 + phase).sin() * 3.0, (time * 0.38 + phase).cos())
                    * scene.scale;
            }
        } else {
            center = scene.point(visual.home);
            let agility = if visual.orbital {
                0.2
            } else {
                (85.0 / visual.size.max(20.0)).clamp(0.4, 2.0)
            };
            let flight_time = (time - self.timeline.entrance_duration).max(0.0);
            let amplitude = if visual.orbital {
                6.0
            } else {
                28.0
            } * scene.scale;
            center += vec2(
                (flight_time * 0.22 * agility + phase).sin() * amplitude,
                (flight_time * 0.33 * agility + phase).cos() * amplitude * 0.7,
            );
            if !visual.orbital {
                center.x += direction * 60.0 * scene.scale * agility * smooth(flight_time / 10.0);
                // Each ship eases in at its own speed, then continues a banking flight path.
                let delay = noise(index as u32 + 141) * 0.35;
                let entered =
                    if actor.retreat_at.is_some_and(|at| at < self.timeline.entrance_duration) {
                        // Immediate withdrawals begin at the defended world. Flying in first would
                        // leave their entire outward departure hidden beyond the viewport edge.
                        1.0
                    } else {
                        smooth((time - delay) / (self.timeline.entrance_duration - 0.35).max(0.1))
                    };
                center.x -= direction * (1.0 - entered) * scene.rect.width() * 0.65;
                center.y += (1.0 - entered) * size * 0.20;
                angle = (flight_time * 0.33 * agility + phase).sin() * 0.065 * agility;
            }
        }
        if let Some(retreat_at) = actor.retreat_at {
            let retreat = ((time - retreat_at) / 1.65).clamp(0.0, 1.0);
            center.x -= direction * retreat.powi(2) * scene.rect.width();
            center.y -= retreat * retreat * scene.rect.height() * 0.2;
            angle -= direction * retreat * 0.25;
        }
        let fired = visual.firing_times.partition_point(|at| *at <= time);
        if let Some(last) = fired.checked_sub(1).map(|index| visual.firing_times[index]) {
            let recoil = ((time - last) / 0.28).clamp(0.0, 1.0);
            let impulse = (recoil * PI).sin().max(0.0);
            center += vec2(-direction * 2.8, 0.7) * impulse * scene.scale;
            angle += direction * impulse * 0.018;
        }
        ActorPose {
            center,
            size,
            angle,
            mirror,
        }
    }

    fn paint_actor(&self, painter: &Painter, scene: Scene, images: &ImageIds, index: usize) {
        let actor = &self.timeline.actors[index];
        if actor.death_at.is_some_and(|at| self.elapsed >= at)
            || actor.retreat_at.is_some_and(|at| self.elapsed >= at + 1.7)
        {
            return;
        }
        let visual = &self.visuals[index];
        let pose = self.actor_pose(scene, index, self.elapsed);
        if !scene.rect.expand(pose.size).contains(pose.center) {
            return;
        }
        let side_color = if pose.mirror {
            GOLD
        } else {
            BLUE
        };
        let direction = if pose.mirror {
            -1.0
        } else {
            1.0
        };
        let state = actor.state_at(self.elapsed);
        let health = state.hull as f32 / actor.max_hull.max(1) as f32;
        let shield_strength = state.shield as f32 / actor.max_shield.max(1) as f32;
        if visual.ground {
            painter.add(Shape::ellipse_filled(
                pose.center + vec2(0.0, pose.size * 0.25),
                vec2(pose.size * 0.34, pose.size * 0.12),
                Color32::from_black_alpha(95),
            ));
        } else if !visual.orbital && !actor.unit.is_fauna() {
            let engine =
                pose.center + rotate(vec2(-direction * 0.26, 0.16) * pose.size, pose.angle);
            let thrust = (self.elapsed * 18.0 + visual.phase).sin() * 0.10 + 0.9;
            let entering = self.elapsed < self.timeline.entrance_duration;
            let fleeing = actor.retreat_at.is_some_and(|at| self.elapsed > at);
            let length = pose.size
                * if entering || fleeing {
                    0.55
                } else {
                    0.23
                }
                * thrust;
            let tail = engine + rotate(vec2(-direction, 0.38) * length, pose.angle);
            exhaust(painter, engine, tail, pose.size * 0.06);
            glow(painter, engine, pose.size * 0.18, BLUE, 0.35);
        }
        let cinematic_texture = images.0.get(&visual.texture);
        let texture = cinematic_texture.or_else(|| images.0.get(&visual.fallback_texture));
        if let Some(texture) = texture {
            let aspect = if cinematic_texture.is_some() {
                visual.aspect
            } else {
                1.0
            };
            let correction = if cinematic_texture.is_some() {
                visual.art_rotation
                    * if pose.mirror {
                        -1.0
                    } else {
                        1.0
                    }
            } else {
                0.0
            };
            rotated_image(
                painter,
                *texture,
                pose.center,
                vec2(pose.size * aspect.min(1.0), pose.size / aspect.max(1.0)),
                pose.angle + correction,
                pose.mirror,
                FULL_UV,
                Color32::WHITE,
            );
        }
        if !visual.ground && state.shield > 0 {
            let toward_enemy = if pose.mirror {
                PI
            } else {
                0.0
            };
            arc(
                painter,
                pose.center,
                pose.size * 0.40,
                toward_enemy - 0.42,
                0.84,
                Stroke::new(scene.scale, alpha(BLUE, shield_strength * 0.10)),
            );
        }
        // Navigation lights and engine pulses make even anchored orbitals feel inhabited.
        let beacon = pose.center + rotate(vec2(direction * 0.18, -0.08) * pose.size, pose.angle);
        let blink = 0.35 + 0.65 * (self.elapsed * 2.1 + visual.phase).sin().powi(8);
        glow(painter, beacon, (pose.size * 0.04).max(1.2), side_color, blink * 0.72);
        if health < 0.52 && state.hull > 0 {
            let damage = (0.6 - health).max(0.0);
            let scar = pose.center + vec2(-0.12, 0.04) * pose.size;
            glow(
                painter,
                scar,
                pose.size * 0.16,
                GOLD,
                damage * (0.7 + (self.elapsed * 12.0).sin() * 0.2),
            );
            for part in 0..4 {
                let age = (self.elapsed * 0.7 + part as f32 * 0.25 + visual.phase).fract();
                let smoke = scar + vec2(-direction * 0.22, -0.24) * pose.size * age;
                painter.circle_filled(
                    smoke,
                    pose.size * (0.035 + age * 0.045),
                    Color32::from_black_alpha((damage * (1.0 - age) * 70.0) as u8),
                );
            }
        }
    }

    fn shot_geometry(&self, scene: Scene, index: usize, shot: &CinematicShot) -> (Pos2, Pos2, f32) {
        let source = self.actor_pose(scene, shot.source, shot.launch_at);
        let target = shot.target.map(|id| self.actor_pose(scene, id, shot.impact_at));
        let mut end = target.map_or_else(
            || {
                scene.planet
                    + vec2(-0.55 + noise(index as u32) * 0.4, -0.55 + noise(index as u32 + 1) * 0.6)
                        * scene.planet_radius
            },
            |pose| pose.center,
        );
        let direction = (end - source.center).normalized();
        let start = source.center + direction * source.size * 0.23;
        if shot.outcome.planetary_shield_damage > 0 && self.show_planet {
            end =
                sphere_entry(start, end, scene.planet, scene.planet_radius * 1.065).unwrap_or(end);
        } else if shot.outcome.shield_damage > 0 {
            end -= direction * target.map_or(14.0, |pose| pose.size * 0.43);
        }
        if shot.outcome.missed
            && shot.outcome.shield_damage == 0
            && shot.outcome.planetary_shield_damage == 0
        {
            let perpendicular = vec2(-direction.y, direction.x);
            end += perpendicular
                * target.map_or(25.0 * scene.scale, |pose| pose.size * 0.75)
                * if index.is_multiple_of(2) {
                    1.0
                } else {
                    -1.0
                };
        }
        (start, end, target.map_or(35.0 * scene.scale, |pose| pose.size))
    }

    fn paint_shot(
        &self,
        painter: &Painter,
        scene: Scene,
        images: &ImageIds,
        index: usize,
        shot: &CinematicShot,
    ) {
        let (start, end, target_size) = self.shot_geometry(scene, index, shot);
        let source = &self.timeline.actors[shot.source];
        let weapon = Weapon::for_shot(source.unit, &shot.outcome);
        let color = weapon_color(weapon.color());
        let release = shot.launch_at;
        let charge_start = weapon.cinematic_charge_start(shot.launch_at, shot.impact_at);
        let stretch = ((shot.impact_at - release) / weapon.flight()).max(0.001);
        let age = (self.elapsed - release) / stretch;
        let progress = (age / weapon.flight()).clamp(0.0, 1.0);
        // Physical size is shared across a salvo, never scaled by damage or faction.
        let source_pose = self.actor_pose(scene, shot.source, shot.launch_at);
        let size = (source_pose.size * 0.55).clamp(26.0 * scene.scale, 100.0 * scene.scale);
        let flight = WeaponFlight {
            weapon,
            origin: BevyVec3::new(start.x, start.y, 0.0),
            destination: BevyVec3::new(end.x, end.y, 0.0),
            size,
            lane: (index % 3) as f32 - 1.0,
        };
        if self.elapsed < release && weapon.charge() > 0.0 {
            // The charging field follows its ship; the released projectile keeps its saved pose.
            let charging_pose = self.actor_pose(scene, shot.source, self.elapsed);
            let charging_muzzle = charging_pose.center
                + (end - charging_pose.center).normalized() * charging_pose.size * 0.23;
            let charging_origin = BevyVec3::new(charging_muzzle.x, charging_muzzle.y, 0.0);
            let charged =
                ((self.elapsed - charge_start) / (release - charge_start)).clamp(0.0, 1.0);
            let radius = weapon.charge_radius(size);
            paint_trail(
                painter,
                images,
                WeaponTrail::Ring(charging_origin, radius, weapon.color().with_alpha(0.6), 1.0),
                charged,
                index as u32,
                scene.scale,
            );
            paint_trail(
                painter,
                images,
                WeaponTrail::Glow(charging_origin, radius * 0.6, weapon.color(), 1.0),
                charged,
                index as u32,
                scene.scale,
            );
            if weapon.massive() {
                for spark in 0..8 {
                    let angle = spark as f32 * TAU / 8.0;
                    let point =
                        charging_muzzle + Vec2::angled(angle) * radius * 0.5 * (1.0 - charged);
                    paint_effect_sprite(
                        painter,
                        images,
                        "combat fx glow",
                        point,
                        Vec2::splat(size * (0.07 + 0.05 * smooth(charged))),
                        0.0,
                        weapon_color(weapon.color().with_alpha(particle_envelope(charged))),
                    );
                }
            }
            if weapon == Weapon::FaunaExtinction {
                for radius in [0.7, 1.1, 1.55] {
                    paint_trail(
                        painter,
                        images,
                        WeaponTrail::Ring(
                            charging_origin,
                            size * radius,
                            weapon.color().with_alpha(0.7),
                            1.0,
                        ),
                        charged,
                        index as u32,
                        scene.scale,
                    );
                }
            }
        }
        if age >= 0.0 {
            if self.elapsed < shot.impact_at {
                paint_weapon_body(painter, images, flight, progress, scene.scale);
            }
            if age < 0.18 {
                paint_trail(
                    painter,
                    images,
                    WeaponTrail::Glow(flight.origin, size * 0.35, weapon.color(), 0.18),
                    age,
                    index as u32,
                    scene.scale,
                );
            }
            // Replay a bounded history of the same 35ms trail emissions as the schematic.
            // Emission times are analytical, so pausing, speed changes and seeking are exact.
            let last = (age.min(weapon.flight()) / 0.035).floor() as usize;
            let first = ((age - 0.48).max(0.0) / 0.035).floor() as usize;
            for step in first..=last {
                let emitted = step as f32 * 0.035;
                if emitted >= weapon.flight() {
                    continue;
                }
                weapon_trail(
                    flight,
                    weapon.charge() + emitted,
                    emitted / weapon.flight(),
                    |trail| {
                        paint_trail(
                            painter,
                            images,
                            trail,
                            age - emitted,
                            index as u32,
                            scene.scale,
                        );
                    },
                );
            }
            if self.elapsed >= shot.impact_at {
                if let Some(width) = weapon.beam_width() {
                    let tail_age = (self.elapsed - shot.impact_at) / stretch;
                    paint_trail(
                        painter,
                        images,
                        WeaponTrail::Beam(
                            flight.origin,
                            flight.destination,
                            size * width,
                            weapon.color().with_alpha(0.7),
                            0.22,
                        ),
                        tail_age,
                        index as u32,
                        scene.scale,
                    );
                    let direction = (end - start).normalized();
                    let normal = vec2(-direction.y, direction.x);
                    for barrel in 0..weapon.barrels() {
                        let offset = normal
                            * (barrel as f32 - (weapon.barrels() - 1) as f32 * 0.5)
                            * size
                            * width
                            * 0.55;
                        let shift = BevyVec3::new(offset.x, offset.y, 0.0);
                        paint_trail(
                            painter,
                            images,
                            WeaponTrail::Beam(
                                flight.origin + shift,
                                flight.destination + shift,
                                size * width * 0.18,
                                Color::WHITE.with_alpha(0.85),
                                0.18,
                            ),
                            tail_age,
                            index as u32,
                            scene.scale,
                        );
                    }
                }
            }
        }
        let impact_age = self.elapsed - shot.impact_at;
        if !(0.0..0.75).contains(&impact_age) {
            return;
        }
        let strength = (1.0 - impact_age / 0.75).powi(2);
        if shot.outcome.planetary_shield_damage > 0 || shot.outcome.shield_damage > 0 {
            let planet_hit = shot.outcome.planetary_shield_damage > 0 && self.show_planet;
            let toward_source = (start - end).normalized();
            let radius = if planet_hit {
                scene.planet_radius * 1.065
            } else {
                target_size * 0.46
            };
            let center = if planet_hit {
                scene.planet
            } else {
                end - toward_source * radius
            };
            let normal = (end - center).normalized();
            let direction_angle = normal.y.atan2(normal.x);
            let spread = if planet_hit {
                0.13
            } else {
                0.70
            } + impact_age * 0.40;
            for layer in 0..3 {
                arc(
                    painter,
                    center,
                    radius + layer as f32 * 2.0 * scene.scale,
                    direction_angle - spread,
                    spread * 2.0,
                    Stroke::new(
                        (4.0 - layer as f32) * scene.scale,
                        alpha(BLUE, strength * (0.45 - layer as f32 * 0.09)),
                    ),
                );
            }
            glow(painter, end, 23.0 * scene.scale, BLUE, strength * 0.75);
            painter.circle_stroke(
                end,
                impact_age * 38.0 * scene.scale,
                Stroke::new(scene.scale, alpha(BLUE, strength * 0.75)),
            );
        }
        if shot.outcome.hull_damage > 0 || (shot.outcome.killed && !shot.outcome.missed) {
            let hull = shot
                .target
                .map_or(end, |index| self.actor_pose(scene, index, shot.impact_at).center);
            if shot.outcome.shield_damage > 0 {
                glow_line(painter, end, hull, 1.6 * scene.scale, color, strength * 0.65);
            }
            glow(painter, hull, (19.0 + target_size * 0.1) * strength, GOLD, strength * 0.7);
            sparks(painter, hull, impact_age, target_size * 0.60, index as u32, GOLD, strength);
        }
    }

    fn paint_planet_attacks(&self, painter: &Painter, scene: Scene, images: &ImageIds) {
        for (index, attack) in self.timeline.planet_attacks.iter().enumerate() {
            if self.elapsed < attack.start_at || self.elapsed > attack.end_at + 3.8 {
                continue;
            }
            let source = self.actor_pose(scene, attack.source, attack.start_at);
            let target = scene.planet + vec2(-0.25, -0.30) * scene.planet_radius;
            let progress = ((self.elapsed - attack.start_at)
                / (attack.end_at - attack.start_at).max(0.1))
            .clamp(0.0, 1.0);
            if self.elapsed < attack.end_at {
                use super::effects::{
                    death_ray_beam_layers, sustained_envelope, DEATH_RAY_COLLAPSE_AT,
                    DEATH_RAY_DISCHARGE_AT, DEATH_RAY_FOCUS_AT,
                };
                // Sample the schematic War Sun charge, converging emitters and sustained
                // discharge inside this report's interval. The recorded impact still decides
                // whether the world is destroyed; there is no new random roll here.
                let age = progress * DEATH_RAY_COLLAPSE_AT;
                let size = source.size * 0.30;
                let focus = source.center.lerp(target, 0.22);
                let layers = death_ray_beam_layers(size);
                let gold = weapon_color(layers[1].1);
                let sprite = |key: &str, center, dimensions, angle, color, opacity| {
                    paint_effect_sprite(
                        painter,
                        images,
                        key,
                        center,
                        dimensions,
                        angle,
                        alpha(color, opacity),
                    );
                };
                let beam = |from: Pos2, to: Pos2, width, color, opacity| {
                    let delta = to - from;
                    sprite(
                        "combat fx beam",
                        from.lerp(to, 0.5),
                        vec2(delta.length(), width),
                        delta.y.atan2(delta.x),
                        color,
                        opacity,
                    );
                };
                if age < 1.65 {
                    let p = age / 1.65;
                    for spark in 0..28 {
                        let offset = Vec2::angled(spark as f32 * TAU / 28.0) * size * 2.2;
                        sprite(
                            "combat fx glow",
                            source.center + offset * (1.0 - p),
                            Vec2::splat(size * (0.045 + 0.135 * smooth(p))),
                            0.0,
                            gold,
                            particle_envelope(p),
                        );
                    }
                }
                if age < 1.9 {
                    let p = age / 1.9;
                    for radius in [1.2, 1.8, 2.4] {
                        sprite(
                            "combat fx ring",
                            source.center,
                            Vec2::splat(size * (radius + (0.15 - radius) * smooth(p))),
                            radius * age,
                            gold,
                            particle_envelope(p) * 0.6,
                        );
                    }
                }
                if age < 2.1 {
                    let p = age / 2.1;
                    sprite(
                        "combat fx glow",
                        source.center,
                        Vec2::splat(size * 1.7 * (1.0 + 0.8 * smooth(p))),
                        0.0,
                        gold,
                        particle_envelope(p),
                    );
                }
                let focus_age = age - DEATH_RAY_FOCUS_AT;
                if (0.0..1.6).contains(&focus_age) {
                    let strength = particle_envelope(focus_age / 1.6);
                    let width_scale = 1.0 - 0.6 * smooth(focus_age / 1.6);
                    for emitter in 0..8 {
                        let origin =
                            source.center + Vec2::angled(emitter as f32 * TAU / 8.0) * size * 0.48;
                        beam(origin, focus, size * 0.07 * width_scale, gold, strength);
                        beam(origin, focus, size * 0.018 * width_scale, Color32::WHITE, strength);
                    }
                    for (key, extent, lifetime, color) in [
                        ("combat fx glow", 0.9, 1.4, Color32::WHITE),
                        ("combat fx ring", 1.8, 1.2, gold),
                    ] {
                        let p = (focus_age / lifetime).clamp(0.0, 1.0);
                        let scale = if key == "combat fx ring" {
                            0.2 + 0.8 * smooth(p)
                        } else {
                            1.0 + 0.8 * smooth(p)
                        };
                        sprite(
                            key,
                            focus,
                            Vec2::splat(size * extent * scale),
                            0.0,
                            color,
                            particle_envelope(p),
                        );
                    }
                }
                let discharge_age = age - DEATH_RAY_DISCHARGE_AT;
                if discharge_age >= 0.0 {
                    let discharge_progress = (discharge_age / 1.72).clamp(0.0, 1.0);
                    let strength = sustained_envelope(discharge_progress);
                    for (width, color) in layers {
                        beam(
                            focus,
                            target,
                            width * (1.0 - 0.6 * smooth(discharge_progress)),
                            weapon_color(color),
                            strength,
                        );
                    }
                    let p = (discharge_age / 1.5).clamp(0.0, 1.0);
                    sprite(
                        "combat fx glow",
                        target,
                        Vec2::splat(size * 3.4 * (1.0 + 0.8 * smooth(p))),
                        0.0,
                        gold,
                        particle_envelope(p) * 0.8,
                    );
                }
            } else if attack.destroyed {
                explosion(
                    painter,
                    images,
                    scene.planet,
                    scene.planet_radius * 2.4,
                    self.elapsed - attack.end_at,
                    index as u32 + 900,
                );
            } else {
                glow(
                    painter,
                    target,
                    scene.planet_radius * 0.3,
                    GOLD,
                    (1.0 - (self.elapsed - attack.end_at) / 1.2).max(0.0) * 0.65,
                );
            }
        }
    }
}

/// One formation for each fleet, the surface and planetary orbit. No unit counts are capped.
fn formation_group(actor: &CinematicActor) -> usize {
    if actor.unit.is_orbital() {
        3
    } else if actor.side == Side::Defender && (actor.unit.is_defense() || actor.unit.is_building())
    {
        2
    } else {
        usize::from(actor.side == Side::Defender)
    }
}

fn formation_home(group: usize, slot: usize, count: usize) -> Vec2 {
    let angle = slot as f32 * 2.399_963_1;
    let radius = if count <= 1 {
        0.0
    } else {
        ((slot as f32 + 0.5) / count as f32).sqrt()
    };
    let disk = vec2(angle.cos(), angle.sin()) * radius;
    match group {
        0 => vec2(0.26, 0.45) + disk * vec2(0.19, 0.32),
        1 => vec2(0.73, 0.33) + disk * vec2(0.17, 0.22),
        2 => vec2(-0.19, -0.17) + disk * vec2(0.64, 0.54),
        _ => vec2(0.81, 0.55) + disk * vec2(0.13, 0.10),
    }
}

fn unit_size(unit: Unit) -> f32 {
    match unit {
        Unit::Ship(ship) => match ship {
            Ship::Probe => 42.0,
            Ship::LightFighter => 66.0,
            Ship::HeavyFighter => 80.0,
            Ship::Destroyer => 107.0,
            Ship::Cruiser | Ship::Bomber => 133.0,
            Ship::Battleship => 167.0,
            Ship::Dreadnought => 196.0,
            Ship::WarSun => 242.0,
            Ship::ColonyShip => 132.0,
        },
        Unit::Defense(Defense::SpaceDock) => 195.0,
        Unit::Defense(Defense::AntiballisticMissile | Defense::InterplanetaryMissile) => 57.0,
        Unit::Defense(Defense::Crawler | Defense::RepairTruck) => 66.0,
        Unit::Defense(_) => 68.0 + unit.production() as f32 * 8.0,
        Unit::Building(_) => 95.0 + unit.production() as f32 * 13.0,
        Unit::Fauna(_) => 60.0 + unit.production() as f32 * 19.0,
    }
}

/// Source artwork keeps its natural canvas proportions; egui user textures do not expose sizes.
fn sprite_aspect(unit: Unit) -> f32 {
    match unit {
        Unit::Ship(Ship::Probe) => 1.0,
        Unit::Ship(Ship::WarSun) => 1.2,
        Unit::Ship(_) => 1.5,
        _ => 1.0,
    }
}

/// Correct the few source views whose native heading differs from the fleet's NE approach.
fn sprite_rotation(unit: Unit) -> f32 {
    let degrees: f32 = match unit {
        Unit::Ship(Ship::Probe) => 12.0,
        Unit::Ship(Ship::ColonyShip) => -12.0,
        Unit::Ship(Ship::LightFighter | Ship::Cruiser) => 5.0,
        Unit::Ship(Ship::HeavyFighter) => -8.0,
        Unit::Ship(Ship::Destroyer | Ship::WarSun) => 8.0,
        Unit::Ship(Ship::Bomber) => -50.0,
        Unit::Ship(Ship::Battleship) => -47.0,
        Unit::Ship(Ship::Dreadnought) => -2.0,
        _ => 0.0,
    };
    degrees.to_radians()
}

/// Stretched trajectories retain their last smoke/beam particles after their exact impact.
fn shot_tail(weapon: Weapon, shot: &CinematicShot) -> f32 {
    (0.48 * (shot.impact_at - shot.launch_at) / weapon.flight()).max(0.75)
}

/// Color and alpha are shared with the schematic weapon palette, independent of faction.
fn weapon_color(color: Color) -> Color32 {
    let color = color.to_srgba();
    Color32::from_rgba_unmultiplied(
        (color.red * 255.0).round() as u8,
        (color.green * 255.0).round() as u8,
        (color.blue * 255.0).round() as u8,
        (color.alpha * 255.0).round() as u8,
    )
}

#[allow(clippy::too_many_arguments)]
fn paint_effect_sprite(
    painter: &Painter,
    images: &ImageIds,
    key: &str,
    center: Pos2,
    size: Vec2,
    angle: f32,
    color: Color32,
) {
    if let Some(texture) = images.0.get(key) {
        rotated_image(painter, *texture, center, size, angle, false, FULL_UV, color);
    }
}

fn paint_weapon_body(
    painter: &Painter,
    images: &ImageIds,
    flight: WeaponFlight,
    progress: f32,
    _scale: f32,
) {
    let sample = flight.sample(progress);
    if sample.position.distance_squared(flight.origin) < 0.0001 {
        return;
    }
    let center = pos2(sample.center.x, sample.center.y);
    let size = vec2(sample.dimensions.x, sample.dimensions.y);
    let angle = sample.direction.y.atan2(sample.direction.x);
    let mask = match flight.weapon {
        Weapon::Missile | Weapon::Bomb => "combat fx missile",
        Weapon::Repair => "combat fx glow",
        _ => "combat fx beam",
    };
    paint_effect_sprite(
        painter,
        images,
        mask,
        center,
        size,
        angle,
        weapon_color(flight.weapon.color().with_alpha(0.6)),
    );
    let physical = matches!(flight.weapon, Weapon::Missile | Weapon::Bomb | Weapon::Repair);
    for barrel in 0..flight.weapon.barrels() {
        let offset = (barrel as f32 - (flight.weapon.barrels() - 1) as f32 * 0.5) * 0.55;
        let barrel_center = center + rotate(vec2(0.0, offset * size.y), angle);
        if flight.weapon.barrels() > 1 {
            paint_effect_sprite(
                painter,
                images,
                "combat fx beam",
                barrel_center,
                size * vec2(1.0, 0.43),
                angle,
                weapon_color(flight.weapon.color()),
            );
        }
        paint_effect_sprite(
            painter,
            images,
            mask,
            barrel_center,
            size * vec2(
                0.96,
                if physical {
                    0.5
                } else {
                    0.18
                },
            ),
            angle,
            if flight.weapon == Weapon::Repair {
                weapon_color(flight.weapon.color())
            } else {
                Color32::WHITE
            },
        );
    }
}

/// Samples the exact schematic particle mask, growth, fade and spark motion at a movie age.
fn paint_trail(
    painter: &Painter,
    images: &ImageIds,
    trail: WeaponTrail,
    age: f32,
    _seed: u32,
    _scale: f32,
) {
    let point = |position: BevyVec3| pos2(position.x, position.y);
    match trail {
        WeaponTrail::Glow(at, size, color, life) | WeaponTrail::Ring(at, size, color, life) => {
            if !(0.0..life).contains(&age) {
                return;
            }
            let p = age / life;
            let ring = matches!(trail, WeaponTrail::Ring(..));
            let diameter = size
                * if ring {
                    0.2 + 0.8 * smooth(p)
                } else {
                    1.0 + 0.8 * smooth(p)
                };
            paint_effect_sprite(
                painter,
                images,
                if ring {
                    "combat fx ring"
                } else {
                    "combat fx glow"
                },
                point(at),
                Vec2::splat(diameter),
                0.0,
                weapon_color(color.with_alpha(color.to_srgba().alpha * particle_envelope(p))),
            );
        },
        WeaponTrail::Beam(from, to, width, color, life) => {
            if !(0.0..life).contains(&age) {
                return;
            }
            let p = age / life;
            let delta = to - from;
            paint_effect_sprite(
                painter,
                images,
                "combat fx beam",
                point((from + to) * 0.5),
                vec2(delta.length(), width * (1.0 - 0.6 * smooth(p))),
                delta.y.atan2(delta.x),
                weapon_color(color.with_alpha(color.to_srgba().alpha * particle_envelope(p))),
            );
        },
        WeaponTrail::Sparks(at, size, color, count) => {
            let life = 0.42;
            if !(0.0..life).contains(&age) {
                return;
            }
            let p = age / life;
            for spark in 0..count {
                let angle = spark as f32 * 2.399_963;
                let direction = vec2(angle.cos(), angle.sin());
                let center = point(at) + direction * size * (0.5 + (spark % 5) as f32 * 0.17) * age;
                let start = vec2(size * 0.035, size * 0.018);
                let dimensions = start + (Vec2::splat(size * 0.008) - start) * smooth(p);
                paint_effect_sprite(
                    painter,
                    images,
                    "combat fx glow",
                    center,
                    dimensions,
                    0.0,
                    weapon_color(color.with_alpha(color.to_srgba().alpha * particle_envelope(p))),
                );
            }
        },
    }
}

fn smooth(progress: f32) -> f32 {
    let progress = progress.clamp(0.0, 1.0);
    progress * progress * (3.0 - 2.0 * progress)
}

/// Stateless visual noise is deliberately unrelated to the persisted simulation random streams.
fn noise(seed: u32) -> f32 {
    let mut value = seed.wrapping_mul(747_796_405).wrapping_add(2_891_336_453);
    value = ((value >> ((value >> 28) + 4)) ^ value).wrapping_mul(277_803_737);
    ((value >> 22) ^ value) as f32 / u32::MAX as f32
}

fn alpha(color: Color32, opacity: f32) -> Color32 {
    color.gamma_multiply(opacity.clamp(0.0, 1.0))
}

fn rotate(point: Vec2, angle: f32) -> Vec2 {
    let (sin, cos) = angle.sin_cos();
    vec2(point.x * cos - point.y * sin, point.x * sin + point.y * cos)
}

#[allow(clippy::too_many_arguments)]
fn rotated_image(
    painter: &Painter,
    texture: TextureId,
    center: Pos2,
    size: Vec2,
    angle: f32,
    mirror: bool,
    uv: Rect,
    tint: Color32,
) {
    let mut mesh = Mesh::with_texture(texture);
    for (corner, coordinates) in [
        (vec2(-0.5, -0.5), uv.left_top()),
        (vec2(0.5, -0.5), uv.right_top()),
        (vec2(0.5, 0.5), uv.right_bottom()),
        (vec2(-0.5, 0.5), uv.left_bottom()),
    ] {
        mesh.vertices.push(Vertex {
            pos: center + rotate(corner * size, angle),
            uv: pos2(
                if mirror {
                    uv.right() + uv.left() - coordinates.x
                } else {
                    coordinates.x
                },
                coordinates.y,
            ),
            color: tint,
        });
    }
    mesh.indices.extend_from_slice(&[0, 1, 2, 0, 2, 3]);
    painter.add(Shape::mesh(mesh));
}

fn glow(painter: &Painter, center: Pos2, radius: f32, color: Color32, strength: f32) {
    if strength <= 0.0 || radius <= 0.0 {
        return;
    }
    for layer in (1..=4).rev() {
        painter.circle_filled(center, radius * layer as f32 / 4.0, alpha(color, strength * 0.065));
    }
    painter.circle_filled(center, radius * 0.16, alpha(color, strength * 0.65));
}

fn glow_line(painter: &Painter, start: Pos2, end: Pos2, width: f32, color: Color32, strength: f32) {
    for (factor, opacity) in [(5.0, 0.06), (2.5, 0.15), (1.0, 0.85)] {
        painter.line_segment(
            [start, end],
            Stroke::new(width * factor, alpha(color, strength * opacity)),
        );
    }
}

/// A tapered, transparent plume avoids the rectangular ends of wide line primitives.
fn exhaust(painter: &Painter, engine: Pos2, tail: Pos2, width: f32) {
    let direction = (tail - engine).normalized();
    let perpendicular = vec2(-direction.y, direction.x);
    let mut mesh = Mesh::default();
    for (scale, center_color) in [(2.0, alpha(BLUE, 0.45)), (0.65, alpha(Color32::WHITE, 0.9))] {
        let base = mesh.vertices.len() as u32;
        mesh.colored_vertex(engine, center_color);
        mesh.colored_vertex(engine + perpendicular * width * scale, Color32::TRANSPARENT);
        mesh.colored_vertex(tail, Color32::TRANSPARENT);
        mesh.colored_vertex(engine - perpendicular * width * scale, Color32::TRANSPARENT);
        mesh.indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    painter.add(Shape::mesh(mesh));
}

/// Light from the upper-left gives the globe a smooth terminator even at cinematic scale.
fn planet_terminator(painter: &Painter, center: Pos2, radius: f32) {
    const RINGS: u32 = 12;
    const SEGMENTS: u32 = 64;
    let mut mesh = Mesh::default();
    for ring in 0..=RINGS {
        let distance = ring as f32 / RINGS as f32;
        let z = (1.0 - distance * distance).max(0.0).sqrt();
        for segment in 0..=SEGMENTS {
            let normal = Vec2::angled(segment as f32 / SEGMENTS as f32 * TAU) * distance;
            let illumination = (normal.dot(vec2(-0.40, -0.58)) + z * 0.71).max(0.0);
            let shade = (1.0 - illumination).powf(1.6) * 0.70;
            mesh.colored_vertex(
                center + normal * radius,
                Color32::from_black_alpha((shade * 255.0) as u8),
            );
        }
    }
    for ring in 0..RINGS {
        for segment in 0..SEGMENTS {
            let a = ring * (SEGMENTS + 1) + segment;
            let b = a + SEGMENTS + 1;
            mesh.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn arc(painter: &Painter, center: Pos2, radius: f32, start: f32, sweep: f32, stroke: Stroke) {
    let points =
        (0..=24).map(|i| center + Vec2::angled(start + sweep * i as f32 / 24.0) * radius).collect();
    painter.add(Shape::line(points, stroke));
}

fn sparks(
    painter: &Painter,
    center: Pos2,
    age: f32,
    radius: f32,
    seed: u32,
    color: Color32,
    strength: f32,
) {
    for fragment in 0..9_u32 {
        let angle = noise(seed.wrapping_mul(31).wrapping_add(fragment)) * TAU;
        let speed = 0.5 + noise(seed.wrapping_add(fragment * 19)) * 1.4;
        let direction = Vec2::angled(angle);
        let head = center + direction * radius * age * speed;
        let tail = center + direction * radius * (age - 0.075).max(0.0) * speed;
        painter.line_segment([tail, head], Stroke::new(1.0 + strength, alpha(color, strength)));
    }
}

fn explosion(painter: &Painter, images: &ImageIds, center: Pos2, size: f32, age: f32, seed: u32) {
    let flash = (1.0 - age / 0.35).clamp(0.0, 1.0);
    glow(painter, center, size * (0.85 + age), GOLD, flash * 0.75);
    painter.circle_stroke(
        center,
        size * (0.20 + age * 1.4),
        Stroke::new(
            (1.0 - age / 0.8).max(0.0) * 3.0,
            alpha(GOLD, (1.0 - age / 0.8).max(0.0) * 0.6),
        ),
    );
    if age < 1.2 {
        if let Some(texture) = images.0.get("explosion") {
            let frame = ((age / 1.2) * 47.0).floor() as usize;
            let uv = Rect::from_min_max(
                pos2((frame % 8) as f32 / 8.0, (frame / 8) as f32 / 6.0),
                pos2((frame % 8 + 1) as f32 / 8.0, (frame / 8 + 1) as f32 / 6.0),
            );
            painter.image(
                *texture,
                Rect::from_center_size(center, Vec2::splat(size * 1.6)),
                uv,
                Color32::WHITE,
            );
        } else {
            glow(painter, center, size * (0.25 + age * 0.35), GOLD, 1.0 - age / 1.2);
        }
    }
    let debris = (1.0 - age / 3.6).max(0.0);
    sparks(painter, center, age, size * 0.65, seed, GOLD, debris.powi(3));
    for fragment in 0..7_u32 {
        let direction = Vec2::angled(noise(seed.wrapping_mul(53).wrapping_add(fragment)) * TAU);
        let point = center + direction * size * age * (0.18 + noise(fragment + seed) * 0.3);
        let span = size * (0.025 + noise(fragment + 39) * 0.025);
        let spin = Vec2::angled(age * 2.0 + fragment as f32) * span;
        painter.line_segment(
            [point - spin, point + spin],
            Stroke::new((size * 0.018).max(0.75), alpha(Color32::from_rgb(110, 112, 124), debris)),
        );
        glow(painter, point, span * 2.0, GOLD, debris.powi(3) * 0.4);
    }
}

/// Intersects the shot segment with the visible globe's shield before it reaches a surface gun.
fn sphere_entry(start: Pos2, end: Pos2, center: Pos2, radius: f32) -> Option<Pos2> {
    let direction = end - start;
    let offset = start - center;
    let a = direction.length_sq();
    if a <= f32::EPSILON {
        return None;
    }
    let b = 2.0 * offset.dot(direction);
    let c = offset.length_sq() - radius * radius;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant < 0.0 {
        return None;
    }
    let t = (-b - discriminant.sqrt()) / (2.0 * a);
    (0.0..=1.0).contains(&t).then_some(start + direction * t)
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic.rs"]
mod tests;
