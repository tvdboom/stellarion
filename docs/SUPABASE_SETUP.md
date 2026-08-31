# Fresh Supabase setup

This project intentionally supports only the current versioned multiplayer schema. `supabase/schema.sql` is a complete fresh-project state file, not a migration chain. Reset old Stellarion database objects instead of upgrading them in place.

## Seven-step setup

1. Create a new project in the [Supabase Dashboard](https://supabase.com/dashboard).
2. Paste and run `supabase/schema.sql` in the Dashboard SQL Editor. It is the only database setup file.
3. In Authentication settings, enable **Allow anonymous sign-ins**. Supabase anonymous users are authenticated users with stable UUIDs; they are not the unauthenticated `anon` database role. See [Anonymous Sign-Ins](https://supabase.com/docs/guides/auth/auth-anonymous).
4. Confirm Realtime is enabled for `public.stellarion_game_events`. The schema creates/uses `supabase_realtime` and adds only that table. Verify with `select * from pg_publication_tables where pubname = 'supabase_realtime';` or the Dashboard Publications/Replication page. See [Postgres Changes](https://supabase.com/docs/guides/realtime/postgres-changes).
5. Copy the project's HTTPS URL into `SUPABASE_URL`.
6. Copy an `sb_publishable_...` key from **Settings → API Keys** into `SUPABASE_PUBLISHABLE_KEY`. A legacy public `anon` key also works, but new projects should use a publishable key. Never use `sb_secret_...` or `service_role`. See [Supabase API keys](https://supabase.com/docs/guides/getting-started/api-keys).
7. Configure and run/package Stellarion using one of the methods below.

## Applying and validating the schema

For a remote project, use the Dashboard SQL Editor or PostgreSQL directly:

```text
psql "$DATABASE_URL" -v ON_ERROR_STOP=1 -f supabase/schema.sql
```

Run the file only against a fresh/reset Stellarion schema. There is deliberately no `supabase/migrations/` directory, version history, upgrade path, or compatibility logic. The supplied schema runs in one transaction and creates:

- `stellarion_games`: versioned state, phase, turn, revision, capacity, and event cursor;
- `stellarion_game_players`: Auth UUID to stable player slot, display name, recovery hash, and coarse presence;
- `stellarion_turn_submissions`: unique per-turn command payloads and idempotency digests;
- `stellarion_game_events`: bounded durable semantic notification log.

It also creates constraints, foreign keys, lookup/resolution indexes, four enabled RLS policies, twelve authenticated RPCs, fixed function grants, and the Realtime publication entry. Direct client writes are revoked. Only authenticated members can select an event row, and the other tables are accessed through authorization-checking RPCs.

The Rust tests statically guard the expected schema/RPC/RLS contract, including null-safe revision checks, bounded map/report/mission/submission validation, and typed malformed-input paths. A PostgreSQL parser accepts the complete file. A real apply must still be performed against the fresh project because parsing alone cannot validate Supabase-owned Auth/Realtime objects or runtime privileges.

## Client configuration

All values below are public client configuration. The publishable key is expected to be recoverable from a browser bundle or executable; RLS and authenticated RPC checks provide authorization.

### Native development

Set environment variables before launching:

```text
SUPABASE_URL=https://YOUR_PROJECT_REF.supabase.co
SUPABASE_PUBLISHABLE_KEY=sb_publishable_...
cargo run -j12
```

Alternatively, copy `stellarion-config.example.json` to `stellarion-config.json` beside the executable or in the current working directory:

```json
{
  "url": "https://YOUR_PROJECT_REF.supabase.co",
  "publishable_key": "sb_publishable_..."
}
```

The same JSON method works for Windows, Linux, and macOS packages.

### WASM and itch.io

Place `stellarion-config.json` beside `index.html`. The browser fetches it over the same origin. The packaging scripts create it from `SUPABASE_URL` and `SUPABASE_PUBLISHABLE_KEY` when both are set, accept an explicit config path, or leave only the example when values are absent.

Compile-time environment values are also supported, but the adjacent JSON file is recommended because it lets one WASM build target different projects without recompilation.

### Credential-free development

When public Supabase configuration is missing, the connection footer identifies the in-memory backend without treating the expected development mode as an error. Invalid supplied configuration is reported explicitly. This supports game-flow development and automated tests without an account. Its data lasts only for that process and is not a multiplayer deployment.

## Authentication and recovery lifecycle

The client calls the Auth signup endpoint to create an anonymous identity, stores its refresh token locally, and refreshes it on later launches and shortly before access-token expiry. A successful refresh must return the same user UUID. Definitive refresh rejection creates a replacement anonymous identity and opens the recovery flow; temporary network failures retain the current identity and retry. Game membership is discovered from `auth.uid()`; the user never chooses a player number on reconnect.

Anonymous identity is device/browser-storage dependent. When a player joins, Stellarion displays a recovery code that should be saved outside the application. Recovery:

1. creates/restores an anonymous user on the replacement installation;
2. sends the game code, SHA-256 of the old recovery code, and SHA-256 of a newly generated code;
3. atomically reassigns that one player row and rotates the hash;
4. invalidates the old Auth user's membership and old recovery code.

Plaintext recovery codes never enter the database. A recovery code is effectively a bearer credential, so users should treat it like a password.

## Operational checks

After setup, verify these flows with two separate browser profiles or one browser and one native build:

- create a two-player lobby and join by six-character code;
- close/reopen one client and resume through the restored anonymous session;
- submit both turns and observe exactly one new revision/turn;
- temporarily block WebSockets and confirm the durable poll catches up;
- recover one slot in a new profile, then confirm the old profile and recovery code no longer work;
- inspect Security Advisor and confirm no unintended direct table privilege was added.

If Realtime is silent, first verify the table appears in `pg_publication_tables` and that the current JWT can select its event rows under RLS. Gameplay remains safe while Realtime is unavailable because durable polling is always enabled.
