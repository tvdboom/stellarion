//! Full-screen loading artwork, measured progress, and the reveal into the menu or map.

use bevy::asset::LoadState;
use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiTextureHandle};
use rand::seq::IteratorRandom;

use crate::core::assets::WorldAssets;
use crate::core::basis_texture::BasisTextureSettings;
use crate::core::states::AppState;
use crate::multiplayer::client::{ConnectionStatus, MultiplayerRequest, MultiplayerSession};

const BACKGROUNDS: [&str; 7] =
    ["orbit", "station", "fleet", "expedition", "nebula", "outpost", "rift"];
const SLIDE_HOLD: f32 = 2.4;
const CROSSFADE: f32 = 0.8;
const MIN_DISPLAY: f32 = 1.2;
const REVEAL: f32 = 0.7;

/// Retains decorative art separately: missing wallpapers must never prevent the game loading.
#[derive(Resource)]
pub(crate) struct LoadingScreen {
    boot: bool,
    menu_texture: Option<egui::TextureId>,
    images: [Handle<Image>; BACKGROUNDS.len()],
    textures: [Option<egui::TextureId>; BACKGROUNDS.len()],
    start: usize,
    current: Option<usize>,
    next: Option<usize>,
    slide_elapsed: f32,
    artwork_elapsed: f32,
    elapsed: f32,
    progress: f32,
    ready: bool,
    ready_elapsed: f32,
    reveal_elapsed: Option<f32>,
    pub(crate) boot_error: Option<String>,
}

impl Default for LoadingScreen {
    fn default() -> Self {
        Self {
            boot: true,
            menu_texture: None,
            // Match wallpapers are optional and must not compete with startup assets.
            images: std::array::from_fn(|_| Handle::default()),
            textures: [None; BACKGROUNDS.len()],
            start: 0,
            current: None,
            next: None,
            slide_elapsed: 0.0,
            artwork_elapsed: 0.0,
            elapsed: 0.0,
            progress: 0.0,
            ready: false,
            ready_elapsed: 0.0,
            reveal_elapsed: None,
            boot_error: None,
        }
    }
}

impl LoadingScreen {
    /// Signals actual completion; a short full-bar pause makes the automatic handoff readable.
    pub(crate) fn finish(&mut self) -> bool {
        self.ready = true;
        // Startup reveals the menu as soon as its real work is done, without a display delay.
        if self.boot {
            self.progress = 1.0;
            return true;
        }
        self.elapsed >= MIN_DISPLAY && self.progress >= 1.0 && self.ready_elapsed >= 0.25
    }

    /// Includes the reveal: notifications must wait until the destination is fully visible.
    pub(crate) fn visible(&self) -> bool {
        self.reveal_elapsed.is_none_or(|elapsed| elapsed < REVEAL)
    }
}

pub(crate) fn begin_loading_screen(
    state: Res<State<AppState>>,
    server: Res<AssetServer>,
    mut screen: ResMut<LoadingScreen>,
) {
    // Keep the loading destination through the reveal, after AppState has already changed.
    screen.boot = *state.get() == AppState::Boot;
    if !screen.boot {
        screen.images = BACKGROUNDS.map(|name| {
            server
                .load_builder()
                .with_settings(|settings: &mut BasisTextureSettings| {
                    settings.linear_filtering = true;
                })
                .load(format!("images/bg/loading/{name}.basisu.ktx2"))
        });
        // This randomness affects presentation only, never the persisted simulation.
        screen.start =
            choose_background([true; BACKGROUNDS.len()], screen.current.or(Some(screen.start)))
                .unwrap_or(0);
    }
    screen.current = None;
    screen.next = None;
    screen.slide_elapsed = 0.0;
    screen.elapsed = 0.0;
    screen.progress = 0.0;
    screen.ready = false;
    screen.ready_elapsed = 0.0;
    screen.reveal_elapsed = None;
    screen.boot_error = None;
    screen.artwork_elapsed = 0.0;
}

pub(crate) fn reveal_loaded_screen(mut screen: ResMut<LoadingScreen>) {
    screen.reveal_elapsed = Some(0.0);
}

/// Updates only presentation timers. Progress comes from loaded dependencies and session readiness.
pub(crate) fn advance_loading_screen(
    time: Res<Time>,
    state: Res<State<AppState>>,
    server: Res<AssetServer>,
    assets: Res<WorldAssets>,
    images: Res<Assets<Image>>,
    session: Res<MultiplayerSession>,
    mut screen: ResMut<LoadingScreen>,
) {
    if !screen.visible() {
        return;
    }
    let delta = time.delta_secs();
    if matches!(state.get(), AppState::Boot | AppState::LoadingGame) {
        screen.elapsed += delta;
        if screen.ready {
            screen.ready_elapsed += delta;
        }
        let target = if screen.ready {
            1.0
        } else if *state.get() == AppState::Boot {
            assets.loading_progress(&server, false) * 0.8
                + if session.connection != ConnectionStatus::Initializing {
                    0.2
                } else {
                    0.0
                }
        } else {
            assets.loading_progress(&server, true) * 0.9
                + if session.active_game.is_some() && session.membership.is_some() {
                    0.05
                } else {
                    0.0
                }
        };
        screen.progress = if screen.boot {
            screen.progress.max(target)
        } else {
            advance_progress(screen.progress, target, delta)
        };
    } else if let Some(elapsed) = screen.reveal_elapsed.as_mut() {
        *elapsed += delta;
    }

    if screen.boot {
        return;
    }

    if screen.current.is_none() {
        // Honor the random choice instead of always taking the first texture to finish.
        if images.contains(&screen.images[screen.start]) {
            screen.current = Some(screen.start);
            screen.slide_elapsed = 0.0;
        } else if matches!(
            server.get_load_state(screen.images[screen.start].id()),
            Some(LoadState::Failed(_))
        ) {
            screen.current = choose_background(
                std::array::from_fn(|index| images.contains(&screen.images[index])),
                Some(screen.start),
            );
        }
    }
    screen.slide_elapsed += delta;
    if let Some(current) = screen.current {
        screen.artwork_elapsed += delta;
        if screen.slide_elapsed >= SLIDE_HOLD && screen.next.is_none() {
            screen.next = choose_background(
                std::array::from_fn(|index| images.contains(&screen.images[index])),
                Some(current),
            );
            // A slow optional download gets a complete crossfade when it finally arrives.
            screen.slide_elapsed = SLIDE_HOLD;
        }
        if screen.slide_elapsed >= SLIDE_HOLD + CROSSFADE {
            if let Some(next) = screen.next.take() {
                screen.current = Some(next);
                screen.slide_elapsed = 0.0;
            }
        }
    }
}

fn choose_background(ready: [bool; BACKGROUNDS.len()], previous: Option<usize>) -> Option<usize> {
    ready
        .into_iter()
        .enumerate()
        .filter_map(|(index, ready)| (ready && Some(index) != previous).then_some(index))
        .choose(&mut rand::rng())
}

fn advance_progress(current: f32, target: f32, delta: f32) -> f32 {
    current.max((current + delta.max(0.0) * 1.4).min(target.clamp(0.0, 1.0)))
}

fn smoothstep(value: f32) -> f32 {
    let value = value.clamp(0.0, 1.0);
    value * value * (3.0 - 2.0 * value)
}

/// Centered cover crop in normalized texture coordinates, preserving the artwork's aspect ratio.
fn cover_uv(source: Vec2, viewport: Vec2) -> egui::Rect {
    let ratio = (viewport.x / viewport.y.max(1.0)) / (source.x / source.y.max(1.0));
    let size = if ratio > 1.0 {
        egui::vec2(1.0, 1.0 / ratio)
    } else {
        egui::vec2(ratio, 1.0)
    };
    egui::Rect::from_center_size(egui::pos2(0.5, 0.5), size)
}

/// Covers the first rendered map frame as well, avoiding a black flash while entities are created.
pub(crate) fn draw_loading_screen(
    mut contexts: EguiContexts,
    state: Res<State<AppState>>,
    assets: Res<WorldAssets>,
    server: Res<AssetServer>,
    images: Res<Assets<Image>>,
    session: Res<MultiplayerSession>,
    mut requests: MessageWriter<MultiplayerRequest>,
    mut screen: ResMut<LoadingScreen>,
) {
    if !screen.visible() {
        return;
    }
    let menu_image = assets.image("menu");
    if screen.boot {
        if screen.menu_texture.is_none() && images.contains(&menu_image) {
            screen.menu_texture =
                Some(contexts.add_image(EguiTextureHandle::Strong(menu_image.clone())));
        }
    } else {
        for index in [screen.current, screen.next].into_iter().flatten() {
            if screen.textures[index].is_none() {
                screen.textures[index] = Some(
                    contexts.add_image(EguiTextureHandle::Strong(screen.images[index].clone())),
                );
            }
        }
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    let viewport = context.content_rect();
    let scale = (viewport.width() / 1200.0).min(viewport.height() / 760.0).clamp(0.7, 1.3);
    let alpha = 1.0 - smoothstep(screen.reveal_elapsed.unwrap_or(0.0) / REVEAL);
    if screen.boot {
        draw_boot_loading(
            context,
            screen.menu_texture.zip(images.get(&menu_image).map(Image::size_f32)),
            screen.progress,
            alpha,
            screen.boot_error.as_deref(),
        );
        return;
    }
    let error = if *state.get() == AppState::LoadingGame {
        session.menu_error.as_deref()
    } else {
        None
    };
    let status = if error.is_some() {
        "Loading interrupted"
    } else if screen.ready {
        "Ready. Entering the sector…"
    } else {
        assets.gameplay_loading_activity(&server)
    };
    egui::Area::new(egui::Id::new("stellarion_loading_screen"))
        .order(egui::Order::Foreground)
        .fixed_pos(viewport.min)
        .show(context, |ui| {
            ui.set_opacity(alpha);
            ui.set_min_size(viewport.size());
            // Consume pointer input until the destination has been revealed.
            ui.allocate_rect(viewport, egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(viewport, 0.0, egui::Color32::from_rgb(5, 11, 20));
            for (index, opacity) in [
                (screen.current, smoothstep(screen.artwork_elapsed / 0.6)),
                (screen.next, smoothstep((screen.slide_elapsed - SLIDE_HOLD) / CROSSFADE)),
            ] {
                if let Some(index) = index {
                    if let (Some(texture), Some(image)) =
                        (screen.textures[index], images.get(&screen.images[index]))
                    {
                        painter.image(
                            texture,
                            viewport,
                            cover_uv(
                                image.size_f32(),
                                Vec2::new(viewport.width(), viewport.height()),
                            ),
                            egui::Color32::WHITE.gamma_multiply(opacity),
                        );
                    }
                }
            }
            // A gentle dark gradient keeps text readable without a card obscuring the artwork.
            let mut gradient = egui::Mesh::default();
            for (position, opacity) in [
                (viewport.left_top(), 0.12),
                (viewport.right_top(), 0.12),
                (viewport.left_bottom(), 0.92),
                (viewport.right_bottom(), 0.92),
            ] {
                gradient.colored_vertex(position, egui::Color32::BLACK.gamma_multiply(opacity));
            }
            gradient.add_triangle(0, 1, 2);
            gradient.add_triangle(1, 3, 2);
            painter.add(gradient);

            let width = (560.0 * scale).min((viewport.width() - 40.0).max(1.0));
            let center = viewport.center().x;
            let bar_y = viewport.bottom() - 116.0 * scale;
            painter.text(
                egui::pos2(center, bar_y - 52.0 * scale),
                egui::Align2::CENTER_CENTER,
                "S T E L L A R I O N",
                egui::FontId::proportional(30.0 * scale),
                egui::Color32::from_rgb(237, 245, 255),
            );
            let bar = egui::Rect::from_center_size(
                egui::pos2(center, bar_y),
                egui::vec2(width, 6.0 * scale),
            );
            painter.rect_filled(bar, 3.0, egui::Color32::from_rgb(30, 45, 61));
            if screen.progress > 0.0 {
                let filled = egui::Rect::from_min_size(
                    bar.min,
                    egui::vec2(width * screen.progress, bar.height()),
                );
                painter.rect_filled(
                    filled.expand(3.0),
                    5.0,
                    egui::Color32::from_rgba_premultiplied(8, 30, 42, 65),
                );
                painter.rect_filled(filled, 3.0, egui::Color32::from_rgb(106, 204, 237));
            }
            painter.text(
                egui::pos2(bar.right(), bar.top() - 10.0 * scale),
                egui::Align2::RIGHT_BOTTOM,
                format!("{:.0}%", screen.progress * 100.0),
                egui::FontId::monospace(12.0 * scale),
                egui::Color32::from_rgb(177, 207, 226),
            );
            // Layout text using the available width, including narrow portrait windows.
            let galley = painter.layout(
                status.to_uppercase(),
                egui::FontId::proportional(13.0 * scale),
                egui::Color32::from_rgb(148, 177, 201),
                width,
            );
            painter.galley(
                egui::pos2(center - galley.size().x * 0.5, bar.bottom() + 16.0 * scale),
                galley,
                egui::Color32::WHITE,
            );
            if let Some(error) = error {
                // Errors replace the hero so arbitrarily long diagnostics remain scrollable.
                let size = egui::vec2(width, (viewport.height() * 0.5).min(320.0 * scale));
                let rect = egui::Rect::from_center_size(
                    egui::pos2(center, viewport.top() + viewport.height() * 0.35),
                    size,
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(12, 23, 36))
                        .inner_margin(16.0)
                        .corner_radius(6.0)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(size.y - 70.0).show(ui, |ui| {
                                ui.label(error);
                            });
                            if ui.button("Back to Menu").clicked() {
                                requests.write(MultiplayerRequest::LeaveGame);
                            }
                        });
                });
            }
        });
    context.request_repaint();
}

/// The startup artwork stays identical to the menu while its title and actions fade into view.
fn draw_boot_loading(
    context: &egui::Context,
    image: Option<(egui::TextureId, Vec2)>,
    progress: f32,
    alpha: f32,
    error: Option<&str>,
) {
    let viewport = context.content_rect();
    let scale = (viewport.width() / 1200.0).min(viewport.height() / 760.0).clamp(0.7, 1.3);
    egui::Area::new(egui::Id::new("stellarion_loading_screen"))
        .order(egui::Order::Foreground)
        .fixed_pos(viewport.min)
        .show(context, |ui| {
            ui.set_opacity(alpha);
            ui.set_min_size(viewport.size());
            ui.allocate_rect(viewport, egui::Sense::hover());
            let painter = ui.painter();
            if let Some((texture, size)) = image {
                painter.image(
                    texture,
                    viewport,
                    cover_uv(size, Vec2::new(viewport.width(), viewport.height())),
                    egui::Color32::WHITE,
                );
            } else {
                painter.rect_filled(viewport, 0.0, egui::Color32::from_rgb(5, 11, 20));
            }
            let center = viewport.center().x;
            let bottom = viewport.bottom() - 60.0 * scale;
            painter.text(
                egui::pos2(center, bottom - 28.0 * scale),
                egui::Align2::CENTER_CENTER,
                "Loading game",
                egui::FontId::proportional(18.0 * scale),
                egui::Color32::from_rgb(237, 245, 255),
            );
            painter.text(
                egui::pos2(center, bottom),
                egui::Align2::CENTER_CENTER,
                format!("{:.0}%", progress * 100.0),
                egui::FontId::monospace(14.0 * scale),
                egui::Color32::from_rgb(177, 207, 226),
            );
            if let Some(error) = error {
                let size = egui::vec2(
                    (560.0 * scale).min((viewport.width() - 40.0).max(1.0)),
                    (viewport.height() * 0.5).min(320.0 * scale),
                );
                let rect = egui::Rect::from_center_size(viewport.center(), size);
                ui.scope_builder(egui::UiBuilder::new().max_rect(rect), |ui| {
                    egui::Frame::new()
                        .fill(egui::Color32::from_rgb(12, 23, 36))
                        .inner_margin(16.0)
                        .corner_radius(6.0)
                        .show(ui, |ui| {
                            egui::ScrollArea::vertical().max_height(size.y).show(ui, |ui| {
                                ui.label(error);
                            });
                        });
                });
            }
        });
    context.request_repaint();
}

#[cfg(test)]
#[path = "../../tests/core/loading_screen.rs"]
mod tests;
