//! In-game notifications rendered with the egui version bundled by `bevy_egui`.

use std::collections::VecDeque;

use bevy::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPrimaryContextPass};

use crate::core::audio::PlayAudioMsg;
use crate::core::constants::MESSAGE_DURATION;

/// Severity used for notification color and sound selection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MessageLevel {
    /// Routine status information.
    Info,
    /// A recoverable problem or caution.
    Warning,
    /// An operation that failed.
    Error,
}

/// Requests a transient notification.
#[derive(Message, Clone, Debug)]
pub struct MessageMsg {
    /// User-facing notification text.
    pub message: String,
    /// Severity of the notification.
    pub level: MessageLevel,
}

impl MessageMsg {
    /// Creates a notification with an explicit severity.
    pub fn new(message: impl Into<String>, level: MessageLevel) -> Self {
        Self {
            message: message.into(),
            level,
        }
    }

    /// Creates an informational notification.
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, MessageLevel::Info)
    }

    /// Creates a warning notification.
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, MessageLevel::Warning)
    }

    /// Creates an error notification.
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, MessageLevel::Error)
    }
}

#[derive(Clone, Debug)]
/// One queued transient notification with its remaining display timer.
struct ActiveMessage {
    text: String,
    level: MessageLevel,
    remaining_seconds: f32,
}

/// Active notification queue.
#[derive(Resource, Default)]
pub struct Messages(VecDeque<ActiveMessage>);

impl Messages {
    /// Queues a notification and bounds retained messages to the configured capacity.
    fn push(&mut self, message: &MessageMsg) {
        self.0.push_back(ActiveMessage {
            text: message.message.clone(),
            level: message.level,
            remaining_seconds: MESSAGE_DURATION as f32,
        });

        // Keep an error storm from permanently covering the game viewport.
        while self.0.len() > 6 {
            self.0.pop_front();
        }
    }
}

/// Checks messages input/state and applies the resulting transition.
fn check_messages(
    mut contexts: EguiContexts,
    time: Res<Time>,
    mut messages: ResMut<Messages>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut message_msg: MessageReader<MessageMsg>,
) {
    // Only make one sound per severity per frame.
    let (mut info_sound, mut warning_sound, mut error_sound) = (true, true, true);

    for message in message_msg.read() {
        let play = match message.level {
            MessageLevel::Info if info_sound => {
                info_sound = false;
                Some("message")
            },
            MessageLevel::Warning if warning_sound => {
                warning_sound = false;
                Some("warning")
            },
            MessageLevel::Error if error_sound => {
                error_sound = false;
                Some("error")
            },
            _ => None,
        };
        if let Some(name) = play {
            play_audio_msg.write(PlayAudioMsg::new(name));
        }
        messages.push(message);
    }

    let elapsed = time.delta_secs();
    messages.0.retain_mut(|message| {
        message.remaining_seconds -= elapsed;
        message.remaining_seconds > 0.0
    });
    if messages.0.is_empty() {
        return;
    }

    let Ok(context) = contexts.ctx_mut() else {
        return;
    };
    egui::Area::new("stellarion_notifications".into())
        .anchor(egui::Align2::RIGHT_TOP, egui::vec2(-12.0, 70.0))
        .order(egui::Order::Foreground)
        .interactable(false)
        .show(context, |ui| {
            ui.set_max_width(360.0);
            for message in &messages.0 {
                let (fill, accent) = match message.level {
                    MessageLevel::Info => (
                        egui::Color32::from_rgba_unmultiplied(28, 36, 48, 235),
                        egui::Color32::from_rgb(112, 190, 255),
                    ),
                    MessageLevel::Warning => (
                        egui::Color32::from_rgba_unmultiplied(55, 43, 20, 240),
                        egui::Color32::from_rgb(255, 196, 82),
                    ),
                    MessageLevel::Error => (
                        egui::Color32::from_rgba_unmultiplied(58, 25, 29, 240),
                        egui::Color32::from_rgb(255, 105, 120),
                    ),
                };
                egui::Frame::new()
                    .fill(fill)
                    .stroke(egui::Stroke::new(1.0, accent))
                    .corner_radius(5.0)
                    .inner_margin(egui::Margin::symmetric(12, 8))
                    .show(ui, |ui| {
                        ui.label(egui::RichText::new(&message.text).small().color(accent));
                    });
                ui.add_space(6.0);
            }
        });
}

/// Installs notification collection, expiration, sound, and rendering.
#[derive(Default)]
pub struct MessagesPlugin;

impl Plugin for MessagesPlugin {
    /// Registers this plugin's resources, messages, and ordered systems.
    fn build(&self, app: &mut App) {
        app.init_resource::<Messages>().add_systems(EguiPrimaryContextPass, check_messages);
    }
}
