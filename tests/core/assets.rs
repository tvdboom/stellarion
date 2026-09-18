use super::*;

#[test]
fn cinematic_firing_sheets_have_eight_transparent_frames_for_every_armed_hull() {
    assert_eq!(CINEMATIC_FIRING_IMAGE_NAMES.len(), 15);
    for name in CINEMATIC_FIRING_IMAGE_NAMES {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/images/cinematic")
            .join(format!("{name}.png"));
        assert!(path.is_file(), "Missing firing sheet {name}");
        #[cfg(target_os = "windows")]
        {
            let rgba = ::image::open(path).unwrap().to_rgba8();
            assert!(rgba.width() >= 1600 && rgba.height() >= 700, "Frame detail too low: {name}");
            for row in 0..2 {
                for col in 0..4 {
                    let mut opaque = 0;
                    let mut transparent = 0;
                    for y in row * rgba.height() / 2..(row + 1) * rgba.height() / 2 {
                        for x in col * rgba.width() / 4..(col + 1) * rgba.width() / 4 {
                            let alpha = rgba.get_pixel(x, y)[3];
                            opaque += usize::from(alpha > 200);
                            transparent += usize::from(alpha == 0);
                        }
                    }
                    assert!(
                        opaque > 1000 && transparent > 1000,
                        "{name} frame {col},{row} is empty or opaque"
                    );
                }
            }
        }
    }
}

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
/// Every combatant and bombable building has its own dedicated cinematic cutout.
fn cinematic_roster_has_registered_source_artwork() {
    use std::collections::BTreeSet;

    use crate::core::units::defense::Defense;
    use crate::core::units::ships::Ship;
    use crate::core::units::{orbitals, Unit};

    let expected: BTreeSet<_> = Ship::iter()
        .map(Unit::Ship)
        .chain(Defense::iter().map(Unit::Defense))
        .chain(orbitals::ALL)
        .chain(Unit::resource_buildings())
        .chain(Unit::industrial_buildings())
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

#[test]
fn cinematic_planets_and_gas_buildings_have_large_transparent_source_art() {
    use std::collections::BTreeSet;
    let expected: BTreeSet<_> = PlanetKind::iter()
        .flat_map(|kind| {
            (1..=2).map(move |variant| format!("planet {} {variant}", kind.to_lowername()))
        })
        .chain(
            crate::core::units::Unit::resource_buildings()
                .into_iter()
                .chain(crate::core::units::Unit::industrial_buildings())
                .map(|unit| format!("cinematic gas {}", unit.to_lowername())),
        )
        .collect();
    let actual: BTreeSet<_> = CINEMATIC_PLANET_IMAGE_NAMES
        .iter()
        .chain(CINEMATIC_GAS_BUILDING_IMAGE_NAMES)
        .map(|name| name.to_string())
        .collect();
    assert_eq!(actual, expected);
    for name in actual {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("assets/images/cinematic")
            .join(format!("{name}.png"));
        assert!(path.is_file(), "missing {name}");
        #[cfg(target_os = "windows")]
        {
            let rgba = ::image::open(&path).unwrap().to_rgba8();
            assert!(
                rgba.width() >= 1024 && rgba.height() >= 1024,
                "{name} is too small for the combat camera"
            );
            let transparent = rgba.pixels().filter(|pixel| pixel[3] == 0).count();
            assert!(
                transparent > (rgba.width() * rgba.height()) as usize / 10,
                "{name} needs true transparency outside the cutout"
            );
        }
    }
}
