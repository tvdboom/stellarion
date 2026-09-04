# Stellarion contributor instructions

These instructions apply throughout this repository. Keep them aligned with the
actual code, `Cargo.toml`, and `Justfile` when the project changes.

## Database workflow: one complete reset script

- The user explicitly wants **no SQL migrations**. Do not create a migrations
  directory, numbered SQL updates, upgrade scripts, or downgrade scripts.
- `supabase/schema.sql` is the sole source of truth for the database. Change it
  directly whenever tables, constraints, indexes, functions, permissions,
  Realtime configuration, or scheduled jobs change.
- `supabase/` must contain only `schema.sql`. All application backend setup must
  be executable from that file in the Supabase SQL Editor. Do not introduce
  Edge Functions, separate server deployments, or compatibility for old games.
- The script must reset the application database and recreate the complete
  current state in one transaction. It must work on both a fresh Supabase
  project and a previously initialized project, without an earlier SQL script.
- Remove obsolete definitions instead of retaining historical schema versions
  or adding data backfills. Losing existing application data on a reset is
  intentional in this project.
- Reset the entire application-facing `public` schema and recreate Stellarion
  jobs. Keep Supabase-managed infrastructure (`auth`, `storage`, extension
  schemas, and the scheduler itself) operational.
- Preserve the current retention rule: finished games are permanently deleted
  after 48 hours, including their memberships, recovery hashes, submissions,
  and events. The database cleanup job runs every minute. Lobby and active
  games also expire 30 days after their last snapshot save, as do finished games
  if that deadline arrives first. Presence and reconnects do not extend retention.
- Explain database setup and behavior in comments in `supabase/schema.sql`.
  Do not add a README inside `supabase/`. Report local SQL verification
  separately from actually applying a hosted reset.

## Project map

Stellarion is a Rust 2021, Bevy, and egui space strategy game sharing one codebase
between desktop and WebAssembly. Multiplayer uses Supabase and a mock backend.

| Path | Responsibility |
| --- | --- |
| `src/main.rs` | Native/browser entry point, plugins, window, canvas, logging |
| `src/lib.rs` | Shared library exports; public APIs are documented |
| `src/core/simulation.rs`, `src/core/random.rs` | Persisted game model, deterministic turn resolution, seeded random streams |
| `src/core/map/`, `src/core/combat/`, `src/core/units/` | Map and unit models, combat rules, reports, presentation adapters |
| `src/core/menu/`, `src/core/ui/` | egui menus, lobby, game HUD, input and layout |
| `src/core/mod.rs`, `app.rs`, `states.rs`, `turns.rs`, `audio.rs` | Module boundaries, Bevy system registration, application states, turn presentation, sound |
| `src/core/missions.rs`, `src/core/mission_systems.rs` | Persisted missions and shared rules, optional Bevy mission presentation and commands |
| `src/multiplayer/` | Backend contract, transport models, Supabase RPCs, mock storage, recovery, client coordination, Realtime |
| `src/platform/` | Public configuration and native/browser client-local storage |
| `scripts/build-assets.rs` | Incremental asset generation and verification |
| `assets/` | Source images, fonts, and audio |
| `assets-runtime/` | Generated runtime assets; loaded by the game and included in packages |
| `supabase/schema.sql` | Complete database reset and current schema |
| `src/multiplayer/authority.rs` | Shared deterministic snapshot and command validation |
| `tests/core/`, `tests/multiplayer/`, `tests/platform/` | Rust tests grouped by subsystem, outside production source |
| `tests/sql/fixtures.rs`, `tests/sql/verify-schema.mjs` | Current Rust test snapshots and disposable SQL verification |
| `web/index.html`, `scripts/` | Browser shell and native/web packaging scripts |
| `docs/`, `README.md` | Supporting documentation, attribution, gameplay and controls |
| `.github/workflows/` | Build, quality checks, packaging, release/deployment workflows |

`target/`, `dist/`, and `assets-runtime/` are ignored build outputs. Edit asset
sources in `assets/` and regenerate outputs rather than editing generated files.

Keep all tests and test-only helpers under the top-level `tests/` directory.
Rust source modules use `#[cfg(test)]` and `#[path = "..."]` declarations to load
their external test files, preserving private access and `cargo test --lib`.
Keep test filenames descriptive and group them by subsystem without creating
single-file directories. Likewise, use sibling source files when a module
directory would contain only one file.

## Working locally

Run commands from the repository root. Use the stable Rust toolchain with
`rustfmt` and `clippy`; add `wasm32-unknown-unknown` for browser builds. The
`Justfile` is the command reference; `just --list` lists available recipes.

| Task | Command |
| --- | --- |
| Run desktop development build | `just run` |
| Check native targets/features | `just check` |
| Check browser compilation | `just check-wasm` |
| Verify disposable SQL reset and RPC contract | `just verify-sql` |
| Format Rust | `just fmt` |
| Check formatting | `just fmt-check` |
| Clippy with warnings rejected | `just lint` |
| All tests/targets/features | `just test` |
| Generate changed runtime assets | `just assets` |
| Verify generated assets | `just assets-check` |
| Full local quality gate | `just ci` |
| Package desktop/browser builds | `just package-native` / `just package-web` |

Without `just`, use the corresponding Cargo command from the `Justfile`.
Useful focused commands:

```text
cargo test --lib multiplayer:: -j12
cargo test --lib --no-default-features -j12
cargo check --target wasm32-unknown-unknown --bin stellarion -j12
```

The default `app` feature includes the Bevy application. Library tests without
default features exercise the shared rules and backends without the app layer.
`asset-pipeline` enables the asset-builder binary. Build concurrency defaults
to 12; `STELLARION_JOBS` and `STELLARION_ASSET_JOBS` override Just recipes.

The native client normally uses the built-in public Supabase configuration in
`src/platform/config.rs`. For isolated local multiplayer, set
`STELLARION_BACKEND=mock` before launching. PowerShell example:

```powershell
$env:STELLARION_BACKEND = 'mock'
just run
```

Mock games last only for that process. Browser mock selection uses the same
variable at compile time. Local Practice is available in debug builds.

Generate assets with `just assets` before running if runtime assets are missing
or stale. Texture conversion requires KTX-Software. Web packaging also requires
the exact `wasm-bindgen-cli` version in `scripts/wasm-bindgen-version.txt`.
Use the existing `.ps1` / `.sh` packaging scripts; keep their behavior aligned.
SQL verification uses Node.js 24, PGlite, and the Rust `sql-fixtures` binary
(enabled by `sql-verification`). `just verify-sql` installs pinned test tooling
under ignored `target/sql-verification/`; no root npm manifest or lockfile is
needed. It executes PostgreSQL application SQL with
scheduler registration stubbed, without contacting a hosted project. Client
writes call authenticated SQL RPCs directly. Shared Rust code simulates turns;
SQL enforces membership, lifecycle, revisions, and submission completeness.
The database does not independently run Rust or provide anti-cheat validation
against modified clients. No service-role key belongs in the client.

## Rust and Bevy practices

- Follow the existing module boundaries and `rustfmt.toml`. Prefer small,
  cohesive functions and explicit types over new generic frameworks.
- Use enums and the existing game/player identity types to express valid
  states. Prefer exhaustive matches to silently ignoring new variants.
- Borrow data where practical; clone when ownership or an asynchronous task
  needs it. Keep allocations and repeated asset/text work out of per-frame
  loops when they are avoidable.
- Use `Result`, `Option`, and existing typed errors for expected failures.
  Propagate errors with context. Avoid `unwrap`, `expect`, or panics on network
  responses, saved data, and user input; assertions are appropriate in tests.
- Validate external data at boundaries. Use checked or saturating arithmetic
  where game counts, resources, revisions, or identifiers could overflow;
  choose the behavior according to the game rule.
- Keep mutable borrows and mutex guards short. Never hold a synchronous lock
  across an `.await`. Do not block the Bevy frame loop with network or disk work.
- Use the existing backend task infrastructure and platform-specific futures.
  Guard native-only APIs with `cfg`; preserve browser compilation and behavior.
- Put systems in the correct state/schedule. Make ordering explicit when one
  system consumes another's changes or messages. Use existing Bevy resources,
  components, and messages rather than parallel global state.
- Document public items and non-obvious invariants. Explain the reason for
  unusual behavior rather than restating the implementation.
- Keep dependencies and feature flags lean. Update `Cargo.lock` when dependency
  resolution changes, and check both native and WASM support.

## Gameplay and multiplayer invariants

- Make authoritative gameplay changes in the deterministic model. Bevy/egui
  systems project state and collect commands; camera position, selection,
  animation, sound, and UI timing must not change a turn's outcome.
- Use persisted random streams for simulation. Do not introduce wall-clock
  time, frame timing, process randomness, or unordered map iteration into
  authoritative resolution. Sort collections when their iteration affects it.
- Keep Rust transport types, validation, Supabase RPC JSON, and the in-memory
  backend consistent when changing the persistence contract.
- Preserve revision checks, idempotent submissions, authenticated membership
  checks, recovery-secret rotation, and durable event replay. Realtime messages
  are wake-up hints; reload authoritative state through the backend.
- Lobbies are temporary coordination records, never resumable games. When the
  host leaves an unstarted lobby, delete it and all related data, and return its
  guests to the menu. Only active and finished games belong in Resume Game or
  client recent-game history; leaving a started match must not delete it.
- Keep secrets out of client code, logs, and source control. Only public
  publishable configuration belongs in the browser build. Store recovery
  hashes on the server and do not expose them through client reads.
- Maintain RLS, explicit grants/revokes, and fixed `search_path` values for
  privileged SQL functions. Database cleanup remains inaccessible to ordinary
  anonymous/authenticated clients.
- Preserve the existing visual style. Size UI from available viewport space
  and actual text measurements; verify scrolling, smaller windows, disabled
  controls, and input behavior when changing interactive screens.

## Verification and collaboration

- Inspect `git status` and relevant diffs first. Preserve unrelated work;
  other tasks may be editing this checkout. Avoid broad rewrites and formatting
  changes outside the requested scope.
- Run focused tests for changed behavior, then the relevant compile/lint gates.
  Add regression tests for meaningful rules, boundaries, errors, concurrency,
  or persistence behavior; do not add tests that merely duplicate code.
- For shared Rust changes, consider both native and WASM. For source assets,
  regenerate and verify them. Use `just ci` for complete release/PR validation
  when that scope is appropriate.
- For SQL changes, verify fresh installation and a second reset against a
  disposable database, including permissions, RPC shapes, jobs, and cascading
  deletes. The Rust schema-contract checks alone do not execute PostgreSQL.
- Distinguish failures caused by the change from unrelated work or missing
  local tools. State what was checked and any remaining verification limits.
- Keep temporary verification files out of the source tree. Finish with a
  concise account of the change, validation, and any remaining deployment step.
