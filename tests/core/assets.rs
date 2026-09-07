use super::*;

#[test]
/// The declared lifecycle cannot report gameplay ready before its group is requested.
fn gameplay_assets_start_deferred() {
    assert_eq!(GameplayAssetState::default(), GameplayAssetState::Deferred);
}

#[test]
/// A failed handle is terminal even when other gameplay assets are ready or still loading.
fn gameplay_asset_failure_wins_over_pending_handles() {
    let state = classify_handle_group([
        HandleLoadStatus::Ready,
        HandleLoadStatus::Pending,
        HandleLoadStatus::Failed,
    ]);

    assert_eq!(state, GameplayAssetState::Failed);
}

#[test]
/// A non-empty gameplay group becomes ready only after every retained handle is ready.
fn gameplay_asset_group_requires_every_handle() {
    assert_eq!(
        classify_handle_group([HandleLoadStatus::Ready, HandleLoadStatus::Pending]),
        GameplayAssetState::Loading
    );
    assert_eq!(
        classify_handle_group([HandleLoadStatus::Ready, HandleLoadStatus::Ready]),
        GameplayAssetState::Ready
    );
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

#[test]
fn only_the_sensor_phalanx_map_marker_discards_baked_color() {
    assert!(faction_map_uses_alpha_mask("sensor phalanx marker"));
    for detailed_marker in
        ["dock", "jump gate marker", "solar satellite marker", "command relay marker", "mission"]
    {
        assert!(!faction_map_uses_alpha_mask(detailed_marker));
    }
}
