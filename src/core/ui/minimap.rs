//! Live overview of the strategic map and its camera viewport.

use bevy::camera::NormalizedRenderTarget;
use bevy::picking::backend::{HitData, PointerHits};
use bevy::picking::pointer::{PointerId, PointerLocation};
use bevy::prelude::*;
use bevy_egui::egui::{self, Color32, CursorIcon, Order, PointerButton, Pos2, Stroke, StrokeKind};
use bevy_egui::{EguiContexts, EguiPickingOrder};

use super::systems::{scaled_hud_panel_frame, strategic_hud_scale, viewport_ui_scale, UiState};
use crate::core::camera::MainCamera;
use crate::core::map::model::Map;
use crate::core::map::systems::visible_planet_affiliation;
use crate::core::map::utils::{MAIN_BUTTON_BOTTOM, MAIN_BUTTON_HEIGHT, MAIN_BUTTON_RIGHT};
use crate::core::missions::Missions;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::multiplayer::client::MultiplayerSession;
use crate::utils::ToColor32;

const PANEL_ID: &str = "stellarion_minimap";
const PANEL_SIZE: f32 = 160.0;
const PANEL_GAP: f32 = 10.0;
const PANEL_PADDING: f32 = 3.0;

/// Keeps drawing and input enabled by the same preference.
pub(crate) fn minimap_shown(settings: Res<Settings>) -> bool {
    settings.show_minimap
}

/// Gives the minimap priority over world picking on the initial press.
pub(crate) fn block_minimap_world_picking(
    mut contexts: EguiContexts,
    pointers: Query<(&PointerId, &PointerLocation)>,
    camera: Single<(Entity, &Camera), With<MainCamera>>,
    window: Single<(Entity, &Window)>,
    order: Res<EguiPickingOrder>,
    mut hits: MessageWriter<PointerHits>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    // The background egui layer can receive widget input, but egui's pointer-over-area
    // check treats that layer as the world. Block sprite picks before press dispatch.
    let layer = egui::LayerId::new(Order::Background, egui::Id::new(PANEL_ID));
    let Some(panel) = context.memory(|memory| {
        memory.areas().visible_last_frame(&layer).then(|| memory.area_rect(layer.id)).flatten()
    }) else {
        return;
    };
    let (window_entity, window) = window.into_inner();
    let (camera_entity, camera) = camera.into_inner();
    let scale = window.scale_factor() / context.pixels_per_point();
    for (pointer, location) in &pointers {
        let Some(location) = location.location.as_ref() else {
            continue;
        };
        if matches!(location.target, NormalizedRenderTarget::Window(target) if target.entity() == window_entity)
            && panel.contains(Pos2::new(location.position.x * scale, location.position.y * scale))
        {
            hits.write(PointerHits::new(
                *pointer,
                vec![(camera_entity, HitData::new(camera_entity, 0.0, None, None))],
                camera.order as f32 + **order,
            ));
        }
    }
}

/// Paints the camera overview below floating HUD panels.
pub(crate) fn draw_minimap(
    mut contexts: EguiContexts,
    camera: Single<(&Camera, &GlobalTransform), With<MainCamera>>,
    window: Single<&Window>,
    map: Res<Map>,
    player: Res<Player>,
    missions: Res<Missions>,
    session: Res<MultiplayerSession>,
    mut state: ResMut<UiState>,
    mut markers: Local<Vec<WorldMarker>>,
    mut grab_offset: Local<Vec2>,
) {
    // Resolving an affiliation can read mission reports. Camera motion only needs reprojection.
    if map.is_changed() || player.is_changed() || missions.is_changed() || session.is_changed() {
        markers.clear();
        markers.extend(map.planets.iter().filter(|world| !world.is_destroyed).map(|world| {
            let color = visible_planet_affiliation(&map, &player, &missions.0, world)
                .map_or(Color32::from_rgb(157, 174, 191), |id| {
                    session.player_color(id).color().to_color32()
                });
            WorldMarker {
                position: world.position,
                color,
                radius: if world.is_moon() {
                    1.4
                } else {
                    2.35
                },
            }
        }));
    }

    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let (camera, transform) = camera.into_inner();
    let Some(view) = camera_bounds(camera, transform) else {
        return;
    };
    let viewport = context.content_rect();
    let display_scale = window.scale_factor() / context.pixels_per_point();
    let button_scale =
        viewport_ui_scale(egui::vec2(window.width(), window.height())) * display_scale;
    let Some(panel) = panel_rect(viewport, button_scale) else {
        return;
    };

    let scale = panel.width() / PANEL_SIZE;
    let frame = scaled_hud_panel_frame(scale)
        .inner_margin(egui::Margin::same((PANEL_PADDING * scale).round() as i8));
    egui::Area::new(egui::Id::new(PANEL_ID))
        .fixed_pos(panel.min)
        .default_size(panel.size())
        .order(Order::Background)
        .movable(false)
        .sense(egui::Sense::hover())
        .constrain(false)
        .fade_in(false)
        .show(context, |ui| {
            frame.show(ui, |ui| {
                let (canvas, _) = ui.allocate_exact_size(
                    panel.size() - frame.total_margin().sum(),
                    egui::Sense::hover(),
                );
                if let Some(minimap) = Minimap::fit(map.rect, canvas) {
                    let response = ui.interact(
                        minimap.chart,
                        ui.id().with("pan"),
                        egui::Sense::click_and_drag(),
                    );
                    if let Some(target) =
                        minimap.navigation_target(response, view, &mut grab_offset)
                    {
                        state.minimap_target = Some(target);
                    }
                    minimap.paint(ui.painter(), &markers, view, scale);
                }
            });
        });
}

fn camera_bounds(camera: &Camera, transform: &GlobalTransform) -> Option<Rect> {
    let viewport = camera.logical_viewport_rect()?;
    let top_left = camera.viewport_to_world_2d(transform, viewport.min).ok()?;
    let bottom_right = camera.viewport_to_world_2d(transform, viewport.max).ok()?;
    Some(Rect::from_corners(top_left, bottom_right))
}

fn panel_rect(viewport: egui::Rect, button_scale: f32) -> Option<egui::Rect> {
    let scale = strategic_hud_scale(viewport.size());
    let gap = PANEL_GAP * scale;
    // Keep the panel anchored to End turn even when floating HUD panels overlap it.
    let corner = viewport.max
        - egui::vec2(
            MAIN_BUTTON_RIGHT * button_scale,
            (MAIN_BUTTON_BOTTOM + MAIN_BUTTON_HEIGHT) * button_scale + gap,
        );
    let room = corner - viewport.min - egui::Vec2::splat(gap);
    let side = (PANEL_SIZE * scale).min(room.x).min(room.y);
    (side > 0.0).then(|| egui::Rect::from_min_max(corner - egui::Vec2::splat(side), corner))
}

struct Minimap {
    world: Rect,
    chart: egui::Rect,
    points_per_unit: f32,
}

/// Cached world position and appearance for a minimap dot.
pub(crate) struct WorldMarker {
    position: Vec2,
    color: Color32,
    radius: f32,
}

impl Minimap {
    fn fit(world: Rect, canvas: egui::Rect) -> Option<Self> {
        let size = world.size();
        if !size.is_finite() || size.min_element() <= 0.0 {
            return None;
        }
        let points_per_unit = (canvas.width() / size.x).min(canvas.height() / size.y);
        let chart = egui::Rect::from_center_size(
            canvas.center(),
            egui::vec2(size.x, size.y) * points_per_unit,
        );
        Some(Self {
            world,
            chart,
            points_per_unit,
        })
    }

    fn project(&self, point: Vec2) -> Pos2 {
        let offset = egui::vec2(point.x - self.world.min.x, self.world.max.y - point.y);
        self.chart.min + offset * self.points_per_unit
    }

    fn unproject(&self, point: Pos2) -> Vec2 {
        let offset = (point - self.chart.min) / self.points_per_unit;
        Vec2::new(self.world.min.x + offset.x, self.world.max.y - offset.y)
    }

    fn navigation_target(
        &self,
        response: egui::Response,
        view: Rect,
        grab_offset: &mut Vec2,
    ) -> Option<Vec2> {
        let primary = PointerButton::Primary;
        let outline = self.viewport_outline(view);
        if response.drag_started_by(primary) {
            let press = response.ctx.input(|input| input.pointer.press_origin());
            // Preserve where the player grabbed the outline, including when it is clipped.
            *grab_offset = press
                .filter(|&point| outline.is_some_and(|outline| outline.contains(point)))
                .map_or(Vec2::ZERO, |point| view.center() - self.unproject(point));
        }
        let dragging = response.dragged_by(primary);
        let stopped = response.drag_stopped_by(primary);
        if dragging {
            response.ctx.set_cursor_icon(CursorIcon::Grabbing);
        }
        let offset = if dragging || stopped {
            *grab_offset
        } else if response.clicked_by(primary) {
            Vec2::ZERO
        } else {
            let over_outline = response
                .hover_pos()
                .is_some_and(|point| outline.is_some_and(|outline| outline.contains(point)));
            response.on_hover_cursor(if over_outline {
                CursorIcon::Grab
            } else {
                CursorIcon::PointingHand
            });
            return None;
        };
        let point = response.interact_pointer_pos()?;
        Some((self.unproject(point) + offset).clamp(self.world.min, self.world.max))
    }

    fn viewport_outline(&self, view: Rect) -> Option<egui::Rect> {
        // Clip overscroll to the chart so the outline also shows when the whole map is visible.
        let outline = egui::Rect::from_two_pos(self.project(view.min), self.project(view.max))
            .intersect(self.chart);
        outline.is_positive().then_some(outline)
    }

    fn paint(&self, painter: &egui::Painter, markers: &[WorldMarker], view: Rect, scale: f32) {
        self.draw_grid(painter);
        let outline = self.viewport_outline(view);
        if let Some(outline) = outline {
            painter.rect_filled(outline, 0.0, Color32::from_white_alpha(24));
        }
        for marker in markers {
            painter.circle_filled(
                self.project(marker.position),
                marker.radius * scale,
                marker.color,
            );
        }
        if let Some(outline) = outline {
            painter.rect_stroke(
                outline,
                0.0,
                Stroke::new(1.25, Color32::WHITE),
                StrokeKind::Inside,
            );
        }
    }

    fn draw_grid(&self, painter: &egui::Painter) {
        let grid = Stroke::new(1.0, Color32::from_rgba_unmultiplied(102, 141, 176, 36));
        painter.rect_filled(self.chart, 0.0, Color32::from_rgb(7, 13, 22));
        painter.rect_stroke(self.chart, 0.0, grid, StrokeKind::Inside);
        for fraction in [0.25, 0.5, 0.75] {
            let point = self.chart.lerp_inside(egui::Vec2::splat(fraction));
            painter.vline(point.x, self.chart.y_range(), grid);
            painter.hline(self.chart.x_range(), point.y, grid);
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/core/ui_minimap.rs"]
mod tests;
