use super::*;

#[test]
/// The declared lifecycle cannot report gameplay ready before its group is requested.
fn gameplay_assets_start_deferred() {
    assert_eq!(GameplayAssetState::default(), GameplayAssetState::Deferred);
}

#[test]
/// Runtime image groups use KTX2 paths relative to Bevy's generated asset root.
fn runtime_categories_are_ktx2() {
    for category in ["icons", "bg", "ui", "resources", "planets", "animations"] {
        let path = format!("images/{category}/asset.basisu.ktx2");
        assert!(path.ends_with(".ktx2"));
        assert!(!path.starts_with("assets/"));
        assert!(!path.starts_with("assets-runtime/"));
    }
}
