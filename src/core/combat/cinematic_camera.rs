//! Local navigation over the replay canvas, independent of the movie clock and strategic map.

use bevy_egui::egui::{self, emath::TSTransform, vec2, Pos2, Rect, Vec2};

use crate::core::camera::{clamp_overscroll, settle_position};
use crate::core::constants::ZOOM_FACTOR;

const MIN_ZOOM: f32 = 0.65;
const MAX_ZOOM: f32 = 4.0;

#[derive(Clone, Debug)]
pub(crate) struct CinematicCamera {
    zoom: f32,
    /// Fractions of the unzoomed viewport preserve framing when the window is resized.
    center: Vec2,
    input_frame: Option<u64>,
}

impl Default for CinematicCamera {
    fn default() -> Self {
        Self {
            zoom: 1.0,
            center: Vec2::splat(0.5),
            input_frame: None,
        }
    }
}

impl CinematicCamera {
    pub fn transform(&self, viewport: Rect) -> TSTransform {
        let focus = viewport.min + viewport.size() * self.center;
        TSTransform {
            scaling: self.zoom,
            translation: viewport.center().to_vec2() - focus.to_vec2() * self.zoom,
        }
    }

    /// Background layers inherit only part of the battle camera's translation and zoom.
    pub fn parallax_transform(&self, viewport: Rect, depth: f32) -> TSTransform {
        let scaling = self.zoom.powf(depth * 0.7);
        let offset = (self.center - Vec2::splat(0.5)) * viewport.size();
        TSTransform {
            scaling,
            translation: viewport.center().to_vec2() * (1.0 - scaling) - offset * self.zoom * depth,
        }
    }

    fn bound_center(&mut self, viewport: Rect, held: bool, seconds: f32) -> bool {
        use bevy::math::Vec2 as BVec2;
        // Keep the viewport within a small margin around the original battle canvas. At the
        // widest zoom it returns to the center, while a held drag can still stretch its edges.
        let travel = (0.75 - 0.5 / self.zoom).max(0.0);
        let target = self.center.clamp(Vec2::splat(0.5 - travel), Vec2::splat(0.5 + travel));
        let size = BVec2::new(viewport.width(), viewport.height());
        let position = BVec2::new(self.center.x, self.center.y) * size;
        let target = BVec2::new(target.x, target.y) * size;
        let limited = target + clamp_overscroll(position - target, size / self.zoom);
        let next = if held {
            limited
        } else {
            settle_position(limited, target, seconds)
        };
        self.center = vec2(next.x / size.x, next.y / size.y);
        next != target
    }

    fn zoom_at(&mut self, viewport: Rect, pointer: Pos2, steps: f32) {
        let anchor = self.transform(viewport).inverse() * pointer;
        self.zoom =
            (self.zoom * ZOOM_FACTOR.powf(steps.clamp(-64.0, 64.0))).clamp(MIN_ZOOM, MAX_ZOOM);
        let focus = anchor - (pointer - viewport.center()) / self.zoom;
        self.center = (focus - viewport.min) / viewport.size();
        self.bound_center(viewport, true, 0.0);
    }

    fn pan(&mut self, viewport: Rect, screen_delta: Vec2) {
        self.center -= screen_delta / (viewport.size() * self.zoom);
        self.bound_center(viewport, true, 0.0);
    }

    /// Only the scene owns drag/wheel input; foreground buttons and popovers keep theirs.
    pub fn navigate(&mut self, response: &egui::Response, seconds: f32) {
        let context = &response.ctx;
        let frame = context.cumulative_frame_nr();
        if self.input_frame == Some(frame)
            || response.rect.width().min(response.rect.height()) < 1.0
        {
            return;
        }
        self.input_frame = Some(frame);
        if response.hovered() {
            context.input(|input| {
                if let Some(pointer) = input.pointer.hover_pos() {
                    for event in &input.events {
                        if let egui::Event::MouseWheel {
                            unit,
                            delta,
                            ..
                        } = event
                        {
                            if delta.y.is_finite() && delta.y != 0.0 {
                                let steps = delta.y
                                    * match unit {
                                        egui::MouseWheelUnit::Line => 1.0,
                                        egui::MouseWheelUnit::Point => 1.0 / 50.0,
                                        egui::MouseWheelUnit::Page => 6.0,
                                    };
                                self.zoom_at(response.rect, pointer, steps);
                            }
                        }
                    }
                }
            });
        }
        if response.dragged_by(egui::PointerButton::Primary) {
            self.pan(response.rect, response.drag_delta());
            context.set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if response.hovered() {
            context.set_cursor_icon(egui::CursorIcon::Grab);
        }
        if !context.egui_wants_keyboard_input() || response.has_focus() {
            let movement = context.input(|input| {
                if input.modifiers.any() || !input.focused {
                    return Vec2::ZERO;
                }
                let pressed = |key| u8::from(input.key_down(key)) as f32;
                vec2(
                    pressed(egui::Key::A) - pressed(egui::Key::D),
                    pressed(egui::Key::W) - pressed(egui::Key::S),
                )
            });
            if movement != Vec2::ZERO {
                // Match the strategic map's 600 screen-pixels-per-second movement, including
                // when playback is paused or running faster. Never jump after a stalled frame.
                self.pan(response.rect, movement * 600.0 * seconds.clamp(0.0, 0.1));
                context.request_repaint();
            }
        }
        // Keep the return animated even while playback is paused or the pointer is over a HUD.
        // Like the map, a held drag retains the stretch; keyboard movement can press against it.
        if self.bound_center(
            response.rect,
            response.is_pointer_button_down_on(),
            seconds.clamp(0.0, 0.1),
        ) {
            context.request_repaint();
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_camera.rs"]
mod tests;
