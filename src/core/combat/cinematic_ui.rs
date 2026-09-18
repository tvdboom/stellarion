//! Cinematic replay lifecycle and controls, separate from the schematic phase machine.

use bevy::prelude::*;
use bevy_egui::egui::epaint::text::{FontInsert, FontPriority, InsertFontFamily};
use bevy_egui::{egui, EguiContexts, EguiTextureHandle};

use super::cinematic::CinematicPlayback;
use super::effects::{wreck_cue, EffectTextures, Weapon};
use super::report::{combat_strength_ranges, MissionReport, ReportId, Side};
use super::result_banner;
use super::systems::combat_identity_participants;
use crate::core::audio::{set_ui_sound, PlayAudioMsg, SoundEffect, StopAudioMsg};
use crate::core::constants::BUTTON_TEXT_SIZE;
use crate::core::map::utils::{
    MAIN_BUTTON_BOTTOM, MAIN_BUTTON_HEIGHT, MAIN_BUTTON_RIGHT, MAIN_BUTTON_WIDTH,
};
use crate::core::player::Player;
use crate::core::settings::Settings;
use crate::core::states::GameState;
use crate::core::ui::systems::{viewport_ui_scale, UiState};
use crate::core::ui::utils::ImageIds;
use crate::multiplayer::client::MultiplayerSession;
use crate::utils::ToColor32;

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
            if !playback.actor_visible(shot.source)
                || shot.target.is_some_and(|target| !playback.actor_visible(target))
            {
                continue;
            }
            let weapon =
                Weapon::for_shot(playback.timeline.actors[shot.source].unit, &shot.outcome);
            if let Some(cue) = weapon.launch_cue() {
                cues.push((shot.launch_at, cue));
            }
            if let Some(cue) = weapon.impact_cue(&shot.outcome) {
                cues.push((shot.impact_at, cue));
            }
        }
        for (index, actor) in playback.timeline.actors.iter().enumerate() {
            if !playback.actor_visible(index) {
                continue;
            }
            if let Some(at) = actor.death_at {
                let (delay, cue) = wreck_cue(actor.unit);
                cues.push((at + delay, cue));
            }
        }
        for repair in &playback.timeline.repairs {
            if !playback.actor_visible(repair.target)
                || repair.source.is_some_and(|source| !playback.actor_visible(source))
            {
                continue;
            }
            cues.push((repair.start_at, PlayAudioMsg::new("repair")));
        }
        for attack in &playback.timeline.planet_attacks {
            cues.push((attack.start_at, PlayAudioMsg::new("death ray")));
        }
        cues.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Sound is a bounded mix of the same visible actions the renderer presents.
        let mut last = std::collections::BTreeMap::new();
        cues.retain(|(at, cue)| {
            let key = (cue.name, cue.volume.to_bits(), cue.playback_rate.to_bits());
            let previous = last.entry(key).or_insert(-1.0_f32);
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

    fn advance(&mut self, elapsed: f32, audio: &mut Messages<PlayAudioMsg>) {
        if elapsed < self.previous_time {
            self.next = 0;
        }
        self.previous_time = elapsed;
        let mut frame_cues = std::collections::BTreeSet::new();
        while let Some((at, cue)) = self.cues.get(self.next).filter(|(at, _)| *at <= elapsed) {
            // At high speed or after a stall, discard expired transients instead of playing a
            // backlog on top of the result banner. Preserve distinct heavy/light variations
            // of the same recording while bounding the complete mix to eight transients.
            let key = (cue.name, cue.volume.to_bits(), cue.playback_rate.to_bits());
            if elapsed - at < 0.35 && frame_cues.len() < 8 && frame_cues.insert(key) {
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

#[allow(clippy::too_many_arguments)]
pub(crate) fn advance_cinematic(
    playback: Option<ResMut<CinematicPlayback>>,
    time: Res<Time>,
    mut settings: ResMut<Settings>,
    state: Res<UiState>,
    player: Res<Player>,
    mut audio: ResMut<Messages<PlayAudioMsg>>,
    mut soundtrack: Option<ResMut<CinematicSoundtrack>>,
    keys: Res<ButtonInput<KeyCode>>,
    mut stop_audio: MessageWriter<StopAudioMsg>,
) {
    let Some(mut playback) = playback else {
        return;
    };
    if keys.any_pressed([KeyCode::ControlLeft, KeyCode::ControlRight])
        && keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight])
        && keys.just_pressed(KeyCode::ArrowLeft)
        && !keys.just_pressed(KeyCode::ArrowRight)
    {
        // Every visual is sampled from this clock, including restored hulls and the planet.
        // Keep the loaded sprite dimensions and the recorded report while rewinding.
        playback.elapsed = 0.0;
        settings.combat_paused = false;
        let mut sounds = std::collections::BTreeSet::from(["horn", "victory", "draw", "defeat"]);
        if let Some(soundtrack) = soundtrack.as_mut() {
            soundtrack.next = 0;
            soundtrack.previous_time = 0.0;
            sounds.extend(soundtrack.cues.iter().map(|(_, cue)| cue.name));
        }
        // Audio stops before new requests play in PostUpdate; leave music/drums running.
        audio.clear();
        for name in sounds {
            stop_audio.write(StopAudioMsg::new(name));
        }
        audio.write(PlayAudioMsg::new("horn"));
        return;
    }
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

#[derive(Default)]
pub(crate) struct CinematicHud {
    fonts_installed: bool,
    result_elapsed: f32,
    report_id: Option<ReportId>,
    attackers: Vec<(String, Color, u128)>,
    defenders: Vec<(String, Color, u128)>,
}

fn draw_identity(
    painter: &egui::Painter,
    viewport: egui::Rect,
    right: bool,
    role: &str,
    participants: &[(String, Color, u128)],
    fallback: egui::Color32,
) {
    let scale = viewport_ui_scale(viewport.size());
    let inset = 18.0 * scale;
    let max_width = (viewport.width() - inset * 3.0) * 0.5;
    // Match the schematic's Fira Mono role/name sizes at its reference viewport.
    let font_scale = (viewport.height() / 460.0).max(1.25);
    let family = egui::FontFamily::Name("combat identity".into());
    let role_font = egui::FontId::new(
        if participants.is_empty() {
            9.0
        } else {
            7.0
        } * font_scale,
        family.clone(),
    );
    let name_font = egui::FontId::new(9.0 * font_scale, family);
    let role_galley = painter.layout_no_wrap(
        role.into(),
        role_font,
        if participants.is_empty() {
            fallback
        } else {
            egui::Color32::from_rgb(166, 188, 211)
        },
    );
    let names: Vec<_> = participants
        .iter()
        .map(|(name, color, _)| {
            painter.layout_no_wrap(name.clone(), name_font.clone(), color.to_color32())
        })
        .collect();
    let text_width = names.iter().map(|name| name.size().x).fold(role_galley.size().x, f32::max);
    let text_height =
        role_galley.size().y + names.iter().map(|name| name.size().y + 1.0).sum::<f32>();
    let width = (if names.is_empty() {
        132.0
    } else {
        220.0
    } * scale)
        .max(text_width + 35.0 * scale)
        .min(max_width);
    let height = (38.0 + 16.0 * names.len() as f32).max(text_height + 14.0 * scale);
    let top = viewport.top() + 64.0 * scale;
    let left = if right {
        viewport.right() - inset - width
    } else {
        viewport.left() + inset
    };
    let panel = egui::Rect::from_min_size(egui::pos2(left, top), egui::vec2(width, height));
    painter.rect_filled(panel, 0.0, Color::srgba(0.025, 0.045, 0.07, 0.88).to_color32());
    let accent = egui::Rect::from_min_max(
        panel.left_top() + egui::vec2(10.0, 7.0) * scale,
        panel.left_bottom() + egui::vec2(13.0, -7.0) * scale,
    );
    if participants.is_empty() {
        painter.rect_filled(accent, 0.0, fallback);
    } else {
        let strengths: Vec<_> = participants.iter().map(|(_, _, strength)| *strength).collect();
        for ((_, color, _), (start, end)) in
            participants.iter().zip(combat_strength_ranges(&strengths))
        {
            if end > start {
                painter.rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(accent.left(), accent.top() + start * accent.height()),
                        egui::pos2(accent.right(), accent.top() + end * accent.height()),
                    ),
                    0.0,
                    color.to_color32(),
                );
            }
        }
    }
    let text_painter = painter.with_clip_rect(panel.shrink2(egui::vec2(12.0 * scale, 3.0)));
    // Keep the two role headings aligned even when only one side has allied fleets.
    let first_line_height =
        role_galley.size().y + names.first().map_or(0.0, |name| name.size().y + 1.0);
    let mut position = egui::pos2(
        panel.left() + 23.0 * scale,
        panel.top() + ((54.0 - first_line_height) * 0.5).max(7.0 * scale),
    );
    let role_height = role_galley.size().y;
    text_painter.galley(position, role_galley, egui::Color32::WHITE);
    position.y += role_height + 1.0;
    for name in names {
        let height = name.size().y;
        text_painter.galley(position, name, egui::Color32::WHITE);
        position.y += height + 1.0;
    }
}

fn exit_button_rect(viewport: egui::Rect) -> egui::Rect {
    let bottom_right = viewport.right_bottom() - egui::vec2(MAIN_BUTTON_RIGHT, MAIN_BUTTON_BOTTOM);
    egui::Rect::from_min_max(
        bottom_right - egui::vec2(MAIN_BUTTON_WIDTH, MAIN_BUTTON_HEIGHT),
        bottom_right,
    )
}

fn draw_exit_button(context: &egui::Context, images: &ImageIds) -> bool {
    let rect = exit_button_rect(context.content_rect());
    egui::Area::new(egui::Id::new("cinematic exit"))
        .fixed_pos(rect.min)
        .order(egui::Order::Foreground)
        .show(context, |ui| {
            let (rect, response) = ui.allocate_exact_size(rect.size(), egui::Sense::click());
            let response = response.on_hover_cursor(egui::CursorIcon::PointingHand);
            let hovered = response.hovered() && !response.is_pointer_button_down_on();
            if let Some(texture) = images.0.get("long button") {
                let top = if hovered {
                    0.5
                } else {
                    0.0
                };
                ui.painter().image(
                    *texture,
                    rect,
                    egui::Rect::from_min_max(egui::pos2(0.0, top), egui::pos2(1.0, top + 0.5)),
                    egui::Color32::WHITE,
                );
            } else {
                ui.painter().rect_filled(rect, 0.0, egui::Color32::from_rgb(20, 34, 48));
            }
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                "Exit combat",
                egui::FontId::new(BUTTON_TEXT_SIZE, egui::FontFamily::Name("combat action".into())),
                egui::Color32::WHITE,
            );
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Exit combat")
            });
            if response.clicked() {
                set_ui_sound(context, Some(SoundEffect::Button));
            }
            response.clicked()
        })
        .inner
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn draw_cinematic(
    mut contexts: EguiContexts,
    playback: Option<Res<CinematicPlayback>>,
    mut images: ResMut<ImageIds>,
    textures: Option<ResMut<Assets<Image>>>,
    state: Res<UiState>,
    player: Res<Player>,
    settings: Res<Settings>,
    time: Res<Time>,
    mut next: ResMut<NextState<GameState>>,
    session: Option<Res<MultiplayerSession>>,
    mut hud: Local<CinematicHud>,
) {
    let Some(playback) = playback else {
        next.set(GameState::CombatMenu);
        return;
    };
    // Register the schematic's generated masks once. Strong egui handles keep these textures
    // available across replays, and headless interaction tests can omit GPU asset resources.
    if !images.0.contains_key("combat fx glow") {
        if let Some(mut textures) = textures {
            for (name, handle) in EffectTextures::cinematic_images(&mut textures) {
                images.0.insert(name.into(), contexts.add_image(EguiTextureHandle::Strong(handle)));
            }
        }
    }
    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    if !hud.fonts_installed {
        for (name, bytes) in [
            (
                "combat identity",
                include_bytes!("../../../assets/fonts/FiraMono-Medium.ttf").as_slice(),
            ),
            ("combat action", include_bytes!("../../../assets/fonts/FiraSans-Bold.ttf").as_slice()),
        ] {
            context.add_font(FontInsert::new(
                name,
                egui::FontData::from_static(bytes),
                vec![InsertFontFamily {
                    family: egui::FontFamily::Name(name.into()),
                    priority: FontPriority::Highest,
                }],
            ));
        }
        hud.fonts_installed = true;
        // Named families become available on the next pass, before any labels use them.
        context.request_repaint();
        return;
    }
    // Bevy already handled playback shortcuts during Update. Avoid also activating or
    // adjusting a focused HUD control with those same key presses.
    context.input_mut(|input| {
        input.consume_key(egui::Modifiers::NONE, egui::Key::Space);
        input.consume_key(
            egui::Modifiers {
                ctrl: true,
                shift: true,
                ..egui::Modifiers::NONE
            },
            egui::Key::ArrowLeft,
        );
    });
    let report =
        state.in_combat.and_then(|id| player.reports.iter().find(|report| report.id == id));
    if !playback.is_finished() || hud.report_id != state.in_combat {
        hud.result_elapsed = 0.0;
    } else {
        hud.result_elapsed = (hud.result_elapsed + time.delta_secs() * settings.speed())
            .min(result_banner::ENTER_SECONDS);
    }
    if let Some(report) = report {
        if hud.report_id != Some(report.id)
            || session.as_ref().is_some_and(|session| session.is_changed())
        {
            let fallback = MultiplayerSession::default();
            let session = session.as_deref().unwrap_or(&fallback);
            hud.attackers = combat_identity_participants(report, &Side::Attacker, session);
            hud.defenders = combat_identity_participants(report, &Side::Defender, session);
            hud.report_id = Some(report.id);
        }
    }
    egui::Area::new(egui::Id::new("cinematic scene"))
        .fixed_pos(context.content_rect().min)
        .order(egui::Order::Background)
        .show(context, |ui| {
            let rect = context.content_rect();
            ui.set_min_size(rect.size());
            // Consume the scene's pointer area so clicks cannot select the map underneath.
            ui.allocate_rect(rect, egui::Sense::click_and_drag());
            playback.paint(ui.painter(), rect, &images);
            draw_identity(
                ui.painter(),
                rect,
                false,
                "ATTACKER",
                &hud.attackers,
                egui::Color32::from_rgb(150, 158, 170),
            );
            draw_identity(
                ui.painter(),
                rect,
                true,
                if report.is_some_and(MissionReport::is_space_fauna_encounter) {
                    "SPACE FAUNA"
                } else {
                    "DEFENDER"
                },
                &hud.defenders,
                egui::Color32::from_rgb(150, 158, 170),
            );
            if playback.is_finished() {
                let status = report.map_or("draw", |report| report.status(&player));
                let banner = egui::Rect::from_center_size(
                    rect.center(),
                    egui::vec2(
                        rect.width(),
                        (rect.height() * result_banner::BAR_HEIGHT_FRACTION)
                            .clamp(result_banner::BAR_MIN_HEIGHT, result_banner::BAR_MAX_HEIGHT),
                    ),
                );
                ui.painter().rect_filled(
                    banner,
                    0.0,
                    egui::Color32::from_black_alpha(result_banner::BAR_ALPHA),
                );
                if let Some(texture) = images.0.get(status) {
                    let art = result_banner::artwork(status);
                    let full_edge = (rect.width() * art.width_fraction).min(art.max_width);
                    let edge = full_edge * result_banner::entrance_scale(hud.result_elapsed);
                    ui.painter().with_clip_rect(banner).image(
                        *texture,
                        egui::Rect::from_center_size(
                            banner.center() + egui::vec2(0.0, full_edge * art.center_offset),
                            egui::Vec2::splat(edge),
                        ),
                        egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                        egui::Color32::WHITE,
                    );
                }
                if ui
                    .interact(banner, egui::Id::new("cinematic result"), egui::Sense::click())
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    next.set(GameState::CombatMenu);
                }
            } else if settings.combat_paused {
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "PAUSED",
                    egui::FontId::proportional((rect.width() * 0.026).clamp(20.0, 36.0)),
                    egui::Color32::from_rgb(225, 236, 248),
                );
            }
        });
    if draw_exit_button(context, &images) {
        next.set(GameState::CombatMenu);
    }
    if !settings.combat_paused
        && (!playback.is_finished() || hud.result_elapsed < result_banner::ENTER_SECONDS)
    {
        context.request_repaint();
    }
}

#[cfg(test)]
#[path = "../../../tests/core/combat_cinematic_ui.rs"]
mod tests;

#[cfg(all(test, target_os = "windows"))]
#[path = "../../../tests/core/combat_cinematic_capture.rs"]
mod capture;
