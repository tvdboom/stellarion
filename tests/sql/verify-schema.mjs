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
    (id, code, created_by, max_players, status, state, current_turn)
  values ('10000000-0000-0000-0000-000000000001', 'ABCDEF',
    '00000000-0000-0000-0000-000000000001', 2, 'lobby', '{}'::jsonb, 1);
  create table public.obsolete_game_data (id int);
  insert into public.obsolete_game_data values (1);
  create function public.obsolete_game_rpc() returns int language sql as $$ select 1 $$;
  select cron.schedule('stellarion-stale-cleanup', '0 0 * * *', 'select 1');
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
assert.equal(
  (await db.query(
    "select count(*)::int as n from pg_class c join pg_namespace s on s.oid = c.relnamespace where s.nspname = 'public' and c.relrowsecurity",
  )).rows[0].n,
  5,
);
console.log(
  "Second reset removed app data, obsolete objects/jobs/history, and recreated current RLS and schedule; managed auth and unrelated jobs survived.",
);
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
  "stellarion_respond_joint_attack(uuid,bigint,text,jsonb)",
  "stellarion_cancel_joint_attack(uuid,bigint)",
  "stellarion_load_joint_attacks(uuid)",
  "stellarion_set_protection_permission(uuid,bigint,bigint,boolean)",
  "stellarion_set_player_color(uuid,smallint)",
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
await assert.rejects(rpc(host,
  "select public.stellarion_set_protection_permission($1, $2, $3, $4) as result",
  [id, active.persisted.state.players[0].home_planet, 2, true]),
  /STLR_INVALID_DATA:protection_player/,
  "two-player games never enable protection");
const startedSavedAt = active.saved_at;
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
assert.equal(savedDrafts.length, 1);
assert.equal(savedDrafts[0].ready, false);
assert.deepEqual(savedDrafts[0].submission.commands, []);
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
await assert.rejects(rpc(host,
  "select public.stellarion_submit_turn($1, $2) as result",
  [id, { player_id: 1, turn: 1, commands: [] }]), /STLR_INVALID_DATA:submission/);
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
  ["XYZABC", "Guest", "CDEF-0123-4567-89AB"]);
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

// Protection access is an immediate compact patch in matches with at least three active players.
const protectionLobby = await create(host, "PRTCT1");
let protectionGame = (await rpc(guest,
  "select public.stellarion_join_game($1, $2, $3) as result",
  ["PRTCT1", "Protector", "2222-2222-2222-2222"])).game;
protectionGame = (await rpc(outsider,
  "select public.stellarion_join_game($1, $2, $3) as result",
  ["PRTCT1", "Third", "3333-3333-3333-3333"])).game;
protectionGame = await rpc(host,
  "select public.stellarion_start_game($1, $2, $3) as result",
  [protectionLobby.game.id, protectionGame.revision, fixtures.active_three]);
const protectedPlanet = protectionGame.persisted.state.players[0].home_planet;
const protectorHome = protectionGame.persisted.state.players[1].home_planet;
const thirdHome = protectionGame.persisted.state.players[2].home_planet;
const protectionSavedAt = protectionGame.saved_at;

// Joint-attack coordination stores only the invitation, is visible only to its participants,
// and targets wake-up events to those participants while advancing every member's cursor.
const jointAttack = {
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
      contribution: { player_id: 1, origin: protectedPlanet, army: { "Ship(LightFighter)": 1 } },
    },
    { player_id: 2, response: "pending", contribution: null },
  ],
};
const createJointAttack = (actor, invitation = jointAttack) => rpc(actor,
  "select public.stellarion_create_joint_attack($1, $2) as result",
  [protectionGame.id, invitation]);
const respondJointAttack = (actor, attackId, response, contribution = null) => rpc(actor,
  "select public.stellarion_respond_joint_attack($1, $2, $3, $4) as result",
  [protectionGame.id, attackId, response, contribution]);
const cancelJointAttack = (actor, attackId) => rpc(actor,
  "select public.stellarion_cancel_joint_attack($1, $2) as result",
  [protectionGame.id, attackId]);
const loadJointAttacks = actor => rpc(actor,
  "select public.stellarion_load_joint_attacks($1) as result", [protectionGame.id]);
await assert.rejects(createJointAttack(guest), /STLR_INVALID_DATA:joint_attack/);
assert.equal((await createJointAttack(host)).id, jointAttack.id);
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
};
let updatedJointAttack = await respondJointAttack(guest, jointAttack.id, "accepted", guestContribution);
assert.equal(updatedJointAttack.participants[1].response, "accepted");
assert.deepEqual(updatedJointAttack.participants[1].contribution, guestContribution);
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
  player_id: 3,
  origin: thirdHome,
  army: { "Ship(LightFighter)": 1 },
}), /STLR_FORBIDDEN/, "a target owner may see an unknown-intel invite but cannot accept it");
console.log("Private joint-attack invitations, live responses, cancellation, rejection, and own-target acceptance guard passed.");

const setProtection = (allowed) => rpc(host,
  "select public.stellarion_set_protection_permission($1, $2, $3, $4) as result",
  [protectionGame.id, protectedPlanet, 2, allowed]);
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
protectionRecord.persisted.state.map.planets
  .find(planet => planet.id === protectorHome).army.controller[protectedUnit] = 1;
assert.equal((await rpc(guest,
  "select public.stellarion_submit_turn($1, $2) as result",
  [protectionGame.id, {
    player_id: 2,
    turn: protectionRecord.persisted.state.turn,
    generation: 0,
    commands: [{ SendMission: {
      mission_id: 91,
      origin: protectorHome,
      destination: protectedPlanet,
      objective: "Protect",
      army: { [protectedUnit]: 1 },
      bombing: "None",
      combat_probes: false,
      jump_gate: false,
    } }],
  }])).disposition, "inserted");

// Seed a stationed protector into the canonical snapshot so revocation exercises the immediate
// return path without waiting for a turn-resolution snapshot.
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
assert(!("2" in revokedPlanet.army.protectors));
assert.equal(protectionRecord.persisted.state.players[1].protection_intel[protectedPlanet], 1,
  "revocation preserves controller intelligence");
const immediateReturn = protectionRecord.persisted.state.missions
  .find(mission => mission.owner === 2 && mission.origin === protectedPlanet);
assert.equal(immediateReturn.destination, protectorHome);
assert.equal(immediateReturn.objective, "Deploy");
assert.equal(immediateReturn.return_objective, "Protect");
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
