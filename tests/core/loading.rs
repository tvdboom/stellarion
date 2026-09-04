use super::*;

#[test]
/// The loading lifecycle distinguishes deferred, in-flight, and ready groups.
fn loading_state_has_explicit_transitions() {
    assert_ne!(GameplayAssetState::Deferred, GameplayAssetState::Loading);
    assert_ne!(GameplayAssetState::Loading, GameplayAssetState::Ready);
    assert_ne!(GameplayAssetState::Loading, GameplayAssetState::Failed);
}
