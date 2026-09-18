//! Cinematic replay lifecycle and controls, separate from the schematic phase machine.

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts};

use super::cinematic::CinematicPlayback;
use super::effects::Weapon;
use crate::core::audio::PlayAudioMsg;
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::ui::systems::UiState;
use crate::core::ui::utils::ImageIds;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CombatView {
    #[default]
    Schematic,
    Cinematic,
}

#[derive(Resource)]
pub(crate) struct CinematicSoundtrack {
    cues: Vec<(f32, PlayAudioMsg)>,
    next: usize,
    previous_time: f32,
}

impl CinematicSoundtrack {
    fn new(playback: &CinematicPlayback) -> Self {
        let mut cues = Vec::new();
        for shot in &playback.timeline.shots {
            if let Some(cue) =
                Weapon::for_unit(playback.timeline.actors[shot.source].unit).launch_cue()
            {
                cues.push((shot.launch_at, cue));
            }
            if !shot.outcome.missed {
                if shot.outcome.shield_damage > 0 || shot.outcome.planetary_shield_damage > 0 {
                    cues.push((shot.impact_at, PlayAudioMsg::new("shield impact")));
                }
                if shot.outcome.hull_damage > 0 {
                    cues.push((shot.impact_at, PlayAudioMsg::new("short explosion")));
                }
            }
        }
        for actor in &playback.timeline.actors {
            if let Some(at) = actor.death_at {
                cues.push((at, PlayAudioMsg::new("large explosion")));
            }
        }
        for repair in &playback.timeline.repairs {
            cues.push((repair.start_at, PlayAudioMsg::new("repair")));
        }
        for attack in &playback.timeline.planet_attacks {
            cues.push((attack.start_at, PlayAudioMsg::new("death ray")));
        }
        cues.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Sound is a bounded mix; every projectile and recorded outcome remains visible.
        let mut last = std::collections::BTreeMap::new();
        cues.retain(|(at, cue)| {
            let previous = last.entry(cue.name).or_insert(-1.0_f32);
            if *at - *previous < 0.09 {
                false
            } else {
                *previous = *at;
                true
            }
        });
        Self {
            cues,
            next: 0,
            previous_time: 0.0,
        }
    }

    fn advance(&mut self, elapsed: f32, audio: &mut MessageWriter<PlayAudioMsg>) {
        if elapsed < self.previous_time {
            self.next = 0;
        }
        self.previous_time = elapsed;
        let mut frame_cues = std::collections::BTreeSet::new();
        while let Some((at, cue)) = self.cues.get(self.next).filter(|(at, _)| *at <= elapsed) {
            // At high speed or after a stall, discard expired transients instead of playing a
            // backlog on top of the result banner. One cue per channel per frame is sufficient.
            if elapsed - at < 0.35 && frame_cues.insert(cue.name) {
                audio.write(cue.clone());
            }
            self.next += 1;
        }
    }
}

pub(crate) fn cinematic_selected(state: Option<Res<UiState>>) -> bool {
    // Run conditions are also evaluated before a match has installed its local UI state.
    state.is_some_and(|state| state.combat_view == CombatView::Cinematic)
}

pub(crate) fn setup_cinematic(
    mut commands: Commands,
    state: Res<UiState>,
    player: Res<Player>,
    mut audio: MessageWriter<PlayAudioMsg>,
    assets: Option<Res<crate::core::assets::WorldAssets>>,
    images: Option<Res<Assets<Image>>>,
) {
    if let Some(report) =
        state.in_combat.and_then(|id| player.reports.iter().find(|report| report.id == id))
    {
        let mut playback = CinematicPlayback::new(report);
        if let (Some(assets), Some(images)) = (assets, images) {
            for (name, handle) in &assets.images {
                if let Some(image) = images.get(handle) {
                    playback.set_sprite_size(name, image.width(), image.height());
                }
            }
        }
        commands.insert_resource(CinematicSoundtrack::new(&playback));
        commands.insert_resource(playback);
        audio.write(PlayAudioMsg::new("horn"));
    }
}

pub(crate) fn advance_cinematic(
    playback: Option<ResMut<CinematicPlayback>>,
    time: Res<Time>,
    settings: Res<Settings>,
    state: Res<UiState>,
    player: Res<Player>,
    mut audio: MessageWriter<PlayAudioMsg>,
    soundtrack: Option<ResMut<CinematicSoundtrack>>,
) {
    let Some(mut playback) = playback else {
        return;
    };
    let finished = playback.is_finished();
    playback.advance(time.delta_secs(), settings.combat_speed, settings.combat_paused);
    if let Some(mut soundtrack) = soundtrack {
        soundtrack.advance(playback.elapsed, &mut audio);
    }
    if !finished && playback.is_finished() {
        if let Some(report) =
            state.in_combat.and_then(|id| player.reports.iter().find(|report| report.id == id))
        {
            audio.write(PlayAudioMsg::new(report.status(&player)));
        }
    }
}

pub(crate) fn exit_cinematic(mut commands: Commands) {
    commands.remove_resource::<CinematicPlayback>();
    commands.remove_resource::<CinematicSoundtrack>();
}

pub(crate) fn draw_cinematic(
    mut contexts: EguiContexts,
    playback: Option<ResMut<CinematicPlayback>>,
    images: Res<ImageIds>,
    state: Res<UiState>,
    player: Res<Player>,
    mut settings: ResMut<Settings>,
    mut next: ResMut<NextState<GameState>>,
) {
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    // The shared Bevy shortcut already handled Space during Update. Do not let egui also
    // activate a focused playback button and undo that pause (or change speed) in this pass.
    context.input_mut(|input| {
        input.consume_key(egui::Modifiers::NONE, egui::Key::Space);
    });
    let Some(mut playback) = playback else {
        next.set(GameState::CombatMenu);
        return;
    };
    let report =
        state.in_combat.and_then(|id| player.reports.iter().find(|report| report.id == id));
    egui::Area::new(egui::Id::new("cinematic scene"))
        .fixed_pos(context.content_rect().min)
        .order(egui::Order::Background)
        .show(context, |ui| {
            let rect = context.content_rect();
            ui.set_min_size(rect.size());
            // Consume the scene's pointer area so clicks cannot select the map underneath.
            ui.allocate_rect(rect, egui::Sense::click_and_drag());
            playback.paint(ui.painter(), rect, &images);
            let title = report.map_or("Combat replay", |report| report.planet.name.as_str());
            let font_size = (rect.width() * 0.021).clamp(14.0, 25.0);
            ui.painter().text(
                rect.left_top() + egui::vec2(24.0, 24.0),
                egui::Align2::LEFT_TOP,
                title,
                egui::FontId::proportional(font_size),
                egui::Color32::from_rgb(216, 231, 244),
            );
            if playback.is_finished() {
                let status = report.map_or("Replay complete", |report| report.status(&player));
                let banner = egui::Rect::from_center_size(
                    rect.center(),
                    egui::vec2(rect.width(), (rect.height() * 0.18).clamp(64.0, 140.0)),
                );
                ui.painter().rect_filled(banner, 0.0, egui::Color32::from_black_alpha(190));
                ui.painter().text(
                    banner.center(),
                    egui::Align2::CENTER_CENTER,
                    status.to_uppercase(),
                    egui::FontId::proportional((rect.width() * 0.042).clamp(24.0, 64.0)),
                    egui::Color32::from_rgb(225, 236, 248),
                );
            }
        });
    egui::Area::new(egui::Id::new("cinematic playback controls"))
        .anchor(egui::Align2::CENTER_BOTTOM, egui::vec2(0.0, -18.0))
        .order(egui::Order::Foreground)
        .show(context, |ui| {
            egui::Frame::new()
                .fill(egui::Color32::from_rgba_unmultiplied(9, 17, 29, 238))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(62, 88, 116)))
                .corner_radius(8.0)
                .inner_margin(10)
                .show(ui, |ui| {
                    ui.set_max_width((context.content_rect().width() - 44.0).max(120.0));
                    ui.spacing_mut().button_padding = egui::vec2(10.0, 7.0);
                    ui.spacing_mut().interact_size.y = 32.0;
                    for style in [egui::TextStyle::Button, egui::TextStyle::Body] {
                        ui.style_mut().text_styles.insert(style, egui::FontId::proportional(16.0));
                    }
                    ui.horizontal_wrapped(|ui| {
                        if playback.is_finished() {
                            if ui.button("Replay").clicked() {
                                playback.restart();
                                settings.combat_paused = false;
                            }
                        } else if ui
                            .button(if settings.combat_paused {
                                "Play"
                            } else {
                                "Pause"
                            })
                            .on_hover_text("Space")
                            .clicked()
                        {
                            settings.combat_paused = !settings.combat_paused;
                        }
                        if ui
                            .add_enabled(settings.combat_speed > 0.25, egui::Button::new("−"))
                            .on_hover_text("Slower · Left arrow")
                            .clicked()
                        {
                            settings.combat_speed = (settings.combat_speed * 0.5).max(0.25);
                        }
                        ui.label(format!("{}×", settings.combat_speed));
                        if ui
                            .add_enabled(settings.combat_speed < 64.0, egui::Button::new("+"))
                            .on_hover_text("Faster · Right arrow")
                            .clicked()
                        {
                            settings.combat_speed = (settings.combat_speed * 2.0).min(64.0);
                        }
                        ui.separator();
                        if ui
                            .button("Close")
                            .on_hover_text("Return to battle selection · Esc")
                            .clicked()
                        {
                            next.set(GameState::CombatMenu);
                        }
                    });
                });
        });
    if !settings.combat_paused && !playback.is_finished() {
        context.request_repaint();
    }
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_ui.rs"]
mod tests;

#[cfg(all(test, target_os = "windows"))]
#[path = "../../../tests/core/combat_cinematic_capture.rs"]
mod capture;
