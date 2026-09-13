# Performance and duplication audit

13 September 2026. Static review covered the simulation, combat, map and egui
presentation, asset pipeline, multiplayer backends, local storage, SQL, packaging,
and their tests. Changes are measured against the working checkout at the start of
this task, including the user's existing uncommitted work.

## Changes implemented

| Area | Finding and change |
| --- | --- |
| Combat report details | Opening the total-round view copied combatants and their nested shot histories every frame. `CombatRoundView` now borrows the original rounds; grids also borrow opposing combatants. `CombatStatistics` counts damage, repairs and shot categories in one pass without temporary shot vectors. |
| Multiplayer requests | Several operations copied an entire `GameRecord` to retain its ID across an asynchronous call. They now copy the ID and, when needed, revision. Save and submission validation borrow the current game. Operations that actually need an owned simulation snapshot retain it. |
| Turn simulation | Ordinary commands are applied through a borrowed filtered iterator. Due joint missions move out of their queue with `extract_if`. Adding an immediate trade rolls back the appended agreement on validation failure instead of cloning the complete game. |
| Recycling | Fleet-loss accounting reads combined garrison counts without constructing two combined armies. Expired and future reports are rejected before inspecting their fleets; duplicate-report precedence remains unchanged. |
| UI and map lookup | Literal image lookup no longer allocates a `String`. Resource and industrial building predicates consult static rosters. Attacker membership checks query the existing map directly. Hover selection finds the earliest matching mission in one pass instead of sorting all missions. |
| Shared behavior | Backend success/error mapping, linear asset loading, gameplay asset registration, HUD button backgrounds, energy accumulation, temperature generation and raw asset copying share existing rules or small helpers. |
| SQL | Save and End Turn share submission-envelope validation and joint-attack launch freezing. Both helpers are private, retain a fixed `search_path`, and execute within the calling RPC's transaction. The launch helper runs after membership/lifecycle checks under the existing game-row lock. |
| Tests | Repeated neutral mission-report and development-art fixtures share test-only constructors; scenario-specific state stays explicit at each call site. Regression coverage checks borrowed report identity/order, aggregate statistics and trade rollback. SQL checks cover helper permissions and malformed submissions through both public RPCs. |

The existing seeded random draw order, transport shapes, authentication checks,
submission generations, revision handling and retention deadlines are preserved.
No dependency, migration, hosted service, or serialized model format was added.

The full gate also exposed existing test/layout inconsistencies: the intel panel
had seven orbital entries while its test expected six, its group spacing exceeded
the compact-height budget, and a color test expected full opacity at a partially
faded zoom level. The entry count and test zoom are now explicit, the panel spacing
fits its existing budget, and constant assertions use a const block for Clippy.

## Line count

The **15% reduction target was not achieved**. Counting physical lines, including
comments and blanks, in `.rs`, `.sql`, `.mjs`, `.ps1`, `.sh` and `.html` files:

| Scope | Before | After | Removed |
| --- | ---: | ---: | ---: |
| Rust application/library | 50,704 | 50,683 | 21 |
| Tests and verification | 35,012 | 35,060 | -48 |
| Build/package scripts | 993 | 985 | 8 |
| Database reset | 3,178 | 3,156 | 22 |
| Browser shell | 118 | 118 | 0 |
| **Total** | **90,005** | **90,002** | **3 (0.003%)** |

Generated outputs, assets, documentation and dependency files are excluded.
Typed borrowing helpers and regression coverage offset most of the removed
boilerplate. Reaching 15% would require removing about 13,501 lines from this
baseline. This pass did not identify that volume of redundant code; it preserves
the established formatting and existing behavioral coverage.

## Remaining candidates for profiling

These are code-level observations, not measured end-to-end bottlenecks.

| Priority | Location | Evidence and next measurement |
| --- | --- | --- |
| High | `src/core/combat/resolution.rs::choose_combat_target` | Each shot scans the enemy roster to find a priority and again to select a target. Benchmark large fleets and rapid-fire chains before introducing indexed target groups. Any replacement must preserve target ordering and seeded RNG consumption. |
| High | `src/multiplayer/authority.rs::same_snapshot` | Comparison materializes two complete JSON value trees. Measure peak allocations on snapshots containing large combat histories. A replacement must preserve JSON equality semantics, including numeric values, and validation errors. |
| Medium | `src/core/map/details.rs::refresh_details` | Cache invalidation includes the broad `MultiplayerSession` change flag, so session updates unrelated to map artwork can trigger refresh work. Instrument refresh frequency before splitting presentation revisions; cover colors, intelligence and late asset loading. |
| Medium | `src/core/combat/systems.rs::update_combat_stats` | Per-frame count calculations revisit combat reports and replace text strings. Measure CPU time and Bevy text-layout invalidation; cache displayed counts while leaving animated shield/hull interpolation active. |
| Medium | `src/core/ui/systems.rs::next_turn_planet` | Multiple production views clone and project the same planets. Measure visible HUD/tooltip workloads before adding a cache tied to pending commands and model revisions. |
| Medium | Persisted report history | Reports are bounded at 512 per player, but each can contain many rounds and shots. Profile worst-case snapshot memory, serialization and failure-atomic simulation copies before changing ownership or representation. |

The asset pipeline already distinguishes straight-alpha world textures from
premultiplied UI textures, uses compressed runtime assets and defers gameplay
loading. Storage already performs native writes off the frame thread; Realtime
already bounds incoming processing. Those mechanisms remain in place.

## Measurement and verification

The repeatable combat-preparation comparison lives in
`tests/core/ui_combat.rs::benchmark_combat_report_preparation`:

```text
cargo test --all-features --lib benchmark_combat_report_preparation -j12 -- --ignored --nocapture
```

It compares copied versus borrowed preparation with the same statistics pass over
eight rounds of 1,024 combatants with eight shots each, repeated 100 times. This
isolates report preparation; it does not measure frame rate, GPU performance,
whole-game peak memory or browser runtime performance.

One local run using the repository's optimized development/test profile measured
**366.5482 ms copied versus 18.4478 ms borrowed**, approximately **19.9 times
faster** for this workload. Both paths use the same new statistics accumulator,
so this comparison isolates the borrowing improvement rather than also crediting
the reduction in statistics passes. The borrowed path allocates no combatant or
shot-history copies.

Completed checks:

- Rust formatting and Clippy with warnings rejected across all targets/features.
- All-target/all-feature tests: 731 passed, six optional tests ignored. The
  preparation benchmark was run separately and passed.
- Shared-library tests with default features disabled: 270 passed, one ignored.
  This configuration retains the pre-existing unused `Mission::includes_bombing`
  warning; the all-feature Clippy gate is clean.
- Browser compilation for `wasm32-unknown-unknown`.
- Asset verification: all 436 runtime assets match their source hashes.
- Packaging cleanup path-boundary checks.
- Disposable SQL verification: fresh installation, second reset, authenticated
  RPCs, helper access restrictions, readiness/revision races, retention and
  cascading cleanup. Scheduler registration was stubbed by the existing PGlite
  harness. The dependency fetch required a retry with the existing approved
  network permission after the sandbox blocked npm.
- `git diff --check`.

No interactive desktop/browser playthrough, GPU capture, production allocation
profile or hosted database measurement was performed. No hosted reset was
applied. Installing the SQL refactor on Supabase remains a separate destructive
reset through `supabase/schema.sql`, under the repository's existing setup policy.
