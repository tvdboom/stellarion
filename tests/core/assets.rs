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
/// Every constructible hull, defense and orbital has its own dedicated cinematic cutout.
fn cinematic_roster_has_registered_source_artwork() {
    use std::collections::BTreeSet;

    use crate::core::units::defense::Defense;
    use crate::core::units::ships::Ship;
    use crate::core::units::{orbitals, Unit};

    let expected: BTreeSet<_> = Ship::iter()
        .map(Unit::Ship)
        .chain(Defense::iter().map(Unit::Defense))
        .chain(orbitals::ALL)
        .map(|unit| format!("cinematic {}", unit.to_lowername()))
        .collect();
    let registered: BTreeSet<_> =
        CINEMATIC_IMAGE_NAMES.iter().map(|name| name.to_string()).collect();
    assert_eq!(registered.len(), CINEMATIC_IMAGE_NAMES.len(), "duplicate cinematic asset key");
    assert_eq!(registered, expected, "cinematic registry must cover the complete shop roster");

    for name in CINEMATIC_IMAGE_NAMES {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/images/cinematic")
            .join(format!("{name}.png"));
        assert!(path.is_file(), "missing cinematic source artwork: {}", path.display());
        // The PNG decoder is already a Windows dependency for the native window icon.
        // Other targets still verify complete roster coverage and checked-in source files.
        #[cfg(target_os = "windows")]
        {
            let image = ::image::open(&path).unwrap_or_else(|error| panic!("{name}: {error}"));
            assert!(image.color().has_alpha(), "{name} must retain transparent edge pixels");
            let rgba = image.to_rgba8();
            let opaque = rgba.pixels().filter(|pixel| pixel[3] > 200).count();
            let transparent = rgba.pixels().filter(|pixel| pixel[3] == 0).count();
            let area = (u64::from(rgba.width()) * u64::from(rgba.height())) as usize;
            assert!(opaque > area / 20, "{name} has no substantial visible sprite");
            assert!(
                transparent > area / 10,
                "{name} must be an isolated cutout, not a shop thumbnail"
            );
        }
    }
}
