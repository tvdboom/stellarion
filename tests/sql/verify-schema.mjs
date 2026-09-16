import { readFile } from "node:fs/promises";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createRequire } from "node:module";
import { fileURLToPath } from "node:url";

const root = new URL("../../", import.meta.url);
// Verification tooling lives in ignored build output, independent of game setup.
const requireTool = createRequire(new URL("target/sql-verification/loader.cjs", root));
const { PGlite } = requireTool("@electric-sql/pglite");
const { pgcrypto } = requireTool("@electric-sql/pglite/contrib/pgcrypto");
const generated = spawnSync("cargo", [
  "run", "--quiet", "--no-default-features", "--features", "sql-verification",
  "--bin", "sql-fixtures", "-j", process.env.STELLARION_JOBS ?? "12",
], { cwd: fileURLToPath(root), encoding: "utf8", maxBuffer: 8 * 1024 * 1024 });
assert.equal(generated.status, 0, generated.error?.message ?? generated.stderr);
const fixtures = JSON.parse(generated.stdout);
const schemaPath = new URL("supabase/schema.sql", root);
const db = new PGlite({ extensions: { pgcrypto } });
await db.exec(`
  create role anon;
  create role authenticated;
  create role service_role bypassrls;
  create schema auth;
  create table auth.users (id uuid primary key);
  create function auth.uid() returns uuid language sql stable as $$
    select nullif(current_setting('request.jwt.claim.sub', true), '')::uuid
  $$;
  create schema cron;
  create table cron.job (jobid bigint generated always as identity,
    jobname text primary key, schedule text, command text,
    database text default current_database());
  create table cron.job_run_details (jobid bigint, status text, end_time timestamptz);
  create function cron.schedule(text, text, text) returns bigint language sql as $$
    insert into cron.job (jobname, schedule, command) values ($1, $2, $3)
    on conflict (jobname) do update set schedule = excluded.schedule, command = excluded.command
    returning jobid
  $$;
  create function cron.unschedule(bigint) returns boolean language sql as $$
    with removed as (delete from cron.job where jobid = $1 returning jobid)
    select exists(select 1 from removed)
  $$;
`);
// PGlite has no background workers: stub only pg_cron, execute all app SQL.
const schema = (await readFile(schemaPath, "utf8"))
  .replace("create extension pg_cron with schema pg_catalog;", "null;");
await db.exec(schema);
assert.equal(
  (await db.query("select count(*)::int as n from cron.job")).rows[0].n,
  1,
);
console.log(
  "Fresh installation of schema.sql passed (pg_cron registration stubbed).",
);
await db.exec(`
  insert into auth.users values ('00000000-0000-0000-0000-000000000001');
  insert into public.stellarion_games
    (id, code, created_by, max_players, status, state, current_turn)
  values ('10000000-0000-0000-0000-000000000001', 'ABCDEF',
    '00000000-0000-0000-0000-000000000001', 2, 'lobby', '{}'::jsonb, 1);
  create table public.obsolete_game_data (id int);
  insert into public.obsolete_game_data values (1);
  create function public.obsolete_game_rpc() returns int language sql as $$ select 1 $$;
  select cron.schedule('stellarion-stale-cleanup', '0 0 * * *', 'select 1');
  select cron.schedule('external-job', '0 0 * * *', 'select 2');
  insert into cron.job_run_details (jobid, status) select jobid, 'succeeded' from cron.job;
`);
await db.exec(schema);
assert.equal(
  (await db.query("select count(*)::int as n from public.stellarion_games"))
    .rows[0].n,
  0,
);
assert.equal(
  (await db.query("select to_regclass('public.obsolete_game_data') as obj"))
    .rows[0].obj,
  null,
);
assert.equal(
  (await db.query(
    "select to_regprocedure('public.obsolete_game_rpc()') as obj",
  )).rows[0].obj,
  null,
);
assert.equal(
  (await db.query("select count(*)::int as n from auth.users")).rows[0].n,
  1,
);
assert.deepEqual(
  (await db.query("select jobname from cron.job order by jobname")).rows.map(
    (row) => row.jobname,
  ),
  ["external-job", "stellarion-delete-expired-games"],
);
assert.equal(
  (await db.query("select count(*)::int as n from cron.job_run_details"))
    .rows[0].n,
  1,
);
const job = (await db.query(
  "select * from cron.job where jobname = 'stellarion-delete-expired-games'",
)).rows[0];
assert.equal(job.schedule, "* * * * *");
assert.equal(job.command, "select public.stellarion_delete_expired_games();");
// A duplicate B-tree adds storage and write work but cannot improve these scans.
for (const table of ["stellarion_turn_submissions", "stellarion_game_events"]) {
  assert.equal((await db.query(
    "select count(*)::int as n from pg_indexes where schemaname = 'public' and tablename = $1",
    [table],
  )).rows[0].n, 1);
}
assert.equal((await db.query(
  "select relreplident from pg_class where oid = 'public.stellarion_game_events'::regclass",
)).rows[0].relreplident, "d");

await db.exec(`
  insert into cron.job (jobname, database) values ('stellarion-foreign-job', 'another_database');
  insert into cron.job_run_details (jobid, status, end_time)
    select jobid, 'old', now() - interval '49 hours' from cron.job;
  insert into cron.job_run_details (jobid, status, end_time)
    select jobid, 'recent', now() - interval '47 hours' from cron.job;
  insert into cron.job_run_details (jobid, status)
    select jobid, 'running' from cron.job;
`);
await db.query(job.command);
const logs = (await db.query(`
  select j.jobname, d.status from cron.job_run_details d
  join cron.job j using (jobid) where d.status in ('old', 'recent', 'running')
`)).rows;
assert(!logs.some(log => log.jobname === job.jobname && log.status === "old"));
assert.equal(logs.length, 8, "keep recent/running logs and all unrelated or foreign-database logs");
await db.exec("delete from cron.job where jobname = 'stellarion-foreign-job'");
console.log("Duplicate indexes removed; default event identity and bounded, isolated scheduler logs verified.");
assert.equal(
  (await db.query(
    "select count(*)::int as n from pg_class c join pg_namespace s on s.oid = c.relnamespace where s.nspname = 'public' and c.relrowsecurity",
  )).rows[0].n,
  6,
);
console.log(
  "Second reset removed app data, obsolete objects/jobs/history, and recreated current RLS and schedule; managed auth and unrelated jobs survived.",
);
for (const route of fixtures.trade_routes) {
  const result = await db.query(
    `select public.stellarion_trade_route_valid($1, $2) as valid,
      public.stellarion_trade_capacity($1, ($1::jsonb ->> 'owned')::bigint)::int as first_capacity,
      public.stellarion_trade_capacity($2, ($2::jsonb ->> 'owned')::bigint)::int as second_capacity`,
    [route.first, route.second],
  );
  assert.deepEqual(result.rows[0], {
    valid: route.valid, first_capacity: route.first_capacity, second_capacity: route.second_capacity,
  },
    `trade route must match Rust: levels ${route.first.army.controller["Building(TradingPost)"]}/` +
    `${route.second.army.controller["Building(TradingPost)"]} at ${route.second.position[0]} world units`);
}
console.log("SQL trade range and capacity match Rust for both post levels, missing posts, and range boundaries.");
await db.exec(`
  insert into public.stellarion_games
    (id, code, created_by, max_players, status, state, current_turn, finished_at)
  select ('20000000-0000-0000-0000-' || lpad(i::text, 12, '0'))::uuid,
    lpad(i::text, 6, '0'), '00000000-0000-0000-0000-000000000001', 2,
    case when i = 1 then 'lobby' when i = 2 then 'active' else 'finished' end,
    '{}'::jsonb, 7,
    case when i = 3 then now() - interval '47 hours' else now() - interval '48 hours' end
  from generate_series(1, 4) i;
  insert into public.stellarion_game_players
    (game_id, player_id, user_id, display_name, recovery_code, is_creator)
  select id, 1, created_by, 'Tester', '0123-4567-89AB-CDEF', true from public.stellarion_games;
  insert into public.stellarion_turn_submissions (game_id, turn, player_id, submission, digest)
  select game_id, 7, player_id, '{}'::jsonb, repeat('b', 64) from public.stellarion_game_players;
  insert into public.stellarion_game_events (game_id, sequence, kind)
  select id, 1, 'state_changed' from public.stellarion_games;
`);
assert.equal(
  (await db.query(job.command)).rows[0].stellarion_delete_expired_games,
  1,
);
assert.deepEqual(
  (await db.query("select code from public.stellarion_games order by code"))
    .rows.map((row) => row.code),
  ["000001", "000002", "000003"],
);
for (
  const table of [
    "stellarion_game_players",
    "stellarion_turn_submissions",
    "stellarion_game_events",
  ]
) {
  assert.equal(
    (await db.query(`select count(*)::int as n from public.${table}`)).rows[0]
      .n,
    3,
  );
}
for (const role of ["anon", "authenticated"]) {
  await db.exec(`set role ${role}`);
  await assert.rejects(db.query(job.command), /permission denied/);
  await db.exec("reset role");
}
console.log(
  "Current cleanup command, 48-hour cutoff, cascading deletion, and restricted cleanup permissions passed.",
);

// Cross both deadlines for every lifecycle state. Recent presence must not extend
// snapshot retention, and recent completion must not override save expiry.
await db.exec(`
  delete from public.stellarion_games;
  insert into public.stellarion_games
    (id, code, created_by, max_players, status,
     state, current_turn, created_at, saved_at, updated_at, finished_at)
  select ('20000000-0000-0000-0000-' || lpad(i::text, 12, '0'))::uuid,
    lpad(i::text, 6, '0'), '00000000-0000-0000-0000-000000000001', 2,
    case when i <= 3 then 'lobby' when i <= 6 then 'active' else 'finished' end,
    '{}'::jsonb, 7, now() - interval '90 days',
    now() - case when i % 3 = 1 then interval '29 days'
                when i % 3 = 2 then interval '30 days'
                else interval '31 days' end,
    now(), case when i >= 7 then now() - interval '1 day' else null end
  from generate_series(1, 9) i;
  insert into public.stellarion_game_players
    (game_id, player_id, user_id, display_name, recovery_code, is_creator)
  select id, 1, created_by, 'Tester', '0123-4567-89AB-CDEF', true from public.stellarion_games;
  insert into public.stellarion_turn_submissions (game_id, turn, player_id, submission, digest)
  select game_id, 7, player_id, '{}'::jsonb, repeat('b', 64) from public.stellarion_game_players;
  insert into public.stellarion_game_events (game_id, sequence, kind)
  select id, 1, 'state_changed' from public.stellarion_games;
  select set_config('request.jwt.claim.sub', '00000000-0000-0000-0000-000000000001', false);
`);
const retained = (await db.query("select public.stellarion_list_games() as result")).rows[0].result;
assert.deepEqual(retained.map(game => game.code).sort(), ["000004", "000007"]);
assert.equal((await db.query(job.command)).rows[0].stellarion_delete_expired_games, 6);
assert.deepEqual(
  (await db.query("select code from public.stellarion_games order by code")).rows.map(row => row.code),
  ["000001", "000004", "000007"],
);
for (const table of ["stellarion_game_players", "stellarion_turn_submissions", "stellarion_game_events"]) {
  assert.equal((await db.query(`select count(*)::int as n from public.${table}`)).rows[0].n, 3);
}
assert.equal((await db.query(job.command)).rows[0].stellarion_delete_expired_games, 0);
console.log("30-day last-save cutoff across all statuses, resume filtering, and cascading cleanup passed.");

// Exercise the same authenticated RPCs as native/browser clients, without a
// service-role dispatcher, server runtime, Docker, or any hosted connection.
await db.exec("delete from public.stellarion_games");
const host = "00000000-0000-0000-0000-000000000001";
const guest = "00000000-0000-0000-0000-000000000002";
const outsider = "00000000-0000-0000-0000-000000000003";
const hostRecovery = "0123-4567-89AB-CDEF";
const guestRecovery = "FEDC-BA98-7654-3210";
await db.query("insert into auth.users values ($1), ($2)", [guest, outsider]);
const rpc = async (actor, sql, params = [], role = "authenticated") => {
  await db.query("select set_config('request.jwt.claim.sub', $1, false)", [actor ?? ""]);
  await db.exec(`set role ${role}`);
  try {
    return (await db.query(sql, params)).rows[0]?.result;
  } finally {
    await db.exec("reset role");
  }
};
const create = (actor, code = "ABCDEF", role = "authenticated", persisted = fixtures.lobby) => rpc(actor,
  "select public.stellarion_create_game($1, $2, $3, $4, $5) as result",
  [code, "Host", hostRecovery, 4, persisted], role);
await assert.rejects(create(null, "ABCDEF", "anon"), /permission denied/);
await assert.rejects(create(null), /STLR_UNAUTHENTICATED/);
const incompleteLobby = structuredClone(fixtures.lobby);
delete incompleteLobby.state.rules.practice_mode;
await assert.rejects(create(host, "ABCDEG", "authenticated", incompleteLobby), /STLR_INVALID_DATA:rules/);
const missingOrbitalStrikes = structuredClone(fixtures.lobby);
delete missingOrbitalStrikes.state.orbital_strikes;
await assert.rejects(create(host, "ABCDEN", "authenticated", missingOrbitalStrikes), /STLR_INVALID_DATA:persisted object/);
const malformedOrbitalStrikes = structuredClone(fixtures.lobby);
malformedOrbitalStrikes.state.orbital_strikes = {};
await assert.rejects(create(host, "ABCDEP", "authenticated", malformedOrbitalStrikes), /STLR_INVALID_DATA:orbital_strikes/);
const unsupportedColonization = structuredClone(fixtures.lobby);
unsupportedColonization.state.rules.colonizable_percent = 100;
await assert.rejects(create(host, "ABCDEJ", "authenticated", unsupportedColonization), /STLR_INVALID_DATA:rules/);
const snapshotWithRemovedField = structuredClone(fixtures.lobby);
snapshotWithRemovedField.removed_field = true;
await assert.rejects(create(host, "ABCDEH", "authenticated", snapshotWithRemovedField), /STLR_INVALID_DATA:persisted object/);
await assert.rejects(rpc(host,
  "select public.stellarion_create_game($1, $2, $3, $4, $5) as result",
  ["ABCDEK", "ABCDEFGHIJKLMNOPQ", hostRecovery, 4, fixtures.lobby]),
  /STLR_INVALID_DATA:display_name/);
const created = await create(host);
const id = created.game.id;
assert.equal(created.membership.player_id, 1);
assert.equal(created.game.status, "lobby");
assert(Number.isSafeInteger(created.game.saved_at) && created.game.saved_at > 0);
assert.equal(created.recovery_code, hostRecovery);
await assert.rejects(create(host), /STLR_CODE_COLLISION/);
assert.deepEqual(await rpc(host, "select public.stellarion_list_games() as result"), []);
await assert.rejects(rpc(host, "select state as result from public.stellarion_games"), /permission denied/);
await assert.rejects(rpc(host, "select recovery_code as result from public.stellarion_game_players"), /permission denied/);
await assert.rejects(rpc(outsider, "select public.stellarion_load_game($1) as result", [id]), /STLR_FORBIDDEN/);
assert.equal((await db.query("select to_regprocedure('public.stellarion_trusted_write(uuid,text,text,bigint,integer)') as obj")).rows[0].obj, null);
for (const signature of [
  "stellarion_create_game(text,text,text,smallint,jsonb)",
  "stellarion_start_game(uuid,bigint,jsonb)",
  "stellarion_create_joint_attack(uuid,jsonb)",
  "stellarion_respond_joint_attack(uuid,bigint,bigint,text,jsonb)",
  "stellarion_cancel_joint_attack(uuid,bigint)",
  "stellarion_load_joint_attacks(uuid)",
  "stellarion_create_trade(uuid,jsonb,boolean)",
  "stellarion_respond_trade(uuid,bigint,bigint,jsonb,text)",
  "stellarion_load_trades(uuid)",
  "stellarion_set_protection_permission(uuid,bigint,bigint,boolean)",
  "stellarion_set_player_color(uuid,smallint)",
  "stellarion_save_game(uuid,bigint,jsonb)",
  "stellarion_submit_turn(uuid,jsonb)",
  "stellarion_withdraw_turn(uuid,bigint,bigint)",
  "stellarion_publish_resolution(uuid,bigint,bigint,jsonb)",
  "stellarion_load_turn_submissions(uuid,bigint,text)",
  "stellarion_sync_game(uuid,bigint,boolean,text)",
]) {
  for (const role of ["anon", "authenticated"]) {
    assert.equal((await db.query("select has_function_privilege($1, $2, 'execute') as allowed",
      [role, `public.${signature}`])).rows[0].allowed, role === "authenticated", `${role}: ${signature}`);
  }
}
for (const signature of [
  "stellarion_submission_ids(jsonb)",
  "stellarion_freeze_joint_attack_launches(uuid,jsonb,bigint,bigint)",
]) {
  for (const role of ["anon", "authenticated"]) {
    assert.equal((await db.query("select has_function_privilege($1, $2, 'execute') as allowed",
      [role, `public.${signature}`])).rows[0].allowed, false, `${role}: ${signature}`);
  }
}
const joined = await rpc(guest, "select public.stellarion_join_game($1, $2, $3) as result",
  ["ABCDEF", "Guest", guestRecovery]);
assert.equal(joined.recovery_code, guestRecovery);
const setColor = (actor, color) => rpc(actor,
  "select public.stellarion_set_player_color($1, $2) as result", [id, color]);
let lobby = joined.game;
await assert.rejects(setColor(host, 6), /STLR_INVALID_DATA:player_color/);
lobby = await setColor(host, 2);
assert.equal(lobby.persisted.state.players[0].color, 2);
const winningRevision = lobby.revision;
const losingClaim = await setColor(guest, 2);
assert.equal(losingClaim.revision, winningRevision);
assert.equal(losingClaim.persisted.state.players[0].color, 2);
assert.equal(losingClaim.persisted.state.players[1].color, 1);
lobby = await setColor(host, 0);
// Snapshot writes acknowledge server metadata without echoing the uploaded state.
// Reconstruct as the client does, then compare with an independent canonical load.
const snapshotWrite = async (actor, query, args) => {
  const metadata = await rpc(actor, query, args);
  assert(!("persisted" in metadata), "snapshot uploads must not echo their large payload");
  const reconstructed = { ...metadata, persisted: args.at(-1) };
  const loaded = await rpc(actor, "select public.stellarion_load_game($1) as result", [args[0]]);
  assert.deepEqual(reconstructed, loaded);
  if (query.includes("publish_resolution")) {
    console.log(`Resolution acknowledgement: ${Buffer.byteLength(JSON.stringify(metadata))} JSON bytes versus ${Buffer.byteLength(JSON.stringify(loaded))} bytes with the fixture snapshot; excludes HTTP/compression.`);
  }
  return reconstructed;
};
const start = (actor, persisted = fixtures.active, revision = lobby.revision) => snapshotWrite(actor,
  "select public.stellarion_start_game($1, $2, $3) as result", [id, revision, persisted]);
await assert.rejects(start(guest), /STLR_INVALID_STATUS/);
await assert.rejects(start(host, fixtures.active, lobby.revision - 1), /STLR_CONFLICT/);
const changedRules = structuredClone(fixtures.active);
changedRules.state.rules.moons_percent = 30;
await assert.rejects(start(host, changedRules), /STLR_FORBIDDEN/);
let active = await start(host);
assert.equal(active.status, "active");
assert.equal(active.max_players, 2);
assert.equal(active.members.length, 2);
const twoPlayerJointAttack = {
  id: 6999,
  revision: 0,
  turn: active.persisted.state.turn,
  inviter: 1,
  destination: active.persisted.state.players[1].home_planet,
  objective: "Attack",
  bombing: "None",
  combat_probes: false,
  canceled: false,
  launched: false,
  participants: [
    {
      player_id: 1,
      response: "accepted",
      contribution: {
        player_id: 1,
        origin: active.persisted.state.players[0].home_planet,
        army: { "Ship(LightFighter)": 1 },
        bombing: "None",
        combat_probes: false,
      },
    },
    { player_id: 2, response: "pending", contribution: null },
  ],
};
await assert.rejects(rpc(host,
  "select public.stellarion_create_joint_attack($1, $2) as result",
  [id, twoPlayerJointAttack]), /STLR_INVALID_DATA:joint_attack/,
  "two active players cannot create a joint attack");
assert.deepEqual(await rpc(host,
  "select public.stellarion_load_joint_attacks($1) as result", [id]), []);
await assert.rejects(rpc(host,
  "select public.stellarion_set_protection_permission($1, $2, $3, $4) as result",
  [id, active.persisted.state.players[0].home_planet, 2, true]),
  /STLR_INVALID_DATA:protection_player/,
  "two-player games never enable protection");
const startedSavedAt = active.saved_at;
// Both draft-saving and readiness must reject the same malformed submission envelopes.
for (const invalidSubmission of [
  null,
  { player_id: 1, turn: 1, generation: 0, commands: [], extra: true },
  { player_id: 1, turn: 1, generation: 0, commands: {} },
  { player_id: 1, turn: 1, generation: 0, commands: Array(1025).fill({}) },
  { player_id: 0, turn: 1, generation: 0, commands: [] },
  { player_id: 1, turn: 1, generation: -1, commands: [] },
]) {
  await assert.rejects(rpc(host, "select public.stellarion_save_game($1, $2, $3) as result",
    [id, active.revision, invalidSubmission]), /STLR_INVALID_DATA:submission/);
  await assert.rejects(rpc(host, "select public.stellarion_submit_turn($1, $2) as result",
    [id, invalidSubmission]), /STLR_INVALID_DATA:submission/);
}
const summaries = await rpc(host, "select public.stellarion_list_games() as result");
assert.equal(summaries.length, 1);
assert.equal(summaries[0].saved_at, startedSavedAt);
const save = (actor, record, player, turn = 1, commands = [], generation = 0) => rpc(actor,
  "select public.stellarion_save_game($1, $2, $3) as result",
  [id, record.revision, { player_id: player, turn, commands, generation }]);
await assert.rejects(save(guest, active, 1), /STLR_FORBIDDEN/);
await assert.rejects(save(outsider, active, 1), /STLR_FORBIDDEN/);
await assert.rejects(save(host, active, 1, 2), /STLR_STALE_SUBMISSION/);
await assert.rejects(save(host, { ...active, revision: active.revision - 1 }, 1), /STLR_CONFLICT/);
await assert.rejects(rpc(host,
  "select public.stellarion_save_game($1, $2, $3) as result",
  [id, active.revision, { player_id: 1, turn: 1, commands: [] }]), /STLR_INVALID_DATA:submission/);
const acknowledgement = await save(host, active, 1);
assert.equal(acknowledgement.revision, active.revision);
active = { ...active, ...acknowledgement };
assert(active.saved_at >= startedSavedAt);
const savedDrafts = await rpc(host,
  "select public.stellarion_load_turn_submissions($1, $2) as result", [id, 1]);
const scopedOrders = (actor, scope, turn = 1) => rpc(actor,
  "select public.stellarion_load_turn_submissions($1, $2, $3) as result", [id, turn, scope]);
assert.deepEqual(await scopedOrders(host, "mine"), savedDrafts);
assert.deepEqual(await scopedOrders(guest, "mine"), []);
assert.deepEqual(await scopedOrders(host, "resolution"), []);
await assert.rejects(scopedOrders(outsider, "resolution"), /STLR_FORBIDDEN/);
await assert.rejects(scopedOrders(null, "mine"), /STLR_UNAUTHENTICATED/);
await assert.rejects(scopedOrders(host, "invalid"), /STLR_INVALID_DATA:submission_scope/);
await assert.rejects(scopedOrders(host, null), /STLR_INVALID_DATA:submission_scope/);
assert.equal(savedDrafts.length, 1);
assert.equal(savedDrafts[0].ready, false);
assert.deepEqual(savedDrafts[0].submission.commands, []);
const submit = (actor, player, turn = 1, commands = [], generation = 0) => rpc(actor,
  "select public.stellarion_submit_turn($1, $2) as result",
  [id, { player_id: player, turn, commands, generation }]);
const withdraw = (actor, turn = 1, generation = 0) => rpc(actor,
  "select public.stellarion_withdraw_turn($1, $2, $3) as result", [id, turn, generation]);
const publish = (actor, persisted = fixtures.resolved, revision = active.revision) => snapshotWrite(actor,
  "select public.stellarion_publish_resolution($1, $2, $3, $4) as result",
  [id, revision, 1, persisted]);
await assert.rejects(submit(host, 2), /STLR_FORBIDDEN/);
await assert.rejects(submit(outsider, 1), /STLR_FORBIDDEN/);
await assert.rejects(submit(host, 1, 2), /STLR_STALE_SUBMISSION/);
await assert.rejects(rpc(host,
  "select public.stellarion_submit_turn($1, $2) as result",
  [id, { player_id: 1, turn: 1, commands: [] }]), /STLR_INVALID_DATA:submission/);
await assert.rejects(publish(host), /STLR_TURN_INCOMPLETE/);
assert.equal((await submit(host, 1)).disposition, "inserted");
assert.equal((await submit(host, 1)).disposition, "duplicate");
assert.deepEqual(await scopedOrders(host, "resolution"), [], "do not resend orders while waiting");
await assert.rejects(submit(host, 1, 1, [{}]), /STLR_DUPLICATE_SUBMISSION/);
await assert.rejects(publish(host), /STLR_TURN_INCOMPLETE/);
await assert.rejects(withdraw(outsider), /STLR_FORBIDDEN/);
await assert.rejects(withdraw(null), /STLR_UNAUTHENTICATED/);
await assert.rejects(withdraw(host, 2), /STLR_STALE_SUBMISSION/);
const draft = await withdraw(host);
assert.equal(draft.generation, 1);
assert.deepEqual(await withdraw(host), draft, "withdrawal retry preserves the draft");
assert.deepEqual((await rpc(host, "select public.stellarion_load_game($1) as result", [id])).submitted_players, []);
await assert.rejects(submit(host, 1), /STLR_DUPLICATE_SUBMISSION/, "late ready cannot undo Continue turn");
assert.equal((await submit(guest, 2)).disposition, "inserted");
assert.deepEqual(await scopedOrders(guest, "resolution"), [], "a withdrawn draft is incomplete");
await assert.rejects(publish(guest), /STLR_TURN_INCOMPLETE/, "withdrawn orders cannot resolve");
assert.equal((await submit(host, 1, 1, [], draft.generation)).disposition, "inserted");
assert.equal((await submit(host, 1, 1, [], draft.generation)).disposition, "duplicate");
await assert.rejects(withdraw(host, 1, draft.generation), /STLR_TURN_COMMITTED/);
await assert.rejects(withdraw(guest), /STLR_TURN_COMMITTED/);
await assert.rejects(withdraw(host), /STLR_DUPLICATE_SUBMISSION/, "late withdrawal cannot undo a newer ready");
const submissions = await rpc(host, "select public.stellarion_load_turn_submissions($1, $2) as result", [id, 1]);
assert.deepEqual(submissions.map(s => s.submission.player_id), [1, 2]);
assert.deepEqual(await scopedOrders(host, "resolution"), submissions);
await db.query(`update public.stellarion_games
  set state = jsonb_set(state, '{state,players,1,spectator}', 'true') where id = $1`, [id]);
assert.deepEqual((await scopedOrders(host, "resolution")).map(s => s.submission.player_id), [1],
  "spectator orders must not enter turn resolution");
await db.query(`update public.stellarion_games
  set state = jsonb_set(state, '{state,players,1,spectator}', 'false') where id = $1`, [id]);
assert.deepEqual((await scopedOrders(guest, "mine")).map(s => s.submission.player_id), [2]);
const fullOrderBytes = Buffer.byteLength(JSON.stringify(submissions));
console.log(`Incomplete resolver response: 2 JSON bytes instead of repeated order payloads (${fullOrderBytes} bytes for the two-player empty-order fixture); excludes HTTP/compression.`);
const reseeded = structuredClone(fixtures.resolved);
reseeded.state.rng.seed[0] += 1;
await assert.rejects(publish(host, reseeded), /STLR_FORBIDDEN/);
await assert.rejects(publish(outsider), /STLR_FORBIDDEN/);
const resolved = await publish(host);
assert.equal(resolved.persisted.state.turn, 2);
assert.equal(resolved.revision, active.revision + 1);
assert(resolved.saved_at >= active.saved_at);
assert.deepEqual(resolved.submitted_players, []);
assert.deepEqual(await scopedOrders(host, "resolution"), [], "do not send already-resolved orders");
assert.deepEqual(await scopedOrders(host, "all"), [], "resolved commands are no longer stored");
assert.equal((await db.query(
  "select count(*)::int as n from public.stellarion_turn_submissions where game_id = $1", [id],
)).rows[0].n, 0);
await assert.rejects(submit(host, 1), /STLR_STALE_SUBMISSION/);
await assert.rejects(publish(guest), /STLR_CONFLICT/);
const events = await rpc(guest, "select public.stellarion_events_since($1, $2) as result", [id, 0]);
assert(events.events.some(e => e.kind === "turn_resolved"));
assert(events.events.some(e => e.kind === "turn_withdrawn"));
const sync = (actor, cursor, renew, token = null, gameId = id) => rpc(actor,
  "select public.stellarion_sync_game($1, $2, $3, $4) as result", [gameId, cursor, renew, token]);
await sync(guest, 0, true);
const firstSync = await sync(host, 0, true);
assert.equal(firstSync.members.length, 2);
assert.equal(firstSync.roster_token.length, 64);
assert(!JSON.stringify(firstSync).includes("recovery_code"));
assert(!JSON.stringify(firstSync).includes("persisted"));
const quietSync = await sync(host, firstSync.batch.cursor, true, firstSync.roster_token);
assert.equal(quietSync.members, null, "heartbeats must not resend an unchanged roster");
assert.deepEqual(quietSync.batch.events, [], "heartbeats must not create durable events");
assert.equal(quietSync.roster_token, firstSync.roster_token);
const seenAt = async () => (await db.query(
  "select last_seen_at::text as value from public.stellarion_game_players where game_id = $1 and user_id = $2",
  [id, host],
)).rows[0].value;
const beforeSyncRead = await seenAt();
await sync(host, quietSync.batch.cursor, false, quietSync.roster_token);
assert.equal(await seenAt(), beforeSyncRead, "event-only reads do not write presence");
await db.query(
  "update public.stellarion_game_players set last_seen_at = clock_timestamp() - interval '16 seconds' where game_id = $1 and user_id = $2",
  [id, guest],
);
const expiredSync = await sync(host, quietSync.batch.cursor, false, quietSync.roster_token);
assert.equal(expiredSync.members.find(member => member.user_id === guest).connected, false);
assert.notEqual(expiredSync.roster_token, quietSync.roster_token, "lease expiry changes the roster token without an event");
await sync(guest, expiredSync.batch.cursor, true);
for (const renew of [true, false]) {
  await assert.rejects(sync(outsider, 0, renew, firstSync.roster_token), /STLR_FORBIDDEN/);
  await assert.rejects(sync(null, 0, renew), /STLR_UNAUTHENTICATED/);
}
await assert.rejects(sync(host, -1, true), /STLR_INVALID_DATA:event_cursor/);
await assert.rejects(sync(host, 0, null), /STLR_INVALID_DATA:connected/);
assert.equal((await rpc(host, "select public.stellarion_load_game($1) as result", [id])).saved_at, resolved.saved_at);
const oldQuietBytes = Buffer.byteLength(JSON.stringify({ok: true, members: firstSync.members}))
  + Buffer.byteLength(JSON.stringify({events: [], cursor: quietSync.batch.cursor}));
const quietSyncBytes = Buffer.byteLength(JSON.stringify(quietSync));
assert(quietSyncBytes < oldQuietBytes / 2);
console.log(`Quiet two-player sync: ${quietSyncBytes} JSON bytes vs ${oldQuietBytes} for separate roster/event responses; excludes HTTP/compression. Resolved command rows: 0.`);
const temporary = await create(host, "XYZABC");
await rpc(guest, "select public.stellarion_join_game($1, $2, $3) as result",
  ["XYZABC", "Guest", "CDEF-0123-4567-89AB"]);
await db.query(`select public.stellarion_emit_event($1, 'trade_changed', 1, 1)
  from generate_series(1, 2050)`, [temporary.game.id]);
const missedPrivate = await sync(guest, 0, false, null, temporary.game.id);
assert.deepEqual(missedPrivate.batch.events, [], "sync preserves private event filtering");
assert(missedPrivate.batch.resync_required, "pruned cursors must request canonical reload");
assert(missedPrivate.batch.cursor >= 256, "hidden events still advance the replay cursor");
assert.equal((await db.query(
  "select count(*)::int as n from public.stellarion_game_events where game_id = $1", [temporary.game.id],
)).rows[0].n, 2048);
const deletedLobbyPresence = await rpc(host,
  "select public.stellarion_set_connected($1, false) as result", [temporary.game.id]);
assert.deepEqual(deletedLobbyPresence, { ok: true, members: [] });
await assert.rejects(rpc(guest, "select public.stellarion_load_game($1) as result", [temporary.game.id]), /STLR_GAME_NOT_FOUND/);
await rpc(host, "select public.stellarion_set_connected($1, false) as result", [id]);
const disconnected = await rpc(guest, "select public.stellarion_load_game($1) as result", [id]);
assert.equal(disconnected.status, "active");
assert.equal(disconnected.saved_at, resolved.saved_at, "presence is not a gameplay save");
// Hosts and guests share the same short presence lease. Reads and event polls
// must expose expiry without renewing it or deleting an active match.
for (const [departed, observer] of [[host, guest], [guest, host]]) {
  for (const actor of [host, guest]) {
    const heartbeat = await rpc(actor,
      "select public.stellarion_set_connected($1, true) as result", [id]);
    assert.equal(heartbeat.ok, true);
    assert.equal(heartbeat.members.length, 2);
    assert(!("persisted" in heartbeat), "presence must not return the gameplay snapshot");
  }
  const before = await rpc(observer, "select public.stellarion_events_since($1, $2) as result", [id, 0]);
  await db.query(
    "update public.stellarion_game_players set last_seen_at = clock_timestamp() - interval '14 seconds' where game_id = $1 and user_id = $2",
    [id, departed],
  );
  const live = await rpc(observer, "select public.stellarion_load_game($1) as result", [id]);
  assert.equal(live.saved_at, resolved.saved_at);
  assert(live.members.find(member => member.user_id === departed).connected);
  await db.query(
    "update public.stellarion_game_players set last_seen_at = clock_timestamp() - interval '15 seconds' where game_id = $1 and user_id = $2",
    [id, departed],
  );
  for (let refresh = 0; refresh < 2; refresh += 1) {
    const expired = await rpc(observer, "select public.stellarion_load_game($1) as result", [id]);
    assert.equal(expired.members.find(member => member.user_id === departed).connected, false);
    assert(expired.members.find(member => member.user_id === observer).connected);
    assert.equal(expired.status, "active");
    assert.equal(expired.revision, live.revision);
    assert.equal(expired.saved_at, resolved.saved_at);
    const quiet = await rpc(observer, "select public.stellarion_events_since($1, $2) as result", [id, before.cursor]);
    assert.deepEqual(quiet.events, []);
  }
  await assert.rejects(
    rpc(host, "select public.stellarion_resume_game($1) as result", [id]),
    /STLR_INVALID_STATUS/,
  );
  await rpc(departed, "select public.stellarion_set_connected($1, true) as result", [id]);
  const reconnected = await rpc(observer, "select public.stellarion_load_game($1) as result", [id]);
  assert(reconnected.members.every(member => member.connected));
}
await rpc(guest, "select public.stellarion_set_connected($1, false) as result", [id]);
const recovered = await rpc(
  outsider,
  "select public.stellarion_recover_player($1, $2) as result",
  ["ABCDEF", guestRecovery],
);
assert.equal(recovered.membership.user_id, outsider);
assert.equal(recovered.recovery_code, guestRecovery);
await rpc(outsider, "select public.stellarion_set_connected($1, false) as result", [id]);
const recoveredAgain = await rpc(
  guest,
  "select public.stellarion_recover_player($1, $2) as result",
  ["ABCDEF", guestRecovery],
);
assert.equal(recoveredAgain.membership.user_id, guest);
assert.equal(recoveredAgain.recovery_code, guestRecovery);

// Submitted trade offers bind acceptance to their current revision.
const tradeLobby = await create(host, "TRADE1");
const joinedTrade = (await rpc(guest,
  "select public.stellarion_join_game($1, $2, $3) as result",
  ["TRADE1", "Trader", "4444-4444-4444-4444"])).game;
const tradeSnapshot = structuredClone(fixtures.active);
const tradeHomes = tradeSnapshot.state.players.map(player => player.home_planet);
for (const [index, home] of tradeHomes.entries()) {
  const planet = tradeSnapshot.state.map.planets.find(planet => planet.id === home);
  // Level-five posts must negotiate and finalize across their full 7.5 AU range.
  planet.position = [index * 750, 0];
  planet.army.controller["Building(TradingPost)"] = index === 0 ? 0 : 5;
  tradeSnapshot.state.players[index].resources = { metal: 10000, crystal: 10000, deuterium: 10000 };
}
const tradeGame = await snapshotWrite(host,
  "select public.stellarion_start_game($1, $2, $3) as result",
  [tradeLobby.game.id, joinedTrade.revision, tradeSnapshot]);
const tradeResources = (metal = 0, crystal = 0, deuterium = 0) => ({ metal, crystal, deuterium });
let tradeDraft = {
  id: 9901, revision: 0, turn: 1, proposer: 1, canceled: false, finalized: false,
  participants: [
    { player_id: 1, planet_id: tradeHomes[0], resources: tradeResources(2000), response: "accepted" },
    { player_id: 2, planet_id: tradeHomes[1], resources: tradeResources(), response: "pending" },
  ],
};
await assert.rejects(
  rpc(host, "select public.stellarion_create_trade($1, $2, false) as result", [tradeGame.id, tradeDraft]),
  /STLR_INVALID_DATA:trade/,
  "an unsaved Trading Post route must be declared as projected",
);
tradeDraft = await rpc(host, "select public.stellarion_create_trade($1, $2, true) as result", [tradeGame.id, tradeDraft]);
const projectedTradeGame = await rpc(host, "select public.stellarion_load_game($1) as result", [tradeGame.id]);
assert.equal(
  projectedTradeGame.persisted.state.map.planets.find(planet => planet.id === tradeHomes[0])
    .army.controller["Building(TradingPost)"],
  5,
  "creating a projected route commits the proposer's completed Trading Post",
);
const respondTrade = (actor, revision, resources, response) => rpc(actor,
  "select public.stellarion_respond_trade($1, $2, $3, $4, $5) as result",
  [tradeGame.id, tradeDraft.id, revision, resources, response]);
tradeDraft = await respondTrade(host, tradeDraft.revision, tradeResources(2000), "pending");
assert.equal(tradeDraft.revision, 0, "withdrawing consent keeps the resource revision");
assert.equal(tradeDraft.participants[0].response, "pending");
tradeDraft = await respondTrade(host, tradeDraft.revision, tradeResources(2000), "accepted");
assert.equal(tradeDraft.finalized, false);
for (const [actor, playerId, resources] of [
  [guest, 2, tradeResources(0, 200)],
  [host, 1, tradeResources(2250)],
  [guest, 2, tradeResources()],
  [guest, 2, tradeResources(0, 250)],
]) {
  const otherId = playerId === 1 ? 2 : 1;
  const other = playerId === 1 ? guest : host;
  const otherResources = tradeDraft.participants.find(item => item.player_id === otherId).resources;
  tradeDraft = await respondTrade(other, tradeDraft.revision, otherResources, "accepted");
  assert.equal(tradeDraft.finalized, false);
  const previousRevision = tradeDraft.revision;
  tradeDraft = await respondTrade(actor, previousRevision, resources, "pending");
  assert.equal(tradeDraft.revision, previousRevision + 1);
  assert(tradeDraft.participants.every(item => item.response === "pending"));
  for (const viewer of [host, guest]) {
    assert.deepEqual(await rpc(viewer, "select public.stellarion_load_trades($1) as result", [tradeGame.id]), [tradeDraft]);
  }
  await assert.rejects(respondTrade(other, previousRevision, otherResources, "accepted"), /STLR_FORBIDDEN/);
}
await assert.rejects(respondTrade(outsider, tradeDraft.revision, tradeResources(1), "pending"), /STLR_FORBIDDEN/);
await assert.rejects(respondTrade(host, tradeDraft.revision, tradeResources(2600), "accepted"), /STLR_INVALID_DATA:trade_resources/);
tradeDraft = await respondTrade(host, tradeDraft.revision, tradeResources(2250), "accepted");
assert.equal(tradeDraft.finalized, false);
tradeDraft = await respondTrade(guest, tradeDraft.revision, tradeResources(0, 250), "accepted");
assert.equal(tradeDraft.finalized, true);
const tradedGame = await rpc(host, "select public.stellarion_load_game($1) as result", [tradeGame.id]);
assert.equal(tradedGame.persisted.state.trades.length, 1);
assert.deepEqual(tradedGame.persisted.state.trades[0].parties.map(party => party.resources),
  [tradeResources(2250), tradeResources(0, 250)], "both current offers are reserved for settlement");
await assert.rejects(respondTrade(host, tradeDraft.revision, tradeResources(), "pending"), /STLR_FORBIDDEN/);
console.log("Submitted bilateral trade offers, consent withdrawal, stale acceptance, and finalization passed.");

// Protection access is an immediate compact patch in matches with at least three active players.
const protectionLobby = await create(host, "PRTCT1");
let protectionGame = (await rpc(guest,
  "select public.stellarion_join_game($1, $2, $3) as result",
  ["PRTCT1", "Protector", "2222-2222-2222-2222"])).game;
protectionGame = (await rpc(outsider,
  "select public.stellarion_join_game($1, $2, $3) as result",
  ["PRTCT1", "Third", "3333-3333-3333-3333"])).game;
protectionGame = await snapshotWrite(host,
  "select public.stellarion_start_game($1, $2, $3) as result",
  [protectionLobby.game.id, protectionGame.revision, fixtures.active_three]);
const protectedPlanet = protectionGame.persisted.state.players[0].home_planet;
const protectorHome = protectionGame.persisted.state.players[1].home_planet;
const thirdHome = protectionGame.persisted.state.players[2].home_planet;

// Joint-attack coordination stores only the invitation, is visible only to its participants,
// and targets wake-up events to those participants while advancing every member's cursor.
const jointAttack = {
  revision: 0,
  launched: false,
  id: 7001,
  turn: protectionGame.persisted.state.turn,
  inviter: 1,
  destination: thirdHome,
  objective: "Attack",
  bombing: "None",
  combat_probes: false,
  canceled: false,
  participants: [
    {
      player_id: 1,
      response: "accepted",
      contribution: { player_id: 1, origin: protectedPlanet, army: { "Ship(LightFighter)": 1 }, bombing: "None", combat_probes: false },
    },
    { player_id: 2, response: "pending", contribution: null },
  ],
};
const createJointAttack = (actor, invitation = jointAttack) => rpc(actor,
  "select public.stellarion_create_joint_attack($1, $2) as result",
  [protectionGame.id, invitation]);
const respondJointAttack = (actor, attackId, response, contribution = null, revision = 0) => rpc(actor,
  "select public.stellarion_respond_joint_attack($1, $2, $3, $4, $5) as result",
  [protectionGame.id, attackId, revision, response, contribution]);
const cancelJointAttack = (actor, attackId) => rpc(actor,
  "select public.stellarion_cancel_joint_attack($1, $2) as result",
  [protectionGame.id, attackId]);
const loadJointAttacks = actor => rpc(actor,
  "select public.stellarion_load_joint_attacks($1) as result", [protectionGame.id]);
await assert.rejects(createJointAttack(guest), /STLR_INVALID_DATA:joint_attack/);
const emptyJointAttack = structuredClone(jointAttack);
emptyJointAttack.participants[0].contribution.army = {};
await assert.rejects(createJointAttack(host, emptyJointAttack),
  /STLR_INVALID_DATA:joint_attack/, "the inviter must select ships before proposing");
emptyJointAttack.participants[0].contribution.army = { "Ship(LightFighter)": 0 };
await assert.rejects(createJointAttack(host, emptyJointAttack),
  /STLR_INVALID_DATA:joint_attack/, "zero-count ships are still an empty proposal");
emptyJointAttack.participants[0].contribution.army = { "Building(TradingPost)": 1 };
await assert.rejects(createJointAttack(host, emptyJointAttack),
  /STLR_INVALID_DATA:joint_attack/, "a proposal needs ships, not another unit type");
assert.equal((await createJointAttack(host)).id, jointAttack.id);
const guestRouteEdit = structuredClone(jointAttack);
guestRouteEdit.objective = "Destroy";
guestRouteEdit.participants[1].response = "accepted";
guestRouteEdit.participants[1].contribution = {
  player_id: 2, origin: protectorHome, army: { "Ship(LightFighter)": 1 },
  bombing: "None", combat_probes: false,
};
await assert.rejects(createJointAttack(guest, guestRouteEdit),
  /STLR_INVALID_DATA:joint_attack/, "only the inviter may revise the shared route");
const removedInvitee = structuredClone(jointAttack);
removedInvitee.participants[1].player_id = 3;
await assert.rejects(createJointAttack(host, removedInvitee),
  /STLR_INVALID_DATA:joint_attack_invitees/, "an invitee remains until mission cancellation");
assert.equal((await loadJointAttacks(host)).length, 1);
assert.equal((await loadJointAttacks(guest)).length, 1);
assert.deepEqual(await loadJointAttacks(outsider), []);
const outsiderEvents = await rpc(outsider,
  "select public.stellarion_events_since($1, $2) as result", [protectionGame.id, 0]);
assert(!outsiderEvents.events.some(event => event.kind === "joint_attack_changed"));
assert(outsiderEvents.cursor > 0, "private events must not stall an uninvolved member's cursor");
const outsiderRealtimeKinds = await rpc(outsider,
  "select coalesce(jsonb_agg(kind), '[]'::jsonb) as result from public.stellarion_game_events where game_id = $1",
  [protectionGame.id]);
assert(!outsiderRealtimeKinds.includes("joint_attack_changed"),
  "Realtime table visibility must hide private joint-attack wake-ups");
const guestContribution = {
  player_id: 2,
  origin: protectorHome,
  army: { "Ship(LightFighter)": 1 },
  bombing: "None",
  combat_probes: true,
};
const conflictingGuestBombing = structuredClone(guestContribution);
conflictingGuestBombing.bombing = "Industrial";
await assert.rejects(
  respondJointAttack(guest, jointAttack.id, "pending", conflictingGuestBombing),
  /STLR_INVALID_DATA:joint_attack_contribution/,
  "the inviter's bombing objective applies to every allied fleet",
);
// A protector blocks acceptance only while access remains active. After revocation it stays in
// the snapshot until turn resolution returns it home, but can attack its former host.
const revokedTargetState = structuredClone(protectionGame.persisted);
const revokedTarget = revokedTargetState.state.map.planets.find(planet => planet.id === thirdHome);
revokedTarget.army.protectors["2"] = { "Ship(LightFighter)": 1 };
revokedTarget.protection_permissions.push(2);
revokedTargetState.state.players[1].protection_intel[thirdHome] = 3;
await db.query("update public.stellarion_games set state = $2 where id = $1",
  [protectionGame.id, revokedTargetState]);
await assert.rejects(respondJointAttack(guest, jointAttack.id, "accepted", guestContribution),
  /STLR_FORBIDDEN/, "an active stationed protector cannot attack its host");
revokedTarget.protection_permissions = [];
await db.query("update public.stellarion_games set state = $2 where id = $1",
  [protectionGame.id, revokedTargetState]);
let updatedJointAttack = await respondJointAttack(guest, jointAttack.id, "accepted", guestContribution);
assert.equal(updatedJointAttack.participants[1].response, "accepted");
assert.deepEqual(updatedJointAttack.participants[1].contribution, guestContribution);
await db.query("update public.stellarion_games set state = $2 where id = $1",
  [protectionGame.id, protectionGame.persisted]);
const jointCommand = {
  kind: "send_joint_mission",
  attack_id: jointAttack.id,
  mission_id: 8001,
  destination: jointAttack.destination,
  objective: jointAttack.objective,
  bombing: jointAttack.bombing,
  combat_probes: jointAttack.combat_probes,
  contributions: [jointAttack.participants[0].contribution, guestContribution],
};
const saveJointDraft = commands => rpc(host,
  "select public.stellarion_save_game($1, $2, $3) as result",
  [protectionGame.id, protectionGame.revision, {
    player_id: 1,
    turn: protectionGame.persisted.state.turn,
    commands,
    generation: 0,
  }]);
const spoofedJointCommand = structuredClone(jointCommand);
spoofedJointCommand.contributions.pop();
await assert.rejects(saveJointDraft([spoofedJointCommand]), /STLR_INVALID_DATA:joint_attack/);
assert.equal((await saveJointDraft([jointCommand])).revision, protectionGame.revision);
await assert.rejects(cancelJointAttack(host, jointAttack.id), /STLR_FORBIDDEN/,
  "an allied attack cannot be canceled after its launch command is saved");
await db.query(
  "delete from public.stellarion_turn_submissions where game_id = $1 and turn = $2 and player_id = 1",
  [protectionGame.id, protectionGame.persisted.state.turn],
);
await assert.rejects(respondJointAttack(guest, jointAttack.id, "rejected"), /STLR_FORBIDDEN/,
  "an accepted contribution is final");
await assert.rejects(respondJointAttack(host, jointAttack.id, "accepted", guestContribution), /STLR_FORBIDDEN/);

const rejectedAttack = structuredClone(jointAttack);
rejectedAttack.id = 7002;
await createJointAttack(host, rejectedAttack);
updatedJointAttack = await respondJointAttack(guest, rejectedAttack.id, "rejected");
assert.equal(updatedJointAttack.participants[1].response, "rejected");
assert.equal(updatedJointAttack.participants[1].contribution, null);

const canceledAttack = structuredClone(jointAttack);
canceledAttack.id = 7004;
await createJointAttack(host, canceledAttack);
const acceptedCanceledAttack = await respondJointAttack(
  guest, canceledAttack.id, "accepted", guestContribution);
await assert.rejects(cancelJointAttack(guest, canceledAttack.id), /STLR_FORBIDDEN/);
updatedJointAttack = await cancelJointAttack(host, canceledAttack.id);
assert.equal(updatedJointAttack.canceled, true);
assert((await loadJointAttacks(guest)).some(invitation =>
  invitation.id === canceledAttack.id && invitation.canceled));
await assert.rejects(respondJointAttack(guest, canceledAttack.id, "rejected"), /STLR_FORBIDDEN/);
const canceledJointCommand = structuredClone(jointCommand);
canceledJointCommand.attack_id = canceledAttack.id;
canceledJointCommand.mission_id = 8002;
canceledJointCommand.contributions = acceptedCanceledAttack.participants
  .map(participant => participant.contribution)
  .filter(Boolean);
await assert.rejects(saveJointDraft([canceledJointCommand]), /STLR_INVALID_DATA:joint_attack/);

const ownTargetAttack = structuredClone(jointAttack);
ownTargetAttack.id = 7003;
ownTargetAttack.participants[1] = { player_id: 3, response: "pending", contribution: null };
await createJointAttack(host, ownTargetAttack);
await assert.rejects(respondJointAttack(outsider, ownTargetAttack.id, "accepted", {
  ...guestContribution,
  player_id: 3,
  origin: thirdHome,
  army: { "Ship(LightFighter)": 1 },
}), /STLR_FORBIDDEN/, "a target owner may see an unknown-intel invite but cannot accept it");
// Submitted drafts, editable owner proposals, reversible acceptance, and a launch with an unanswered guest.
const planningAttack = structuredClone(jointAttack);
planningAttack.id = 7010;
planningAttack.participants.push({ player_id: 3, response: "pending", contribution: null });
await createJointAttack(host, planningAttack);
let livePlanning = await respondJointAttack(guest, planningAttack.id, "pending", guestContribution);
planningAttack.revision = livePlanning.revision;
for (const viewer of [host, outsider]) {
  const live = (await loadJointAttacks(viewer)).find(item => item.id === planningAttack.id);
  assert.deepEqual(live.participants[1].contribution, guestContribution);
  assert.equal(live.participants[1].response, "pending");
}
await respondJointAttack(guest, planningAttack.id, "accepted", guestContribution, planningAttack.revision);
assert.equal((await respondJointAttack(guest, planningAttack.id, "pending", guestContribution, planningAttack.revision))
  .participants[1].response, "pending", "acceptance can be undone before launch");
await respondJointAttack(guest, planningAttack.id, "accepted", guestContribution, planningAttack.revision);
planningAttack.bombing = "Industrial";
planningAttack.participants[0].contribution.bombing = "Industrial";
let revisedBombing = await createJointAttack(host, planningAttack);
assert.equal(revisedBombing.participants[1].response, "pending");
assert.equal(revisedBombing.participants[1].contribution.bombing, "Industrial",
  "an inviter bombing change rewrites every saved fleet to the shared objective");
planningAttack.revision = revisedBombing.revision;
planningAttack.bombing = "None";
planningAttack.participants[0].contribution.bombing = "None";
revisedBombing = await createJointAttack(host, planningAttack);
assert.equal(revisedBombing.participants[1].contribution.bombing, "None");
planningAttack.revision = revisedBombing.revision;
planningAttack.objective = "Destroy";
const revisedPlanning = await createJointAttack(host, planningAttack);
assert.equal(revisedPlanning.revision, planningAttack.revision + 1);
assert.equal(revisedPlanning.participants[1].response, "pending");
assert.deepEqual(revisedPlanning.participants[1].contribution, guestContribution);
await assert.rejects(respondJointAttack(guest, planningAttack.id, "accepted", guestContribution, 0),
  /STLR_FORBIDDEN/, "a stale screen cannot accept a changed mission");
planningAttack.revision = revisedPlanning.revision;
planningAttack.objective = "Attack";
livePlanning = await createJointAttack(host, planningAttack);
const finalPlanning = await respondJointAttack(guest, planningAttack.id, "accepted", guestContribution, livePlanning.revision);
assert.equal(finalPlanning.participants[2].response, "pending");
const plannedLaunch = { ...jointCommand, attack_id: planningAttack.id, mission_id: 8010,
  contributions: finalPlanning.participants.filter(item => item.player_id === 1 || item.response === "accepted")
    .map(item => item.contribution) };
const tamperedOrders = structuredClone(plannedLaunch);
tamperedOrders.contributions[1].combat_probes = false;
await assert.rejects(saveJointDraft([tamperedOrders]), /STLR_INVALID_DATA:joint_attack/);
await saveJointDraft([plannedLaunch]);
assert((await loadJointAttacks(guest)).find(item => item.id === planningAttack.id).launched);
await assert.rejects(respondJointAttack(guest, planningAttack.id, "pending", guestContribution, finalPlanning.revision),
  /STLR_FORBIDDEN/, "launch freezes the accepted roster");
await assert.rejects(respondJointAttack(outsider, planningAttack.id, "accepted",
  { ...guestContribution, player_id: 3, origin: thirdHome }, finalPlanning.revision), /STLR_FORBIDDEN/);
await assert.rejects(createJointAttack(host, { ...planningAttack, revision: finalPlanning.revision }), /STLR_FORBIDDEN/);
console.log("Live allied planning, shared bombing, per-fleet probe orders, proposal revisions, undo acceptance, and early launch passed.");

// All participants may revise an accepted offer; consent belongs to the other offers.
const editableAttack = structuredClone(jointAttack);
editableAttack.id = 7011;
editableAttack.destination = protectionGame.persisted.state.map.planets.find(planet =>
  planet.owned == null && planet.controlled == null && !planet.is_destroyed).id;
editableAttack.participants.push({ player_id: 3, response: "pending", contribution: null });
let editable = await createJointAttack(host, editableAttack);
editable = await respondJointAttack(guest, editable.id, "accepted", guestContribution, editable.revision);
assert.equal(editable.participants[0].response, "pending", "guest edits invalidate host consent");
const thirdContribution = { ...guestContribution, player_id: 3, origin: thirdHome };
editable = await respondJointAttack(outsider, editable.id, "accepted", thirdContribution, editable.revision);
assert.equal(editable.participants[1].response, "pending");
editable = await respondJointAttack(guest, editable.id, "accepted", guestContribution, editable.revision);
const staleRevision = editable.revision;
const largerFleet = { ...guestContribution, army: { "Ship(LightFighter)": 2 } };
editable = await respondJointAttack(guest, editable.id, "accepted", largerFleet, editable.revision);
assert.equal(editable.participants[1].response, "accepted", "own edit preserves consent to other offers");
assert.equal(editable.participants[2].response, "pending");
await assert.rejects(respondJointAttack(outsider, editable.id, "accepted", thirdContribution, staleRevision), /STLR_FORBIDDEN/);
editable = await respondJointAttack(outsider, editable.id, "accepted", thirdContribution, editable.revision);
editable = await respondJointAttack(guest, editable.id, "accepted",
  { ...largerFleet, origin: protectedPlanet }, editable.revision);
assert.equal(editable.participants[2].response, "pending", "origin changes require fresh consent");
const originEdit = structuredClone(editableAttack);
originEdit.revision = editable.revision;
originEdit.participants[0].contribution.origin = protectorHome;
editable = await createJointAttack(host, originEdit);
assert.equal(editable.participants[0].response, "accepted", "host accepts the proposal it publishes");
assert.equal(editable.participants[1].response, "pending");
assert.equal(editable.participants[2].response, "pending");
const guestProposal = structuredClone(editable);
guestProposal.destination = protectionGame.persisted.state.map.planets.find(planet =>
  planet.owned == null && planet.controlled == null && !planet.is_destroyed
    && planet.id !== editable.destination).id;
guestProposal.objective = "Destroy";
guestProposal.participants[1].response = "accepted";
await assert.rejects(createJointAttack(guest, guestProposal), /STLR_INVALID_DATA:joint_attack/,
  "guests cannot change the shared target or objective");
const tamperedProposal = structuredClone(editable);
tamperedProposal.participants[0].contribution.army = { "Ship(LightFighter)": 9 };
await assert.rejects(createJointAttack(guest, tamperedProposal), /STLR_INVALID_DATA:joint_attack/);
editable = await respondJointAttack(host, editable.id, "accepted",
  editable.participants[0].contribution, editable.revision);
assert.equal(editable.participants[0].response, "accepted");
editable = await respondJointAttack(outsider, editable.id, "rejected", null, editable.revision);
editable = await respondJointAttack(guest, editable.id, "accepted", guestContribution, editable.revision);
assert.equal(editable.participants[2].response, "rejected", "edits never revive a rejected participant");
const rejectedProposal = structuredClone(editable);
rejectedProposal.objective = "Attack";
rejectedProposal.participants[2].response = "accepted";
rejectedProposal.participants[2].contribution = thirdContribution;
await assert.rejects(createJointAttack(outsider, rejectedProposal), /STLR_INVALID_DATA:joint_attack/);
await cancelJointAttack(host, editable.id);
console.log("Editable fleets, host-controlled route, shared consent revisions, and permanent rejection passed.");

await assert.rejects(rpc(host,
  "select public.stellarion_submit_turn($1, $2) as result",
  [protectionGame.id, { player_id: 1, turn: protectionGame.persisted.state.turn,
    commands: [], generation: 1 }]), /STLR_INVALID_DATA:Send or cancel your allied mission/,
  "the owner must finish all open allied planning before ending the turn");
await cancelJointAttack(host, rejectedAttack.id);
await cancelJointAttack(host, ownTargetAttack.id);
await db.query(
  "delete from public.stellarion_turn_submissions where game_id = $1 and turn = $2 and player_id = 1",
  [protectionGame.id, protectionGame.persisted.state.turn],
);

console.log("Private joint-attack invitations, live responses, cancellation, rejection, and own-target acceptance guard passed.");

const setProtection = (allowed) => rpc(host,
  "select public.stellarion_set_protection_permission($1, $2, $3, $4) as result",
  [protectionGame.id, protectedPlanet, 2, allowed]);
// Earlier draft saves deliberately renewed this checkpoint. Give it a distinct
// timestamp now so the permission assertion cannot pass or fail by wall-clock luck.
await db.query("update public.stellarion_games set saved_at = now() - interval '1 day' where id = $1",
  [protectionGame.id]);
const protectionSavedAt = (await rpc(host,
  "select public.stellarion_load_game($1) as result", [protectionGame.id])).saved_at;
const granted = await setProtection(true);
assert.deepEqual(Object.keys(granted).sort(),
  ["allowed", "controller", "planet_id", "protector", "revision", "turn"]);
assert.equal(granted.revision, protectionGame.revision + 1);
let protectionRecord = await rpc(guest,
  "select public.stellarion_load_game($1) as result", [protectionGame.id]);
assert.equal(protectionRecord.saved_at, protectionSavedAt,
  "permission patches do not renew the snapshot retention checkpoint");
assert(protectionRecord.persisted.state.map.planets
  .find(planet => planet.id === protectedPlanet).protection_permissions.includes(2));
assert.equal(protectionRecord.persisted.state.players[1].protection_intel[protectedPlanet], 1);
const protectedUnit = "Ship(LightFighter)";
assert.equal((await rpc(guest,
  "select public.stellarion_submit_turn($1, $2) as result",
  [protectionGame.id, {
    player_id: 2,
    turn: protectionRecord.persisted.state.turn,
    generation: 0,
    commands: [],
  }])).disposition, "inserted");

// Seed a stationed protector into the canonical snapshot. Revocation leaves it available for
// the owner's orders until turn resolution applies the fallback return.
const protectedState = protectionRecord.persisted.state.map.planets
  .find(planet => planet.id === protectedPlanet);
protectedState.army.protectors["2"] = { [protectedUnit]: 1 };
await db.query("update public.stellarion_games set state = $2 where id = $1",
  [protectionGame.id, protectionRecord.persisted]);

const revoked = await setProtection(false);
assert.equal(revoked.revision, granted.revision + 1);
protectionRecord = await rpc(guest,
  "select public.stellarion_load_game($1) as result", [protectionGame.id]);
const revokedPlanet = protectionRecord.persisted.state.map.planets
  .find(planet => planet.id === protectedPlanet);
assert(!revokedPlanet.protection_permissions.includes(2));
assert.equal(revokedPlanet.army.protectors["2"][protectedUnit], 1);
assert.equal(protectionRecord.persisted.state.players[1].protection_intel[protectedPlanet], 1,
  "revocation preserves controller intelligence");
assert(!protectionRecord.persisted.state.missions.some(mission =>
  mission.owner === 2 && mission.origin === protectedPlanet),
  "revocation must not create a homeward mission before the next turn");
assert(!protectionRecord.submitted_players.includes(2));
const protectionDrafts = await rpc(guest,
  "select public.stellarion_load_turn_submissions($1, $2) as result",
  [protectionGame.id, protectionRecord.persisted.state.turn]);
assert.equal(protectionDrafts.length, 1);
assert.equal(protectionDrafts[0].ready, false);
const protectionEvents = await rpc(guest,
  "select public.stellarion_events_since($1, $2) as result", [protectionGame.id, 0]);
assert.equal(protectionEvents.events.filter(event => event.kind === "protection_changed").length, 2);
assert(protectionEvents.events.some(event =>
  event.kind === "turn_withdrawn" && event.player_id === 2));
console.log("Host and guest presence expires after 15 seconds; read-only refreshes, resume guards, reconnects, and saved games remain consistent.");
console.log("Direct authenticated SQL RPCs: create/join/recovery/start/save/ready/continue/resolve, stable recovery codes, permissions, lobby colors, readiness retries, revision races, events, and lobby deletion passed.");
await db.close();
