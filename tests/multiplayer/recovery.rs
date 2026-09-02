use super::*;

#[test]
/// Recovery secrets use four blocks and round-trip formatting.
fn recovery_codes_are_strong_and_parseable() {
    let code = RecoveryCode::generate().unwrap();
    assert_eq!(normalize(code.expose()).len(), RECOVERY_SYMBOLS);
    assert_eq!(code.expose().split('-').collect::<Vec<_>>().len(), 4);
    assert!(code.expose().split('-').all(|block| block.len() == 4));
    let reparsed = RecoveryCode::parse(code.expose().to_ascii_lowercase()).unwrap();
    assert!(code.hash().constant_time_eq(&reparsed.hash()));
}

#[test]
/// Previously issued ten-block recovery codes remain usable after shortening new codes.
fn accepts_legacy_recovery_codes() {
    let legacy = group(&"0".repeat(LEGACY_RECOVERY_SYMBOLS), 4);
    assert!(RecoveryCode::parse(legacy).is_ok());
}

#[test]
/// Malformed recovery strings fail before a backend request is made.
fn rejects_malformed_recovery_codes() {
    for invalid in ["", "ABC", "not-a-recovery-code!", "!!!!!!!!!!!!!!!!"] {
        assert!(matches!(RecoveryCode::parse(invalid), Err(RecoveryCodeError::Malformed)));
    }
}
