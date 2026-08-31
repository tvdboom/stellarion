# Multiplayer architecture

Stellarion is a deterministic peer-resolved game. There is no custom game server and no player is a permanent host. Rust clients run the same rules; Supabase supplies anonymous identity, transactional coordination, persistence, authorization, durable events, and Realtime wake-ups.

## Boundaries

The code is split into three layers:

- `src/core/simulation.rs` is the plain deterministic model. It has no socket, filesystem, clock, task-pool, or Bevy-world dependency. `GameModel`, `TurnSubmission`, and `resolve_turn` are serializable and testable in isolation.
- `src/multiplayer` defines an object-safe backend contract. `SupabaseBackend` and `InMemoryBackend` implement the same create/join/recover/load/save/submit/resolve/event operations.
- Bevy systems in `src/multiplayer/client.rs` translate UI intent into backend operations. `src/core/loading.rs` installs a validated canonical snapshot into the ECS only after deferred gameplay assets are ready.

Supabase is authoritative for which transaction wins, but it does not execute gameplay rules. This design prevents synchronization corruption and lost writes; it is not intended to prevent a modified client from cheating.

## Identity and membership

At startup the client restores a refresh token from platform storage or creates a Supabase anonymous user. Anonymous users have stable UUIDs and use PostgreSQL's `authenticated` role. A `stellarion_game_players` row maps that UUID to a stable numeric `PlayerId` within one game. The client checks expiry every 30 seconds and refreshes five minutes early. It verifies that refresh did not change the user UUID; a conclusively rejected token routes a newly created identity to **Recover Player**, while offline refresh failures retry without discarding the old identity.

A game supports exactly two, three, or four slots. Player IDs, ownership references, home worlds, submissions, and events all use the stable slot ID rather than a Renet connection ID. Reconnecting with the same Auth session recovers the mapping automatically.

Each newly claimed slot receives a locally generated 192-bit recovery code. Only its domain-separated SHA-256 hash reaches Supabase. Recovery from a different anonymous user verifies the old hash inside a locked transaction, reassigns the membership, increments `identity_version`, and rotates to a new recovery hash. The old user and old code stop working.

## Persistence and revisions

`PersistedGame` is a versioned envelope:

```text
PersistedGame
  schema_version = 1
  state = GameModel
    rules, status, turn, deterministic RNG cursor
    all players and resources
    complete map, ownership, queues, and armies
    all missions and their logs/state
```

Database `revision` is independent from `schema_version`. Any member may save, but every state-changing RPC receives the revision it loaded. The row is locked and the write succeeds only when that revision is still current. Null and negative revisions are rejected, and PostgreSQL uses null-safe comparison so an omitted value cannot bypass the check. A losing writer receives `BackendError::Conflict`, reloads, and never overwrites newer data.

Successful Supabase JSON is treated as untrusted transport input. Before a record reaches the ECS, Rust validates the schema, core cross-references, status/rule agreement, game capacity, unique ordered player/user mappings, creator mapping, event cursors, and submission boundaries.

## Simultaneous turns

Only intentional commands are submitted. Local hover state, windows, selections, animations, and other rendering details never enter multiplayer traffic.

1. Every active, non-spectating player submits one `TurnSubmission` for the current turn.
2. `(game_id, turn, player_id)` is unique. An identical retry returns `Duplicate`; a different payload for that key is rejected.
3. A durable `turn_submitted` event wakes every connected client.
4. Any client may load the canonical submissions and run `resolve_turn` when the complete active-player set is present.
5. Each resolver publishes the byte-equivalent next snapshot with the revision and resolved turn it read.
6. PostgreSQL locks the game, rechecks membership, revision, phase, turn, and submission completeness, then accepts one transaction. Competing resolvers receive a conflict and reload the accepted state.

There is deliberately no designated host. If one client disconnects after all commands are submitted, another client resolves. Retrying after an unknown network outcome is safe because submissions are idempotent and state publication is compare-and-swap.

The random stream is `ChaCha8Rng` derived from a 256-bit match seed plus a serialized sequence counter. Resolution sorts or otherwise stabilizes externally unordered inputs before consuming randomness. The same state and submissions therefore produce the same serialized next state on native and WASM clients.

## Realtime and disconnects

`SupabaseRealtimeClient` opens the documented Phoenix WebSocket endpoint with the anonymous user's access token. It subscribes only to `INSERT` events for the selected `game_id` in `stellarion_game_events` and selects only the event identity columns needed for a wake-up.

Realtime payloads never become canonical game data. A message sets a wake-up flag; the client then replays durable events through an authenticated RPC and reloads the game record. This avoids stale, duplicated, missed, or forged notification payloads changing gameplay.

The socket sends heartbeats, reconnects with bounded exponential backoff, and surfaces `Connected`, `Reconnecting…`, `Offline`, or `Sync conflict`. A two-second durable poll continues independently, so a missed Realtime message or long disconnect cannot strand the game. Event cursors are monotonic, replay batches are bounded to 256, and the database retains the latest 2,048 per game.

## Local storage

Local storage contains only the Auth session, recent game IDs, and convenience preferences:

- browser: origin-scoped `localStorage` keys under `stellarion.v1.*`;
- native: the platform's per-user application configuration directory through `directories::ProjectDirs`, with fsynced temporary replacement and backup recovery;
- tests: a mutex-protected in-memory implementation.

The complete multiplayer game is never stored only on the client. Clearing local data loses the anonymous session, but a recovery code can move a slot to a new identity.

## Bevy projection and loading

Boot requests only the menu background, button textures, essential UI icons, fonts, and menu audio. Selecting or starting a game enters `AppState::LoadingGame`, requests the world/unit/effect/audio groups once, waits on recursive asset load states, then projects the selected player's map, resources, visible missions, and turn into Bevy resources. Returning to gameplay after a canonical next turn reinstalls the new projection rather than merging stale ECS state.

Gameplay writes flow in the opposite direction as `TurnCommand` values accumulated in `PendingTurnCommands`; the core resolver, not rendering systems, defines the persisted transition. A reload that changes only the same turn's revision preserves the ECS projection and draft commands. A changed game, phase, or turn installs a fresh projection and clears commands that no longer belong to the canonical turn.

## Security properties

- Client builds contain only a publishable public key and a user's JWT. Secret/service-role keys and database credentials are rejected by configuration validation.
- Tables are RPC-only except the RLS-filtered semantic event stream needed by Realtime.
- Every mutating RPC is `SECURITY DEFINER`, has a fixed `search_path`, authenticates `auth.uid()`, validates inputs, and performs authorization and concurrency checks in the same transaction.
- Recovery hashes are never returned or directly selectable by client roles.
- Join and recovery are narrowly controlled RPCs rather than public table inserts/updates.
- Collections with operational growth are bounded: each player retains 512 reports, a game retains 4,096 active missions and at most 64 MiB of persisted JSON, each turn submission accepts 1,024 commands and at most 1 MiB of JSON, events retain 2,048 rows per game, old turn submissions retain a short resolution window, and local recent games retain twenty IDs.

See [Supabase setup](SUPABASE_SETUP.md), [assets](ASSETS.md), and [build/release](BUILD_AND_RELEASE.md).
