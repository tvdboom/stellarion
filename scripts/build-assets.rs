//! Reproducible `assets` to `assets-runtime` KTX2 conversion pipeline.

use std::collections::{BTreeMap, VecDeque};
use std::env;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use sha2::{Digest, Sha256};

const PIPELINE_VERSION: &str = "stellarion-ktx2-v7-uastc-q2-rdo1.5-zstd18-sha256";
const MANIFEST_NAME: &str = ".stellarion-assets";
const MAX_JOBS: usize = 12;

// Files from the original repository that are not referenced by gameplay or
// packaging. They remain available as sources/screenshots but are omitted from
// runtime downloads.
const UNUSED_IMAGES: &[&str] = &[
    "images/bg/cover.png",
    "images/buildings/robotics.png",
    "images/buildings/senate.png",
    "images/buildings/small_shield.png",
    "images/buildings/terraformer.png",
    "images/defense/truck.png",
    "images/planets/planets.png",
    "images/resources/energy.png",
    "images/scenery/combat.png",
    "images/scenery/incombat.png",
    "images/scenery/map.png",
    "images/scenery/mission.png",
    "images/scenery/report.png",
    "images/scenery/shop.png",
    "images/ships/sattelite.png",
];

#[derive(Clone, Debug)]
/// One source PNG and its deterministic KTX2 destination/options.
struct Conversion {
    source: PathBuf,
    destination: PathBuf,
    source_relative: String,
    relative: String,
    mipmaps: bool,
}

#[derive(Clone, Debug)]
/// Expected source fingerprint and runtime destination for one manifest entry.
struct ExpectedAsset {
    source_relative: String,
    source_hash: String,
    destination: PathBuf,
}

#[derive(Clone, Debug)]
/// Persisted source/output hashes for one generated runtime asset.
struct AssetRecord {
    source_relative: String,
    source_hash: String,
    output_hash: String,
}

#[derive(Clone, Copy, Debug, Default)]
/// Validated command-line controls for asset checking and conversion.
struct Options {
    check: bool,
    force: bool,
    jobs: usize,
}

/// Parses options, builds runtime assets, and reports actionable failures.
fn main() {
    if let Err(error) = run() {
        eprintln!("asset build failed: {error}");
        std::process::exit(1);
    }
}

/// Executes one incremental or verification-only asset build.
fn run() -> Result<(), String> {
    let options = parse_options()?;
    let repository = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let source_root = repository.join("assets");
    let runtime_root = repository.join("assets-runtime");
    if !source_root.is_dir() {
        return Err(format!("source directory does not exist: {}", source_root.display()));
    }

    let mut source_files = Vec::new();
    collect_files(&source_root, &mut source_files)?;
    source_files.sort();

    let previous_manifest = read_manifest(&runtime_root.join(MANIFEST_NAME))?;
    let previous_is_current =
        previous_manifest.as_ref().is_some_and(|manifest| manifest.version == PIPELINE_VERSION);
    let mut expected = BTreeMap::new();
    let mut copies = Vec::new();
    let mut conversions = Vec::new();

    for source in source_files {
        let relative_path =
            source.strip_prefix(&source_root).map_err(|error| error.to_string())?.to_path_buf();
        let relative = normalized(&relative_path)?;
        if UNUSED_IMAGES.contains(&relative.as_str()) {
            continue;
        }
        let source_hash = file_hash(&source)?;

        match source.extension().and_then(OsStr::to_str).map(str::to_ascii_lowercase) {
            Some(extension) if extension == "png" && relative == "images/icons/planet.png" => {
                let destination = runtime_root.join(&relative_path);
                expected.insert(
                    relative.clone(),
                    ExpectedAsset {
                        source_relative: relative.clone(),
                        source_hash,
                        destination: destination.clone(),
                    },
                );
                copies.push((source, destination, relative.clone(), relative));
            },
            Some(extension) if extension == "png" => {
                let mut destination_relative = relative_path.clone();
                destination_relative.set_extension("basisu.ktx2");
                let destination_name = normalized(&destination_relative)?;
                let destination = runtime_root.join(&destination_relative);
                expected.insert(
                    destination_name.clone(),
                    ExpectedAsset {
                        source_relative: relative.clone(),
                        source_hash,
                        destination: destination.clone(),
                    },
                );
                conversions.push(Conversion {
                    source,
                    destination,
                    source_relative: relative.clone(),
                    relative: destination_name,
                    mipmaps: should_generate_mipmaps(&relative),
                });
            },
            Some(extension) if extension == "ogg" || extension == "ttf" => {
                let destination = runtime_root.join(&relative_path);
                expected.insert(
                    relative.clone(),
                    ExpectedAsset {
                        source_relative: relative.clone(),
                        source_hash,
                        destination: destination.clone(),
                    },
                );
                copies.push((source, destination, relative.clone(), relative));
            },
            Some(extension) => {
                return Err(format!("unsupported source asset extension .{extension}: {relative}"));
            },
            None => return Err(format!("source asset has no extension: {relative}")),
        }
    }

    let force = options.force || !previous_is_current;
    let mut stale_copies = Vec::new();
    for copy in copies {
        let (_, destination, source_relative, relative) = &copy;
        let source_hash = &expected[relative].source_hash;
        if force
            || !record_is_current(
                previous_manifest.as_ref(),
                relative,
                source_relative,
                source_hash,
                destination,
            )?
        {
            stale_copies.push(copy);
        }
    }
    let mut stale_conversions = Vec::new();
    for conversion in conversions {
        let source_hash = &expected[&conversion.relative].source_hash;
        if force
            || !record_is_current(
                previous_manifest.as_ref(),
                &conversion.relative,
                &conversion.source_relative,
                source_hash,
                &conversion.destination,
            )?
        {
            stale_conversions.push(conversion);
        }
    }

    let removed = previous_manifest
        .as_ref()
        .map(|manifest| {
            manifest
                .records
                .keys()
                .filter(|relative| !expected.contains_key(*relative))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    if options.check {
        let manifest_missing = !previous_is_current;
        if manifest_missing
            || !stale_copies.is_empty()
            || !stale_conversions.is_empty()
            || !removed.is_empty()
        {
            return Err(format!(
                "runtime assets are stale (manifest: {}, copies: {}, KTX2: {}, removed: {}); run `cargo run --features asset-pipeline --bin build-assets`",
                if manifest_missing { "missing/outdated" } else { "current" },
                stale_copies.len(),
                stale_conversions.len(),
                removed.len()
            ));
        }
        println!("verified {} SHA-256-pinned runtime assets", expected.len());
        return Ok(());
    }

    fs::create_dir_all(&runtime_root)
        .map_err(|error| format!("could not create {}: {error}", runtime_root.display()))?;
    for relative in removed {
        remove_previous_output(&runtime_root, &relative)?;
    }
    for (source, destination, _, relative) in stale_copies {
        copy_asset(&source, &destination)
            .map_err(|error| format!("could not copy {relative}: {error}"))?;
    }

    let toktx = env::var_os("TOKTX").unwrap_or_else(|| "toktx".into());
    let toktx_version = if stale_conversions.is_empty() {
        previous_manifest
            .as_ref()
            .map_or_else(|| "not-invoked".to_string(), |manifest| manifest.tool.clone())
    } else {
        tool_version(&toktx)?
    };
    convert_all(stale_conversions, &toktx, options.jobs)?;

    let expected_count = expected.len();
    let mut records = BTreeMap::new();
    for (relative, asset) in expected {
        records.insert(
            relative,
            AssetRecord {
                source_relative: asset.source_relative,
                source_hash: asset.source_hash,
                output_hash: file_hash(&asset.destination)?,
            },
        );
    }
    write_manifest(
        &runtime_root.join(MANIFEST_NAME),
        &AssetManifest {
            version: PIPELINE_VERSION.to_string(),
            tool: toktx_version,
            records,
        },
    )?;

    let source_bytes = directory_size(&source_root)?;
    let runtime_bytes = directory_size(&runtime_root)?;
    let percent = if source_bytes == 0 {
        0.0
    } else {
        100.0 * runtime_bytes as f64 / source_bytes as f64
    };
    println!(
        "built {} runtime assets: {:.1} MiB -> {:.1} MiB ({percent:.1}% of source size)",
        expected_count,
        source_bytes as f64 / 1_048_576.0,
        runtime_bytes as f64 / 1_048_576.0,
    );
    Ok(())
}

/// Parses `--check`, `--force`, and a bounded `--jobs N` argument.
fn parse_options() -> Result<Options, String> {
    let mut options = Options {
        jobs: env::var("STELLARION_ASSET_JOBS")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(MAX_JOBS)
            .clamp(1, MAX_JOBS),
        ..Options::default()
    };
    let mut arguments = env::args().skip(1);
    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--check" => options.check = true,
            "--force" => options.force = true,
            "--jobs" => {
                let value =
                    arguments.next().ok_or_else(|| "--jobs requires a value".to_string())?;
                let jobs =
                    value.parse::<usize>().map_err(|_| format!("invalid --jobs value: {value}"))?;
                if !(1..=MAX_JOBS).contains(&jobs) {
                    return Err(format!("--jobs must be in 1..={MAX_JOBS}"));
                }
                options.jobs = jobs;
            },
            "-h" | "--help" => {
                println!(
                    "Usage: build-assets [--check] [--force] [--jobs 1..={MAX_JOBS}]\n\
                     Converts source PNGs to UASTC/Zstd KTX2 for the pure-Rust runtime loader and copies fonts/audio incrementally."
                );
                std::process::exit(0);
            },
            _ => return Err(format!("unknown asset-build option: {argument}")),
        }
    }
    Ok(options)
}

/// Recursively collects files without following directory symlinks.
fn collect_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(directory)
        .map_err(|error| format!("could not read {}: {error}", directory.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| error.to_string())?;
        let file_type = entry.file_type().map_err(|error| error.to_string())?;
        if file_type.is_symlink() {
            return Err(format!(
                "source asset symlinks are not supported: {}",
                entry.path().display()
            ));
        }
        if file_type.is_dir() {
            collect_files(&entry.path(), output)?;
        } else if file_type.is_file() {
            output.push(entry.path());
        }
    }
    Ok(())
}

/// Converts a relative path to a stable, traversal-free manifest spelling.
fn normalized(path: &Path) -> Result<String, String> {
    if path.components().any(|component| !matches!(component, Component::Normal(_))) {
        return Err(format!("asset path is not a simple relative path: {}", path.display()));
    }
    Ok(path
        .components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/"))
}

/// Chooses mipmaps for artwork that is independently sampled or continuously scaled.
fn should_generate_mipmaps(source_relative: &str) -> bool {
    source_relative.starts_with("images/bg/")
        || source_relative.ends_with(" large.png")
        || source_relative.starts_with("images/animations/solar star ")
        || source_relative.starts_with("images/ambient/")
}

/// Returns whether source and output bytes match their manifest fingerprints.
fn record_is_current(
    manifest: Option<&AssetManifest>,
    relative: &str,
    source_relative: &str,
    source_hash: &str,
    destination: &Path,
) -> Result<bool, String> {
    let Some(record) = manifest.and_then(|manifest| manifest.records.get(relative)) else {
        return Ok(false);
    };
    if record.source_relative != source_relative || record.source_hash != source_hash {
        return Ok(false);
    }
    match file_hash(destination) {
        Ok(output_hash) => Ok(output_hash == record.output_hash),
        Err(_) if !destination.exists() => Ok(false),
        Err(error) => Err(error),
    }
}

/// Copies one passthrough asset after creating its runtime parent directory.
fn copy_asset(source: &Path, destination: &Path) -> std::io::Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, destination).map(|_| ())
}

/// Reads and validates the previous generated-file manifest when present.
fn read_manifest(path: &Path) -> Result<Option<AssetManifest>, String> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("could not read {}: {error}", path.display())),
    };
    let mut version = None;
    let mut tool = None;
    let mut records = BTreeMap::new();
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("version=") {
            version = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("tool=") {
            tool = Some(value.to_string());
        } else if let Some(value) = line.strip_prefix("asset=") {
            let mut fields = value.split('\t');
            let relative = fields.next().unwrap_or_default();
            let source_relative = fields.next().unwrap_or_default();
            let source_hash = fields.next().unwrap_or_default();
            let output_hash = fields.next().unwrap_or_default();
            if fields.next().is_some()
                || normalized(Path::new(relative))? != relative
                || normalized(Path::new(source_relative))? != source_relative
                || !valid_hash(source_hash)
                || !valid_hash(output_hash)
                || records
                    .insert(
                        relative.to_string(),
                        AssetRecord {
                            source_relative: source_relative.to_string(),
                            source_hash: source_hash.to_string(),
                            output_hash: output_hash.to_string(),
                        },
                    )
                    .is_some()
            {
                return Err(format!("invalid asset manifest record: {line}"));
            }
        } else if let Some(value) = line.strip_prefix("file=") {
            // Version 6 recorded names only. Parsing it lets the normal stale-output path
            // regenerate and upgrade the manifest without treating the file as corrupt.
            normalized(Path::new(value))?;
            records.entry(value.to_string()).or_insert_with(|| AssetRecord {
                source_relative: String::new(),
                source_hash: String::new(),
                output_hash: String::new(),
            });
        } else if !line.trim().is_empty() {
            return Err(format!("invalid asset manifest line: {line}"));
        }
    }
    Ok(Some(AssetManifest {
        version: version.ok_or_else(|| "asset manifest has no version".to_string())?,
        tool: tool.unwrap_or_else(|| "unknown".to_string()),
        records,
    }))
}

#[derive(Debug)]
/// Versioned, hash-pinned description of every generated runtime asset.
struct AssetManifest {
    version: String,
    tool: String,
    records: BTreeMap<String, AssetRecord>,
}

/// Writes a deterministic manifest after every output has succeeded.
fn write_manifest(path: &Path, manifest: &AssetManifest) -> Result<(), String> {
    let mut text = format!("version={}\ntool={}\n", manifest.version, manifest.tool.trim());
    for (relative, record) in &manifest.records {
        text.push_str("asset=");
        text.push_str(relative);
        text.push('\t');
        text.push_str(&record.source_relative);
        text.push('\t');
        text.push_str(&record.source_hash);
        text.push('\t');
        text.push_str(&record.output_hash);
        text.push('\n');
    }
    fs::write(path, text).map_err(|error| format!("could not write {}: {error}", path.display()))
}

/// Hashes one regular file with SHA-256 for checkout-independent freshness checks.
fn file_hash(path: &Path) -> Result<String, String> {
    let mut file = File::open(path)
        .map_err(|error| format!("could not open {} for hashing: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("could not hash {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

/// Returns whether text is one canonical lowercase SHA-256 digest.
fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value.bytes().all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

/// Deletes only an exact path previously recorded by this pipeline.
fn remove_previous_output(runtime_root: &Path, relative: &str) -> Result<(), String> {
    let relative_path = Path::new(relative);
    normalized(relative_path)?;
    let target = runtime_root.join(relative_path);
    if target.is_file() {
        fs::remove_file(&target)
            .map_err(|error| format!("could not remove stale {}: {error}", target.display()))?;
    }
    Ok(())
}

/// Reads the encoder version so build output records the external tool used.
fn tool_version(toktx: &OsStr) -> Result<String, String> {
    let output = Command::new(toktx)
        .arg("--version")
        .output()
        .map_err(|error| format!("could not launch {:?}: {error}", toktx))?;
    if !output.status.success() {
        return Err(format!(
            "{:?} --version failed: {}",
            toktx,
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let version = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).trim().to_string()
    } else {
        String::from_utf8_lossy(&output.stdout).trim().to_string()
    };
    if !version.starts_with("toktx v4.") {
        return Err(format!("toktx 4.x is required, found `{version}`"));
    }
    Ok(version)
}

/// Converts all stale PNGs with at most twelve single-threaded encoder workers.
fn convert_all(conversions: Vec<Conversion>, toktx: &OsStr, jobs: usize) -> Result<(), String> {
    if conversions.is_empty() {
        return Ok(());
    }
    let total = conversions.len();
    let queue = Arc::new(Mutex::new(VecDeque::from(conversions)));
    let errors = Arc::new(Mutex::new(Vec::<String>::new()));
    let completed = Arc::new(AtomicUsize::new(0));

    thread::scope(|scope| {
        for _ in 0..jobs.min(total) {
            let queue = Arc::clone(&queue);
            let errors = Arc::clone(&errors);
            let completed = Arc::clone(&completed);
            scope.spawn(move || loop {
                let conversion = match queue.lock() {
                    Ok(mut queue) => queue.pop_front(),
                    Err(_) => {
                        if let Ok(mut errors) = errors.lock() {
                            errors.push("asset work queue lock was poisoned".to_string());
                        }
                        return;
                    },
                };
                let Some(conversion) = conversion else {
                    return;
                };
                if let Err(error) = convert_one(&conversion, toktx) {
                    if let Ok(mut errors) = errors.lock() {
                        errors.push(error);
                    }
                }
                let count = completed.fetch_add(1, Ordering::Relaxed) + 1;
                if count == total || count.is_multiple_of(20) {
                    println!("converted {count}/{total} KTX2 textures");
                }
            });
        }
    });

    let errors = errors.lock().map_err(|_| "asset error list lock was poisoned".to_string())?;
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("\n"))
    }
}

/// Encodes one PNG to an atomically replaced sRGB UASTC/Zstd KTX2 texture.
fn convert_one(conversion: &Conversion, toktx: &OsStr) -> Result<(), String> {
    let parent = conversion
        .destination
        .parent()
        .ok_or_else(|| format!("runtime asset has no parent: {}", conversion.relative))?;
    fs::create_dir_all(parent)
        .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
    let file_name = conversion
        .destination
        .file_stem()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("invalid runtime filename: {}", conversion.relative))?;
    let temporary = parent.join(format!("{file_name}.tmp.ktx2"));
    if temporary.exists() {
        fs::remove_file(&temporary)
            .map_err(|error| format!("could not clear {}: {error}", temporary.display()))?;
    }

    let mut command = Command::new(toktx);
    // KTX-Software 4.x expects mipmap controls before the encoder controls.
    if conversion.mipmaps {
        // Clamp is the documented toktx default; its Windows 4.3 build rejects
        // an explicit `--wmode clamp` despite advertising the flag in help.
        command.args(["--genmipmap", "--filter", "lanczos4"]);
    }
    command.args([
        "--t2",
        "--encode",
        "uastc",
        "--uastc_quality",
        "2",
        "--uastc_rdo_l",
        "1.5",
        "--zcmp",
        "18",
        "--threads",
        "1",
        "--assign_oetf",
        "srgb",
        "--assign_primaries",
        "srgb",
    ]);
    let output = command
        .arg("--")
        .arg(&temporary)
        .arg(&conversion.source)
        .output()
        .map_err(|error| format!("could not encode {}: {error}", conversion.relative))?;
    if !output.status.success() {
        let _ = fs::remove_file(&temporary);
        let stderr = String::from_utf8_lossy(&output.stderr);
        let detail = stderr.lines().take(12).collect::<Vec<_>>().join("\n");
        return Err(format!("toktx failed for {}: {detail}", conversion.relative));
    }

    if conversion.destination.exists() {
        fs::remove_file(&conversion.destination).map_err(|error| {
            format!("could not replace {}: {error}", conversion.destination.display())
        })?;
    }
    fs::rename(&temporary, &conversion.destination).map_err(|error| {
        format!(
            "could not install {} as {}: {error}",
            temporary.display(),
            conversion.destination.display()
        )
    })
}

/// Sums regular-file sizes for a concise source/runtime download audit.
fn directory_size(directory: &Path) -> Result<u64, String> {
    let mut files = Vec::new();
    collect_files(directory, &mut files)?;
    files.into_iter().try_fold(0_u64, |total, file| {
        fs::metadata(&file)
            .map(|metadata| total.saturating_add(metadata.len()))
            .map_err(|error| format!("could not stat {}: {error}", file.display()))
    })
}
