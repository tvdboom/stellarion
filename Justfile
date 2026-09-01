set windows-shell := ["powershell.exe", "-NoLogo", "-NoProfile", "-Command"]

jobs := env_var_or_default("STELLARION_JOBS", "12")
asset_jobs := env_var_or_default("STELLARION_ASSET_JOBS", "12")
native_package_command := if os() == "windows" { "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-native.ps1" } else { "bash scripts/package-native.sh" }
web_package_command := if os() == "windows" { "powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-web.ps1" } else { "bash scripts/package-web.sh" }

# List the available project commands.
default:
    @just --list

# Run the native game in development mode. Pass game arguments after `--`.
run *args:
    cargo run --bin stellarion -j{{ jobs }} {{ args }}

# Run an optimized native build. Pass game arguments after `--`.
run-release *args:
    cargo run --release --bin stellarion -j{{ jobs }} {{ args }}

# Build the native game in development mode.
build:
    cargo build --bin stellarion -j{{ jobs }}

# Build the optimized native game.
build-release:
    cargo build --release --bin stellarion -j{{ jobs }}

# Check all native targets and features without producing binaries.
check:
    cargo check --all-targets --all-features -j{{ jobs }}

# Check the browser target.
check-wasm:
    cargo check --target wasm32-unknown-unknown --bin stellarion -j{{ jobs }}

# Format the Rust source.
fmt:
    cargo fmt --all

# Verify Rust formatting without changing files.
fmt-check:
    cargo fmt --all -- --check

# Run Clippy and reject warnings.
lint:
    cargo clippy --all-targets --all-features -j{{ jobs }} -- -D warnings

# Run all tests. Additional arguments are passed to Cargo.
test *args:
    cargo test --all-targets --all-features -j{{ jobs }} {{ args }}

# Generate changed runtime assets with KTX-Software.
assets:
    cargo run --features asset-pipeline --bin build-assets -j{{ jobs }} -- --jobs {{ asset_jobs }}

# Regenerate every runtime asset with KTX-Software.
assets-force:
    cargo run --features asset-pipeline --bin build-assets -j{{ jobs }} -- --force --jobs {{ asset_jobs }}

# Verify that committed runtime assets match their sources.
assets-check:
    cargo run --features asset-pipeline --bin build-assets -j{{ jobs }} -- --check --jobs {{ asset_jobs }}

# Run the same quality gates used by CI.
ci: fmt-check lint test assets-check check-wasm

# Package the native game for the current host.
package-native:
    {{ native_package_command }}

# Package the browser build.
package-web:
    {{ web_package_command }}
