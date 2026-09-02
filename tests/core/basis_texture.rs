use super::*;

/// Reads one reproducibly generated runtime texture.
fn runtime_asset(path: &str) -> Vec<u8> {
    std::fs::read(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("assets-runtime").join(path),
    )
    .unwrap_or_else(|error| panic!("generated runtime asset {path} is missing: {error}"))
}

#[test]
/// Pure Rust produces valid RGBA data for arbitrary browser GPUs without compressed support.
fn transcodes_uastc_without_native_libraries() {
    let target = TranscodeTarget::from_features(WgpuFeatures::empty());
    let image =
        transcode_basis_texture(&runtime_asset("images/ui/button.basisu.ktx2"), target, &default())
            .expect("UASTC menu texture should transcode");
    assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
    assert_eq!(image.texture_descriptor.size.width, 135);
    assert_eq!(image.texture_descriptor.size.height, 29);
    assert_eq!(image.texture_descriptor.mip_level_count, 1);
}

#[test]
/// Odd-sized UI sprites avoid invalid BC texture descriptors on compressed-capable GPUs.
fn odd_sized_texture_falls_back_from_bc7() {
    let target = TranscodeTarget::from_features(WgpuFeatures::TEXTURE_COMPRESSION_BC);
    let image =
        transcode_basis_texture(&runtime_asset("images/ui/button.basisu.ktx2"), target, &default())
            .expect("odd-sized UASTC UI texture should transcode");
    assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
    assert_eq!(image.texture_descriptor.size.width, 135);
    assert_eq!(image.texture_descriptor.size.height, 29);
    assert_eq!(image.data.as_ref().map(Vec::len), Some(135 * 29 * 4));
}

#[test]
/// Mips and a GPU-compressed target survive the full generated-file decode path.
fn transcodes_mip_chain_to_bc7() {
    let target = TranscodeTarget::from_features(WgpuFeatures::TEXTURE_COMPRESSION_BC);
    let image =
        transcode_basis_texture(&runtime_asset("images/bg/menu.basisu.ktx2"), target, &default())
            .expect("mipmapped UASTC background should transcode");
    assert_eq!(image.texture_descriptor.format, TextureFormat::Bc7RgbaUnormSrgb);
    assert!(image.texture_descriptor.mip_level_count > 1);
}

#[test]
fn smooth_result_banners_preserve_pixels_alpha_and_mips_on_native_and_browser() {
    use bevy::image::ImageFilterMode;

    let bytes = runtime_asset("images/bg/victory.basisu.ktx2");
    for features in [WgpuFeatures::empty(), WgpuFeatures::TEXTURE_COMPRESSION_BC] {
        let target = TranscodeTarget::from_features(features);
        let original = transcode_basis_texture(&bytes, target, &default()).unwrap();
        let smooth = transcode_basis_texture(
            &bytes,
            target,
            &BasisTextureSettings {
                linear_filtering: true,
                ..default()
            },
        )
        .unwrap();
        assert_eq!(smooth.texture_descriptor.format, original.texture_descriptor.format);
        assert_eq!(smooth.texture_descriptor.size, original.texture_descriptor.size);
        assert_eq!(
            smooth.texture_descriptor.mip_level_count,
            original.texture_descriptor.mip_level_count
        );
        assert!(smooth.texture_descriptor.mip_level_count > 1);
        assert_eq!(smooth.data, original.data, "smoothing must preserve the artwork and alpha");
        let ImageSampler::Descriptor(sampler) = smooth.sampler else {
            panic!("result banners must override the app's nearest-neighbor default");
        };
        assert_eq!(sampler.mag_filter, ImageFilterMode::Linear);
        assert_eq!(sampler.min_filter, ImageFilterMode::Linear);
        assert_eq!(sampler.mipmap_filter, ImageFilterMode::Linear);
    }
}

#[test]
fn player_color_masks_preserve_alpha_in_every_mip_on_native_and_browser() {
    for name in ["mission", "mission jump", "dock"] {
        let bytes = runtime_asset(&format!("images/icons/{name}.basisu.ktx2"));
        let original = transcode_basis_texture(
            &bytes,
            TranscodeTarget::from_features(WgpuFeatures::empty()),
            &default(),
        )
        .unwrap();
        for features in [WgpuFeatures::empty(), WgpuFeatures::TEXTURE_COMPRESSION_BC] {
            let mask = transcode_basis_texture(
                &bytes,
                TranscodeTarget::from_features(features),
                &BasisTextureSettings {
                    alpha_mask: true,
                    ..default()
                },
            )
            .unwrap();
            assert_eq!(mask.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
            assert_eq!(mask.texture_descriptor.size, original.texture_descriptor.size);
            assert_eq!(
                mask.texture_descriptor.mip_level_count,
                original.texture_descriptor.mip_level_count
            );
            let pixels = mask.data.as_ref().unwrap();
            let source = original.data.as_ref().unwrap();
            assert_eq!(pixels.len(), source.len());
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
            for (pixel, source_pixel) in pixels.chunks_exact(4).zip(source.chunks_exact(4)) {
                assert_eq!(&pixel[..3], &[255; 3]);
                assert_eq!(pixel[3], source_pixel[3], "{name}: silhouette coverage changed");
            }
        }
    }
}

#[test]
fn mission_ui_artwork_preserves_transparency_and_hover_on_native_and_browser() {
    for name in ["mission", "mission jump", "mission hover", "mission jump hover", "dock"] {
        let bytes = runtime_asset(&format!("images/icons/{name}.basisu.ktx2"));
        let original = transcode_basis_texture(
            &bytes,
            TranscodeTarget::from_features(WgpuFeatures::empty()),
            &default(),
        )
        .unwrap();
        for features in [WgpuFeatures::empty(), WgpuFeatures::TEXTURE_COMPRESSION_BC] {
            let image = transcode_basis_texture(
                &bytes,
                TranscodeTarget::from_features(features),
                &BasisTextureSettings {
                    premultiply_alpha: true,
                    ..default()
                },
            )
            .unwrap();
            assert_eq!(image.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
            assert_eq!(image.texture_descriptor.size, original.texture_descriptor.size);
            assert_eq!(
                image.texture_descriptor.mip_level_count,
                original.texture_descriptor.mip_level_count
            );
            let pixels = image.data.as_ref().unwrap();
            let source = original.data.as_ref().unwrap();
            assert_eq!(pixels.len(), source.len());
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 0));
            assert!(pixels.chunks_exact(4).any(|pixel| pixel[3] == 255));
            assert!(pixels.chunks_exact(4).any(|pixel| (1..255).contains(&pixel[3])));
            for (pixel, source_pixel) in pixels.chunks_exact(4).zip(source.chunks_exact(4)) {
                assert_eq!(pixel[3], source_pixel[3], "{name}: coverage changed");
                match pixel[3] {
                    // Egui adds texture RGB directly: even zero-alpha pixels must be black.
                    0 => assert_eq!(pixel, &[0; 4], "{name}: transparent area adds color"),
                    255 => assert_eq!(pixel, source_pixel, "{name}: artwork color changed"),
                    alpha => assert!(pixel[..3].iter().all(|channel| *channel <= alpha)),
                }
            }
        }
    }
}

#[test]
fn masked_icon_loads_separate_ui_artwork() {
    use std::time::{Duration, Instant};

    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        AssetPlugin {
            file_path: format!("{}/assets-runtime", env!("CARGO_MANIFEST_DIR")),
            meta_check: bevy::asset::AssetMetaCheck::Never,
            ..default()
        },
    ))
    .init_asset::<Image>()
    .register_asset_loader(BasisTextureLoader::from_features(WgpuFeatures::empty()));

    let server = app.world().resource::<AssetServer>().clone();
    let load = |path: &str| -> Handle<Image> {
        server
            .load_builder()
            .with_settings(|settings: &mut BasisTextureSettings| settings.alpha_mask = true)
            .load(path.to_string())
    };
    let sprite = load("images/icons/mission.basisu.ktx2");
    let ui = load("images/icons/mission.basisu.ktx2#ui");
    let deadline = Instant::now() + Duration::from_secs(10);
    while !(server.is_loaded_with_dependencies(sprite.id())
        && server.is_loaded_with_dependencies(ui.id()))
    {
        assert!(Instant::now() < deadline, "map mask and UI artwork did not both load");
        app.update();
        std::thread::sleep(Duration::from_millis(10));
    }

    let images = app.world().resource::<Assets<Image>>();
    let sprite = images.get(&sprite).unwrap().data.as_ref().unwrap();
    let ui = images.get(&ui).unwrap().data.as_ref().unwrap();
    assert_eq!(sprite.len(), ui.len());
    assert!(sprite.chunks_exact(4).any(|pixel| pixel == [255, 255, 255, 0]));
    assert!(ui.chunks_exact(4).any(|pixel| pixel == [0, 0, 0, 0]));
    assert!(ui.chunks_exact(4).all(|pixel| pixel[3] != 0 || pixel[..3] == [0; 3]));
}
