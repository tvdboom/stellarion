//! Crawler-style survivor pulses and floating resource gains in cinematic playback.

use super::*;
use crate::core::combat::systems::{
    SALVAGE_HIGHLIGHT_TIME_MS, SALVAGE_PICKUP_REVEAL_TIME_MS, SALVAGE_PICKUP_TIME_MS,
    SALVAGE_RETURN_TIME_MS,
};
use crate::core::resources::{ResourceName, Resources};

pub(super) struct ResourcePickup {
    pub actors: Vec<usize>,
    resources: Vec<Resources>,
    starts_at: f32,
    raiding: bool,
}

impl ResourcePickup {
    pub fn new(report: &MissionReport, timeline: &CinematicTimeline) -> Option<Self> {
        let (side, resources) = report.conclusion_resources()?;
        let raiding = side == Side::Attacker;
        let mut actors = timeline
            .actors
            .iter()
            .enumerate()
            .filter(|(_, actor)| {
                actor.side == side
                    && actor.death_at.is_none()
                    && actor.retreat_at.is_none()
                    && if side == Side::Defender {
                        actor.unit == Unit::crawler()
                    } else {
                        actor.unit.is_ship()
                    }
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        actors.sort_by_key(|index| (timeline.actors[*index].unit, timeline.actors[*index].id));
        if !raiding {
            actors.truncate(1);
        }
        let weights = actors
            .iter()
            .map(|index| timeline.actors[*index].unit.production().saturating_mul(10))
            .collect::<Vec<_>>();
        let shares = if raiding {
            super::super::rewards::resource_shares(resources, &weights)
        } else {
            vec![resources; actors.len()]
        };
        let mut resources = vec![Resources::default(); timeline.actors.len()];
        for (index, share) in actors.iter().zip(shares) {
            resources[*index] = share;
        }
        actors.retain(|index| !resources[*index].is_empty());
        if actors.is_empty() {
            return None;
        }
        Some(Self {
            actors,
            resources,
            starts_at: timeline.duration - super::super::cinematic_timeline::CLOSING_HOLD,
            raiding,
        })
    }

    pub fn resources_for(&self, actor: usize) -> Resources {
        self.resources[actor]
    }

    pub fn ends_at(&self) -> f32 {
        self.starts_at + (SALVAGE_HIGHLIGHT_TIME_MS + SALVAGE_PICKUP_TIME_MS) as f32 / 1000.0
    }

    pub fn scale_at(&self, time: f32) -> f32 {
        let elapsed = time - self.starts_at;
        let highlight = SALVAGE_HIGHLIGHT_TIME_MS as f32 / 1000.0;
        let returning = SALVAGE_RETURN_TIME_MS as f32 / 1000.0;
        if elapsed < 0.0 {
            1.0
        } else if elapsed < highlight {
            1.0 + 0.3 * smooth(elapsed / highlight)
        } else {
            1.0 + 0.3 * (1.0 - smooth(((elapsed - highlight) / returning).clamp(0.0, 1.0)))
        }
    }
}

impl CinematicPlayback {
    pub(crate) fn resource_pickup_cue(&self) -> Option<(f32, crate::core::audio::PlayAudioMsg)> {
        self.resource_pickup.as_ref().map(|pickup| {
            (
                pickup.starts_at + SALVAGE_HIGHLIGHT_TIME_MS as f32 / 1000.0,
                super::super::rewards::resource_pickup_cue(pickup.raiding),
            )
        })
    }

    pub(super) fn paint_resource_pickups(
        &self,
        painter: &Painter,
        scene: Scene,
        images: &ImageIds,
    ) {
        let Some(pickup) = &self.resource_pickup else {
            return;
        };
        let elapsed = self.elapsed - pickup.starts_at - SALVAGE_HIGHLIGHT_TIME_MS as f32 / 1000.0;
        let duration = SALVAGE_PICKUP_TIME_MS as f32 / 1000.0;
        if !(0.0..duration).contains(&elapsed) {
            return;
        }
        let reveal = SALVAGE_PICKUP_REVEAL_TIME_MS as f32 / 1000.0;
        let scale =
            smooth((elapsed / reveal).min(1.0)) * smooth(((duration - elapsed) / reveal).min(1.0));
        for actor in &pickup.actors {
            let resources = pickup.resources_for(*actor);
            let pose = self.actor_pose(scene, *actor, self.elapsed);
            let size = pose.size.clamp(40.0 * scene.scale, 90.0 * scene.scale);
            let gains = [ResourceName::Metal, ResourceName::Crystal, ResourceName::Deuterium]
                .into_iter()
                .filter(|kind| resources.get(kind) > 0);
            let middle = gains.clone().count().saturating_sub(1) as f32 * 0.5;
            let drift = 1.0 - (1.0 - elapsed / duration).powi(2);
            for (index, kind) in gains.enumerate() {
                let lane = index as f32 - middle;
                let center = pose.center
                    + if pickup.raiding {
                        vec2(0.0, size * (0.66 + index as f32 * 0.5 + 1.35 * drift))
                    } else {
                        vec2(lane * size * (0.82 + 0.2 * drift), -size * (0.66 + 1.35 * drift))
                    };
                let rect = Rect::from_center_size(center, vec2(size * 0.9, size * 0.48) * scale);
                painter.rect_filled(rect, 0.0, alpha(Color32::BLACK, 0.72));
                let key = match kind {
                    ResourceName::Metal => "metal",
                    ResourceName::Crystal => "crystal",
                    ResourceName::Deuterium => "deuterium",
                };
                if let Some(texture) = images.0.get(key) {
                    painter.image(
                        *texture,
                        Rect::from_center_size(
                            center + vec2(-size * 0.22 * scale, 0.0),
                            Vec2::splat(size * 0.34 * scale),
                        ),
                        FULL_UV,
                        Color32::WHITE,
                    );
                }
                if scale > 0.01 {
                    painter.text(
                        center + vec2(size * 0.18 * scale, 0.0),
                        bevy_egui::egui::Align2::CENTER_CENTER,
                        format!("+{}", resources.get(&kind)),
                        FontId::proportional(size * 0.27 * scale),
                        GREEN,
                    );
                }
            }
        }
    }
}
