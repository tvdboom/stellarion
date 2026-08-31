# Build, package, and release

Stellarion uses one Rust application for native and WebAssembly targets. Runtime packages contain the executable/bindings, generated `assets-runtime/`, license, readme, and optional public Supabase JSON.

## Prerequisites

- current stable Rust and Cargo;
- `wasm32-unknown-unknown` for browser builds;
- `wasm-bindgen-cli` at the exact version in `scripts/wasm-bindgen-version.txt`;
- `zip` on Linux/macOS packaging hosts;
- KTX-Software 4.x only when regenerating changed assets;
- Linux native development libraries for ALSA, udev, X11, Xi, Xcursor, Xrandr, and xkbcommon.

The repository intentionally bounds Cargo and conversion parallelism to twelve workers.

## Quality gates

```text
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -j12 -- -D warnings
cargo test --all-targets --all-features -j12
cargo run --features asset-pipeline --bin build-assets -j12 -- --check --jobs 12
cargo check --target wasm32-unknown-unknown --bin stellarion -j12
```

Run entirely without Supabase by omitting configuration; the application selects `InMemoryBackend`, and all automated backend tests use it directly.

## Browser / itch.io HTML5

Install the matching binding generator once:

```text
cargo install wasm-bindgen-cli --version 0.2.127 --locked -j12
```

PowerShell:

```text
./scripts/package-web.ps1
```

Bash:

```text
bash scripts/package-web.sh
```

Both build the `wasm-release` profile, run `wasm-bindgen --target web --no-typescript`, stage `index.html`, bindings, WASM, and assets, and create `dist/stellarion-html.zip`. Set the two Supabase environment variables or pass `-ConfigPath`/`CONFIG_PATH` to include deployment configuration. Packaging rejects partial configuration, modern `sb_secret_*` keys, and legacy JWTs carrying the `service_role` claim before staging any files.

For a manual itch.io upload, create an HTML project, upload the ZIP, mark it as playable in the browser, and use the default responsive viewport. The build uses WebGL2 and does not require raw sockets, port forwarding, a custom server, TypeScript, or shared-memory response headers.

## Native packages

On Windows PowerShell:

```text
./scripts/package-native.ps1 -Target x86_64-pc-windows-msvc -Channel windows
```

On Linux:

```text
TARGET=x86_64-unknown-linux-gnu CHANNEL=linux bash scripts/package-native.sh
```

On macOS for one architecture:

```text
TARGET=aarch64-apple-darwin CHANNEL=mac bash scripts/package-native.sh
```

The scripts generate `dist/stellarion-windows.zip`, `stellarion-linux.zip`, or `stellarion-mac.zip`. They accept an already-built binary through `-BinaryPath`/`BINARY_PATH` and `-SkipBuild`/`SKIP_BUILD=1`.

Release CI builds a universal macOS executable by compiling both `aarch64-apple-darwin` and `x86_64-apple-darwin`, then combining them with `lipo`. Packages are unsigned; Apple code signing/notarization would require a Developer ID and additional CI secrets not currently requested.

## GitHub Actions

`.github/workflows/build-release.yml` runs on pull requests, `master`, version tags, and manual dispatch:

- formatting, Clippy with warnings denied, all-target tests, and hash-based asset verification;
- WebAssembly build and itch.io-ready HTML ZIP;
- Windows x86_64 and Linux x86_64 release ZIPs;
- macOS universal release ZIP;
- GitHub Release creation for `v*` tags;
- optional Butler deployment from `master`.

Pull requests require no external credentials. If Supabase repository variables are absent, packages contain the example configuration and run in mock mode until configured.

Repository variables:

- `SUPABASE_URL`
- `SUPABASE_PUBLISHABLE_KEY`
- `ITCH_USER`
- `ITCH_GAME`

Repository secret:

- `BUTLER_API_KEY`

The publishable Supabase values are public configuration and belong in GitHub Variables, not server-secret storage. `BUTLER_API_KEY` is a deployment credential and must remain a secret.

When all three itch settings are present, CI downloads Butler from itch.io and pushes these channels:

```text
user/game:html
user/game:windows
user/game:linux
user/game:mac
```

Butler accepts ZIP input directly, so the CI artifact is also the uploaded build. See the official [Butler pushing guide](https://itch.io/docs/butler/pushing.html).

## Local serving

Do not open `index.html` as a `file://` URL. Serve the staged directory over HTTP, for example with any static server available on the machine, then open its localhost URL. The Supabase project must be reachable over HTTPS/WSS for non-local deployments.

## Verified local outputs

On 2026-08-31, the final Windows toolchain produced and inspected:

- `dist/stellarion-windows.zip`: 75.87 MiB, containing the stripped 69.81 MiB executable, 215 verified runtime assets plus their manifest, documentation, license, and the supplied public configuration; SHA-256 `7F8905C8B3315D978FF3351A3E4361A6D44AE1A39AF103819353CAC80DE23351`;
- `dist/stellarion-html.zip`: 59.53 MiB, containing `index.html`, generated JavaScript, a 26.58 MiB optimized WASM module, the same assets, and the supplied public configuration; SHA-256 `74C0FAFE4EC2D33E9A71FB4FF4EDD477CC4E25B5CC5A8F8ACDBEB8EE97D8DA68`.

The optimized WASM build, bindings, and ZIP completed locally, as did the Windows x86_64 release. Linux x86_64 and universal macOS are built by the native GitHub Actions runners; they were not cross-claimed as locally executed from Windows.

## Release checklist

1. Apply/test the fresh `supabase/schema.sql` state and enable anonymous Auth.
2. Regenerate and verify assets if sources changed.
3. Run all quality gates and native/WASM builds.
4. Configure public Supabase repository variables.
5. Push a `v*` tag to create downloadable GitHub artifacts.
6. Configure Butler variables/secret only when automatic itch.io deployment is desired.
7. Smoke-test create/join/recover/submit/reconnect with separate identities on the published build.
