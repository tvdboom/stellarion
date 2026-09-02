//! Reversible readiness preserves drafts and orders uncertain network retries.

use bevy::prelude::Resource;

use crate::core::simulation::{TurnCommand, MAX_COMMANDS_PER_SUBMISSION};

/// Delivery state of the local simultaneous-turn draft.
#[derive(Clone, Copy, Default, Debug, Eq, PartialEq)]
pub enum SubmissionState {
    /// Commands may still be edited.
    #[default]
    Draft,
    /// Recovering the saved draft after opening an existing game.
    Loading,
    /// A request is in flight; its payload must remain unchanged.
    Sending,
    /// Delivery was uncertain; retry the same immutable payload.
    Retry,
    /// The player is ready, but may continue while others are still playing.
    Accepted,
    /// Clearing readiness before the draft becomes editable again.
    Resuming,
    /// Clearing readiness failed; the next interaction can retry it safely.
    ResumeRetry,
}

/// Commands accumulated by the Bevy UI for the current simultaneous turn.
#[derive(Resource, Clone, Default)]
pub struct PendingTurnCommands {
    /// Turn for which commands are being collected.
    pub turn: u64,
    /// Intentional commands in local interaction order.
    pub commands: Vec<TurnCommand>,
    /// Orders readiness changes so delayed network requests cannot restore an old decision.
    pub generation: u64,
    /// Whether the payload can be edited, sent, or retried.
    pub submission: SubmissionState,
    /// The player asked to continue, including while the ready request was still in flight.
    pub resume_requested: bool,
}

impl PendingTurnCommands {
    /// Clears delivery state only when installing a canonical game/turn.
    pub fn reset(&mut self, turn: u64) {
        *self = Self {
            turn,
            ..Self::default()
        };
    }

    /// Whether gameplay commands can still be changed.
    pub fn is_editable(&self) -> bool {
        self.submission == SubmissionState::Draft
    }

    /// Queues a return to editing without racing an in-flight ready request.
    pub fn request_resume(&mut self) {
        if !self.is_editable() {
            self.resume_requested = true;
        }
    }

    /// Freezes the draft, or begins an identical retry after uncertain delivery.
    pub fn begin_submission(&mut self) -> bool {
        if self.resume_requested
            || !matches!(self.submission, SubmissionState::Draft | SubmissionState::Retry)
        {
            return false;
        }
        self.submission = SubmissionState::Sending;
        true
    }

    /// Appends an intent only while the bounded draft is editable.
    pub fn push(&mut self, command: TurnCommand) -> bool {
        if !self.is_editable() || self.commands.len() >= MAX_COMMANDS_PER_SUBMISSION {
            return false;
        }
        self.commands.push(command);
        true
    }

    /// Text displayed by the end-turn control.
    pub fn button_label(&self) -> &'static str {
        if self.resume_requested {
            return "Continuing…";
        }
        match self.submission {
            SubmissionState::Draft => "End turn",
            SubmissionState::Loading => "Loading turn…",
            SubmissionState::Sending | SubmissionState::Accepted => "Continue turn",
            SubmissionState::Retry => "Retry end turn",
            SubmissionState::Resuming => "Continuing…",
            SubmissionState::ResumeRetry => "Retry continue",
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/multiplayer/client_submission.rs"]
mod tests;
