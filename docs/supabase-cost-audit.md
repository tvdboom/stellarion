# Supabase cost audit

Repository audit, 13 September 2026. This reviews `supabase/schema.sql`, the Rust
Supabase and mock backends, and the client request loop. No hosted database,
usage dashboard, or invoice was accessed, and no hosted reset was applied.

| Finding | Implemented reduction | Preserved behavior |
| --- | --- | --- |
| Start and resolution responses echoed the uploaded snapshot | Return server metadata; the writer retains the exact accepted snapshot after validating the acknowledgement | Canonical revision, roster, status, save time, and normal full loads for other players |
| Resolution retries downloaded commands before all players were ready | `resolution` reads return `[]` until the active current turn is complete, then return ready participant orders | Polling intervals, spectator rules, submission completeness checks, and competing-resolver revision checks |
| Draft recovery downloaded other players' orders | `mine` reads select the authenticated caller's row | Saved drafts, withdrawal, readiness generations, and recovery |
| Two B-tree indexes duplicated primary keys exactly | Remove `stellarion_turn_submissions_resolution` and `stellarion_game_events_replay` | Indexed resolution, replay, and cascading deletion |
| Scheduler execution history grew indefinitely | The existing minute cleanup prunes completed Stellarion job logs older than 48 hours | Recent diagnostics, unfinished runs, unrelated jobs, and Supabase-managed scheduler infrastructure |
| Event pruning logged complete old rows | Use default primary-key replica identity | INSERT-only Realtime notifications and durable replay |

Local SQL fixtures measure uncompressed JSON bodies: the two-player resolution
response shrinks from **6,635 to 553 bytes (91.7%)**. An incomplete resolution read
returns **2 bytes** instead of repeatedly returning available commands. The
two-player empty-order fixture occupies 317 bytes when complete; real command
payloads vary. These numbers exclude HTTP headers, WebSocket framing, and
compression, and are not a forecast of total billed egress.

The database keeps one current snapshot per game. Frequent presence responses
already exclude snapshots; ordinary draft saves already use compact acknowledgements.
Realtime publishes only semantic events, with fallback replay every 30 seconds
when connected and every 2 seconds when disconnected. Presence renews every
5 seconds with a 15-second lease. These timing guarantees remain unchanged.
Native HTTP already enables gzip and Brotli; browser HTTP uses browser-managed
compression. No additional application compression format or service was added.

The latest 2,048 durable events and the existing submission history remain
available. Resolved trades and joint attacks are pruned as before. Finished games
expire after 48 hours; every game also expires 30 days after its last snapshot
save, whichever deadline comes first. Memberships, recovery codes, orders, and
events cascade with game deletion. Presence never renews that retention deadline.
Auth identities are not purged: they can own other games or be needed for resume.

These changes reduce application data transfer, live index storage, and future
log growth. They do not prove a global minimum or automatically reduce provisioned
disk capacity. Supabase distinguishes database contents from disk usage, which
also includes WAL and other infrastructure. See [database and disk size](https://supabase.com/docs/guides/platform/database-size).

To quantify hosted savings, compare Database and Realtime egress for similar
numbers of player-hours and completed turns, and inspect table/index/TOAST sizes
and dead tuples. [Supabase's egress guidance](https://supabase.com/docs/guides/platform/manage-your-usage/egress)
supports returning fewer fields and avoiding unnecessary full-row write responses.
[pg_cron documents](https://github.com/citusdata/pg_cron#monitoring-jobs) that run
history is not cleaned automatically.

The disposable SQL suite verifies a fresh install and second reset, permissions,
retention and cascading deletion, scoped reads, and reconstruction of snapshot
write acknowledgements against independent canonical loads. It stubs scheduler
registration and does not exercise hosted Realtime delivery or actual billing.
Run `just verify-sql`, `cargo test --lib multiplayer:: -j12`, `just lint`, and
`just check-wasm` to verify the coordinated SQL/client contract.

Recorded results: the final disposable SQL run passed; multiplayer tests passed
with the app feature (92 tests) and without it (53 tests); browser compilation,
formatting of changed Rust files, and diff whitespace checks passed. Full Clippy
was blocked by unrelated `assertions_on_constants` diagnostics in
`tests/core/map_systems.rs` at lines 441 and 442. Those map-rendering changes
were already part of the shared checkout and were left intact.

Deployment requires the complete `supabase/schema.sql` reset and a rebuilt client
together. The reset intentionally deletes existing application data. There are
no migration files or additional backend services.
