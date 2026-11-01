//! A continuous, decorative camera over the recorded battle; no simulation runs here.

use std::f32::consts::{PI, TAU};

use bevy::prelude::{Alpha, Color, Resource, Vec3 as BevyVec3};
use bevy_egui::egui::{
    epaint::{Mesh, Vertex},
    pos2, vec2, Color32, FontId, Painter, Pos2, Rect, Shape, Stroke, TextureId, Vec2,
};

use super::cinematic_camera::CinematicCamera;
use super::cinematic_timeline::{
    CinematicActor, CinematicShot, CinematicTimeline, LEVEL_LOSS_INTERVAL,
};
use super::effects::{particle_envelope, weapon_trail, Weapon, WeaponFlight, WeaponTrail};
use super::report::{MissionReport, Side};
use crate::core::identity::PlayerId;
use crate::core::map::icon::Icon;
use crate::core::map::planet::{Planet, PlanetKind};
use crate::core::map::utils::{MAIN_BUTTON_BOTTOM, MAIN_BUTTON_HEIGHT};
use crate::core::missions::BombingRaid;
use crate::core::player::PlayerColor;
use crate::core::ui::utils::ImageIds;
use crate::core::units::defense::Defense;
use crate::core::units::ships::Ship;
use crate::core::units::Unit;

const FULL_UV: Rect = Rect::from_min_max(Pos2::ZERO, pos2(1.0, 1.0));
const BLUE: Color32 = Color32::from_rgb(104, 210, 255);
const GOLD: Color32 = Color32::from_rgb(255, 177, 90);
const GREEN: Color32 = Color32::from_rgb(102, 255, 182);
#[path = "cinematic_sprites.rs"]
mod sprites;
use sprites::{firing_sheet, FiringSheet};
#[path = "cinematic_damage.rs"]
mod damage;
#[path = "cinematic_environment.rs"]
mod environment;
#[path = "cinematic_formation.rs"]
mod formation;
#[path = "cinematic_motion.rs"]
mod motion;
#[path = "cinematic_rewards.rs"]
mod rewards;
/// Planet-sized blasts linger longer than ship wrecks; soundtrack peaks use the same clock.
pub(crate) const PLANET_BLAST_TIME_SCALE: f32 = 1.25;
/// The texture swap occurs inside the opaque core of the planet's detonation.
const PLANET_SWAP_AT: f32 = 0.68;
const DESTROYED_PLANET_UV: Rect = Rect::from_min_max(pos2(0.01, 0.005), pos2(0.99, 0.9825));

/// The movie has one clock. Manual camera navigation remains available while it is paused.
#[derive(Resource)]
pub(crate) struct CinematicPlayback {
    pub timeline: CinematicTimeline,
    pub elapsed: f32,
    pub camera: CinematicCamera,
    visuals: Vec<ActorVisual>,
    draw_order: Vec<usize>,
    maximum_shot_lifetime: f32,
    maximum_charge_lead: f32,
    repair_order: Vec<usize>,
    /// Only visible stations participate in berth clearance; never scan all actors per frame.
    orbital_obstacles: Vec<usize>,
    maximum_repair_lifetime: f32,
    planet_image: String,
    destroyed_planet_image: &'static str,
    planet_uv: Rect,
    atmosphere_color: Color32,
    show_planet: bool,
    gas_planet: bool,
    missile_strike: bool,
    resource_pickup: Option<rewards::ResourcePickup>,
}

struct ActorVisual {
    owner_color: Option<Color32>,
    firing_sheet: Option<FiringSheet>,
    texture: String,
    fallback_texture: String,
    aspect: f32,
    art_heading: f32,
    home: Vec2,
    size: f32,
    phase: f32,
    ground: bool,
    visible: bool,
    /// A recorded surface target guides the bomber's approach without simulating new orders.
    bombing_target: Option<Vec2>,
    firing_times: Vec<f32>,
    firing_shots: Vec<usize>,
    /// Incoming events sorted by impact time for bounded local light and surface scars.
    incoming_shots: Vec<usize>,
    /// Penetrating surface contacts only; later shield hits must not erase older scars.
    surface_hits: Vec<usize>,
    repair_visits: Vec<usize>,
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
        let scale = (rect.width() / 1440.0).min(rect.height() / 820.0);
        let planet_radius = rect.height().min(rect.width() * 0.78) * 0.32;
        // Reserve room for the near-side settlement above the fixed-size exit control.
        let planet_y = (rect.height() * 0.77).min(
            rect.height()
                - MAIN_BUTTON_BOTTOM
                - MAIN_BUTTON_HEIGHT
                - 18.0
                - 58.0 * scale
                - planet_radius * 0.44,
        );
        Self {
            rect,
            scale,
            planet: pos2(rect.left() + rect.width() * 0.79, rect.top() + planet_y),
            planet_radius,
        }
    }

    fn point(self, p: Vec2) -> Pos2 {
        self.rect.min + self.rect.size() * p
    }
}

impl CinematicPlayback {
    pub fn new(report: &MissionReport) -> Self {
        let mut timeline = CinematicTimeline::new(report);
        let resource_pickup = rewards::ResourcePickup::new(report, &timeline);
        if let Some(pickup) = &resource_pickup {
            timeline.duration = timeline.duration.max(pickup.ends_at());
        }
        let gas_planet = report.planet.kind == PlanetKind::Gas;
        let missile_strike = report.mission.objective == Icon::MissileStrike;
        let visible: Vec<_> = timeline
            .actors
            .iter()
            .map(|actor| {
                (!missile_strike
                    || actor.side != Side::Defender
                    || !actor.unit.is_ship() && !actor.unit.is_orbital())
                    && (!actor.unit.is_orbital() || actor.unit == Unit::space_dock())
                    && (actor.initial_levels.is_none()
                        || (actor.unit.is_economic_building()
                            && report.mission.includes_bombing(&BombingRaid::Economic))
                        || (actor.unit.is_industrial_building()
                            && report.mission.includes_bombing(&BombingRaid::Industrial)))
            })
            .collect();
        let mut counts = [0_usize; 5];
        for (actor, visible) in timeline.actors.iter().zip(&visible) {
            if *visible {
                counts[formation_group(actor)] += 1;
            }
        }
        let fleet_count = counts[0].max(counts[1]).max(counts[3]);
        let mut slots = [0_usize; 5];
        let mut visuals = Vec::with_capacity(timeline.actors.len());
        for (index, actor) in timeline.actors.iter().enumerate() {
            let group = formation_group(actor);
            let slot = slots[group];
            slots[group] += usize::from(visible[index]);
            let name = actor.unit.to_lowername();
            let texture = if actor.unit.is_fauna() {
                format!("map {name}")
            } else if gas_planet && actor.initial_levels.is_some() {
                format!("cinematic gas {name}")
            } else {
                format!("cinematic {name}")
            };
            // Both fleets share a scale: a fighter must not grow just because its
            // side fields fewer ships. Surface equipment uses its own available ground area.
            let density_count = if group == 4 {
                1
            } else if group == 2 {
                counts[2]
            } else {
                fleet_count
            };
            let base_density = if group == 2 {
                14
            } else {
                9
            };
            let density =
                (base_density as f32 / density_count.max(base_density) as f32).sqrt().max(0.11);
            visuals.push(ActorVisual {
                owner_color: actor.owner.filter(|_| !actor.unit.is_fauna()).map(|owner| {
                    let [r, g, b] = PlayerColor::for_player(owner).rgb();
                    Color32::from_rgb(r, g, b)
                }),
                firing_sheet: firing_sheet(actor.unit),
                texture,
                fallback_texture: name,
                aspect: sprite_aspect(actor.unit),
                art_heading: sprite_heading(actor.unit),
                home: if group == 2 && counts[4] > 0 {
                    let old = formation_home(group, slot, counts[group]);
                    // Keep a full defensive terrace above the separate settlement row.
                    let y = -0.57 + (old.y + 0.24) * 0.65;
                    vec2(old.x * (1.0 - y * y).sqrt(), y)
                } else {
                    formation_home(group, slot, counts[group])
                },
                size: unit_size(actor.unit) * density,
                phase: {
                    let identity = actor.id.unwrap_or(index as u64);
                    noise((identity ^ (identity >> 32)) as u32) * TAU
                        + actor.owner.unwrap_or(0) as f32 * 0.07
                },
                ground: group == 2 || group == 4,
                visible: visible[index],
                bombing_target: None,
                firing_times: Vec::new(),
                firing_shots: Vec::new(),
                incoming_shots: Vec::new(),
                surface_hits: Vec::new(),
                repair_visits: Vec::new(),
            });
        }
        formation::layout_fleets(&mut visuals, &timeline.actors);
        environment::layout_surface(&mut visuals, &timeline.actors, counts[4] > 0);
        for (shot_index, shot) in timeline.shots.iter().enumerate() {
            visuals[shot.source].firing_times.push(shot.launch_at);
            visuals[shot.source].firing_shots.push(shot_index);
            if let Some(target) = shot.target {
                visuals[target].incoming_shots.push(shot_index);
            }
            if let Some(target) = shot.target.filter(|target| {
                timeline.actors[shot.source].unit == Unit::Ship(Ship::Bomber)
                    && visuals[*target].ground
                    && visuals[*target].visible
            }) {
                visuals[shot.source].bombing_target = Some(visuals[target].home);
            }
        }
        for visual in &mut visuals {
            visual.incoming_shots.sort_by(|a, b| {
                timeline.shots[*a].impact_at.total_cmp(&timeline.shots[*b].impact_at)
            });
            if visual.ground {
                visual.surface_hits = visual
                    .incoming_shots
                    .iter()
                    .copied()
                    .filter(|shot| environment::scorches_surface(&timeline.shots[*shot]))
                    .collect();
            }
        }
        let mut draw_order: Vec<_> = (0..visuals.len()).filter(|index| visible[*index]).collect();
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
        for &visit in &repair_order {
            if let Some(source) = timeline.repairs[visit].source {
                visuals[source].repair_visits.push(visit);
            }
        }
        let maximum_repair_lifetime = timeline
            .repairs
            .iter()
            .map(|repair| repair.end_at - repair.start_at)
            .fold(0.0_f32, f32::max);
        let planet_image = cinematic_planet_image(&report.planet);
        let planet_uv = cinematic_planet_uv(&planet_image);
        let orbital_obstacles = timeline
            .actors
            .iter()
            .enumerate()
            .filter(|(index, actor)| visible[*index] && actor.unit == Unit::space_dock())
            .map(|(index, _)| index)
            .collect();
        Self {
            timeline,
            elapsed: 0.0,
            camera: CinematicCamera::default(),
            visuals,
            draw_order,
            maximum_shot_lifetime,
            maximum_charge_lead,
            repair_order,
            orbital_obstacles,
            maximum_repair_lifetime,
            planet_image,
            destroyed_planet_image: if report.planet.is_moon() {
                "moon0"
            } else {
                "planet0"
            },
            planet_uv,
            atmosphere_color: environment::atmosphere_color(report.planet.kind),
            show_planet: !report.is_space_encounter(),
            gas_planet,
            missile_strike,
            resource_pickup,
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

    /// Presentation filtering is shared with sound; hidden scenery has no audible events.
    pub fn actor_visible(&self, index: usize) -> bool {
        self.visuals[index].visible
    }

    /// Cache the match palette, including lobby color choices and allied fleet ownership.
    pub fn set_owner_colors(&mut self, mut color: impl FnMut(PlayerId) -> Color32) {
        for (actor, visual) in self.timeline.actors.iter().zip(&mut self.visuals) {
            visual.owner_color = actor.owner.filter(|_| !actor.unit.is_fauna()).map(&mut color);
        }
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
        let mut painter = painter.with_clip_rect(rect);
        let scene = Scene::new(rect);
        self.paint_space(&painter, scene, images);
        // Sample all geometry in the original canvas, then transform it together. Zoom must
        // never recalculate trajectories, shield contacts, muzzle positions or effect sizes.
        let transform = self.camera.transform(rect);
        let clip = painter.clip_rect();
        painter.set_clip_rect(transform.inverse().mul_rect(clip));
        let first_world_shape =
            painter.ctx().graphics_mut(|graphics| graphics.entry(painter.layer_id()).next_idx());
        if self.show_planet {
            self.paint_planet(&painter, scene, images);
            self.paint_surface_damage(&painter, scene);
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
            if !self.visuals[repair.target].visible
                || repair.source.is_some_and(|source| !self.visuals[source].visible)
                || self.elapsed < repair.start_at
                || self.elapsed > repair.end_at + 0.4
            {
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
            if !self.visuals[shot.source].visible
                || shot.target.is_some_and(|target| !self.visuals[target].visible)
            {
                continue;
            }
            let index = first_shot + offset;
            let weapon = Weapon::for_shot(self.timeline.actors[shot.source].unit, &shot.outcome);
            if self.elapsed >= weapon.cinematic_charge_start(shot.launch_at, shot.impact_at)
                && self.elapsed <= shot.impact_at + shot_tail(weapon, shot)
            {
                self.paint_shot(&painter, scene, images, index, shot);
            }
        }
        for (index, actor) in self.timeline.actors.iter().enumerate() {
            if !self.visuals[index].visible {
                continue;
            }
            if let Some(death) = actor.death_at {
                let age = self.elapsed - death;
                if (0.0..3.6).contains(&age) {
                    if actor.unit.is_ship() {
                        self.paint_actor_destruction(&painter, scene, images, index, death);
                    } else {
                        let pose = self.actor_pose(scene, index, death);
                        explosion(&painter, images, pose.center, pose.size, age, actor.unit);
                    }
                }
            }
        }
        self.paint_level_losses(&painter, scene, images);
        self.paint_planet_attacks(&painter, scene, images);
        self.paint_resource_pickups(&painter, scene, images);
        painter.ctx().graphics_mut(|graphics| {
            let shapes = graphics.entry(painter.layer_id());
            shapes.transform_range(first_world_shape, shapes.next_idx(), transform);
        });
        painter.set_clip_rect(clip);
        environment::paint_vignette(&painter, rect);
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
            // Use the strategic map's unaltered star field. Aspect-fill its 3:2 canvas instead
            // of stretching the image or covering it with an enlarged nebula cutout.
            let height = scene.rect.height().max(scene.rect.width() / 1.5) + 60.0;
            painter.image(
                *texture,
                Rect::from_center_size(
                    scene.rect.center() + camera * 14.0,
                    vec2(height * 1.5, height),
                ),
                FULL_UV,
                Color32::WHITE,
            );
        }
        // A restrained exposure gradient separates bright hulls and weapon light from the
        // busy map starfield while retaining the source artwork and its natural palette.
        let mut exposure = Mesh::default();
        for (point, opacity) in [
            (scene.rect.left_top(), 0.20),
            (scene.rect.right_top(), 0.20),
            (scene.rect.right_bottom(), 0.08),
            (scene.rect.left_bottom(), 0.08),
        ] {
            exposure.colored_vertex(point, alpha(Color32::from_rgb(3, 8, 17), opacity));
        }
        exposure.add_triangle(0, 1, 2);
        exposure.add_triangle(0, 2, 3);
        painter.add(exposure);
        // Three camera-linked depths use the map's cool/white/warm palette and progressively
        // brighter, sharper twinkles. Stable seeds preserve stars across replay and seeking.
        for (layer, (depth, count, drift)) in [
            (0.34, 420_u32, vec2(0.28, -0.12)),
            (0.57, 280, vec2(-0.34, 0.24)),
            (0.80, 220, vec2(0.90, -0.42)),
        ]
        .into_iter()
        .enumerate()
        {
            let transform = self.camera.parallax_transform(scene.rect, depth);
            let period = scene.rect.size() * 2.0 * transform.scaling;
            let layer = layer as f32;
            for i in 0..count {
                let seed = i * 11 + layer as u32 * 1703 + 502;
                let base = vec2(noise(seed + 1), noise(seed + 2));
                let point = transform
                    * (scene.point(base * 2.0 - Vec2::splat(0.5))
                        + drift * self.elapsed * scene.scale);
                // Wrap beyond the visible edges, even at minimum zoom, so dragging or bouncing
                // never exposes an empty strip or teleports a visible star across the screen.
                let offset = point - scene.rect.center() + period * 0.5;
                let center = scene.rect.center()
                    + vec2(offset.x.rem_euclid(period.x), offset.y.rem_euclid(period.y))
                    - period * 0.5;
                if !scene.rect.expand(16.0).contains(center) {
                    continue;
                }
                let pulse = (0.5
                    + 0.5
                        * (self.elapsed
                            * (0.22 + layer * 0.22 + noise(seed + 3) * (0.68 + layer * 0.22))
                            + noise(seed + 4) * TAU)
                            .sin())
                .powf(1.25 + layer * 1.075);
                let minimum = 0.48 - layer * 0.20 + noise(seed + 5) * 0.14;
                let brightness = (0.28 + layer * 0.13 + noise(seed + 6) * 0.35)
                    * (minimum + (1.0 - minimum) * pulse);
                let temperature = noise(seed + 7);
                let tint = if temperature < 0.24 {
                    Color32::from_rgb(158, 194, 255)
                } else if temperature > 0.88 {
                    Color32::from_rgb(255, 209, 148)
                } else {
                    Color32::from_rgb(230, 242, 255)
                };
                let size = (0.8 + layer * 0.2 + noise(seed + 8) * (2.0 + layer * 0.7))
                    * 0.5
                    * scene.scale.max(0.55)
                    * transform.scaling.min(1.5)
                    * (0.82 + 0.30 * pulse);
                let color = alpha(tint, brightness);
                painter.circle_filled(center, size, color);
                if i % 17 == 0 {
                    glow(painter, center, size * 6.0, tint, brightness * 0.35);
                    let length = size * (2.0 + pulse);
                    for ray in [vec2(length, 0.0), vec2(0.0, length * 0.75)] {
                        painter.line_segment(
                            [center - ray, center + ray],
                            Stroke::new(0.65, alpha(color, pulse * 0.8)),
                        );
                    }
                }
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
        if let Some(age) = destroyed_at.map(|at| self.elapsed - at).filter(|age| *age >= 0.0) {
            self.paint_collapsing_planet(painter, scene, images, age);
            return;
        }
        let radius = scene.planet_radius;
        environment::paint_atmosphere(painter, scene.planet, radius, self.atmosphere_color);
        if let Some(texture) = images.0.get(&self.planet_image) {
            painter.image(
                *texture,
                Rect::from_center_size(scene.planet, Vec2::splat(radius * 2.0)),
                self.planet_uv,
                Color32::WHITE,
            );
        } else {
            painter.circle_filled(scene.planet, radius, Color32::from_rgb(20, 41, 56));
        }
        // Strategic missiles bypass the fleet and planetary shield; only surface defenses and
        // their antiballistic interceptors participate in this presentation.
        let shield = if self.missile_strike {
            0
        } else {
            self.timeline.planetary_shield_at(self.elapsed)
        };
        if shield > 0 {
            let ratio = (shield as f32 / self.timeline.initial_planetary_shield.max(1) as f32)
                .clamp(0.0, 1.0);
            // The strategic map's filament artwork supplies the same electromagnetic field.
            // Its three-second breath and two slow counterflows are sampled from the movie
            // clock, so pausing, speeding up and replaying also control the entire shield.
            let pulse = 0.5 - 0.5 * (TAU * self.elapsed / 3.0).cos();
            let strength = 0.25 + ratio * 0.75;
            let color = Color32::from_rgb(62, 171, 250);
            painter.circle_filled(
                scene.planet,
                radius * 1.065,
                alpha(color, strength * (0.025 + pulse * 0.035)),
            );
            if let Some(texture) = images.0.get("planetary shield marker") {
                for (rotation, opacity, diameter) in [
                    (self.elapsed * 0.14, 0.22 + pulse * 0.32, 2.38),
                    (-self.elapsed * 0.09 + 1.8, 0.08 + (1.0 - pulse) * 0.12, 2.34),
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

    fn paint_collapsing_planet(
        &self,
        painter: &Painter,
        scene: Scene,
        images: &ImageIds,
        age: f32,
    ) {
        // Preserve the globe's visible diameter. The map's destroyed artwork has a small
        // transparent margin, unlike the tightly cropped cinematic planet variants.
        let (name, uv) = if age < PLANET_SWAP_AT {
            (self.planet_image.as_str(), self.planet_uv)
        } else {
            (self.destroyed_planet_image, DESTROYED_PLANET_UV)
        };
        let Some(texture) = images.0.get(name) else {
            return;
        };
        painter.image(
            *texture,
            Rect::from_center_size(scene.planet, Vec2::splat(scene.planet_radius * 2.0)),
            uv,
            Color32::WHITE,
        );
    }

    fn actor_pose(&self, scene: Scene, index: usize, time: f32) -> ActorPose {
        let mut pose = self.actor_flight_pose(scene, index, time);
        let impact_bank = if self.timeline.actors[index].unit.is_ship() {
            pose.angle
        } else {
            0.0
        };
        let actor = &self.timeline.actors[index];
        let visual = &self.visuals[index];
        if self.missile_strike && actor.unit == Unit::interplanetary_missile() {
            // The source artwork points upward; rotate it onto the saved straight approach rather
            // than applying the banking and cannon-aiming behavior used by maneuvering ships.
            let velocity = self.actor_flight_pose(scene, index, time + 0.025).center
                - self.actor_flight_pose(scene, index, time - 0.025).center;
            if velocity.length_sq() > 0.000_001 {
                pose.angle = velocity.angle() - visual.art_heading;
            }
            pose.mirror = false;
            return pose;
        }
        if !visual.ground && actor.unit != Unit::space_dock() {
            // Face the actual course, including approach, shield clearance and withdrawal.
            // A hull-sized look along the immutable path gives heavy ships a deliberate
            // helm response instead of amplifying tiny tangent changes into visible rocking.
            // Keep rapid departures precise; they already include their own steering arc.
            let sample = if actor.retreat_at.is_some_and(|at| time >= at - 0.2) {
                0.025
            } else {
                (unit_size(actor.unit) / 95.0 * 0.085).clamp(0.06, 0.22)
            };
            // The helm anticipates its course, never a hit that has not happened yet.
            // Hull shock is sampled at the exact current time and added separately.
            let velocity = self.actor_course_pose(scene, index, time + sample).center
                - self.actor_course_pose(scene, index, time - sample).center;
            if velocity.length_sq() > 0.000_001 {
                // Mirror horizontally when changing course instead of rotating through
                // a half-turn and exposing an upside-down isometric hull.
                pose.mirror = velocity.x < 0.0;
                let bow = Vec2::angled(visual.art_heading)
                    * vec2(
                        if pose.mirror {
                            -1.0
                        } else {
                            1.0
                        },
                        1.0,
                    );
                let heading = velocity.angle() - bow.angle();
                let bank = heading.sin().atan2(heading.cos());
                let limit = (0.80 * 95.0 / unit_size(actor.unit)).clamp(0.30, 0.65);
                pose.angle = bank.clamp(-limit, limit) + impact_bank;
            }
            if actor.retreat_at.is_none_or(|at| time < at) {
                self.aim_ship_cannon(scene, index, time, &mut pose);
            }
        }
        if actor.unit == Unit::space_dock() {
            pose.mirror = false;
        }
        if actor.unit == Unit::repair_truck() {
            // Heading comes from road travel, never the gas platform's vertical bob.
            let (_, direction) = self.repair_truck_motion(scene, index, time);
            let art_heading = 145.0_f32.to_radians();
            pose.mirror = direction.x > 0.0;
            let bow = Vec2::angled(art_heading)
                * vec2(
                    if pose.mirror {
                        -1.0
                    } else {
                        1.0
                    },
                    1.0,
                );
            let turn = direction.angle() - bow.angle();
            pose.angle = turn.sin().atan2(turn.cos()).clamp(-1.0, 1.0);
        }
        for attack in &self.timeline.planet_attacks {
            if (attack.start_at..=attack.end_at).contains(&time) && attack.sources.contains(&index)
            {
                self.aim_planet_cannon(
                    index,
                    time,
                    self.planet_attack_focus(scene, attack),
                    &mut pose,
                );
                break;
            }
        }
        pose
    }

    fn actor_flight_pose(&self, scene: Scene, index: usize, time: f32) -> ActorPose {
        let mut pose = self.actor_course_pose(scene, index, time);
        if self.timeline.actors[index].unit.is_ship() {
            let response = self.hull_impact_response(scene, index, time);
            let mut displacement = response.displacement;
            if self.show_planet {
                // Limit only the impulse's inward component. Re-projecting the whole
                // already-cleared course would introduce a jump when a tiny hit starts.
                let offset = pose.center - scene.planet;
                let normal = offset.normalized();
                let inward = displacement.dot(normal).min(0.0);
                let gap =
                    (offset.length() - scene.planet_radius * 1.11 - pose.size * 0.56).max(0.0);
                let allowed = if gap > 0.0001 {
                    -gap * (1.0 - (inward / gap).exp())
                } else {
                    0.0
                };
                displacement += normal * (allowed - inward);
            }
            pose.center += displacement;
            pose.angle = response.bank;
        }
        pose
    }

    /// Planned flight excludes incoming impacts so independent actors never recurse through
    /// each other's hit responses, and steering cannot react before a recorded contact.
    fn actor_course_pose(&self, scene: Scene, index: usize, time: f32) -> ActorPose {
        let mut pose = self.actor_cruise_pose(scene, index, time);
        // Axial superweapons make a deliberate firing run. Only the participating hulls
        // change course; the rest of the recorded battle continues around them.
        for attack in &self.timeline.planet_attacks {
            let lead = 2.0;
            if time < attack.start_at - lead
                || time > attack.end_at + lead
                || !attack.sources.contains(&index)
            {
                continue;
            }
            let mut origin = self.actor_cruise_pose(scene, index, attack.start_at).center;
            let focus = self.planet_attack_focus(scene, attack);
            // Form a shallow firing fan before discharge: axial guns must line up
            // without banking a capital hull vertically toward a nearby focus.
            let slot = attack.sources.iter().position(|source| *source == index).unwrap_or(0);
            let rows = attack.sources.len().min(3);
            let lane = (slot % rows) as f32 - (rows - 1) as f32 * 0.5;
            let bearing = lane
                * if rows == 2 {
                    0.44
                } else {
                    0.24
                };
            let distance = origin.distance(focus).max(pose.size * 2.2)
                + (slot / rows) as f32 * pose.size * 1.25;
            origin = focus - Vec2::angled(bearing) * distance;
            let direction = (focus - origin).normalized();
            let firing_run = origin + direction * (time - attack.start_at) * 6.0 * scene.scale;
            let blend = smooth(((time - attack.start_at + lead) / lead).clamp(0.0, 1.0))
                * (1.0 - smooth(((time - attack.end_at) / lead).clamp(0.0, 1.0)));
            pose.center = pose.center.lerp(firing_run, blend);
            break;
        }
        pose
    }

    fn repair_truck_motion(&self, scene: Scene, index: usize, time: f32) -> (Pos2, Vec2) {
        let visual = &self.visuals[index];
        let mut from = scene.planet + visual.home * scene.planet_radius;
        let mut available = 0.0_f32;
        let mut heading = Vec2::angled(145.0_f32.to_radians());
        for &visit in &visual.repair_visits {
            let repair = &self.timeline.repairs[visit];
            let target = &self.visuals[repair.target];
            // Park beside the repaired gun, leaving its barrel and firing line clear.
            let mut destination = scene.planet
                + target.home * scene.planet_radius
                + vec2(-0.28, 0.34) * (target.size + visual.size) * scene.scale;
            let offset = destination - scene.planet;
            let clearance = (scene.planet_radius - visual.size * scene.scale * 0.6).max(1.0);
            if offset.length() > clearance {
                destination = scene.planet + offset.normalized() * clearance;
            }
            if destination.distance(from) < scene.scale {
                if time <= repair.end_at {
                    return (from, heading);
                }
                available = available.max(repair.end_at);
                continue;
            }
            let travel = (destination.distance(from) / (45.0 * scene.scale)).max(0.6);
            // Stop and steer before reversing for another job, even when its report
            // timestamp is already past. Never change direction at nonzero road speed.
            let steer_ready = if available > 0.0 {
                available + 0.4
            } else {
                0.0
            };
            let depart = (repair.start_at - travel).max(steer_ready);
            // Tight jobs may start while the truck is still approaching. Keep driving at
            // a plausible speed; the recorded repair beam still originates at its real pose.
            let arrive = depart + travel;
            let control = from.lerp(destination, 0.5).lerp(scene.planet, 0.08);
            let departure_heading = (control - from).normalized();
            if time < depart {
                let turn = departure_heading.angle() - heading.angle();
                let steering = smooth(((time - depart + 0.4) / 0.4).clamp(0.0, 1.0));
                return (
                    from,
                    Vec2::angled(heading.angle() + turn.sin().atan2(turn.cos()) * steering),
                );
            }
            if time < arrive {
                let progress = smooth(((time - depart) / (arrive - depart)).clamp(0.0, 1.0));
                return (
                    from.lerp(control, progress)
                        .lerp(control.lerp(destination, progress), progress),
                    ((control - from) * (1.0 - progress) + (destination - control) * progress)
                        .normalized(),
                );
            }
            heading = (destination - control).normalized();
            if time <= repair.end_at.max(arrive) {
                return (destination, heading);
            }
            from = destination;
            available = repair.end_at.max(arrive);
        }
        (from, heading)
    }

    fn actor_cruise_pose(&self, scene: Scene, index: usize, time: f32) -> ActorPose {
        let actor = &self.timeline.actors[index];
        let visual = &self.visuals[index];
        if self.missile_strike && actor.unit == Unit::interplanetary_missile() {
            return self.incoming_missile_pose(scene, index, time);
        }
        let mirror = actor.side == Side::Defender;
        let direction = if mirror {
            -1.0
        } else {
            1.0
        };
        let phase = visual.phase;
        let size = visual.size
            * scene.scale
            * if visual.ground {
                environment::surface_perspective(visual.home)
            } else {
                1.0
            };
        let mut center;
        let mut angle = 0.0;
        if visual.ground {
            // Home positions live on the visible globe, so turrets never float in open space.
            center = scene.planet + visual.home * scene.planet_radius;
            if actor.unit == Unit::repair_truck() {
                center = self.repair_truck_motion(scene, index, time).0;
            } else if actor.unit == Unit::crawler() {
                center += vec2((time * 0.16 + phase).sin(), (time * 0.16 + phase).cos())
                    * environment::crawler_patrol_extent(visual.size)
                    * scene.scale;
            }
            if self.gas_planet {
                center.y += (time * 0.85 + phase).sin() * 3.0 * scene.scale;
            }
        } else if actor.unit == Unit::space_dock() && self.show_planet {
            // Keep a fixed orbital radius and attitude: a station should not bob or rock.
            // Choose a visible starting point once, rather than clamping its path every frame.
            let altitude = scene.planet_radius * (1.15 + visual.home.y.abs())
                + size * 0.60
                + 22.0 * scene.scale;
            let top = scene.rect.top() + size * 0.55 + 12.0 * scene.scale;
            let visible_start = -PI + ((scene.planet.y - top) / altitude).clamp(0.0, 1.0).asin();
            let start_angle = (-2.15 + visual.home.x * 0.45).min(visible_start);
            let elapsed = time.max(0.0);
            let arrival = self.timeline.entrance_duration.max(0.1);
            let p = (elapsed / arrival).min(1.0);
            // Integrate a smooth change from the arrival orbit to a much slower combat drift.
            // Both position and angular velocity are continuous as the fleets reach the planet.
            let travel =
                0.003 * elapsed + (0.024 - 0.003) * arrival * (p - p.powi(3) + 0.5 * p.powi(4));
            // Ease into the visible upper orbital arc during very long replays, without reversing
            // direction or clipping through the screen edge or the planet's energy field.
            let arc = (start_angle + PI - 0.18).clamp(0.01, 0.42);
            let orbit_angle = start_angle - arc * (1.0 - (-travel / arc).exp());
            center = scene.planet + Vec2::angled(orbit_angle) * altitude;
        } else {
            center = scene.point(visual.home);
            // Maneuverability follows the actual hull class, independent of fleet density.
            let agility = (95.0 / unit_size(actor.unit)).clamp(0.48, 1.65);
            let flight_time = (time - self.timeline.entrance_duration) * 0.55;
            // Keep every escort in a visible outer lane during the opening exchange.
            // The full formation opens out gradually as the fleets close under fire.
            let approach = smooth((time / self.timeline.entrance_duration).clamp(0.0, 1.0));
            let spine = if mirror {
                0.73
            } else {
                0.26
            };
            center.x -= (visual.home.x - spine) * scene.rect.width() * 0.22 * (1.0 - approach);
            let course = flight_time * 0.16 * agility / (1.0 + (flight_time / 35.0).powi(2)).sqrt()
                + (phase - PI) * 0.08;
            let advance = flight_time * 0.14 * agility;
            let cruise_thrust = advance / (1.0 + (advance / 4.0).powi(2)).sqrt();
            // Long forward passes with shallow lateral bends. Closed 2D ellipses force
            // a flat isometric sprite through vertical and upside-down attitudes. Retain
            // cruise thrust after the arrival braking, with finite travel so long battles
            // stay framed. Lateral maneuvers settle with forward travel rather than
            // continuing to oscillate after forward thrust has almost stopped.
            center += vec2(direction * scene.rect.width() * 0.055, 0.0)
                + rotate(
                    scene.rect.size()
                        * vec2(
                            direction * (advance.atan() + cruise_thrust * 0.10) * 0.13 * agility,
                            direction * (course + phase).cos() * 0.046 * agility,
                        ),
                    // High and low approach lanes converge toward the encounter instead
                    // of imposing the same upward tilt on every ship in the formation.
                    -direction * (visual.home.y - 0.40) * 0.60,
                );
            if let Some(target) = visual.bombing_target {
                // Keep each bomber's formation lane through the approach. A common focus
                // squeezed an entire raid into one overlapping stack at peak convergence.
                let lane = (visual.home - vec2(0.24, 0.44)) * scene.rect.size();
                let approach = scene.planet
                    + (vec2(-1.25, -0.55) + target * 0.25).normalized()
                        * scene.planet_radius
                        * 1.48
                    + lane * vec2(0.45, 0.55);
                let pass = smooth(0.5 + 0.5 * (course - 0.6).sin());
                center = center.lerp(approach, pass * 0.72);
            }
            {
                // Position, velocity and acceleration meet the cruising path smoothly, so
                // neither the hull nor its tangent-based heading snaps at the end of arrival.
                let visible_entry = 1.35;
                let entered = if actor.retreat_at.is_some_and(|at| at < visible_entry) {
                    // Immediate withdrawals begin at the defended world. Flying in first would
                    // leave their entire outward departure hidden beyond the viewport edge.
                    1.0
                } else {
                    // Ships enter already under thrust. A clamped, per-ship start delay
                    // introduces an instantaneous velocity change when that delay ends.
                    // Extend the curve slightly before zero for stable heading samples.
                    (time / visible_entry).clamp(-0.1, 1.0)
                };
                let remaining = 1.0 - entered;
                let edge = if mirror {
                    scene.rect.right() + size * 1.05
                } else {
                    scene.rect.left() - size * 1.05
                };
                // A short entrance only covers the gap from this outer lane to its edge.
                // Deeper capital lanes can already be visible when the shot opens; forcing
                // every hull across a whole viewport here creates a sprint and hard brake.
                let available = (direction * (center.x - edge)).max(0.0);
                let budget = 100.0 * scene.scale;
                let distance = budget * (available / budget.max(0.001)).tanh();
                let lane = (phase * 1.7).sin();
                // Preserve a visible banking arc even for lanes whose seeded phase lies
                // near zero; otherwise the level artwork exposes a straight slide-in.
                let bend = (0.85 + lane.abs() * 0.25).copysign(lane);
                let from = vec2(-direction * distance, -distance * visual.art_heading.tan());
                let through = vec2(
                    -direction * distance * 0.43,
                    from.y * 0.14
                        + bend
                            * 22.0
                            * agility.sqrt()
                            * scene.scale
                            * (distance / budget.max(0.001)),
                );
                center += from * remaining.powi(4) + through * (4.0 * remaining.powi(3) * entered);
            }
        }
        if let Some(retreat_at) = actor.retreat_at {
            let retreat = ((time - retreat_at) / 1.65).clamp(0.0, 1.0);
            // Bank away before accelerating outward, rather than cancelling forward thrust
            // and abruptly reversing on the same line.
            center.x -= direction * retreat.powi(3) * scene.rect.width() * 1.4;
            center.y -= (retreat * PI * 0.5).sin().powi(2) * scene.rect.height() * 0.8;
            angle -= direction * retreat * 0.25;
        }
        if actor.unit.is_ship() {
            center = self.station_berth(scene, index, time, center, size);
        }
        if self.show_planet && !visual.ground && actor.unit != Unit::space_dock() {
            // Both fleets stay in space even after the shield has fallen. Reserve room
            // for the entire hull, not just its center or the attacking fleet's nose.
            let offset = center - scene.planet;
            let clearance = scene.planet_radius * 1.11 + size * 0.56;
            // Anticipate the exclusion zone over a hull-sized band. The integrated
            // smoothstep joins the free path with continuous velocity and acceleration;
            // projecting abruptly onto a circle visibly snapped ships onto the shield rim.
            let distance = offset.length();
            let margin = (size * 0.8).max(28.0 * scene.scale);
            if distance < clearance + margin {
                let outward = if offset.length_sq() > 0.01 {
                    offset.normalized()
                } else {
                    vec2(-1.0, 0.0)
                };
                let blend = ((distance - clearance + margin) / (2.0 * margin)).clamp(0.0, 1.0);
                let radius = clearance + margin * blend.powi(3) * (2.0 - blend);
                center = scene.planet + outward * radius;
            }
        }
        ActorPose {
            center,
            size,
            angle,
            mirror,
        }
    }

    /// Strategic missiles are themselves the projectile: one straight descent replaces the
    /// fleet arrival, orbit and second projectile that ordinary combat actors use.
    fn incoming_missile_pose(&self, scene: Scene, index: usize, time: f32) -> ActorPose {
        let visual = &self.visuals[index];
        let actor = &self.timeline.actors[index];
        let strike = self.timeline.shots.iter().find(|shot| shot.source == index);
        let destination = strike
            .and_then(|shot| {
                shot.target
                    .map(|target| self.actor_cruise_pose(scene, target, shot.impact_at).center)
            })
            .unwrap_or_else(|| {
                let lane = noise(index as u32 + 911) - 0.5;
                scene.planet + vec2(-0.72, -0.24 + lane * 0.72) * scene.planet_radius
            });
        // An intercepted missile is still heading for the surface when it is destroyed. Giving
        // that unseen destination a little extra travel time leaves the recorded intercept aloft.
        let arrival = strike.map_or_else(
            || {
                actor
                    .retreat_at
                    .or(actor.death_at.map(|at| at + 0.8))
                    .unwrap_or(self.timeline.duration - 0.5)
            },
            |shot| shot.impact_at,
        );
        let lane = noise(index as u32 + 733) - 0.5;
        let start = pos2(
            scene.rect.left() - visual.size * scene.scale,
            (destination.y - scene.rect.height() * (0.18 + lane * 0.08))
                .clamp(scene.rect.top() + 20.0, scene.rect.bottom() - 20.0),
        );
        let progress = (time / arrival.max(0.1)).clamp(0.0, 1.0);
        ActorPose {
            center: start.lerp(destination, progress),
            size: visual.size * scene.scale,
            angle: 0.0,
            mirror: false,
        }
    }

    fn paint_actor(&self, painter: &Painter, scene: Scene, images: &ImageIds, index: usize) {
        let actor = &self.timeline.actors[index];
        if actor.death_at.is_some_and(|at| self.elapsed >= at)
            || self.missile_strike
                && actor.unit == Unit::interplanetary_missile()
                && self
                    .timeline
                    .shots
                    .iter()
                    .any(|shot| shot.source == index && self.elapsed >= shot.impact_at)
            || actor.retreat_at.is_some_and(|at| self.elapsed >= at + 1.7)
            || !self.visuals[index].visible
        {
            return;
        }
        let visual = &self.visuals[index];
        let mut pose = self.actor_pose(scene, index, self.elapsed);
        if let Some(pickup) = &self.resource_pickup {
            if !pickup.resources_for(index).is_empty() {
                pose.size *= pickup.scale_at(self.elapsed);
            }
        }
        // The painter clip is already in camera-space world coordinates. Culling against
        // the original canvas made arriving ships disappear while panning or zooming out.
        if !painter.clip_rect().expand(pose.size * 1.5).contains(pose.center) {
            return;
        }
        let state = actor.state_at(self.elapsed);
        let shield_strength = state.shield as f32 / actor.max_shield.max(1) as f32;
        if visual.ground {
            if self.gas_planet {
                // Settlements have painted lift platforms; defenses get compact floating pads.
                // There is no rocky ground or contact shadow on a gas world's cloud deck.
                if actor.initial_levels.is_none() {
                    painter.add(Shape::ellipse_filled(
                        pose.center + vec2(0.0, pose.size * 0.24),
                        vec2(pose.size * 0.40, pose.size * 0.13),
                        Color32::from_rgb(31, 51, 66),
                    ));
                }
                for jet in [-0.23, 0.23] {
                    let lift = pose.center + vec2(jet, 0.34) * pose.size;
                    let pulse = 0.35 + 0.15 * (self.elapsed * 4.0 + visual.phase).sin();
                    glow(painter, lift, pose.size * 0.15, BLUE, pulse);
                }
            } else {
                environment::paint_surface_shadow(painter, pose);
            }
        }
        self.paint_damage_wake(painter, scene, index);
        self.paint_ship_propulsion(painter, scene, index, pose);
        let cinematic_texture = images.0.get(&visual.texture);
        let texture = cinematic_texture.or_else(|| images.0.get(&visual.fallback_texture));
        if let Some((sheet, texture)) = visual
            .firing_sheet
            .and_then(|sheet| images.0.get(sheet.texture).map(|texture| (sheet, *texture)))
        {
            self.paint_firing_sprite(painter, scene, index, pose, sheet, texture);
        } else if let Some(texture) = texture {
            let aspect = if cinematic_texture.is_some() {
                visual.aspect
            } else {
                1.0
            };
            let dimensions = self.hull_dimensions(
                scene,
                index,
                pose,
                self.elapsed,
                vec2(pose.size * aspect.min(1.0), pose.size / aspect.max(1.0)),
            );
            sprites::paint_owner_hull(
                painter,
                *texture,
                pose.center,
                dimensions,
                pose.angle,
                pose.mirror,
                FULL_UV,
                visual.owner_color,
            );
            self.paint_hull_illumination(
                painter, scene, index, pose, *texture, dimensions, FULL_UV,
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
        if actor.levels_at(self.elapsed).is_some() {
            // Small industrial lights and drifting chimney exhaust animate the actual building,
            // without turning its levels into a row of fictitious independent structures.
            for light in 0..3 {
                let point = pose.center + vec2(-0.19 + light as f32 * 0.13, 0.13) * pose.size;
                let intensity =
                    0.35 + 0.4 * (self.elapsed * 1.7 + light as f32 + visual.phase).sin().powi(2);
                glow(painter, point, pose.size * 0.035, BLUE, intensity);
            }
            for puff in 0..5 {
                let age = (self.elapsed * 0.24 + puff as f32 / 5.0 + visual.phase).fract();
                let point = pose.center + vec2(-0.12 - age * 0.23, -0.21 - age * 0.43) * pose.size;
                environment::soft_ellipse(
                    painter,
                    point,
                    Vec2::splat(pose.size * (0.02 + age * 0.075)),
                    Color32::from_rgb(124, 144, 159),
                    (1.0 - age) * age * 0.34,
                );
            }
        }
        self.paint_damage_vent(painter, scene, index, pose);
    }

    fn paint_level_losses(&self, painter: &Painter, scene: Scene, images: &ImageIds) {
        let first =
            self.timeline.level_losses.partition_point(|loss| loss.impact_at < self.elapsed - 2.4);
        let last =
            self.timeline.level_losses.partition_point(|loss| loss.impact_at <= self.elapsed);
        if first == last {
            return;
        }
        let caption = painter.layout_no_wrap(
            "-1 level".into(),
            FontId::monospace((16.0 * scene.scale).max(12.0)),
            Color32::PLACEHOLDER,
        );
        // Actual glyph height varies with fonts and display scale. Match the saved impact
        // spacing to that measured height so rapid losses never draw over one another.
        let rise_speed = (52.0 * scene.scale)
            .max((caption.size().y + 2.0 * scene.scale.max(1.0)) / LEVEL_LOSS_INTERVAL);
        for loss in &self.timeline.level_losses[first..last] {
            if !self.visuals[loss.target].visible {
                continue;
            }
            let age = self.elapsed - loss.impact_at;
            let pose = self.actor_pose(scene, loss.target, loss.impact_at);
            // A partial loss gets a localized impact; only the final level triggers a full wreck.
            if loss.remaining_levels > 0 {
                explosion(
                    painter,
                    images,
                    pose.center,
                    pose.size * 0.43,
                    age,
                    self.timeline.actors[loss.target].unit,
                );
            }
            let position = pose.center
                + vec2(
                    -caption.size().x * 0.5,
                    -pose.size * 0.42 - age * rise_speed - caption.size().y,
                );
            let opacity = ((2.2 - age) / 0.6).clamp(0.0, 1.0);
            painter.galley(
                position + Vec2::splat(1.5 * scene.scale),
                caption.clone(),
                alpha(Color32::BLACK, opacity),
            );
            painter.galley(
                position,
                caption.clone(),
                alpha(Color32::from_rgb(255, 186, 111), opacity),
            );
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
        let start = self.actor_muzzle(scene, shot.source, shot.launch_at, end);
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
        let (mut start, end, target_size) = self.shot_geometry(scene, index, shot);
        let source = &self.timeline.actors[shot.source];
        let weapon = Weapon::for_shot(source.unit, &shot.outcome);
        let direct_missile = self.missile_strike && source.unit == Unit::interplanetary_missile();
        if !direct_missile && weapon.beam_width().is_some() {
            // A completed salvo's afterglow must not follow a War Sun into its new
            // planet-strike formation and appear to shoot toward an abandoned target.
            start = self.actor_muzzle(scene, shot.source, self.elapsed.min(shot.impact_at), end);
        }
        let color = weapon_color(weapon.color());
        let release = shot.launch_at;
        let charge_start = weapon.cinematic_charge_start(shot.launch_at, shot.impact_at);
        let stretch = ((shot.impact_at - release) / weapon.flight()).max(0.001);
        let age = (self.elapsed - release) / stretch;
        let progress = (age / weapon.flight()).clamp(0.0, 1.0);
        // Physical size is shared across a salvo, never scaled by damage or faction.
        let source_pose = self.actor_pose(scene, shot.source, shot.launch_at);
        // Surface formations can be dense, but their fire must remain readable against the
        // planet. Use the same weapon meshes/trails at a minimum visible gun calibre.
        let minimum_size = if self.visuals[shot.source].ground {
            60.0
        } else {
            26.0
        };
        let size = (source_pose.size * 0.55).clamp(minimum_size * scene.scale, 100.0 * scene.scale);
        let flight = WeaponFlight {
            weapon,
            origin: BevyVec3::new(start.x, start.y, 0.0),
            destination: BevyVec3::new(end.x, end.y, 0.0),
            size,
            lane: (index % 3) as f32 - 1.0,
        };
        if !direct_missile && self.elapsed < release && weapon.charge() > 0.0 {
            // The charging field follows its ship; the released projectile keeps its saved pose.
            let charging_muzzle = self.actor_muzzle(scene, shot.source, self.elapsed, end);
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
        if !direct_missile && age >= 0.0 {
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
            if matches!(weapon, Weapon::Missile | Weapon::Bomb) {
                paint_missile_smoke(painter, flight, age, scene.scale);
            }
            // Ordinary projectiles have continuous wakes. Re-emitting glowing particles on
            // top creates a dotted chain and makes several small weapons look like one beam.
            // Retain only the electrical, organic and repair accents that carry a distinct shape.
            if cinematic_particle_trail(weapon) {
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
        // Residual shield flashes and hull sparks travel with the struck vessel. Wrecks
        // stay at the recorded death pose instead of carrying their explosion offscreen.
        let struck_center = shot.target.map(|target| {
            let actor = &self.timeline.actors[target];
            let time = actor.death_at.map_or(self.elapsed, |death| self.elapsed.min(death));
            self.actor_pose(scene, target, time).center
        });
        let end = if shot.outcome.planetary_shield_damage == 0 {
            match (shot.target, struck_center) {
                (Some(target), Some(center)) => {
                    end + (center - self.actor_pose(scene, target, shot.impact_at).center)
                },
                _ => end,
            }
        } else {
            end
        };
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
            glow_arc(
                painter,
                center,
                radius,
                direction_angle - spread,
                spread * 2.0,
                1.4 * scene.scale,
                BLUE,
                strength * 0.75,
            );
            glow(painter, end, (18.0 + impact_age * 24.0) * scene.scale, BLUE, strength * 0.8);
            // The pressure front follows the shield curvature, not a flat circle over the hull.
            glow_arc(
                painter,
                center,
                radius + impact_age * 14.0 * scene.scale,
                direction_angle - spread * 1.45,
                spread * 2.9,
                scene.scale * 0.65,
                BLUE,
                strength * 0.26,
            );
            let tangent = vec2(-normal.y, normal.x);
            let flash = (1.0 - impact_age / 0.16).max(0.0).powi(2);
            glow_line(
                painter,
                end - tangent * 13.0 * scene.scale,
                end + tangent * 13.0 * scene.scale,
                scene.scale * 1.2,
                Color32::WHITE,
                flash * 0.8,
            );
        }
        if shot.outcome.hull_damage > 0 || (shot.outcome.killed && !shot.outcome.missed) {
            let hull = struck_center.unwrap_or(end);
            if shot.outcome.shield_damage > 0 {
                glow_line(painter, end, hull, 1.6 * scene.scale, color, strength * 0.65);
            }
            let flash = (1.0 - impact_age / 0.13).max(0.0).powi(2);
            glow(
                painter,
                hull,
                (15.0 + target_size * 0.16) * (1.0 + impact_age),
                GOLD,
                strength * 0.65,
            );
            glow(
                painter,
                hull,
                (7.0 + target_size * 0.06) * (1.0 + impact_age * 2.0),
                Color32::WHITE,
                flash,
            );
            paint_impact_sparks(
                painter,
                hull,
                start - end,
                impact_age,
                target_size * 0.75,
                index as u32,
                scene.scale,
            );
        }
    }

    fn planet_attack_focus(
        &self,
        scene: Scene,
        _attack: &super::cinematic_timeline::CinematicPlanetAttack,
    ) -> Pos2 {
        // The strike belongs to the planet. High fleet lanes and camera navigation must
        // never pull the convergence point northward or away from the target world.
        scene.planet + vec2(-1.5, -0.05) * scene.planet_radius
    }

    fn paint_planet_attacks(&self, painter: &Painter, scene: Scene, images: &ImageIds) {
        use super::effects::{death_ray_beam_layers, sustained_envelope, DEATH_RAY_FOCUS_AT};
        for attack in &self.timeline.planet_attacks {
            if self.elapsed < attack.start_at || self.elapsed > attack.end_at + 3.8 {
                continue;
            }
            let target = scene.planet + vec2(-0.48, -0.05) * scene.planet_radius;
            if self.elapsed >= attack.discharge_at
                && (!attack.destroyed || self.elapsed < attack.end_at)
            {
                self.paint_planet_ripples(painter, scene, self.elapsed - attack.discharge_at);
            }
            if self.elapsed < attack.end_at {
                let age = self.elapsed - attack.start_at;
                let focus = self.planet_attack_focus(scene, attack);
                let maximum_size = attack
                    .sources
                    .iter()
                    .map(|source| self.visuals[*source].size)
                    .fold(0.0_f32, f32::max)
                    * scene.scale
                    * 0.30;
                let combined_size =
                    maximum_size * (attack.sources.len() as f32).sqrt().clamp(1.0, 2.6);
                let layers = death_ray_beam_layers(combined_size);
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
                let focus_age = age - DEATH_RAY_FOCUS_AT;
                let focus_span = attack.end_at - attack.start_at - DEATH_RAY_FOCUS_AT;
                let focus_strength = sustained_envelope((focus_age / focus_span).clamp(0.0, 1.0));
                for source in &attack.sources {
                    // Every eligible War Sun charges and contributes its own ray. The live
                    // muzzle follows its ship while the common focus remains steady in space.
                    let pose = self.actor_pose(scene, *source, self.elapsed);
                    let size = pose.size * 0.30;
                    let muzzle = self.actor_muzzle(scene, *source, self.elapsed, focus);
                    if age < 1.65 {
                        let p = age / 1.65;
                        for spark in 0..28 {
                            let offset = Vec2::angled(spark as f32 * TAU / 28.0) * size * 2.2;
                            sprite(
                                "combat fx glow",
                                muzzle + offset * (1.0 - p),
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
                                muzzle,
                                Vec2::splat(size * (radius + (0.15 - radius) * smooth(p))),
                                radius * age,
                                gold,
                                particle_envelope(p) * 0.6,
                            );
                        }
                    }
                    if focus_age >= 0.0 {
                        let destination = muzzle.lerp(focus, smooth(focus_age / 0.38));
                        beam(muzzle, destination, size * 0.17, gold, focus_strength);
                        beam(muzzle, destination, size * 0.045, Color32::WHITE, focus_strength);
                        sprite(
                            "combat fx glow",
                            muzzle,
                            Vec2::splat(size * 0.85),
                            0.0,
                            gold,
                            focus_strength,
                        );
                    }
                }
                if focus_age >= 0.0 {
                    let growth = smooth(
                        focus_age / (attack.discharge_at - attack.start_at - DEATH_RAY_FOCUS_AT),
                    );
                    sprite(
                        "combat fx glow",
                        focus,
                        Vec2::splat(combined_size * (0.6 + growth * 1.7)),
                        0.0,
                        gold,
                        focus_strength,
                    );
                    sprite(
                        "combat fx glow",
                        focus,
                        Vec2::splat(combined_size * (0.2 + growth * 0.55)),
                        0.0,
                        Color32::WHITE,
                        focus_strength,
                    );
                    sprite(
                        "combat fx ring",
                        focus,
                        Vec2::splat(combined_size * (1.1 + growth * 0.9)),
                        age,
                        gold,
                        focus_strength * 0.6,
                    );
                }
                let discharge_age = self.elapsed - attack.discharge_at;
                if discharge_age >= 0.0 {
                    let progress =
                        (discharge_age / (attack.end_at - attack.discharge_at)).clamp(0.0, 1.0);
                    let strength = sustained_envelope(progress);
                    // These three shared glow/core layers describe one outgoing beam, regardless
                    // of fleet size; no particular ship is invented as the successful shooter.
                    for (width, color) in layers {
                        beam(
                            focus,
                            target,
                            width * (1.0 - 0.35 * smooth(progress)),
                            weapon_color(color),
                            strength,
                        );
                    }
                    sprite(
                        "combat fx glow",
                        target,
                        Vec2::splat(combined_size * (2.6 + progress * 2.0)),
                        0.0,
                        gold,
                        strength * 0.8,
                    );
                }
            } else if attack.destroyed {
                self.paint_planet_explosion(painter, scene, images, self.elapsed - attack.end_at);
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

    fn paint_planet_ripples(&self, painter: &Painter, scene: Scene, age: f32) {
        // A pressure front follows the spherical surface from the actual beam contact.
        // Project only the near hemisphere, so the wave wraps behind the limb naturally.
        let contact = BevyVec3::new(-0.25, -0.30, (1.0_f32 - 0.25 * 0.25 - 0.30 * 0.30).sqrt());
        let tangent = contact.cross(BevyVec3::Y).normalize();
        let bitangent = contact.cross(tangent);
        for wave in 0..3 {
            let progress = (age - wave as f32 * 0.34) / 2.8;
            if !(0.0..1.0).contains(&progress) {
                continue;
            }
            let spread = progress * PI;
            let strength = smooth(progress / 0.08) * (1.0 - progress).powi(2);
            let mut points = Vec::with_capacity(81);
            let wave_width = (1.8 + (1.0 - progress) * 1.6) * scene.scale;
            let paint_front = |points: Vec<Pos2>| {
                painter.add(Shape::line(
                    points.clone(),
                    Stroke::new(wave_width * 3.5, alpha(BLUE, strength * 0.22)),
                ));
                painter.add(Shape::line(
                    points,
                    Stroke::new(wave_width, alpha(Color32::from_rgb(224, 244, 255), strength)),
                ));
            };
            for step in 0..=80 {
                let angle = step as f32 * TAU / 80.0;
                let point = contact * spread.cos()
                    + (tangent * angle.cos() + bitangent * angle.sin()) * spread.sin();
                if point.z >= 0.0 {
                    points.push(scene.planet + vec2(point.x, point.y) * scene.planet_radius);
                } else if points.len() >= 2 {
                    paint_front(std::mem::take(&mut points));
                } else {
                    points.clear();
                }
            }
            if points.len() >= 2 {
                paint_front(points);
            }
        }
    }

    fn paint_planet_explosion(&self, painter: &Painter, scene: Scene, images: &ImageIds, age: f32) {
        // The same brief white-hot cover used by the schematic hides the texture swap.
        // Its opaque plateau spans the swap on both sides; atlas clouds draw over it.
        let cover = smooth((age - 0.45) / 0.15) * (1.0 - smooth((age - 0.78) / 0.40));
        if cover > 0.0 {
            let radius = scene.planet_radius + 2.0 * scene.scale;
            let color = alpha(Color32::from_rgb(255, 246, 228), cover);
            // Feather the opaque core into the clouds: a hard circular edge reads as a
            // flat replacement planet rather than the brief light of a detonation.
            let mut halo = Mesh::default();
            for step in 0..=128 {
                let angle = step as f32 * TAU / 128.0;
                let ray = Vec2::angled(angle);
                // Overlap the antialiased core edge so no dark seam appears at high zoom.
                for (distance, tint) in
                    [(radius - 4.0 * scene.scale, color), (radius * 1.38, Color32::TRANSPARENT)]
                {
                    halo.vertices.push(Vertex {
                        pos: scene.planet + ray * distance,
                        uv: bevy_egui::egui::epaint::WHITE_UV,
                        color: tint,
                    });
                }
                if step > 0 {
                    let i = step * 2;
                    halo.indices.extend_from_slice(&[i - 2, i - 1, i, i, i - 1, i + 1]);
                }
            }
            painter.add(Shape::mesh(halo));
            painter.circle_filled(scene.planet, radius, color);
        }
        // Reuse the schematic's atlas, secondary blasts, flashes, shock fronts and tiny
        // shards. Staggered surface clouds engulf the globe instead of stretching one frame.
        for cluster in 0..7_u32 {
            let (center, size, delay) = if cluster == 0 {
                (scene.planet, scene.planet_radius * 2.2, 0.0)
            } else {
                let angle = (cluster - 1) as f32 * TAU / 6.0 + 0.35;
                let offset = Vec2::angled(angle) * scene.planet_radius * 0.64;
                (
                    scene.planet + offset,
                    scene.planet_radius * (0.75 + noise(cluster + 913) * 0.20),
                    ((cluster - 1) % 3) as f32 * 0.24,
                )
            };
            let local_age = (age - delay) / PLANET_BLAST_TIME_SCALE;
            if local_age < 0.0 {
                continue;
            }
            super::effects::sample_wreck(
                size,
                Unit::planetary_shield(),
                local_age,
                |mut sample| {
                    if sample.texture == "combat fx shard" {
                        // Keep the shared, size-capped fragments near the former globe long
                        // enough to read against space once the explosion cloud has dissipated.
                        sample.center *= 0.40;
                        sample.color =
                            Color::srgb(0.90, 0.60, 0.32).with_alpha(sample.color.alpha());
                    }
                    paint_wreck_sample(painter, images, center, sample);
                },
            );
        }
    }
}

/// Stable planet identity chooses artwork without changing the persisted strategic-map image.
fn cinematic_planet_image(planet: &Planet) -> String {
    let kind = match planet.kind {
        PlanetKind::Dry => "dry",
        PlanetKind::Gas => "gas",
        PlanetKind::Ice => "ice",
        PlanetKind::Metallic => "metallic",
        PlanetKind::Water => "water",
        PlanetKind::Blue => "blue",
        PlanetKind::Brown => "brown",
        PlanetKind::Gray => "gray",
        PlanetKind::Red => "red",
        PlanetKind::Yellow => "yellow",
    };
    format!("planet {kind} {}", 1 + (planet.id ^ planet.image) % 2)
}

/// Match the opaque globe rim to the camera radius without resampling source artwork.
/// Generation leaves different transparent margins, especially around the rocky moons.
fn cinematic_planet_uv(image: &str) -> Rect {
    let diameter = match image {
        "planet dry 1" => 0.970,
        "planet dry 2" => 0.989,
        "planet ice 1" => 0.953,
        "planet ice 2" => 0.979,
        "planet metallic 1" => 0.950,
        "planet metallic 2" => 0.978,
        "planet blue 1" => 0.957,
        "planet blue 2" => 0.969,
        "planet brown 1" => 0.864,
        "planet brown 2" => 0.858,
        "planet gray 1" => 0.899,
        "planet gray 2" => 0.850,
        "planet red 1" => 0.921,
        "planet red 2" => 0.940,
        "planet yellow 1" => 0.927,
        "planet yellow 2" => 0.920,
        _ => 0.970,
    };
    Rect::from_center_size(pos2(0.5, 0.5), Vec2::splat(diameter))
}

/// Keep the full recorded roster; only the visible forces take space in these formations.
fn formation_group(actor: &CinematicActor) -> usize {
    if actor.initial_levels.is_some() {
        4
    } else if actor.unit.is_orbital() {
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
        0 => vec2(0.24, 0.44) + disk * vec2(0.15, 0.23),
        1 => vec2(0.76, 0.32) + disk * vec2(0.13, 0.15),
        2 => {
            // Deliberate rows make individual barrels and mobile support units readable.
            let columns = (count as f32 * 1.65).sqrt().ceil().max(1.0) as usize;
            let rows = count.div_ceil(columns).max(1);
            let row = slot / columns;
            let in_row = count.saturating_sub(row * columns).min(columns).max(1);
            let x = (slot % columns) as f32 - (in_row - 1) as f32 * 0.5;
            let y = row as f32 - (rows - 1) as f32 * 0.5;
            let stagger = if rows > 1 {
                if row.is_multiple_of(2) {
                    -0.025
                } else {
                    0.025
                }
            } else {
                0.0
            };
            let x = -0.08 + x * 1.10 / columns.max(2).saturating_sub(1) as f32 + stagger;
            vec2(x, -0.24 + y * 0.84 / rows.max(2).saturating_sub(1) as f32 + x * x * 0.10)
        },
        4 => vec2(
            -0.54 + (slot % 3) as f32 * 0.43,
            if count <= 3 {
                0.32
            } else {
                0.18 + (slot / 3) as f32 * 0.26
            },
        ),
        _ => disk * vec2(0.70, 0.20),
    }
}

/// Surface weapons fire from their raised gun housing rather than the ground-level base.
fn weapon_muzzle(pose: ActorPose, ground: bool, target: Pos2) -> Pos2 {
    let housing = pose.center
        + if ground {
            vec2(0.0, -pose.size * 0.24)
        } else {
            Vec2::ZERO
        };
    housing
        + (target - housing).normalized()
            * pose.size
            * if ground {
                0.32
            } else {
                0.23
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
        Unit::Defense(Defense::SpaceDock) => 340.0,
        Unit::Defense(Defense::AntiballisticMissile | Defense::InterplanetaryMissile) => 57.0,
        Unit::Defense(Defense::Crawler | Defense::RepairTruck) => 66.0,
        Unit::Defense(Defense::RocketLauncher) => 62.0,
        Unit::Defense(Defense::LightLaser) => 72.0,
        Unit::Defense(Defense::HeavyLaser) => 84.0,
        Unit::Defense(Defense::GaussCannon) => 102.0,
        Unit::Defense(Defense::IonCannon) => 116.0,
        Unit::Defense(Defense::PlasmaTurret) => 134.0,
        Unit::Building(_) if !unit.is_orbital() => 76.0,
        Unit::Building(_) => 95.0 + unit.production() as f32 * 13.0,
        Unit::Fauna(_) => 60.0 + unit.production() as f32 * 19.0,
    }
}

/// Source artwork keeps its natural canvas proportions; egui user textures do not expose sizes.
fn sprite_aspect(unit: Unit) -> f32 {
    if let Some(sheet) = firing_sheet(unit) {
        return sheet.aspect;
    }
    match unit {
        Unit::Ship(Ship::Probe) => 1.0,
        Unit::Ship(Ship::WarSun) => 1.2,
        Unit::Ship(_) => 1.5,
        _ => 1.0,
    }
}

/// Projected stern-to-bow direction in the original artwork, before the defender's mirror.
/// Preserve that camera perspective and use it for approach paths and attached engine plumes.
fn sprite_heading(unit: Unit) -> f32 {
    if firing_sheet(unit).is_some() {
        return 0.0;
    }
    let degrees: f32 = match unit {
        Unit::Ship(Ship::Probe | Ship::Destroyer | Ship::WarSun) => -30.0,
        Unit::Ship(Ship::ColonyShip | Ship::HeavyFighter) => -15.0,
        Unit::Ship(Ship::LightFighter | Ship::Cruiser) => -27.0,
        Unit::Ship(Ship::Bomber | Ship::Battleship) => 25.0,
        Unit::Ship(Ship::Dreadnought) => -23.0,
        Unit::Defense(Defense::InterplanetaryMissile) => -28.0,
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
    scale: f32,
) {
    let sample = flight.sample(progress);
    if sample.position.distance_squared(flight.origin) < 0.0001 {
        return;
    }
    let mut center = pos2(sample.center.x, sample.center.y);
    let mut size = vec2(sample.dimensions.x, sample.dimensions.y);
    let angle = sample.direction.y.atan2(sample.direction.x);
    let color = weapon_color(flight.weapon.color());
    let physical = matches!(flight.weapon, Weapon::Missile | Weapon::Bomb | Weapon::Repair);
    if matches!(flight.weapon, Weapon::Missile | Weapon::Bomb) {
        // Preserve the recorded leading tip while giving a distant guided round a readable body.
        size.y = size.y.max(3.2 * scale.max(0.65));
        size.x = size.x.max(7.0 * scale.max(0.65)).min(sample.position.distance(flight.origin));
        center = pos2(sample.position.x, sample.position.y) - Vec2::angled(angle) * size.x * 0.5;
    }
    if flight.weapon.beam_width().is_some() {
        glow_line(
            painter,
            pos2(flight.origin.x, flight.origin.y),
            pos2(sample.position.x, sample.position.y),
            size.y * 1.8,
            color,
            0.28,
        );
    } else if flight.weapon != Weapon::Repair {
        paint_projectile_wake(painter, flight, progress, scale);
    }
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
    for barrel in 0..flight.weapon.barrels() {
        let offset = barrel as f32 - (flight.weapon.barrels() - 1) as f32 * 0.5;
        let spacing = (size.y * 0.55).max(1.7 * scale.max(0.75));
        let barrel_center = center + rotate(vec2(0.0, offset * spacing), angle);
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
        if !physical {
            let direction = Vec2::angled(angle);
            let beam = flight.weapon.beam_width().is_some();
            let width = (size.y
                * if flight.weapon.barrels() > 1 {
                    0.11
                } else {
                    0.16
                })
            .max(
                if beam {
                    0.78
                } else {
                    0.68
                } * scale.max(0.75),
            );
            paint_weapon_core(
                painter,
                barrel_center - direction * size.x * 0.5,
                barrel_center + direction * size.x * 0.5,
                width,
                color,
                !beam,
                0.95,
            );
        }
    }
    if flight.weapon.beam_width().is_some() {
        let tip = pos2(sample.position.x, sample.position.y);
        glow(painter, tip, (size.y * 1.4).max(3.0 * scale), color, 0.45);
    }
}

/// Texture filtering can turn a subpixel core into dotted light. A continuous mesh retains
/// readable calibre, a bright front and tapered tail without changing a projectile's position.
fn paint_weapon_core(
    painter: &Painter,
    start: Pos2,
    end: Pos2,
    width: f32,
    color: Color32,
    tapered: bool,
    strength: f32,
) {
    let direction = end - start;
    if direction.length_sq() < 0.0001 || strength <= 0.0 {
        return;
    }
    let normal = vec2(-direction.y, direction.x).normalized();
    let rows = if tapered {
        [(0.0, 0.0, 0.0), (0.18, 0.65, 0.7), (0.78, 1.0, 1.0), (0.96, 0.55, 0.85), (1.0, 0.0, 0.0)]
    } else {
        [(0.0, 0.5, 0.6), (0.025, 1.0, 1.0), (0.50, 1.0, 1.0), (0.975, 1.0, 1.0), (1.0, 0.5, 0.75)]
    };
    let mut mesh = Mesh::default();
    for (along, spread, brightness) in rows {
        for (across, radiance, tint) in [
            (-1.8, 0.0, color),
            (-0.6, 0.65, color),
            (0.0, 1.0, Color32::WHITE),
            (0.6, 0.65, color),
            (1.8, 0.0, color),
        ] {
            mesh.colored_vertex(
                start + direction * along + normal * width * spread * across,
                emissive_color(tint, strength * brightness * radiance),
            );
        }
    }
    for row in 0..4 {
        for column in 0..4 {
            let a = row * 5 + column;
            mesh.indices.extend_from_slice(&[a, a + 1, a + 6, a, a + 6, a + 5]);
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn cinematic_particle_trail(weapon: Weapon) -> bool {
    matches!(
        weapon,
        Weapon::Ion
            | Weapon::FaunaSonic
            | Weapon::FaunaLightning
            | Weapon::FaunaBioPlasma
            | Weapon::FaunaGravity
            | Weapon::FaunaVoid
            | Weapon::FaunaFire
            | Weapon::FaunaExtinction
            | Weapon::Repair
    )
}

/// A continuous ribbon samples the projectile's own curved path, independent of frame rate.
/// Its transparent tail replaces the dotted look of isolated trail particles at high speed.
fn paint_projectile_wake(painter: &Painter, flight: WeaponFlight, progress: f32, scale: f32) {
    const STEPS: u32 = 10;
    let missile = matches!(flight.weapon, Weapon::Missile | Weapon::Bomb);
    let history = match flight.weapon {
        Weapon::Railgun => 0.22,
        Weapon::Broadside => 0.15,
        Weapon::Missile => 0.16,
        Weapon::Bomb => 0.09,
        _ => 0.065,
    };
    let from = (progress - history).max(0.0);
    let width = (flight.size
        * if missile {
            0.025
        } else {
            0.012
        })
    .max(0.55 * scale);
    let color = weapon_color(flight.weapon.color());
    let mut mesh = Mesh::default();
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let p = from + (progress - from) * t;
        let position = flight.position(p);
        let tangent = flight.position((p + 0.002).min(1.0)) - flight.position((p - 0.002).max(0.0));
        let normal = vec2(-tangent.y, tangent.x).normalized();
        let center = pos2(position.x, position.y);
        let spread = width
            * if missile {
                0.35 + (1.0 - t) * 1.35
            } else {
                0.3 + t * 0.7
            };
        let brightness = t
            * t
            * if missile {
                0.85
            } else {
                0.45
            };
        for (offset, intensity) in [(-2.8, 0.0), (0.0, brightness), (2.8, 0.0)] {
            mesh.colored_vertex(
                center + normal * spread * offset,
                emissive_color(color, intensity),
            );
        }
        if step > 0 {
            let current = step * 3;
            for column in 0..2 {
                let a = current + column;
                mesh.indices.extend_from_slice(&[a - 3, a, a + 1, a - 3, a + 1, a - 2]);
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// A continuous, expanding smoke ribbon follows a physical round's sampled path. Its dark
/// tail lingers after impact, while the bright exhaust belongs only to the flying projectile.
fn paint_missile_smoke(painter: &Painter, flight: WeaponFlight, age: f32, scale: f32) {
    const LIFE: f32 = 0.48;
    const STEPS: u32 = 14;
    let first = (age - LIFE).max(0.0);
    let last = age.min(flight.weapon.flight());
    if last <= first || !age.is_finite() {
        return;
    }
    let mut mesh = Mesh::default();
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let emitted = first + (last - first) * t;
        let progress = emitted / flight.weapon.flight();
        let smoke_age = age - emitted;
        let point = flight.position(progress);
        let tangent = flight.position((progress + 0.002).min(1.0))
            - flight.position((progress - 0.002).max(0.0));
        let normal = vec2(-tangent.y, tangent.x).normalized();
        let center = pos2(point.x, point.y)
            + normal * (emitted * 17.0).sin() * flight.size * 0.025 * smoke_age;
        let width = (flight.size * 0.040 + scale * 0.5) * (1.0 + smoke_age * 3.0);
        let opacity = particle_envelope(smoke_age / LIFE) * 0.30;
        for (across, density) in [(-1.0, 0.0), (-0.35, 0.65), (0.0, 1.0), (0.35, 0.65), (1.0, 0.0)]
        {
            mesh.colored_vertex(
                center + normal * width * across,
                alpha(Color32::from_rgb(111, 106, 104), opacity * density),
            );
        }
        if step > 0 {
            for column in 0..4 {
                let a = step * 5 + column;
                mesh.indices.extend_from_slice(&[a - 5, a, a + 1, a - 5, a + 1, a - 4]);
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Samples the shared particle lifetimes with feathered cinematic light and directional sparks.
fn paint_trail(
    painter: &Painter,
    images: &ImageIds,
    trail: WeaponTrail,
    age: f32,
    seed: u32,
    scale: f32,
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
            glow_line(
                painter,
                point(from),
                point(to),
                width * (1.0 - 0.6 * smooth(p)),
                weapon_color(color),
                (1.0 - p).powi(2),
            );
            // A beam begins fading immediately after contact; it must not blink out and
            // then brighten again under a particle's fade-in envelope.
            paint_weapon_core(
                painter,
                point(from),
                point(to),
                (width * 0.14).max(0.55 * scale.max(0.75)),
                weapon_color(color),
                false,
                (1.0 - p).powi(2) * color.to_srgba().alpha * 0.65,
            );
        },
        WeaponTrail::Sparks(at, size, color, count) => {
            let life = 0.42;
            if !(0.0..life).contains(&age) {
                return;
            }
            let p = age / life;
            for spark in 0..count {
                let angle = spark as f32 * 2.399_963 + noise(seed) * TAU;
                let direction = vec2(angle.cos(), angle.sin());
                let speed = size * (0.5 + (spark % 5) as f32 * 0.17);
                let center = point(at) + direction * speed * age;
                let tail = point(at) + direction * speed * (age - 0.045).max(0.0);
                glow_line(
                    painter,
                    tail,
                    center,
                    (size * 0.012).max(scale * 0.45),
                    weapon_color(color),
                    particle_envelope(p),
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
    if strength <= 0.0 || radius <= 0.0 || !radius.is_finite() || !strength.is_finite() {
        return;
    }
    // A single radial falloff avoids the visible concentric disks of stacked fills.
    // Additive vertices illuminate the artwork underneath instead of painting over it.
    const SEGMENTS: u32 = 20;
    let mut mesh = Mesh::default();
    mesh.colored_vertex(center, emissive_color(color, strength * 0.8));
    for (ring, (fraction, brightness)) in
        [(0.12, 0.48), (0.32, 0.16), (0.64, 0.025), (1.0, 0.0)].into_iter().enumerate()
    {
        for segment in 0..SEGMENTS {
            mesh.colored_vertex(
                center + Vec2::angled(segment as f32 * TAU / SEGMENTS as f32) * radius * fraction,
                emissive_color(color, strength * brightness),
            );
        }
        let current = 1 + ring as u32 * SEGMENTS;
        for segment in 0..SEGMENTS {
            let next = (segment + 1) % SEGMENTS;
            if ring == 0 {
                mesh.indices.extend_from_slice(&[0, current + segment, current + next]);
            } else {
                let previous = current - SEGMENTS;
                mesh.indices.extend_from_slice(&[
                    previous + segment,
                    current + segment,
                    current + next,
                    previous + segment,
                    current + next,
                    previous + next,
                ]);
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn glow_line(painter: &Painter, start: Pos2, end: Pos2, width: f32, color: Color32, strength: f32) {
    let delta = end - start;
    if width <= 0.0
        || strength <= 0.0
        || !width.is_finite()
        || !strength.is_finite()
        || delta.length_sq() < 0.0001
    {
        return;
    }
    let normal = vec2(-delta.y, delta.x).normalized();
    let mut mesh = Mesh::default();
    // Feather both the ends and the cross-section, retaining a narrow luminous core.
    for (along, envelope) in [(0.0, 0.0), (0.06, 1.0), (0.94, 1.0), (1.0, 0.0)] {
        for (across, brightness) in [
            (-2.5, 0.0),
            (-1.0, 0.08),
            (-0.25, 0.5),
            (0.0, 0.85),
            (0.25, 0.5),
            (1.0, 0.08),
            (2.5, 0.0),
        ] {
            mesh.colored_vertex(
                start + delta * along + normal * width * across,
                emissive_color(color, strength * envelope * brightness),
            );
        }
    }
    for row in 0..3 {
        for column in 0..6 {
            let a = row * 7 + column;
            mesh.indices.extend_from_slice(&[a, a + 1, a + 8, a, a + 8, a + 7]);
        }
    }
    painter.add(Shape::mesh(mesh));
}

/// Premultiplied RGB with zero alpha adds light without dimming the underlying scene.
fn emissive_color(color: Color32, strength: f32) -> Color32 {
    let strength = strength.clamp(0.0, 1.0);
    Color32::from_rgba_premultiplied(
        (color.r() as f32 * strength) as u8,
        (color.g() as f32 * strength) as u8,
        (color.b() as f32 * strength) as u8,
        0,
    )
}

fn arc(painter: &Painter, center: Pos2, radius: f32, start: f32, sweep: f32, stroke: Stroke) {
    let points =
        (0..=24).map(|i| center + Vec2::angled(start + sweep * i as f32 / 24.0) * radius).collect();
    painter.add(Shape::line(points, stroke));
}

/// A shield contact is a luminous pressure front, feathered across and along its curved rim.
fn glow_arc(
    painter: &Painter,
    center: Pos2,
    radius: f32,
    start: f32,
    sweep: f32,
    width: f32,
    color: Color32,
    strength: f32,
) {
    const STEPS: u32 = 24;
    let mut mesh = Mesh::default();
    for step in 0..=STEPS {
        let t = step as f32 / STEPS as f32;
        let direction = Vec2::angled(start + sweep * t);
        let envelope = (PI * t).sin().max(0.0).powf(0.8);
        for (distance, brightness) in
            [(-2.4, 0.0), (-0.8, 0.10), (0.0, 0.85), (0.8, 0.10), (2.4, 0.0)]
        {
            mesh.colored_vertex(
                center + direction * (radius + distance * width),
                emissive_color(color, strength * envelope * brightness),
            );
        }
        if step > 0 {
            for column in 0..4 {
                let a = step * 5 + column;
                mesh.indices.extend_from_slice(&[a - 5, a, a + 1, a - 5, a + 1, a - 4]);
            }
        }
    }
    painter.add(Shape::mesh(mesh));
}

fn paint_impact_sparks(
    painter: &Painter,
    center: Pos2,
    toward_source: Vec2,
    age: f32,
    radius: f32,
    seed: u32,
    scale: f32,
) {
    let incoming_angle = toward_source.y.atan2(toward_source.x);
    for fragment in 0..12_u32 {
        let variation = noise(seed.wrapping_mul(37).wrapping_add(fragment * 17));
        let lifetime = 0.24 + variation * 0.42;
        if !(0.0..lifetime).contains(&age) {
            continue;
        }
        // Most splinters spray back from the struck armor, with a few penetrating fragments.
        let angle = incoming_angle
            + (variation - 0.5) * 2.4
            + if fragment.is_multiple_of(4) {
                PI
            } else {
                0.0
            };
        let direction = Vec2::angled(angle);
        let speed = radius * (0.65 + noise(seed.wrapping_add(fragment * 73)) * 1.6);
        let travel = |time: f32| 1.0 - (-time.max(0.0) * 3.0).exp();
        let head = center + direction * speed * travel(age);
        let tail = center + direction * speed * travel(age - 0.045);
        let strength = (1.0 - age / lifetime).powi(2);
        glow_line(painter, tail, head, scale * (0.6 + variation * 0.55), GOLD, strength);
    }
}

fn explosion(painter: &Painter, images: &ImageIds, center: Pos2, size: f32, age: f32, unit: Unit) {
    super::effects::sample_wreck(size, unit, age, |sample| {
        // Retain the shared delayed, spinning hull fragments after the atlas flame clears.
        // Ring/glow samples stay omitted so destruction never becomes an opaque orange disk.
        if sample.atlas_frame.is_some() || sample.texture == "combat fx shard" {
            paint_wreck_sample(painter, images, center, sample);
        }
    });
}

fn paint_wreck_sample(
    painter: &Painter,
    images: &ImageIds,
    center: Pos2,
    sample: super::effects::WreckSample,
) {
    // Atlas fire and physical fragments share the exact seekable destruction clock.
    if sample.atlas_frame.is_none() && sample.texture != "combat fx shard" {
        return;
    }
    let Some(texture) = images.0.get(sample.texture) else {
        return;
    };
    let uv = sample.atlas_frame.map_or(FULL_UV, |frame| {
        Rect::from_min_max(
            pos2((frame % 8) as f32 / 8.0, (frame / 8) as f32 / 6.0),
            pos2((frame % 8 + 1) as f32 / 8.0, (frame / 8 + 1) as f32 / 6.0),
        )
    });
    rotated_image(
        painter,
        *texture,
        center + vec2(sample.center.x, -sample.center.y),
        vec2(sample.size.x, sample.size.y),
        -sample.rotation,
        false,
        uv,
        weapon_color(sample.color),
    );
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

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_motion.rs"]
mod motion_tests;

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_vfx.rs"]
mod vfx_tests;
