use super::*;

#[test]
/// Recovery secrets use four blocks and round-trip formatting.
fn recovery_codes_are_strong_and_parseable() {
    let code = RecoveryCode::generate().unwrap();
    assert_eq!(normalize(code.expose()).len(), RECOVERY_SYMBOLS);
    assert_eq!(code.expose().split('-').collect::<Vec<_>>().len(), 4);
    assert!(code.expose().split('-').all(|block| block.len() == 4));
    let reparsed = RecoveryCode::parse(code.expose().to_ascii_lowercase()).unwrap();
    assert_eq!(code.expose(), reparsed.expose());
}

#[test]
/// Only the current four-block recovery-code shape is accepted.
fn rejects_non_current_recovery_code_lengths() {
    for length in [RECOVERY_SYMBOLS - 1, RECOVERY_SYMBOLS + 1, 39] {
        assert!(matches!(
            RecoveryCode::parse(group(&"0".repeat(length), 4)),
            Err(RecoveryCodeError::Malformed)
        ));
    }
}

#[test]
/// Malformed recovery strings fail before a backend request is made.
fn rejects_malformed_recovery_codes() {
    for invalid in ["", "ABC", "not-a-recovery-code!", "!!!!!!!!!!!!!!!!"] {
        assert!(matches!(RecoveryCode::parse(invalid), Err(RecoveryCodeError::Malformed)));
    }
}
