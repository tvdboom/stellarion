use std::time::Duration;

use bevy::prelude::*;
use bevy_egui::egui::RichText;
use bevy_egui::EguiContexts;
use egui_notify::{Anchor, Toast, Toasts};

use crate::core::audio::PlayAudioMsg;
use crate::core::constants::MESSAGE_DURATION;

pub enum MessageLevel {
    Info,
    Warning,
    Error,
}

#[derive(Message)]
pub struct MessageMsg {
    pub message: String,
    pub level: MessageLevel,
}

impl MessageMsg {
    pub fn new(message: impl Into<String>, level: MessageLevel) -> Self {
        Self {
            message: message.into(),
            level,
        }
    }

    pub fn info(message: impl Into<String>) -> Self {
        Self::new(message, MessageLevel::Info)
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(message, MessageLevel::Warning)
    }

    pub fn error(message: impl Into<String>) -> Self {
        Self::new(message, MessageLevel::Error)
    }
}

#[derive(Resource)]
pub struct Messages(pub Toasts);

impl Messages {
    pub fn info(&mut self, message: &String) -> &mut Toast {
        self.0
            .info(RichText::new(message).small())
            .duration(Some(Duration::from_secs(MESSAGE_DURATION)))
    }

    pub fn warning(&mut self, message: &String) -> &mut Toast {
        self.0
            .warning(RichText::new(message).small())
            .duration(Some(Duration::from_secs(MESSAGE_DURATION)))
    }

    pub fn error(&mut self, message: &String) -> &mut Toast {
        self.0
            .error(RichText::new(message).small())
            .duration(Some(Duration::from_secs(MESSAGE_DURATION)))
    }
}

fn check_messages(
    contexts: EguiContexts,
    mut messages: ResMut<Messages>,
    mut play_audio_msg: MessageWriter<PlayAudioMsg>,
    mut message_msg: MessageReader<MessageMsg>,
) {
    // Only make one sound per level per frame
    let (mut info, mut warning, mut error) = (true, true, true);

    for message in message_msg.read() {
        match message.level {
            MessageLevel::Info => {
                if info {
                    play_audio_msg.write(PlayAudioMsg::new("message"));
                    info = false;
                }
                messages.info(&message.message);
            },
            MessageLevel::Warning => {
                if warning {
                    play_audio_msg.write(PlayAudioMsg::new("warning"));
                    warning = false;
                }
                messages.warning(&message.message);
            },
            MessageLevel::Error => {
                if error {
                    play_audio_msg.write(PlayAudioMsg::new("error"));
                    error = false;
                }
                messages.error(&message.message);
            },
        };
    }

    messages.0.show(contexts.ctx().unwrap());
}

pub struct MessagesPlugin {
    builder: Option<fn() -> Toasts>,
}

impl Default for MessagesPlugin {
    fn default() -> Self {
        Self {
            builder: Some(|| {
                Toasts::default()
                    .with_margin([0., 70.].into())
                    .with_padding([5., 5.].into())
                    .with_anchor(Anchor::TopRight)
            }),
        }
    }
}

impl Plugin for MessagesPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Messages(self.builder.map(|f| f()).unwrap_or_default()))
            .add_systems(Update, check_messages);
    }
}
