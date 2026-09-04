use super::*;

#[test]
fn ambiguous_delivery_freezes_and_retries_the_original_draft() {
    let mut pending = PendingTurnCommands::default();
    pending.reset(7);
    assert!(pending.begin_submission());
    assert!(!pending.is_editable());
    assert!(!pending.begin_submission());
    pending.submission = SubmissionState::Retry;
    assert!(pending.begin_submission());
    assert_eq!(pending.turn, 7);
    pending.submission = SubmissionState::Accepted;
    assert!(!pending.begin_submission());
    pending.reset(8);
    assert!(pending.is_editable());
}

#[test]
fn failed_delivery_and_same_turn_reload_preserve_the_locked_payload() {
    use super::super::*;
    use crate::core::units::Unit;
    let mut pending = PendingTurnCommands::default();
    pending.reset(1);
    let command = crate::core::simulation::TurnCommand::BuyUnits {
        planet_id: 0,
        unit: Unit::probe(),
        count: 1,
    };
    assert!(pending.push(command.clone()));
    assert!(pending.begin_submission());
    let payload = serde_json::to_value(&pending.commands).unwrap();
    let mut runtime = ClientRuntime {
        backend: None,
        realtime_config: None,
        storage: Arc::new(MemoryStorage::default()),
        profile: ClientProfile::default(),
        practice_return: None,
    };
    let mut session = MultiplayerSession::default();
    let mut form = MultiplayerForm::default();
    let mut next = NextState::default();
    apply_output(
        BackendOutput::Failed(Operation::Submit, BackendError::Offline("timeout".into())),
        &mut runtime,
        &mut session,
        &mut form,
        &mut pending,
        &mut next,
        true,
    );
    assert_eq!(pending.submission, SubmissionState::Retry);
    assert!(pending.can_accept_commands());
    assert!(pending.begin_submission());
    assert_eq!(serde_json::to_value(&pending.commands).unwrap(), payload);
    apply_output(
        BackendOutput::Submitted(1),
        &mut runtime,
        &mut session,
        &mut form,
        &mut pending,
        &mut next,
        true,
    );
    assert_eq!(pending.submission, SubmissionState::Accepted);
    assert!(!pending.begin_submission());
    assert!(pending.push(command.clone()));
    assert_eq!(serde_json::to_value(&pending.commands).unwrap(), payload);
    assert_eq!(pending.queued_commands.len(), 1);
    assert!(pending.resume_requested);
    apply_output(
        BackendOutput::Failed(Operation::Withdraw, BackendError::Offline("timeout".into())),
        &mut runtime,
        &mut session,
        &mut form,
        &mut pending,
        &mut next,
        true,
    );
    assert_eq!(pending.submission, SubmissionState::ResumeRetry);
    assert!(!pending.is_editable(), "orders stay safe until readiness is cleared");
    let mut draft = TurnSubmission::new(1, 1, pending.commands.clone());
    draft.generation = 1;
    apply_output(
        BackendOutput::Withdrawn(draft),
        &mut runtime,
        &mut session,
        &mut form,
        &mut pending,
        &mut next,
        true,
    );
    assert!(pending.is_editable());
    assert_eq!(pending.commands.len(), 2);
    assert_eq!(
        serde_json::to_value(&pending.commands[1]).unwrap(),
        serde_json::to_value(command).unwrap()
    );
    assert!(pending.queued_commands.is_empty());
    assert_eq!(pending.generation, 1);
    assert!(!pending.resume_requested);
    assert_eq!(session.submitted_turn, None);
}

#[test]
fn gameplay_actions_withdraw_readiness_without_mutating_in_flight_orders() {
    for submission in [
        SubmissionState::Sending,
        SubmissionState::Accepted,
        SubmissionState::Retry,
        SubmissionState::Resuming,
        SubmissionState::ResumeRetry,
    ] {
        let mut pending = PendingTurnCommands {
            submission,
            ..Default::default()
        };
        assert!(pending.can_accept_commands());
        assert!(!pending.resume_requested, "viewing controls must not clear readiness");
        assert!(pending.push(TurnCommand::BuyUnits {
            planet_id: 0,
            unit: crate::core::units::Unit::probe(),
            count: 1,
        }));
        assert!(pending.resume_requested);
        assert!(pending.commands.is_empty());
        assert_eq!(pending.queued_commands.len(), 1);
        assert!(!pending.begin_submission());
        pending.reset(2);
        assert!(pending.queued_commands.is_empty());
    }
}

#[test]
fn rejected_commands_do_not_withdraw_readiness() {
    let command = TurnCommand::BuyUnits {
        planet_id: 0,
        unit: crate::core::units::Unit::probe(),
        count: 1,
    };
    let mut pending = PendingTurnCommands {
        submission: SubmissionState::Loading,
        ..Default::default()
    };
    assert!(!pending.push(command.clone()));
    assert!(!pending.resume_requested);
    pending.submission = SubmissionState::Accepted;
    pending.commands = vec![command.clone(); MAX_COMMANDS_PER_SUBMISSION];
    assert!(!pending.push(command));
    assert!(!pending.resume_requested);
}
