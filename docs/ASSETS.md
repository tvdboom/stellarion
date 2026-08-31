# Asset pipeline and deferred loading

Stellarion follows the useful split established in the sibling Arcana repository: editable source art is separate from generated runtime content, conversion is scripted, texture loading understands KTX2, and asset groups follow application states instead of one eager startup load.

## Layout

- `assets/` contains original PNG, OGG, and TTF inputs.
- `assets-runtime/` is generated runtime content and is the only asset tree included in packages.
- `src/bin/build_assets.rs` is the reproducible converter and manifest verifier.
- `assets-runtime/.stellarion-assets` records the pipeline/tool version plus SHA-256 of every used source and generated output.

Fifteen screenshot, combined-sheet, obsolete-unit, or otherwise unreferenced PNGs remain in `assets` but are omitted from runtime downloads. Audio and fonts are copied byte-for-byte. The Windows icon remains PNG because the native icon decoder needs it before Bevy asset loading.

## KTX2 encoding

The pipeline requires Khronos KTX-Software `toktx` 4.x and converts 196 suitable PNGs with:

- KTX2 container;
- UASTC quality 2 Basis payload;
- UASTC RDO level 1.5;
- Zstandard level 18 supercompression;
- sRGB transfer and primaries metadata;
- one encoder thread per job, with at most twelve jobs;
- Lanczos4 mipmaps only for independently sampled backgrounds and large planet previews.

UI sprites and atlas sheets keep a single level to avoid sampling bleed and needless download bytes. The current generated runtime is about 52.2 MiB versus 122.8 MiB of sources (42.5%).

At runtime `BasisTexturePlugin` registers the compound `.basisu.ktx2` extension before handles are created. Its pure-Rust `basisu` transcoder selects ASTC, BC7, ETC2, or RGBA8 based on the active render device, preserves every mip, marks textures sRGB, and drops the CPU copy after upload. Arbitrary-sized UI textures fall back to RGBA8 when their dimensions are not whole GPU compression blocks, preserving exact layout dimensions without invalid wgpu descriptors. This avoids the native C++ Basis Universal toolchain that breaks portable `wasm32-unknown-unknown` builds.

## Commands

Generate incrementally:

```text
cargo run --features asset-pipeline --bin build-assets -j12 -- --jobs 12
```

Force a complete re-encode:

```text
cargo run --features asset-pipeline --bin build-assets -j12 -- --force --jobs 12
```

Verify committed outputs without invoking `toktx`:

```text
cargo run --features asset-pipeline --bin build-assets -j12 -- --check
```

`--check` hashes sources and outputs. It is independent of checkout timestamps and fails for missing, modified, stale, unexpected, or old-pipeline outputs. The converter refuses symlinks, traversal-like manifest paths, unsupported extensions, and worker counts outside `1..=12`. It writes each texture through a temporary file and removes only exact paths listed by the prior manifest.

Set `TOKTX` to an explicit executable path if `toktx` is not on `PATH`. `STELLARION_ASSET_JOBS` may lower the default worker count; command-line `--jobs` takes precedence.

## Deferred loading

`WorldAssets` has two handle groups:

- menu: menu background, button textures, immediate fonts/icons, and basic UI sounds;
- gameplay: map/world art, planets/moons, units, mission/combat/effect textures, music, and gameplay audio.

The gameplay group begins in `Deferred`. Entering `AppState::LoadingGame` changes it to `Loading` and requests each deduplicated handle once. The loading screen remains interactive until `AssetServer` reports all recursive dependencies ready, then canonical game state is projected and the state becomes `Ready`.

Tests assert that menu construction leaves gameplay deferred and that runtime category paths use KTX2. Loader tests transcode real committed UASTC files to RGBA and BC7, including mip chains. Khronos `ktx validate` can additionally validate containers after a forced conversion.

Generated assets are committed so normal builds and pull requests do not need KTX-Software. Re-run the converter whenever a used source changes, and commit both the runtime file and manifest update.
