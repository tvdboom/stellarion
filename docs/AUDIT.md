# Repository audit

This audit records material issues addressed during the multiplayer/native/WASM refactor. It focuses on correctness and maintainability rather than speculative micro-optimizations.

## Correctness and synchronization

- Removed Renet connection IDs, host/client state, UDP transport, and host-only save assumptions. Stable `PlayerId` values and one player vector now drive ownership and persistence.
- Moved turn resolution into a clone-then-commit deterministic core. Invalid submissions cannot partially mutate canonical state.
- Replaced ambient/thread randomness in persisted transitions with a stored seed and sequence-derived ChaCha8 stream.
- Stabilized army/unit serialization and player/mission ordering so identical inputs serialize identically.
- Added supported 2–4 player boundaries, duplicate ID detection, cross-reference checks, spectator/home consistency, and schema-version rejection.
- Found and removed an unbounded randomized map-placement rejection loop. Dense seeds could permanently jam during game creation; finite shuffled hex placement now covers the maximum 160-body supported map.
- Fixed the map resource-factor calculation sorting farthest worlds first and taking four despite its three-nearest-world rule.
- Added saturating/checked resource, revision, turn, and sequence arithmetic where wraparound could corrupt long-lived state.
- Added database compare-and-swap saves and resolution publication; simultaneous writers/resolvers now accept exactly one winner.
- Fixed a PostgreSQL null-semantics hole where an omitted `expected_revision` could bypass `<>` compare-and-swap checks. All three state-writing RPCs now reject malformed revisions and use `IS DISTINCT FROM`.
- Made submissions idempotent by canonical digest and rejected stale, cross-player, spectator, incomplete, conflicting, oversized, or overlong writes.
- Preserved local command drafts when a same-turn save or Realtime reload changes only the canonical revision; new turns still rebuild the ECS projection and clear the old draft.
- Bounded combat to 100 rounds and each probabilistic rapid-fire chain to 256 shots so adversarial or zero-damage fleets cannot loop forever, made mission movement snap without overshooting, and fixed repair animation events targeting the wrong same-kind unit.

## Security and recovery

- Restricted client configuration to HTTPS project URLs and public/publishable keys; runtime configuration and every release packaging path reject modern secret prefixes and legacy JWT `service_role` claims before a server credential can be shipped.
- Replaced direct table mutation with authenticated transactional RPCs and enabled RLS on every multiplayer table.
- Kept recovery hashes out of returned records and direct table privileges.
- Added high-entropy domain-separated recovery hashes, constant-time in-memory comparison, and one-use rotation on identity transfer.
- Added semantic validation of successful production RPC payloads before they can become canonical client state.
- Added proactive access-token refresh and same-user verification. A rejected anonymous refresh creates a replacement identity only after the rejection is definitive and directs the player through recovery instead of silently assigning a new slot.
- Fixed `SECURITY DEFINER` search paths and revoked helper/RPC defaults before granting only the intended authenticated entry points.
- Normalized configuration loaded from JSON/environment, parsed project URLs structurally, rejected embedded credentials/paths/query data, and retained HTTP only for loopback development.

## Disconnect and collection behavior

- Realtime is now a per-game authenticated wake-up channel rather than a source of mutable state.
- Added monotonic durable replay plus periodic polling, bounded reconnect backoff, and explicit connection/conflict UI states.
- Bounded event history, active missions, per-player report history, per-turn command drafts/payloads, old submission retention, recent-game storage, notices, and task processing so abandoned or hostile games do not grow collections indefinitely.
- Added recovery, reconnect-after-missed-events, exact resume, and concurrent race tests without timing sleeps.

## Native/WASM and storage

- Removed raw UDP/native-network dependencies from browser builds.
- Isolated filesystem/config paths from browser localStorage and browser Fetch/WebSocket APIs.
- Migrated the application to current Bevy APIs and separated native/WASM task future bounds.
- Added adjacent JSON and environment configuration for all platforms; local storage contains no authoritative game snapshot.
- Made native profile replacement recoverable across interrupted writes with an fsynced temporary file, primary backup, rollback, and backup read fallback.
- Replaced native-only Basis C++ linkage with a pure-Rust adaptive KTX2 loader, allowing the same assets and code to compile for WASM.
- Fixed compressed KTX2 uploads for odd-sized UI sprites: BC7/ASTC/ETC2 now fall back to RGBA8 unless both dimensions satisfy the selected GPU format's block geometry.
- Added a bounded two-thread Tokio reactor for native reqwest work. Configured native clients previously panicked as soon as Supabase opened a TCP connection because Bevy's task pool did not provide Tokio's network driver; executor failures now become an offline UI state.
- Bound Bevy to the packaged `#bevy` canvas, resolved browser configuration relative to the page URL, and left canvas positioning to the browser so HTML builds neither create a second undersized canvas nor emit monitor-selection failures.
- Restored the pre-refactor Fira/Nord menu presentation, flat interaction colors, and unobscured artwork; menu backgrounds now use an aspect-preserving cover crop so every native and browser viewport is filled.

## Assets and loading

- Split editable `assets` from generated `assets-runtime`, omitted fifteen confirmed unused runtime images, and converted 196 textures to UASTC/Zstd KTX2.
- Reduced runtime asset bytes from roughly 122.8 MiB to 52.2 MiB (42.5%).
- Added source/output SHA-256 fingerprints so CI freshness is based on bytes rather than unreliable checkout timestamps.
- Added finite worker limits, one-thread encoder processes, safe path validation, stale-output tracking, and temporary output files.
- Deferred world/unit/combat/effect/music groups until a selected game enters its loading state; boot requests only menu essentials.
- Removed Bevy's unused built-in audio/Vorbis feature after optimized WASM LTO exposed duplicate `cpal` WebAudio globals; Kira remains the sole audio backend and release size is lower.

## Error and panic handling

- Backend, storage, configuration, recovery, and simulation failures use typed errors rather than network-path panics.
- Missing asset registry keys now log and return harmless handles instead of indexing panics.
- Malformed persisted JSON is validated as a complete envelope before installation.
- Production `unwrap`/`expect` sites were removed from fallible UI, asset, camera, storage, generation, and transport paths. The two remaining map lookup panics encode the documented invariant that validated contiguous planet IDs permit direct O(1) indexing; untrusted boundaries use `try_get` and full snapshot validation.

## Verification scope

Automated coverage includes lobby sizes, joining/full/duplicate behavior, session reconnect, recovery rotation/failures, saves by multiple players, stale and simultaneous CAS writes, idempotent/stale submissions, bounded command drafts and combat chains, concurrent resolution, durable event catch-up, deterministic resolution, serialization, malformed state, completion, resource and ownership properties, storage, deferred asset state, responsive cover cropping, real Basis transcoding, native network-reactor availability, Realtime protocol messages, production error mapping, and static SQL security contracts. The final native suite contains 50 passing tests, strict Clippy passes with warnings denied, the WASM development check passes without warnings, and optimized Windows and HTML5 packages build successfully.

The single fresh-state `supabase/schema.sql` file passes a PostgreSQL parser, but it must still be applied to the supplied fresh Supabase/PostgreSQL project. Its public Auth settings endpoint is reachable, but anonymous sign-ins were disabled at verification time and must be enabled before multiplayer authentication can work. No server key, database password, or itch.io credential is present in this repository, and Docker Desktop was unavailable during the local audit. Linux and universal macOS release jobs are configured in CI but could not be executed natively from this Windows machine.

## Dependency refresh

Every direct crate was checked against crates.io on 2026-08-31 and pinned to the latest stable release available that day. `cargo update` refreshed the compatible transitive graph and lockfile. Old Renet, bincode, file-dialog, notification, regex, and host-era support crates were removed; redundant Bevy audio features were also removed after release-profile validation showed they were unused and incompatible with Kira under WASM LTO.
