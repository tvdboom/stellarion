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
  create table cron.job_run_details (jobid bigint, status text);
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
    (id, code, created_by, max_players, status, persisted_schema_version, state, current_turn)
  values ('10000000-0000-0000-0000-000000000001', 'ABCDEF',
    '00000000-0000-0000-0000-000000000001', 2, 'lobby', 1, '{}'::jsonb, 1);
  create table public.obsolete_game_data (id int);
  insert into public.obsolete_game_data values (1);
  create function public.obsolete_game_rpc() returns int language sql as $$ select 1 $$;
  select cron.schedule('stellarion-old-cleanup', '0 0 * * *', 'select 1');
  select cron.schedule('external-job', '0 0 * * *', 'select 2');
  insert into cron.job_run_details select jobid, 'succeeded' from cron.job;
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
  ["external-job", "stellarion-delete-finished-games"],
);
assert.equal(
  (await db.query("select count(*)::int as n from cron.job_run_details"))
    .rows[0].n,
  1,
);
const job = (await db.query(
  "select * from cron.job where jobname = 'stellarion-delete-finished-games'",
)).rows[0];
assert.equal(job.schedule, "* * * * *");
assert.equal(job.command, "select public.stellarion_delete_expired_games();");
assert.equal(
  (await db.query(
    "select count(*)::int as n from pg_class c join pg_namespace s on s.oid = c.relnamespace where s.nspname = 'public' and c.relrowsecurity",
  )).rows[0].n,
  4,
);
console.log(
  "Second reset removed app data, obsolete objects/jobs/history, and recreated current RLS and schedule; managed auth and unrelated jobs survived.",
);
await db.exec(`
  insert into public.stellarion_games
    (id, code, created_by, max_players, status, persisted_schema_version, state, current_turn, finished_at)
  select ('20000000-0000-0000-0000-' || lpad(i::text, 12, '0'))::uuid,
    lpad(i::text, 6, '0'), '00000000-0000-0000-0000-000000000001', 2,
    case when i = 1 then 'lobby' when i = 2 then 'active' else 'finished' end,
    1, '{}'::jsonb, 7,
    case when i = 3 then now() - interval '47 hours' else now() - interval '48 hours' end
  from generate_series(1, 4) i;
  insert into public.stellarion_game_players
    (game_id, player_id, user_id, display_name, recovery_hash, is_creator)
  select id, 1, created_by, 'Tester', repeat('a', 64), true from public.stellarion_games;
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

// Exercise the same authenticated RPCs as native/browser clients, without a
// service-role dispatcher, server runtime, Docker, or any hosted connection.
await db.exec("delete from public.stellarion_games");
const host = "00000000-0000-0000-0000-000000000001";
const guest = "00000000-0000-0000-0000-000000000002";
const outsider = "00000000-0000-0000-0000-000000000003";
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
const create = (actor, code = "ABCDEF", role = "authenticated") => rpc(actor,
  "select public.stellarion_create_game($1, $2, $3, $4, $5) as result",
  [code, "Host", "a".repeat(64), 4, fixtures.lobby], role);
await assert.rejects(create(null, "ABCDEF", "anon"), /permission denied/);
await assert.rejects(create(null), /STLR_UNAUTHENTICATED/);
const created = await create(host);
const id = created.game.id;
assert.equal(created.membership.player_id, 1);
assert.equal(created.game.status, "lobby");
assert(Number.isSafeInteger(created.game.saved_at) && created.game.saved_at > 0);
assert(!JSON.stringify(created).includes("recovery_hash"));
await assert.rejects(create(host), /STLR_CODE_COLLISION/);
assert.deepEqual(await rpc(host, "select public.stellarion_list_games() as result"), []);
await assert.rejects(rpc(host, "select state as result from public.stellarion_games"), /permission denied/);
await assert.rejects(rpc(host, "select recovery_hash as result from public.stellarion_game_players"), /permission denied/);
await assert.rejects(rpc(outsider, "select public.stellarion_load_game($1) as result", [id]), /STLR_FORBIDDEN/);
assert.equal((await db.query("select to_regprocedure('public.stellarion_trusted_write(uuid,text,text,bigint,integer)') as obj")).rows[0].obj, null);
for (const signature of [
  "stellarion_create_game(text,text,text,smallint,jsonb)",
  "stellarion_start_game(uuid,bigint,jsonb)",
  "stellarion_save_game(uuid,bigint,jsonb)",
  "stellarion_submit_turn(uuid,jsonb)",
  "stellarion_withdraw_turn(uuid,bigint,bigint)",
  "stellarion_publish_resolution(uuid,bigint,bigint,jsonb)",
]) {
  for (const role of ["anon", "authenticated"]) {
    assert.equal((await db.query("select has_function_privilege($1, $2, 'execute') as allowed",
      [role, `public.${signature}`])).rows[0].allowed, role === "authenticated", `${role}: ${signature}`);
  }
}
const joined = await rpc(guest, "select public.stellarion_join_game($1, $2, $3) as result",
  ["ABCDEF", "Guest", "b".repeat(64)]);
const save = (actor, record, persisted) => rpc(actor,
  "select public.stellarion_save_game($1, $2, $3) as result", [id, record.revision, persisted]);
let lobby = joined.game;
const occupied = structuredClone(lobby.persisted);
occupied.state.players[0].color = occupied.state.players[1].color;
await assert.rejects(save(host, lobby, occupied), /STLR_INVALID_DATA:color_unavailable/);
const otherPlayer = structuredClone(lobby.persisted);
otherPlayer.state.players[1].color = 5;
await assert.rejects(save(host, lobby, otherPlayer), /STLR_FORBIDDEN/);
const recolored = structuredClone(lobby.persisted);
[recolored.state.players[0].color, recolored.state.players[2].color] =
  [recolored.state.players[2].color, recolored.state.players[0].color];
lobby = await save(host, lobby, recolored);
assert.equal(lobby.persisted.state.players[0].color, 2);
const forgedLobby = structuredClone(lobby.persisted);
forgedLobby.state.players[0].resources.metal += 1;
await assert.rejects(save(host, lobby, forgedLobby), /STLR_FORBIDDEN/);
lobby = await save(host, lobby, fixtures.lobby);
const start = (actor, persisted = fixtures.active, revision = lobby.revision) => rpc(actor,
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
const startedSavedAt = active.saved_at;
const summaries = await rpc(host, "select public.stellarion_list_games() as result");
assert.equal(summaries.length, 1);
assert.equal(summaries[0].saved_at, startedSavedAt);
for (const field of ["resources", "color"]) {
  const forged = structuredClone(active.persisted);
  if (field === "resources") forged.state.players[0].resources.metal += 1;
  else forged.state.players[0].color = 5;
  await assert.rejects(save(guest, active, forged), /STLR_FORBIDDEN/);
}
active = await save(host, active, active.persisted);
assert(active.saved_at >= startedSavedAt);
const submit = (actor, player, turn = 1, commands = [], generation = 0) => rpc(actor,
  "select public.stellarion_submit_turn($1, $2) as result",
  [id, { player_id: player, turn, commands, generation }]);
const withdraw = (actor, turn = 1, generation = 0) => rpc(actor,
  "select public.stellarion_withdraw_turn($1, $2, $3) as result", [id, turn, generation]);
const publish = (actor, persisted = fixtures.resolved, revision = active.revision) => rpc(actor,
  "select public.stellarion_publish_resolution($1, $2, $3, $4) as result",
  [id, revision, 1, persisted]);
await assert.rejects(submit(host, 2), /STLR_FORBIDDEN/);
await assert.rejects(submit(outsider, 1), /STLR_FORBIDDEN/);
await assert.rejects(submit(host, 1, 2), /STLR_STALE_SUBMISSION/);
await assert.rejects(publish(host), /STLR_TURN_INCOMPLETE/);
assert.equal((await submit(host, 1)).disposition, "inserted");
assert.equal((await submit(host, 1)).disposition, "duplicate");
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
await assert.rejects(publish(guest), /STLR_TURN_INCOMPLETE/, "withdrawn orders cannot resolve");
assert.equal((await submit(host, 1, 1, [], draft.generation)).disposition, "inserted");
assert.equal((await submit(host, 1, 1, [], draft.generation)).disposition, "duplicate");
await assert.rejects(withdraw(host, 1, draft.generation), /STLR_TURN_COMMITTED/);
await assert.rejects(withdraw(guest), /STLR_TURN_COMMITTED/);
await assert.rejects(withdraw(host), /STLR_DUPLICATE_SUBMISSION/, "late withdrawal cannot undo a newer ready");
const submissions = await rpc(host, "select public.stellarion_load_turn_submissions($1, $2) as result", [id, 1]);
assert.deepEqual(submissions.map(s => s.submission.player_id), [1, 2]);
const reseeded = structuredClone(fixtures.resolved);
reseeded.state.rng.seed[0] += 1;
await assert.rejects(publish(host, reseeded), /STLR_FORBIDDEN/);
await assert.rejects(publish(outsider), /STLR_FORBIDDEN/);
const resolved = await publish(host);
assert.equal(resolved.persisted.state.turn, 2);
assert.equal(resolved.revision, active.revision + 1);
assert(resolved.saved_at >= active.saved_at);
assert.deepEqual(resolved.submitted_players, []);
await assert.rejects(publish(guest), /STLR_CONFLICT/);
const events = await rpc(guest, "select public.stellarion_events_since($1, $2) as result", [id, 0]);
assert(events.events.some(e => e.kind === "turn_resolved"));
assert(events.events.some(e => e.kind === "turn_withdrawn"));
const temporary = await create(host, "XYZABC");
await rpc(guest, "select public.stellarion_join_game($1, $2, $3) as result",
  ["XYZABC", "Guest", "c".repeat(64)]);
await rpc(host, "select public.stellarion_set_connected($1, false) as result", [temporary.game.id]);
await assert.rejects(rpc(guest, "select public.stellarion_load_game($1) as result", [temporary.game.id]), /STLR_GAME_NOT_FOUND/);
await rpc(host, "select public.stellarion_set_connected($1, false) as result", [id]);
const disconnected = await rpc(guest, "select public.stellarion_load_game($1) as result", [id]);
assert.equal(disconnected.status, "active");
assert.equal(disconnected.saved_at, resolved.saved_at, "presence is not a gameplay save");
// Hosts and guests share the same short presence lease. Reads and event polls
// must expose expiry without renewing it or deleting an active match.
for (const [departed, observer] of [[host, guest], [guest, host]]) {
  for (const actor of [host, guest]) {
    await rpc(actor, "select public.stellarion_set_connected($1, true) as result", [id]);
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
console.log("Host and guest presence expires after 15 seconds; read-only refreshes, resume guards, reconnects, and saved games remain consistent.");
console.log("Direct authenticated SQL RPCs: create/join/start/save/ready/continue/resolve, permissions, lobby colors, readiness retries, revision races, events, and lobby deletion passed.");
await db.close();
