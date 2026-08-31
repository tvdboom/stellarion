//! Pure-Rust Basis Universal KTX2 loading for native and browser builds.

use basisu::{DecodeFlags, TargetFormat, Transcoder};
use bevy::asset::{AssetLoader, RenderAssetUsages};
use bevy::image::ImageSampler;
use bevy::prelude::*;
use bevy::render::render_resource::{
    AstcBlock, AstcChannel, Extent3d, TextureDataOrder, TextureDescriptor, TextureDimension,
    TextureFormat, TextureUsages, WgpuFeatures,
};
use bevy::render::{renderer::RenderDevice, RenderApp};
use thiserror::Error;

/// Registers the `.basisu.ktx2` loader after detecting the active GPU formats.
pub struct BasisTexturePlugin;

impl Plugin for BasisTexturePlugin {
    /// Reserves the compound extension before any deferred handles are requested.
    fn build(&self, app: &mut App) {
        app.preregister_asset_loader::<BasisTextureLoader>(&["basisu.ktx2"]);
    }

    /// Selects ASTC, BC7, ETC2, or an RGBA fallback from the actual render device.
    fn finish(&self, app: &mut App) {
        let features = app.sub_app_mut(RenderApp).world().resource::<RenderDevice>().features();
        app.register_asset_loader(BasisTextureLoader::from_features(features));
    }
}

#[derive(Clone, Copy)]
/// GPU texture format and block geometry selected for Basis transcoding.
struct TranscodeTarget {
    basis: TargetFormat,
    texture: TextureFormat,
}

impl TranscodeTarget {
    /// Chooses the first high-quality compressed format supported by the device.
    fn from_features(features: WgpuFeatures) -> Self {
        if features.contains(WgpuFeatures::TEXTURE_COMPRESSION_ASTC) {
            Self {
                basis: TargetFormat::Astc4x4Rgba,
                texture: TextureFormat::Astc {
                    block: AstcBlock::B4x4,
                    channel: AstcChannel::UnormSrgb,
                },
            }
        } else if features.contains(WgpuFeatures::TEXTURE_COMPRESSION_BC) {
            Self {
                basis: TargetFormat::Bc7Rgba,
                texture: TextureFormat::Bc7RgbaUnormSrgb,
            }
        } else if features.contains(WgpuFeatures::TEXTURE_COMPRESSION_ETC2) {
            Self {
                basis: TargetFormat::Etc2Rgba,
                texture: TextureFormat::Etc2Rgba8UnormSrgb,
            }
        } else {
            Self {
                basis: TargetFormat::Rgba32,
                texture: TextureFormat::Rgba8UnormSrgb,
            }
        }
    }

    /// Falls back to RGBA8 when the base extent violates this format's block geometry.
    fn for_dimensions(self, width: u32, height: u32) -> Self {
        let (block_width, block_height) = self.texture.block_dimensions();
        if width.is_multiple_of(block_width) && height.is_multiple_of(block_height) {
            self
        } else {
            Self {
                basis: TargetFormat::Rgba32,
                texture: TextureFormat::Rgba8UnormSrgb,
            }
        }
    }
}

#[derive(TypePath)]
/// Pure-Rust Bevy asset loader for compound .basisu.ktx2 files.
struct BasisTextureLoader {
    target: TranscodeTarget,
}

impl BasisTextureLoader {
    /// Creates a loader whose output matches the current GPU's advertised features.
    fn from_features(features: WgpuFeatures) -> Self {
        Self {
            target: TranscodeTarget::from_features(features),
        }
    }
}

/// Failure to read or transcode one generated Basis KTX2 asset.
#[derive(Debug, Error)]
pub enum BasisTextureError {
    /// Asset bytes could not be read from the configured Bevy asset source.
    #[error("failed to read Basis KTX2 bytes: {0}")]
    Io(#[from] std::io::Error),
    /// The container, mip chain, or requested target was invalid.
    #[error("failed to transcode Basis KTX2: {0}")]
    Transcode(String),
    /// Stellarion's pipeline produced an unsupported layered or cubemap texture.
    #[error("only plain two-dimensional Basis KTX2 textures are supported")]
    UnsupportedLayout,
}

impl AssetLoader for BasisTextureLoader {
    /// Asset type produced by this loader.
    type Asset = Image;
    /// Per-load settings type; Basis textures require no custom settings.
    type Settings = ();
    /// Typed loader error returned for invalid containers or transcoding failures.
    type Error = BasisTextureError;

    /// Reads and transcodes every mip into one GPU-ready Bevy image.
    async fn load(
        &self,
        reader: &mut dyn bevy::asset::io::Reader,
        _settings: &Self::Settings,
        _load_context: &mut bevy::asset::LoadContext<'_>,
    ) -> Result<Self::Asset, Self::Error> {
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).await?;
        transcode_basis_texture(&bytes, self.target)
    }

    /// Uses a compound extension so Bevy's built-in KTX2 loader never claims these files.
    fn extensions(&self) -> &[&str] {
        &["basisu.ktx2"]
    }
}

/// Transcodes a pipeline-owned 2D UASTC texture without C/C++ or platform APIs.
fn transcode_basis_texture(
    bytes: &[u8],
    target: TranscodeTarget,
) -> Result<Image, BasisTextureError> {
    let transcoder = Transcoder::new(bytes)
        .map_err(|error| BasisTextureError::Transcode(format!("{error:?}")))?;
    if transcoder.layer_count() > 1 || transcoder.face_count() > 1 {
        return Err(BasisTextureError::UnsupportedLayout);
    }

    let level_count = transcoder.level_count();
    let (width, height) = transcoder.base_dimensions();
    // wgpu requires compressed texture descriptors to use whole block extents. Source UI
    // sprites intentionally have arbitrary pixel dimensions, so keeping their logical extent
    // requires an uncompressed upload rather than padding and subtly changing layout sizes.
    let target = target.for_dimensions(width, height);
    let mut data = Vec::new();
    for level in 0..level_count {
        let mut level_data =
            transcoder
                .transcode(level, target.basis, DecodeFlags::HIGH_QUALITY)
                .map_err(|error| BasisTextureError::Transcode(format!("mip {level}: {error:?}")))?;
        data.append(&mut level_data);
    }

    Ok(Image {
        data: Some(data),
        data_order: TextureDataOrder::MipMajor,
        texture_descriptor: TextureDescriptor {
            label: None,
            size: Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: level_count,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: target.texture,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        },
        sampler: ImageSampler::Default,
        texture_view_descriptor: None,
        asset_usage: RenderAssetUsages::RENDER_WORLD,
        copy_on_resize: false,
    })
}

#[cfg(test)]
mod tests {
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
        let image = transcode_basis_texture(&runtime_asset("images/ui/button.basisu.ktx2"), target)
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
        let image = transcode_basis_texture(&runtime_asset("images/ui/button.basisu.ktx2"), target)
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
        let image = transcode_basis_texture(&runtime_asset("images/bg/menu.basisu.ktx2"), target)
            .expect("mipmapped UASTC background should transcode");
        assert_eq!(image.texture_descriptor.format, TextureFormat::Bc7RgbaUnormSrgb);
        assert!(image.texture_descriptor.mip_level_count > 1);
    }
}
